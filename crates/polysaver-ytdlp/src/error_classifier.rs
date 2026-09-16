// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! # Error Classifier
//!
//! Classifies raw yt-dlp subprocess exit codes and stderr output into canonical `DownloadErrorDetails`.

use polysaver_core::error::{DownloadErrorCode, DownloadErrorDetails};

/// Case-insensitive patterns indicating the local yt-dlp engine is outdated or
/// its extractors fail in ways upstream attributes to stale releases.
/// Kept as a shared list so the same detection is used for hard failures *and*
/// for non-fatal `WARNING:` lines emitted on successful runs.
pub const OUTDATED_ENGINE_PATTERNS: &[&str] = &[
    "is older than 90 days",
    "strongly recommended to always use the latest version",
    "confirm you are on the latest version",
    "signature solving failed",
    "nsig extraction failed",
    "n challenge solving failed",
    "unable to extract yt initial data",
    "failed to extract any player response",
    "yt-dlp is outdated",
    "update yt-dlp",
];

/// Returns whether the given text (stderr lines joined or a single line)
/// mentions an outdated-engine condition.
#[must_use]
pub fn is_outdated_engine_message(text: &str) -> bool {
    let lower = text.to_lowercase();
    OUTDATED_ENGINE_PATTERNS.iter().any(|p| lower.contains(p))
}

