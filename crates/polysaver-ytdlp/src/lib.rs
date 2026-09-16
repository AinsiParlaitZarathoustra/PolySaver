// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! # PolySaver yt-dlp Adapter
//!
//! Encapsulates `yt-dlp` execution and implements the `MediaProvider` and `MediaDownloader` ports.

pub mod aggregator;
pub mod error_classifier;
pub mod js_runtime;
pub mod node_installer;
pub mod process_runner;
pub mod updater;

use async_trait::async_trait;
use polysaver_binres::BinaryResolver;
use polysaver_core::domain::{
    CookiesBrowser, DownloadPreset, FormatOption, MediaUrl, OutputFormat, PlaylistEntry,
    ProbeResult, PLAYLIST_ENTRIES_LIMIT,
};
use polysaver_core::error::{CoreError, DownloadErrorCode, DownloadErrorDetails};
use polysaver_core::ports::media_downloader::{
    CookiesDiagnostic, DownloadStreamRequest, DownloadedStreams, JsRuntimeSpec, MediaDownloader,
    StreamProgress,
};
use polysaver_core::ports::playlist_detector::{PlaylistDetection, PlaylistDetector};
use polysaver_core::ports::MediaProvider;
use process_runner::{parse_fallback_progress_line, YtDlpProcessRunner};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// `before_dl` print template describing the streams yt-dlp will fetch.
///
/// `requested_formats.N` only exists when the selector merged ≥ 2 formats; for
/// single-file selections (audio presets, muxed `best`) the entries are `NA` and
/// the `fallback_*` tokens are used instead.
const META_PRINT_TEMPLATE: &str = "before_dl:[POLYSAVER_META] title:%(title)s\tduration:%(duration)s\tf0:%(requested_formats.0.format_id)s|%(requested_formats.0.filesize,filesize_approx)s\tf1:%(requested_formats.1.format_id)s|%(requested_formats.1.filesize,filesize_approx)s\tf2:%(requested_formats.2.format_id)s|%(requested_formats.2.filesize,filesize_approx)s\tfallback_id:%(format_id)s\tfallback_size:%(filesize,filesize_approx)s";

/// Progress template. Uses `%(info.format_id)s` (the reliable stream boundary
/// marker) and numeric fields only; the percentage is computed downstream.
const PROGRESS_TEMPLATE: &str = "download:[POLYSAVER_PROGRESS] status:%(progress.status)s downloaded:%(progress.downloaded_bytes)s total:%(progress.total_bytes)s est:%(progress.total_bytes_estimate)s speed:%(progress.speed)s stream:%(info.format_id)s";

/// Appends `--cookies-from-browser <browser>` when a browser is configured.
///
/// The value comes from a closed enum, so no arbitrary string can reach the
/// command line. Never combined with `--cookies` (yt-dlp rejects both at once).
fn push_cookies_from_browser_arg(args: &mut Vec<String>, browser: Option<CookiesBrowser>) {
    if let Some(browser) = browser {
        args.push("--cookies-from-browser".to_string());
        args.push(browser.as_str().to_string());
    }
}

/// Appends `--js-runtimes <name>:<absolute path>` when a runtime is available.
///
/// The path must be absolute: a GUI app on macOS does not inherit the shell
/// `PATH`, so a bare `node` would not resolve. `--no-js-runtimes` is never
/// passed, so a user-installed Deno keeps its default priority over Node.
/// `--remote-components` is not used either: official yt-dlp builds already
/// bundle `yt-dlp-ejs`.
fn push_js_runtime_arg(args: &mut Vec<String>, runtime: Option<&JsRuntimeSpec>) {
    if let Some(runtime) = runtime {
        args.push("--js-runtimes".to_string());
        args.push(format!("{}:{}", runtime.name, runtime.path));
    }
}

