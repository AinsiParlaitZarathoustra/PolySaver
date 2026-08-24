// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use polysaver_core::domain::format::DownloadPresetDto;
use polysaver_core::domain::history::DownloadHistoryEntry;
use serde::{Deserialize, Serialize};

/// Serializable DTO for download history entries exposed via IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadHistoryEntryDto {
    pub id: String,
    pub download_id: String,
    pub source_url: String,
    pub title: String,
    pub preset: DownloadPresetDto,
    pub destination_path: String,
    pub completed_at: u64,
}

impl From<&DownloadHistoryEntry> for DownloadHistoryEntryDto {
    fn from(entry: &DownloadHistoryEntry) -> Self {
        Self {
            id: entry.id().as_str(),
            download_id: entry.download_id().to_string(),
            source_url: entry.source_url().as_str().to_string(),
            title: entry.title().to_string(),
            preset: DownloadPresetDto::from(&entry.preset()),
            destination_path: entry.destination_path().to_string(),
            completed_at: entry.completed_at(),
        }
    }
}
