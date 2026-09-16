// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! # yt-dlp Runtime Engine Updater
//!
//! Downloads a recent yt-dlp release, verifies it cryptographically against the
//! release `SHA2-256SUMS`, installs it atomically into the writable app data
//! `bin` directory, and reports what was installed.
//!
//! ## Security constraints (hard requirements)
//!
//! Every HTTP request performed here is subject to:
//! 1. HTTPS only, with an explicit host allowlist (GitHub release infrastructure).
//! 2. Host validation on the *parsed* URL (`url::Url`), never on a raw string.
//! 3. Resolution of the host, validation of *all* A/AAAA answers against
//!    reserved/local ranges, and pinning of the validated addresses through a
//!    custom `reqwest::dns::Resolve` (anti-DNS-rebinding).
//! 4. Every redirect hop re-validated against the same allowlist.
//! 5. SHA-256 verified in streaming mode while the file is written; a mismatch
//!    aborts without touching the installed binary.

use polysaver_core::domain::engine_version::{
    compare_versions, current_utc_date, is_version_outdated,
};
use polysaver_core::domain::media_url::is_public_ip;
use polysaver_core::error::CoreError;
use reqwest::redirect;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
use url::Url;

/// Release channel of the yt-dlp engine.
///
/// Upstream recommends nightly builds to regular users because YouTube fixes
/// land there faster, but stable remains the default here for predictability.
/// The channel is an internal constant, never a user-facing setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YtDlpChannel {
    Stable,
    Nightly,
}

impl YtDlpChannel {
    /// GitHub repository hosting releases for this channel.
    #[must_use]
    pub const fn repo(self) -> &'static str {
        match self {
            Self::Stable => "yt-dlp/yt-dlp",
            Self::Nightly => "yt-dlp/yt-dlp-nightly-builds",
        }
    }

    /// Stable machine-readable channel name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Nightly => "nightly",
        }
    }
}

/// Maximum number of redirect hops followed during engine downloads.
const MAX_REDIRECTS: usize = 3;

/// How long a successful GitHub API release check stays valid (anti-rate-limit).
const UPDATE_CHECK_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Hosts allowed for engine downloads and metadata lookups.
/// Result of comparing the local engine with the latest known release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineUpdateStatus {
    pub current_version: Option<String>,
    pub latest_version: Option<String>,
    pub channel: String,
    pub outdated: bool,
    pub can_update: bool,
}

/// Outcome of a successful engine installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineUpdateOutcome {
    pub installed_version: String,
    pub updated: bool,
}

/// Persisted anti-rate-limit cache of the last GitHub release lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCheckCache {
    last_check_unix: u64,
    tag_name: String,
}

/// Runtime updater for the yt-dlp engine.
#[derive(Debug, Clone)]
pub struct YtDlpUpdater {
    client: reqwest::Client,
    app_bin_dir: PathBuf,
    channel: YtDlpChannel,
    cache_file: Option<PathBuf>,
}

impl YtDlpUpdater {
    /// Builds an updater writing into `app_bin_dir`, using the default stable channel.
    pub fn new(app_bin_dir: PathBuf, cache_file: Option<PathBuf>) -> Result<Self, CoreError> {
        Self::with_channel(app_bin_dir, cache_file, YtDlpChannel::Stable)
    }

    /// Builds an updater for an explicit channel (internal use/tests).
    pub fn with_channel(
        app_bin_dir: PathBuf,
        cache_file: Option<PathBuf>,
        channel: YtDlpChannel,
    ) -> Result<Self, CoreError> {
        let client = build_download_client()?;

        Ok(Self {
            client,
            app_bin_dir,
            channel,
            cache_file,
        })
    }

    /// Returns the channel used by this updater.
    #[must_use]
    pub const fn channel(&self) -> YtDlpChannel {
        self.channel
    }

