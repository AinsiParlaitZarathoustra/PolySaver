// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::format::DownloadPreset;
use crate::domain::media_url::MediaUrl;
use crate::error::CoreError;
use std::path::PathBuf;
use std::sync::Arc;

/// Progress information reported during stream download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamProgress {
    pub percent: Option<u8>,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    /// Estimated total when the provider cannot report an exact size
    /// (e.g. HLS/DASH). Non-monotonic; never overrides `total_bytes`.
    pub total_bytes_estimate: Option<u64>,
    pub speed_bytes_per_second: Option<u64>,
}

/// Request to download raw streams for a media URL.
#[derive(Debug, Clone)]
pub struct DownloadStreamRequest {
    pub url: MediaUrl,
    pub preset: DownloadPreset,
    pub temp_dir: PathBuf,
    pub cancellation_token: Option<tokio_util::sync::CancellationToken>,
}

/// JavaScript runtime passed to yt-dlp as `--js-runtimes <name>:<path>`.
///
/// `path` must be absolute: a GUI app on macOS does not inherit the shell `PATH`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsRuntimeSpec {
    /// Runtime name as understood by yt-dlp (`deno` or `node`).
    pub name: &'static str,
    /// Absolute path to the runtime executable.
    pub path: String,
}

/// Outcome of the `--cookies-from-browser` import, as reported by the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookiesDiagnostic {
    /// Cookies were read successfully from the configured browser.
    Imported,
    /// The browser's encryption key could not be obtained (e.g. refused
    /// macOS Keychain prompt); yt-dlp silently continues without cookies.
    DecryptFailed,
    /// The browser database was not readable (Safari requires Full Disk Access).
    PermissionDenied,
}

/// Artifacts produced by the raw stream download.
#[derive(Debug, Clone)]
pub struct DownloadedStreams {
    pub raw_artifacts: Vec<PathBuf>,
    pub video_path: Option<PathBuf>,
    pub audio_path: Option<PathBuf>,
    pub title: String,
    pub duration_seconds: Option<u64>,
    /// True when the engine reported itself as outdated during an otherwise
    /// successful download (non-fatal signal for the UI).
    pub engine_outdated: bool,
    /// Cookie-import outcome, when a browser was configured.
    pub cookies_diagnostic: Option<CookiesDiagnostic>,
}

use async_trait::async_trait;

/// Trait implemented by adapters downloading raw media streams (e.g. yt-dlp).
#[async_trait]
pub trait MediaDownloader: Send + Sync {
    /// Downloads raw streams to a temporary directory with progress callbacks.
    async fn download_stream(
        &self,
        request: DownloadStreamRequest,
        progress_callback: Arc<dyn Fn(StreamProgress) + Send + Sync>,
    ) -> Result<DownloadedStreams, CoreError>;
}
