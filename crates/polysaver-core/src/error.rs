// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::download_job::DownloadId;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Named canonical error code for downloads and media operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DownloadErrorCode {
    YtdlpNotFound,
    YtdlpStartFailed,
    YtdlpUpdateRequired,
    NetworkUnavailable,
    VideoUnavailable,
    AuthenticationRequired,
    RateLimited,
    FormatNotAvailable,
    FfmpegNotFound,
    FfmpegFailed,
    OutputPermissionDenied,
    OutputFileNotFound,
    SourceUrlInvalid,
    HistorySaveFailed,
    DiskFull,
    DownloadCanceled,
    DownloadProcessFailed,
}

impl DownloadErrorCode {
    /// Machine-readable static string representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::YtdlpNotFound => "YTDLP_NOT_FOUND",
            Self::YtdlpStartFailed => "YTDLP_START_FAILED",
            Self::YtdlpUpdateRequired => "YTDLP_UPDATE_REQUIRED",
            Self::NetworkUnavailable => "NETWORK_UNAVAILABLE",
            Self::VideoUnavailable => "VIDEO_UNAVAILABLE",
            Self::AuthenticationRequired => "AUTHENTICATION_REQUIRED",
            Self::RateLimited => "RATE_LIMITED",
            Self::FormatNotAvailable => "FORMAT_NOT_AVAILABLE",
            Self::FfmpegNotFound => "FFMPEG_NOT_FOUND",
            Self::FfmpegFailed => "FFMPEG_FAILED",
            Self::OutputPermissionDenied => "OUTPUT_PERMISSION_DENIED",
            Self::OutputFileNotFound => "OUTPUT_FILE_NOT_FOUND",
            Self::SourceUrlInvalid => "SOURCE_URL_INVALID",
            Self::HistorySaveFailed => "HISTORY_SAVE_FAILED",
            Self::DiskFull => "DISK_FULL",
            Self::DownloadCanceled => "DOWNLOAD_CANCELED",
            Self::DownloadProcessFailed => "DOWNLOAD_PROCESS_FAILED",
        }
    }

    /// Default technical English description for this error.
    #[must_use]
    pub const fn default_user_message(&self) -> &'static str {
        match self {
            Self::YtdlpNotFound => "Download engine not found.",
            Self::YtdlpStartFailed => "Failed to start download engine.",
            Self::YtdlpUpdateRequired => "Download engine update required.",
            Self::NetworkUnavailable => "Network unavailable or connection failed.",
            Self::VideoUnavailable => "Media stream unavailable or deleted.",
            Self::AuthenticationRequired => "Authentication required.",
            Self::RateLimited => "Rate limit exceeded by provider.",
            Self::FormatNotAvailable => "Requested quality or format not available.",
            Self::FfmpegNotFound => "Media conversion engine not found.",
            Self::FfmpegFailed => "Media processing or muxing failed.",
            Self::OutputPermissionDenied => "Permission denied for output directory.",
            Self::OutputFileNotFound => "Output media file not found.",
            Self::SourceUrlInvalid => "Source media URL is invalid.",
            Self::HistorySaveFailed => "Failed to persist download history entry.",
            Self::DiskFull => "Insufficient disk space.",
            Self::DownloadCanceled => "Download canceled.",
            Self::DownloadProcessFailed => "Download process failed.",
        }
    }

    /// Whether this error condition can typically be retried.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        match self {
            Self::NetworkUnavailable
            | Self::RateLimited
            | Self::DownloadProcessFailed
            | Self::HistorySaveFailed => true,
            Self::YtdlpNotFound
            | Self::YtdlpStartFailed
            | Self::YtdlpUpdateRequired
            | Self::VideoUnavailable
            | Self::AuthenticationRequired
            | Self::FormatNotAvailable
            | Self::FfmpegNotFound
            | Self::FfmpegFailed
            | Self::OutputPermissionDenied
            | Self::OutputFileNotFound
            | Self::SourceUrlInvalid
            | Self::DiskFull
            | Self::DownloadCanceled => false,
        }
    }
}

impl fmt::Display for DownloadErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Structured details for a download failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadErrorDetails {
    pub code: DownloadErrorCode,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr_tail: Option<String>,
}

impl DownloadErrorDetails {
    /// Creates a new `DownloadErrorDetails` with default retryability.
    #[must_use]
    pub fn from_code(code: DownloadErrorCode) -> Self {
        Self {
            message: code.default_user_message().to_string(),
            retryable: code.is_retryable(),
            code,
            component: None,
            exit_code: None,
            stderr_tail: None,
        }
    }

    /// Creates a new `DownloadErrorDetails` with custom message.
    #[must_use]
    pub fn new(code: DownloadErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
            component: None,
            exit_code: None,
            stderr_tail: None,
        }
    }
}

impl fmt::Display for DownloadErrorDetails {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

/// Sovereign domain errors for PolySaver V2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoreError {
    /// Invalid URL syntax or forbidden scheme.
    InvalidUrl(String),

    /// Playlist URLs are not supported in this version.
    PlaylistNotSupported(String),

