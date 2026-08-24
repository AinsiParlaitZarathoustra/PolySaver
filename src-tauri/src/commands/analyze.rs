// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::app_state::AppState;
use crate::dto::{AnalyzeUrlRequest, IpcError, ProbeResultDto};
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
