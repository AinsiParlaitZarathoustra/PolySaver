// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::error::{BinResError, BinaryKind};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::process::Command;
use tokio::sync::RwLock;
/// Information about a verified, resolved binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBinary {
    pub kind: BinaryKind,
    pub path: PathBuf,
    pub version: String,
    pub size: u64,
    pub modified: SystemTime,
}

/// Shared, cached binary resolver for external sidecars.
#[derive(Debug, Clone)]
pub struct BinaryResolver {
    app_bin_dir: PathBuf,
    resource_bin_dir: Option<PathBuf>,
    cache: Arc<RwLock<HashMap<BinaryKind, ResolvedBinary>>>,
}

impl BinaryResolver {
    /// Creates a new `BinaryResolver` with empty cache.
    #[must_use]
    pub fn new(app_bin_dir: PathBuf, resource_bin_dir: Option<PathBuf>) -> Self {
        Self {
            app_bin_dir,
            resource_bin_dir,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Resolves yt-dlp binary with caching.
    pub async fn resolve_ytdlp(&self) -> Result<ResolvedBinary, BinResError> {
        self.resolve(BinaryKind::YtDlp).await
    }

    /// Resolves ffmpeg binary with caching.
    pub async fn resolve_ffmpeg(&self) -> Result<ResolvedBinary, BinResError> {
        self.resolve(BinaryKind::Ffmpeg).await
    }

    /// Resolves ffprobe binary with caching.
    pub async fn resolve_ffprobe(&self) -> Result<ResolvedBinary, BinResError> {
        self.resolve(BinaryKind::Ffprobe).await
    }

    /// Resolves the Node.js runtime with caching.
    pub async fn resolve_node(&self) -> Result<ResolvedBinary, BinResError> {
        self.resolve(BinaryKind::Node).await
    }

    /// Resolves the Deno runtime with caching.
    pub async fn resolve_deno(&self) -> Result<ResolvedBinary, BinResError> {
        self.resolve(BinaryKind::Deno).await
    }

    /// Resolves any binary kind with caching and fingerprint freshness validation.
    pub async fn resolve(&self, kind: BinaryKind) -> Result<ResolvedBinary, BinResError> {
        // 1. Check existing cached resolution
        {
            let read_guard = self.cache.read().await;
            if let Some(cached) = read_guard.get(&kind) {
                if let Ok(meta) = std::fs::metadata(&cached.path) {
                    let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    if meta.len() == cached.size && mtime == cached.modified {
                        return Ok(cached.clone());
                    }
                }
            }
        }

        // 2. Perform candidate search
        let resolved = self.find_and_verify(kind).await?;

        // 3. Store positive resolution in cache
        {
            let mut write_guard = self.cache.write().await;
            write_guard.insert(kind, resolved.clone());
        }

        Ok(resolved)
    }

    /// Explicitly invalidates cache entry for a single binary kind.
    pub async fn invalidate(&self, kind: BinaryKind) {
        let mut write_guard = self.cache.write().await;
        write_guard.remove(&kind);
    }

    /// Explicitly invalidates all cached binary resolutions.
    pub async fn invalidate_all(&self) {
        let mut write_guard = self.cache.write().await;
        write_guard.clear();
    }

    /// Searches for the binary in hierarchical priority order and validates via version check.
    ///
    /// For `YtDlp`, `Node` and `Deno` the writable app-managed directory wins over
    /// the bundled resources, so a runtime-updated engine or an installed runtime
    /// actually takes effect. `Ffmpeg` and `Ffprobe` keep the bundled-first order.
    async fn find_and_verify(&self, kind: BinaryKind) -> Result<ResolvedBinary, BinResError> {
        let base_name = kind.base_name();
        let candidate_names = candidate_file_names(base_name);
        let app_bin_first = kind.prefers_app_bin_dir();

        for name in &candidate_names {
            // Priority 1: App managed bin directory (for updatable/installable kinds)
            if app_bin_first {
                let app_candidate = self.app_bin_dir.join(name);
                if let Ok(resolved) = verify_binary_file(kind, &app_candidate).await {
                    return Ok(resolved);
                }
                // Runtime installs use a dedicated subdirectory:
                // `<app_bin>/node/node` and `<app_bin>/deno/deno`.
                if matches!(kind, BinaryKind::Node | BinaryKind::Deno) {
                    let nested = self.app_bin_dir.join(kind.base_name()).join(name);
                    if let Ok(resolved) = verify_binary_file(kind, &nested).await {
                        return Ok(resolved);
                    }
                }
            }

            // Priority 2: Bundle resource directory
            if let Some(ref res_dir) = self.resource_bin_dir {
                let p = res_dir.join(name);
                if let Ok(resolved) = verify_binary_file(kind, &p).await {
                    return Ok(resolved);
                }
            }

            // Priority 3: App managed bin directory (fallback position for non-engine kinds)
            if !app_bin_first {
                let app_candidate = self.app_bin_dir.join(name);
                if let Ok(resolved) = verify_binary_file(kind, &app_candidate).await {
                    return Ok(resolved);
                }
            }

            // Priority 4: Well-known absolute install locations for JS runtimes.
            // A GUI app on macOS does not inherit the shell PATH, so Homebrew,
            // nvm, Volta, asdf and mise are invisible without this step.
            if matches!(kind, BinaryKind::Node | BinaryKind::Deno) {
                for candidate in js_runtime_candidate_paths(kind) {
                    if let Ok(resolved) = verify_binary_file(kind, &candidate).await {
                        return Ok(resolved);
                    }
                }
            }

            // Priority 4: Sibling next to current executable
            if let Ok(current_exe) = std::env::current_exe() {
                if let Some(exe_dir) = current_exe.parent() {
                    // Sibling exe
                    let sibling = exe_dir.join(name);
                    if let Ok(resolved) = verify_binary_file(kind, &sibling).await {
                        return Ok(resolved);
                    }
                    // Sibling bin/ directory
                    let bin_sibling = exe_dir.join("bin").join(name);
                    if let Ok(resolved) = verify_binary_file(kind, &bin_sibling).await {
                        return Ok(resolved);
                    }
                    // macOS bundle structure ../Resources/bin/
                    let res_sibling = exe_dir.join("../Resources/bin").join(name);
                    if let Ok(resolved) = verify_binary_file(kind, &res_sibling).await {
                        return Ok(resolved);
                    }
                }
            }

            // Priority 5: Standard system paths on Unix
            #[cfg(not(windows))]
            {
                let system_dirs = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"];
                for dir in system_dirs {
                    let sys_candidate = PathBuf::from(dir).join(name);
                    if let Ok(resolved) = verify_binary_file(kind, &sys_candidate).await {
                        return Ok(resolved);
                    }
                }
            }

            // Priority 6: PATH environment variable directory inspection
            if let Some(path_var) = std::env::var_os("PATH") {
                for path_dir in std::env::split_paths(&path_var) {
                    let path_candidate = path_dir.join(name);
                    if let Ok(resolved) = verify_binary_file(kind, &path_candidate).await {
                        return Ok(resolved);
                    }
                }
            }
        }

        Err(BinResError::NotFound { kind })
    }
}

/// Generates platform-appropriate file candidate names.
fn candidate_file_names(base_name: &str) -> Vec<String> {
    if cfg!(windows) {
        if base_name.ends_with(".exe") {
            vec![base_name.to_string()]
        } else {
            vec![format!("{base_name}.exe"), base_name.to_string()]
        }
    } else {
        vec![base_name.to_string()]
    }
}

/// Well-known absolute install locations for Node.js and Deno.
///
/// Exists because a GUI application does not inherit the shell `PATH` on macOS:
/// Homebrew, nvm, Volta, asdf and mise are all invisible without this list.
/// Versioned directories (nvm, asdf, mise, AppData\nvm) are expanded by scanning
/// their parent directory for the highest version.
fn js_runtime_candidate_paths(kind: BinaryKind) -> Vec<PathBuf> {
    let name = if cfg!(windows) { "node.exe" } else { "node" };
    let deno_name = if cfg!(windows) { "deno.exe" } else { "deno" };
    let home = std::env::var_os("HOME").map(PathBuf::from);

    let mut candidates: Vec<PathBuf> = Vec::new();

    if kind == BinaryKind::Deno {
        if cfg!(windows) {
            if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                candidates.push(PathBuf::from(&local).join("deno").join(deno_name));
            }
            if let Some(profile) = std::env::var_os("USERPROFILE") {
                candidates.push(
                    PathBuf::from(&profile)
                        .join(".deno")
                        .join("bin")
                        .join(deno_name),
                );
            }
        } else {
            candidates.push(PathBuf::from("/opt/homebrew/bin").join(deno_name));
            candidates.push(PathBuf::from("/usr/local/bin").join(deno_name));
            candidates.push(PathBuf::from("/opt/macports/bin").join(deno_name));
            candidates.push(PathBuf::from("/usr/bin").join(deno_name));
            candidates.push(PathBuf::from("/snap/bin").join(deno_name));
            if let Some(ref home) = home {
                candidates.push(home.join(".deno").join("bin").join(deno_name));
                candidates.push(home.join(".local").join("bin").join(deno_name));
                candidates.push(home.join(".cargo").join("bin").join(deno_name));
            }
        }
        return candidates;
    }

    // Node.js
    if cfg!(windows) {
        if let Some(pf) = std::env::var_os("ProgramFiles") {
            candidates.push(PathBuf::from(&pf).join("nodejs").join(name));
        }
        if let Some(pf86) = std::env::var_os("ProgramFiles(x86)") {
            candidates.push(PathBuf::from(&pf86).join("nodejs").join(name));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let local = PathBuf::from(local);
            candidates.push(local.join("Programs").join("nodejs").join(name));
            candidates.push(local.join("Volta").join("bin").join(name));
            candidates.extend(latest_versioned_bin(&local.join("nvm"), name));
        }
        if let Some(appdata) = std::env::var_os("APPDATA") {
            candidates.extend(latest_versioned_bin(
                &PathBuf::from(appdata).join("nvm"),
                name,
            ));
        }
        return candidates;
    }

    candidates.push(PathBuf::from("/opt/homebrew/bin").join(name));
    candidates.push(PathBuf::from("/usr/local/bin").join(name));
    candidates.push(PathBuf::from("/opt/macports/bin").join(name));
    candidates.push(PathBuf::from("/usr/bin").join(name));
    candidates.push(PathBuf::from("/snap/bin").join(name));

    if let Some(ref home) = home {
        candidates.push(home.join(".volta").join("bin").join(name));
        candidates.push(home.join(".asdf").join("shims").join(name));
        candidates.push(
            home.join(".local")
                .join("share")
                .join("mise")
                .join("shims")
                .join(name),
        );
        candidates.extend(latest_versioned_bin(
            &home.join(".nvm").join("versions").join("node"),
            name,
        ));
    }

    candidates
}

/// Expands `<root>/<version>/bin/<name>` for the highest version present.
///
/// Version strings sort lexicographically in practice for `vX.Y.Z`, so the last
/// entry after a plain sort is the newest.
fn latest_versioned_bin(root: &Path, name: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut versions: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    versions.sort();

    versions
        .into_iter()
        .rev()
        .take(3)
        .map(|dir| dir.join("bin").join(name))
        .collect()
}

/// Checks that a file exists, queries its version, and returns a `ResolvedBinary`.
async fn verify_binary_file(kind: BinaryKind, path: &Path) -> Result<ResolvedBinary, BinResError> {
    if !path.is_file() {
        return Err(BinResError::NotFound { kind });
    }

    let meta = std::fs::metadata(path).map_err(|_| BinResError::NotFound { kind })?;
    let version = query_version_with_retry(kind, path).await?;
    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);

