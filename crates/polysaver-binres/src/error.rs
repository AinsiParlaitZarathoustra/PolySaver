// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use std::fmt;

/// External sidecar binary kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryKind {
    YtDlp,
    Ffmpeg,
    Ffprobe,
}

impl BinaryKind {
    /// Returns the canonical executable name for this tool.
    #[must_use]
    pub const fn base_name(self) -> &'static str {
        match self {
            Self::YtDlp => "yt-dlp",
            Self::Ffmpeg => "ffmpeg",
            Self::Ffprobe => "ffprobe",
        }
    }

    /// Returns the canonical CLI argument to query tool version.
    #[must_use]
    pub const fn version_flag(self) -> &'static str {
        match self {
            Self::YtDlp => "--version",
            Self::Ffmpeg | Self::Ffprobe => "-version",
        }
    }
}

impl fmt::Display for BinaryKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.base_name())
    }
}

/// Errors originating from binary resolution or version querying.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum BinResError {
    #[error("Binary '{kind}' was not found in any searched locations")]
    NotFound { kind: BinaryKind },

    #[error("Failed to execute probe for binary '{kind}': {error}")]
    ProbeFailed { kind: BinaryKind, error: String },
}
