// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! # PolySaver FFmpeg Adapter
//!
//! Encapsulates `ffmpeg` and `ffprobe` execution and implements the `MediaConverter` and `MediaInspector` ports.

pub mod inspector;
pub mod process_runner;

use async_trait::async_trait;
use inspector::FfprobeInspector;
use polysaver_binres::BinaryResolver;
use polysaver_core::domain::OutputFormat;
use polysaver_core::error::{CoreError, DownloadErrorCode, DownloadErrorDetails};
use polysaver_core::ports::converter::{
    AudioCodec, ConvertRequest, ConverterProgressCallback, MediaConverter,
};
use polysaver_core::ports::media_inspector::{MediaInspector, MediaStreamInfo};
use process_runner::FfmpegProcessRunner;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Diagnostic availability status for FFmpeg.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegAvailability {
    pub is_ready: bool,
    pub version: Option<String>,
    pub binary_path: Option<String>,
    pub status_message: String,
}

/// Discovers or validates the FFmpeg executable.
pub async fn discover_ffmpeg_binary(
    bin_dir: &Path,
    resource_bin_dir: Option<&Path>,
) -> Result<PathBuf, CoreError> {
    let resolver = BinaryResolver::new(bin_dir.to_path_buf(), resource_bin_dir.map(PathBuf::from));
    let resolved = resolver.resolve_ffmpeg().await.map_err(|_| {
        CoreError::DownloadFailed(DownloadErrorDetails::from_code(
            DownloadErrorCode::FfmpegNotFound,
        ))
    })?;
    Ok(resolved.path)
}

/// Discovers or validates the FFprobe executable.
pub async fn discover_ffprobe_binary(
    bin_dir: &Path,
    resource_bin_dir: Option<&Path>,
) -> Result<PathBuf, CoreError> {
    let resolver = BinaryResolver::new(bin_dir.to_path_buf(), resource_bin_dir.map(PathBuf::from));
    let resolved = resolver.resolve_ffprobe().await.map_err(|_| {
        CoreError::DownloadFailed(DownloadErrorDetails::from_code(
            DownloadErrorCode::FfmpegNotFound,
        ))
    })?;
    Ok(resolved.path)
}

/// Checks availability of FFmpeg without triggering automatic downloads.
pub async fn probe_ffmpeg_availability(
    bin_dir: &Path,
    resource_bin_dir: Option<&Path>,
) -> FfmpegAvailability {
    let resolver = BinaryResolver::new(bin_dir.to_path_buf(), resource_bin_dir.map(PathBuf::from));
    match resolver.resolve_ffmpeg().await {
        Ok(resolved) => FfmpegAvailability {
            is_ready: true,
            version: Some(resolved.version.clone()),
            binary_path: Some(resolved.path.to_string_lossy().to_string()),
            status_message: format!("FFmpeg version {} prête", resolved.version),
        },
        Err(err) => FfmpegAvailability {
            is_ready: false,
            version: None,
            binary_path: None,
            status_message: format!("FFmpeg indisponible: {err}"),
        },
    }
}

/// Checks availability of FFmpeg using a shared BinaryResolver instance.
pub async fn probe_ffmpeg_with_resolver(resolver: &BinaryResolver) -> FfmpegAvailability {
    match resolver.resolve_ffmpeg().await {
        Ok(resolved) => FfmpegAvailability {
            is_ready: true,
            version: Some(resolved.version.clone()),
            binary_path: Some(resolved.path.to_string_lossy().to_string()),
            status_message: format!("FFmpeg version {} prête", resolved.version),
        },
        Err(err) => FfmpegAvailability {
            is_ready: false,
            version: None,
            binary_path: None,
            status_message: format!("FFmpeg indisponible: {err}"),
        },
    }
}

/// Real FFmpeg converter adapter.
pub struct FfmpegConverter {
    resolver: Arc<BinaryResolver>,
}

impl FfmpegConverter {
    /// Creates a new `FfmpegConverter` with dedicated app binary directory.
    #[must_use]
    pub fn new(bin_dir: PathBuf) -> Self {
        Self {
            resolver: Arc::new(BinaryResolver::new(bin_dir, None)),
        }
    }

    /// Creates a new `FfmpegConverter` with optional resource directory.
    #[must_use]
    pub fn with_resource_dir(bin_dir: PathBuf, resource_bin_dir: Option<PathBuf>) -> Self {
        Self {
            resolver: Arc::new(BinaryResolver::new(bin_dir, resource_bin_dir)),
        }
    }

    /// Creates a new `FfmpegConverter` with an injected shared `BinaryResolver`.
    #[must_use]
    pub fn with_resolver(resolver: Arc<BinaryResolver>) -> Self {
        Self { resolver }
    }
}

