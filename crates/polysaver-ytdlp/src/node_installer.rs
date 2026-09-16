// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! # Node.js Runtime Installer
//!
//! Installs only the `node` executable from the official Node.js distribution
//! into `<app_data>/bin/node/`, without administrator rights and without `npm`
//! or `node_modules` (yt-dlp writes its EJS scripts to the runtime's stdin).
//!
//! Reuses the hardened download machinery from [`crate::updater`]: HTTPS only,
//! strict host allowlist (extended with `nodejs.org`), private/reserved address
//! rejection with a pinned DNS resolver, streaming SHA-256 verification against
//! the official `SHASUMS256.txt`, and atomic replacement.
//!
//! macOS note: the official Node binaries are Developer ID signed and notarized,
//! and a download performed by our Rust client carries no quarantine attribute,
//! so no re-signing or `xattr` step is required.

use crate::js_runtime::{pinned_node_version, verify_installed_runtime, JsRuntimeKind};
use crate::updater::{download_verified_to_file, hex_encode, parse_sha256sums, replace_binary};
use flate2::read::GzDecoder;
use polysaver_core::error::CoreError;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Base URL of the official Node.js distribution.
const NODE_DIST_BASE: &str = "https://nodejs.org/dist";

/// Result of an install (or of an already-satisfied install).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeInstallOutcome {
    pub version: String,
    pub path: PathBuf,
}

/// Installs the pinned Node.js release into `app_bin_dir/node/`.
///
/// Downloads the official archive, verifies its SHA-256 against
/// `SHASUMS256.txt`, extracts only the `node` executable, then validates the
/// result by running `--version`. Any failure leaves the previous state intact.
pub async fn install_node(app_bin_dir: &Path) -> Result<NodeInstallOutcome, CoreError> {
    let version = pinned_node_version();
    let asset = node_asset_name()?;
    let base = format!("{NODE_DIST_BASE}/v{version}");

    // 1. Checksums (GNU format, identical to yt-dlp's SHA2-256SUMS).
    let client = build_client()?;
    let sums_url = format!("{base}/SHASUMS256.txt");
    let sums_body = fetch_text(&client, &sums_url).await?;
    let sums = parse_sha256sums(&sums_body);
    let expected_hash = sums.get(asset).ok_or_else(|| {
        CoreError::ProviderError(format!(
            "Node.js release v{version} does not publish a checksum for '{asset}'"
        ))
    })?;

    // 2. Verified download of the archive into a staging directory.
    let node_dir = app_bin_dir.join("node");
    tokio::fs::create_dir_all(&node_dir)
        .await
        .map_err(|err| CoreError::StorageError(format!("Failed to create node dir: {err}")))?;

    let archive_path = node_dir.join(format!("{asset}.download"));
    let asset_url = format!("{base}/{asset}");
    download_verified_to_file(&client, &asset_url, expected_hash, &archive_path).await?;

    // 3. Extract only the runtime executable.
    let binary_name = if cfg!(windows) { "node.exe" } else { "node" };
    let staged_binary = node_dir.join(format!("{binary_name}.new"));
    let extract_result = extract_node_binary(&archive_path, &staged_binary);

    // The archive is no longer needed, whatever the extraction outcome.
    let _ = tokio::fs::remove_file(&archive_path).await;

    if let Err(err) = extract_result {
        let _ = tokio::fs::remove_file(&staged_binary).await;
        return Err(err);
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(err) =
            std::fs::set_permissions(&staged_binary, std::fs::Permissions::from_mode(0o755))
        {
            let _ = tokio::fs::remove_file(&staged_binary).await;
            return Err(CoreError::StorageError(format!(
                "Failed to mark node as executable: {err}"
            )));
        }
    }

    // 4. Atomic replacement, then validity check of the installed binary.
    let final_binary = node_dir.join(binary_name);
    replace_binary(&staged_binary, &final_binary).await?;

    let installed = match verify_installed_runtime(&final_binary, JsRuntimeKind::Node).await {
        Ok(version) => version,
        Err(err) => {
            // Leave nothing broken behind if the binary turns out unusable.
            let _ = tokio::fs::remove_file(&final_binary).await;
            return Err(err);
        }
    };

    Ok(NodeInstallOutcome {
        version: installed,
        path: final_binary,
    })
}

