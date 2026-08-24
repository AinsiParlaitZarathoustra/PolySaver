// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! Asynchronous non-blocking FFmpeg process runner with progress streaming and diagnostic stderr capture.

use polysaver_core::error::{CoreError, DownloadErrorCode, DownloadErrorDetails};
use polysaver_core::ports::ConverterProgress;
use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Maximum number of stderr lines retained in memory for diagnostic reporting.
const MAX_STDERR_LINES: usize = 25;

/// Execution result from the FFmpeg process.
#[derive(Debug, Clone)]
pub struct FfmpegRunResult {
    pub exit_code: Option<i32>,
    pub stderr_tail: String,
    pub success: bool,
}

pub struct FfmpegProcessRunner;

impl FfmpegProcessRunner {
    /// Executes FFmpeg with machine-readable progress streaming on stdout, error capture on stderr, and cancellation support.
    pub async fn run(
        ffmpeg_bin: &Path,
        args: &[String],
        working_dir: &Path,
        duration_seconds: Option<u64>,
        cancellation_token: Option<&tokio_util::sync::CancellationToken>,
        progress_callback: Option<Arc<dyn Fn(ConverterProgress) + Send + Sync>>,
    ) -> Result<FfmpegRunResult, CoreError> {
        if let Some(token) = cancellation_token {
            if token.is_cancelled() {
                return Err(CoreError::OperationCancelled);
            }
        }

        let mut cmd = Command::new(ffmpeg_bin);
        cmd.args(args)
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|err| {
            let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::FfmpegFailed);
            details.message = format!(
                "Impossible de démarrer le processus FFmpeg '{}': {err}",
                ffmpeg_bin.display()
            );
            CoreError::DownloadFailed(details)
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            CoreError::ConverterError("Failed to capture FFmpeg stdout".to_string())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            CoreError::ConverterError("Failed to capture FFmpeg stderr".to_string())
        })?;

        // 1. Asynchronous stdout reader task for -progress pipe:1
        let stdout_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            let mut current_time_us: Option<u64> = None;

            while let Ok(Some(line)) = reader.next_line().await {
                let trimmed = line.trim();
                if let Some(val) = trimmed.strip_prefix("out_time_us=") {
                    if let Ok(us) = val.parse::<u64>() {
                        current_time_us = Some(us);
                        if let Some(dur) = duration_seconds {
                            if dur > 0 {
                                let total_us = dur.saturating_mul(1_000_000);
                                let pct =
                                    ((us as f64 / total_us as f64) * 100.0).clamp(0.0, 99.0) as u8;
                                if let Some(ref cb) = progress_callback {
                                    cb(ConverterProgress { percent: Some(pct) });
                                }
                            }
                        } else if let Some(ref cb) = progress_callback {
                            cb(ConverterProgress { percent: None });
                        }
                    }
                } else if let Some(val) = trimmed.strip_prefix("out_time=") {
                    if current_time_us.is_none() {
                        if let Some(us) = parse_time_string_to_us(val) {
                            if let Some(dur) = duration_seconds {
                                if dur > 0 {
                                    let total_us = dur.saturating_mul(1_000_000);
                                    let pct = ((us as f64 / total_us as f64) * 100.0)
                                        .clamp(0.0, 99.0)
                                        as u8;
                                    if let Some(ref cb) = progress_callback {
                                        cb(ConverterProgress { percent: Some(pct) });
                                    }
                                }
                            }
                        }
                    }
                } else if trimmed == "progress=end" {
                    if let Some(ref cb) = progress_callback {
                        cb(ConverterProgress { percent: Some(100) });
                    }
                }
            }
        });

        // 2. Asynchronous stderr reader task with bounded memory ring buffer
        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            let mut buffer = VecDeque::with_capacity(MAX_STDERR_LINES);

            while let Ok(Some(line)) = reader.next_line().await {
                if buffer.len() >= MAX_STDERR_LINES {
                    buffer.pop_front();
                }
                buffer.push_back(line);
            }

            buffer.into_iter().collect::<Vec<_>>().join("\n")
        });

        let status = if let Some(token) = cancellation_token {
            tokio::select! {
                res = child.wait() => {
                    res.map_err(|err| {
                        let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::FfmpegFailed);
                        details.message = format!("FFmpeg process wait failure: {err}");
                        CoreError::DownloadFailed(details)
                    })?
                }
                _ = token.cancelled() => {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    let _ = stdout_task.await;
                    let _ = stderr_task.await;
                    return Err(CoreError::OperationCancelled);
                }
            }
        } else {
            child.wait().await.map_err(|err| {
                let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::FfmpegFailed);
                details.message = format!("FFmpeg process wait failure: {err}");
                CoreError::DownloadFailed(details)
            })?
        };

        let _ = stdout_task.await;
        let stderr_tail = stderr_task.await.unwrap_or_default();

        Ok(FfmpegRunResult {
            exit_code: status.code(),
            stderr_tail,
            success: status.success(),
        })
    }
}

/// Parses "HH:MM:SS.microseconds" format into microseconds.
pub fn parse_time_string_to_us(time_str: &str) -> Option<u64> {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 3 {
        return None;
    }

    let hours: u64 = parts[0].parse().ok()?;
    let minutes: u64 = parts[1].parse().ok()?;
    let sec_parts: Vec<&str> = parts[2].split('.').collect();
    let seconds: u64 = sec_parts[0].parse().ok()?;

    let mut micros: u64 = 0;
    if sec_parts.len() > 1 {
        let frac_str = sec_parts[1];
        if let Ok(frac) = frac_str.parse::<u64>() {
            let len = frac_str.len();
            if len <= 6 {
                micros = frac * 10u64.pow((6 - len) as u32);
            } else {
                micros = frac / 10u64.pow((len - 6) as u32);
            }
        }
    }

    Some(
        hours
            .saturating_mul(3600)
            .saturating_add(minutes.saturating_mul(60))
            .saturating_add(seconds)
            .saturating_mul(1_000_000)
            .saturating_add(micros),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_time_string_to_us() {
        assert_eq!(parse_time_string_to_us("00:00:01.000000"), Some(1_000_000));
        assert_eq!(parse_time_string_to_us("00:01:30.500000"), Some(90_500_000));
        assert_eq!(
            parse_time_string_to_us("01:00:00.000000"),
            Some(3_600_000_000)
        );
        assert_eq!(parse_time_string_to_us("invalid"), None);
    }
}