/// Inspects stderr lines for cookie import outcomes so failures are never silent.
#[must_use]
pub fn detect_cookies_diagnostic(stderr_lines: &[String]) -> Option<CookiesDiagnostic> {
    let joined = stderr_lines.join("\n").to_lowercase();
    if joined.contains("cannot decrypt v10 cookies")
        || joined.contains("find-generic-password failed")
        || joined.contains("failed to decrypt")
    {
        return Some(CookiesDiagnostic::DecryptFailed);
    }
    if joined.contains("permission denied")
        && (joined.contains("cookie") || joined.contains("safari"))
    {
        return Some(CookiesDiagnostic::PermissionDenied);
    }
    if joined.contains("extracted") && joined.contains("cookies from") {
        return Some(CookiesDiagnostic::Imported);
    }
    None
}

/// Diagnostic availability status for yt-dlp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtDlpAvailability {
    pub is_ready: bool,
    pub version: Option<String>,
    pub binary_path: Option<String>,
    pub status_message: String,
}

/// Matches the placeholder titles yt-dlp lists for entries that cannot be played.
fn is_unavailable_title(title: &str) -> bool {
    const MARKERS: &[&str] = &[
        "[private video]",
        "[deleted video]",
        "[unavailable video]",
        "[members-only video]",
    ];
    let lower = title.trim().to_ascii_lowercase();
    MARKERS.contains(&lower.as_str())
}

/// Builds a playlist entry from one raw flat-playlist JSON object.
///
/// In flat mode `webpage_url` is absent at entry level; `url` already holds a full
/// watch URL on YouTube. When it does not (other extractors), the `id` is used to
/// rebuild a canonical YouTube watch URL. Entries with neither are dropped by the
/// caller. `availability` is always `null` in flat mode, so unplayable entries are
/// detected from their bracketed placeholder title instead.
fn playlist_entry_from_json(entry: &serde_json::Value, index: usize) -> Option<PlaylistEntry> {
    let url = entry["url"]
        .as_str()
        .map(String::from)
        .or_else(|| {
            entry["id"]
                .as_str()
                .map(|id| format!("https://www.youtube.com/watch?v={id}"))
        })
        .and_then(|raw| MediaUrl::parse(&raw).ok())?;

    let title = entry["title"].as_str().unwrap_or("").to_string();
    let thumbnail_url = entry["thumbnails"]
        .as_array()
        .and_then(|thumbs| thumbs.first())
        .and_then(|thumb| thumb["url"].as_str())
        .map(String::from);

    Some(PlaylistEntry {
        index: u32::try_from(index).unwrap_or(u32::MAX),
        url,
        available: !is_unavailable_title(&title),
        title,
        duration_seconds: entry["duration"].as_u64(),
        thumbnail_url,
    })
}