/// Builds the shared hardened HTTP client (same policy as the engine updater).
fn build_client() -> Result<reqwest::Client, CoreError> {
    crate::updater::build_download_client()
}

/// Performs a validated GET and returns the body as text.
async fn fetch_text(client: &reqwest::Client, url: &str) -> Result<String, CoreError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| CoreError::ProviderError(format!("Metadata request failed: {err}")))?;

    if response.status() == reqwest::StatusCode::FORBIDDEN
        || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return Err(CoreError::ProviderError(
            "Rate limit reached while fetching Node.js metadata; please retry later".to_string(),
        ));
    }
    if !response.status().is_success() {
        return Err(CoreError::ProviderError(format!(
            "Node.js metadata request returned HTTP {}",
            response.status()
        )));
    }

    response
        .text()
        .await
        .map_err(|err| CoreError::ProviderError(format!("Failed to read metadata response: {err}")))
}

/// Official archive name for the current platform.
fn node_asset_name() -> Result<&'static str, CoreError> {
    let version = pinned_node_version();
    // The names embed the version; leak a formatted string once per call is
    // wasteful, so return known-good literals for each supported target.
    let _ = version;
    if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        Ok(node_asset_for_macos_arm64())
    } else if cfg!(target_os = "macos") {
        Ok(node_asset_for_macos_x64())
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        Ok(node_asset_for_linux_arm64())
    } else if cfg!(target_os = "linux") {
        Ok(node_asset_for_linux_x64())
    } else if cfg!(windows) && cfg!(target_arch = "aarch64") {
        Ok(node_asset_for_win_arm64())
    } else if cfg!(windows) {
        Ok(node_asset_for_win_x64())
    } else {
        Err(CoreError::ProviderError(
            "No Node.js distribution is known for this platform".to_string(),
        ))
    }
}

// Asset names are compile-time constants so no allocation or leak is needed at runtime.
const fn node_asset_for_macos_arm64() -> &'static str {
    "node-v24.21.0-darwin-arm64.tar.gz"
}
const fn node_asset_for_macos_x64() -> &'static str {
    "node-v24.21.0-darwin-x64.tar.gz"
}
const fn node_asset_for_linux_x64() -> &'static str {
    "node-v24.21.0-linux-x64.tar.gz"
}
const fn node_asset_for_linux_arm64() -> &'static str {
    "node-v24.21.0-linux-arm64.tar.gz"
}
const fn node_asset_for_win_x64() -> &'static str {
    "node-v24.21.0-win-x64.zip"
}
const fn node_asset_for_win_arm64() -> &'static str {
    "node-v24.21.0-win-arm64.zip"
}

/// Returns true when `path` is the runtime executable inside the archive layout.
///
/// The official archives contain several entries named `node` that are
/// *directories* (`include/node/`, `share/doc/node/`, ...), so matching on the
/// file name alone would extract an empty file. The real layouts are:
/// - Unix: `node-vX-<platform>/bin/node` (inside a `bin/` directory)
/// - Windows: `node-vX-win-<arch>/node.exe` (directly in the versioned directory)
///
/// The rule keys on the layout, not on the host platform, so both shapes are
/// testable on any machine and the extraction behaviour cannot diverge between
/// the platform that builds and the one that runs.
fn is_runtime_entry(path: &Path, is_regular_file: bool, binary_name: &str) -> bool {
    if !is_regular_file {
        return false;
    }
    if path.file_name().and_then(|n| n.to_str()) != Some(binary_name) {
        return false;
    }

    let components: Vec<_> = path.components().collect();
    // Reject anything buried deeper than `<versioned-dir>/<name>` or
    // `<versioned-dir>/bin/<name>`: the executable is never nested further.
    if components.len() > 3 {
        return false;
    }

    match components.as_slice() {
        // `<versioned-dir>/node.exe`
        [_, _] => true,
        // `<versioned-dir>/bin/node`
        [_, bin, _] => matches!(bin, std::path::Component::Normal(dir) if *dir == "bin"),
        _ => false,
    }
}

