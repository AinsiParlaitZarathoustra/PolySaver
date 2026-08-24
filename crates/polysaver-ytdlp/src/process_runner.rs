// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! # yt-dlp Process Runner
//!
//! Hardened, deadlock-free process executor for `yt-dlp` with concurrent stdout/stderr streams,
//! multi-stream progress aggregation, and cancellation support.

use crate::aggregator::MultiStreamProgressAggregator;
use crate::error_classifier::classify_ytdlp_error;
use polysaver_core::error::{CoreError, DownloadErrorCode, DownloadErrorDetails};
use polysaver_core::ports::media_downloader::StreamProgress;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::LazyLock;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Lazily compiled regex for fallback human progress parsing.
static FALLBACK_PROGRESS_RE: LazyLock<Result<Regex, regex::Error>> = LazyLock::new(|| {
    Regex::new(
        r"\[download\]\s+(\d+(?:\.\d+)?)%\s+of\s+(?:~?\s*)(\d+(?:\.\d+)?)(KiB|MiB|GiB|B)?(?:\s+at\s+(\d+(?:\.\d+)?)(KiB|MiB|GiB|B)/s)?",
    )
});

/// Parsed metadata from `before_dl:[POLYSAVER_META]` print output.
#[derive(Debug, Clone, Default)]
pub struct EarlyMediaMetadata {
    pub title: Option<String>,
    pub duration_seconds: Option<u64>,
    pub format_ids: Vec<String>,
    pub stream_sizes: Vec<Option<u64>>,
}

/// Result of a successful yt-dlp process run.
#[derive(Debug, Clone)]
pub struct ProcessRunResult {
    pub output_files: Vec<PathBuf>,
    pub stdout_lines: Vec<String>,
    pub early_meta: Option<EarlyMediaMetadata>,
}

/// Hardened executor for yt-dlp.
pub struct YtDlpProcessRunner;

impl YtDlpProcessRunner {
    /// Executes yt-dlp with given arguments, concurrent stream readers, cancellation token, and progress callback.
    pub async fn run(
        ytdlp_bin: &Path,
        ffmpeg_bin: Option<&Path>,
        args: &[String],
        temp_dir: &Path,
        cancellation_token: Option<&CancellationToken>,
        progress_callback: Option<Arc<dyn Fn(StreamProgress) + Send + Sync>>,
        version: Option<String>,
    ) -> Result<ProcessRunResult, CoreError> {
        if let Some(token) = cancellation_token {
            if token.is_cancelled() {
                return Err(CoreError::OperationCancelled);
            }
        }

        let mut cmd = Command::new(ytdlp_bin);
        cmd.arg("--ignore-config");
        cmd.arg("--no-playlist");
        cmd.arg("--no-colors");

        if let Some(ff) = ffmpeg_bin {
            cmd.arg("--ffmpeg-location");
            if let Some(parent) = ff.parent() {
                cmd.arg(parent);
            } else {
                cmd.arg(ff);
            }
        }

        cmd.args(args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|err| {
            let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::YtdlpStartFailed);
            details.message = format!("Impossible de lancer le moteur de téléchargement: {err}");
            CoreError::DownloadFailed(details)
        })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CoreError::ProviderError("Failed to open stdout pipe".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| CoreError::ProviderError("Failed to open stderr pipe".to_string()))?;

        let stderr_lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured_outputs = Arc::new(Mutex::new(Vec::<PathBuf>::new()));
        let captured_early_meta = Arc::new(Mutex::new(None::<EarlyMediaMetadata>));
        let progress_aggregator = Arc::new(Mutex::new(MultiStreamProgressAggregator::new(2)));

