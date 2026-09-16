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
    ///
    /// `cancellation_token` allows the caller to abort the underlying probe
    /// (e.g. killing the external process) as soon as the operation becomes moot.
    async fn probe(
        &self,
        url: &MediaUrl,
        cancellation_token: Option<tokio_util::sync::CancellationToken>,
    ) -> Result<ProbeResult, CoreError>;
}
