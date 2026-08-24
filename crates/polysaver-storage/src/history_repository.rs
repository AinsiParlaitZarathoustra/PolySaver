// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use async_trait::async_trait;
use polysaver_core::domain::download_job::DownloadId;
use polysaver_core::domain::format::{DownloadPreset, DownloadPresetDto};
use polysaver_core::domain::history::{DownloadHistoryEntry, HistoryEntryId};
use polysaver_core::domain::media_url::MediaUrl;
use polysaver_core::error::CoreError;
use polysaver_core::ports::history_repository::DownloadHistoryRepository;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use uuid::Uuid;

const HISTORY_SCHEMA_VERSION: u32 = 1;
const NDJSON_HISTORY_FILE_NAME: &str = "download_history.ndjson";
const LEGACY_JSON_HISTORY_FILE_NAME: &str = "download_history.json";
const MAX_JOURNAL_SIZE_BYTES: u64 = 4 * 1024 * 1024; // 4 MB

/// DTO for individual history entry JSON serialization.
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

impl TryFrom<DownloadHistoryEntryDto> for DownloadHistoryEntry {
    type Error = CoreError;

    fn try_from(dto: DownloadHistoryEntryDto) -> Result<Self, Self::Error> {
        let id = HistoryEntryId::try_from(dto.id.as_str())?;
        let download_id = DownloadId::try_from(dto.download_id.as_str())?;
        let source_url = MediaUrl::parse(&dto.source_url)?;
        let preset = DownloadPreset::try_from(dto.preset)?;

        DownloadHistoryEntry::reconstruct(
            id,
            download_id,
            source_url,
            dto.title,
            preset,
            dto.destination_path,
            dto.completed_at,
        )
    }
}

impl From<&DownloadHistoryEntry> for DownloadHistoryEntryDto {
    fn from(entry: &DownloadHistoryEntry) -> Self {
        Self {
            id: entry.id().as_str().to_string(),
            download_id: entry.download_id().to_string(),
            source_url: entry.source_url().as_str().to_string(),
            title: entry.title().to_string(),
            preset: DownloadPresetDto::from(&entry.preset()),
            destination_path: entry.destination_path().to_string(),
            completed_at: entry.completed_at(),
        }
    }
}

/// DTO for legacy JSON history document migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyDownloadHistoryDocumentDto {
    pub version: u32,
    pub entries: Vec<DownloadHistoryEntryDto>,
}

/// Journal operation line written to the NDJSON append-only log.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum HistoryJournalOp {
    Upsert {
        v: u32,
        entry: DownloadHistoryEntryDto,
    },
    Remove {
        v: u32,
        id: String,
    },
}

/// Crash-resilient, append-only NDJSON repository with automatic compaction and legacy migration.
#[derive(Debug, Clone)]
pub struct JsonDownloadHistoryRepository {
    config_dir: PathBuf,
    ndjson_file: PathBuf,
    legacy_file: PathBuf,
    write_lock: Arc<Mutex<()>>,
}

