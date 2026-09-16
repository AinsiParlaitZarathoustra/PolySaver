// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

pub mod download_job;
pub mod engine_version;
pub mod format;
pub mod history;
pub mod media_url;
pub mod probe;
pub mod settings;

pub use download_job::{DownloadId, DownloadJob, DownloadStatus};
pub use engine_version::{is_version_outdated, version_age_days, OUTDATED_THRESHOLD_DAYS};
pub use format::{
    DownloadPreset, DownloadPresetDto, Language, Mp3Quality, OutputFormat, ThemeMode, VideoQuality,
};
pub use history::{DownloadHistoryEntry, HistoryEntryId};
pub use media_url::{MediaUrl, MediaUrlKind};
pub use probe::{
    FormatOption, MediaKind, PlaylistEntry, ProbeResult, MAX_PLAYLIST_DOWNLOADS,
    PLAYLIST_ENTRIES_LIMIT,
};
pub use settings::{
    AppSettings, AppSettingsDto, CookiesBrowser, EngineChannel, SETTINGS_SCHEMA_VERSION,
};
