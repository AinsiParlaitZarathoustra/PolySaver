// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! # yt-dlp Process Runner
//!
//! Hardened, deadlock-free process executor for `yt-dlp` with concurrent stdout/stderr streams,
//! multi-stream progress aggregation, and cancellation support.

use crate::aggregator::MultiStreamProgressAggregator;
use crate::detect_cookies_diagnostic;
use crate::error_classifier::{classify_ytdlp_error, is_outdated_engine_message};
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
    /// yt-dlp emits an "older than 90 days" WARNING on stderr even when the
    /// download succeeds (exit 0); surfaced here so callers can advise an update.
    pub engine_outdated: bool,
    /// Cookie-import outcome read from stderr (only when cookies were requested).
    pub cookies_diagnostic: Option<polysaver_core::ports::media_downloader::CookiesDiagnostic>,
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

        // Own process group on Unix: yt-dlp spawns ffmpeg for muxing, and killing
        // only the direct child would leave ffmpeg running after a cancellation.
        // The whole group is signalled on cancel (see below).
        #[cfg(unix)]
        cmd.process_group(0);

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
        let progress_aggregator = Arc::new(Mutex::new(MultiStreamProgressAggregator::new()));

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
                    if let Some((stream_id, parsed, status_finished)) =
                        parse_polysaver_progress_line_with_stream(&trimmed)
                    {
                        let mut agg_lock = aggregator_ref.lock().await;
                        let agg_progress = agg_lock.feed_with_status(
                            stream_id.as_deref(),
                            &parsed,
                            status_finished,
                        );
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
                    terminate_process_group(&mut child).await;
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

        // Flush a final 100% now that the process exited successfully. Without
        // this the UI could sit at 99% until the muxing phase reports progress.
        if let Some(ref callback) = progress_callback {
            let aggregator = progress_aggregator.lock().await;
            callback(aggregator.finish());
        }

        // Even on success, yt-dlp warns on stderr when its build is stale.
        let engine_outdated = is_outdated_engine_message(&collected_stderr.join("\n"));
        let cookies_diagnostic = detect_cookies_diagnostic(&collected_stderr);

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
            engine_outdated,
            cookies_diagnostic,
        })
    }
}

/// Terminates the child process and every process it spawned.
///
/// yt-dlp runs ffmpeg as a subprocess during muxing. Since the child is started
/// in its own process group (Unix), signalling the group kills both; on Windows
/// only the direct child is terminated, which is documented behaviour there.
async fn terminate_process_group(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            // Negative pid targets the whole process group.
            let pgid = -(pid as i32);
            if libc_kill(pgid) == 0 {
                let _ = child.wait().await;
                return;
            }
        }
    }

    // Fallback (and Windows path): kill the direct child only.
    let _ = child.start_kill();
    let _ = child.wait().await;
}

/// Sends SIGKILL to a process group. Isolated so the FFI call stays tiny.
#[cfg(unix)]
fn libc_kill(pid: i32) -> i32 {
    // SIGKILL = 9 on every Unix we support. `kill` is async-signal-safe and we
    // only ever pass a pid of a child we spawned in its own process group.
    unsafe { kill(pid, 9) }
}

#[cfg(unix)]
extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

/// Parses early media metadata printed before download.
///
/// Format (tab-separated):
/// `[POLYSAVER_META] title:My Video<TAB>duration:120<TAB>f0:137|50000000<TAB>f1:140|5000000<TAB>fallback_id:137+140<TAB>fallback_size:55000000`
///
/// `requested_formats.N` only exists when the selector merged ≥ 2 formats, so
/// the `fallback_*` tokens are used for single-file selections (audio presets).
pub fn parse_polysaver_meta_line(line: &str) -> Option<EarlyMediaMetadata> {
    let mut meta = EarlyMediaMetadata::default();
    let payload = line.split("[POLYSAVER_META]").nth(1)?.trim();

    let mut stream_entries: Vec<(u8, String, Option<u64>)> = Vec::new();
    let mut fallback_id: Option<String> = None;
    let mut fallback_size: Option<u64> = None;

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
        } else if let Some(val) = trimmed.strip_prefix("fallback_id:") {
            let clean = val.trim();
            if !clean.is_empty() && clean != "NA" && clean != "None" {
                fallback_id = Some(clean.to_string());
            }
        } else if let Some(val) = trimmed.strip_prefix("fallback_size:") {
            fallback_size = parse_optional_size(val);
        } else if let Some(rest) = trimmed.strip_prefix('f') {
            // Indexed entries: `0:137|50000000`
            if let Some((idx_str, payload)) = rest.split_once(':') {
                if let Ok(idx) = idx_str.parse::<u8>() {
                    let mut parts = payload.split('|');
                    let id = parts.next().unwrap_or("").trim();
                    let size = parts.next().and_then(parse_optional_size);
                    if !id.is_empty() && id != "NA" && id != "None" {
                        stream_entries.push((idx, id.to_string(), size));
                    }
                }
            }
        }
    }

    stream_entries.sort_by_key(|(idx, _, _)| *idx);
    for (_, id, size) in stream_entries {
        meta.format_ids.push(id);
        meta.stream_sizes.push(size);
    }

    // Single-file selection: `requested_formats` was unavailable, use the fallback.
    if meta.format_ids.is_empty() {
        if let Some(id) = fallback_id {
            meta.format_ids.push(id);
            meta.stream_sizes.push(fallback_size);
        }
    }

    Some(meta)
}

