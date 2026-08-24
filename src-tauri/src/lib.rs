// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! # PolySaver Tauri Composition Root
//!
//! Wires peripheral adapters into the sovereign core services and registers Tauri IPC commands.

pub mod app_state;
pub mod commands;
pub mod dto;
pub mod events;
pub mod path_resolver;

use app_state::AppState;
use events::TauriEventSink;
use polysaver_core::services::{AnalyzeUrlService, StartDownloadService};
use polysaver_ffmpeg::FfmpegConverter;
use polysaver_storage::{JsonDownloadHistoryRepository, JsonSettingsRepository};
use polysaver_ytdlp::YtDlpDownloader;
use std::sync::Arc;
use tauri::Manager;

/// Runs the PolySaver desktop application.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // Resolve app directories
            let app_data_dir = app.path().app_data_dir()?;
            let app_config_dir = app.path().app_config_dir()?;
            let home_dir = app.path().home_dir()?;
            let default_download_dir = match app.path().download_dir() {
                Ok(d) => d.join("PolySaver"),
                Err(_) => home_dir.join("Downloads").join("PolySaver"),
            };

            let bin_dir = app_data_dir.join("bin");
            let temp_dir = app_data_dir.join("temp");
            let resource_bin_dir = app.path().resource_dir().ok().map(|r| r.join("bin"));

            // Ensure directories exist
            std::fs::create_dir_all(&bin_dir)?;
            std::fs::create_dir_all(&temp_dir)?;
            std::fs::create_dir_all(&app_config_dir)?;
            std::fs::create_dir_all(&default_download_dir)?;

            let default_settings =
                polysaver_core::domain::AppSettings::defaults_for(&default_download_dir)?;

            // Instantiate centralized binary resolver with positive caching
            let resolver = Arc::new(polysaver_binres::BinaryResolver::new(
                bin_dir,
                resource_bin_dir,
            ));

            // Instantiate peripheral adapters sharing the centralized resolver
            let event_sink = Arc::new(TauriEventSink::new(handle));
            let ytdlp_downloader = Arc::new(YtDlpDownloader::with_resolver(Arc::clone(&resolver)));
            let ffmpeg_converter = Arc::new(FfmpegConverter::with_resolver(Arc::clone(&resolver)));
            let settings_repo = Arc::new(JsonSettingsRepository::new(
                app_config_dir.clone(),
                default_settings,
            ));
            let history_repo = Arc::new(JsonDownloadHistoryRepository::new(app_config_dir));

            // Instantiate core use case services
            let analyze_service = Arc::new(AnalyzeUrlService::new(ytdlp_downloader.clone()));
            let start_download_service = Arc::new(StartDownloadService::new(
                ytdlp_downloader,
                ffmpeg_converter.clone(),
                ffmpeg_converter,
                settings_repo.clone(),
                history_repo,
                Some(event_sink),
                temp_dir,
            ));

            let state = AppState {
                start_download_service,
                analyze_service,
                settings_repo,
                resolver,
                home_dir,
            };

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
            commands::analyze_url,
            commands::get_settings,
            commands::set_settings,
            commands::list_downloads,
            commands::start_download,
            commands::dismiss_download,
            commands::open_download_source_url,
            commands::reveal_downloaded_file,
            commands::open_downloaded_file,
            commands::list_download_history,
            commands::remove_download_history_entry,
            commands::reveal_history_file,
            commands::open_history_file,
            commands::open_history_source_url,
            commands::cancel_download,
            commands::open_support_page,
        ])
        .run(tauri::generate_context!())?;

    Ok(())
}
