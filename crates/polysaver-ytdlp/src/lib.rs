// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! # PolySaver yt-dlp Adapter
//!
//! Encapsulates `yt-dlp` execution and implements the `MediaProvider` and `MediaDownloader` ports.

pub mod aggregator;
pub mod error_classifier;
pub mod process_runner;

use async_trait::async_trait;
use polysaver_binres::BinaryResolver;
use polysaver_core::domain::{DownloadPreset, FormatOption, MediaUrl, OutputFormat, ProbeResult};
use polysaver_core::error::{CoreError, DownloadErrorCode, DownloadErrorDetails};
use polysaver_core::ports::media_downloader::{
    DownloadStreamRequest, DownloadedStreams, MediaDownloader, StreamProgress,
};
use polysaver_core::ports::MediaProvider;
use process_runner::{parse_fallback_progress_line, YtDlpProcessRunner};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Diagnostic availability status for yt-dlp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtDlpAvailability {
    pub is_ready: bool,
    pub version: Option<String>,
    pub binary_path: Option<String>,
    pub status_message: String,
}

/// Discovers or validates the yt-dlp executable.
pub async fn discover_ytdlp_binary(bin_dir: &Path) -> Result<PathBuf, CoreError> {
    let resolver = BinaryResolver::new(bin_dir.to_path_buf(), None);
    let resolved = resolver.resolve_ytdlp().await.map_err(|_| {
        CoreError::DownloadFailed(DownloadErrorDetails::from_code(
            DownloadErrorCode::YtdlpNotFound,
        ))
    })?;
    Ok(resolved.path)
}

/// Checks availability of yt-dlp without side effects using the shared or fresh resolver.
pub async fn probe_ytdlp_availability(
    bin_dir: &Path,
    resource_bin_dir: Option<&Path>,
) -> YtDlpAvailability {
    let resolver = BinaryResolver::new(bin_dir.to_path_buf(), resource_bin_dir.map(PathBuf::from));
    match resolver.resolve_ytdlp().await {
        Ok(resolved) => YtDlpAvailability {
            is_ready: true,
            version: Some(resolved.version.clone()),
            binary_path: Some(resolved.path.to_string_lossy().to_string()),
            status_message: format!("yt-dlp version {} prête", resolved.version),
        },
        Err(err) => YtDlpAvailability {
            is_ready: false,
            version: None,
            binary_path: None,
            status_message: format!("yt-dlp indisponible: {err}"),
        },
    }
}

/// Checks availability of yt-dlp using a shared BinaryResolver instance.
pub async fn probe_ytdlp_with_resolver(resolver: &BinaryResolver) -> YtDlpAvailability {
    match resolver.resolve_ytdlp().await {
        Ok(resolved) => YtDlpAvailability {
            is_ready: true,
            version: Some(resolved.version.clone()),
            binary_path: Some(resolved.path.to_string_lossy().to_string()),
            status_message: format!("yt-dlp version {} prête", resolved.version),
        },
        Err(err) => YtDlpAvailability {
            is_ready: false,
            version: None,
            binary_path: None,
            status_message: format!("yt-dlp indisponible: {err}"),
        },
    }
}

/// Real yt-dlp provider implementing `MediaProvider` and `MediaDownloader`.
pub struct YtDlpDownloader {
    resolver: Arc<BinaryResolver>,
}

impl YtDlpDownloader {
    /// Creates a new `YtDlpDownloader` with dedicated app binary directory.
    #[must_use]
    pub fn new(bin_dir: PathBuf) -> Self {
        Self {
            resolver: Arc::new(BinaryResolver::new(bin_dir, None)),
        }
    }

    /// Creates a new `YtDlpDownloader` with optional resource directory.
    #[must_use]
    pub fn with_resource_dir(bin_dir: PathBuf, resource_bin_dir: Option<PathBuf>) -> Self {
        Self {
            resolver: Arc::new(BinaryResolver::new(bin_dir, resource_bin_dir)),
        }
    }

    /// Creates a new `YtDlpDownloader` with an injected shared `BinaryResolver`.
    #[must_use]
    pub fn with_resolver(resolver: Arc<BinaryResolver>) -> Self {
        Self { resolver }
    }
}

