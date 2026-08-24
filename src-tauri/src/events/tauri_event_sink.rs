// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::dto::media::DownloadJobDto;
use polysaver_core::domain::DownloadJob;
use polysaver_core::error::DownloadErrorDetails;
use polysaver_core::ports::event_sink::{DownloadProgressEvent, EventSink};
use tauri::{AppHandle, Emitter};

/// Tauri v2 implementation of the sovereign EventSink port.
#[derive(Clone)]
pub struct TauriEventSink {
    app_handle: AppHandle,
}

impl TauriEventSink {
    #[must_use]
    pub fn new(app_handle: AppHandle) -> Self {
        Self { app_handle }
    }
}

impl EventSink for TauriEventSink {
    fn emit_queued(&self, job: &DownloadJob) {
        let dto = DownloadJobDto::from(job);
        let _ = self.app_handle.emit("download://queued", dto);
    }

    fn emit_progress(&self, progress: &DownloadProgressEvent) {
        let _ = self.app_handle.emit("download://progress", progress);
    }

    fn emit_completed(&self, job: &DownloadJob) {
        let dto = DownloadJobDto::from(job);
        let _ = self.app_handle.emit("download://completed", dto);
    }

    fn emit_failed(&self, job: &DownloadJob, _error: &DownloadErrorDetails) {
        let dto = DownloadJobDto::from(job);
        let _ = self.app_handle.emit("download://failed", dto);
    }

    fn emit_canceled(&self, job: &DownloadJob) {
        let dto = DownloadJobDto::from(job);
        let _ = self.app_handle.emit("download://canceled", dto);
    }

    fn emit_warning(&self, warning: &polysaver_core::ports::event_sink::DownloadWarningEvent) {
        let _ = self.app_handle.emit("download://warning", warning);
    }
}