    /// Checks whether a newer engine release is available, without downloading it.
    ///
    /// The GitHub API is queried at most once per 24h; the last known tag is
    /// cached on disk and reused (also as a fallback on rate limiting).
    pub async fn check_update(
        &self,
        current_version: Option<&str>,
    ) -> Result<EngineUpdateStatus, CoreError> {
        let cached = self.load_cache().await;
        let latest_version = match self.fresh_cached_tag(&cached) {
            Some(tag) => Some(tag),
            None => {
                let fetched = self.fetch_latest_tag().await;
                match fetched {
                    Ok(tag) => {
                        self.save_cache(&tag).await;
                        Some(tag)
                    }
                    Err(err) => match cached.as_ref() {
                        // Fall back to the last known tag rather than failing hard.
                        Some(entry) => Some(entry.tag_name.clone()),
                        None => return Err(err),
                    },
                }
            }
        };

        let today = current_utc_date();
        let outdated = match (current_version, latest_version.as_deref()) {
            (Some(current), Some(latest)) => match compare_versions(current, latest) {
                Some(std::cmp::Ordering::Less) => true,
                Some(_) => is_version_outdated(current, today),
                None => is_version_outdated(current, today),
            },
            (Some(current), None) => is_version_outdated(current, today),
            (None, _) => false,
        };

        let can_update = match (current_version, latest_version.as_deref()) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(current), Some(latest)) => {
                // Only offer an update when the remote release is strictly newer.
                matches!(
                    compare_versions(latest, current),
                    Some(std::cmp::Ordering::Greater)
                )
            }
        };

        Ok(EngineUpdateStatus {
            current_version: current_version.map(String::from),
            latest_version,
            channel: self.channel.as_str().to_string(),
            outdated,
            can_update,
        })
    }

    /// Downloads, verifies, and atomically installs the latest engine release.
    ///
    /// On any verification failure the previously installed binary is left
    /// untouched. Returns the version reported by the newly installed binary.
    pub async fn update(&self) -> Result<EngineUpdateOutcome, CoreError> {
        let tag = match self.fresh_cached_tag(&self.load_cache().await) {
            Some(tag) => tag,
            None => {
                let tag = self.fetch_latest_tag().await?;
                self.save_cache(&tag).await;
                tag
            }
        };

        let asset = asset_name_for_current_platform()?;
        let base = format!(
            "https://github.com/{}/releases/download/{}",
            self.channel.repo(),
            tag
        );

        let sums_url = format!("{base}/SHA2-256SUMS");
        let sums_body = self.fetch_text(&sums_url).await?;
        let sums = parse_sha256sums(&sums_body);
        let expected_hash = sums.get(asset).ok_or_else(|| {
            CoreError::ProviderError(format!(
                "Release '{tag}' does not publish a checksum for asset '{asset}'"
            ))
        })?;

        let asset_url = format!("{base}/{asset}");
        let installed_version = self
            .download_verify_and_install(&asset_url, expected_hash)
            .await?;

        Ok(EngineUpdateOutcome {
            installed_version,
            updated: true,
        })
    }

    /// Fetches and validates the latest release tag via the GitHub API.
    async fn fetch_latest_tag(&self) -> Result<String, CoreError> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/latest",
            self.channel.repo()
        );
        let body = self.fetch_text(&url).await?;
        let parsed: serde_json::Value = serde_json::from_str(&body).map_err(|err| {
            CoreError::ProviderError(format!("Malformed GitHub release metadata: {err}"))
        })?;
        let tag = parsed["tag_name"]
            .as_str()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or_else(|| {
                CoreError::ProviderError(
                    "GitHub release metadata did not contain a tag_name".to_string(),
                )
            })?;
        Ok(tag.to_string())
    }

    /// Performs a validated GET and returns the response body as text.
    async fn fetch_text(&self, url: &str) -> Result<String, CoreError> {
        let parsed = validate_engine_url(url)?;
        let response = self.client.get(parsed).send().await.map_err(|err| {
            CoreError::ProviderError(format!("Engine metadata request failed: {err}"))
        })?;

        if response.status() == reqwest::StatusCode::FORBIDDEN
            || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
        {
            return Err(CoreError::ProviderError(
                "GitHub rate limit reached; please retry later".to_string(),
            ));
        }
        if !response.status().is_success() {
            return Err(CoreError::ProviderError(format!(
                "Engine metadata request returned HTTP {}",
                response.status()
            )));
        }

        response.text().await.map_err(|err| {
            CoreError::ProviderError(format!("Failed to read metadata response: {err}"))
        })
    }

    /// Streams the asset to `<bin_dir>/<name>.new`, hashes it on the fly,
    /// verifies the digest, then installs it atomically under the canonical name.
    async fn download_verify_and_install(
        &self,
        asset_url: &str,
        expected_hash: &str,
    ) -> Result<String, CoreError> {
        tokio::fs::create_dir_all(&self.app_bin_dir)
            .await
            .map_err(|err| CoreError::StorageError(format!("Failed to create bin dir: {err}")))?;

        let final_name = engine_binary_file_name();
        let final_path = self.app_bin_dir.join(final_name);
        // The temporary file must live on the same filesystem as the target so
        // that the final `rename` is atomic.
        let temp_path = self.app_bin_dir.join(format!("{final_name}.new"));

        // Shared with the Node.js installer: validated HTTPS download, streaming
        // SHA-256 verification, fsync, abort without touching the target on mismatch.
        download_verified_to_file(&self.client, asset_url, expected_hash, &temp_path).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(err) =
                std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o755))
            {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err(CoreError::StorageError(format!(
                    "Failed to mark engine binary as executable: {err}"
                )));
            }
        }

        // Keep the outgoing binary as `yt-dlp.previous` so the UI can roll back.
        let previous_path = self.app_bin_dir.join("yt-dlp.previous");
        if final_path.is_file() {
            let _ = tokio::fs::remove_file(&previous_path).await;
            if let Err(err) = tokio::fs::rename(&final_path, &previous_path).await {
                // The rollback copy is best-effort: never block a valid update on it.
                eprintln!("[PolySaver Updater] Could not back up the previous engine: {err}");
            }
        }

        replace_binary(&temp_path, &final_path).await?;

        // Confirm the freshly installed binary is runnable before reporting success.
        let version = query_installed_version(&final_path).await?;
        Ok(version)
    }

    /// Returns a fresh cached tag, if any, without hitting the network.
    fn fresh_cached_tag(&self, cached: &Option<UpdateCheckCache>) -> Option<String> {
        let entry = cached.as_ref()?;
        let now = now_unix();
        if now.saturating_sub(entry.last_check_unix) < UPDATE_CHECK_TTL.as_secs() {
            Some(entry.tag_name.clone())
        } else {
            None
        }
    }

    async fn load_cache(&self) -> Option<UpdateCheckCache> {
        let path = self.cache_file.as_ref()?;
        let raw = tokio::fs::read_to_string(path).await.ok()?;
        serde_json::from_str(&raw).ok()
    }

    async fn save_cache(&self, tag: &str) {
        let Some(path) = self.cache_file.as_ref() else {
            return;
        };
        let entry = UpdateCheckCache {
            last_check_unix: now_unix(),
            tag_name: tag.to_string(),
        };
        if let Ok(serialized) = serde_json::to_string_pretty(&entry) {
            let _ = tokio::fs::write(path, serialized).await;
        }
    }
}