        let stderr_collector = Arc::clone(&stderr_lines);
        let stderr_task = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    let mut lock = stderr_collector.lock().await;
                    if lock.len() >= 60 {
                        lock.remove(0);
                    }
                    lock.push(trimmed);
                }
            }
        });

        let outputs_collector = Arc::clone(&captured_outputs);
        let early_meta_collector = Arc::clone(&captured_early_meta);
        let aggregator_ref = Arc::clone(&progress_aggregator);
        let cb = progress_callback.clone();
        let stdout_lines_buf = Arc::new(Mutex::new(Vec::<String>::new()));
        let stdout_collector = Arc::clone(&stdout_lines_buf);

        let stdout_task = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim().to_string();
                if trimmed.is_empty() {
                    continue;
                }

                // Check for [POLYSAVER_META] line (from --print before_dl:...)
                if trimmed.contains("[POLYSAVER_META]") {
                    if let Some(meta) = parse_polysaver_meta_line(&trimmed) {
                        let mut agg_lock = aggregator_ref.lock().await;
                        for (idx, stream_id) in meta.format_ids.iter().enumerate() {
                            let size = meta.stream_sizes.get(idx).copied().flatten();
                            agg_lock.register_stream_size(stream_id, size);
                        }
                        let mut meta_lock = early_meta_collector.lock().await;
                        *meta_lock = Some(meta);
                    }
                }

                // Check for [POLYSAVER_OUTPUT] line
                if let Some(rest) = trimmed.strip_prefix("[POLYSAVER_OUTPUT]") {
                    let file_path = PathBuf::from(rest.trim());
                    let mut lock = outputs_collector.lock().await;
                    lock.push(file_path);
                }

                // Check for [POLYSAVER_PROGRESS] line
                if trimmed.contains("[POLYSAVER_PROGRESS]") {
                    if let Some((stream_id, parsed)) =
                        parse_polysaver_progress_line_with_stream(&trimmed)
                    {
                        let mut agg_lock = aggregator_ref.lock().await;
                        let agg_progress = agg_lock.feed(stream_id.as_deref(), &parsed);
                        if let Some(ref callback) = cb {
                            callback(agg_progress);
                        }
                    }
                } else if trimmed.starts_with("[download]") {
                    // Fallback to standard human progress line parser
                    if let Some(parsed) = parse_fallback_progress_line(&trimmed) {
                        let mut agg_lock = aggregator_ref.lock().await;
                        let agg_progress = agg_lock.feed(None, &parsed);
                        if let Some(ref callback) = cb {
                            callback(agg_progress);
                        }
                    }
                }

                let mut lock = stdout_collector.lock().await;
                if lock.len() < 200 {
                    lock.push(trimmed);
                }
            }
        });

        // Wait for child process with cancellation awareness
        let status = if let Some(token) = cancellation_token {
            tokio::select! {
                res = child.wait() => {
                    res.map_err(|err| {
                        CoreError::ProviderError(format!("Failed waiting for yt-dlp process: {err}"))
                    })?
                }
                _ = token.cancelled() => {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    let _ = tokio::join!(stdout_task, stderr_task);
                    return Err(CoreError::OperationCancelled);
                }
            }
        } else {
            child.wait().await.map_err(|err| {
                CoreError::ProviderError(format!("Failed waiting for yt-dlp process: {err}"))
            })?
        };

        let _ = tokio::join!(stdout_task, stderr_task);

        let collected_stderr = {
            let lock = stderr_lines.lock().await;
            lock.clone()
        };

        if !status.success() {
            let details = classify_ytdlp_error(status.code(), &collected_stderr, version);
            return Err(CoreError::DownloadFailed(details));
        }

        let mut output_files = {
            let lock = captured_outputs.lock().await;
            lock.clone()
        };

        // Filter out intermediate files deleted post-merge
        output_files.retain(|p| p.exists() && p.is_file());

        // Fallback: if no output explicitly captured, inspect temp_dir
        if output_files.is_empty() {
            if let Ok(mut read_dir) = tokio::fs::read_dir(temp_dir).await {
                while let Ok(Some(entry)) = read_dir.next_entry().await {
                    let path = entry.path();
                    if path.is_file() {
                        if let Ok(meta) = entry.metadata().await {
                            if meta.len() > 0 {
                                output_files.push(path);
                            }
                        }
                    }
                }
            }
        }

        let stdout_lines = {
            let lock = stdout_lines_buf.lock().await;
            lock.clone()
        };

        let early_meta = {
            let lock = captured_early_meta.lock().await;
            lock.clone()
        };

        Ok(ProcessRunResult {
            output_files,
            stdout_lines,
            early_meta,
        })
    }
}

