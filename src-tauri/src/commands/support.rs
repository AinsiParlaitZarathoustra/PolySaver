// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::dto::IpcError;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

pub const SUPPORT_ISSUES_URL: &str = "https://github.com/AinsiParlaitZarathoustra/PolySaver/issues";

/// Opens the official PolySaver GitHub Issues page.
///
/// Invariant: Accepts no external URL parameter to guarantee that only the
/// hardcoded, validated repository issues URL can be opened.
#[tauri::command]
pub async fn open_support_page(app: AppHandle) -> Result<(), IpcError> {
    app.opener()
        .open_url(SUPPORT_ISSUES_URL, None::<&str>)
        .map_err(|err| {
            IpcError::new(
                "SUPPORT_PAGE_OPEN_FAILED",
                format!("Failed to open support page: {err}"),
            )
        })
}