/// Hosts allowed for engine and runtime downloads and metadata lookups.
const ALLOWED_HOSTS: &[&str] = &[
    "github.com",
    "api.github.com",
    "release-assets.githubusercontent.com",
    "objects.githubusercontent.com",
    // Node.js official distribution (used by the JavaScript runtime installer).
    "nodejs.org",
];

/// Builds the hardened HTTP client shared by the engine updater and the
/// Node.js installer: HTTPS-only allowlist, redirect re-validation, pinned
/// public-address DNS resolver, explicit User-Agent, bounded timeouts.
pub(crate) fn build_download_client() -> Result<reqwest::Client, CoreError> {
    reqwest::Client::builder()
        .user_agent("PolySaver/2.0 (+https://github.com/AinsiParlaitZarathoustra/PolySaver)")
        .redirect(redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("too many redirects during download")
            } else if is_allowed_engine_url(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("redirect to a host outside the download allowlist")
            }
        }))
        .dns_resolver(PinnedPublicResolver)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|err| CoreError::ProviderError(format!("Failed to build HTTP client: {err}")))
}

/// Streams an allowlisted HTTPS URL to `dest`, verifying its SHA-256 as it goes.
///
/// Shared by the engine updater and the Node.js installer:
/// - refuses any host outside [`ALLOWED_HOSTS`] and every redirect hop that leaves it,
/// - rejects private/reserved addresses through the pinned DNS resolver,
/// - hashes incrementally (never buffers the archive),
/// - fsyncs before returning, and removes the file on any failure.
///
/// On error the destination never contains a partially verified file.
pub(crate) async fn download_verified_to_file(
    client: &reqwest::Client,
    url: &str,
    expected_hash: &str,
    dest: &Path,
) -> Result<(), CoreError> {
    let parsed = validate_engine_url(url)?;
    let mut response = client
        .get(parsed)
        .send()
        .await
        .map_err(|err| CoreError::ProviderError(format!("Download failed: {err}")))?;

    if response.status() == reqwest::StatusCode::FORBIDDEN
        || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return Err(CoreError::ProviderError(
            "Download rate limit reached; please retry later".to_string(),
        ));
    }
    if !response.status().is_success() {
        return Err(CoreError::ProviderError(format!(
            "Download returned HTTP {}",
            response.status()
        )));
    }

    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|err| CoreError::StorageError(format!("Failed to create temp file: {err}")))?;

    let mut hasher = Sha256::new();
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(err) => {
                let _ = tokio::fs::remove_file(dest).await;
                return Err(CoreError::ProviderError(format!(
                    "Download interrupted: {err}"
                )));
            }
        };
        hasher.update(&chunk);
        if let Err(err) = file.write_all(&chunk).await {
            let _ = tokio::fs::remove_file(dest).await;
            return Err(CoreError::StorageError(format!(
                "Failed to write downloaded file: {err}"
            )));
        }
    }

    if let Err(err) = file.sync_all().await {
        let _ = tokio::fs::remove_file(dest).await;
        return Err(CoreError::StorageError(format!(
            "Failed to flush downloaded file: {err}"
        )));
    }
    drop(file);

    let actual_hash = hex_encode(&hasher.finalize());
    if !actual_hash.eq_ignore_ascii_case(expected_hash.trim()) {
        let _ = tokio::fs::remove_file(dest).await;
        return Err(CoreError::ProviderError(
            "Downloaded file failed SHA-256 verification; installation aborted".to_string(),
        ));
    }

    Ok(())
}