impl JsonDownloadHistoryRepository {
    /// Creates a repository rooted in the given configuration directory.
    #[must_use]
    pub fn new(config_dir: impl AsRef<Path>) -> Self {
        let dir = config_dir.as_ref().to_path_buf();
        let ndjson_file = dir.join(NDJSON_HISTORY_FILE_NAME);
        let legacy_file = dir.join(LEGACY_JSON_HISTORY_FILE_NAME);
        Self {
            config_dir: dir,
            ndjson_file,
            legacy_file,
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Returns the canonical path to the NDJSON history file.
    #[must_use]
    pub fn history_file(&self) -> &Path {
        &self.ndjson_file
    }

    /// Migrates legacy JSON history file to NDJSON format if present.
    async fn migrate_legacy_if_needed(&self) -> Result<(), CoreError> {
        if !self.legacy_file.exists() || self.ndjson_file.exists() {
            return Ok(());
        }

        if let Ok(raw_bytes) = tokio::fs::read(&self.legacy_file).await {
            if !raw_bytes.is_empty() {
                if let Ok(doc) =
                    serde_json::from_slice::<LegacyDownloadHistoryDocumentDto>(&raw_bytes)
                {
                    let mut lines = Vec::new();
                    for entry_dto in doc.entries {
                        let op = HistoryJournalOp::Upsert {
                            v: HISTORY_SCHEMA_VERSION,
                            entry: entry_dto,
                        };
                        if let Ok(json_line) = serde_json::to_string(&op) {
                            lines.push(json_line);
                        }
                    }
                    if !lines.is_empty() {
                        let content = format!("{}\n", lines.join("\n"));
                        let _ = tokio::fs::write(&self.ndjson_file, content.as_bytes()).await;
                    }
                }
            }
        }

        // Rename old file to avoid re-migration
        let migrated_dest = self
            .config_dir
            .join(format!("{LEGACY_JSON_HISTORY_FILE_NAME}.migrated"));
        let _ = tokio::fs::rename(&self.legacy_file, migrated_dest).await;
        Ok(())
    }

    /// Loads history entries from the append-only NDJSON journal, replaying operations.
    async fn load_internal(&self) -> Result<(Vec<DownloadHistoryEntry>, usize, u64), CoreError> {
        self.migrate_legacy_if_needed().await?;

        if !self.ndjson_file.exists() {
            return Ok((Vec::new(), 0, 0));
        }

        let raw_bytes = tokio::fs::read(&self.ndjson_file).await.map_err(|err| {
            CoreError::StorageError(format!(
                "Failed to read download history journal '{}': {err}",
                self.ndjson_file.display()
            ))
        })?;

        let file_size = raw_bytes.len() as u64;
        if raw_bytes.is_empty() {
            return Ok((Vec::new(), 0, 0));
        }

        let content = String::from_utf8_lossy(&raw_bytes);
        let mut live_map: HashMap<String, DownloadHistoryEntry> = HashMap::new();
        let mut op_count: usize = 0;
        let mut rejected_lines: Vec<String> = Vec::new();

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        for (idx, raw_line) in lines.iter().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }

            op_count += 1;
            match serde_json::from_str::<HistoryJournalOp>(line) {
                Ok(op) => match op {
                    HistoryJournalOp::Upsert { entry, .. } => {
                        match DownloadHistoryEntry::try_from(entry) {
                            Ok(validated) => {
                                // De-duplicate by both entry id and download_id
                                live_map.retain(|_, v| v.download_id() != validated.download_id());
                                live_map.insert(validated.id().as_str().to_string(), validated);
                            }
                            Err(val_err) => {
                                eprintln!(
                                    "[PolySaver History] Invalid entry line {}: {val_err}",
                                    idx + 1
                                );
                                rejected_lines.push(line.to_string());
                            }
                        }
                    }
                    HistoryJournalOp::Remove { id, .. } => {
                        live_map.remove(&id);
                    }
                },
                Err(parse_err) => {
                    // If this is the last line in the file, it could be an incomplete write from a sudden crash/power loss
                    if idx + 1 == total_lines {
                        eprintln!("[PolySaver History] Ignoring truncated trailing journal line: {parse_err}");
                    } else {
                        eprintln!(
                            "[PolySaver History] Corrupted journal line {}: {parse_err}",
                            idx + 1
                        );
                        rejected_lines.push(line.to_string());
                    }
                }
            }
        }

        // Export rejected lines if any complete line was corrupted
        if !rejected_lines.is_empty() {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let rejected_backup = self
                .config_dir
                .join(format!("{NDJSON_HISTORY_FILE_NAME}.rejected_{timestamp}"));
            let _ = tokio::fs::write(&rejected_backup, rejected_lines.join("\n").as_bytes()).await;
        }

        let mut entries: Vec<DownloadHistoryEntry> = live_map.into_values().collect();
        entries.sort_by_key(|b| std::cmp::Reverse(b.completed_at()));

        Ok((entries, op_count, file_size))
    }

    /// Rewrites the NDJSON journal with only live entries atomically.
    async fn compact_internal(
        &self,
        live_entries: &[DownloadHistoryEntry],
    ) -> Result<(), CoreError> {
        tokio::fs::create_dir_all(&self.config_dir)
            .await
            .map_err(|err| {
                CoreError::StorageError(format!(
                    "Failed to create history directory '{}': {err}",
                    self.config_dir.display()
                ))
            })?;

        let mut lines = Vec::with_capacity(live_entries.len());
        for entry in live_entries {
            let op = HistoryJournalOp::Upsert {
                v: HISTORY_SCHEMA_VERSION,
                entry: DownloadHistoryEntryDto::from(entry),
            };
            if let Ok(json_line) = serde_json::to_string(&op) {
                lines.push(json_line);
            }
        }

        let payload = if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        };

        let tmp_file = self.config_dir.join(format!(
            ".{NDJSON_HISTORY_FILE_NAME}_{}.tmp",
            Uuid::new_v4()
        ));

