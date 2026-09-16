// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::app_state::AppState;
use crate::dto::{HealthResponse, IpcError, JsRuntimeAvailability};
use polysaver_ytdlp::js_runtime::detect_js_runtime;
use tauri::State;

/// IPC command checking the diagnostic availability of core and adapters.
#[tauri::command]
pub async fn health_check(state: State<'_, AppState>) -> Result<HealthResponse, IpcError> {
    let ytdlp = polysaver_ytdlp::probe_ytdlp_with_resolver(&state.resolver).await;
    let ffmpeg = polysaver_ffmpeg::probe_ffmpeg_with_resolver(&state.resolver).await;
    let runtime = detect_js_runtime(&state.resolver).await;

    // The UI composes its own localized message from `isReady`/`version`;
    // this field stays a neutral technical description.
    let status_message = match (runtime.kind, &runtime.version) {
        (Some(kind), Some(version)) => format!("{} {version} detected", kind.as_str()),
        _ if runtime.version_too_old => {
            "detected runtime is below the required version".to_string()
        }
        _ => "no JavaScript runtime detected".to_string(),
    };

    let js_runtime = JsRuntimeAvailability {
        is_ready: runtime.is_usable,
        kind: runtime.kind.map(|k| k.as_str().to_string()),
        version: runtime.version,
        binary_path: runtime.path,
        status_message,
    };

    Ok(HealthResponse {
        core_status: "ready".to_string(),
        ytdlp,
        ffmpeg,
        js_runtime,
    })
}
