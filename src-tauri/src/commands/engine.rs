// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::app_state::AppState;
use crate::dto::{EngineUpdateResultDto, EngineUpdateStatusDto, IpcError, JsRuntimeStatusDto};
use polysaver_binres::{BinaryKind, BinaryResolver};
use polysaver_core::domain::engine_version::compare_versions;
use polysaver_core::domain::EngineChannel;
use polysaver_core::error::CoreError;
use polysaver_core::ports::SettingsRepository;
use polysaver_core::services::EngineUpdater;
use polysaver_ytdlp::js_runtime::{detect_js_runtime, JsRuntimeStatus};
use polysaver_ytdlp::node_installer::install_node;
use polysaver_ytdlp::updater::{YtDlpChannel, YtDlpUpdater};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;

/// Engine updater implementation used by the download service retry loop.
///
/// Reads the persisted channel, downloads/verifies/installs the release, then
/// invalidates the resolver cache so the next attempt uses the new binary.
pub struct TauriEngineUpdater {
    app_bin_dir: PathBuf,
    cache_file: PathBuf,
    resolver: Arc<BinaryResolver>,
    settings_repo: Arc<dyn SettingsRepository>,
}

impl TauriEngineUpdater {
    /// Creates a new updater bound to the app-managed binary directory.
    #[must_use]
    pub fn new(
        app_bin_dir: PathBuf,
        cache_file: PathBuf,
        resolver: Arc<BinaryResolver>,
        settings_repo: Arc<dyn SettingsRepository>,
    ) -> Self {
        Self {
            app_bin_dir,
            cache_file,
            resolver,
            settings_repo,
        }
    }
}

#[async_trait::async_trait]
impl EngineUpdater for TauriEngineUpdater {
    async fn update_engine(&self) -> Result<String, CoreError> {
        let channel = match self.settings_repo.load().await {
            Ok(settings) => settings.engine_channel(),
            Err(_) => EngineChannel::Stable,
        };
        let updater = YtDlpUpdater::with_channel(
            self.app_bin_dir.clone(),
            Some(self.cache_file.clone()),
            to_updater_channel(channel),
        )?;
        let outcome = updater.update().await?;
        self.resolver.invalidate(BinaryKind::YtDlp).await;
        Ok(outcome.installed_version)
    }
}

/// Maps the persisted domain channel onto the updater channel.
fn to_updater_channel(channel: EngineChannel) -> YtDlpChannel {
    match channel {
        EngineChannel::Stable => YtDlpChannel::Stable,
        EngineChannel::Nightly => YtDlpChannel::Nightly,
    }
}

/// Resolves the current engine version from the shared binary resolver.
async fn current_engine_version(state: &AppState) -> Result<String, IpcError> {
    let resolved = state
        .resolver
        .resolve_ytdlp()
        .await
        .map_err(|err| IpcError::new("YTDLP_NOT_FOUND", err.to_string()))?;
    Ok(resolved.version)
}

/// Reads the configured channel from persisted settings (stable by default).
async fn configured_channel(state: &AppState) -> Result<EngineChannel, IpcError> {
    let settings = state.settings_repo.load().await.map_err(IpcError::from)?;
    Ok(settings.engine_channel())
}

/// Builds an updater bound to the configured channel.
fn build_updater(state: &AppState, channel: EngineChannel) -> Result<YtDlpUpdater, IpcError> {
    YtDlpUpdater::with_channel(
        state.app_bin_dir.clone(),
        Some(state.engine_update_cache_file.clone()),
        to_updater_channel(channel),
    )
    .map_err(IpcError::from)
}

/// Path of the rollback copy of the previous engine binary.
fn previous_binary_path(app_bin_dir: &Path) -> std::path::PathBuf {
    app_bin_dir.join("yt-dlp.previous")
}

/// IPC command checking whether a newer yt-dlp release is available (no download).
///
/// The age-based signal is deterministic: when the remote lookup is unavailable
/// (offline, rate-limited), the status still reports obsolescence computed from
/// the local version date.
#[tauri::command]
pub async fn check_engine_update(
    state: State<'_, AppState>,
) -> Result<EngineUpdateStatusDto, IpcError> {
    let current_version = current_engine_version(&state).await?;
    let channel = configured_channel(&state).await?;
    let updater = build_updater(&state, channel)?;

    let status = match updater.check_update(Some(&current_version)).await {
        Ok(status) => EngineUpdateStatusDto {
            current_version: status.current_version,
            latest_version: status.latest_version,
            channel: status.channel,
            outdated: status.outdated,
            can_update: status.can_update,
            can_rollback: previous_binary_path(&state.app_bin_dir).is_file(),
        },
        Err(_) => {
            use polysaver_core::domain::engine_version::{current_utc_date, is_version_outdated};
            EngineUpdateStatusDto {
                current_version: Some(current_version.clone()),
                latest_version: None,
                channel: updater.channel().as_str().to_string(),
                outdated: is_version_outdated(&current_version, current_utc_date()),
                can_update: false,
                can_rollback: previous_binary_path(&state.app_bin_dir).is_file(),
            }
        }
    };

    Ok(status)
}