#[async_trait]
impl MediaProvider for YtDlpDownloader {
    async fn probe(&self, url: &MediaUrl) -> Result<ProbeResult, CoreError> {
        let resolved = self.resolver.resolve_ytdlp().await.map_err(|_| {
            CoreError::DownloadFailed(DownloadErrorDetails::from_code(
                DownloadErrorCode::YtdlpNotFound,
            ))
        })?;
        let binary = resolved.path;
        let version = Some(resolved.version);
        let ffmpeg_bin = self.resolver.resolve_ffmpeg().await.ok().map(|r| r.path);

        let temp_dir = std::env::temp_dir();
        let args = vec![
            "--dump-single-json".to_string(),
            "--no-warnings".to_string(),
            url.as_str().to_string(),
        ];

        let run_result = YtDlpProcessRunner::run(
            &binary,
            ffmpeg_bin.as_deref(),
            &args,
            &temp_dir,
            None,
            None,
            version,
        )
        .await?;

        let json_str = run_result.stdout_lines.join("\n");
        let parsed: serde_json::Value = serde_json::from_str(&json_str).map_err(|err| {
            CoreError::ProviderError(format!("Failed to parse metadata JSON: {err}"))
        })?;

        let title = parsed["title"].as_str().unwrap_or("Sans titre").to_string();
        let duration_seconds = parsed["duration"].as_u64();
        let uploader = parsed["uploader"]
            .as_str()
            .or_else(|| parsed["channel"].as_str())
            .map(String::from);
        let thumbnail_url = parsed["thumbnail"].as_str().map(String::from);

        let mut formats = Vec::new();
        if let Some(formats_arr) = parsed["formats"].as_array() {
            for f in formats_arr {
                let protocol = f["protocol"].as_str().unwrap_or("");
                let format_note = f["format_note"].as_str().unwrap_or("");
                let vcodec = f["vcodec"].as_str().unwrap_or("none");
                let acodec = f["acodec"].as_str().unwrap_or("none");
                let format_id = f["format_id"].as_str().unwrap_or("");

                // Filter out storyboards, thumbnails, mhtml protocol
                if protocol == "mhtml"
                    || format_note.to_lowercase().contains("storyboard")
                    || format_id.starts_with("sb")
                {
                    continue;
                }

                let has_video = vcodec != "none" && !vcodec.is_empty();
                let has_audio = acodec != "none" && !acodec.is_empty();
                let height = f["height"].as_u64().map(|h| h as u32);
                let ext = f["ext"].as_str().unwrap_or("mp4").to_string();
                let filesize_approx = f["filesize"]
                    .as_u64()
                    .or_else(|| f["filesize_approx"].as_u64());

                formats.push(FormatOption {
                    format_id: format_id.to_string(),
                    height,
                    has_video,
                    has_audio,
                    extension: ext,
                    filesize_approx_bytes: filesize_approx,
                    note: if format_note.is_empty() {
                        None
                    } else {
                        Some(format_note.to_string())
                    },
                });
            }
        }

        Ok(ProbeResult::new(
            url.clone(),
            title,
            duration_seconds,
            thumbnail_url,
            uploader,
            formats,
        ))
    }
}

