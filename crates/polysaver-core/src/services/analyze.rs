// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::media_url::MediaUrl;
use crate::domain::probe::ProbeResult;
use crate::error::CoreError;
use crate::ports::media_provider::MediaProvider;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Hard upper bound on a single URL analysis, so a stuck probe can never hang the UI.
const ANALYZE_TIMEOUT: Duration = Duration::from_secs(45);

/// Playlist enumeration pages through the listing and can legitimately take longer
/// than a single-video probe.
const PLAYLIST_ANALYZE_TIMEOUT: Duration = Duration::from_secs(60);

/// Sovereign use case service for analyzing media URLs.
/// Receives raw untrusted string, parses and validates MediaUrl, and calls injected provider.
pub struct AnalyzeUrlService {
    provider: Arc<dyn MediaProvider>,
    active: Mutex<Option<CancellationToken>>,
}

impl AnalyzeUrlService {
    /// Creates a new analyze URL service with an injected media provider.
    pub fn new(provider: Arc<dyn MediaProvider>) -> Self {
        Self {
            provider,
            active: Mutex::new(None),
        }
    }

    /// Validates raw URL and retrieves metadata via provider.
    ///
    /// Any analysis already running is canceled first; the new analysis is bounded
    /// by `ANALYZE_TIMEOUT` and can be aborted at any time via [`Self::cancel_current`].
    pub async fn analyze(&self, raw_url: &str) -> Result<ProbeResult, CoreError> {
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

        let timeout = if media_url.is_playlist() {
            PLAYLIST_ANALYZE_TIMEOUT
        } else {
            ANALYZE_TIMEOUT
        };

        let probe = self.provider.probe(&media_url, Some(token.clone()));
        let result = match tokio::time::timeout(timeout, probe).await {
            Ok(result) => result,
            Err(_) => {
                token.cancel();
                Err(CoreError::ProviderError(format!(
                    "Analyse expirée: délai de {} secondes dépassé",
                    timeout.as_secs()
                )))
            }
        };

        // Only clear the slot if it still holds our token; a newer analysis may have replaced it.
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

    /// Cancels the currently running analysis, if any, and clears it.
    pub async fn cancel_current(&self) {
        let mut active = self.active.lock().await;
        if let Some(token) = active.take() {
            token.cancel();
        }
    }
}
