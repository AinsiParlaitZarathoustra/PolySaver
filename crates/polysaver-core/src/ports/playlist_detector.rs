// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::media_url::MediaUrl;
use crate::error::CoreError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Native answer to "is this URL a playlist?".
///
/// This is deliberately distinct from the offline shape heuristic
/// (`MediaUrlKind`): only the download engine itself knows what an URL really
/// points at, and it can answer that question without downloading anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistDetection {
    /// True when the engine reports a playlist/multi-video listing.
    pub is_playlist: bool,
}

/// Port for asking the media engine whether an URL is a playlist.
#[async_trait]
pub trait PlaylistDetector: Send + Sync {
    /// Performs a cheap detection that never downloads media.
    ///
    /// `cancellation_token` aborts the underlying engine process as soon as the
    /// answer is no longer needed (for example when the user keeps typing).
    async fn detect_playlist(
        &self,
        url: &MediaUrl,
        cancellation_token: Option<tokio_util::sync::CancellationToken>,
    ) -> Result<PlaylistDetection, CoreError>;
}