#[async_trait]
impl MediaDownloader for YtDlpDownloader {
    async fn download_stream(
        &self,
        request: DownloadStreamRequest,
        progress_callback: Arc<dyn Fn(StreamProgress) + Send + Sync>,
    ) -> Result<DownloadedStreams, CoreError> {
        let resolved = self.resolver.resolve_ytdlp().await.map_err(|_| {
            CoreError::DownloadFailed(DownloadErrorDetails::from_code(
                DownloadErrorCode::YtdlpNotFound,
            ))
        })?;
        let binary = resolved.path;
        let version = Some(resolved.version);
        let ffmpeg_bin = self.resolver.resolve_ffmpeg().await.ok().map(|r| r.path);

        // 1. Determine format selector based on preset
        let format_selector = match request.preset {
            DownloadPreset::Video { format, quality } => match (format, quality.target_height()) {
                (OutputFormat::Mp4, Some(h)) => {
                    format!("bestvideo[height={h}][ext=mp4]+bestaudio[ext=m4a]/bestvideo[height={h}]+bestaudio/best[height={h}]")
                }
                (OutputFormat::Mp4, None) => {
                    "bestvideo[ext=mp4]+bestaudio[ext=m4a]/bestvideo+bestaudio/best".to_string()
                }
                (OutputFormat::Mov, Some(h)) => {
                    format!("bestvideo[height={h}]+bestaudio/best[height={h}]")
                }
                (OutputFormat::Mov, None) => "bestvideo+bestaudio/best".to_string(),
                _ => "bestvideo+bestaudio/best".to_string(),
            },
            DownloadPreset::Mp3 { .. } | DownloadPreset::Flac => "bestaudio/best".to_string(),
        };

        // 2. Direct single execution download with before_dl metadata printing and multi-stream progress template
        let out_template = request.temp_dir.join("stream.%(ext)s");
        let mut download_args = vec![
            "-f".to_string(),
            format_selector,
            "--output".to_string(),
            out_template.to_string_lossy().to_string(),
            "--newline".to_string(),
            "--progress".to_string(),
            "--print".to_string(),
            "before_dl:[POLYSAVER_META] title:%(title)s\tduration:%(duration)s\tformats:%(format_id)s\tfilesize:%(filesize,filesize_approx)s".to_string(),
            "--progress-template".to_string(),
            "download:[POLYSAVER_PROGRESS] percent:%(progress._percent_str)s downloaded:%(progress.downloaded_bytes)s total:%(progress.total_bytes,progress.total_bytes_estimate)s speed:%(progress.speed)s stream:%(progress.info_dict.format_id)s file:%(progress.filename)s".to_string(),
            "--print".to_string(),
            "after_move:[POLYSAVER_OUTPUT] %(filepath)s".to_string(),
        ];

        if request.parallel_segments > 1 {
            download_args.push("-N".to_string());
            download_args.push(request.parallel_segments.to_string());
        }

        download_args.push(request.url.as_str().to_string());

        let run_result = YtDlpProcessRunner::run(
            &binary,
            ffmpeg_bin.as_deref(),
            &download_args,
            &request.temp_dir,
            request.cancellation_token.as_ref(),
            Some(progress_callback),
            version,
        )
        .await?;

        let downloaded_files = run_result.output_files;
        if downloaded_files.is_empty() {
            return Err(CoreError::ProviderError(
                "No media files produced by yt-dlp in the temporary directory".to_string(),
            ));
        }

        let title = run_result
            .early_meta
            .as_ref()
            .and_then(|m| m.title.clone())
            .unwrap_or_else(|| "PolySaver_Media".to_string());
        let duration_seconds = run_result
            .early_meta
            .as_ref()
            .and_then(|m| m.duration_seconds);

        // Map downloaded artifacts
        let (video_path, audio_path) = match request.preset {
            DownloadPreset::Video { .. } => {
                if downloaded_files.len() == 1 {
                    (Some(downloaded_files[0].clone()), None)
                } else {
                    let mut v = None;
                    let mut a = None;
                    for file in &downloaded_files {
                        let ext = file.extension().and_then(|s| s.to_str()).unwrap_or("");
                        if matches!(ext, "mp4" | "mkv" | "webm" | "mov") && v.is_none() {
                            v = Some(file.clone());
                        } else if matches!(ext, "m4a" | "webm" | "opus" | "mp3" | "aac")
                            && a.is_none()
                        {
                            a = Some(file.clone());
                        }
                    }
                    (v.or_else(|| downloaded_files.first().cloned()), a)
                }
            }
            DownloadPreset::Mp3 { .. } | DownloadPreset::Flac => {
                (None, Some(downloaded_files[0].clone()))
            }
        };

        Ok(DownloadedStreams {
            raw_artifacts: downloaded_files,
            video_path,
            audio_path,
            title,
            duration_seconds,
        })
    }
}

/// Helper function to parse human progress line for backward compatibility in unit tests.
pub fn parse_ytdlp_progress_line(line: &str) -> StreamProgress {
    parse_fallback_progress_line(line).unwrap_or(StreamProgress {
        percent: None,
        downloaded_bytes: None,
        total_bytes: None,
        speed_bytes_per_second: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_probe_ytdlp_availability_runs_without_panic() {
        let bin_dir =
            std::env::temp_dir().join(format!("polysaver_test_probe_{}", uuid::Uuid::new_v4()));
        let avail = probe_ytdlp_availability(&bin_dir, None).await;
        assert!(!avail.status_message.is_empty());
    }

    #[test]
    fn test_parse_ytdlp_progress_line() {
        let line = "[download]  45.2% of 100.00MiB at 5.50MiB/s ETA 00:10";
        let progress = parse_ytdlp_progress_line(line);
        assert_eq!(progress.percent, Some(45));
        assert_eq!(progress.total_bytes, Some(100 * 1024 * 1024));
        assert_eq!(
            progress.speed_bytes_per_second,
            Some((5.5 * 1024.0 * 1024.0) as u64)
        );
    }
}
