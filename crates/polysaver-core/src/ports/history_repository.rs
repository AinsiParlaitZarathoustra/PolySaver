// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::history::{DownloadHistoryEntry, HistoryEntryId};
use crate::error::CoreError;
use async_trait::async_trait;

/// Sovereign port for persistent storage of completed download history.
#[async_trait]
pub trait DownloadHistoryRepository: Send + Sync {
    /// Loads all history entries sorted descending by completion timestamp (newest first).
    async fn load(&self) -> Result<Vec<DownloadHistoryEntry>, CoreError>;

    /// Persists all history entries atomically.
    async fn save(&self, entries: &[DownloadHistoryEntry]) -> Result<(), CoreError>;

    /// Appends or updates a single entry idempotently (newest first).
    async fn append(&self, entry: DownloadHistoryEntry) -> Result<(), CoreError>;

    /// Removes an entry by ID.
    async fn remove(&self, id: HistoryEntryId) -> Result<(), CoreError>;
}
