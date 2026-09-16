// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use super::download_job::DownloadId;
use super::format::DownloadPreset;
use super::media_url::MediaUrl;
use crate::error::CoreError;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Unique strongly-typed identifier for a history entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HistoryEntryId(Uuid);

impl HistoryEntryId {
    /// Generates a new random history entry ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Converts the ID to a string slice representation.
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for HistoryEntryId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for HistoryEntryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<&str> for HistoryEntryId {
    type Error = CoreError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(CoreError::ValidationError(
                "History entry ID cannot be empty".to_string(),
            ));
        }

        Uuid::parse_str(trimmed).map(Self).map_err(|_| {
            CoreError::ValidationError(format!("Invalid UUID for HistoryEntryId: '{value}'"))
        })
    }
}

/// Immutable, canonical model representing a completed download history entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadHistoryEntry {
    id: HistoryEntryId,
    download_id: DownloadId,
    source_url: MediaUrl,
    title: String,
    preset: DownloadPreset,
    destination_path: String,
    completed_at: u64,
    /// Download directory this entry was written into.
    /// Optional for backward compatibility with entries created before the field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    download_dir: Option<String>,
}

/// Maximum accepted length for a persisted destination path.
const MAX_DESTINATION_PATH_BYTES: usize = 4096;

/// Validates a destination path from untrusted input.
///
/// Rules: non-empty, absolute, no null bytes, no `..` traversal component,
/// bounded length.
fn validate_destination_path(raw: &str) -> Result<String, CoreError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CoreError::ValidationError(
            "Destination path cannot be empty in history entry".to_string(),
        ));
    }
    if trimmed.contains('\0') {
        return Err(CoreError::ValidationError(
            "Destination path cannot contain null bytes".to_string(),
        ));
    }
    if trimmed.len() > MAX_DESTINATION_PATH_BYTES {
        return Err(CoreError::ValidationError(
            "Destination path is too long".to_string(),
        ));
    }
    let path = std::path::Path::new(trimmed);
    if !path.is_absolute() {
        return Err(CoreError::ValidationError(
            "Destination path must be absolute".to_string(),
        ));
    }
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(CoreError::ValidationError(
            "Destination path cannot contain '..' components".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

impl DownloadHistoryEntry {
    /// Creates a new validated history entry.
    pub fn new(
        download_id: DownloadId,
        source_url: MediaUrl,
        title: String,
        preset: DownloadPreset,
        destination_path: String,
        completed_at: Option<u64>,
    ) -> Result<Self, CoreError> {
        Self::build(
            HistoryEntryId::new(),
            download_id,
            source_url,
            title,
            preset,
            destination_path,
            completed_at,
            None,
        )
    }

    /// Creates a new validated history entry that remembers its download directory.
    pub fn new_with_dir(
        download_id: DownloadId,
        source_url: MediaUrl,
        title: String,
        preset: DownloadPreset,
        destination_path: String,
        completed_at: Option<u64>,
        download_dir: String,
    ) -> Result<Self, CoreError> {
        Self::build(
            HistoryEntryId::new(),
            download_id,
            source_url,
            title,
            preset,
            destination_path,
            completed_at,
            Some(download_dir),
        )
    }

    /// Reconstructs a history entry with an existing ID (used by repository deserializers).
    pub fn reconstruct(
        id: HistoryEntryId,
        download_id: DownloadId,
        source_url: MediaUrl,
        title: String,
        preset: DownloadPreset,
        destination_path: String,
        completed_at: u64,
    ) -> Result<Self, CoreError> {
        Self::build(
            id,
            download_id,
            source_url,
            title,
            preset,
            destination_path,
            Some(completed_at),
            None,
        )
    }

    /// Reconstructs a history entry including its recorded download directory.
    #[allow(clippy::too_many_arguments)]
    pub fn reconstruct_with_dir(
        id: HistoryEntryId,
        download_id: DownloadId,
        source_url: MediaUrl,
        title: String,
        preset: DownloadPreset,
        destination_path: String,
        completed_at: u64,
        download_dir: Option<String>,
    ) -> Result<Self, CoreError> {
        Self::build(
            id,
            download_id,
            source_url,
            title,
            preset,
            destination_path,
            Some(completed_at),
            download_dir,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        id: HistoryEntryId,
        download_id: DownloadId,
        source_url: MediaUrl,
        title: String,
        preset: DownloadPreset,
        destination_path: String,
        completed_at: Option<u64>,
        download_dir: Option<String>,
    ) -> Result<Self, CoreError> {
        let sanitized_title = title.trim();
        let final_title = if sanitized_title.is_empty() {
            "PolySaver_Media".to_string()
        } else {
            sanitized_title.to_string()
        };

        let validated_dest = validate_destination_path(&destination_path)?;

        let timestamp = match completed_at {
            Some(ts) => ts,
            None => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        };

        let validated_dir = match download_dir {
            Some(dir) => Some(validate_destination_path(&dir)?),
            None => None,
        };

        Ok(Self {
            id,
            download_id,
            source_url,
            title: final_title,
            preset,
            destination_path: validated_dest,
            completed_at: timestamp,
            download_dir: validated_dir,
        })
    }

    /// Returns the unique entry ID.
    pub fn id(&self) -> HistoryEntryId {
        self.id
    }

    /// Returns the associated download ID.
    pub fn download_id(&self) -> DownloadId {
        self.download_id
    }

    /// Returns the source URL.
    pub fn source_url(&self) -> &MediaUrl {
        &self.source_url
    }

    /// Returns the title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the download preset.
    pub fn preset(&self) -> DownloadPreset {
        self.preset
    }

    /// Returns the destination path.
    pub fn destination_path(&self) -> &str {
        &self.destination_path
    }

    /// Returns the completion timestamp in unix milliseconds.
    pub fn completed_at(&self) -> u64 {
        self.completed_at
    }

    /// Returns the recorded download directory, when known.
    pub fn download_dir(&self) -> Option<&str> {
        self.download_dir.as_deref()
    }
}
