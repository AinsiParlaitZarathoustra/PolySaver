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
    pub speed_bytes_per_second: Option<u64>,
}

/// Request to download raw streams for a media URL.
#[derive(Debug, Clone)]
pub struct DownloadStreamRequest {
    pub url: MediaUrl,
    pub preset: DownloadPreset,
    pub temp_dir: PathBuf,
    pub parallel_segments: usize,
    pub cancellation_token: Option<tokio_util::sync::CancellationToken>,
}

/// Artifacts produced by the raw stream download.
#[derive(Debug, Clone)]
pub struct DownloadedStreams {
    pub raw_artifacts: Vec<PathBuf>,
    pub video_path: Option<PathBuf>,
    pub audio_path: Option<PathBuf>,
    pub title: String,
    pub duration_seconds: Option<u64>,
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
