// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use polysaver_core::domain::{AppSettingsDto, DownloadJob, DownloadPresetDto, DownloadStatus};
use polysaver_core::error::DownloadErrorDetails;
use polysaver_ffmpeg::FfmpegAvailability;
use polysaver_ytdlp::YtDlpAvailability;
use serde::{Deserialize, Serialize};

/// Request DTO for starting a download job.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartDownloadRequestDto {
    pub url: String,
    pub preset: Option<DownloadPresetDto>,
    pub output_directory: Option<String>,
}

/// Request DTO for downloading a selection of playlist entries.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPlaylistDownloadRequestDto {
    pub url: String,
    pub preset: Option<DownloadPresetDto>,
    pub output_directory: Option<String>,
    /// URLs of the entries the user checked. Each one is re-validated server-side.
    pub selected_urls: Vec<String>,
}

/// Request DTO for analyzing a media URL.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeUrlRequest {
    pub url: String,
}

/// Request DTO for updating application settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSettingsRequest {
    pub settings: AppSettingsDto,
}

/// Full serializable DTO for a download job.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadJobDto {
    pub id: String,
    pub url: String,
    pub preset: DownloadPresetDto,
    pub title: Option<String>,
    pub status: DownloadStatus,
    pub progress_percent: Option<u8>,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub speed_bytes_per_second: Option<u64>,
    pub destination_path: Option<String>,
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_details: Option<DownloadErrorDetails>,
    /// Number of automatic retries already performed for this job.
    pub retry_count: u32,
}

impl From<&DownloadJob> for DownloadJobDto {
    fn from(job: &DownloadJob) -> Self {
        Self {
            id: job.id().to_string(),
            url: job.url().as_str().to_string(),
            preset: DownloadPresetDto::from(&job.preset()),
            title: job.title().map(String::from),
            status: job.status(),
            progress_percent: job.progress_percent(),
            downloaded_bytes: job.downloaded_bytes(),
            total_bytes: job.total_bytes(),
            speed_bytes_per_second: job.speed_bytes_per_second(),
            destination_path: job.destination_path().map(String::from),
            error_message: job.error_message().map(String::from),
            error_details: job.error_details().cloned(),
            retry_count: u32::from(job.retry_count()),
        }
    }
}

/// Diagnostic health status response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub core_status: String,
    pub ytdlp: YtDlpAvailability,
    pub ffmpeg: FfmpegAvailability,
    /// JavaScript runtime availability (Deno preferred, then Node).
    pub js_runtime: JsRuntimeAvailability,
}

/// Diagnostic availability status for the JavaScript runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsRuntimeAvailability {
    pub is_ready: bool,
    /// `deno` or `node` when a runtime was detected.
    pub kind: Option<String>,
    pub version: Option<String>,
    pub binary_path: Option<String>,
    pub status_message: String,
}

/// Detailed JavaScript runtime status returned to the setup dialog.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsRuntimeStatusDto {
    /// `deno` or `node`; absent when nothing was detected.
    pub kind: Option<String>,
    pub version: Option<String>,
    pub path: Option<String>,
    pub is_ready: bool,
    /// A runtime exists but is below the version required by yt-dlp.
    pub version_too_old: bool,
}

/// Current state of the yt-dlp engine relative to the latest known release.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineUpdateStatusDto {
    pub current_version: Option<String>,
    pub latest_version: Option<String>,
    pub channel: String,
    pub outdated: bool,
    pub can_update: bool,
    /// True when a `yt-dlp.previous` binary is available for rollback.
    pub can_rollback: bool,
}

/// Outcome of a successfully performed engine update.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineUpdateResultDto {
    pub installed_version: String,
    /// False when the installed version already matched the remote release.
    pub updated: bool,
    /// Remote version, when it could be determined without downloading.
    pub latest_version: Option<String>,
}

/// Format option DTO in URL analysis response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatOptionDto {
    pub format_id: String,
    pub height: Option<u32>,
    pub has_video: bool,
    pub has_audio: bool,
    pub extension: String,
    pub filesize_approx_bytes: Option<u64>,
    pub tbr: Option<f64>,
    pub note: Option<String>,
}

impl From<&polysaver_core::domain::FormatOption> for FormatOptionDto {
    fn from(f: &polysaver_core::domain::FormatOption) -> Self {
        Self {
            format_id: f.format_id.clone(),
            height: f.height,
            has_video: f.has_video,
            has_audio: f.has_audio,
            extension: f.extension.clone(),
            filesize_approx_bytes: f.filesize_approx_bytes,
            tbr: f.tbr,
            note: f.note.clone(),
        }
    }
}

/// One playlist entry in a URL analysis response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistEntryDto {
    pub index: u32,
    pub url: String,
    pub title: String,
    pub duration_seconds: Option<u64>,
    pub thumbnail_url: Option<String>,
    pub available: bool,
}

impl From<&polysaver_core::domain::PlaylistEntry> for PlaylistEntryDto {
    fn from(entry: &polysaver_core::domain::PlaylistEntry) -> Self {
        Self {
            index: entry.index,
            url: entry.url.as_str().to_string(),
            title: entry.title.clone(),
            duration_seconds: entry.duration_seconds,
            thumbnail_url: entry.thumbnail_url.clone(),
            available: entry.available,
        }
    }
}

/// Response DTO for the native "is this a playlist?" detection.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistDetectionDto {
    pub is_playlist: bool,
}

impl From<polysaver_core::ports::PlaylistDetection> for PlaylistDetectionDto {
    fn from(detection: polysaver_core::ports::PlaylistDetection) -> Self {
        Self {
            is_playlist: detection.is_playlist,
        }
    }
}

/// Dedicated explicit IPC response DTO for URL analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResultDto {
    pub url: String,
    pub title: String,
    pub duration_seconds: Option<u64>,
    pub thumbnail_url: Option<String>,
    pub uploader: Option<String>,
    pub formats: Vec<FormatOptionDto>,
    pub available_video_qualities: Vec<polysaver_core::domain::VideoQuality>,
    /// `single` or `playlist`.
    pub kind: polysaver_core::domain::MediaKind,
    /// Always empty for a single item (retro-compatible shape).
    pub entries: Vec<PlaylistEntryDto>,
    /// Total number of videos reported by the provider, when known.
    pub playlist_total: Option<u64>,
    /// Maximum number of entries the backend enumerates (`--playlist-end`).
    /// Let the UI explain that only the first N videos are listed.
    pub entries_limit: u32,
}

impl From<&polysaver_core::domain::ProbeResult> for ProbeResultDto {
    fn from(probe: &polysaver_core::domain::ProbeResult) -> Self {
        Self {
            url: probe.url.as_str().to_string(),
            title: probe.title.clone(),
            duration_seconds: probe.duration_seconds,
            thumbnail_url: probe.thumbnail_url.clone(),
            uploader: probe.uploader.clone(),
            formats: probe.formats.iter().map(FormatOptionDto::from).collect(),
            available_video_qualities: probe.available_video_qualities.clone(),
            kind: probe.kind,
            entries: probe.entries.iter().map(PlaylistEntryDto::from).collect(),
            playlist_total: probe.playlist_total,
            entries_limit: polysaver_core::domain::PLAYLIST_ENTRIES_LIMIT as u32,
        }
    }
}
