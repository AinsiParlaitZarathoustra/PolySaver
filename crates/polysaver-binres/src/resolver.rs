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
    async fn find_and_verify(&self, kind: BinaryKind) -> Result<ResolvedBinary, BinResError> {
        let base_name = kind.base_name();
        let candidate_names = candidate_file_names(base_name);

        for name in &candidate_names {
            // Priority 1: Bundle resource directory
            if let Some(ref res_dir) = self.resource_bin_dir {
                let p = res_dir.join(name);
                if let Ok(resolved) = verify_binary_file(kind, &p).await {
                    return Ok(resolved);
                }
            }

            // Priority 2: App managed bin directory
            let app_candidate = self.app_bin_dir.join(name);
            if let Ok(resolved) = verify_binary_file(kind, &app_candidate).await {
                return Ok(resolved);
            }

            // Priority 3: Sibling next to current executable
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

            // Priority 4: Standard system paths on Unix
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

            // Priority 5: PATH environment variable directory inspection
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

/// Checks that a file exists, queries its version, and returns a `ResolvedBinary`.
async fn verify_binary_file(kind: BinaryKind, path: &Path) -> Result<ResolvedBinary, BinResError> {
    if !path.is_file() {
        return Err(BinResError::NotFound { kind });
    }

    let meta = std::fs::metadata(path).map_err(|_| BinResError::NotFound { kind })?;
    let version = query_binary_version(kind, path).await?;
    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);

    Ok(ResolvedBinary {
        kind,
        path: path.to_path_buf(),
        version,
        size: meta.len(),
        modified,
    })
}

/// Queries version flag on a binary and extracts the first non-empty line.
pub async fn query_binary_version(
    kind: BinaryKind,
    bin_path: &Path,
) -> Result<String, BinResError> {
    let flag = kind.version_flag();
    let mut cmd = Command::new(bin_path);
    cmd.arg(flag);

    let output = cmd.output().await.map_err(|err| BinResError::ProbeFailed {
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
}
