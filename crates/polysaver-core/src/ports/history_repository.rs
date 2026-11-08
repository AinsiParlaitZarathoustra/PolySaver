// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::history::{DownloadHistoryEntry, HistoryEntryId};
use crate::error::CoreError;
use async_trait::async_trait;
use std::path::Path;

/// Sovereign port for persistent storage of completed download history.
#[async_trait]
pub trait DownloadHistoryRepository: Send + Sync {
    /// Loads all history entries sorted descending by completion timestamp (newest first).
    async fn load(&self) -> Result<Vec<DownloadHistoryEntry>, CoreError>;

    /// Persists all history entries atomically.
    async fn save(&self, entries: &[DownloadHistoryEntry]) -> Result<(), CoreError>;

    /// Appends or updates a single entry idempotently (newest first).
    async fn append(&self, entry: DownloadHistoryEntry) -> Result<(), CoreError>;

    /// Removes an entry by ID, along with its stored thumbnail.
    async fn remove(&self, id: HistoryEntryId) -> Result<(), CoreError>;

    /// Stores a copy of `image` (JPEG) as the thumbnail of entry `id`.
    ///
    /// Thumbnails are optional decoration: the default implementation keeps none.
    async fn save_thumbnail(&self, _id: HistoryEntryId, _image: &Path) -> Result<(), CoreError> {
        Ok(())
    }

    /// Returns the JPEG thumbnail stored for entry `id`, if any.
    async fn load_thumbnail(&self, _id: HistoryEntryId) -> Option<Vec<u8>> {
        None
    }
}