/// Validates scheme and parsed host against the strict engine allowlist.
fn validate_engine_url(url: &str) -> Result<Url, CoreError> {
    let parsed = Url::parse(url)
        .map_err(|err| CoreError::ProviderError(format!("Invalid engine URL: {err}")))?;
    if parsed.scheme() != "https" {
        return Err(CoreError::ProviderError(
            "Engine downloads require HTTPS".to_string(),
        ));
    }
    if !is_allowed_engine_url(&parsed) {
        return Err(CoreError::ProviderError(format!(
            "Host '{}' is not in the engine download allowlist",
            parsed.host_str().unwrap_or("<none>")
        )));
    }
    Ok(parsed)
}

/// Host allowlist check operating on the *parsed* URL host.
fn is_allowed_engine_url(url: &Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    match url.host_str() {
        Some(host) => {
            let host = host.to_ascii_lowercase();
            ALLOWED_HOSTS.contains(&host.as_str())
        }
        None => false,
    }
}

/// Custom resolver validating every resolved address before it is used.
///
/// This pins the connection to addresses that passed validation on this very
/// lookup, which mitigates DNS rebinding between validation and connection.
#[derive(Debug, Clone, Copy)]
struct PinnedPublicResolver;

impl reqwest::dns::Resolve for PinnedPublicResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let host = name.as_str().to_string();
            let resolved = tokio::net::lookup_host((host.as_str(), 0)).await.map_err(
                |err| -> Box<dyn std::error::Error + Send + Sync> {
                    format!("DNS resolution failed for '{host}': {err}").into()
                },
            )?;

            let addrs: Vec<SocketAddr> = resolved.filter(|addr| is_public_ip(addr.ip())).collect();
            if addrs.is_empty() {
                return Err(format!(
                    "DNS resolution for '{host}' produced no public address; request refused"
                )
                .into());
            }

            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Parses GitHub's GNU-style `SHA2-256SUMS` content into `<sha256, asset>` pairs.
