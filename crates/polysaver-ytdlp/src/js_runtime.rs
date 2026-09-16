// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! # JavaScript Runtime Support for yt-dlp
//!
//! yt-dlp needs an external JavaScript runtime to solve YouTube's `n`/signature
//! challenges. Without one, YouTube serves an incomplete format list ("Only
//! images are available", late `HTTP Error 403`).
//!
//! Detection prefers **Deno** over **Node**: `deno` is enabled by default in
//! yt-dlp and takes priority over Node, so passing Node while Deno is installed
//! would be ignored unless `--no-js-runtimes` were used (which would degrade a
//! working Deno setup).
//!
//! Minimum versions are the thresholds enforced by yt-dlp itself:
//! Deno ≥ 2.3.0, Node ≥ 22.0.0.

use polysaver_binres::{BinResError, BinaryKind, BinaryResolver, ResolvedBinary};
use polysaver_core::error::CoreError;
use polysaver_core::ports::media_downloader::JsRuntimeSpec;
use std::path::Path;

/// Minimum Deno version enforced by yt-dlp.
pub const MIN_DENO_VERSION: &str = "2.3.0";
/// Minimum Node.js version enforced by yt-dlp.
pub const MIN_NODE_VERSION: &str = "22.0.0";

/// Detected JavaScript runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsRuntimeKind {
    Deno,
    Node,
}

impl JsRuntimeKind {
    /// Value used in `--js-runtimes <kind>:<path>`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Deno => "deno",
            Self::Node => "node",
        }
    }

    /// Corresponding binary kind for resolver lookups.
    #[must_use]
    pub const fn binary_kind(self) -> BinaryKind {
        match self {
            Self::Deno => BinaryKind::Deno,
            Self::Node => BinaryKind::Node,
        }
    }
}

/// Result of probing the machine for a usable JavaScript runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsRuntimeStatus {
    pub kind: Option<JsRuntimeKind>,
    pub version: Option<String>,
    pub path: Option<String>,
    /// True when a runtime was found *and* satisfies the minimum version.
    pub is_usable: bool,
    /// Version detected but below the yt-dlp threshold.
    pub version_too_old: bool,
}

impl JsRuntimeStatus {
    /// Status when nothing usable was found.
    #[must_use]
    pub fn missing() -> Self {
        Self {
            kind: None,
            version: None,
            path: None,
            is_usable: false,
            version_too_old: false,
        }
    }

    /// Converts to the port type consumed by the yt-dlp adapter.
    #[must_use]
    pub fn to_spec(&self) -> Option<JsRuntimeSpec> {
        match (self.kind, &self.path) {
            (Some(kind), Some(path)) if self.is_usable => Some(JsRuntimeSpec {
                name: kind.as_str(),
                path: path.clone(),
            }),
            _ => None,
        }
    }
}

/// Extracts a numeric version from `deno --version` or `node --version` output.
///
/// Deno prints `deno 2.9.6 (stable, release, ...)`; Node prints `v24.21.0`.
/// Returns `None` when no numeric version is present.
#[must_use]
pub fn parse_runtime_version(kind: JsRuntimeKind, raw_output: &str) -> Option<String> {
    let first_line = raw_output.lines().next()?.trim();
    match kind {
        JsRuntimeKind::Deno => {
            // `deno 2.9.6 (...)` — take the first token that parses as a version.
            for token in first_line.split_whitespace() {
                let candidate = token.trim_start_matches('v');
                if candidate.chars().next().is_some_and(|c| c.is_ascii_digit())
                    && candidate.contains('.')
                {
                    return Some(candidate.to_string());
                }
            }
            None
        }
        JsRuntimeKind::Node => {
            let candidate = first_line.trim_start_matches('v');
            let numeric = candidate.chars().next().is_some_and(|c| c.is_ascii_digit());
            (numeric && candidate.contains('.')).then(|| candidate.to_string())
        }
    }
}

