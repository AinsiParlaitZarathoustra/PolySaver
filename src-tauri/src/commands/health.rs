// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::app_state::AppState;
use crate::dto::{HealthResponse, IpcError};
use tauri::State;

/// IPC command checking the diagnostic availability of core and adapters.
#[tauri::command]
pub async fn health_check(state: State<'_, AppState>) -> Result<HealthResponse, IpcError> {
    let ytdlp = polysaver_ytdlp::probe_ytdlp_with_resolver(&state.resolver).await;
    let ffmpeg = polysaver_ffmpeg::probe_ffmpeg_with_resolver(&state.resolver).await;

    Ok(HealthResponse {
        core_status: "ready".to_string(),
        ytdlp,
        ffmpeg,
    })
}