pub(crate) fn parse_sha256sums(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else { continue };
        let Some(name) = parts.next() else { continue };
        // GNU format may prefix the name with '*'; strip it when present.
        let name = name.trim_start_matches('*');
        if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
            map.insert(name.to_string(), hash.to_ascii_lowercase());
        }
    }
    map
}

/// Release asset name for the current target platform.
fn asset_name_for_current_platform() -> Result<&'static str, CoreError> {
    if cfg!(target_os = "macos") {
        Ok("yt-dlp_macos")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        Ok("yt-dlp_linux_aarch64")
    } else if cfg!(target_os = "linux") {
        Ok("yt-dlp_linux")
    } else if cfg!(target_os = "windows") && cfg!(target_arch = "aarch64") {
        Ok("yt-dlp_arm64.exe")
    } else if cfg!(target_os = "windows") {
        Ok("yt-dlp.exe")
    } else {
        Err(CoreError::ProviderError(
            "No yt-dlp release asset is known for this platform".to_string(),
        ))
    }
}

/// Canonical installed file name for the current platform.
fn engine_binary_file_name() -> &'static str {
    if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

/// Replaces `final_path` with `temp_path`.
#[cfg(unix)]
pub(crate) async fn replace_binary(temp_path: &Path, final_path: &Path) -> Result<(), CoreError> {
    tokio::fs::rename(temp_path, final_path)
        .await
        .map_err(|err| CoreError::StorageError(format!("Failed to install engine binary: {err}")))
}

/// Windows cannot overwrite or delete a running `.exe`, but it can rename it.
/// Sequence: move the old binary aside, place the new one, then try to delete
/// the leftover (deferred by the OS until the process exits if still in use).
#[cfg(windows)]
pub(crate) async fn replace_binary(temp_path: &Path, final_path: &Path) -> Result<(), CoreError> {
    if final_path.exists() {
        let backup = final_path.with_extension("old");
        let _ = tokio::fs::remove_file(&backup).await;
        tokio::fs::rename(final_path, &backup)
            .await
            .map_err(|err| {
                CoreError::StorageError(format!("Failed to move existing engine aside: {err}"))
            })?;
        match tokio::fs::rename(temp_path, final_path).await {
            Ok(()) => {
                // Best effort: fails while the old binary is still running and is
                // then removed by the OS on next opportunity.
                let _ = tokio::fs::remove_file(&backup).await;
                Ok(())
            }
            Err(err) => {
                let _ = tokio::fs::rename(&backup, final_path).await;
                let _ = tokio::fs::remove_file(temp_path).await;
                Err(CoreError::StorageError(format!(
                    "Failed to install engine binary: {err}"
                )))
            }
        }
    } else {
        tokio::fs::rename(temp_path, final_path)
            .await
            .map_err(|err| {
                CoreError::StorageError(format!("Failed to install engine binary: {err}"))
            })
    }
}

