// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::media_url::MediaUrl;
use crate::error::CoreError;
use crate::ports::playlist_detector::{PlaylistDetection, PlaylistDetector};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Upper bound on a single detection, so a stuck engine call cannot stay pending.
const DETECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Service answering "is this a playlist?" using the engine as the source of truth.
///
/// Detection costs one network round-trip, so it is debounced by the caller (the
/// form) rather than requested on every keystroke, and each new detection cancels
/// the previous one.
pub struct DetectPlaylistService {
    detector: Arc<dyn PlaylistDetector>,
    active: Mutex<Option<CancellationToken>>,
}

impl DetectPlaylistService {
    /// Creates a new detection service with an injected detector.
    pub fn new(detector: Arc<dyn PlaylistDetector>) -> Self {
        Self {
            detector,
            active: Mutex::new(None),
        }
    }

    /// Validates the URL and asks the engine whether it is a playlist.
    ///
    /// Any detection already running is canceled first; the new one is bounded by
    /// [`DETECT_TIMEOUT`] and can be aborted through [`Self::cancel_current`].
    pub async fn detect(&self, raw_url: &str) -> Result<PlaylistDetection, CoreError> {
        let media_url = MediaUrl::parse(raw_url)?;

        let token = {
            let mut active = self.active.lock().await;
            if let Some(previous) = active.take() {
                previous.cancel();
            }
            let token = CancellationToken::new();
            *active = Some(token.clone());
            token
        };

        let detection = self
            .detector
            .detect_playlist(&media_url, Some(token.clone()));
        let result = match tokio::time::timeout(DETECT_TIMEOUT, detection).await {
            Ok(result) => result,
            Err(_) => {
                token.cancel();
                Err(CoreError::ProviderError(
                    "Détection de playlist expirée.".to_string(),
                ))
            }
        };

        // Only clear the slot if it still holds our token; a newer call may have
        // replaced it, and clearing it would then cancel the wrong detection.
        {
            let mut active = self.active.lock().await;
            if let Some(current) = active.as_ref() {
                if current == &token {
                    *active = None;
                }
            }
        }

        result
    }

    /// Cancels the currently running detection, if any, and clears it.
    pub async fn cancel_current(&self) {
        let mut active = self.active.lock().await;
        if let Some(token) = active.take() {
            token.cancel();
        }
    }
}
