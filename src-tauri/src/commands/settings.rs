// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::app_state::AppState;
use crate::dto::{IpcError, SetSettingsRequest};
use crate::path_resolver::resolve_user_directory;
use polysaver_core::domain::{AppSettings, AppSettingsDto};
use tauri::State;

/// IPC command fetching current application settings.
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettingsDto, IpcError> {
    let settings = state.settings_repo.load().await.map_err(IpcError::from)?;
    Ok(AppSettingsDto::from(&settings))
}

/// IPC command saving updated application settings after core validation.
#[tauri::command]
pub async fn set_settings(
    state: State<'_, AppState>,
    mut request: SetSettingsRequest,
) -> Result<AppSettingsDto, IpcError> {
    let resolved_dir =
        resolve_user_directory(&request.settings.download_directory, &state.home_dir)?;
    request.settings.download_directory = resolved_dir.to_string_lossy().to_string();

    let validated = AppSettings::try_from(request.settings).map_err(IpcError::from)?;
    state
        .settings_repo
        .save(&validated)
        .await
        .map_err(IpcError::from)?;
    state
        .start_download_service
        .set_max_concurrent(validated.effective_max_concurrent());
    Ok(AppSettingsDto::from(&validated))
}