        tokio::fs::write(&tmp_file, payload.as_bytes())
            .await
            .map_err(|err| {
                CoreError::StorageError(format!(
                    "Failed to write compacted history file '{}': {err}",
                    tmp_file.display()
                ))
            })?;

        tokio::fs::rename(&tmp_file, &self.ndjson_file)
            .await
            .map_err(|err| {
                let _ = std::fs::remove_file(&tmp_file);
                CoreError::StorageError(format!(
                    "Failed to atomically rename compacted history file: {err}"
                ))
            })?;

        Ok(())
    }

    /// Appends a single operation line to the NDJSON journal under write lock.
    async fn append_op(&self, op: &HistoryJournalOp) -> Result<(), CoreError> {
        tokio::fs::create_dir_all(&self.config_dir)
            .await
            .map_err(|err| {
                CoreError::StorageError(format!(
                    "Failed to create history directory '{}': {err}",
                    self.config_dir.display()
                ))
            })?;

        let line = serde_json::to_string(op).map_err(|err| {
            CoreError::StorageError(format!("Failed to serialize journal op: {err}"))
        })?;

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.ndjson_file)
            .await
            .map_err(|err| {
                CoreError::StorageError(format!(
                    "Failed to open history journal '{}' for append: {err}",
                    self.ndjson_file.display()
                ))
            })?;

        file.write_all(format!("{line}\n").as_bytes())
            .await
            .map_err(|err| {
                CoreError::StorageError(format!("Failed to append to history journal: {err}"))
            })?;

        file.flush().await.map_err(|err| {
            CoreError::StorageError(format!("Failed to flush history journal: {err}"))
        })?;

        Ok(())
    }
}

#[async_trait]
impl DownloadHistoryRepository for JsonDownloadHistoryRepository {
    async fn load(&self) -> Result<Vec<DownloadHistoryEntry>, CoreError> {
        let _lock = self.write_lock.lock().await;
        let (entries, op_count, file_size) = self.load_internal().await?;

        // Compaction threshold: file size > 4 MB or total ops > 2 * live_entries + 100
        let should_compact = file_size > MAX_JOURNAL_SIZE_BYTES
            || (op_count > entries.len() * 2 + 100 && op_count > 100);

        if should_compact {
            let _ = self.compact_internal(&entries).await;
        }

        Ok(entries)
    }

    async fn save(&self, entries: &[DownloadHistoryEntry]) -> Result<(), CoreError> {
        let _lock = self.write_lock.lock().await;
        self.compact_internal(entries).await
    }

    async fn append(&self, entry: DownloadHistoryEntry) -> Result<(), CoreError> {
        let _lock = self.write_lock.lock().await;
        let op = HistoryJournalOp::Upsert {
            v: HISTORY_SCHEMA_VERSION,
            entry: DownloadHistoryEntryDto::from(&entry),
        };
        self.append_op(&op).await
    }

    async fn remove(&self, id: HistoryEntryId) -> Result<(), CoreError> {
        let _lock = self.write_lock.lock().await;
        let op = HistoryJournalOp::Remove {
            v: HISTORY_SCHEMA_VERSION,
            id: id.to_string(),
        };
        self.append_op(&op).await
    }
}

/// In-memory download history repository for testing.
#[derive(Debug, Clone, Default)]
pub struct InMemoryDownloadHistoryRepository {
    entries: std::sync::Arc<tokio::sync::RwLock<Vec<DownloadHistoryEntry>>>,
}

impl InMemoryDownloadHistoryRepository {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl DownloadHistoryRepository for InMemoryDownloadHistoryRepository {
    async fn load(&self) -> Result<Vec<DownloadHistoryEntry>, CoreError> {
        let lock = self.entries.read().await;
        let mut entries = lock.clone();
        entries.sort_by_key(|b| std::cmp::Reverse(b.completed_at()));
        Ok(entries)
    }

    async fn save(&self, entries: &[DownloadHistoryEntry]) -> Result<(), CoreError> {
        let mut lock = self.entries.write().await;
        let mut sorted = entries.to_vec();
        sorted.sort_by_key(|b| std::cmp::Reverse(b.completed_at()));
        *lock = sorted;
        Ok(())
    }

    async fn append(&self, entry: DownloadHistoryEntry) -> Result<(), CoreError> {
        let mut lock = self.entries.write().await;
        lock.retain(|e| e.download_id() != entry.download_id() && e.id() != entry.id());
        lock.insert(0, entry);
        Ok(())
    }