/// Compares two dotted numeric versions. Returns `true` when `version >= minimum`.
///
/// Non-numeric versions are treated as not satisfying the requirement.
#[must_use]
pub fn version_meets_minimum(version: &str, minimum: &str) -> bool {
    let parse = |v: &str| -> Option<Vec<u64>> {
        v.split('.')
            .map(|part| {
                // Tolerate suffixes such as `-rc.1` by taking the leading digits.
                let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
                digits.parse::<u64>().ok()
            })
            .collect()
    };

    let (Some(found), Some(required)) = (parse(version), parse(minimum)) else {
        return false;
    };

    for i in 0..found.len().max(required.len()) {
        let a = found.get(i).copied().unwrap_or(0);
        let b = required.get(i).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    true
}

/// Probes a single runtime kind through the shared resolver.
async fn probe_kind(resolver: &BinaryResolver, kind: JsRuntimeKind) -> Option<JsRuntimeStatus> {
    let resolved: ResolvedBinary = match kind {
        JsRuntimeKind::Deno => resolver.resolve_deno().await.ok()?,
        JsRuntimeKind::Node => resolver.resolve_node().await.ok()?,
    };

    let minimum = match kind {
        JsRuntimeKind::Deno => MIN_DENO_VERSION,
        JsRuntimeKind::Node => MIN_NODE_VERSION,
    };

    let version =
        parse_runtime_version(kind, &resolved.version).unwrap_or(resolved.version.clone());
    let meets = version_meets_minimum(&version, minimum);

    Some(JsRuntimeStatus {
        kind: Some(kind),
        version: Some(version),
        path: Some(resolved.path.to_string_lossy().to_string()),
        is_usable: meets,
        version_too_old: !meets,
    })
}

/// Detects the best available JavaScript runtime: Deno first, then Node.
///
/// When Deno is present but too old, Node is still considered (a stale Deno must
/// not mask a working Node installation).
pub async fn detect_js_runtime(resolver: &BinaryResolver) -> JsRuntimeStatus {
    let deno = probe_kind(resolver, JsRuntimeKind::Deno).await;
    if let Some(status) = deno.as_ref() {
        if status.is_usable {
            return status.clone();
        }
    }

    let node = probe_kind(resolver, JsRuntimeKind::Node).await;
    if let Some(status) = node.as_ref() {
        if status.is_usable {
            return status.clone();
        }
    }

    // Nothing usable: report the stale Deno/Node so the UI can explain why.
    deno.or(node).unwrap_or_else(JsRuntimeStatus::missing)
}

/// Version of Node installed by PolySaver, pinned at build time.
///
/// The value is read from `node-version.txt` at the repository root, the single
/// source of truth shared with `scripts/prepare-sidecars.sh`.
pub const PINNED_NODE_VERSION: &str = include_str!("../../../node-version.txt");

/// Returns the pinned Node version without surrounding whitespace.
#[must_use]
pub fn pinned_node_version() -> &'static str {
    PINNED_NODE_VERSION.trim()
}

/// Verifies an installed runtime binary reports a supported version.
pub async fn verify_installed_runtime(
    path: &Path,
    kind: JsRuntimeKind,
) -> Result<String, CoreError> {
    let output = tokio::process::Command::new(path)
        .arg("--version")
        .output()
        .await
        .map_err(|err| {
            CoreError::ProviderError(format!("Failed to run the installed runtime: {err}"))
        })?;

    if !output.status.success() {
        return Err(CoreError::ProviderError(
            "The installed runtime failed to report its version".to_string(),
        ));
    }

    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    let version = parse_runtime_version(kind, &raw).ok_or_else(|| {
        CoreError::ProviderError("Could not parse the runtime version output".to_string())
    })?;

    let minimum = match kind {
        JsRuntimeKind::Deno => MIN_DENO_VERSION,
        JsRuntimeKind::Node => MIN_NODE_VERSION,
    };
    if !version_meets_minimum(&version, minimum) {
        return Err(CoreError::ProviderError(format!(
            "Installed runtime version {version} is below the required minimum {minimum}"
        )));
    }

    Ok(version)
}

