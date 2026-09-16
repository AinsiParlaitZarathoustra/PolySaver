# PolySaver 2.5 — Release & CI notes

Operational notes for the release pipeline. Written on 2026-09-13.

## Hidden prerequisites

### 1. The release needs a minisign signing key

`release.yml` refuses to start when `TAURI_SIGNING_PRIVATE_KEY` is missing, and the
updater cannot work without it. The secret lives on **`AinsiParlaitZarathoustra/PolySaver`**
(the repository that publishes releases), deliberately **not** on `PolySaver-2.5`:

| Secret | Where | Value |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | `PolySaver` | The **full content** of the minisign private key file (never a hash or a re-encoded base64 blob) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | `PolySaver` | Its password, or an empty value when the key has none |

Its absence from `PolySaver-2.5` is intentional: it makes a release attempted from the
development repository fail immediately, so no release can be created by accident there.
`release.yml` also carries an explicit guard that stops with a clear message before the
secret check is even reached.

The public half must match `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`.
If the key is regenerated, the pubkey must be replaced in the same commit —
otherwise every existing installation silently stops receiving updates.

### 2. Repository roles and the updater endpoint

| Repository | Role | Releases |
|---|---|---|
| `AinsiParlaitZarathoustra/PolySaver` | **Release** repository — public, hosts the published releases | yes |
| `AinsiParlaitZarathoustra/PolySaver-2.5` | **Development** repository | no (guarded) |

The configured endpoint is:

```
https://github.com/AinsiParlaitZarathoustra/PolySaver/releases/latest/download/latest.json
```

It must point at the repository that actually publishes releases. GitHub requires
authentication to read releases of a **private** repository, so the release repository
has to be **public** — publishing from a private repository would ship an application
that can never update itself. The development repository can be private without
affecting users, since the updater never queries it.

The same rule applies to every generated artifact URL: `latest.json` is built from
`GITHUB_REPOSITORY`, so it always points at whichever repository runs the workflow
(see the import checklist below).

### 3. Sidecars must be provisioned before any cargo command

`tauri.conf.json` declares `resources/bin/*` as a bundle resource, and `tauri-build`
fails with `GlobPathNotFound` when that glob matches nothing. Since
`src-tauri/resources/bin/` is gitignored, **every** job that runs cargo must call
`bash scripts/prepare-sidecars.sh <platform>` first. This was the root cause of the
previously broken `e2e-downloads.yml`.

`prepare-sidecars.sh` deletes and recreates the target directory on every run, so
caching `src-tauri/resources/bin` is pointless: the CI caches the expensive
**macOS ffmpeg build** (`/tmp/polysaver_macos-aarch64_ffmpeg`, `..._ffprobe`) instead
and lets the yt-dlp download re-run (it is small and always hash-verified).

## Importing these workflows into the release repository

The workflows are plain files and are **repository-agnostic**: nothing hardcodes an
owner, and `GITHUB_REPOSITORY` is passed to `generate-release-metadata.mjs`, so the
generated URLs always point at whichever repository runs the release. Importing them
into `PolySaver` therefore means copying files, not rewriting logic.

Files to copy (all of them, otherwise a job fails on a missing script):

| Path | Why |
|---|---|
| `.github/workflows/ci.yml` | cross-platform build/test on push and PR |
| `.github/workflows/release.yml` | the release pipeline itself |
| `.github/workflows/e2e-downloads.yml` | weekly real-download smoke test |
| `scripts/check-boundaries.sh` | used by `npm run check:boundaries` in `ci.yml` |
| `scripts/prepare-sidecars.sh` | provisions `src-tauri/resources/bin/` before cargo |
| `scripts/generate-release-metadata.mjs` | builds `latest.json` + `SHA256SUMS.txt` |
| `node-version.txt`, `ytdlp-version.txt` | pinned versions read by CI, the installer and the e2e test |
| `package.json` | provides `check:boundaries`, `lint`, `typecheck`, `test`, `build` |
| `.gitignore` | must keep ignoring `src-tauri/resources/bin/`, `target/`, `node_modules/` |

Checklist before triggering a release there:

1. The tree in `PolySaver` carries the version you want to publish in **all three**
   places (`Cargo.toml` `[workspace.package] version`, `package.json`,
   `src-tauri/tauri.conf.json`), because `validate` cross-checks them.
2. The workflow file names match what already exists in that repository. Both
   repositories currently hold the same six workflow files, so importing replaces
   them in place — review the diff, do not add a second copy under another name.
3. The `Release` workflow's `workflow_dispatch` runs on the branch you select; make
   sure the version-consistency check reads the tree you intend to release.
