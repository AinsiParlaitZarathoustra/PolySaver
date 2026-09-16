// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::dto::IpcError;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

pub const SUPPORT_ISSUES_URL: &str = "https://github.com/AinsiParlaitZarathoustra/PolySaver/issues";

pub const CONTACT_EMAIL: &str = "git.ainsiparlaitzarathoustra@gmail.com";

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

/// Opens the default mail client to contact the PolySaver developer.
///
/// Invariant: Accepts no external recipient parameter to guarantee that only the
/// hardcoded, validated contact address can be opened.
#[tauri::command]
pub async fn open_contact_email(app: AppHandle) -> Result<(), IpcError> {
    let url = format!("mailto:{CONTACT_EMAIL}");
    app.opener().open_url(url, None::<&str>).map_err(|err| {
        IpcError::new(
            "CONTACT_EMAIL_OPEN_FAILED",
            format!("Failed to open mail client: {err}"),
        )
    })
}
