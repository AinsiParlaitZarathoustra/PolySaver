// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::app_state::AppState;
use crate::dto::history::DownloadHistoryEntryDto;
use crate::dto::media::{DownloadJobDto, StartDownloadRequestDto};
use crate::dto::IpcError;
use crate::path_resolver::resolve_user_directory;
use polysaver_core::domain::history::HistoryEntryId;
use polysaver_core::domain::{DownloadId, DownloadPreset};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

/// IPC command listing current download jobs in memory.
#[tauri::command]
pub async fn list_downloads(state: State<'_, AppState>) -> Result<Vec<DownloadJobDto>, IpcError> {
    let jobs = state.start_download_service.list_downloads().await;
    Ok(jobs.iter().map(DownloadJobDto::from).collect())
}

/// IPC command starting a new download job with optional preset and optional custom output directory.
#[tauri::command]
pub async fn start_download(
    state: State<'_, AppState>,
    request: StartDownloadRequestDto,
) -> Result<DownloadJobDto, IpcError> {
    let preset = match request.preset {
        Some(dto) => Some(DownloadPreset::try_from(dto).map_err(IpcError::from)?),
        None => None,
    };

    let custom_output_dir = match request.output_directory {
        Some(ref dir_str) => {
            let resolved = resolve_user_directory(dir_str, &state.home_dir)?;
            Some(resolved)
        }
        None => None,
    };

    let job = state
        .start_download_service
        .start_download(&request.url, preset, custom_output_dir)
        .await
        .map_err(IpcError::from)?;

    Ok(DownloadJobDto::from(&job))
}

/// IPC command dismissing a job from active queue in memory.
#[tauri::command]
pub async fn dismiss_download(
    state: State<'_, AppState>,
    download_id: String,
) -> Result<(), IpcError> {
    let job_id = DownloadId::try_from(download_id.as_str()).map_err(IpcError::from)?;
    state
        .start_download_service
        .dismiss_download(job_id)
        .await
        .map_err(IpcError::from)?;
    Ok(())
}

/// IPC command opening the source URL of an active/known job in default browser.
#[tauri::command]
pub async fn open_download_source_url(
    app: AppHandle,
    state: State<'_, AppState>,
    download_id: String,
) -> Result<(), IpcError> {
    let job_id = DownloadId::try_from(download_id.as_str()).map_err(IpcError::from)?;
    let url = state
        .start_download_service
        .get_download_source_url(job_id)
        .await
        .map_err(IpcError::from)?;

    app.opener().open_url(url, None::<&str>).map_err(|err| {
        IpcError::new("OPENER_FAILED", format!("Failed to open source URL: {err}"))
    })?;

    Ok(())
}

/// IPC command revealing a completed download file in the system file manager.
#[tauri::command]
pub async fn reveal_downloaded_file(
    app: AppHandle,
    state: State<'_, AppState>,
    download_id: String,
) -> Result<(), IpcError> {
    let job_id = DownloadId::try_from(download_id.as_str()).map_err(IpcError::from)?;
    let path = state
        .start_download_service
        .get_completed_download_path(job_id)
        .await
        .map_err(IpcError::from)?;

    let path_str = path.to_string_lossy().to_string();
    app.opener()
        .reveal_item_in_dir(path_str)
        .map_err(|err| IpcError::new("OPENER_FAILED", format!("Failed to reveal file: {err}")))?;

    Ok(())
}

/// IPC command opening a completed download file with the default system application.
#[tauri::command]
pub async fn open_downloaded_file(
    app: AppHandle,
    state: State<'_, AppState>,
    download_id: String,
) -> Result<(), IpcError> {
    let job_id = DownloadId::try_from(download_id.as_str()).map_err(IpcError::from)?;
    let path = state
        .start_download_service
        .get_completed_download_path(job_id)
        .await
        .map_err(IpcError::from)?;

    let path_str = path.to_string_lossy().to_string();
    app.opener()
        .open_path(path_str, None::<&str>)
        .map_err(|err| IpcError::new("OPENER_FAILED", format!("Failed to open file: {err}")))?;

    Ok(())
}

/// IPC command listing persistent download history.
#[tauri::command]
pub async fn list_download_history(
    state: State<'_, AppState>,
) -> Result<Vec<DownloadHistoryEntryDto>, IpcError> {
    let entries = state
        .start_download_service
        .list_history()
        .await
        .map_err(IpcError::from)?;
    Ok(entries.iter().map(DownloadHistoryEntryDto::from).collect())
}

/// IPC command removing an entry from persistent download history without deleting media file.
#[tauri::command]
pub async fn remove_download_history_entry(
    state: State<'_, AppState>,
    history_id: String,
) -> Result<(), IpcError> {
    let id = HistoryEntryId::try_from(history_id.as_str()).map_err(IpcError::from)?;
    state
        .start_download_service
        .remove_history_entry(id)
        .await
        .map_err(IpcError::from)?;
    Ok(())
}

/// IPC command revealing a history item's downloaded file in Finder / file explorer.
#[tauri::command]
pub async fn reveal_history_file(
    app: AppHandle,
    state: State<'_, AppState>,
    history_id: String,
) -> Result<(), IpcError> {
    let id = HistoryEntryId::try_from(history_id.as_str()).map_err(IpcError::from)?;
    let path = state
        .start_download_service
        .get_history_file_path(id)
        .await
        .map_err(IpcError::from)?;

    let path_str = path.to_string_lossy().to_string();
    app.opener()
        .reveal_item_in_dir(path_str)
        .map_err(|err| IpcError::new("OPENER_FAILED", format!("Failed to reveal file: {err}")))?;

    Ok(())
}

/// IPC command opening a history item's downloaded file with default player.
#[tauri::command]
pub async fn open_history_file(
    app: AppHandle,
    state: State<'_, AppState>,
    history_id: String,
) -> Result<(), IpcError> {
    let id = HistoryEntryId::try_from(history_id.as_str()).map_err(IpcError::from)?;
    let path = state
        .start_download_service
        .get_history_file_path(id)
        .await
        .map_err(IpcError::from)?;

    let path_str = path.to_string_lossy().to_string();
    app.opener()
        .open_path(path_str, None::<&str>)
        .map_err(|err| IpcError::new("OPENER_FAILED", format!("Failed to open file: {err}")))?;

    Ok(())
}

/// IPC command opening a history item's source URL in default browser.
#[tauri::command]
pub async fn open_history_source_url(
    app: AppHandle,
    state: State<'_, AppState>,
    history_id: String,
) -> Result<(), IpcError> {
    let id = HistoryEntryId::try_from(history_id.as_str()).map_err(IpcError::from)?;
    let url = state
        .start_download_service
        .get_history_source_url(id)
        .await
        .map_err(IpcError::from)?;

    app.opener().open_url(url, None::<&str>).map_err(|err| {
        IpcError::new("OPENER_FAILED", format!("Failed to open source URL: {err}"))
    })?;

    Ok(())
}

/// IPC command canceling an in-progress or queued download job.
#[tauri::command]
pub async fn cancel_download(
    state: State<'_, AppState>,
    download_id: String,
) -> Result<DownloadJobDto, IpcError> {
    let job_id = DownloadId::try_from(download_id.as_str()).map_err(IpcError::from)?;
    let job = state
        .start_download_service
        .cancel_download(job_id)
        .await
        .map_err(IpcError::from)?;
    Ok(DownloadJobDto::from(&job))
}