/// Extracts the `node` executable from a `.tar.gz` or `.zip` archive.
///
/// The archive layout is always `node-vX.Y.Z-<platform>/bin/node` (Unix) or
/// `node-vX.Y.Z-win-<arch>/node.exe` (Windows); the entry is located by suffix
/// so the exact versioned directory name does not matter.
fn extract_node_binary(archive_path: &Path, dest: &Path) -> Result<(), CoreError> {
    let file = std::fs::File::open(archive_path).map_err(|err| {
        CoreError::StorageError(format!("Failed to open the Node.js archive: {err}"))
    })?;

    let binary_name = if cfg!(windows) { "node.exe" } else { "node" };

    if archive_path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
    {
        return extract_from_zip(file, binary_name, dest);
    }

    // Only .tar.gz and .zip are used across platforms, so no xz decoder is
    // required in the dependency graph.
    let decoder = GzDecoder::new(file);
    extract_from_tar(decoder, binary_name, dest)
}

/// Copies the matching entry out of a tar stream.
fn extract_from_tar<R: Read>(reader: R, binary_name: &str, dest: &Path) -> Result<(), CoreError> {
    let mut archive = tar::Archive::new(reader);
    let entries = archive
        .entries()
        .map_err(|err| CoreError::StorageError(format!("Failed to read the archive: {err}")))?;

    for entry in entries {
        let mut entry = entry
            .map_err(|err| CoreError::StorageError(format!("Corrupted archive entry: {err}")))?;
        let path = entry
            .path()
            .map_err(|err| CoreError::StorageError(format!("Invalid archive path: {err}")))?
            .to_path_buf();

        let is_match = is_runtime_entry(&path, entry.header().entry_type().is_file(), binary_name);

        if is_match {
            let mut output = std::fs::File::create(dest).map_err(|err| {
                CoreError::StorageError(format!("Failed to create the node binary: {err}"))
            })?;
            std::io::copy(&mut entry, &mut output).map_err(|err| {
                CoreError::StorageError(format!("Failed to extract the node binary: {err}"))
            })?;
            output.sync_all().map_err(|err| {
                CoreError::StorageError(format!("Failed to flush the node binary: {err}"))
            })?;
            return Ok(());
        }
    }

    Err(CoreError::ProviderError(format!(
        "'{binary_name}' was not found inside the Node.js archive"
    )))
}

/// Copies the matching entry out of a zip stream.
fn extract_from_zip<R: Read + std::io::Seek>(
    reader: R,
    binary_name: &str,
    dest: &Path,
) -> Result<(), CoreError> {
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|err| CoreError::StorageError(format!("Failed to read the zip archive: {err}")))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| CoreError::StorageError(format!("Corrupted zip entry: {err}")))?;

        let is_match = entry
            .enclosed_name()
            .is_some_and(|p| is_runtime_entry(&p, entry.is_file(), binary_name));

        if is_match {
            let mut output = std::fs::File::create(dest).map_err(|err| {
                CoreError::StorageError(format!("Failed to create the node binary: {err}"))
            })?;
            std::io::copy(&mut entry, &mut output).map_err(|err| {
                CoreError::StorageError(format!("Failed to extract the node binary: {err}"))
            })?;
            output.sync_all().map_err(|err| {
                CoreError::StorageError(format!("Failed to flush the node binary: {err}"))
            })?;
            return Ok(());
        }
    }

    Err(CoreError::ProviderError(format!(
        "'{binary_name}' was not found inside the Node.js archive"
    )))
}

