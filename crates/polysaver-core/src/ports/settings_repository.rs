// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::settings::AppSettings;
use crate::error::CoreError;
use async_trait::async_trait;

/// Port for persisting and loading application settings.
#[async_trait]
pub trait SettingsRepository: Send + Sync {
    /// Loads the stored application settings.
    async fn load(&self) -> Result<AppSettings, CoreError>;

    /// Persists application settings.
    async fn save(&self, settings: &AppSettings) -> Result<(), CoreError>;
}