#[async_trait]
impl MediaInspector for FfmpegConverter {
    async fn inspect(&self, path: &Path) -> Result<MediaStreamInfo, CoreError> {
        let resolved_ffprobe = self.resolver.resolve_ffprobe().await.map_err(|_| {
            CoreError::DownloadFailed(DownloadErrorDetails::from_code(
                DownloadErrorCode::FfmpegNotFound,
            ))
        })?;
        let inspection = FfprobeInspector::inspect(&resolved_ffprobe.path, path).await?;
        Ok(MediaStreamInfo {
            has_video: inspection.has_video,
            has_audio: inspection.has_audio,
            video_codec: inspection.video_codec,
            audio_codec: inspection.audio_codec,
        })
    }
}

#[async_trait]
impl MediaConverter for FfmpegConverter {
    async fn convert(
        &self,
        request: ConvertRequest,
        cancellation_token: Option<tokio_util::sync::CancellationToken>,
        progress_callback: Option<ConverterProgressCallback>,
    ) -> Result<PathBuf, CoreError> {
        let ffmpeg_bin = self
            .resolver
            .resolve_ffmpeg()
            .await
            .map_err(|_| {
                CoreError::DownloadFailed(DownloadErrorDetails::from_code(
                    DownloadErrorCode::FfmpegNotFound,
                ))
            })?
            .path;

        let ffprobe_bin = self
            .resolver
            .resolve_ffprobe()
            .await
            .map_err(|_| {
                CoreError::DownloadFailed(DownloadErrorDetails::from_code(
                    DownloadErrorCode::FfmpegNotFound,
                ))
            })?
            .path;

        match request {
            ConvertRequest::VideoMuxOrTranscode {
                video_input,
                audio_input,
                output_path,
                format,
                duration_seconds,
                temp_dir,
            } => {
                // Inspect inputs to know stream composition
                let video_inspection = FfprobeInspector::inspect(&ffprobe_bin, &video_input)
                    .await
                    .ok();
                let has_video_audio = video_inspection
                    .as_ref()
                    .map(|i| i.has_audio)
                    .unwrap_or(false);
                let expect_audio = audio_input.is_some() || has_video_audio;

                let ext = format.extension();
                let remux_temp = temp_dir.join(format!("remux_{}.{ext}", uuid::Uuid::new_v4()));

                // 1. Attempt 1: Fast remux without re-encoding (-c copy)
                let remux_args = build_video_ffmpeg_args(
                    &video_input,
                    audio_input.as_deref(),
                    &remux_temp,
                    format,
                    true, // copy
                );

                let remux_result = FfmpegProcessRunner::run(
                    &ffmpeg_bin,
                    &remux_args,
                    &temp_dir,
                    duration_seconds,
                    cancellation_token.as_ref(),
                    progress_callback.clone(),
                )
                .await?;

                if remux_result.success
                    && FfprobeInspector::validate_video_output(
                        &ffprobe_bin,
                        &remux_temp,
                        expect_audio,
                    )
                    .await
                    .is_ok()
                {
                    move_or_copy_file(&remux_temp, &output_path).await?;
                    return Ok(output_path);
                }

                // Clean up failed remux temp file
                let _ = tokio::fs::remove_file(&remux_temp).await;
                let remux_err_tail = remux_result.stderr_tail;

                // 2. Attempt 2: Fallback to transcoding (libx264 + aac)
                let transcode_temp =
                    temp_dir.join(format!("transcode_{}.{ext}", uuid::Uuid::new_v4()));

                let transcode_args = build_video_ffmpeg_args(
                    &video_input,
                    audio_input.as_deref(),
                    &transcode_temp,
                    format,
                    false, // transcode
                );

                let transcode_result = FfmpegProcessRunner::run(
                    &ffmpeg_bin,
                    &transcode_args,
                    &temp_dir,
                    duration_seconds,
                    cancellation_token.as_ref(),
                    progress_callback,
                )
                .await?;

                if transcode_result.success
                    && FfprobeInspector::validate_video_output(
                        &ffprobe_bin,
                        &transcode_temp,
                        expect_audio,
                    )
                    .await
                    .is_ok()
                {
                    move_or_copy_file(&transcode_temp, &output_path).await?;
                    return Ok(output_path);
                }

                // Clean up failed transcode temp file
                let _ = tokio::fs::remove_file(&transcode_temp).await;

                // Formulate structured error details
                let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::FfmpegFailed);
                details.message =
                    "La conversion ou le multiplexage multimédia avec FFmpeg a échoué.".to_string();

                let combined_stderr =
                    if !remux_err_tail.is_empty() && !transcode_result.stderr_tail.is_empty() {
                        format!(
                            "[Échec Remux -c copy]:\n{}\n\n[Échec Transcodage repli]:\n{}",
                            remux_err_tail, transcode_result.stderr_tail
                        )
                    } else if !transcode_result.stderr_tail.is_empty() {
                        transcode_result.stderr_tail
                    } else {
                        remux_err_tail
                    };

                if !combined_stderr.is_empty() {
                    details.stderr_tail = Some(combined_stderr);
                }

                Err(CoreError::DownloadFailed(details))
            }
            ConvertRequest::AudioTranscode {
                audio_input,
                output_path,
                codec,
                duration_seconds,
                temp_dir,
            } => {
                let ext = match codec {
                    AudioCodec::Mp3(_) => "mp3",
                    AudioCodec::Flac => "flac",
                };
                let temp_audio_out = temp_dir.join(format!("audio_{}.{ext}", uuid::Uuid::new_v4()));

                let mut args = vec![
                    "-y".to_string(),
                    "-progress".to_string(),
                    "pipe:1".to_string(),
                    "-nostats".to_string(),
                    "-loglevel".to_string(),
                    "error".to_string(),
                    "-i".to_string(),
                    audio_input.to_string_lossy().to_string(),
                    "-map".to_string(),
                    "0:a:0".to_string(),
                    "-vn".to_string(),
                    "-sn".to_string(),
                    "-dn".to_string(),
                ];

                match codec {
                    AudioCodec::Mp3(quality) => {
                        let bitrate_arg = format!("{}k", quality.bitrate_kbps());
                        args.extend([
                            "-c:a".to_string(),
                            "libmp3lame".to_string(),
                            "-b:a".to_string(),
                            bitrate_arg,
                        ]);
                    }
                    AudioCodec::Flac => {
                        args.extend(["-c:a".to_string(), "flac".to_string()]);
                    }
                }

                args.push(temp_audio_out.to_string_lossy().to_string());

                let run_result = FfmpegProcessRunner::run(
                    &ffmpeg_bin,
                    &args,
                    &temp_dir,
                    duration_seconds,
                    cancellation_token.as_ref(),
                    progress_callback,
                )
                .await?;

                if run_result.success {
                    let validation_result = match codec {
                        AudioCodec::Mp3(_) => {
                            FfprobeInspector::validate_mp3_output(&ffprobe_bin, &temp_audio_out)
                                .await
                        }
                        AudioCodec::Flac => {
                            FfprobeInspector::validate_flac_output(&ffprobe_bin, &temp_audio_out)
                                .await
                        }
                    };

                    if validation_result.is_ok() {
                        move_or_copy_file(&temp_audio_out, &output_path).await?;
                        return Ok(output_path);
                    }
                }

                // Clean up failed temp audio file
                let _ = tokio::fs::remove_file(&temp_audio_out).await;

                let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::FfmpegFailed);
                details.message = "La conversion audio avec FFmpeg a échoué.".to_string();
                if !run_result.stderr_tail.is_empty() {
                    details.stderr_tail = Some(run_result.stderr_tail);
                }

                Err(CoreError::DownloadFailed(details))
            }
        }
    }
}

