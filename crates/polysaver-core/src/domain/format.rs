// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::error::CoreError;
use serde::{Deserialize, Serialize};

/// Supported media output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Mp4,
    Mov,
    Mp3,
    Flac,
}

impl OutputFormat {
    /// Returns true if this format is a video container.
    #[must_use]
    pub const fn is_video(self) -> bool {
        matches!(self, Self::Mp4 | Self::Mov)
    }

    /// Returns true if this format is an audio format.
    #[must_use]
    pub const fn is_audio(self) -> bool {
        matches!(self, Self::Mp3 | Self::Flac)
    }

    /// Standard file extension for this format.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
            Self::Mp3 => "mp3",
            Self::Flac => "flac",
        }
    }
}

/// Target video quality / resolution for video downloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VideoQuality {
    P144,
    P240,
    P360,
    P480,
    P720,
    P1080,
    P1440,
    P2160,
    Best,
}

impl VideoQuality {
    /// Maps a vertical height in pixels to the corresponding VideoQuality enum variant.
    #[must_use]
    pub const fn from_height(height: u32) -> Option<Self> {
        match height {
            144 => Some(Self::P144),
            240 => Some(Self::P240),
            360 => Some(Self::P360),
            480 => Some(Self::P480),
            720 => Some(Self::P720),
            1080 => Some(Self::P1080),
            1440 => Some(Self::P1440),
            2160 => Some(Self::P2160),
            _ => None,
        }
    }

    /// Target vertical height in pixels, or None for best available.
    #[must_use]
    pub const fn target_height(self) -> Option<u32> {
        match self {
            Self::P144 => Some(144),
            Self::P240 => Some(240),
            Self::P360 => Some(360),
            Self::P480 => Some(480),
            Self::P720 => Some(720),
            Self::P1080 => Some(1080),
            Self::P1440 => Some(1440),
            Self::P2160 => Some(2160),
            Self::Best => None,
        }
    }

    /// Human-readable label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::P144 => "144p",
            Self::P240 => "240p",
            Self::P360 => "360p",
            Self::P480 => "480p · SD",
            Self::P720 => "720p · HD",
            Self::P1080 => "1080p · Full HD",
            Self::P1440 => "1440p · 2K",
            Self::P2160 => "2160p · 4K",
            Self::Best => "best",
        }
    }
}

/// Target bitrate for MP3 audio encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mp3Quality {
    K128,
    K192,
    K256,
    K320,
}

impl Mp3Quality {
    /// Bitrate in kilobits per second.
    #[must_use]
    pub const fn bitrate_kbps(self) -> u32 {
        match self {
            Self::K128 => 128,
            Self::K192 => 192,
            Self::K256 => 256,
            Self::K320 => 320,
        }
    }

    /// Human-readable label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::K128 => "128 kb/s",
            Self::K192 => "192 kb/s",
            Self::K256 => "256 kb/s",
            Self::K320 => "320 kb/s",
        }
    }
}

/// UI Theme mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    System,
}

/// Supported application languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    #[serde(rename = "fr")]
    French,
    #[serde(rename = "en")]
    English,
}

impl Language {
    /// Returns the ISO 639-1 language code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::French => "fr",
            Self::English => "en",
        }
    }
}

/// Canonical download preset enforcing valid combinations of format and quality.
///
/// Invariants:
/// - MP4/MOV must specify a `VideoQuality`.
/// - MP3 must specify an `Mp3Quality`.
/// - FLAC has no quality selector (lossless transcode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DownloadPreset {
    Video {
        format: OutputFormat,
        quality: VideoQuality,
    },
    Mp3 {
        quality: Mp3Quality,
    },
    Flac,
}

impl DownloadPreset {
    /// Creates a video preset (MP4 or MOV).
    pub fn video(format: OutputFormat, quality: VideoQuality) -> Result<Self, CoreError> {
        if !format.is_video() {
            return Err(CoreError::InvalidSettings(format!(
                "Format '{:?}' is not a valid video format for video preset",
                format
            )));
        }
        Ok(Self::Video { format, quality })
    }

    /// Creates an MP3 preset.
    #[must_use]
    pub const fn mp3(quality: Mp3Quality) -> Self {
        Self::Mp3 { quality }
    }

    /// Creates a FLAC preset.
    #[must_use]
    pub const fn flac() -> Self {
        Self::Flac
    }

    /// Returns the target output format.
    #[must_use]
    pub const fn format(&self) -> OutputFormat {
        match self {
            Self::Video { format, .. } => *format,
            Self::Mp3 { .. } => OutputFormat::Mp3,
            Self::Flac => OutputFormat::Flac,
        }
    }

    /// Returns the video quality if applicable.
    #[must_use]
    pub const fn video_quality(&self) -> Option<VideoQuality> {
        match self {
            Self::Video { quality, .. } => Some(*quality),
            Self::Mp3 { .. } | Self::Flac => None,
        }
    }

    /// Returns the MP3 quality if applicable.
    #[must_use]
    pub const fn mp3_quality(&self) -> Option<Mp3Quality> {
        match self {
            Self::Mp3 { quality } => Some(*quality),
            Self::Video { .. } | Self::Flac => None,
        }
    }
}

impl Default for DownloadPreset {
    fn default() -> Self {
        Self::Video {
            format: OutputFormat::Mp4,
            quality: VideoQuality::P1080,
        }
    }
}

/// DTO for untrusted preset deserialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadPresetDto {
    pub format: OutputFormat,
    pub video_quality: Option<VideoQuality>,
    pub mp3_quality: Option<Mp3Quality>,
}

impl From<&DownloadPreset> for DownloadPresetDto {
    fn from(preset: &DownloadPreset) -> Self {
        match preset {
            DownloadPreset::Video { format, quality } => Self {
                format: *format,
                video_quality: Some(*quality),
                mp3_quality: None,
            },
            DownloadPreset::Mp3 { quality } => Self {
                format: OutputFormat::Mp3,
                video_quality: None,
                mp3_quality: Some(*quality),
            },
            DownloadPreset::Flac => Self {
                format: OutputFormat::Flac,
                video_quality: None,
                mp3_quality: None,
            },
        }
    }
}

impl TryFrom<DownloadPresetDto> for DownloadPreset {
    type Error = CoreError;

    fn try_from(dto: DownloadPresetDto) -> Result<Self, Self::Error> {
        match dto.format {
            OutputFormat::Mp4 | OutputFormat::Mov => {
                if dto.mp3_quality.is_some() {
                    return Err(CoreError::InvalidSettings(
                        "Video formats (MP4/MOV) cannot have an MP3 quality specified".to_string(),
                    ));
                }
                let quality = dto.video_quality.unwrap_or(VideoQuality::Best);
                Self::video(dto.format, quality)
            }
            OutputFormat::Mp3 => {
                if dto.video_quality.is_some() {
                    return Err(CoreError::InvalidSettings(
                        "MP3 audio format cannot have a video quality specified".to_string(),
                    ));
                }
                let quality = dto.mp3_quality.ok_or_else(|| {
                    CoreError::InvalidSettings(
                        "MP3 format requires an MP3 quality (bitrate)".to_string(),
                    )
                })?;
                Ok(Self::mp3(quality))
            }
            OutputFormat::Flac => {
                if dto.video_quality.is_some() || dto.mp3_quality.is_some() {
                    return Err(CoreError::InvalidSettings(
                        "FLAC format does not accept video or MP3 quality parameters".to_string(),
                    ));
                }
                Ok(Self::flac())
            }
        }
    }
}