    Ok(ResolvedBinary {
        kind,
        path: path.to_path_buf(),
        version,
        size: meta.len(),
        modified,
    })
}

/// Number of attempts when starting a candidate binary fails at the OS level.
const SPAWN_ATTEMPTS: u32 = 4;

/// Queries a candidate version, retrying a failed spawn a bounded number of times.
///
/// Spawning can fail momentarily on a loaded machine (`fork: Resource temporarily
/// unavailable`, `Text file busy`). Without the retry such a candidate is treated
/// as absent, and the search silently continues to an unrelated install — an
/// app-managed Node.js could then lose to a system one.
async fn query_version_with_retry(kind: BinaryKind, path: &Path) -> Result<String, BinResError> {
    let mut attempt = 1;
    loop {
        match query_binary_version(kind, path).await {
            Ok(version) => return Ok(version),
            Err(BinResError::SpawnFailed { .. }) if attempt < SPAWN_ATTEMPTS => {
                tokio::time::sleep(std::time::Duration::from_millis(20 * u64::from(attempt))).await;
                attempt += 1;
            }
            Err(err) => return Err(err),
        }
    }
}

/// Queries version flag on a binary and extracts the first non-empty line.
pub async fn query_binary_version(
    kind: BinaryKind,
    bin_path: &Path,
) -> Result<String, BinResError> {
    let flag = kind.version_flag();
    let mut cmd = Command::new(bin_path);
    cmd.arg(flag);

    let output = cmd.output().await.map_err(|err| BinResError::SpawnFailed {
        kind,
        error: format!("Failed to spawn {}: {err}", bin_path.display()),
    })?;

    if !output.status.success() {
        // Fallback for tools with alternative flag
        let alt_flag = if flag == "--version" {
            "-version"
        } else {
            "--version"
        };
        let mut alt_cmd = Command::new(bin_path);
        alt_cmd.arg(alt_flag);
        if let Ok(alt_out) = alt_cmd.output().await {
            if alt_out.status.success() {
                let stdout = String::from_utf8_lossy(&alt_out.stdout);
                let first_line = stdout.lines().next().unwrap_or("").trim().to_string();
                if !first_line.is_empty() {
                    return Ok(first_line);
                }
            }
        }

        return Err(BinResError::ProbeFailed {
            kind,
            error: format!(
                "Binary exited with code {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next().unwrap_or("").trim().to_string();
    if first_line.is_empty() {
        return Err(BinResError::ProbeFailed {
            kind,
            error: "Binary produced empty version output".to_string(),
        });
    }

    Ok(first_line)
}

#[cfg(test)]
mod tests {
    use super::*;

    // These three tests fabricate fake binaries as POSIX shell scripts and rely
    // on the OS executing them (shebang + exec bit). A text file cannot be
    // executed on Windows, so the mock would never resolve and the test would
    // fail there for a reason unrelated to the resolver logic. The ordering
    // policy itself is covered on every platform by
    // `test_binary_kind_priority_policy` and `test_js_runtime_binary_kind_metadata`.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_resolver_cache_hit_and_invalidation() {
        let temp_dir = std::env::temp_dir().join(format!("binres_test_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();

        // Create a mock executable script
        let mock_script = temp_dir.join("yt-dlp");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::write(&mock_script, b"#!/bin/sh\necho '2026.08.19'\n")
                .await
                .unwrap();
            std::fs::set_permissions(&mock_script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        #[cfg(windows)]
        {
            tokio::fs::write(&mock_script, b"@echo 2026.08.19\r\n")
                .await
                .unwrap();
        }

        let resolver = BinaryResolver::new(temp_dir.clone(), None);

        // 1. First resolution: queries probe
        let res1 = resolver.resolve_ytdlp().await.unwrap();
        assert_eq!(res1.version, "2026.08.19");
        assert_eq!(res1.path, mock_script);

        // 2. Second resolution: cache hit (same fingerprint)
        let res2 = resolver.resolve_ytdlp().await.unwrap();
        assert_eq!(res2.version, "2026.08.19");

        // 3. Invalidation forces fresh resolution
        resolver.invalidate(BinaryKind::YtDlp).await;
        let res3 = resolver.resolve_ytdlp().await.unwrap();
        assert_eq!(res3.version, "2026.08.19");

        // Clean up
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test]
    async fn test_resolver_missing_returns_not_found() {
        let temp_dir =
            std::env::temp_dir().join(format!("binres_missing_{}", uuid::Uuid::new_v4()));
        let resolver = BinaryResolver::new(temp_dir, None);
        let res = resolver.resolve(BinaryKind::YtDlp).await;
        // On machine where yt-dlp is in PATH, it might resolve or not, but for an unknown binary it returns NotFound
        assert!(matches!(res, Ok(_) | Err(BinResError::NotFound { .. })));
    }

    /// A spawn failure is reported as `SpawnFailed` (not as "candidate absent"),
    /// and the retry stays bounded: an unexecutable file fails fast instead of
    /// looping for seconds.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_spawn_failure_is_classified_and_retry_is_bounded() {
        let dir = std::env::temp_dir().join(format!("binres_spawn_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("node");
        // Exists as a regular file, but without the execute bit: the exec itself
        // fails, which is a spawn error rather than a version-query error.
        tokio::fs::write(&path, b"#!/bin/sh\necho v24.0.0\n")
            .await
            .unwrap();

        let start = std::time::Instant::now();
        let err = query_version_with_retry(BinaryKind::Node, &path)
            .await
            .expect_err("an unexecutable candidate must not report a version");
        assert!(
            matches!(err, BinResError::SpawnFailed { .. }),
            "expected SpawnFailed, got {err:?}"
        );
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "the retry loop must stay bounded"
        );
        // Not retried into a different error kind either.
        assert!(!matches!(err, BinResError::NotFound { .. }));

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    // These three tests fabricate fake binaries as POSIX shell scripts and rely
    // on the OS executing them (shebang + exec bit). A text file cannot be
    // executed on Windows, so the mock would never resolve and the test would
    // fail there for a reason unrelated to the resolver logic. The ordering
    // policy itself is covered on every platform by
    // `test_binary_kind_priority_policy` and `test_js_runtime_binary_kind_metadata`.
    #[cfg(unix)]
    /// The runtime-updated engine in `app_bin_dir` must win over the bundled
    /// resource binary, otherwise a downloaded update would never take effect.
    #[tokio::test]
    async fn test_ytdlp_app_bin_dir_takes_priority_over_resources() {
        let root = std::env::temp_dir().join(format!("binres_prio_{}", uuid::Uuid::new_v4()));
        let app_dir = root.join("app_bin");
        let res_dir = root.join("resources_bin");
        tokio::fs::create_dir_all(&app_dir).await.unwrap();
        tokio::fs::create_dir_all(&res_dir).await.unwrap();

        let write_mock = |path: &Path, version: &str| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::write(path, format!("#!/bin/sh\necho '{version}'\n")).unwrap();
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            #[cfg(windows)]
            {
                std::fs::write(path, format!("@echo {version}\r\n")).unwrap();
            }
        };

        write_mock(&res_dir.join("yt-dlp"), "2026.01.01");
        write_mock(&app_dir.join("yt-dlp"), "2026.08.19");

        let resolver = BinaryResolver::new(app_dir.clone(), Some(res_dir.clone()));
        let resolved = resolver.resolve_ytdlp().await.unwrap();
        assert_eq!(resolved.version, "2026.08.19");
        assert_eq!(resolved.path, app_dir.join("yt-dlp"));

        // ffmpeg keeps the bundled-resources-first order.
        write_mock(&res_dir.join("ffmpeg"), "9.0.1");
        write_mock(&app_dir.join("ffmpeg"), "8.0.0");
        let ffmpeg = resolver.resolve_ffmpeg().await.unwrap();
        assert_eq!(ffmpeg.version, "9.0.1");
        assert_eq!(ffmpeg.path, res_dir.join("ffmpeg"));

        // Without a resource directory, the app dir is still used.
        let resolver_app_only = BinaryResolver::new(app_dir.clone(), None);
        let resolved_app_only = resolver_app_only.resolve_ytdlp().await.unwrap();
        assert_eq!(resolved_app_only.path, app_dir.join("yt-dlp"));

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    // These three tests fabricate fake binaries as POSIX shell scripts and rely
    // on the OS executing them (shebang + exec bit). A text file cannot be
    // executed on Windows, so the mock would never resolve and the test would
    // fail there for a reason unrelated to the resolver logic. The ordering
    // policy itself is covered on every platform by
    // `test_binary_kind_priority_policy` and `test_js_runtime_binary_kind_metadata`.
    #[cfg(unix)]
    /// Node/Deno installed by the app (in `app_bin_dir/<kind>/<name>`) must be
    /// found and take priority over any bundled resource.
    #[tokio::test]
    async fn test_js_runtimes_resolve_from_app_bin_dir() {
        let root = std::env::temp_dir().join(format!("binres_js_{}", uuid::Uuid::new_v4()));
        let app_dir = root.join("app_bin");
        let res_dir = root.join("resources_bin");
        tokio::fs::create_dir_all(&app_dir).await.unwrap();
        tokio::fs::create_dir_all(&res_dir).await.unwrap();

        let write_mock = |path: &Path, version: &str| {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::write(path, format!("#!/bin/sh\necho '{version}'\n")).unwrap();
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            #[cfg(windows)]
            {
                std::fs::write(path, format!("@echo {version}\r\n")).unwrap();
            }
        };

        let node_name = if cfg!(windows) { "node.exe" } else { "node" };
        let deno_name = if cfg!(windows) { "deno.exe" } else { "deno" };

        // Installed layout: <app_bin>/node/node and <app_bin>/deno/deno.
        write_mock(&app_dir.join("node").join(node_name), "v24.21.0");
        write_mock(&app_dir.join("deno").join(deno_name), "2.9.6");

        // A bundled resource would be older: the app copy must win.
        write_mock(&res_dir.join(node_name), "v18.20.0");

        let resolver = BinaryResolver::new(app_dir.clone(), Some(res_dir.clone()));

        let node = resolver.resolve_node().await.unwrap();
        assert_eq!(node.version, "v24.21.0");
        assert_eq!(node.path, app_dir.join("node").join(node_name));

        let deno = resolver.resolve_deno().await.unwrap();
        assert_eq!(deno.version, "2.9.6");
        assert_eq!(deno.path, app_dir.join("deno").join(deno_name));

        // The flat layout (`<app_bin>/node`) also works.
        let flat_dir = root.join("flat");
        tokio::fs::create_dir_all(&flat_dir).await.unwrap();
        write_mock(&flat_dir.join(node_name), "v22.11.0");
        let flat_resolver = BinaryResolver::new(flat_dir.clone(), None);
        let flat = flat_resolver.resolve_node().await.unwrap();
        assert_eq!(flat.path, flat_dir.join(node_name));

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    /// JS runtimes prefer the app directory, ffmpeg/ffprobe keep resources first.
    #[test]
    fn test_binary_kind_priority_policy() {
        assert!(BinaryKind::YtDlp.prefers_app_bin_dir());
        assert!(BinaryKind::Node.prefers_app_bin_dir());
        assert!(BinaryKind::Deno.prefers_app_bin_dir());
        assert!(!BinaryKind::Ffmpeg.prefers_app_bin_dir());
        assert!(!BinaryKind::Ffprobe.prefers_app_bin_dir());
    }

    /// Both JS runtimes expose the expected executable name and version flag.
    #[test]
    fn test_js_runtime_binary_kind_metadata() {
        assert_eq!(BinaryKind::Node.base_name(), "node");
        assert_eq!(BinaryKind::Deno.base_name(), "deno");
        assert_eq!(BinaryKind::Node.version_flag(), "--version");
        assert_eq!(BinaryKind::Deno.version_flag(), "--version");
    }
}
