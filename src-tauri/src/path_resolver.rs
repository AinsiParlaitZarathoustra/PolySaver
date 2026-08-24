// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! Path resolution helper for Tauri IPC boundaries.
//!
//! Translates shell tilde (`~`) notation to the actual user home directory and
//! performs pure lexical path normalization without filesystem access.

use crate::dto::IpcError;
use std::path::{Component, Path, PathBuf};

/// Lexically normalizes a path by removing redundant `.` and resolving `..` components.
///
/// This operation is purely lexical and does not access the filesystem or follow symlinks.
#[must_use]
pub fn normalize_lexical(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(Component::Normal(_)) = components.last() {
                    components.pop();
                }
            }
            c => components.push(c),
        }
    }
    components.into_iter().collect()
}

/// Resolves user-provided raw directory strings (including `~` and `~/...`) to a validated absolute path.
///
/// # Invariants
/// - Trims outer whitespace only.
/// - Rejects empty strings and strings containing null bytes (`\0`).
/// - Rejects relative paths (e.g. `documents`, `../documents`).
/// - Expands `~` to `home_dir`, and `~/...` (or `~\...` on Windows) to `home_dir.join(...)`.
/// - Rejects invalid tilde usage (e.g. `~other`, `abc/~/test`).
/// - Prevents tilde paths from escaping `home_dir` via `..` (e.g. `~/../Shared`).
/// - Preserves explicitly provided absolute paths (e.g. `/Volumes/External/Downloads`).
#[allow(clippy::result_large_err)]
pub fn resolve_user_directory(raw: &str, home_dir: &Path) -> Result<PathBuf, IpcError> {
    let trimmed = raw.trim();

    if trimmed.is_empty() {
        return Err(IpcError::new(
            "INVALID_DIRECTORY",
            "Download directory cannot be empty.",
        ));
    }

    if trimmed.contains('\0') {
        return Err(IpcError::new(
            "INVALID_DIRECTORY",
            "Download directory path contains null byte.",
        ));
    }

    let (raw_combined, is_tilde) = if trimmed == "~" {
        (home_dir.to_path_buf(), true)
    } else if let Some(stripped) = trimmed.strip_prefix("~/") {
        (home_dir.join(stripped), true)
    } else if let Some(stripped) = trimmed.strip_prefix(r"~\") {
        (home_dir.join(stripped), true)
    } else if trimmed.contains('~') {
        return Err(IpcError::new(
            "INVALID_DIRECTORY",
            "Tilde is only allowed at the beginning as ~ or ~/.",
        ));
    } else {
        let p = Path::new(trimmed);
        if !p.is_absolute() {
            return Err(IpcError::new(
                "INVALID_DIRECTORY",
                "Relative directory path is not allowed. Please provide an absolute path or ~.",
            ));
        }
        (p.to_path_buf(), false)
    };

    let normalized = normalize_lexical(&raw_combined);

    if is_tilde {
        let normalized_home = normalize_lexical(home_dir);
        if !normalized.starts_with(&normalized_home) {
            return Err(IpcError::new(
                "INVALID_DIRECTORY",
                "Path escapes user home directory.",
            ));
        }
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_tilde_only() {
        let fake_home = Path::new("/Users/alice");
        let res = resolve_user_directory("~", fake_home).unwrap();
        assert_eq!(res, PathBuf::from("/Users/alice"));

        let res_spaced = resolve_user_directory("  ~  ", fake_home).unwrap();
        assert_eq!(res_spaced, PathBuf::from("/Users/alice"));
    }

    #[test]
    fn test_resolve_tilde_subdirectories() {
        let fake_home = Path::new("/Users/alice");

        // ~/documents -> /Users/alice/documents (never /documents)
        let res1 = resolve_user_directory("~/documents", fake_home).unwrap();
        assert_eq!(res1, PathBuf::from("/Users/alice/documents"));
        assert_ne!(res1, PathBuf::from("/documents"));

        // ~/Documents/PolySaver
        let res2 = resolve_user_directory("~/Documents/PolySaver", fake_home).unwrap();
        assert_eq!(res2, PathBuf::from("/Users/alice/Documents/PolySaver"));

        // Windows backslash notation ~\Downloads\PolySaver
        let res3 = resolve_user_directory(r"~\Downloads\PolySaver", fake_home).unwrap();
        assert_eq!(res3, fake_home.join(Path::new(r"Downloads\PolySaver")));
    }

    #[test]
    fn test_reject_invalid_tilde_forms() {
        let fake_home = Path::new("/Users/alice");

        // ~other rejected
        let err1 = resolve_user_directory("~other", fake_home);
        assert!(err1.is_err());
        assert_eq!(err1.unwrap_err().code, "INVALID_DIRECTORY");

        // embedded tilde rejected
        let err2 = resolve_user_directory("abc/~/test", fake_home);
        assert!(err2.is_err());
        assert_eq!(err2.unwrap_err().code, "INVALID_DIRECTORY");
    }

    #[test]
    fn test_reject_relative_paths_and_empty() {
        let fake_home = Path::new("/Users/alice");

        // empty
        assert!(resolve_user_directory("", fake_home).is_err());
        assert!(resolve_user_directory("   ", fake_home).is_err());

        // relative
        assert!(resolve_user_directory("documents", fake_home).is_err());
        assert!(resolve_user_directory("../documents", fake_home).is_err());
        assert!(resolve_user_directory("./downloads", fake_home).is_err());

        // null byte
        assert!(resolve_user_directory("/Users/alice/\0danger", fake_home).is_err());
    }

    #[test]
    fn test_reject_tilde_escaping_home() {
        let fake_home = Path::new("/Users/alice");

        // ~/../Shared escapes /Users/alice
        let err = resolve_user_directory("~/../Shared", fake_home);
        assert!(err.is_err());
        assert_eq!(err.unwrap_err().code, "INVALID_DIRECTORY");
    }

    #[test]
    fn test_preserve_explicit_absolute_paths() {
        let fake_home = Path::new("/Users/alice");

        // External drive
        let external = "/Volumes/ExternalSSD/PolySaver";
        let res1 = resolve_user_directory(external, fake_home).unwrap();
        assert_eq!(res1, PathBuf::from("/Volumes/ExternalSSD/PolySaver"));

        // Another absolute path with lexical normalization
        let res2 = resolve_user_directory("/opt/storage/./media/../media/vids", fake_home).unwrap();
        assert_eq!(res2, PathBuf::from("/opt/storage/media/vids"));
    }
}
