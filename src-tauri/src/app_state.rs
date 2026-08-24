// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use polysaver_binres::BinaryResolver;
use polysaver_core::ports::SettingsRepository;
use polysaver_core::services::{AnalyzeUrlService, StartDownloadService};
use std::sync::Arc;

/// Central application state injected into Tauri commands.
#[derive(Clone)]
pub struct AppState {
    pub start_download_service: Arc<StartDownloadService>,
    pub analyze_service: Arc<AnalyzeUrlService>,
    pub settings_repo: Arc<dyn SettingsRepository>,
    pub resolver: Arc<BinaryResolver>,
    pub home_dir: std::path::PathBuf,
}
