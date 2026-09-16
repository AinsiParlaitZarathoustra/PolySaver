// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::format::{DownloadPreset, DownloadPresetDto, Language, ThemeMode};
use crate::error::CoreError;
use serde::{Deserialize, Serialize};

/// Current schema version persisted in `settings.json`.
/// Version 0 is the implicit version of files written before the field existed.
pub const SETTINGS_SCHEMA_VERSION: u32 = 1;

/// Release channel of the yt-dlp engine.
///
/// Upstream recommends nightly builds to regular users because YouTube fixes
/// land there faster, but stable remains the default for predictability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineChannel {
    #[default]
    Stable,
    Nightly,
}

impl EngineChannel {
    /// Stable machine-readable channel name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Nightly => "nightly",
        }
    }
}

/// Browser whose cookie database yt-dlp may read via `--cookies-from-browser`.
///
/// This is a closed enum on purpose: the value is injected as a command-line
/// argument, so accepting a free-form string would allow argument injection.
/// PolySaver never reads, copies, or persists cookie contents — only the
/// browser name is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CookiesBrowser {
    Brave,
    Chrome,
    Chromium,
    Edge,
    Firefox,
    Opera,
    Safari,
    Vivaldi,
    Whale,
}

impl CookiesBrowser {
    /// Canonical value passed to `--cookies-from-browser`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Brave => "brave",
            Self::Chrome => "chrome",
            Self::Chromium => "chromium",
            Self::Edge => "edge",
            Self::Firefox => "firefox",
            Self::Opera => "opera",
            Self::Safari => "safari",
            Self::Vivaldi => "vivaldi",
            Self::Whale => "whale",
        }
    }
}

/// Canonical application settings.
/// Enforces invariants: non-empty, absolute download directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppSettings {
    download_directory: String,
    theme_mode: ThemeMode,
    default_preset: DownloadPreset,
    language: Language,
    cookies_from_browser: Option<CookiesBrowser>,
    engine_channel: EngineChannel,
}

impl AppSettings {
    /// Creates and validates application settings.
    ///
    /// Optional capabilities (`cookies_from_browser`, `engine_channel`) keep
    /// their defaults and are set through dedicated builders.
    pub fn new(
        download_directory: String,
        theme_mode: ThemeMode,
        default_preset: DownloadPreset,
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

        Ok(Self {
            download_directory: trimmed_dir.to_string(),
            theme_mode,
            default_preset,
            language,
            cookies_from_browser: None,
            engine_channel: EngineChannel::default(),
        })
    }

    /// Sets the browser used for cookie extraction (validated closed enum).
    #[must_use]
    pub fn with_cookies_from_browser(mut self, browser: Option<CookiesBrowser>) -> Self {
        self.cookies_from_browser = browser;
        self
    }

    /// Sets the engine release channel.
    #[must_use]
    pub fn with_engine_channel(mut self, channel: EngineChannel) -> Self {
        self.engine_channel = channel;
        self
    }

    /// Constructs default sensible settings for a validated system download directory.
    pub fn defaults_for(
        download_directory: impl AsRef<std::path::Path>,
    ) -> Result<Self, CoreError> {
        let dir_str = download_directory.as_ref().to_string_lossy().to_string();
        Self::new(
            dir_str,
            ThemeMode::System,
            DownloadPreset::default(),
            Language::French,
        )
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
    pub const fn default_preset(&self) -> DownloadPreset {
        self.default_preset
    }

    #[must_use]
    pub const fn language(&self) -> Language {
        self.language
    }

    /// Browser used for `--cookies-from-browser`, if any.
    #[must_use]
    pub const fn cookies_from_browser(&self) -> Option<CookiesBrowser> {
        self.cookies_from_browser
    }

    /// Engine release channel (stable by default).
    #[must_use]
    pub const fn engine_channel(&self) -> EngineChannel {
        self.engine_channel
    }
}

/// Raw DTO for untrusted deserialization.
///
/// Unknown legacy keys (e.g. `parallelDownloads`, `maxConcurrent`) are ignored by
/// serde by default, so pre-2.5 `settings.json` files remain readable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsDto {
    pub download_directory: String,
    pub theme_mode: ThemeMode,
    pub default_preset: DownloadPresetDto,
    #[serde(default)]
    pub language: Language,
    #[serde(default)]
    pub cookies_from_browser: Option<CookiesBrowser>,
    #[serde(default)]
    pub engine_channel: EngineChannel,
    /// Schema version of the persisted document; 0 means "written before versioning".
    #[serde(default)]
    pub schema_version: u32,
}

impl From<&AppSettings> for AppSettingsDto {
    fn from(settings: &AppSettings) -> Self {
        Self {
            download_directory: settings.download_directory().to_string(),
            theme_mode: settings.theme_mode(),
            default_preset: DownloadPresetDto::from(&settings.default_preset()),
            language: settings.language(),
            cookies_from_browser: settings.cookies_from_browser(),
            engine_channel: settings.engine_channel(),
            schema_version: SETTINGS_SCHEMA_VERSION,
        }
    }
}

impl TryFrom<AppSettingsDto> for AppSettings {
    type Error = CoreError;

    fn try_from(dto: AppSettingsDto) -> Result<Self, Self::Error> {
        let preset = DownloadPreset::try_from(dto.default_preset)?;
        Ok(
            Self::new(dto.download_directory, dto.theme_mode, preset, dto.language)?
                .with_cookies_from_browser(dto.cookies_from_browser)
                .with_engine_channel(dto.engine_channel),
        )
    }
}