    /// Job ID not found.
    JobNotFound(DownloadId),

    /// State machine violation.
    IllegalTransition {
        current: String,
        attempted: String,
        reason: String,
    },

    /// Progress value is decreasing or outside 0..=100.
    InvalidProgress { current: u8, attempted: u8 },

    /// Domain entity or field validation error.
    ValidationError(String),

    /// Settings invariant violation.
    InvalidSettings(String),

    /// External metadata / download provider error.
    ProviderError(String),

    /// External audio/video converter error or missing binary.
    ConverterError(String),

    /// Converter binary unavailable.
    ConverterUnavailable(String),

    /// Persistence / storage error.
    StorageError(String),

    /// Concurrency or cancellation error.
    Canceled(String),

    /// Operation cancelled explicitly.
    OperationCancelled,

    /// Invalid state for operation.
    InvalidState(String),

    /// Structured named download error.
    DownloadFailed(DownloadErrorDetails),
}

impl CoreError {
    /// Machine-readable stable error code for IPC and telemetry.
    #[must_use]
    pub fn machine_code(&self) -> &'static str {
        match self {
            Self::InvalidUrl(_) => "INVALID_URL",
            Self::PlaylistNotSupported(_) => "PLAYLIST_NOT_SUPPORTED",
            Self::JobNotFound(_) => "JOB_NOT_FOUND",
            Self::IllegalTransition { .. } => "ILLEGAL_TRANSITION",
            Self::InvalidProgress { .. } => "INVALID_PROGRESS",
            Self::ValidationError(_) => "VALIDATION_ERROR",
            Self::InvalidSettings(_) => "INVALID_SETTINGS",
            Self::ProviderError(_) => "PROVIDER_ERROR",
            Self::ConverterError(_) => "CONVERTER_ERROR",
            Self::ConverterUnavailable(_) => "CONVERTER_UNAVAILABLE",
            Self::StorageError(_) => "STORAGE_ERROR",
            Self::Canceled(_) | Self::OperationCancelled => "DOWNLOAD_CANCELED",
            Self::InvalidState(_) => "INVALID_STATE",
            Self::DownloadFailed(details) => details.code.as_str(),
        }
    }

    /// Converts this error to structured `DownloadErrorDetails`.
    #[must_use]
    pub fn to_download_error_details(&self) -> DownloadErrorDetails {
        match self {
            Self::DownloadFailed(details) => details.clone(),
            Self::InvalidUrl(msg) => {
                DownloadErrorDetails::new(DownloadErrorCode::VideoUnavailable, msg, false)
            }
            Self::PlaylistNotSupported(_) => DownloadErrorDetails::new(
                DownloadErrorCode::VideoUnavailable,
                "Les playlists ne sont pas prises en charge.",
                false,
            ),
            Self::Canceled(_) | Self::OperationCancelled => {
                DownloadErrorDetails::from_code(DownloadErrorCode::DownloadCanceled)
            }
            Self::InvalidState(msg) => {
                DownloadErrorDetails::new(DownloadErrorCode::DownloadProcessFailed, msg, false)
            }
            Self::ProviderError(msg) => {
                DownloadErrorDetails::new(DownloadErrorCode::DownloadProcessFailed, msg, true)
            }
            Self::ConverterUnavailable(msg) | Self::ConverterError(msg) => {
                DownloadErrorDetails::new(DownloadErrorCode::FfmpegFailed, msg, false)
            }
            Self::StorageError(msg) => {
                DownloadErrorDetails::new(DownloadErrorCode::OutputPermissionDenied, msg, false)
            }
            _ => DownloadErrorDetails::from_code(DownloadErrorCode::DownloadProcessFailed),
        }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl(msg) => write!(f, "Invalid URL: {msg}"),
            Self::PlaylistNotSupported(msg) => write!(f, "Playlist not supported: {msg}"),
            Self::JobNotFound(id) => write!(f, "Job not found: {id}"),
            Self::IllegalTransition {
                current,
                attempted,
                reason,
            } => write!(
                f,
                "Illegal transition from '{current}' to '{attempted}': {reason}"
            ),
            Self::InvalidProgress { current, attempted } => write!(
                f,
                "Invalid progress: cannot transition from {current}% to {attempted}%"
            ),
            Self::ValidationError(msg) => write!(f, "Validation error: {msg}"),
            Self::InvalidSettings(msg) => write!(f, "Invalid settings: {msg}"),
            Self::ProviderError(msg) => write!(f, "Provider error: {msg}"),
            Self::ConverterError(msg) => write!(f, "Converter error: {msg}"),
            Self::ConverterUnavailable(msg) => write!(f, "Converter unavailable: {msg}"),
            Self::StorageError(msg) => write!(f, "Storage error: {msg}"),
            Self::Canceled(msg) => write!(f, "Operation canceled: {msg}"),
            Self::OperationCancelled => write!(f, "Operation cancelled"),
            Self::InvalidState(msg) => write!(f, "Invalid state: {msg}"),
            Self::DownloadFailed(details) => write!(f, "{details}"),
        }
    }
}

impl std::error::Error for CoreError {}
