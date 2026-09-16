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

    /// Fake home directory that is absolute on every platform.
    ///
    /// A literal like `/Users/alice` is absolute on Unix but *not* on Windows,
    /// where the resolver (rightly) rejects it as a relative path.
    fn fake_home() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\Users\alice")
        } else {
            PathBuf::from("/Users/alice")
        }
    }

    /// Builds an absolute path valid on every platform.
    fn abs_path(suffix: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!(r"C:\polysaver_test\{}", suffix.replace('/', r"\")))
        } else {
            PathBuf::from(format!("/polysaver_test/{suffix}"))
        }
    }

    #[test]
    fn test_resolve_tilde_only() {
        let home = fake_home();
        let res = resolve_user_directory("~", &home).unwrap();
        assert_eq!(res, home);

        let res_spaced = resolve_user_directory("  ~  ", &home).unwrap();
        assert_eq!(res_spaced, home);
    }

    #[test]
    fn test_resolve_tilde_subdirectories() {
        let home = fake_home();

        // ~/documents -> <home>/documents (never a root-level /documents)
        let res1 = resolve_user_directory("~/documents", &home).unwrap();
        assert_eq!(res1, home.join("documents"));
        assert_ne!(res1, PathBuf::from("/documents"));

        // ~/Documents/PolySaver
        let res2 = resolve_user_directory("~/Documents/PolySaver", &home).unwrap();
        assert_eq!(res2, home.join("Documents").join("PolySaver"));

        // Windows backslash notation ~\Downloads\PolySaver
        let res3 = resolve_user_directory(r"~\Downloads\PolySaver", &home).unwrap();
        assert_eq!(res3, home.join(Path::new(r"Downloads\PolySaver")));
    }

    #[test]
    fn test_reject_invalid_tilde_forms() {
        let home = fake_home();

        // ~other rejected
        let err1 = resolve_user_directory("~other", &home);
        assert!(err1.is_err());
        assert_eq!(err1.unwrap_err().code, "INVALID_DIRECTORY");

        // embedded tilde rejected
        let err2 = resolve_user_directory("abc/~/test", &home);
        assert!(err2.is_err());
        assert_eq!(err2.unwrap_err().code, "INVALID_DIRECTORY");
    }

    #[test]
    fn test_reject_relative_paths_and_empty() {
        let home = fake_home();

        // empty
        assert!(resolve_user_directory("", &home).is_err());
        assert!(resolve_user_directory("   ", &home).is_err());

        // relative
        assert!(resolve_user_directory("documents", &home).is_err());
        assert!(resolve_user_directory("../documents", &home).is_err());
        assert!(resolve_user_directory("./downloads", &home).is_err());

        // null byte
        let with_null = format!("{}/\0danger", home.display());
        assert!(resolve_user_directory(&with_null, &home).is_err());
    }

    #[test]
    fn test_reject_tilde_escaping_home() {
        let home = fake_home();

        // ~/../Shared escapes the home directory
        let err = resolve_user_directory("~/../Shared", &home);
        assert!(err.is_err());
        assert_eq!(err.unwrap_err().code, "INVALID_DIRECTORY");
    }

    #[test]
    fn test_preserve_explicit_absolute_paths() {
        let home = fake_home();

        // External volume, passed through untouched
        let external = abs_path("Volumes/ExternalSSD/PolySaver");
        let res1 = resolve_user_directory(&external.to_string_lossy(), &home).unwrap();
        assert_eq!(res1, external);

        // Another absolute path, with lexical normalization of `.` and `..`
        let messy = abs_path("storage/./media/../media/vids");
        let res2 = resolve_user_directory(&messy.to_string_lossy(), &home).unwrap();
        assert_eq!(res2, abs_path("storage/media/vids"));
    }
}
