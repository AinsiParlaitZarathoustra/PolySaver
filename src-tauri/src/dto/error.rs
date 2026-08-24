// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use polysaver_core::error::{CoreError, DownloadErrorDetails};
use serde::{Deserialize, Serialize};

/// Structured IPC error payload returned to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<DownloadErrorDetails>,
}

impl IpcError {
    /// Creates a generic IPC error.
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: false,
            details: None,
        }
    }
}

impl From<CoreError> for IpcError {
    fn from(err: CoreError) -> Self {
        let details = err.to_download_error_details();
        Self {
            code: details.code.as_str().to_string(),
            message: details.message.clone(),
            retryable: details.retryable,
            details: Some(details),
        }
    }
}