4. `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` are present
   on `PolySaver` (they are), and their public half matches
   `plugins.updater.pubkey`.
5. The updater endpoint in the released `tauri.conf.json` still points at the
   repository that publishes the release (it does: `PolySaver`).

Not importable: the Actions **run history and logs**. They are per-repository, so
the build/test evidence stays in whichever repository produced it. That is a reason
to run the release from `PolySaver` rather than to copy runs across.

## Publishing a release

1. Make sure `Cargo.toml` (`[workspace.package] version`), `package.json` and
   `src-tauri/tauri.conf.json` all carry the same version — the `validate` job fails
   otherwise.
2. Run the `Release` workflow **in `AinsiParlaitZarathoustra/PolySaver`** with
   `version: 2.5.0`, or push a `v2.5.0` tag there. Triggering it in the development
   repository stops immediately by design (no signing secret, plus an explicit guard).
3. The three platform jobs build and stage the assets; `publish` assembles them,
   regenerates `latest.json` + `SHA256SUMS.txt`, verifies the checksums of what
   GitHub actually stored, then publishes with `--latest`.

Expected release content is **exactly 12 assets**:

```
PolySaver_<v>_macOS_arm64.dmg
PolySaver_<v>_macOS_arm64.app.tar.gz          + .sig
PolySaver_<v>_Windows_x64_Setup.exe           + .sig
PolySaver_<v>_Windows_x64.msi                 + .sig
PolySaver_<v>_Linux_x64.AppImage              + .sig
PolySaver_<v>_Linux_x64.deb
latest.json
SHA256SUMS.txt
```

`scripts/generate-release-metadata.mjs` validates all of this locally. Its
`--self-test` mode replays the generation on synthetic fixtures and asserts the
failure paths (truncated signature, missing asset), so the script can be checked in
CI without a build:

```bash
node scripts/generate-release-metadata.mjs --self-test
```

`latest.json` is validated **as a whole** by the Tauri updater before it compares
versions: one malformed platform entry breaks updates for every platform. That is
why the script now verifies each `.sig` is a structurally valid minisign block
(`untrusted comment:` + base64 payload starting with the `Ed` algorithm marker)
instead of only checking that it is non-empty.

## Windows/Linux sidecars depend on a third-party build that expires

The Windows and Linux ffmpeg binaries come from the `BtbN/FFmpeg-Builds` project, which
**prunes its old `autobuild-*` tags**. A pinned tag that works today can return 404 in a
few weeks, and that is exactly how both non-macOS CI legs were broken without anyone
noticing — they had never been executed.

When `prepare-sidecars.sh` fails on a 404 for `BtbN/FFmpeg-Builds`:

1. Open <https://github.com/BtbN/FFmpeg-Builds/releases> and pick the newest
   `autobuild-*` tag.
2. Read its `checksums.sha256` asset and copy the line for the wanted artifact.
3. Update both `FFMPEG_ZIP_URL`/`FFMPEG_ZIP_SHA256` (Windows) or
   `FFMPEG_TAR_URL`/`FFMPEG_TAR_SHA256` (Linux) in `scripts/prepare-sidecars.sh`.
4. Verify locally with `shasum -a 256 <downloaded-file>`.

The script now prints these instructions itself when a download fails, instead of
surfacing a bare curl error.

## Tests must wait for state, never sleep a fixed duration

The download pipeline runs in a background task. Tests that assert on its outcome
must poll for the expected state (`wait_for_terminal`, `wait_for_history`,
`wait_until` in `crates/polysaver-core/tests/invariants.rs`), never `sleep` for a
fixed number of milliseconds. Fixed sleeps are what made the Linux CI leg fail
while macOS passed on the very same commit: the runner was simply slower than the
development machine. The polling helpers allow up to 10 s and return as soon as the
state is reached, so they are both faster locally and reliable on CI.

## Runner notes

- `ubuntu-22.04` is used deliberately for the Linux build: it is the last runner
  providing glibc 2.35, which is the compatibility floor for our AppImage/deb.
  GitHub announced its deprecation on **2026-09-17** and removal on **2027-04-17**
  (with job brownouts in between). Before then, move that job into a `debian:12` or
  `ubuntu:22.04` container running on `ubuntu-24.04` to keep the same glibc floor.
- `ubuntu-22.04-arm` only exists for public repositories and is intentionally absent
  from the CI matrix.
- Node is pinned through `node-version.txt` (single source of truth shared with the
  JavaScript runtime installer); do not hardcode a Node version in workflows.