/// Builds the playlist probe result from a flat `--dump-single-json` document.
///
/// An empty `entries` array (empty playlist, empty Mix/radio) is a success, not an
/// error: the UI shows a dedicated "this playlist is empty" message.
fn playlist_probe_result(url: &MediaUrl, parsed: &serde_json::Value) -> ProbeResult {
    let title = parsed["title"].as_str().unwrap_or("Playlist").to_string();
    let uploader = parsed["uploader"]
        .as_str()
        .or_else(|| parsed["channel"].as_str())
        .map(String::from);
    let thumbnail_url = parsed["thumbnails"]
        .as_array()
        .and_then(|thumbs| thumbs.first())
        .and_then(|thumb| thumb["url"].as_str())
        .map(String::from)
        .or_else(|| parsed["thumbnail"].as_str().map(String::from));

    let entries = parsed["entries"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .enumerate()
                .filter_map(|(position, entry)| playlist_entry_from_json(entry, position + 1))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    ProbeResult::new_playlist(
        url.clone(),
        title,
        thumbnail_url,
        uploader,
        entries,
        parsed["playlist_count"].as_u64(),
    )
}

/// Builds the `--dump-single-json` argument list for a probe.
///
/// Playlist URLs get `--flat-playlist` (lists entries without extracting each
/// video: seconds instead of minutes) plus `--playlist-end`. `--lazy-playlist` is
/// never passed: it would return a partial listing instead of the full, ordered
/// set of entries the selection UI needs.
fn build_probe_args(
    url: &MediaUrl,
    cookies: Option<CookiesBrowser>,
    js_runtime: Option<&JsRuntimeSpec>,
) -> Vec<String> {
    let mut args = vec![
        "--dump-single-json".to_string(),
        "--no-warnings".to_string(),
    ];
    if url.is_playlist() {
        args.push("--flat-playlist".to_string());
        args.push("--playlist-end".to_string());
        args.push(PLAYLIST_ENTRIES_LIMIT.to_string());
        // A long listing pages through the extractor; keep the network bounded.
        args.push("--socket-timeout".to_string());
        args.push("30".to_string());
        args.push("--extractor-retries".to_string());
        args.push("2".to_string());
    }
    push_cookies_from_browser_arg(&mut args, cookies);
    push_js_runtime_arg(&mut args, js_runtime);
    args.push(url.as_str().to_string());
    args
}

/// Builds the argument list for the native "is this a playlist?" detection.
///
/// `--playlist-items 0` (an empty selection) asks yt-dlp for the *envelope* only:
/// no entry is resolved, but the extractor still fetches the listing metadata, so
/// `_type`, `title` and `playlist_count` come back without extracting a single
/// video. This is the native answer, as opposed to the URL-shape heuristic: only
/// the engine knows what an URL really resolves to.
///
/// `--flat-playlist` keeps the call cheap on multi-video URLs and has no effect on
/// a single video. `--lazy-playlist` is deliberately never passed (it disables
/// `n_entries` and brings nothing here), and neither is `--skip-download`, which
/// still writes auxiliary files.
///
/// Measured with the bundled engine (2026.08.19, macOS arm64, warm cache): about
/// 1.2 s for a playlist or a channel, 2.3 s for a single video. `playlist_count` is
/// not always available (it is `null` on `/@handle/videos`, where the total is
/// unknown to the extractor), so it must stay optional.
fn build_detect_args(url: &MediaUrl) -> Vec<String> {
    vec![
        "--dump-single-json".to_string(),
        "--no-warnings".to_string(),
        "--flat-playlist".to_string(),
        "--playlist-items".to_string(),
        "0".to_string(),
        url.as_str().to_string(),
    ]
}

/// Interprets a `-J` document as "playlist / not a playlist".
///
/// `_type` is always present in dumped JSON (`sanitize_info` defaults it to
/// `video`), and `multi_video` describes the multi-video results of nine
/// extractors, so it counts as a playlist. A `null` document is a failure, never a
/// video: yt-dlp prints `null` with a non-zero exit code for a private or missing
/// playlist, and the caller surfaces that before reaching here.
fn parse_playlist_detection(parsed: &serde_json::Value) -> Result<PlaylistDetection, CoreError> {
    if parsed.is_null() {
        return Err(CoreError::ProviderError(
            "Le moteur n'a renvoyé aucune information pour cette adresse.".to_string(),
        ));
    }
    let media_type = parsed["_type"].as_str().unwrap_or("video");
    Ok(PlaylistDetection {
        is_playlist: matches!(media_type, "playlist" | "multi_video"),
    })
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
///
/// Cookie extraction is stored on the downloader (not in the request) so the
/// same value applies to both `probe` and `download_stream`; the composition
/// root updates it whenever settings change.
pub struct YtDlpDownloader {
    resolver: Arc<BinaryResolver>,
    cookies_from_browser: Arc<RwLock<Option<CookiesBrowser>>>,
    js_runtime: Arc<RwLock<Option<JsRuntimeSpec>>>,
}

impl YtDlpDownloader {
    /// Creates a new `YtDlpDownloader` with dedicated app binary directory.
    #[must_use]
    pub fn new(bin_dir: PathBuf) -> Self {
        Self {
            resolver: Arc::new(BinaryResolver::new(bin_dir, None)),
            cookies_from_browser: Arc::new(RwLock::new(None)),
            js_runtime: Arc::new(RwLock::new(None)),
        }
    }

    /// Creates a new `YtDlpDownloader` with optional resource directory.
    #[must_use]
    pub fn with_resource_dir(bin_dir: PathBuf, resource_bin_dir: Option<PathBuf>) -> Self {
        Self {
            resolver: Arc::new(BinaryResolver::new(bin_dir, resource_bin_dir)),
            cookies_from_browser: Arc::new(RwLock::new(None)),
            js_runtime: Arc::new(RwLock::new(None)),
        }
    }

    /// Creates a new `YtDlpDownloader` with an injected shared `BinaryResolver`.
    #[must_use]
    pub fn with_resolver(resolver: Arc<BinaryResolver>) -> Self {
        Self {
            resolver,
            cookies_from_browser: Arc::new(RwLock::new(None)),
            js_runtime: Arc::new(RwLock::new(None)),
        }
    }

    /// Sets (or clears) the browser used for `--cookies-from-browser`.
    pub async fn set_cookies_from_browser(&self, browser: Option<CookiesBrowser>) {
        let mut guard = self.cookies_from_browser.write().await;
        *guard = browser;
    }

    /// Reads the currently configured cookie source, if any.
    async fn cookies_from_browser(&self) -> Option<CookiesBrowser> {
        *self.cookies_from_browser.read().await
    }

    /// Sets (or clears) the JavaScript runtime passed to yt-dlp.
    pub async fn set_js_runtime(&self, runtime: Option<JsRuntimeSpec>) {
        let mut guard = self.js_runtime.write().await;
        *guard = runtime;
    }

    /// Reads the currently configured JavaScript runtime, if any.
    async fn js_runtime(&self) -> Option<JsRuntimeSpec> {
        self.js_runtime.read().await.clone()
    }
}

#[async_trait]
impl MediaProvider for YtDlpDownloader {
    async fn probe(
        &self,
        url: &MediaUrl,
        cancellation_token: Option<tokio_util::sync::CancellationToken>,
    ) -> Result<ProbeResult, CoreError> {
        let resolved = self.resolver.resolve_ytdlp().await.map_err(|_| {
            CoreError::DownloadFailed(DownloadErrorDetails::from_code(
                DownloadErrorCode::YtdlpNotFound,
            ))
        })?;
        let binary = resolved.path;
        let version = Some(resolved.version);
        let ffmpeg_bin = self.resolver.resolve_ffmpeg().await.ok().map(|r| r.path);

        let temp_dir = std::env::temp_dir();
        let args = build_probe_args(
            url,
            self.cookies_from_browser().await,
            self.js_runtime().await.as_ref(),
        );

        let run_result = YtDlpProcessRunner::run(
            &binary,
            ffmpeg_bin.as_deref(),
            &args,
            &temp_dir,
            cancellation_token.as_ref(),
            None,
            version,
        )
        .await?;

        let json_str = run_result.stdout_lines.join("\n");
        let parsed: serde_json::Value = serde_json::from_str(&json_str).map_err(|err| {
            CoreError::ProviderError(format!("Failed to parse metadata JSON: {err}"))
        })?;

        // A non-existing/private playlist makes yt-dlp print `null` with a non-zero
        // exit code, which the runner already surfaced as an error above. An empty
        // playlist, by contrast, is a valid result: `_type: playlist`, `entries: []`.
        if parsed.is_null() {
            return Err(CoreError::DownloadFailed(
                polysaver_core::error::DownloadErrorDetails::new(
                    DownloadErrorCode::VideoUnavailable,
                    "Playlist introuvable, privée ou inaccessible.",
                    false,
                ),
            ));
        }
        if url.is_playlist() || parsed["_type"].as_str() == Some("playlist") {
            return Ok(playlist_probe_result(url, &parsed));
        }

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
                let tbr = f["tbr"].as_f64().filter(|t| *t > 0.0);

                formats.push(FormatOption {
                    format_id: format_id.to_string(),
                    height,
                    has_video,
                    has_audio,
                    extension: ext,
                    filesize_approx_bytes: filesize_approx,
                    tbr,
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
impl PlaylistDetector for YtDlpDownloader {
    async fn detect_playlist(
        &self,
        url: &MediaUrl,
        cancellation_token: Option<tokio_util::sync::CancellationToken>,
    ) -> Result<PlaylistDetection, CoreError> {
        let resolved = self.resolver.resolve_ytdlp().await.map_err(|_| {
            CoreError::DownloadFailed(DownloadErrorDetails::from_code(
                DownloadErrorCode::YtdlpNotFound,
            ))
        })?;
        let binary = resolved.path;
        let version = Some(resolved.version);
        let ffmpeg_bin = self.resolver.resolve_ffmpeg().await.ok().map(|r| r.path);

        let temp_dir = std::env::temp_dir();
        let args = build_detect_args(url);

        let run_result = YtDlpProcessRunner::run(
            &binary,
            ffmpeg_bin.as_deref(),
            &args,
            &temp_dir,
            cancellation_token.as_ref(),
            None,
            version,
        )
        .await?;

        let json_str = run_result.stdout_lines.join("\n");
        let parsed: serde_json::Value = serde_json::from_str(&json_str).map_err(|err| {
            CoreError::ProviderError(format!("Failed to parse metadata JSON: {err}"))
        })?;

        parse_playlist_detection(&parsed)
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
            META_PRINT_TEMPLATE.to_string(),
            "--progress-template".to_string(),
            PROGRESS_TEMPLATE.to_string(),
            "--print".to_string(),
            "after_move:[POLYSAVER_OUTPUT] %(filepath)s".to_string(),
        ];

        push_cookies_from_browser_arg(&mut download_args, self.cookies_from_browser().await);
        push_js_runtime_arg(&mut download_args, self.js_runtime().await.as_ref());

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
            engine_outdated: run_result.engine_outdated,
            cookies_diagnostic: run_result.cookies_diagnostic,
        })
    }
}

/// Helper function to parse human progress line for backward compatibility in unit tests.
pub fn parse_ytdlp_progress_line(line: &str) -> StreamProgress {
    parse_fallback_progress_line(line).unwrap_or(StreamProgress {
        percent: None,
        downloaded_bytes: None,
        total_bytes: None,
        total_bytes_estimate: None,
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

    #[test]
    fn test_cookies_argument_is_injected_and_absent_by_default() {
        // No browser configured: no cookie flag at all.
        let mut args = vec!["--dump-single-json".to_string()];
        push_cookies_from_browser_arg(&mut args, None);
        assert!(!args.iter().any(|a| a == "--cookies-from-browser"));

        // Configured browser: exactly one flag followed by the canonical value.
        let mut args = vec!["--dump-single-json".to_string()];
        push_cookies_from_browser_arg(&mut args, Some(CookiesBrowser::Firefox));
        assert_eq!(
            args,
            vec![
                "--dump-single-json".to_string(),
                "--cookies-from-browser".to_string(),
                "firefox".to_string()
            ]
        );

        // Every supported browser maps to its exact yt-dlp value.
        for (browser, expected) in [
            (CookiesBrowser::Brave, "brave"),
            (CookiesBrowser::Chrome, "chrome"),
            (CookiesBrowser::Chromium, "chromium"),
            (CookiesBrowser::Edge, "edge"),
            (CookiesBrowser::Opera, "opera"),
            (CookiesBrowser::Safari, "safari"),
            (CookiesBrowser::Vivaldi, "vivaldi"),
            (CookiesBrowser::Whale, "whale"),
        ] {
            let mut args = Vec::new();
            push_cookies_from_browser_arg(&mut args, Some(browser));
            assert_eq!(
                args,
                vec!["--cookies-from-browser".to_string(), expected.to_string()]
            );
        }
    }

    #[tokio::test]
    async fn test_downloader_cookie_setting_round_trip() {
        let bin_dir =
            std::env::temp_dir().join(format!("polysaver_cookies_{}", uuid::Uuid::new_v4()));
        let downloader = YtDlpDownloader::new(bin_dir);

        assert!(downloader.cookies_from_browser().await.is_none());
        downloader
            .set_cookies_from_browser(Some(CookiesBrowser::Firefox))
            .await;
        assert_eq!(
            downloader.cookies_from_browser().await,
            Some(CookiesBrowser::Firefox)
        );
        downloader.set_cookies_from_browser(None).await;
        assert!(downloader.cookies_from_browser().await.is_none());
    }

    #[test]
    fn test_progress_templates_use_reliable_fields() {
        // The stream identifier must come from `info.format_id`: `progress.info_dict`
        // is removed by yt-dlp before template evaluation and always renders NA.
        assert!(PROGRESS_TEMPLATE.contains("%(info.format_id)s"));
        assert!(!PROGRESS_TEMPLATE.contains("info_dict"));
        // Numeric fields only: `_percent_str` is a string that reads "N/A%".
        assert!(!PROGRESS_TEMPLATE.contains("_percent_str"));
        assert!(PROGRESS_TEMPLATE.contains("%(progress.downloaded_bytes)s"));
        assert!(PROGRESS_TEMPLATE.contains("%(progress.total_bytes_estimate)s"));
        assert!(PROGRESS_TEMPLATE.contains("status:%(progress.status)s"));

        // Indexed per-stream metadata with an explicit single-file fallback.
        assert!(META_PRINT_TEMPLATE.contains("f0:%(requested_formats.0.format_id)s"));
        assert!(META_PRINT_TEMPLATE.contains("fallback_id:%(format_id)s"));
        assert!(META_PRINT_TEMPLATE.contains("fallback_size:%(filesize,filesize_approx)s"));
    }

    #[test]
    fn test_detect_args_request_the_envelope_only() {
        let url = MediaUrl::parse("https://www.youtube.com/watch?v=abc").unwrap();
        let args = build_detect_args(&url);

        assert!(args.iter().any(|a| a == "--dump-single-json"));
        assert!(args.iter().any(|a| a == "--flat-playlist"));

        // An empty selection: the listing metadata is fetched, no entry resolved.
        let items_index = args
            .iter()
            .position(|a| a == "--playlist-items")
            .expect("--playlist-items must be present");
        assert_eq!(args[items_index + 1], "0");

        // `--lazy-playlist` disables `n_entries`; `--skip-download` still writes
        // auxiliary files. Neither belongs in a detection call.
        assert!(!args.iter().any(|a| a == "--lazy-playlist"));
        assert!(!args.iter().any(|a| a == "--skip-download"));
        // The URL stays last, so no user value can be swallowed as a flag argument.
        assert_eq!(args.last().map(String::as_str), Some(url.as_str()));
    }

    #[test]
    fn test_playlist_detection_reads_the_native_type() {
        // A playlist envelope, as returned by `-J --flat-playlist -I 0`.
        let playlist: serde_json::Value = serde_json::from_str(
            r#"{"_type": "playlist", "title": "Ma playlist", "playlist_count": 35, "entries": []}"#,
        )
        .unwrap();
        assert!(parse_playlist_detection(&playlist).unwrap().is_playlist);

        // Multi-video results of nine extractors are playlists too.
        let multi: serde_json::Value =
            serde_json::from_str(r#"{"_type": "multi_video", "entries": []}"#).unwrap();
        assert!(parse_playlist_detection(&multi).unwrap().is_playlist);

        // A single video: `_type` is present and equals `video`.
        let video: serde_json::Value =
            serde_json::from_str(r#"{"_type": "video", "title": "Vidéo"}"#).unwrap();
        assert!(!parse_playlist_detection(&video).unwrap().is_playlist);

        // A missing `_type` falls back to the JSON default (`video`).
        let no_type: serde_json::Value = serde_json::from_str(r#"{"title": "Vidéo"}"#).unwrap();
        assert!(!parse_playlist_detection(&no_type).unwrap().is_playlist);

        // `null` is a failure (private/missing playlist), never a video.
        let null_doc: serde_json::Value = serde_json::from_str("null").unwrap();
        assert!(parse_playlist_detection(&null_doc).is_err());
    }

    #[test]
    fn test_probe_args_enable_flat_playlist_only_for_listings() {
        let video = MediaUrl::parse("https://www.youtube.com/watch?v=abc").unwrap();
        let video_args = build_probe_args(&video, None, None);
        assert!(!video_args.iter().any(|a| a == "--flat-playlist"));
        assert!(!video_args.iter().any(|a| a == "--playlist-end"));
        assert_eq!(video_args.last().map(String::as_str), Some(video.as_str()));

        let playlist = MediaUrl::parse("https://www.youtube.com/playlist?list=PL42").unwrap();
        let playlist_args = build_probe_args(&playlist, None, None);

        assert!(playlist_args.iter().any(|a| a == "--flat-playlist"));
        // `--lazy-playlist` would return a partial listing; it must never appear.
        assert!(!playlist_args.iter().any(|a| a == "--lazy-playlist"));

        let limit_position = playlist_args
            .iter()
            .position(|a| a == "--playlist-end")
            .expect("--playlist-end must be present");
        assert_eq!(
            playlist_args[limit_position + 1],
            PLAYLIST_ENTRIES_LIMIT.to_string()
        );

        for flag in ["--socket-timeout", "--extractor-retries"] {
            assert!(playlist_args.iter().any(|a| a == flag), "missing {flag}");
        }

        // The URL stays last so no user value can be swallowed as a flag argument.
        assert_eq!(
            playlist_args.last().map(String::as_str),
            Some(playlist.as_str())
        );
    }

    #[test]
    fn test_playlist_probe_result_builds_entries_with_positions() {
        let raw = r#"{
            "_type": "playlist",
            "title": "Ma playlist",
            "uploader": "Une chaîne",
            "playlist_count": 12000,
            "thumbnails": [{"url": "https://i.ytimg.com/vi/first/hqdefault.jpg"}],
            "entries": [
                {"url": "https://www.youtube.com/watch?v=aaa", "title": "Première", "duration": 61,
                 "thumbnails": [{"url": "https://i.ytimg.com/vi/aaa/hqdefault.jpg"}]},
                {"url": "https://www.youtube.com/watch?v=bbb", "title": "[Private video]", "duration": null},
                {"id": "ccc", "title": "Sans url"}
            ]
        }"#;
        let parsed: serde_json::Value =
            serde_json::from_str(raw).expect("fixture must be valid JSON");
        let url = MediaUrl::parse("https://www.youtube.com/playlist?list=PL42").expect("valid URL");

        let result = playlist_probe_result(&url, &parsed);

        assert_eq!(result.kind, polysaver_core::domain::MediaKind::Playlist);
        assert_eq!(result.title, "Ma playlist");
        assert_eq!(result.uploader.as_deref(), Some("Une chaîne"));
        assert_eq!(result.playlist_total, Some(12000));
        assert_eq!(result.entries.len(), 3);

        assert_eq!(result.entries[0].index, 1);
        assert_eq!(result.entries[0].title, "Première");
        assert_eq!(result.entries[0].duration_seconds, Some(61));
        assert!(result.entries[0].available);
        assert_eq!(
            result.entries[0].thumbnail_url.as_deref(),
            Some("https://i.ytimg.com/vi/aaa/hqdefault.jpg")
        );

        // Flat mode never fills `availability`: unplayable entries are detected from
        // their bracketed placeholder title, and the row stays visible but disabled.
        assert!(!result.entries[1].available);
        assert_eq!(result.entries[1].title, "[Private video]");

        // No `url`, but an `id`: the watch URL is rebuilt.
        assert_eq!(result.entries[2].index, 3);
        assert_eq!(
            result.entries[2].url.as_str(),
            "https://www.youtube.com/watch?v=ccc"
        );
        assert_eq!(result.available_entries().len(), 2);
    }

    #[test]
    fn test_playlist_probe_result_accepts_empty_playlist() {
        let raw = r#"{"_type": "playlist", "title": "Vide", "entries": []}"#;
        let parsed: serde_json::Value =
            serde_json::from_str(raw).expect("fixture must be valid JSON");
        let url =
            MediaUrl::parse("https://www.youtube.com/playlist?list=PLempty").expect("valid URL");

        let result = playlist_probe_result(&url, &parsed);

        assert_eq!(result.kind, polysaver_core::domain::MediaKind::Playlist);
        assert!(result.entries.is_empty());
        assert_eq!(result.playlist_total, None);
    }

    #[test]
    fn test_playlist_entry_drops_unusable_urls() {
        let raw = r#"{"_type": "playlist", "title": "Mixte", "entries": [
            {"title": "Ni url ni id"},
            {"url": "file:///etc/passwd", "title": "Schéma interdit"},
            {"url": "http://127.0.0.1/watch", "title": "Adresse locale"},
            {"id": "ok1", "title": "Valide"}
        ]}"#;
        let parsed: serde_json::Value =
            serde_json::from_str(raw).expect("fixture must be valid JSON");
        let url =
            MediaUrl::parse("https://www.youtube.com/playlist?list=PLmix").expect("valid URL");

        let result = playlist_probe_result(&url, &parsed);

        // Entries without a usable, validated URL are dropped entirely: the frontend
        // can never receive an address that `MediaUrl` would reject.
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].title, "Valide");
    }

    #[test]
    fn test_unavailable_title_detection_is_exact() {
        assert!(is_unavailable_title("[Private video]"));
        assert!(is_unavailable_title("[Deleted video]"));
        assert!(is_unavailable_title("[Unavailable video]"));
        assert!(is_unavailable_title("[Members-only video]"));
        // A real video whose title merely starts with a bracket is not a placeholder.
        assert!(!is_unavailable_title("[Official] Ma vidéo"));
        assert!(!is_unavailable_title("Private video"));
    }

    #[test]
    fn test_cookies_decryption_failures_are_detected() {
        let decrypt = vec!["WARNING: cannot decrypt v10 cookies: no key found".to_string()];
        assert_eq!(
            detect_cookies_diagnostic(&decrypt),
            Some(CookiesDiagnostic::DecryptFailed)
        );

        let keychain = vec!["ERROR: find-generic-password failed".to_string()];
        assert_eq!(
            detect_cookies_diagnostic(&keychain),
            Some(CookiesDiagnostic::DecryptFailed)
        );

        let safari = vec!["ERROR: Permission denied when reading Safari cookies".to_string()];
        assert_eq!(
            detect_cookies_diagnostic(&safari),
            Some(CookiesDiagnostic::PermissionDenied)
        );

        let success = vec!["Extracted 42 cookies from firefox".to_string()];
        assert_eq!(
            detect_cookies_diagnostic(&success),
            Some(CookiesDiagnostic::Imported)
        );

        let unrelated = vec!["[download] 100% of 10MiB".to_string()];
        assert_eq!(detect_cookies_diagnostic(&unrelated), None);
    }
}