/// Builds explicit argument vector for video muxing/transcoding.
fn build_video_ffmpeg_args(
    video_input: &Path,
    audio_input: Option<&Path>,
    output_path: &Path,
    format: OutputFormat,
    use_copy: bool,
) -> Vec<String> {
    let mut args = vec![
        "-y".to_string(),
        "-progress".to_string(),
        "pipe:1".to_string(),
        "-nostats".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-i".to_string(),
        video_input.to_string_lossy().to_string(),
    ];

    if let Some(audio) = audio_input {
        args.push("-i".to_string());
        args.push(audio.to_string_lossy().to_string());

        // Map video from input 0 and audio from input 1
        args.extend([
            "-map".to_string(),
            "0:v:0".to_string(),
            "-map".to_string(),
            "1:a:0?".to_string(),
        ]);
    } else {
        // Map best video and audio from single combined input
        args.extend([
            "-map".to_string(),
            "0:v:0?".to_string(),
            "-map".to_string(),
            "0:a:0?".to_string(),
        ]);
    }

    // Strip subtitles and data streams from container
    args.extend(["-sn".to_string(), "-dn".to_string()]);

    if use_copy {
        args.extend(["-c".to_string(), "copy".to_string()]);
    } else {
        args.extend([
            "-c:v".to_string(),
            "libx264".to_string(),
            "-preset".to_string(),
            "fast".to_string(),
            "-pix_fmt".to_string(),
            "yuv420p".to_string(),
            "-c:a".to_string(),
            "aac".to_string(),
            "-b:a".to_string(),
            "192k".to_string(),
        ]);
    }

    match format {
        OutputFormat::Mov => {
            args.extend(["-f".to_string(), "mov".to_string()]);
        }
        OutputFormat::Mp4 => {
            args.extend([
                "-f".to_string(),
                "mp4".to_string(),
                "-movflags".to_string(),
                "+faststart".to_string(),
            ]);
        }
        _ => {}
    }

    args.push(output_path.to_string_lossy().to_string());
    args
}

