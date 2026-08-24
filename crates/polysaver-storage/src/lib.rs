// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! # PolySaver Storage Adapter
//!
//! Persistence adapters for settings and download queues.
//! All external JSON data is treated as untrusted and strictly validated via `TryFrom`.

pub mod history_repository;
pub mod json_repository;

use async_trait::async_trait;
pub use history_repository::{
    DownloadHistoryEntryDto, InMemoryDownloadHistoryRepository, JsonDownloadHistoryRepository,
};
pub use json_repository::JsonSettingsRepository;
use polysaver_core::domain::{AppSettings, AppSettingsDto};
use polysaver_core::error::CoreError;
use polysaver_core::ports::SettingsRepository;
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory settings repository for tests.
#[derive(Debug, Clone)]
pub struct InMemorySettingsRepository {
    settings: Arc<RwLock<AppSettings>>,
}

impl InMemorySettingsRepository {
    /// Initializes repository with default validated settings.
    #[must_use]
    pub fn new() -> Self {
        let default_settings =
            AppSettings::defaults_for("/tmp/polysaver_tests").unwrap_or_else(|_| {
                AppSettings::new(
                    "/tmp/polysaver_tests".to_string(),
                    polysaver_core::domain::ThemeMode::System,
                    false,
                    polysaver_core::domain::DownloadPreset::video(
                        polysaver_core::domain::OutputFormat::Mp4,
                        polysaver_core::domain::VideoQuality::Best,
                    )
                    .unwrap(),
                    3,
                    polysaver_core::domain::Language::French,
                )
                .unwrap()
            });
        Self {
            settings: Arc::new(RwLock::new(default_settings)),
        }
    }

    /// Initializes repository with specific settings.
    #[must_use]
    pub fn from_settings(settings: AppSettings) -> Self {
        Self {
            settings: Arc::new(RwLock::new(settings)),
        }
    }
}

impl Default for InMemorySettingsRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SettingsRepository for InMemorySettingsRepository {
    async fn load(&self) -> Result<AppSettings, CoreError> {
        Ok(self.settings.read().await.clone())
    }

    async fn save(&self, settings: &AppSettings) -> Result<(), CoreError> {
        *self.settings.write().await = settings.clone();
        Ok(())
    }
}

/// Parses untrusted JSON into validated canonical AppSettings using TryFrom.
pub fn parse_untrusted_settings_json(raw_json: &str) -> Result<AppSettings, CoreError> {
    let dto: AppSettingsDto = serde_json::from_str(raw_json)
        .map_err(|err| CoreError::StorageError(format!("Malformed settings JSON: {err}")))?;
    AppSettings::try_from(dto)
}

#[cfg(test)]
mod tests {
    use super::*;
    use polysaver_core::domain::download_job::DownloadId;
    use polysaver_core::domain::history::DownloadHistoryEntry;
    use polysaver_core::domain::media_url::MediaUrl;
    use polysaver_core::domain::{DownloadPreset, Language, OutputFormat, ThemeMode, VideoQuality};
    use polysaver_core::ports::DownloadHistoryRepository;

    #[tokio::test]
    async fn test_json_settings_repository_lifecycle() {
        let test_dir =
            std::env::temp_dir().join(format!("polysaver_storage_test_{}", uuid::Uuid::new_v4()));
        let default_download_dir = test_dir.join("PolySaver");
        let default_settings = AppSettings::defaults_for(&default_download_dir).unwrap();
        let repo = JsonSettingsRepository::new(&test_dir, default_settings.clone());

        // 1. Missing file returns defaults (ThemeMode::System, Language::French)
        let loaded_default = repo.load().await.unwrap();
        assert_eq!(
            loaded_default.download_directory(),
            default_download_dir.to_str().unwrap()
        );
        assert_eq!(loaded_default.theme_mode(), ThemeMode::System);
        assert_eq!(loaded_default.language(), Language::French);

        // 2. Save modified settings
        let custom_preset = DownloadPreset::video(OutputFormat::Mov, VideoQuality::P2160).unwrap();
        let custom = AppSettings::new(
            "/custom/download/path".to_string(),
            ThemeMode::Light,
            false,
            custom_preset,
            6,
            Language::English,
        )
        .unwrap();

        repo.save(&custom).await.unwrap();

        // 3. Reload from fresh repository instance (cache-busted)
        let repo2 = JsonSettingsRepository::new(&test_dir, default_settings.clone());
        let loaded_custom = repo2.load().await.unwrap();
        assert_eq!(loaded_custom.download_directory(), "/custom/download/path");
        assert_eq!(loaded_custom.theme_mode(), ThemeMode::Light);
        assert!(!loaded_custom.parallel_downloads());
        assert_eq!(loaded_custom.default_preset(), custom_preset);
        assert_eq!(loaded_custom.max_concurrent(), 6);
        assert_eq!(loaded_custom.language(), Language::English);

        // 4. Test backward-compatibility migration of legacy ~/Downloads/PolySaver
        let legacy_json = r#"{
            "downloadDirectory": "~/Downloads/PolySaver",
            "themeMode": "dark",
            "parallelDownloads": true,
            "defaultPreset": {
                "format": "mp4",
                "videoQuality": "p1080"
            },
            "maxConcurrent": 3
        }"#;
        tokio::fs::write(repo2.config_file(), legacy_json.as_bytes())
            .await
            .unwrap();
        let repo_legacy = JsonSettingsRepository::new(&test_dir, default_settings.clone());
        let migrated = repo_legacy.load().await.unwrap();
        assert_eq!(
            migrated.download_directory(),
            default_download_dir.to_str().unwrap()
        );
        assert_eq!(migrated.theme_mode(), ThemeMode::Dark);
        assert_eq!(migrated.language(), Language::French);

