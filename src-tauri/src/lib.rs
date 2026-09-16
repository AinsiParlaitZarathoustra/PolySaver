// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! # PolySaver Tauri Composition Root
//!
//! Wires peripheral adapters into the sovereign core services and registers Tauri IPC commands.

// `IpcError` embeds structured download details and is deliberately not boxed:
// it is the serialized IPC contract consumed by the frontend, and Tauri commands
// must return it by value. Boxing it would only add an allocation per command.
#![allow(clippy::result_large_err)]

pub mod app_state;
pub mod commands;
pub mod dto;
pub mod events;
pub mod path_resolver;

use app_state::AppState;
use events::TauriEventSink;
use polysaver_core::ports::SettingsRepository as _;
use polysaver_core::services::{AnalyzeUrlService, DetectPlaylistService, StartDownloadService};
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
            let engine_update_cache_file = app_data_dir.join("engine-update-check.json");
            let resource_bin_dir = app.path().resource_dir().ok().map(|r| r.join("bin"));

            // Ensure directories exist
            std::fs::create_dir_all(&bin_dir)?;
            std::fs::create_dir_all(&temp_dir)?;
            std::fs::create_dir_all(&app_config_dir)?;
            std::fs::create_dir_all(&default_download_dir)?;

            // Purge leftover job_* workspaces from a previous crash or SIGKILL.
            // Anything younger than 24 h is left untouched so a concurrently
            // running instance never loses its in-flight temp files.
            let purged = tauri::async_runtime::block_on(
                polysaver_core::services::start_download::purge_orphan_temp_dirs(&temp_dir),
            );
            if purged > 0 {
                eprintln!("[PolySaver] Purged {purged} orphaned temporary job director(ies)");
            }

            let default_settings =
                polysaver_core::domain::AppSettings::defaults_for(&default_download_dir)?;

            // Instantiate centralized binary resolver with positive caching
            let resolver = Arc::new(polysaver_binres::BinaryResolver::new(
                bin_dir.clone(),
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

            // Apply the persisted cookie source to the downloader before any use,
            // then detect the JavaScript runtime so yt-dlp gets the absolute path.
            // `setup` is synchronous, so block briefly on these local operations.
            if let Ok(settings) = tauri::async_runtime::block_on(settings_repo.load()) {
                tauri::async_runtime::block_on(
                    ytdlp_downloader.set_cookies_from_browser(settings.cookies_from_browser()),
                );
            }
            let detected_runtime = tauri::async_runtime::block_on(
                polysaver_ytdlp::js_runtime::detect_js_runtime(&resolver),
            );
            tauri::async_runtime::block_on(
                ytdlp_downloader.set_js_runtime(detected_runtime.to_spec()),
            );

            // Engine updater used by the single post-update retry path and by IPC.
            let engine_updater: Arc<dyn polysaver_core::services::EngineUpdater> =
                Arc::new(crate::commands::TauriEngineUpdater::new(
                    bin_dir.clone(),
                    engine_update_cache_file.clone(),
                    Arc::clone(&resolver),
                    settings_repo.clone(),
                ));

            // Instantiate core use case services
            let analyze_service = Arc::new(AnalyzeUrlService::new(ytdlp_downloader.clone()));
            // Same adapter, separate use case: the engine answers whether an URL is
            // a playlist, without downloading anything.
            let detect_playlist_service =
                Arc::new(DetectPlaylistService::new(ytdlp_downloader.clone()));
            let start_download_service = Arc::new(
                StartDownloadService::new_with_retry_policy(
                    ytdlp_downloader.clone(),
                    ffmpeg_converter.clone(),
                    ffmpeg_converter,
                    settings_repo.clone(),
                    history_repo,
                    Some(event_sink),
                    temp_dir,
                    polysaver_core::services::RetryPolicy::default(),
                )
                .with_engine_updater(Arc::clone(&engine_updater)),
            );

            let state = AppState {
                start_download_service,
                analyze_service,
                detect_playlist_service,
                settings_repo,
                resolver,
                home_dir,
                app_bin_dir: bin_dir,
                engine_update_cache_file,
                engine_updater,
                ytdlp_downloader,
            };

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
            commands::analyze_url,
            commands::detect_playlist,
            commands::cancel_playlist_detection,
            commands::cancel_analyze,
            commands::get_settings,
            commands::set_settings,
            commands::list_downloads,
            commands::start_download,
            commands::start_playlist_download,
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
            commands::retry_download,
            commands::check_engine_update,
            commands::update_engine,
            commands::rollback_engine,
            commands::check_js_runtime,
            commands::install_js_runtime,
            commands::open_support_page,
            commands::open_contact_email,
        ])
        .run(tauri::generate_context!())?;

    Ok(())
}