/// Parses a numeric byte size token, treating `NA`/`None`/empty as absent.
fn parse_optional_size(raw: &str) -> Option<u64> {
    let clean = raw.trim();
    if clean.is_empty() || clean == "NA" || clean == "None" {
        return None;
    }
    clean
        .parse::<f64>()
        .ok()
        .filter(|v| *v > 0.0)
        .map(|v| v as u64)
}

/// Parses a structured PolySaver progress line with stream identifier.
///
/// Format: `[POLYSAVER_PROGRESS] status:downloading downloaded:12345 total:12345 est:23456 speed:1234567.8 stream:137`
/// Only numeric fields are parsed; the percentage is computed by the aggregator.
pub fn parse_polysaver_progress_line_with_stream(
    line: &str,
) -> Option<(Option<String>, StreamProgress, bool)> {
    let mut downloaded_bytes = None;
    let mut total_bytes = None;
    let mut total_bytes_estimate = None;
    let mut speed_bytes = None;
    let mut stream_id = None;
    let mut status_finished = false;

    for token in line.split_whitespace() {
        if let Some(val) = token.strip_prefix("downloaded:") {
            if let Some(v) = parse_optional_size(val) {
                downloaded_bytes = Some(v);
            } else if val.trim() == "0" {
                downloaded_bytes = Some(0);
            }
        } else if let Some(val) = token.strip_prefix("total:") {
            total_bytes = parse_optional_size(val);
        } else if let Some(val) = token.strip_prefix("est:") {
            total_bytes_estimate = parse_optional_size(val);
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
        } else if let Some(val) = token.strip_prefix("status:") {
            if val.trim().eq_ignore_ascii_case("finished") {
                status_finished = true;
            }
        }
    }

    if downloaded_bytes.is_some()
        || total_bytes.is_some()
        || total_bytes_estimate.is_some()
        || speed_bytes.is_some()
        || status_finished
    {
        Some((
            stream_id,
            StreamProgress {
                percent: None,
                downloaded_bytes,
                total_bytes,
                total_bytes_estimate,
                speed_bytes_per_second: speed_bytes,
            },
            status_finished,
        ))
    } else {
        None
    }
}

