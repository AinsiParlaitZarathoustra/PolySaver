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
pub use engine_version::{OUTDATED_THRESHOLD_DAYS, is_version_outdated, version_age_days};
pub use format::{
    DownloadPreset, DownloadPresetDto, Language, Mp3Quality, OutputFormat, ThemeMode, VideoQuality,
};
pub use history::{DownloadHistoryEntry, HistoryEntryId};
pub use media_url::{MediaUrl, MediaUrlKind};
pub use probe::{
    FormatOption, MAX_PLAYLIST_DOWNLOADS, MediaKind, PLAYLIST_ENTRIES_LIMIT, PlaylistEntry,
    ProbeResult,
};
pub use settings::{
    AppSettings, AppSettingsDto, CookiesBrowser, EngineChannel, SETTINGS_SCHEMA_VERSION,
};
