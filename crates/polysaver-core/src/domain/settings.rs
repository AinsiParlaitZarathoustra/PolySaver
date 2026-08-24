// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::format::{DownloadPreset, DownloadPresetDto, Language, ThemeMode};
use crate::error::CoreError;
use serde::{Deserialize, Serialize};

/// Canonical application settings.
/// Enforces invariants: max_concurrent in 1..=8, non-empty directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppSettings {
    download_directory: String,
    theme_mode: ThemeMode,
    parallel_downloads: bool,
    default_preset: DownloadPreset,
    max_concurrent: u8,
    language: Language,
}

impl AppSettings {
    /// Creates and validates application settings.
    pub fn new(
        download_directory: String,
        theme_mode: ThemeMode,
        parallel_downloads: bool,
        default_preset: DownloadPreset,
        max_concurrent: u8,
        language: Language,
    ) -> Result<Self, CoreError> {
        let trimmed_dir = download_directory.trim();
        if trimmed_dir.is_empty() {
            return Err(CoreError::InvalidSettings(
                "Download directory cannot be empty".to_string(),
            ));
        }

        if trimmed_dir.starts_with('~') {
            return Err(CoreError::InvalidSettings(
                "Download directory cannot start with '~'".to_string(),
            ));
        }

        if trimmed_dir.contains('\0') {
            return Err(CoreError::InvalidSettings(
                "Download directory cannot contain null bytes".to_string(),
            ));
        }

        let path = std::path::Path::new(trimmed_dir);
        if !path.is_absolute() {
            return Err(CoreError::InvalidSettings(format!(
                "Download directory must be an absolute path: '{trimmed_dir}'"
            )));
        }

        if !(1..=8).contains(&max_concurrent) {
            return Err(CoreError::InvalidSettings(format!(
                "Max concurrent downloads must be between 1 and 8, got {max_concurrent}"
            )));
        }

        Ok(Self {
            download_directory: trimmed_dir.to_string(),
            theme_mode,
            parallel_downloads,
            default_preset,
            max_concurrent,
            language,
        })
    }

    /// Constructs default sensible settings for a validated system download directory.
    pub fn defaults_for(
        download_directory: impl AsRef<std::path::Path>,
    ) -> Result<Self, CoreError> {
        let dir_str = download_directory.as_ref().to_string_lossy().to_string();
        Self::new(
            dir_str,
            ThemeMode::System,
            true,
            DownloadPreset::default(),
            3,
            Language::French,
        )
    }

    /// Effective maximum concurrent downloads based on the `parallel_downloads` toggle.
    /// Returns 1 when parallel downloads are disabled, otherwise `max_concurrent`.
    #[must_use]
    pub const fn effective_max_concurrent(&self) -> usize {
        if self.parallel_downloads {
            self.max_concurrent as usize
        } else {
            1
        }
    }

    /// Returns the number of parallel segments per stream.
    /// When parallel downloads is disabled, returns 1.
    /// When enabled, returns up to 8.
    #[must_use]
    pub const fn effective_parallel_segments(&self) -> usize {
        if self.parallel_downloads {
            8
        } else {
            1
        }
    }

    #[must_use]
    pub fn download_directory(&self) -> &str {
        &self.download_directory
    }

    #[must_use]
    pub const fn theme_mode(&self) -> ThemeMode {
        self.theme_mode
    }

    #[must_use]
    pub const fn parallel_downloads(&self) -> bool {
        self.parallel_downloads
    }

    #[must_use]
    pub const fn default_preset(&self) -> DownloadPreset {
        self.default_preset
    }

    #[must_use]
    pub const fn max_concurrent(&self) -> u8 {
        self.max_concurrent
    }

    #[must_use]
    pub const fn language(&self) -> Language {
        self.language
    }
}

/// Raw DTO for untrusted deserialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsDto {
    pub download_directory: String,
    pub theme_mode: ThemeMode,
    pub parallel_downloads: bool,
    pub default_preset: DownloadPresetDto,
    pub max_concurrent: u8,
    #[serde(default)]
    pub language: Language,
}

impl From<&AppSettings> for AppSettingsDto {
    fn from(settings: &AppSettings) -> Self {
        Self {
            download_directory: settings.download_directory().to_string(),
            theme_mode: settings.theme_mode(),
            parallel_downloads: settings.parallel_downloads(),
            default_preset: DownloadPresetDto::from(&settings.default_preset()),
            max_concurrent: settings.max_concurrent(),
            language: settings.language(),
        }
    }
}

impl TryFrom<AppSettingsDto> for AppSettings {
    type Error = CoreError;

    fn try_from(dto: AppSettingsDto) -> Result<Self, Self::Error> {
        let preset = DownloadPreset::try_from(dto.default_preset)?;
        Self::new(
            dto.download_directory,
            dto.theme_mode,
            dto.parallel_downloads,
            preset,
            dto.max_concurrent,
            dto.language,
        )
    }
}
