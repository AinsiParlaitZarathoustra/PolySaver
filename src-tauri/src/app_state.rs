// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use polysaver_binres::BinaryResolver;
use polysaver_core::ports::SettingsRepository;
use polysaver_core::services::{AnalyzeUrlService, DetectPlaylistService, StartDownloadService};
use std::sync::Arc;

/// Central application state injected into Tauri commands.
#[derive(Clone)]
pub struct AppState {
    pub start_download_service: Arc<StartDownloadService>,
    pub analyze_service: Arc<AnalyzeUrlService>,
    /// Native "is this a playlist?" detection, answered by the engine itself.
    pub detect_playlist_service: Arc<DetectPlaylistService>,
    pub settings_repo: Arc<dyn SettingsRepository>,
    pub resolver: Arc<BinaryResolver>,
    pub home_dir: std::path::PathBuf,
    pub app_bin_dir: std::path::PathBuf,
    pub engine_update_cache_file: std::path::PathBuf,
    pub engine_updater: Arc<dyn polysaver_core::services::EngineUpdater>,
    /// Shared yt-dlp adapter, used to apply settings that affect yt-dlp arguments.
    pub ytdlp_downloader: Arc<polysaver_ytdlp::YtDlpDownloader>,
}
