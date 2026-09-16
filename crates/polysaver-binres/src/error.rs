// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use std::fmt;

/// External sidecar binary kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryKind {
    YtDlp,
    Ffmpeg,
    Ffprobe,
    /// Node.js: JavaScript runtime required by yt-dlp for YouTube challenges.
    Node,
    /// Deno: preferred JavaScript runtime (yt-dlp enables it by default).
    Deno,
}

impl BinaryKind {
    /// Returns the canonical executable name for this tool.
    #[must_use]
    pub const fn base_name(self) -> &'static str {
        match self {
            Self::YtDlp => "yt-dlp",
            Self::Ffmpeg => "ffmpeg",
            Self::Ffprobe => "ffprobe",
            Self::Node => "node",
            Self::Deno => "deno",
        }
    }

    /// Returns the canonical CLI argument to query tool version.
    #[must_use]
    pub const fn version_flag(self) -> &'static str {
        match self {
            Self::YtDlp | Self::Node | Self::Deno => "--version",
            Self::Ffmpeg | Self::Ffprobe => "-version",
        }
    }

    /// Whether this kind is resolved from the app-managed directory first.
    ///
    /// `YtDlp`, `Node` and `Deno` can be installed/updated at runtime into
    /// `app_data/bin`, so that copy must win over any bundled resource.
    #[must_use]
    pub const fn prefers_app_bin_dir(self) -> bool {
        matches!(self, Self::YtDlp | Self::Node | Self::Deno)
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

    /// The candidate could not be started at all. Distinct from [`Self::ProbeFailed`]
    /// because a spawn failure can be caused by momentary OS exhaustion
    /// (`fork: Resource temporarily unavailable`) rather than by the binary itself,
    /// and is therefore worth a bounded retry.
    #[error("Failed to spawn binary '{kind}': {error}")]
    SpawnFailed { kind: BinaryKind, error: String },
}