/// Parses early media metadata printed before download.
/// Format: `[POLYSAVER_META] title:My Video duration:120 formats:137+140 filesize:10000000,2000000`
pub fn parse_polysaver_meta_line(line: &str) -> Option<EarlyMediaMetadata> {
    let mut meta = EarlyMediaMetadata::default();
    let payload = line.split("[POLYSAVER_META]").nth(1)?.trim();

    for token in payload.split('\t') {
        let trimmed = token.trim();
        if let Some(val) = trimmed.strip_prefix("title:") {
            let clean = val.trim();
            if !clean.is_empty() && clean != "NA" && clean != "None" {
                meta.title = Some(clean.to_string());
            }
        } else if let Some(val) = trimmed.strip_prefix("duration:") {
            let clean = val.trim();
            if let Ok(d) = clean.parse::<f64>() {
                if d > 0.0 {
                    meta.duration_seconds = Some(d.round() as u64);
                }
            }
        } else if let Some(val) = trimmed.strip_prefix("formats:") {
            let clean = val.trim();
            for f in clean.split('+') {
                let f_clean = f.trim();
                if !f_clean.is_empty() && f_clean != "NA" && f_clean != "None" {
                    meta.format_ids.push(f_clean.to_string());
                }
            }
        } else if let Some(val) = trimmed.strip_prefix("filesize:") {
            let clean = val.trim();
            for s in clean.split(',') {
                let s_clean = s.trim();
                let parsed_size = if s_clean != "NA" && s_clean != "None" {
                    s_clean.parse::<f64>().ok().map(|bytes| bytes as u64)
                } else {
                    None
                };
                meta.stream_sizes.push(parsed_size);
            }
        }
    }

    Some(meta)
}

/// Parses a structured PolySaver progress line with stream identifier.
/// Format: `[POLYSAVER_PROGRESS] percent:XX.X% downloaded:12345 total:12345 speed:1234567.8 stream:137`
pub fn parse_polysaver_progress_line_with_stream(
    line: &str,
) -> Option<(Option<String>, StreamProgress)> {
    let mut percent = None;
    let mut downloaded_bytes = None;
    let mut total_bytes = None;
    let mut speed_bytes = None;
    let mut stream_id = None;

    for token in line.split_whitespace() {
        if let Some(val) = token.strip_prefix("percent:") {
            let clean = val.trim_matches('%').trim();
            if let Ok(p) = clean.parse::<f32>() {
                percent = Some(p.round().clamp(0.0, 100.0) as u8);
            }
        } else if let Some(val) = token.strip_prefix("downloaded:") {
            let clean = val.trim();
            if clean != "NA" && clean != "None" {
                if let Ok(d) = clean.parse::<f64>() {
                    if d > 0.0 {
                        downloaded_bytes = Some(d as u64);
                    }
                }
            }
        } else if let Some(val) = token.strip_prefix("total:") {
            let clean = val.trim();
            if clean != "NA" && clean != "None" {
                if let Ok(t) = clean.parse::<f64>() {
                    if t > 0.0 {
                        total_bytes = Some(t as u64);
                    }
                }
            }
        } else if let Some(val) = token.strip_prefix("speed:") {
            let clean = val.trim().trim_end_matches("B/s").trim();
            if clean != "NA" && clean != "None" {
                if let Ok(s) = clean.parse::<f64>() {
                    if s > 0.0 {
                        speed_bytes = Some(s as u64);
                    }
                }
            }
        } else if let Some(val) = token.strip_prefix("stream:") {
            let clean = val.trim();
            if !clean.is_empty() && clean != "NA" && clean != "None" {
                stream_id = Some(clean.to_string());
            }
        }
    }

    if percent.is_some()
        || downloaded_bytes.is_some()
        || total_bytes.is_some()
        || speed_bytes.is_some()
    {
        Some((
            stream_id,
            StreamProgress {
                percent,
                downloaded_bytes,
                total_bytes,
                speed_bytes_per_second: speed_bytes,
            },
        ))
    } else {
        None
    }
}

