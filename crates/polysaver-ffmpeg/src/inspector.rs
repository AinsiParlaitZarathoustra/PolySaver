// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! Media inspection and validation using the bundled FFprobe sidecar.

use polysaver_core::error::{CoreError, DownloadErrorCode, DownloadErrorDetails};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::process::Command;

/// Summary metadata extracted by FFprobe.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaInspection {
    pub has_video: bool,
    pub video_codec: Option<String>,
    pub has_audio: bool,
    pub audio_codec: Option<String>,
    pub audio_bitrate: Option<u64>,
}

pub struct FfprobeInspector;

impl FfprobeInspector {
    /// Inspects media streams using the provided FFprobe binary.
    pub async fn inspect(
        ffprobe_bin: &Path,
        media_path: &Path,
    ) -> Result<MediaInspection, CoreError> {
        let output = Command::new(ffprobe_bin)
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=index,codec_type,codec_name,bit_rate",
                "-of",
                "json",
            ])
            .arg(media_path)
            .output()
            .await
            .map_err(|err| {
                CoreError::ConverterError(format!(
                    "Failed to execute ffprobe on '{}': {err}",
                    media_path.display()
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::FfmpegFailed);
            details.message = format!("ffprobe verification failed on '{}'", media_path.display());
            details.stderr_tail = Some(stderr.trim().to_string());
            return Err(CoreError::DownloadFailed(details));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).map_err(|err| {
            CoreError::ConverterError(format!("Failed to parse ffprobe JSON output: {err}"))
        })?;

        let mut inspection = MediaInspection::default();

        if let Some(streams) = parsed["streams"].as_array() {
            for s in streams {
                let codec_type = s["codec_type"].as_str().unwrap_or("");
                let codec_name = s["codec_name"].as_str().map(String::from);

                if codec_type == "video" {
                    inspection.has_video = true;
                    if inspection.video_codec.is_none() {
                        inspection.video_codec = codec_name;
                    }
                } else if codec_type == "audio" {
                    inspection.has_audio = true;
                    if inspection.audio_codec.is_none() {
                        inspection.audio_codec = codec_name;
                    }
                    if let Some(br_str) = s["bit_rate"].as_str() {
                        if let Ok(br) = br_str.parse::<u64>() {
                            inspection.audio_bitrate = Some(br);
                        }
                    } else if let Some(br) = s["bit_rate"].as_u64() {
                        inspection.audio_bitrate = Some(br);
                    }
                }
            }
        }

        Ok(inspection)
    }

    /// Validates that generated video media contains valid video and expected audio streams.
    pub async fn validate_video_output(
        ffprobe_bin: &Path,
        media_path: &Path,
        expect_audio: bool,
    ) -> Result<(), CoreError> {
        let inspection = Self::inspect(ffprobe_bin, media_path).await?;

        if !inspection.has_video {
            let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::FfmpegFailed);
            details.message = format!(
                "Generated file '{}' contains no video stream",
                media_path.display()
            );
            return Err(CoreError::DownloadFailed(details));
        }

        if expect_audio && !inspection.has_audio {
            let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::FfmpegFailed);
            details.message = format!(
                "Generated file '{}' is missing the expected audio stream",
                media_path.display()
            );
            return Err(CoreError::DownloadFailed(details));
        }

        Ok(())
    }

    /// Validates that generated MP3 media has an audio stream with the mp3 codec.
    pub async fn validate_mp3_output(
        ffprobe_bin: &Path,
        media_path: &Path,
    ) -> Result<(), CoreError> {
        let inspection = Self::inspect(ffprobe_bin, media_path).await?;

        if !inspection.has_audio {
            let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::FfmpegFailed);
            details.message = format!(
                "Generated MP3 file '{}' contains no audio stream",
                media_path.display()
            );
            return Err(CoreError::DownloadFailed(details));
        }

        if let Some(ref codec) = inspection.audio_codec {
            if codec != "mp3" {
                let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::FfmpegFailed);
                details.message = format!(
                    "Generated file '{}' codec is '{codec}', expected 'mp3'",
                    media_path.display()
                );
                return Err(CoreError::DownloadFailed(details));
            }
        }

        Ok(())
    }

    /// Validates that generated FLAC media has an audio stream with the flac codec.
    pub async fn validate_flac_output(
        ffprobe_bin: &Path,
        media_path: &Path,
    ) -> Result<(), CoreError> {
        let inspection = Self::inspect(ffprobe_bin, media_path).await?;

        if !inspection.has_audio {
            let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::FfmpegFailed);
            details.message = format!(
                "Generated FLAC file '{}' contains no audio stream",
                media_path.display()
            );
            return Err(CoreError::DownloadFailed(details));
        }

        if let Some(ref codec) = inspection.audio_codec {
            if codec != "flac" {
                let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::FfmpegFailed);
                details.message = format!(
                    "Generated file '{}' codec is '{codec}', expected 'flac'",
                    media_path.display()
                );
                return Err(CoreError::DownloadFailed(details));
            }
        }

        Ok(())
    }
}