/// Convenience wrapper used by the composition root to resolve a runtime path.
pub async fn resolve_runtime_binary(
    resolver: &BinaryResolver,
    kind: JsRuntimeKind,
) -> Result<ResolvedBinary, CoreError> {
    let result = match kind {
        JsRuntimeKind::Deno => resolver.resolve_deno().await,
        JsRuntimeKind::Node => resolver.resolve_node().await,
    };
    result.map_err(|err: BinResError| CoreError::ProviderError(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_node_version_output() {
        assert_eq!(
            parse_runtime_version(JsRuntimeKind::Node, "v24.21.0\n"),
            Some("24.21.0".to_string())
        );
        assert_eq!(
            parse_runtime_version(JsRuntimeKind::Node, "v22.11.0"),
            Some("22.11.0".to_string())
        );
        // Garbage output must not produce a version.
        assert_eq!(
            parse_runtime_version(JsRuntimeKind::Node, "command not found"),
            None
        );
        assert_eq!(parse_runtime_version(JsRuntimeKind::Node, ""), None);
    }

    #[test]
    fn test_parse_deno_version_output() {
        assert_eq!(
            parse_runtime_version(
                JsRuntimeKind::Deno,
                "deno 2.9.6 (stable, release, aarch64-apple-darwin)\n"
            ),
            Some("2.9.6".to_string())
        );
        assert_eq!(
            parse_runtime_version(JsRuntimeKind::Deno, "deno 2.3.0"),
            Some("2.3.0".to_string())
        );
        assert_eq!(parse_runtime_version(JsRuntimeKind::Deno, "not deno"), None);
    }

    #[test]
    fn test_version_minimum_thresholds() {
        // Node: 22.0.0 required.
        assert!(version_meets_minimum("22.0.0", MIN_NODE_VERSION));
        assert!(version_meets_minimum("24.21.0", MIN_NODE_VERSION));
        assert!(version_meets_minimum(
            "v24.21.0".trim_start_matches('v'),
            MIN_NODE_VERSION
        ));
        assert!(!version_meets_minimum("21.7.3", MIN_NODE_VERSION));
        assert!(!version_meets_minimum("20.0.0", MIN_NODE_VERSION));

        // Deno: 2.3.0 required.
        assert!(version_meets_minimum("2.3.0", MIN_DENO_VERSION));
        assert!(version_meets_minimum("2.9.6", MIN_DENO_VERSION));
        assert!(!version_meets_minimum("2.2.9", MIN_DENO_VERSION));
        assert!(!version_meets_minimum("1.46.0", MIN_DENO_VERSION));

        // Unparseable versions never satisfy a minimum.
        assert!(!version_meets_minimum("nightly", MIN_NODE_VERSION));
        assert!(!version_meets_minimum("", MIN_NODE_VERSION));
    }

    #[test]
    fn test_status_to_spec_requires_usable_runtime() {
        let usable = JsRuntimeStatus {
            kind: Some(JsRuntimeKind::Deno),
            version: Some("2.9.6".to_string()),
            path: Some("/usr/local/bin/deno".to_string()),
            is_usable: true,
            version_too_old: false,
        };
        let spec = usable.to_spec().unwrap();
        assert_eq!(spec.name, "deno");
        assert_eq!(spec.path, "/usr/local/bin/deno");

        let too_old = JsRuntimeStatus {
            is_usable: false,
            version_too_old: true,
            ..usable.clone()
        };
        assert!(too_old.to_spec().is_none());

        assert!(JsRuntimeStatus::missing().to_spec().is_none());
    }

    #[test]
    fn test_pinned_node_version_is_committed_and_parseable() {
        let version = pinned_node_version();
        assert!(
            version.starts_with('2'),
            "unexpected pinned Node version: {version}"
        );
        assert!(version.split('.').count() >= 3);
        // The pinned version must itself satisfy the yt-dlp minimum.
        assert!(version_meets_minimum(version, MIN_NODE_VERSION));
    }

    #[tokio::test]
    async fn test_verify_installed_runtime_rejects_old_versions() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir =
                std::env::temp_dir().join(format!("polysaver_jstest_{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();

            let fake = dir.join("node");
            std::fs::write(&fake, "#!/bin/sh\necho 'v18.20.0'\n").unwrap();
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

            let result = verify_installed_runtime(&fake, JsRuntimeKind::Node).await;
            assert!(result.is_err(), "v18 must be rejected as below the minimum");

            std::fs::write(&fake, "#!/bin/sh\necho 'v24.21.0'\n").unwrap();
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
            let ok = verify_installed_runtime(&fake, JsRuntimeKind::Node).await;
            assert_eq!(ok.unwrap(), "24.21.0");

            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