    async fn remove(&self, id: HistoryEntryId) -> Result<(), CoreError> {
        let mut lock = self.entries.write().await;
        lock.retain(|e| e.id() != id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polysaver_core::domain::format::{OutputFormat, VideoQuality};

    #[tokio::test]
    async fn test_ndjson_history_append_load_remove_compaction() {
        let temp_dir = std::env::temp_dir().join(format!("ndjson_test_{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();

        let repo = JsonDownloadHistoryRepository::new(&temp_dir);

        let url = MediaUrl::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
        let preset = DownloadPreset::video(OutputFormat::Mp4, VideoQuality::P1080).unwrap();

        let entry1 = DownloadHistoryEntry::new(
            DownloadId::new(),
            url.clone(),
            "Video 1".to_string(),
            preset,
            "/path/to/video1.mp4".to_string(),
            Some(1000),
        )
        .unwrap();

        let entry2 = DownloadHistoryEntry::new(
            DownloadId::new(),
            url.clone(),
            "Video 2".to_string(),
            preset,
            "/path/to/video2.mp4".to_string(),
            Some(2000),
        )
        .unwrap();

        // 1. Append operations
        repo.append(entry1.clone()).await.unwrap();
        repo.append(entry2.clone()).await.unwrap();

        // 2. Load returns both entries sorted newest first (entry2, then entry1)
        let loaded = repo.load().await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id(), entry2.id());
        assert_eq!(loaded[1].id(), entry1.id());

        // 3. Remove entry1
        repo.remove(entry1.id()).await.unwrap();

        let loaded_after_remove = repo.load().await.unwrap();
        assert_eq!(loaded_after_remove.len(), 1);
        assert_eq!(loaded_after_remove[0].id(), entry2.id());

        // 4. Force compaction
        repo.save(&loaded_after_remove).await.unwrap();
        let compacted = repo.load().await.unwrap();
        assert_eq!(compacted.len(), 1);
        assert_eq!(compacted[0].id(), entry2.id());

        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test]
    async fn test_ndjson_truncated_trailing_line_resilience() {
        let temp_dir = std::env::temp_dir().join(format!("ndjson_trunc_test_{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();

        let repo = JsonDownloadHistoryRepository::new(&temp_dir);

        let url = MediaUrl::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
        let preset = DownloadPreset::video(OutputFormat::Mp4, VideoQuality::P720).unwrap();

        let entry = DownloadHistoryEntry::new(
            DownloadId::new(),
            url,
            "Valid Entry".to_string(),
            preset,
            "/path/to/valid.mp4".to_string(),
            Some(5000),
        )
        .unwrap();

        repo.append(entry.clone()).await.unwrap();

        // Simulate abrupt process kill with truncated JSON line at the end
        let mut file = tokio::fs::OpenOptions::new()
            .append(true)
            .open(repo.history_file())
            .await
            .unwrap();
        file.write_all(b"{\"op\":\"upsert\",\"v\":1,\"entry\":{\"id\":\"incomplete\n")
            .await
            .unwrap();
        file.flush().await.unwrap();
        drop(file);

        // Load must succeed, reading the valid entry and safely ignoring the incomplete trailing line
        let loaded = repo.load().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id(), entry.id());

        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test]
    async fn test_legacy_json_migration() {
        let temp_dir = std::env::temp_dir().join(format!("legacy_migr_{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();

        let legacy_file = temp_dir.join(LEGACY_JSON_HISTORY_FILE_NAME);
        let legacy_json = r#"{
            "version": 1,
            "entries": [
                {
                    "id": "e81792bc-7095-46ff-b4e8-db2bb82a7a40",
                    "downloadId": "91a6136d-1bf9-4700-be4c-f0505c210d65",
                    "sourceUrl": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
                    "title": "Legacy Video",
                    "preset": {
                        "format": "mp4",
                        "videoQuality": "p1080"
                    },
                    "destinationPath": "/tmp/legacy.mp4",
                    "completedAt": 1700000000
                }
            ]
        }"#;
        tokio::fs::write(&legacy_file, legacy_json.as_bytes())
            .await
            .unwrap();

        let repo = JsonDownloadHistoryRepository::new(&temp_dir);

        // Loading triggers migration
        let loaded = repo.load().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].title(), "Legacy Video");

        // Verify ndjson file exists and legacy file was renamed to .migrated
        assert!(repo.history_file().exists());
        assert!(!legacy_file.exists());
        assert!(temp_dir
            .join(format!("{LEGACY_JSON_HISTORY_FILE_NAME}.migrated"))
            .exists());

        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }
}
