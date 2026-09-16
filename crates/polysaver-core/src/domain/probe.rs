// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::format::VideoQuality;
use crate::domain::media_url::MediaUrl;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Canonical metadata about an analyzed media format option.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatOption {
    pub format_id: String,
    pub height: Option<u32>,
    pub has_video: bool,
    pub has_audio: bool,
    pub extension: String,
    pub filesize_approx_bytes: Option<u64>,
    /// Average total bitrate in kbps, when reported by the provider.
    /// Used as a size fallback (`duration * tbr`) for HLS/DASH formats that
    /// cannot expose `filesize_approx_bytes`.
    pub tbr: Option<f64>,
    pub note: Option<String>,
}

/// Maximum number of playlist entries enumerated for the selection list
/// (`--playlist-end`). Channels can hold tens of thousands of videos, so the
/// listing is always bounded.
pub const PLAYLIST_ENTRIES_LIMIT: usize = 200;

/// Maximum number of entries accepted in a single playlist download request.
pub const MAX_PLAYLIST_DOWNLOADS: usize = 200;

/// Kind of media resolved by an analysis: a single item or a playlist/listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    /// One video or audio item.
    Single,
    /// A playlist, channel or other multi-video listing.
    Playlist,
}

/// One entry of a playlist, as listed by a flat (non-extracted) enumeration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistEntry {
    /// 1-based position in the playlist.
    pub index: u32,
    pub url: MediaUrl,
    pub title: String,
    pub duration_seconds: Option<u64>,
    pub thumbnail_url: Option<String>,
    /// False for placeholders such as `[Private video]` that cannot be downloaded.
    pub available: bool,
}

/// Canonical result of probing a media URL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub url: MediaUrl,
    pub title: String,
    pub duration_seconds: Option<u64>,
    pub thumbnail_url: Option<String>,
    pub uploader: Option<String>,
    pub formats: Vec<FormatOption>,
    pub available_video_qualities: Vec<VideoQuality>,
    /// Whether this result describes a single item or a playlist.
    pub kind: MediaKind,
    /// Entries of a playlist; always empty for a single item.
    pub entries: Vec<PlaylistEntry>,
    /// Total number of items reported by the provider, when known.
    /// Larger than `entries.len()` when the enumeration was capped.
    pub playlist_total: Option<u64>,
}

impl ProbeResult {
    /// Creates a single-item probe result and derives the downloadable video qualities.
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
            kind: MediaKind::Single,
            entries: Vec::new(),
            playlist_total: None,
        }
    }

    /// Creates a playlist probe result.
    ///
    /// A flat enumeration exposes no per-video format information, so `formats` stays
    /// empty and the UI falls back to the static quality list.
    #[must_use]
    pub fn new_playlist(
        url: MediaUrl,
        title: String,
        thumbnail_url: Option<String>,
        uploader: Option<String>,
        entries: Vec<PlaylistEntry>,
        playlist_total: Option<u64>,
    ) -> Self {
        Self {
            url,
            title,
            duration_seconds: None,
            thumbnail_url,
            uploader,
            formats: Vec::new(),
            available_video_qualities: Vec::new(),
            kind: MediaKind::Playlist,
            entries,
            playlist_total,
        }
    }

    /// Returns the entries that can actually be downloaded.
    #[must_use]
    pub fn available_entries(&self) -> Vec<&PlaylistEntry> {
        self.entries.iter().filter(|e| e.available).collect()
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
