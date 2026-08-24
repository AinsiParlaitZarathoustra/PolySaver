// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::download_job::{DownloadId, DownloadJob};
use crate::error::DownloadErrorDetails;
use serde::{Deserialize, Serialize};

/// High-level execution phase of a download job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProgressPhase {
    Preparing,
    Probing,
    Downloading,
    Converting,
    Finalizing,
}

/// Typed real-time progress event emitted via IPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgressEvent {
    pub download_id: DownloadId,
    pub phase: ProgressPhase,
    pub percent: Option<u8>,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub speed_bytes_per_second: Option<u64>,
}

/// Typed warning event emitted when non-fatal operations encounter an issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadWarningEvent {
    pub download_id: DownloadId,
    pub code: String,
    pub message: String,
}

/// Sink for publishing real-time domain events to the outside world.
pub trait EventSink: Send + Sync {
    /// Emits when a job is added to the queue.
    fn emit_queued(&self, job: &DownloadJob);

    /// Emits fine-grained progress updates.
    fn emit_progress(&self, progress: &DownloadProgressEvent);

    /// Emits when a job finishes successfully.
    fn emit_completed(&self, job: &DownloadJob);

    /// Emits when a job fails with structured error details.
    fn emit_failed(&self, job: &DownloadJob, error: &DownloadErrorDetails);

    /// Emits when a job is canceled.
    fn emit_canceled(&self, job: &DownloadJob);

    /// Emits when a non-fatal warning occurs during execution.
    fn emit_warning(&self, _warning: &DownloadWarningEvent) {}
}
