// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use async_trait::async_trait;
use polysaver_core::domain::{AppSettings, AppSettingsDto};
use polysaver_core::error::CoreError;
use polysaver_core::ports::SettingsRepository;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Persistent JSON file settings repository.
/// Performs atomic writes and validates untrusted JSON via `TryFrom<AppSettingsDto>`.
#[derive(Debug, Clone)]
pub struct JsonSettingsRepository {
    config_file: PathBuf,
    default_settings: AppSettings,
    cache: Arc<RwLock<Option<AppSettings>>>,
}

impl JsonSettingsRepository {
    /// Creates a repository storing settings in `<config_dir>/settings.json` with injected defaults.
    #[must_use]
    pub fn new(config_dir: impl AsRef<Path>, default_settings: AppSettings) -> Self {
        let config_file = config_dir.as_ref().join("settings.json");
        Self {
            config_file,
            default_settings,
            cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Explicit path constructor for testing.
    #[must_use]
    pub fn with_file_path(config_file: PathBuf, default_settings: AppSettings) -> Self {
        Self {
            config_file,
            default_settings,
            cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Access the underlying configuration file path.
    #[must_use]
    pub fn config_file(&self) -> &Path {
        &self.config_file
    }
}

#[async_trait]
impl SettingsRepository for JsonSettingsRepository {
    async fn load(&self) -> Result<AppSettings, CoreError> {
        // Fast-path: return cached settings if already loaded
        {
            let cache_read = self.cache.read().await;
            if let Some(ref cached) = *cache_read {
                return Ok(cached.clone());
            }
        }

        if !self.config_file.exists() {
            let mut cache_write = self.cache.write().await;
            *cache_write = Some(self.default_settings.clone());
            return Ok(self.default_settings.clone());
        }

        let raw_json = tokio::fs::read_to_string(&self.config_file)
            .await
            .map_err(|err| {
                CoreError::StorageError(format!(
                    "Failed to read settings file '{}': {err}",
                    self.config_file.display()
                ))
            })?;

        let mut dto: AppSettingsDto = serde_json::from_str(&raw_json).map_err(|err| {
            CoreError::StorageError(format!(
                "Malformed JSON in settings file '{}': {err}",
                self.config_file.display()
            ))
        })?;

        // One-time migration of legacy literal "~/Downloads/PolySaver"
        let mut migrated = false;
        if dto.download_directory == "~/Downloads/PolySaver" {
            dto.download_directory = self.default_settings.download_directory().to_string();
            migrated = true;
        }

        let settings = AppSettings::try_from(dto)?;

        if migrated {
            let _ = self.save(&settings).await;
        }

        // Update in-memory cache
        let mut cache_write = self.cache.write().await;
        *cache_write = Some(settings.clone());

        Ok(settings)
    }

    async fn save(&self, settings: &AppSettings) -> Result<(), CoreError> {
        let parent_dir = self.config_file.parent().ok_or_else(|| {
            CoreError::StorageError(format!(
                "Invalid configuration path '{}': missing parent directory",
                self.config_file.display()
            ))
        })?;

        tokio::fs::create_dir_all(parent_dir).await.map_err(|err| {
            CoreError::StorageError(format!(
                "Failed to create configuration directory '{}': {err}",
                parent_dir.display()
            ))
        })?;

        let dto = AppSettingsDto::from(settings);
        let serialized = serde_json::to_string_pretty(&dto).map_err(|err| {
            CoreError::StorageError(format!("Failed to serialize settings: {err}"))
        })?;

        // Atomic write: write to temporary file then rename
        let tmp_file = parent_dir.join(format!(".settings_{}.tmp", uuid::Uuid::new_v4().simple()));

        tokio::fs::write(&tmp_file, serialized.as_bytes())
            .await
            .map_err(|err| {
                CoreError::StorageError(format!(
                    "Failed to write temporary settings file '{}': {err}",
                    tmp_file.display()
                ))
            })?;

        tokio::fs::rename(&tmp_file, &self.config_file)
            .await
            .map_err(|err| {
                CoreError::StorageError(format!(
                    "Failed to atomically persist settings file '{}': {err}",
                    self.config_file.display()
                ))
            })?;

        // Update cache
        let mut cache_write = self.cache.write().await;
        *cache_write = Some(settings.clone());

        Ok(())
    }
}
