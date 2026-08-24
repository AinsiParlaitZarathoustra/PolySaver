// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::format::VideoQuality;
use crate::domain::media_url::MediaUrl;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Canonical metadata about an analyzed media format option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatOption {
    pub format_id: String,
    pub height: Option<u32>,
    pub has_video: bool,
    pub has_audio: bool,
    pub extension: String,
    pub filesize_approx_bytes: Option<u64>,
    pub note: Option<String>,
}

/// Canonical result of probing a media URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub url: MediaUrl,
    pub title: String,
    pub duration_seconds: Option<u64>,
    pub thumbnail_url: Option<String>,
    pub uploader: Option<String>,
    pub formats: Vec<FormatOption>,
    pub available_video_qualities: Vec<VideoQuality>,
}

impl ProbeResult {
    /// Creates a new ProbeResult and derives the list of available downloadable video qualities.
    #[must_use]
    pub fn new(
        url: MediaUrl,
        title: String,
        duration_seconds: Option<u64>,
        thumbnail_url: Option<String>,
        uploader: Option<String>,
        formats: Vec<FormatOption>,
    ) -> Self {
        let available_video_qualities = Self::compute_available_video_qualities(&formats);
        Self {
            url,
            title,
            duration_seconds,
            thumbnail_url,
            uploader,
            formats,
            available_video_qualities,
        }
    }

    /// Computes the deduplicated list of available downloadable video qualities,
    /// sorted from highest to lowest resolution.
    #[must_use]
    pub fn compute_available_video_qualities(formats: &[FormatOption]) -> Vec<VideoQuality> {
        let mut heights = BTreeSet::new();
        for f in formats {
            if f.has_video {
                if let Some(h) = f.height {
                    if h > 0 {
                        heights.insert(h);
                    }
                }
            }
        }

        let mut qualities = Vec::new();
        for h in heights.into_iter().rev() {
            if let Some(q) = VideoQuality::from_height(h) {
                if !qualities.contains(&q) {
                    qualities.push(q);
                }
            }
        }
        qualities
    }

    /// Returns true if at least one downloadable video stream was detected.
    #[must_use]
    pub fn has_video_stream(&self) -> bool {
        self.formats.iter().any(|f| f.has_video)
    }
}
