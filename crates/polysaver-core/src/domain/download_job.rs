// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::format::DownloadPreset;
use crate::domain::media_url::MediaUrl;
use crate::error::{CoreError, DownloadErrorDetails};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Strongly-typed identifier for a download job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DownloadId(Uuid);

impl DownloadId {
    /// Generates a new random UUID v4 identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates a typed identifier from an existing UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Access the underlying UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for DownloadId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for DownloadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<&str> for DownloadId {
    type Error = CoreError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let uuid = Uuid::parse_str(value).map_err(|err| {
            CoreError::InvalidSettings(format!("Invalid DownloadId UUID '{value}': {err}"))
        })?;
        Ok(Self(uuid))
    }
}

impl TryFrom<String> for DownloadId {
    type Error = CoreError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

/// Lifecycle status of a download job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadStatus {
    Queued,
    Preparing,
    Probing,
    Downloading,
    Converting,
    Finalizing,
    Completed,
    Failed,
    Canceled,
}

/// Sovereign domain model of a download job.
/// All fields are private to guarantee invariant enforcement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DownloadJob {
    id: DownloadId,
    url: MediaUrl,
    preset: DownloadPreset,
    title: Option<String>,
    status: DownloadStatus,
    progress_percent: Option<u8>,
    downloaded_bytes: Option<u64>,
    total_bytes: Option<u64>,
    speed_bytes_per_second: Option<u64>,
    destination_path: Option<String>,
    error_message: Option<String>,
    error_details: Option<DownloadErrorDetails>,
}

impl DownloadJob {
    /// Creates a new job in the `Queued` state.
    #[must_use]
    pub fn new(url: MediaUrl, preset: DownloadPreset) -> Self {
        Self {
            id: DownloadId::new(),
            url,
            preset,
            title: None,
            status: DownloadStatus::Queued,
            progress_percent: None,
            downloaded_bytes: None,
            total_bytes: None,
            speed_bytes_per_second: None,
            destination_path: None,
            error_message: None,
            error_details: None,
        }
    }

    /// Read-only accessors
    #[must_use]
    pub const fn id(&self) -> DownloadId {
        self.id
    }

    #[must_use]
    pub const fn url(&self) -> &MediaUrl {
        &self.url
    }

    #[must_use]
    pub const fn preset(&self) -> DownloadPreset {
        self.preset
    }

    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    #[must_use]
    pub const fn status(&self) -> DownloadStatus {
        self.status
    }

    #[must_use]
    pub const fn progress_percent(&self) -> Option<u8> {
        self.progress_percent
    }

    #[must_use]
    pub const fn downloaded_bytes(&self) -> Option<u64> {
        self.downloaded_bytes
    }

    #[must_use]
    pub const fn total_bytes(&self) -> Option<u64> {
        self.total_bytes
    }

    #[must_use]
    pub const fn speed_bytes_per_second(&self) -> Option<u64> {
        self.speed_bytes_per_second
    }

    #[must_use]
    pub fn destination_path(&self) -> Option<&str> {
        self.destination_path.as_deref()
    }

    #[must_use]
    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    #[must_use]
    pub const fn error_details(&self) -> Option<&DownloadErrorDetails> {
        self.error_details.as_ref()
    }

