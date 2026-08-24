// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::media_url::MediaUrl;
use crate::domain::probe::ProbeResult;
use crate::error::CoreError;
use crate::ports::media_provider::MediaProvider;
use std::sync::Arc;

/// Sovereign use case service for analyzing media URLs.
/// Receives raw untrusted string, parses and validates MediaUrl, and calls injected provider.
pub struct AnalyzeUrlService {
    provider: Arc<dyn MediaProvider>,
}

impl AnalyzeUrlService {
    /// Creates a new analyze URL service with an injected media provider.
    pub fn new(provider: Arc<dyn MediaProvider>) -> Self {
        Self { provider }
    }

    /// Validates raw URL and retrieves metadata via provider.
    pub async fn analyze(&self, raw_url: &str) -> Result<ProbeResult, CoreError> {
        let media_url = MediaUrl::parse(raw_url)?;
        self.provider.probe(&media_url).await
    }
}
