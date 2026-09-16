// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::app_state::AppState;
use crate::dto::{AnalyzeUrlRequest, IpcError, PlaylistDetectionDto, ProbeResultDto};
use tauri::State;

/// IPC command analyzing a media URL.
/// Thin adapter: converts request, delegates strictly to sovereign AnalyzeUrlService, maps to ProbeResultDto.
#[tauri::command]
pub async fn analyze_url(
    state: State<'_, AppState>,
    request: AnalyzeUrlRequest,
) -> Result<ProbeResultDto, IpcError> {
    let result = state
        .analyze_service
        .analyze(&request.url)
        .await
        .map_err(IpcError::from)?;
    Ok(ProbeResultDto::from(&result))
}

/// IPC command answering "is this a playlist?" using the engine as the source of truth.
///
/// The answer never downloads media and is a plain engine query, so it is safe to
/// call while the user is still editing the field.
#[tauri::command]
pub async fn detect_playlist(
    state: State<'_, AppState>,
    request: AnalyzeUrlRequest,
) -> Result<PlaylistDetectionDto, IpcError> {
    let detection = state
        .detect_playlist_service
        .detect(&request.url)
        .await
        .map_err(IpcError::from)?;
    Ok(detection.into())
}

/// IPC command canceling the currently running playlist detection immediately.
#[tauri::command]
pub async fn cancel_playlist_detection(state: State<'_, AppState>) -> Result<(), IpcError> {
    state.detect_playlist_service.cancel_current().await;
    Ok(())
}

/// IPC command canceling the currently running URL analysis immediately.
#[tauri::command]
pub async fn cancel_analyze(state: State<'_, AppState>) -> Result<(), IpcError> {
    state.analyze_service.cancel_current().await;
    Ok(())
}