    /// Returns whether the job is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            DownloadStatus::Completed | DownloadStatus::Failed | DownloadStatus::Canceled
        )
    }

    /// Sets the known media title.
    pub fn set_title(&mut self, title: String) {
        self.title = Some(title);
    }

    /// Transitions to `Preparing`. Valid from `Queued`.
    pub fn transition_to_preparing(&mut self) -> Result<(), CoreError> {
        self.ensure_not_terminal("transition_to_preparing")?;
        if self.status != DownloadStatus::Queued {
            return Err(CoreError::IllegalTransition {
                current: format!("{:?}", self.status),
                attempted: "Preparing".to_string(),
                reason: "Can only prepare a queued job".to_string(),
            });
        }
        self.status = DownloadStatus::Preparing;
        self.progress_percent = None;
        self.downloaded_bytes = None;
        self.total_bytes = None;
        self.speed_bytes_per_second = None;
        Ok(())
    }

    /// Transitions to `Probing`. Valid from `Queued` or `Preparing`.
    pub fn transition_to_probing(&mut self) -> Result<(), CoreError> {
        self.ensure_not_terminal("transition_to_probing")?;
        if !matches!(
            self.status,
            DownloadStatus::Queued | DownloadStatus::Preparing
        ) {
            return Err(CoreError::IllegalTransition {
                current: format!("{:?}", self.status),
                attempted: "Probing".to_string(),
                reason: "Can only probe a queued or preparing job".to_string(),
            });
        }
        self.status = DownloadStatus::Probing;
        self.progress_percent = None;
        self.downloaded_bytes = None;
        self.total_bytes = None;
        self.speed_bytes_per_second = None;
        Ok(())
    }

    /// Transitions to `Downloading`. Valid from `Probing` or `Preparing` or `Queued`.
    pub fn transition_to_downloading(&mut self) -> Result<(), CoreError> {
        self.ensure_not_terminal("transition_to_downloading")?;
        if !matches!(
            self.status,
            DownloadStatus::Queued | DownloadStatus::Preparing | DownloadStatus::Probing
        ) {
            return Err(CoreError::IllegalTransition {
                current: format!("{:?}", self.status),
                attempted: "Downloading".to_string(),
                reason: "Job must be Queued, Preparing or Probing to start Downloading".to_string(),
            });
        }
        self.status = DownloadStatus::Downloading;
        self.progress_percent = Some(0);
        self.downloaded_bytes = None;
        self.total_bytes = None;
        self.speed_bytes_per_second = None;
        Ok(())
    }

    /// Updates download progress during `Downloading`.
    /// Progress must be monotonically increasing (0..=100).
    pub fn update_progress(
        &mut self,
        percent: Option<u8>,
        downloaded: Option<u64>,
        total: Option<u64>,
        speed: Option<u64>,
    ) -> Result<(), CoreError> {
        self.ensure_not_terminal("update_progress")?;
        if self.status != DownloadStatus::Downloading {
            return Err(CoreError::IllegalTransition {
                current: format!("{:?}", self.status),
                attempted: "update_progress".to_string(),
                reason: "Can only update progress during Downloading".to_string(),
            });
        }

        if let Some(p) = percent {
            if p > 100 {
                return Err(CoreError::InvalidProgress {
                    current: self.progress_percent.unwrap_or(0),
                    attempted: p,
                });
            }

            if let Some(current) = self.progress_percent {
                if p < current {
                    return Err(CoreError::InvalidProgress {
                        current,
                        attempted: p,
                    });
                }
            }
            self.progress_percent = Some(p);
        }

        self.downloaded_bytes = downloaded;
        self.total_bytes = total;
        self.speed_bytes_per_second = speed;
        Ok(())
    }

    /// Transitions to `Converting`. Valid from `Downloading`.
    pub fn transition_to_converting(&mut self) -> Result<(), CoreError> {
        self.ensure_not_terminal("transition_to_converting")?;
        if self.status != DownloadStatus::Downloading {
            return Err(CoreError::IllegalTransition {
                current: format!("{:?}", self.status),
                attempted: "Converting".to_string(),
                reason: "Can only convert after Downloading".to_string(),
            });
        }
        self.status = DownloadStatus::Converting;
        self.progress_percent = None;
        self.downloaded_bytes = None;
        self.total_bytes = None;
        self.speed_bytes_per_second = None;
        Ok(())
    }

    /// Transitions to `Finalizing`. Valid from `Downloading` or `Converting`.
    pub fn transition_to_finalizing(&mut self) -> Result<(), CoreError> {
        self.ensure_not_terminal("transition_to_finalizing")?;
        if !matches!(
            self.status,
            DownloadStatus::Downloading | DownloadStatus::Converting
        ) {
            return Err(CoreError::IllegalTransition {
                current: format!("{:?}", self.status),
                attempted: "Finalizing".to_string(),
                reason: "Can only finalize after Downloading or Converting".to_string(),
            });
        }
        self.status = DownloadStatus::Finalizing;
        self.progress_percent = None;
        self.downloaded_bytes = None;
        self.total_bytes = None;
        self.speed_bytes_per_second = None;
        Ok(())
    }

    /// Transitions to `Completed` (terminal). Valid from `Downloading`, `Converting` or `Finalizing`.
    pub fn transition_to_completed(&mut self, destination_path: String) -> Result<(), CoreError> {
        self.ensure_not_terminal("transition_to_completed")?;
        if !matches!(
            self.status,
            DownloadStatus::Downloading | DownloadStatus::Converting | DownloadStatus::Finalizing
        ) {
            return Err(CoreError::IllegalTransition {
                current: format!("{:?}", self.status),
                attempted: "Completed".to_string(),
                reason: "Job must be Downloading, Converting or Finalizing to complete".to_string(),
            });
        }
        self.status = DownloadStatus::Completed;
        self.progress_percent = None;
        self.downloaded_bytes = None;
        self.total_bytes = None;
        self.speed_bytes_per_second = None;
        self.destination_path = Some(destination_path);
        self.error_message = None;
        self.error_details = None;
        Ok(())
    }

    /// Transitions to `Failed` (terminal) with structured error details.
    pub fn transition_to_failed(&mut self, error: DownloadErrorDetails) -> Result<(), CoreError> {
        self.ensure_not_terminal("transition_to_failed")?;
        self.status = DownloadStatus::Failed;
        self.error_message = Some(error.message.clone());
        self.error_details = Some(error);
        self.progress_percent = None;
        self.downloaded_bytes = None;
        self.total_bytes = None;
        self.speed_bytes_per_second = None;
        Ok(())
    }

    /// Transitions to `Canceled` (terminal). Valid from any non-terminal state.
    pub fn transition_to_canceled(&mut self) -> Result<(), CoreError> {
        self.ensure_not_terminal("transition_to_canceled")?;
        self.status = DownloadStatus::Canceled;
        self.progress_percent = None;
        self.downloaded_bytes = None;
        self.total_bytes = None;
        self.speed_bytes_per_second = None;
        Ok(())
    }

    fn ensure_not_terminal(&self, attempted: &str) -> Result<(), CoreError> {
        if self.is_terminal() {
            return Err(CoreError::IllegalTransition {
                current: format!("{:?}", self.status),
                attempted: attempted.to_string(),
                reason: "Cannot mutate a terminal download job".to_string(),
            });
        }
        Ok(())
    }
}