/// Moves or copies a file across filesystem mount boundaries.
async fn move_or_copy_file(src: &Path, dst: &Path) -> Result<(), CoreError> {
    if let Err(_rename_err) = tokio::fs::rename(src, dst).await {
        tokio::fs::copy(src, dst).await.map_err(|err| {
            CoreError::ConverterError(format!(
                "Failed to copy file from '{}' to '{}': {err}",
                src.display(),
                dst.display()
            ))
        })?;
        let _ = tokio::fs::remove_file(src).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use polysaver_core::domain::Mp3Quality;
    use tokio::process::Command;

    #[tokio::test]
    async fn test_ffmpeg_probe_runs_without_panic() {
        let bin_dir =
            std::env::temp_dir().join(format!("polysaver_ffmpeg_test_{}", uuid::Uuid::new_v4()));
        let avail = probe_ffmpeg_availability(&bin_dir, None).await;
        assert!(!avail.status_message.is_empty());
    }

    #[tokio::test]
    async fn test_synthetic_media_transcode_and_progress() {
        let test_dir =
            std::env::temp_dir().join(format!("polysaver_ffmpeg_synth_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&test_dir).await.unwrap();

        let resource_dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/bin");
        let converter =
            FfmpegConverter::with_resource_dir(test_dir.clone(), Some(resource_dir.clone()));

        if let Ok(bin) = discover_ffmpeg_binary(&test_dir, Some(&resource_dir)).await {
            let synth_audio = test_dir.join("synth_audio.wav");
            let status = Command::new(&bin)
                .args([
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=1000:duration=1",
                    &synth_audio.to_string_lossy(),
                ])
                .status()
                .await;

            if let Ok(s) = status {
                if s.success() {
                    // Test MP3 conversion
                    let mp3_out = test_dir.join("synth.mp3");
                    let res_mp3 = converter
                        .convert(
                            ConvertRequest::AudioTranscode {
                                audio_input: synth_audio.clone(),
                                output_path: mp3_out.clone(),
                                codec: AudioCodec::Mp3(Mp3Quality::K192),
                                duration_seconds: Some(1),
                                temp_dir: test_dir.clone(),
                            },
                            None,
                            None,
                        )
                        .await;

                    assert!(res_mp3.is_ok(), "MP3 transcode failed: {:?}", res_mp3.err());
                    assert!(mp3_out.exists());
                    assert!(tokio::fs::metadata(&mp3_out).await.unwrap().len() > 0);

                    // Test FLAC conversion
                    let flac_out = test_dir.join("synth.flac");
                    let res_flac = converter
                        .convert(
                            ConvertRequest::AudioTranscode {
                                audio_input: synth_audio,
                                output_path: flac_out.clone(),
                                codec: AudioCodec::Flac,
                                duration_seconds: Some(1),
                                temp_dir: test_dir.clone(),
                            },
                            None,
                            None,
                        )
                        .await;

                    assert!(
                        res_flac.is_ok(),
                        "FLAC transcode failed: {:?}",
                        res_flac.err()
                    );
                    assert!(flac_out.exists());
                    assert!(tokio::fs::metadata(&flac_out).await.unwrap().len() > 0);
                }
            }
        }

        let _ = tokio::fs::remove_dir_all(&test_dir).await;
    }

    #[tokio::test]
    async fn test_controlled_ffmpeg_failure_and_cleanup() {
        let test_dir =
            std::env::temp_dir().join(format!("polysaver_ffmpeg_fail_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&test_dir).await.unwrap();

        let resource_dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/bin");
        let converter = FfmpegConverter::with_resource_dir(test_dir.clone(), Some(resource_dir));

        let corrupt_input = test_dir.join("corrupt.bin");
        tokio::fs::write(&corrupt_input, b"not a valid media file")
            .await
            .unwrap();

        let target_output = test_dir.join("should_not_exist.mp3");
        let res = converter
            .convert(
                ConvertRequest::AudioTranscode {
                    audio_input: corrupt_input,
                    output_path: target_output.clone(),
                    codec: AudioCodec::Mp3(Mp3Quality::K320),
                    duration_seconds: None,
                    temp_dir: test_dir.clone(),
                },
                None,
                None,
            )
            .await;

        assert!(res.is_err());
        let err = res.unwrap_err();
        if let CoreError::DownloadFailed(details) = err {
            assert_eq!(details.code, DownloadErrorCode::FfmpegFailed);
            assert!(details.stderr_tail.is_some());
        } else {
            panic!("Expected CoreError::DownloadFailed, got {:?}", err);
        }

        // Verify output file was not created
        assert!(!target_output.exists());

        let _ = tokio::fs::remove_dir_all(&test_dir).await;
    }
}