/// Backwards-compatible parser wrapper for single progress line.
pub fn parse_polysaver_progress_line(line: &str) -> Option<StreamProgress> {
    parse_polysaver_progress_line_with_stream(line).map(|(_, p, _)| p)
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
        total_bytes_estimate: None,
        speed_bytes_per_second,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_polysaver_progress_template_full() {
        let line = "[POLYSAVER_PROGRESS] status:downloading downloaded:23897120 total:52428800 est:NA speed:2500000.0 stream:137";
        let (stream, parsed, finished) = parse_polysaver_progress_line_with_stream(line).unwrap();
        assert_eq!(stream, Some("137".to_string()));
        assert_eq!(parsed.percent, None, "percentage is computed downstream");
        assert_eq!(parsed.downloaded_bytes, Some(23897120));
        assert_eq!(parsed.total_bytes, Some(52428800));
        assert_eq!(parsed.total_bytes_estimate, None);
        assert_eq!(parsed.speed_bytes_per_second, Some(2500000));
        assert!(!finished);
    }

    #[test]
    fn test_parse_progress_line_hls_estimate_and_finished_status() {
        let line = "[POLYSAVER_PROGRESS] status:finished downloaded:999 total:NA est:123456 speed:NA stream:602";
        let (stream, parsed, finished) = parse_polysaver_progress_line_with_stream(line).unwrap();
        assert_eq!(stream, Some("602".to_string()));
        assert_eq!(parsed.downloaded_bytes, Some(999));
        assert_eq!(parsed.total_bytes, None);
        assert_eq!(parsed.total_bytes_estimate, Some(123456));
        assert_eq!(parsed.speed_bytes_per_second, None);
        assert!(finished);
    }

    #[test]
    fn test_parse_progress_line_without_stream_id() {
        // A line followed by a NA stream id must still be usable.
        let line =
            "[POLYSAVER_PROGRESS] status:downloading downloaded:0 total:1000 est:NA speed:NA stream:NA";
        let (stream, parsed, finished) = parse_polysaver_progress_line_with_stream(line).unwrap();
        assert_eq!(stream, None);
        assert_eq!(parsed.downloaded_bytes, Some(0));
        assert_eq!(parsed.total_bytes, Some(1000));
        assert!(!finished);
    }

    #[test]
    fn test_parse_polysaver_meta_line_two_streams() {
        let line = "[POLYSAVER_META] title:Big Buck Bunny\tduration:596\tf0:137|50000000\tf1:140|5000000\tfallback_id:137+140\tfallback_size:55000000";
        let meta = parse_polysaver_meta_line(line).unwrap();
        assert_eq!(meta.title, Some("Big Buck Bunny".to_string()));
        assert_eq!(meta.duration_seconds, Some(596));
        assert_eq!(meta.format_ids, vec!["137".to_string(), "140".to_string()]);
        assert_eq!(meta.stream_sizes, vec![Some(50000000), Some(5000000)]);
    }

    #[test]
    fn test_parse_polysaver_meta_line_single_file_fallback() {
        // Audio preset: requested_formats is absent, so f0/f1 are NA.
        let line = "[POLYSAVER_META] title:Audio\tduration:200\tf0:NA|NA\tf1:NA|NA\tfallback_id:140\tfallback_size:5000000";
        let meta = parse_polysaver_meta_line(line).unwrap();
        assert_eq!(meta.format_ids, vec!["140".to_string()]);
        assert_eq!(meta.stream_sizes, vec![Some(5000000)]);
    }

    #[test]
    fn test_parse_polysaver_meta_line_missing_sizes() {
        let line =
            "[POLYSAVER_META] title:Odd\tduration:NA\tf0:602|NA\tfallback_id:NA\tfallback_size:NA";
        let meta = parse_polysaver_meta_line(line).unwrap();
        assert_eq!(meta.title, Some("Odd".to_string()));
        assert_eq!(meta.duration_seconds, None);
        assert_eq!(meta.format_ids, vec!["602".to_string()]);
        assert_eq!(meta.stream_sizes, vec![None]);
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

    /// Cancelling must kill the whole process group, so a grandchild process
    /// (as ffmpeg is for yt-dlp) cannot survive the cancellation.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_cancellation_kills_process_group_including_grandchildren() {
        let dir = std::env::temp_dir().join(format!("polysaver_pg_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();

        // Script that spawns a long-lived grandchild and reports its pid, then
        // waits forever itself. Mirrors yt-dlp -> ffmpeg.
        let script = dir.join("parent.sh");
        let pid_file = dir.join("grandchild.pid");
        tokio::fs::write(
            &script,
            format!(
                "#!/bin/sh\n/bin/sleep 300 &\necho $! > '{pid}'\nwhile true; do /bin/sleep 1; done\n",
                pid = pid_file.display()
            ),
        )
        .await
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let token = CancellationToken::new();
        let token_for_cancel = token.clone();

        let runner = tokio::spawn({
            let script = script.clone();
            let dir = dir.clone();
            async move {
                YtDlpProcessRunner::run(
                    &script,
                    None,
                    &[],
                    &dir,
                    Some(&token_for_cancel),
                    None,
                    None,
                )
                .await
            }
        });

        // Wait until the grandchild pid has been recorded.
        let mut grandchild_pid = 0i32;
        for _ in 0..100 {
            if let Ok(content) = tokio::fs::read_to_string(&pid_file).await {
                if let Ok(pid) = content.trim().parse::<i32>() {
                    grandchild_pid = pid;
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(grandchild_pid > 0, "grandchild process never started");

        // The grandchild is alive before cancellation.
        assert!(
            libc_kill_check(grandchild_pid),
            "grandchild should be running before cancellation"
        );

        token.cancel();
        let result = runner.await.unwrap();
        assert!(matches!(result, Err(CoreError::OperationCancelled)));

        // Give the OS a moment to reap, then assert the grandchild is gone.
        let mut gone = false;
        for _ in 0..40 {
            if !libc_kill_check(grandchild_pid) {
                gone = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(
            gone,
            "grandchild process {grandchild_pid} survived the cancellation"
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    /// Returns true when `pid` still exists (signal 0 performs error checking only).
    #[cfg(unix)]
    fn libc_kill_check(pid: i32) -> bool {
        unsafe { kill(pid, 0) == 0 }
    }
}