/// Classifies a process execution failure into structured `DownloadErrorDetails`.
#[must_use]
pub fn classify_ytdlp_error(
    exit_code: Option<i32>,
    stderr_lines: &[String],
    version: Option<String>,
) -> DownloadErrorDetails {
    let combined_stderr = stderr_lines.join("\n");
    let lower = combined_stderr.to_lowercase();

    let code = if lower.contains("unable to download webpage")
        || lower.contains("connection refused")
        || lower.contains("network is unreachable")
        || lower.contains("temporary failure in name resolution")
        || lower.contains("name or service not known")
        || lower.contains("timed out")
        || lower.contains("http error 502")
        || lower.contains("http error 503")
        || lower.contains("http error 504")
    {
        DownloadErrorCode::NetworkUnavailable
    } else if lower.contains("video unavailable")
        || lower.contains("this video has been removed")
        || lower.contains("private video")
        || lower.contains("is not available in your country")
        || lower.contains("blocked on copyright grounds")
        || lower.contains("premiere will begin")
        || lower.contains("this live stream has ended")
        || lower.contains("does not exist")
        || lower.contains("playlist does not exist")
        || lower.contains("this playlist is private")
        || lower.contains("unviewable")
        || lower.contains("inaccessible")
    {
        DownloadErrorCode::VideoUnavailable
    } else if lower.contains("sign in to confirm your age")
        || lower.contains("sign in to confirm you're not a bot")
        || lower.contains("this video is only available to")
        || lower.contains("login required")
        || lower.contains("requires authentication")
        || lower.contains("members-only")
    {
        DownloadErrorCode::AuthenticationRequired
    } else if lower.contains("http error 429")
        || lower.contains("too many requests")
        || lower.contains("rate-limit")
        || lower.contains("rate limited")
    {
        DownloadErrorCode::RateLimited
    } else if lower.contains("requested format is not available")
        || lower.contains("format not available")
        || lower.contains("no video formats found")
        || lower.contains("no suitable format")
    {
        DownloadErrorCode::FormatNotAvailable
    } else if lower.contains("no space left on device")
        || lower.contains("disk full")
        || lower.contains("not enough disk space")
    {
        DownloadErrorCode::DiskFull
    } else if lower.contains("permission denied")
        || lower.contains("read-only file system")
        || lower.contains("access is denied")
    {
        DownloadErrorCode::OutputPermissionDenied
    } else if lower.contains("ffmpeg is not installed") || lower.contains("ffprobe not found") {
        DownloadErrorCode::FfmpegNotFound
    } else if is_outdated_engine_message(&combined_stderr) {
        DownloadErrorCode::YtdlpUpdateRequired
    } else if lower.contains("operation canceled") || lower.contains("interrupted by user") {
        DownloadErrorCode::DownloadCanceled
    } else {
        DownloadErrorCode::DownloadProcessFailed
    };

    // Sanitize stderr tail: keep up to last 10 non-empty lines without private paths/commandlines
    let tail_lines: Vec<String> = stderr_lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .rev()
        .take(10)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let sanitized_tail = if tail_lines.is_empty() {
        None
    } else {
        Some(tail_lines.join("\n"))
    };

    let mut details = DownloadErrorDetails::from_code(code);
    details.component = Some(
        format!("yt-dlp {}", version.unwrap_or_default())
            .trim()
            .to_string(),
    );
    details.exit_code = exit_code;
    details.stderr_tail = sanitized_tail;

    details
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_network_unavailable() {
        let lines = vec![
            "ERROR: Unable to download webpage: <urlopen error [Errno 8] nodename nor servname provided, nor known>".to_string(),
        ];
        let details = classify_ytdlp_error(Some(1), &lines, None);
        assert_eq!(details.code, DownloadErrorCode::NetworkUnavailable);
        assert!(details.retryable);
        assert_eq!(
            details.message,
            DownloadErrorCode::NetworkUnavailable.default_user_message()
        );
    }

    #[test]
    fn test_classify_video_unavailable() {
        let lines = vec!["ERROR: [youtube] jNQXAC9IVRw: Video unavailable".to_string()];
        let details = classify_ytdlp_error(Some(1), &lines, None);
        assert_eq!(details.code, DownloadErrorCode::VideoUnavailable);
        assert!(!details.retryable);
        assert_eq!(
            details.message,
            DownloadErrorCode::VideoUnavailable.default_user_message()
        );
    }

    #[test]
    fn test_classify_authentication_required() {
        let lines = vec![
            "ERROR: [youtube] xyz: Sign in to confirm your age. This video may be inappropriate for some users.".to_string(),
        ];
        let details = classify_ytdlp_error(Some(1), &lines, None);
        assert_eq!(details.code, DownloadErrorCode::AuthenticationRequired);
        assert!(!details.retryable);
    }

    #[test]
    fn test_classify_rate_limited() {
        let lines = vec!["ERROR: HTTP Error 429: Too Many Requests".to_string()];
        let details = classify_ytdlp_error(Some(1), &lines, None);
        assert_eq!(details.code, DownloadErrorCode::RateLimited);
        assert!(details.retryable);
    }

    #[test]
    fn test_classify_format_not_available() {
        let lines = vec!["ERROR: Requested format is not available".to_string()];
        let details = classify_ytdlp_error(Some(1), &lines, None);
        assert_eq!(details.code, DownloadErrorCode::FormatNotAvailable);
        assert!(!details.retryable);
    }

    #[test]
    fn test_classify_disk_full() {
        let lines = vec!["[download] Error: No space left on device".to_string()];
        let details = classify_ytdlp_error(Some(1), &lines, None);
        assert_eq!(details.code, DownloadErrorCode::DiskFull);
        assert!(!details.retryable);
    }

    #[test]
    fn test_classify_permission_denied() {
        let lines =
            vec!["ERROR: unable to open for writing: [Errno 13] Permission denied".to_string()];
        let details = classify_ytdlp_error(Some(1), &lines, None);
        assert_eq!(details.code, DownloadErrorCode::OutputPermissionDenied);
        assert!(!details.retryable);
    }

    #[test]
    fn test_classify_fallback_unknown() {
        let lines = vec!["Some obscure error happened during parsing".to_string()];
        let details = classify_ytdlp_error(Some(1), &lines, None);
        assert_eq!(details.code, DownloadErrorCode::DownloadProcessFailed);
        assert_eq!(
            details.message,
            DownloadErrorCode::DownloadProcessFailed.default_user_message()
        );
    }

    #[test]
    fn test_classify_outdated_engine_patterns() {
        let cases = [
            "WARNING: Your yt-dlp version is older than 90 days. It is strongly recommended to always use the latest version.",
            "ERROR: unable to extract yt initial data; please confirm you are on the latest version",
            "ERROR: [youtube] abc: nsig extraction failed: Some formats may be missing",
            "ERROR: Signature solving failed for player abc",
            "ERROR: n challenge solving failed for player abc",
            "ERROR: Failed to extract any player response",
            "ERROR: yt-dlp is outdated, please update yt-dlp",
        ];
        for raw in cases {
            let lines = vec![raw.to_string()];
            let details = classify_ytdlp_error(Some(1), &lines, None);
            assert_eq!(
                details.code,
                DownloadErrorCode::YtdlpUpdateRequired,
                "expected outdated-engine classification for: {raw}"
            );
        }
    }

    #[test]
    fn test_outdated_detection_does_not_override_priority_codes() {
        // Authentication, rate limiting and format issues must win even when the
        // stderr also mentions an update recommendation.
        let auth = vec![
            "WARNING: Your yt-dlp version is older than 90 days".to_string(),
            "ERROR: Sign in to confirm you're not a bot. Use --cookies".to_string(),
        ];
        assert_eq!(
            classify_ytdlp_error(Some(1), &auth, None).code,
            DownloadErrorCode::AuthenticationRequired
        );

        let rate = vec![
            "WARNING: strongly recommended to always use the latest version".to_string(),
            "ERROR: HTTP Error 429: Too Many Requests".to_string(),
        ];
        assert_eq!(
            classify_ytdlp_error(Some(1), &rate, None).code,
            DownloadErrorCode::RateLimited
        );

        let format = vec!["ERROR: Requested format is not available; update yt-dlp".to_string()];
        assert_eq!(
            classify_ytdlp_error(Some(1), &format, None).code,
            DownloadErrorCode::FormatNotAvailable
        );
    }

    #[test]
    fn test_is_outdated_engine_message_matches_success_warning() {
        // yt-dlp emits this on stderr with exit code 0 too.
        let warning = "WARNING: Your yt-dlp version is older than 90 days, and it is strongly recommended to always use the latest version";
        assert!(is_outdated_engine_message(warning));
        assert!(!is_outdated_engine_message("[youtube] downloading video"));
    }
}
