// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::media_url::MediaUrl;
use crate::domain::probe::ProbeResult;
use crate::error::CoreError;
use async_trait::async_trait;

/// Port for probing and extracting media information.
#[async_trait]
pub trait MediaProvider: Send + Sync {
    /// Probes media metadata for a validated URL.
    async fn probe(&self, url: &MediaUrl) -> Result<ProbeResult, CoreError>;
}
