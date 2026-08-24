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
            note: f.note.clone(),
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
        }
    }
}