/// Backwards-compatible parser wrapper for single progress line.
pub fn parse_polysaver_progress_line(line: &str) -> Option<StreamProgress> {
    parse_polysaver_progress_line_with_stream(line).map(|(_, p)| p)
}

/// Fallback parser for standard human-readable yt-dlp progress line.
pub fn parse_fallback_progress_line(line: &str) -> Option<StreamProgress> {
    let re = FALLBACK_PROGRESS_RE.as_ref().ok()?;
    let caps = re.captures(line)?;

    let percent: Option<u8> = caps
        .get(1)
        .and_then(|m| m.as_str().parse::<f32>().ok())
        .map(|f| f.round().clamp(0.0, 100.0) as u8);

    let total_bytes: Option<u64> = caps
        .get(2)
        .and_then(|m| m.as_str().parse::<f64>().ok())
        .map(|val| {
            let unit = caps.get(3).map_or("MiB", |m| m.as_str());
            match unit {
                "KiB" => (val * 1024.0) as u64,
                "GiB" => (val * 1024.0 * 1024.0 * 1024.0) as u64,
                "B" => val as u64,
                _ => (val * 1024.0 * 1024.0) as u64,
            }
        });

    let speed_bytes_per_second: Option<u64> = caps
        .get(4)
        .and_then(|m| m.as_str().parse::<f64>().ok())
        .map(|val| {
            let unit = caps.get(5).map_or("MiB", |m| m.as_str());
            match unit {
                "KiB" => (val * 1024.0) as u64,
                "GiB" => (val * 1024.0 * 1024.0 * 1024.0) as u64,
                "B" => val as u64,
                _ => (val * 1024.0 * 1024.0) as u64,
            }
        });

    Some(StreamProgress {
        percent,
        downloaded_bytes: None,
        total_bytes,
        speed_bytes_per_second,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_polysaver_progress_template_full() {
        let line =
            "[POLYSAVER_PROGRESS] percent:45.6% downloaded:23897120 total:52428800 speed:2500000.0 stream:137";
        let (stream, parsed) = parse_polysaver_progress_line_with_stream(line).unwrap();
        assert_eq!(stream, Some("137".to_string()));
        assert_eq!(parsed.percent, Some(46));
        assert_eq!(parsed.downloaded_bytes, Some(23897120));
        assert_eq!(parsed.total_bytes, Some(52428800));
        assert_eq!(parsed.speed_bytes_per_second, Some(2500000));
    }

    #[test]
    fn test_parse_polysaver_meta_line() {
        let line = "[POLYSAVER_META] title:Big Buck Bunny\tduration:596\tformats:137+140\tfilesize:50000000,5000000";
        let meta = parse_polysaver_meta_line(line).unwrap();
        assert_eq!(meta.title, Some("Big Buck Bunny".to_string()));
        assert_eq!(meta.duration_seconds, Some(596));
        assert_eq!(meta.format_ids, vec!["137".to_string(), "140".to_string()]);
        assert_eq!(meta.stream_sizes, vec![Some(50000000), Some(5000000)]);
    }

    #[test]
    fn test_parse_fallback_human_progress() {
        let line = "[download]  50.0% of   10.00MiB at    2.50MiB/s ETA 00:02";
        let parsed = parse_fallback_progress_line(line).unwrap();
        assert_eq!(parsed.percent, Some(50));
        assert_eq!(parsed.total_bytes, Some(10 * 1024 * 1024));
        assert_eq!(
            parsed.speed_bytes_per_second,
            Some((2.5 * 1024.0 * 1024.0) as u64)
        );
    }
}