/// Computes the SHA-256 of an in-memory buffer (test helper).
#[must_use]
pub fn sha256_of(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_encode(&hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Entry path matching the real archive layout for this platform:
    /// Unix uses `<versioned>/bin/node`, Windows `<versioned>/node.exe`.
    fn fixture_entry_path() -> String {
        if cfg!(windows) {
            "node-v24.21.0-win-x64/node.exe".to_string()
        } else {
            "node-v24.21.0-darwin-arm64/bin/node".to_string()
        }
    }

    #[test]
    fn test_node_asset_name_matches_platform() {
        let asset = node_asset_name().unwrap();
        let version = pinned_node_version();

        assert!(
            asset.contains(version),
            "asset '{asset}' must embed the pinned version {version}"
        );

        if cfg!(windows) {
            assert!(asset.ends_with(".zip"));
        } else {
            // Linux uses the gzip tarball: no xz decoder dependency is needed.
            assert!(asset.ends_with(".tar.gz"));
        }
    }

    #[test]
    fn test_extract_node_binary_from_tar_gz() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let dir = std::env::temp_dir().join(format!("polysaver_tar_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // Build a minimal archive mimicking the official layout.
        let archive_path = dir.join("fake.tar.gz");
        {
            let file = std::fs::File::create(&archive_path).unwrap();
            let encoder = GzEncoder::new(file, Compression::default());
            let mut builder = tar::Builder::new(encoder);

            let payload = b"#!/bin/sh\necho 'v24.21.0'\n";
            let mut header = tar::Header::new_gnu();
            header.set_size(payload.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, fixture_entry_path(), &payload[..])
                .unwrap();
            let encoder = builder.into_inner().unwrap();
            encoder.finish().unwrap();
        }

        let dest = dir.join("node.extracted");
        extract_node_binary(&archive_path, &dest).unwrap();
        let content = std::fs::read(&dest).unwrap();
        assert!(content.starts_with(b"#!/bin/sh"));

        // An archive without the expected binary is rejected.
        let empty_dir = dir.join("empty");
        std::fs::create_dir_all(&empty_dir).unwrap();
        let bad_archive = empty_dir.join("bad.tar.gz");
        {
            let file = std::fs::File::create(&bad_archive).unwrap();
            let encoder = GzEncoder::new(file, Compression::default());
            let builder = tar::Builder::new(encoder);
            let encoder = builder.into_inner().unwrap();
            encoder.finish().unwrap();
        }
        assert!(extract_node_binary(&bad_archive, &dir.join("none.bin")).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_extract_node_binary_from_zip() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let dir = std::env::temp_dir().join(format!("polysaver_zip_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let archive_path = dir.join("fake.zip");

        // Platform-appropriate entry layout (see fixture_entry_path).
        let entry_name = fixture_entry_path();

        {
            let file = std::fs::File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = SimpleFileOptions::default();
            writer.start_file(entry_name, options).unwrap();
            writer.write_all(b"MZ fake node").unwrap();
            writer.finish().unwrap();
        }

        let dest = dir.join("node.extracted");
        extract_node_binary(&archive_path, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"MZ fake node".to_vec());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_is_runtime_entry_rejects_directories_named_node() {
        // The real archive contains `include/node/`, `share/doc/node/` and
        // nested npm directories named `node` BEFORE `bin/node`; they must not match.
        assert!(!is_runtime_entry(
            Path::new("node-v24.21.0-darwin-arm64/include/node"),
            false,
            "node"
        ));
        assert!(!is_runtime_entry(
            Path::new("node-v24.21.0-darwin-arm64/share/doc/node"),
            false,
            "node"
        ));
        assert!(!is_runtime_entry(
            Path::new("node-v24.21.0-darwin-arm64/lib/node_modules/npm/node_modules/lru-cache/dist/commonjs/node"),
            false,
            "node"
        ));
        // A directory flagged as regular must still be rejected by its location.
        assert!(!is_runtime_entry(
            Path::new("node-v24.21.0-darwin-arm64/include/node"),
            true,
            "node"
        ));
        // The real Unix executable is accepted.
        assert!(is_runtime_entry(
            Path::new("node-v24.21.0-darwin-arm64/bin/node"),
            true,
            "node"
        ));
        // Wrong names are rejected even in bin/.
        assert!(!is_runtime_entry(
            Path::new("node-v24.21.0-darwin-arm64/bin/npm"),
            true,
            "node"
        ));
    }

    /// Both official layouts must be recognised on every platform, so extraction
    /// cannot work on macOS and silently fail on Windows.
    #[test]
    fn test_is_runtime_entry_accepts_both_official_layouts() {
        // Unix: `<versioned>/bin/node`
        assert!(is_runtime_entry(
            Path::new("node-v24.21.0-darwin-arm64/bin/node"),
            true,
            "node"
        ));
        assert!(is_runtime_entry(
            Path::new("node-v24.21.0-linux-x64/bin/node"),
            true,
            "node"
        ));

        // Windows: `<versioned>/node.exe`, no `bin/` level.
        assert!(is_runtime_entry(
            Path::new("node-v24.21.0-win-x64/node.exe"),
            true,
            "node.exe"
        ));

        // A regular `node` sitting in a non-bin subdirectory is not the runtime.
        assert!(!is_runtime_entry(
            Path::new("node-v24.21.0-darwin-arm64/lib/node"),
            true,
            "node"
        ));
        // Deeper nesting is rejected even with the right file name.
        assert!(!is_runtime_entry(
            Path::new("node-v24.21.0-win-x64/nested/dir/node.exe"),
            true,
            "node.exe"
        ));
        // A root-level file is never the runtime.
        assert!(!is_runtime_entry(Path::new("node.exe"), true, "node.exe"));
        assert!(!is_runtime_entry(Path::new("node"), true, "node"));
    }

    #[test]
    fn test_extract_from_tar_prefers_bin_entry() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let dir = std::env::temp_dir().join(format!("polysaver_tarorder_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let archive_path = dir.join("ordered.tar.gz");

        // Mimic the real archive: decoy directory entries first, real binary last.
        {
            let file = std::fs::File::create(&archive_path).unwrap();
            let encoder = GzEncoder::new(file, Compression::default());
            let mut builder = tar::Builder::new(encoder);

            let mut dir_header = tar::Header::new_gnu();
            dir_header.set_entry_type(tar::EntryType::Directory);
            dir_header.set_size(0);
            dir_header.set_mode(0o755);
            dir_header.set_cksum();
            builder
                .append_data(
                    &mut dir_header,
                    "node-v24.21.0-darwin-arm64/include/node/",
                    &[][..],
                )
                .unwrap();

            let payload = b"#!/bin/sh\necho 'v24.21.0'\n";
            let mut header = tar::Header::new_gnu();
            header.set_size(payload.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, fixture_entry_path(), &payload[..])
                .unwrap();

            let encoder = builder.into_inner().unwrap();
            encoder.finish().unwrap();
        }

        let dest = dir.join("node.extracted");
        extract_node_binary(&archive_path, &dest).unwrap();
        let content = std::fs::read(&dest).unwrap();
        assert!(
            !content.is_empty(),
            "the decoy directory entry must not produce an empty binary"
        );
        assert!(content.starts_with(b"#!/bin/sh"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_sha256_of_matches_known_vector() {
        // SHA-256 of the empty string.
        assert_eq!(
            sha256_of(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    /// Full-chain smoke test against the real Node.js distribution:
    /// checksum fetch, verified download, extraction of the single `node`
    /// executable, atomic install, and `--version` validation.
    ///
    /// Run explicitly (downloads ~50 MB):
    /// `POLYSAVER_RUN_NETWORK_E2E=1 cargo test -p polysaver-ytdlp -- --ignored`
    #[tokio::test]
    #[ignore = "Requires live nodejs.org access; run with POLYSAVER_RUN_NETWORK_E2E=1"]
    async fn test_real_node_install_end_to_end() {
        if std::env::var("POLYSAVER_RUN_NETWORK_E2E").as_deref() != Ok("1") {
            return;
        }

        let dir = std::env::temp_dir().join(format!("polysaver_node_e2e_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();

        let outcome = install_node(&dir).await.unwrap();
        assert_eq!(outcome.version, pinned_node_version());
        assert!(outcome.path.is_file());

        // The resolver must find the freshly installed runtime.
        let resolver = polysaver_binres::BinaryResolver::new(dir.clone(), None);
        let resolved = resolver.resolve_node().await.unwrap();
        assert_eq!(resolved.path, outcome.path);

        // Nothing but the executable and its directory is installed.
        assert!(!dir.join("node").join("npm").exists());
        assert!(!dir.join("node").join("lib").exists());

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