/// Runs `--version` on the installed binary to confirm it is valid.
async fn query_installed_version(binary: &Path) -> Result<String, CoreError> {
    let output = tokio::process::Command::new(binary)
        .arg("--version")
        .output()
        .await
        .map_err(|err| {
            CoreError::ProviderError(format!("Failed to run installed engine: {err}"))
        })?;
    if !output.status.success() {
        return Err(CoreError::ProviderError(
            "The installed engine failed to report its version".to_string(),
        ));
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        return Err(CoreError::ProviderError(
            "The installed engine returned an empty version".to_string(),
        ));
    }
    Ok(version)
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    #[test]
    fn test_parse_sha256sums_gnu_format() {
        let content = "\
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  yt-dlp_macos
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb *yt-dlp_linux
not-a-hash  ignored
cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc  yt-dlp.exe
";
        let map = parse_sha256sums(content);
        assert_eq!(
            map.get("yt-dlp_macos").map(String::as_str),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(
            map.get("yt-dlp_linux").map(String::as_str),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
        assert_eq!(
            map.get("yt-dlp.exe").map(String::as_str),
            Some("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
        );
        assert!(!map.contains_key("ignored"));
    }

    #[test]
    fn test_asset_selection_and_binary_name_match_platform() {
        let asset = asset_name_for_current_platform().unwrap();
        if cfg!(windows) {
            assert!(asset.ends_with(".exe"));
            assert_eq!(engine_binary_file_name(), "yt-dlp.exe");
        } else if cfg!(target_os = "macos") {
            assert_eq!(asset, "yt-dlp_macos");
            assert_eq!(engine_binary_file_name(), "yt-dlp");
        } else if cfg!(target_arch = "aarch64") {
            assert_eq!(asset, "yt-dlp_linux_aarch64");
            assert_eq!(engine_binary_file_name(), "yt-dlp");
        } else {
            assert_eq!(asset, "yt-dlp_linux");
            assert_eq!(engine_binary_file_name(), "yt-dlp");
        }
    }

    #[test]
    fn test_host_allowlist_rejects_lookalikes_and_raw_strings() {
        // The parsed host must match exactly; embedded credentials do not fool it.
        let sneaky = Url::parse("https://github.com@127.0.0.1/yt-dlp").unwrap();
        assert!(!is_allowed_engine_url(&sneaky));

        assert!(!is_allowed_engine_url(
            &Url::parse("https://evil.example.com/yt-dlp").unwrap()
        ));
        assert!(!is_allowed_engine_url(
            &Url::parse("https://github.com.evil.example/yt-dlp").unwrap()
        ));
        assert!(!is_allowed_engine_url(
            &Url::parse("http://github.com/yt-dlp").unwrap()
        ));
        assert!(is_allowed_engine_url(
            &Url::parse("https://github.com/yt-dlp/yt-dlp/releases").unwrap()
        ));
        assert!(is_allowed_engine_url(
            &Url::parse("https://release-assets.githubusercontent.com/asset").unwrap()
        ));
        assert!(is_allowed_engine_url(
            &Url::parse("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest").unwrap()
        ));
    }

    #[test]
    fn test_validate_engine_url_enforces_https() {
        assert!(validate_engine_url("http://github.com/x").is_err());
        assert!(validate_engine_url("https://evil.example.com/x").is_err());
        assert!(validate_engine_url("https://github.com/x").is_ok());
    }

    #[test]
    fn test_reserved_and_local_addresses_rejected() {
        let rejected = [
            "127.0.0.1",
            "0.1.2.3",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.10.10",
            "100.64.0.1",
            "198.18.0.1",
            "192.0.2.10",
            "198.51.100.7",
            "203.0.113.9",
            "240.0.0.1",
            "224.0.0.1",
            "::1",
            "::",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "2001:db8::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
        ];
        for raw in rejected {
            let ip: IpAddr = raw.parse().unwrap();
            assert!(!is_public_ip(ip), "expected {raw} to be rejected");
        }

        let accepted = ["140.82.121.4", "2606:50c0:8000::153", "::ffff:140.82.121.4"];
        for raw in accepted {
            let ip: IpAddr = raw.parse().unwrap();
            assert!(is_public_ip(ip), "expected {raw} to be accepted");
        }
    }

    #[test]
    fn test_update_check_cache_ttl() {
        let updater = YtDlpUpdater::new(PathBuf::from("/tmp/polysaver-test-bin"), None).unwrap();
        let now = now_unix();

        let fresh = Some(UpdateCheckCache {
            last_check_unix: now,
            tag_name: "2026.08.19".to_string(),
        });
        assert_eq!(
            updater.fresh_cached_tag(&fresh).as_deref(),
            Some("2026.08.19")
        );

        let stale = Some(UpdateCheckCache {
            last_check_unix: now.saturating_sub(25 * 60 * 60),
            tag_name: "2026.08.19".to_string(),
        });
        assert_eq!(updater.fresh_cached_tag(&stale), None);
        assert_eq!(updater.fresh_cached_tag(&None), None);
    }

    #[tokio::test]
    async fn test_atomic_install_places_binary_in_app_bin_dir() {
        let dir = std::env::temp_dir().join(format!("polysaver_updater_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();

        let temp_path = dir.join("yt-dlp.new");
        let final_path = dir.join("yt-dlp");
        tokio::fs::write(&temp_path, b"fake-binary").await.unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        replace_binary(&temp_path, &final_path).await.unwrap();
        assert!(!temp_path.exists());
        assert_eq!(tokio::fs::read(&final_path).await.unwrap(), b"fake-binary");

        // Replacing an existing binary must succeed on every platform.
        tokio::fs::write(&temp_path, b"fake-binary-v2")
            .await
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        replace_binary(&temp_path, &final_path).await.unwrap();
        assert_eq!(
            tokio::fs::read(&final_path).await.unwrap(),
            b"fake-binary-v2"
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    /// Full-chain smoke test against real GitHub infrastructure:
    /// metadata lookup, download, streaming SHA-256 verification, atomic install,
    /// and `--version` validation of the installed binary.
    ///
    /// Run explicitly (downloads ~40 MB):
    /// `POLYSAVER_RUN_NETWORK_E2E=1 cargo test -p polysaver-ytdlp -- --ignored`
    #[tokio::test]
    #[ignore = "Requires live GitHub access; run with POLYSAVER_RUN_NETWORK_E2E=1"]
    async fn test_real_engine_update_end_to_end() {
        if std::env::var("POLYSAVER_RUN_NETWORK_E2E").as_deref() != Ok("1") {
            return;
        }

        let dir = std::env::temp_dir().join(format!("polysaver_nightly_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();

        let updater = YtDlpUpdater::new(dir.clone(), Some(dir.join("cache.json"))).unwrap();

        // 1. Metadata-only check never downloads the binary.
        let status = updater.check_update(None).await.unwrap();
        assert!(status.latest_version.is_some());
        assert_eq!(status.channel, "stable");

        // 2. Full update: download + verify + atomic install.
        let outcome = updater.update().await.unwrap();
        assert!(outcome.updated);
        assert!(!outcome.installed_version.is_empty());

        let installed = dir.join(engine_binary_file_name());
        assert!(installed.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&installed).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "installed binary must be executable");
        }
        assert!(!dir
            .join(format!("{}.new", engine_binary_file_name()))
            .exists());

        // 3. The repository resolver must now prefer this updated binary.
        let resolver = polysaver_binres::BinaryResolver::new(dir.clone(), None);
        let resolved = resolver.resolve_ytdlp().await.unwrap();
        assert_eq!(resolved.path, installed);
        assert_eq!(resolved.version, outcome.installed_version);

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