/// Returns the number of jobs that are not in a terminal state.
fn active_job_count(jobs: &[polysaver_core::domain::DownloadJob]) -> usize {
    jobs.iter()
        .filter(|job| {
            !matches!(
                job.status(),
                polysaver_core::domain::DownloadStatus::Completed
                    | polysaver_core::domain::DownloadStatus::Failed
                    | polysaver_core::domain::DownloadStatus::Canceled
            )
        })
        .count()
}

/// Returns true when the installed engine already matches the remote release.
///
/// The comparison is strictly by version, never by age. A local build older than
/// 90 days is only an advisory warning shown in the UI (`outdated`); it must not
/// trigger a re-download of an identical ~40 MB release.
fn engine_is_current(current: &str, latest: Option<&str>) -> bool {
    match latest {
        Some(latest) => matches!(
            compare_versions(current, latest),
            Some(std::cmp::Ordering::Equal)
        ),
        // Unknown remote version: cannot claim we are up to date.
        None => false,
    }
}

/// IPC command downloading, verifying, and installing the configured engine release.
///
/// Refused while download jobs are active. When the installed version already
/// matches the remote one, answers "already up to date" without writing.
/// The resolver cache is invalidated afterwards so new operations use the new binary.
#[tauri::command]
pub async fn update_engine(state: State<'_, AppState>) -> Result<EngineUpdateResultDto, IpcError> {
    let active = active_job_count(&state.start_download_service.list_downloads().await);
    if active > 0 {
        return Err(IpcError::new(
            "ACTIVE_DOWNLOADS",
            "Impossible de mettre à jour le moteur pendant un téléchargement actif.",
        ));
    }

    let channel = configured_channel(&state).await?;
    let updater = build_updater(&state, channel)?;
    let current_version = current_engine_version(&state).await.unwrap_or_default();

    // Skip the ~40 MB download when the remote release is the one we already run.
    // Note: `status.outdated` is deliberately ignored here — it reflects the
    // advisory 90-day age warning, not the need for a download.
    if let Ok(status) = updater.check_update(Some(&current_version)).await {
        if engine_is_current(&current_version, status.latest_version.as_deref()) {
            return Ok(EngineUpdateResultDto {
                installed_version: current_version,
                updated: false,
                latest_version: status.latest_version,
            });
        }
    }

    let outcome = updater.update().await.map_err(IpcError::from)?;
    state.resolver.invalidate(BinaryKind::YtDlp).await;

    Ok(EngineUpdateResultDto {
        installed_version: outcome.installed_version,
        updated: outcome.updated,
        latest_version: None,
    })
}

/// IPC command restoring the engine binary backed up before the last update.
#[tauri::command]
pub async fn rollback_engine(
    state: State<'_, AppState>,
) -> Result<EngineUpdateResultDto, IpcError> {
    let active = active_job_count(&state.start_download_service.list_downloads().await);
    if active > 0 {
        return Err(IpcError::new(
            "ACTIVE_DOWNLOADS",
            "Impossible de restaurer le moteur pendant un téléchargement actif.",
        ));
    }

    let previous = previous_binary_path(&state.app_bin_dir);
    if !previous.is_file() {
        return Err(IpcError::new(
            "NO_PREVIOUS_ENGINE",
            "Aucune version précédente du moteur n'est disponible.",
        ));
    }

    let target = state.app_bin_dir.join(if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    });

    // Rename the current binary aside, then restore the backup (works on Windows too).
    let current_backup = state.app_bin_dir.join("yt-dlp.rollback_tmp");
    let _ = std::fs::remove_file(&current_backup);
    if target.exists() {
        std::fs::rename(&target, &current_backup).map_err(|err| {
            IpcError::new("ROLLBACK_FAILED", format!("Échec de la sauvegarde: {err}"))
        })?;
    }
    match std::fs::rename(&previous, &target) {
        Ok(()) => {
            let _ = std::fs::remove_file(&current_backup);
        }
        Err(err) => {
            // Restore the working binary so the app never ends up without an engine.
            let _ = std::fs::rename(&current_backup, &target);
            return Err(IpcError::new(
                "ROLLBACK_FAILED",
                format!("Échec de la restauration: {err}"),
            ));
        }
    }

    state.resolver.invalidate(BinaryKind::YtDlp).await;
    let installed_version = current_engine_version(&state).await.unwrap_or_default();

    Ok(EngineUpdateResultDto {
        installed_version,
        updated: true,
        latest_version: None,
    })
}