        // 5. Corrupted JSON rejected gracefully
        tokio::fs::write(repo2.config_file(), b"not valid json")
            .await
            .unwrap();
        let repo3 = JsonSettingsRepository::new(&test_dir, default_settings);
        let err = repo3.load().await;
        assert!(matches!(err, Err(CoreError::StorageError(_))));

        // Clean up
        let _ = tokio::fs::remove_dir_all(&test_dir).await;
    }

    #[tokio::test]
    async fn test_json_history_repository_lifecycle() {
        let test_dir =
            std::env::temp_dir().join(format!("polysaver_history_test_{}", uuid::Uuid::new_v4()));
        let repo = JsonDownloadHistoryRepository::new(&test_dir);

        // 1. Missing file returns empty history
        let entries = repo.load().await.unwrap();
        assert!(entries.is_empty());

        // 2. Append entry 1
        let job1_id = DownloadId::new();
        let url1 = MediaUrl::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
        let preset1 = DownloadPreset::video(OutputFormat::Mp4, VideoQuality::P1080).unwrap();
        let entry1 = DownloadHistoryEntry::new(
            job1_id,
            url1.clone(),
            "Rick Astley - Never Gonna Give You Up".to_string(),
            preset1,
            "/downloads/video1.mp4".to_string(),
            Some(1000),
        )
        .unwrap();

        repo.append(entry1.clone()).await.unwrap();

        // 3. Append entry 2 (newer timestamp)
        let job2_id = DownloadId::new();
        let url2 = MediaUrl::parse("https://www.youtube.com/watch?v=jNQXAC9IVRw").unwrap();
        let preset2 = DownloadPreset::mp3(polysaver_core::domain::Mp3Quality::K320);
        let entry2 = DownloadHistoryEntry::new(
            job2_id,
            url2.clone(),
            "Me at the zoo".to_string(),
            preset2,
            "/downloads/zoo.mp3".to_string(),
            Some(2000),
        )
        .unwrap();

        repo.append(entry2.clone()).await.unwrap();

        // 4. Reload from fresh instance and verify order (newest first)
        let repo2 = JsonDownloadHistoryRepository::new(&test_dir);
        let loaded = repo2.load().await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].download_id(), job2_id);
        assert_eq!(loaded[1].download_id(), job1_id);

        // 5. Test idempotence: appending existing download_id updates instead of duplicating
        let entry1_updated = DownloadHistoryEntry::new(
            job1_id,
            url1.clone(),
            "Rick Astley - Updated Title".to_string(),
            preset1,
            "/downloads/video1_updated.mp4".to_string(),
            Some(3000),
        )
        .unwrap();
        repo2.append(entry1_updated).await.unwrap();
        let loaded_updated = repo2.load().await.unwrap();
        assert_eq!(loaded_updated.len(), 2);
        assert_eq!(loaded_updated[0].download_id(), job1_id);
        assert_eq!(loaded_updated[0].title(), "Rick Astley - Updated Title");

        // 6. Test removal without deleting physical files
        repo2.remove(loaded_updated[0].id()).await.unwrap();
        let after_removal = repo2.load().await.unwrap();
        assert_eq!(after_removal.len(), 1);
        assert_eq!(after_removal[0].download_id(), job2_id);

        // Clean up
        let _ = tokio::fs::remove_dir_all(&test_dir).await;
    }
}
