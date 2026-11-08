// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::dto::IpcError;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// GitHub API endpoint of the repository that publishes the releases, the
/// same one the updater endpoint in `tauri.conf.json` points at.
const RELEASES_API: &str =
    "https://api.github.com/repos/AinsiParlaitZarathoustra/PolySaver/releases/tags/v";

/// Release texts are a few KB; anything far larger is not one of ours.
const MAX_RELEASE_BODY_BYTES: usize = 256 * 1024;

/// Name and text of a published release, as written on GitHub.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseNotesDto {
    pub name: Option<String>,
    pub body: Option<String>,
}

#[derive(Deserialize)]
struct GithubRelease {
    name: Option<String>,
    body: Option<String>,
}

/// True for a plain `MAJOR.MINOR.PATCH` version, the only shape ever put in
/// the request URL (it comes from the updater, but is still checked).
fn is_plain_version(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit()))
}

/// IPC command returning the name and text of release `version`, so the update
/// dialog shows what is new.
///
/// The updater manifest only carries a one-line note, while the GitHub release
/// holds the full text (later enriched with screenshots). The frontend keeps
/// only the "what's new" part of it. Only the fixed releases endpoint is
/// contacted, with a bounded timeout; any failure leaves the dialog working
/// without the notes.
#[tauri::command]
pub async fn get_release_notes(version: String) -> Result<ReleaseNotesDto, IpcError> {
    if !is_plain_version(&version) {
        return Err(IpcError::new(
            "INVALID_VERSION",
            format!("Invalid release version: '{version}'"),
        ));
    }

    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .user_agent(concat!("PolySaver/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|err| IpcError::new("RELEASE_NOTES_FAILED", err.to_string()))?;

    let response = client
        .get(format!("{RELEASES_API}{version}"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|err| IpcError::new("RELEASE_NOTES_FAILED", err.to_string()))?;
    if !response.status().is_success() {
        return Err(IpcError::new(
            "RELEASE_NOTES_FAILED",
            format!("GitHub answered HTTP {}", response.status()),
        ));
    }

    let raw = response
        .text()
        .await
        .map_err(|err| IpcError::new("RELEASE_NOTES_FAILED", err.to_string()))?;
    let release: GithubRelease = serde_json::from_str(&raw)
        .map_err(|err| IpcError::new("RELEASE_NOTES_FAILED", err.to_string()))?;

    Ok(ReleaseNotesDto {
        name: release.name,
        body: release
            .body
            .filter(|body| body.len() <= MAX_RELEASE_BODY_BYTES),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_only_plain_versions_reach_the_url() {
        assert!(is_plain_version("2.7.0"));
        assert!(is_plain_version("10.0.12"));
        for bad in [
            "2.7",
            "2.7.0-beta",
            "../2.7.0",
            "2.7.0/x",
            "v2.7.0",
            "",
            "2..0",
        ] {
            assert!(!is_plain_version(bad), "{bad} must be refused");
        }
    }

    /// Real request against a published release (needs the network).
    #[tokio::test]
    #[ignore = "Requires live network access. Run with POLYSAVER_RUN_NETWORK_E2E=1"]
    async fn test_real_release_notes_are_fetched() {
        if std::env::var("POLYSAVER_RUN_NETWORK_E2E").as_deref() != Ok("1") {
            return;
        }
        let notes = get_release_notes("2.6.0".to_string()).await.unwrap();
        assert_eq!(notes.name.as_deref(), Some("PolySaver 2.6.0"));
        let body = notes.body.expect("release text");
        assert!(body.contains("PolySaver 2.6.0"), "unexpected body: {body}");
    }
}