/// Builds the UI-facing DTO from a detection result.
fn js_status_dto(status: &JsRuntimeStatus) -> JsRuntimeStatusDto {
    JsRuntimeStatusDto {
        kind: status.kind.map(|k| k.as_str().to_string()),
        version: status.version.clone(),
        path: status.path.clone(),
        is_ready: status.is_usable,
        version_too_old: status.version_too_old,
    }
}

/// IPC command detecting the available JavaScript runtime (Deno preferred, then Node).
#[tauri::command]
pub async fn check_js_runtime(state: State<'_, AppState>) -> Result<JsRuntimeStatusDto, IpcError> {
    let status = detect_js_runtime(&state.resolver).await;
    Ok(js_status_dto(&status))
}

/// IPC command installing the pinned Node.js runtime into the app data directory.
///
/// Refused while downloads are active, and the resolver cache is invalidated
/// afterwards so the next probe/download uses the freshly installed runtime.
#[tauri::command]
pub async fn install_js_runtime(
    state: State<'_, AppState>,
) -> Result<JsRuntimeStatusDto, IpcError> {
    let active = active_job_count(&state.start_download_service.list_downloads().await);
    if active > 0 {
        return Err(IpcError::new(
            "ACTIVE_DOWNLOADS",
            "Impossible d'installer le runtime pendant un téléchargement actif.",
        ));
    }

    let outcome = install_node(&state.app_bin_dir)
        .await
        .map_err(IpcError::from)?;

    state.resolver.invalidate(BinaryKind::Node).await;

    // Make the fresh runtime immediately usable by yt-dlp.
    let status = detect_js_runtime(&state.resolver).await;
    state
        .ytdlp_downloader
        .set_js_runtime(status.to_spec())
        .await;

    Ok(JsRuntimeStatusDto {
        kind: Some("node".to_string()),
        version: Some(outcome.version),
        path: Some(outcome.path.to_string_lossy().to_string()),
        is_ready: true,
        version_too_old: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use polysaver_core::domain::{DownloadJob, DownloadPreset, DownloadStatus, MediaUrl};
    use polysaver_core::error::{DownloadErrorCode, DownloadErrorDetails};

    #[test]
    fn test_active_job_count_ignores_terminal_jobs() {
        let url = MediaUrl::parse("https://example.com/video").unwrap();
        let preset = DownloadPreset::default();

        let queued = DownloadJob::new(url.clone(), preset);
        let mut downloading = DownloadJob::new(url.clone(), preset);
        downloading.transition_to_preparing().unwrap();
        downloading.transition_to_downloading().unwrap();

        let mut completed = DownloadJob::new(url.clone(), preset);
        completed.transition_to_preparing().unwrap();
        completed.transition_to_downloading().unwrap();
        completed
            .transition_to_completed("/tmp/file.mp4".to_string())
            .unwrap();

        let mut failed = DownloadJob::new(url.clone(), preset);
        failed
            .transition_to_failed(DownloadErrorDetails::from_code(
                DownloadErrorCode::NetworkUnavailable,
            ))
            .unwrap();

        let mut canceled = DownloadJob::new(url, preset);
        canceled.transition_to_canceled().unwrap();

        assert_eq!(active_job_count(&[]), 0);
        assert_eq!(active_job_count(std::slice::from_ref(&completed)), 0);
        assert_eq!(active_job_count(&[completed, failed]), 0);
        assert_eq!(active_job_count(&[canceled]), 0);

        // Any non-terminal job blocks an engine update.
        assert_eq!(active_job_count(std::slice::from_ref(&queued)), 1);
        assert_eq!(active_job_count(std::slice::from_ref(&downloading)), 1);
        assert_eq!(active_job_count(&[queued.clone(), downloading]), 2);
        assert_eq!(queued.status(), DownloadStatus::Queued);
    }

    /// An identical remote version must never trigger a re-download, even when
    /// the local build is older than the advisory 90-day threshold.
    #[test]
    fn test_engine_is_current_compares_versions_not_age() {
        // Same version: nothing to do (this is the regression the test pins).
        assert!(engine_is_current("2026.08.19", Some("2026.08.19")));
        // Trailing revision components compare as zero-padded on the right.
        assert!(!engine_is_current("2026.08.19", Some("2026.08.19.1")));
        assert!(!engine_is_current("2026.08.19", Some("2026.09.01")));
        // A nightly build newer than stable is not "current" either.
        assert!(!engine_is_current("2026.08.19", Some("2026.08.30.232658")));
        assert!(engine_is_current(
            "2026.08.30.232658",
            Some("2026.08.30.232658")
        ));
        // Unknown remote version means we cannot claim to be up to date.
        assert!(!engine_is_current("2026.08.19", None));
        // Unparseable versions are never treated as equal.
        assert!(!engine_is_current("nightly", Some("nightly")));
    }
}
