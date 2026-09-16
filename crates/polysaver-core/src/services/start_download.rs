// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::download_job::{DownloadId, DownloadJob, DownloadStatus};
use crate::domain::format::DownloadPreset;
use crate::domain::history::{DownloadHistoryEntry, HistoryEntryId};
use crate::domain::media_url::MediaUrl;
use crate::domain::probe::MAX_PLAYLIST_DOWNLOADS;
use crate::error::{CoreError, DownloadErrorCode, DownloadErrorDetails};
use crate::ports::converter::{AudioCodec, ConvertRequest, MediaConverter};
use crate::ports::event_sink::{DownloadProgressEvent, EventSink, ProgressPhase};
use crate::ports::history_repository::DownloadHistoryRepository;
use crate::ports::media_downloader::{DownloadStreamRequest, MediaDownloader, StreamProgress};
use crate::ports::media_inspector::MediaInspector;
use crate::ports::settings_repository::SettingsRepository;
use crate::services::limiter::ConcurrencyLimiter;
use crate::services::retry::RetryPolicy;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

const MAX_BASE_FILENAME_BYTES: usize = 180;

/// Fixed internal cap on simultaneous download jobs (not user-configurable).
const DEFAULT_MAX_CONCURRENT: usize = 3;

const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Resolves the destination directory for a job, applying the same validation to
/// every entry point (single download, batch download, retry).
fn resolve_target_dir(
    settings: &crate::domain::AppSettings,
    custom_output_dir: Option<PathBuf>,
) -> Result<PathBuf, CoreError> {
    let default_dir = PathBuf::from(settings.download_directory());

    let Some(custom_dir) = custom_output_dir else {
        return Ok(default_dir);
    };

    let path_str = custom_dir.to_string_lossy().trim().to_string();
    if path_str.is_empty() {
        return Ok(default_dir);
    }
    if path_str.starts_with('~') {
        return Err(CoreError::InvalidSettings(
            "Custom download directory cannot start with '~'".to_string(),
        ));
    }
    if path_str.contains('\0') {
        return Err(CoreError::InvalidSettings(
            "Custom download directory cannot contain null bytes".to_string(),
        ));
    }
    if !custom_dir.is_absolute() {
        return Err(CoreError::InvalidSettings(format!(
            "Custom download directory must be an absolute path: '{path_str}'"
        )));
    }
    Ok(custom_dir)
}

/// Execution context encapsulating all runtime dependencies for the download pipeline.
struct PipelineContext {
    job_id: DownloadId,
    url: MediaUrl,
    preset: DownloadPreset,
    target_dir: PathBuf,
    temp_root: PathBuf,
    downloader: Arc<dyn MediaDownloader>,
    converter: Arc<dyn MediaConverter>,
    inspector: Arc<dyn MediaInspector>,
    history_repo: Arc<dyn DownloadHistoryRepository>,
    active_jobs: Arc<Mutex<HashMap<DownloadId, DownloadJob>>>,
    event_sink: Option<Arc<dyn EventSink>>,
    publish_mutex: Arc<Mutex<()>>,
    cancel_token: CancellationToken,
}

/// Immutable description of a job attempt, cloned into a fresh [`PipelineContext`]
/// for every retry (the pipeline consumes its context by value).
#[derive(Clone)]
struct AttemptSpec {
    job_id: DownloadId,
    url: MediaUrl,
    preset: DownloadPreset,
    target_dir: PathBuf,
    temp_root: PathBuf,
    downloader: Arc<dyn MediaDownloader>,
    converter: Arc<dyn MediaConverter>,
    inspector: Arc<dyn MediaInspector>,
    history_repo: Arc<dyn DownloadHistoryRepository>,
    active_jobs: Arc<Mutex<HashMap<DownloadId, DownloadJob>>>,
    event_sink: Option<Arc<dyn EventSink>>,
    publish_mutex: Arc<Mutex<()>>,
    cancel_token: CancellationToken,
}

impl AttemptSpec {
    fn to_context(&self) -> PipelineContext {
        PipelineContext {
            job_id: self.job_id,
            url: self.url.clone(),
            preset: self.preset,
            target_dir: self.target_dir.clone(),
            temp_root: self.temp_root.clone(),
            downloader: Arc::clone(&self.downloader),
            converter: Arc::clone(&self.converter),
            inspector: Arc::clone(&self.inspector),
            history_repo: Arc::clone(&self.history_repo),
            active_jobs: Arc::clone(&self.active_jobs),
            event_sink: self.event_sink.clone(),
            publish_mutex: Arc::clone(&self.publish_mutex),
            cancel_token: self.cancel_token.clone(),
        }
    }
}

/// Service orchestrating the complete download pipeline from URL to finalized media file.
pub struct StartDownloadService {
    downloader: Arc<dyn MediaDownloader>,
    converter: Arc<dyn MediaConverter>,
    inspector: Arc<dyn MediaInspector>,
    settings_repo: Arc<dyn SettingsRepository>,
    history_repo: Arc<dyn DownloadHistoryRepository>,
    event_sink: Option<Arc<dyn EventSink>>,
    temp_root: PathBuf,
    active_jobs: Arc<Mutex<HashMap<DownloadId, DownloadJob>>>,
    limiter: ConcurrencyLimiter,
    cancellation_tokens: Arc<Mutex<HashMap<DownloadId, CancellationToken>>>,
    publish_mutex: Arc<Mutex<()>>,
    retry_policy: RetryPolicy,
    engine_updater: Option<Arc<dyn EngineUpdater>>,
}

/// Port used to refresh the download engine after an `YtdlpUpdateRequired` failure.
#[async_trait::async_trait]
pub trait EngineUpdater: Send + Sync {
    /// Downloads and installs the latest engine build. Implementations are
    /// expected to be idempotent and to fail cleanly when offline.
    async fn update_engine(&self) -> Result<String, CoreError>;
}

impl StartDownloadService {
    /// Creates a new `StartDownloadService` with the default retry policy.
    #[must_use]
    pub fn new(
        downloader: Arc<dyn MediaDownloader>,
        converter: Arc<dyn MediaConverter>,
        inspector: Arc<dyn MediaInspector>,
        settings_repo: Arc<dyn SettingsRepository>,
        history_repo: Arc<dyn DownloadHistoryRepository>,
        event_sink: Option<Arc<dyn EventSink>>,
        temp_root: PathBuf,
    ) -> Self {
        Self::new_with_retry_policy(
            downloader,
            converter,
            inspector,
            settings_repo,
            history_repo,
            event_sink,
            temp_root,
            RetryPolicy::default(),
        )
    }

    /// Creates a new `StartDownloadService` with an injected retry policy.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_retry_policy(
        downloader: Arc<dyn MediaDownloader>,
        converter: Arc<dyn MediaConverter>,
        inspector: Arc<dyn MediaInspector>,
        settings_repo: Arc<dyn SettingsRepository>,
        history_repo: Arc<dyn DownloadHistoryRepository>,
        event_sink: Option<Arc<dyn EventSink>>,
        temp_root: PathBuf,
        retry_policy: RetryPolicy,
    ) -> Self {
        Self {
            downloader,
            converter,
            inspector,
            settings_repo,
            history_repo,
            event_sink,
            temp_root,
            active_jobs: Arc::new(Mutex::new(HashMap::new())),
            limiter: ConcurrencyLimiter::new(DEFAULT_MAX_CONCURRENT),
            cancellation_tokens: Arc::new(Mutex::new(HashMap::new())),
            publish_mutex: Arc::new(Mutex::new(())),
            retry_policy,
            engine_updater: None,
        }
    }

    /// Injects the engine updater used by the single post-update retry path.
    #[must_use]
    pub fn with_engine_updater(mut self, updater: Arc<dyn EngineUpdater>) -> Self {
        self.engine_updater = Some(updater);
        self
    }

    /// Submits a download request, validates inputs, and launches background processing.
    ///
    /// Playlist URLs are refused here: they must go through
    /// [`Self::start_download_batch`] so that each video becomes its own job with its
    /// own progress, cancellation, retry and history entry.
    pub async fn start_download(
        &self,
        raw_url: &str,
        requested_preset: Option<DownloadPreset>,
        custom_output_dir: Option<PathBuf>,
    ) -> Result<DownloadJob, CoreError> {
        let url = MediaUrl::parse(raw_url)?;
        if url.is_playlist() {
            return Err(CoreError::InvalidState(
                "Cette URL est une playlist: utilisez le téléchargement de playlist.".to_string(),
            ));
        }
        let settings = self.settings_repo.load().await?;
        let target_dir = resolve_target_dir(&settings, custom_output_dir)?;

        let mut job = DownloadJob::new(
            url,
            requested_preset.unwrap_or_else(|| settings.default_preset()),
        );
        job.set_target_dir(target_dir.clone());

        Ok(self.enqueue_and_spawn(job, target_dir).await)
    }

    /// Queues one job per selected playlist entry.
    ///
    /// Every URL is re-parsed and re-validated here, so the frontend can never inject
    /// an unchecked address. The concurrency limiter then runs them a few at a time
    /// while each video keeps independent progress, cancellation, retry and history.
    pub async fn start_download_batch(
        &self,
        urls: Vec<MediaUrl>,
        requested_preset: Option<DownloadPreset>,
        custom_output_dir: Option<PathBuf>,
    ) -> Result<Vec<DownloadJob>, CoreError> {
        if urls.is_empty() {
            return Err(CoreError::InvalidSettings(
                "Aucune vidéo sélectionnée.".to_string(),
            ));
        }
        if urls.len() > MAX_PLAYLIST_DOWNLOADS {
            return Err(CoreError::InvalidSettings(format!(
                "Trop de vidéos sélectionnées ({}). Maximum: {MAX_PLAYLIST_DOWNLOADS}.",
                urls.len()
            )));
        }

        let settings = self.settings_repo.load().await?;
        let preset = requested_preset.unwrap_or_else(|| settings.default_preset());
        let target_dir = resolve_target_dir(&settings, custom_output_dir)?;

        let mut jobs = Vec::with_capacity(urls.len());
        for url in urls {
            let mut job = DownloadJob::new(url, preset);
            job.set_target_dir(target_dir.clone());
            jobs.push(self.enqueue_and_spawn(job, target_dir.clone()).await);
        }
        Ok(jobs)
    }

    /// Re-queues a failed job as a brand new queued job preserving URL, preset and target directory.
    ///
    /// A new job (new identifier) is created rather than mutating the failed one, so the
    /// "terminal jobs never mutate" invariant is preserved and no `Failed -> Queued`
    /// transition is added to the domain.
    pub async fn retry_download(&self, job_id: DownloadId) -> Result<DownloadJob, CoreError> {
        let (url, preset, stored_target_dir) = {
            let lock = self.active_jobs.lock().await;
            let job = lock.get(&job_id).ok_or(CoreError::JobNotFound(job_id))?;
            if job.status() != DownloadStatus::Failed {
                return Err(CoreError::InvalidState(format!(
                    "Cannot retry job '{job_id}' in state '{:?}': only failed jobs can be retried",
                    job.status()
                )));
            }
            (
                job.url().clone(),
                job.preset(),
                job.target_dir().map(PathBuf::from),
            )
        };

        // Remove the failed entry (and any token) before re-queueing.
        {
            let mut lock = self.active_jobs.lock().await;
            lock.remove(&job_id);
        }
        {
            let mut tokens = self.cancellation_tokens.lock().await;
            tokens.remove(&job_id);
        }

        let target_dir = match stored_target_dir {
            Some(dir) => dir,
            None => {
                let settings = self.settings_repo.load().await?;
                PathBuf::from(settings.download_directory())
            }
        };

        let mut job = DownloadJob::new(url, preset);
        job.set_target_dir(target_dir.clone());

        Ok(self.enqueue_and_spawn(job, target_dir).await)
    }

    /// Stores a job, emits `queued`, and spawns the background pipeline task.
    async fn enqueue_and_spawn(&self, job: DownloadJob, target_dir: PathBuf) -> DownloadJob {
        let job_id = job.id();
        let cancel_token = CancellationToken::new();

        // Store initial job state and cancellation token
        {
            let mut lock = self.active_jobs.lock().await;
            lock.insert(job_id, job.clone());
        }
        {
            let mut tokens_lock = self.cancellation_tokens.lock().await;
            tokens_lock.insert(job_id, cancel_token.clone());
        }

        // Emit queued event
        if let Some(ref sink) = self.event_sink {
            sink.emit_queued(&job);
        }

        // Spawn background execution task
        let spec = AttemptSpec {
            job_id,
            url: job.url().clone(),
            preset: job.preset(),
            target_dir,
            temp_root: self.temp_root.clone(),
            downloader: Arc::clone(&self.downloader),
            converter: Arc::clone(&self.converter),
            inspector: Arc::clone(&self.inspector),
            history_repo: Arc::clone(&self.history_repo),
            active_jobs: Arc::clone(&self.active_jobs),
            event_sink: self.event_sink.clone(),
            publish_mutex: Arc::clone(&self.publish_mutex),
            cancel_token: cancel_token.clone(),
        };

        let limiter = self.limiter.clone();
        let active_jobs = Arc::clone(&self.active_jobs);
        let cancellation_tokens = Arc::clone(&self.cancellation_tokens);
        let event_sink = self.event_sink.clone();
        let retry_policy = self.retry_policy;
        let engine_updater = self.engine_updater.clone();

        tokio::spawn(async move {
            let permit_res = limiter.acquire(Some(&cancel_token)).await;
            // The permit is held for the whole retry cycle on purpose: releasing
            // it between attempts could let another job take the slot and leave
            // this retry waiting indefinitely.
            let _permit = match permit_res {
                Ok(p) => p,
                Err(CoreError::OperationCancelled) => {
                    Self::handle_job_canceled(
                        &active_jobs,
                        &cancellation_tokens,
                        &event_sink,
                        job_id,
                    )
                    .await;
                    return;
                }
                Err(err) => {
                    let details = err.to_download_error_details();
                    Self::handle_job_failure(
                        &active_jobs,
                        &cancellation_tokens,
                        &event_sink,
                        job_id,
                        details,
                    )
                    .await;
                    return;
                }
            };

            let mut last_error: Option<DownloadErrorDetails> = None;
            let mut post_update_retry_used = false;

            // One extra loop slot is reserved for the single post-update retry,
            // which must remain possible even when the stale-engine error surfaces
            // on the last regular attempt.
            let hard_attempt_cap = retry_policy.max_attempts + 1;
            let mut attempt: u32 = 1;

            while attempt <= hard_attempt_cap {
                let ctx = spec.to_context();
                match Self::run_pipeline(ctx).await {
                    Ok(()) => {
                        let mut tokens = cancellation_tokens.lock().await;
                        tokens.remove(&job_id);
                        return;
                    }
                    Err(CoreError::OperationCancelled) => {
                        Self::handle_job_canceled(
                            &active_jobs,
                            &cancellation_tokens,
                            &event_sink,
                            job_id,
                        )
                        .await;
                        return;
                    }
                    Err(err) => {
                        let details = err.to_download_error_details();

                        // Transient failure with attempts left: warn and back off.
                        if details.retryable
                            && attempt < retry_policy.max_attempts
                            && details.code != DownloadErrorCode::HistorySaveFailed
                        {
                            Self::handle_job_retry(
                                &active_jobs,
                                &event_sink,
                                job_id,
                                attempt + 1,
                                retry_policy.max_attempts,
                            )
                            .await;
                            if !retry_policy
                                .sleep_before_retry(attempt, &cancel_token)
                                .await
                            {
                                Self::handle_job_canceled(
                                    &active_jobs,
                                    &cancellation_tokens,
                                    &event_sink,
                                    job_id,
                                )
                                .await;
                                return;
                            }
                            attempt += 1;
                            continue;
                        }

                        // Stale engine: update it once, then retry exactly once.
                        if details.code == DownloadErrorCode::YtdlpUpdateRequired
                            && !post_update_retry_used
                        {
                            post_update_retry_used = true;
                            let updated = match engine_updater.as_ref() {
                                Some(updater) => updater.update_engine().await.is_ok(),
                                None => false,
                            };
                            if updated {
                                if let Some(ref sink) = event_sink {
                                    sink.emit_warning(
                                        &crate::ports::event_sink::DownloadWarningEvent {
                                            download_id: job_id,
                                            code: "ENGINE_UPDATE_RETRY".to_string(),
                                            message:
                                                "Moteur mis à jour, nouvelle tentative en cours."
                                                    .to_string(),
                                        },
                                    );
                                }
                                Self::handle_job_retry(
                                    &active_jobs,
                                    &event_sink,
                                    job_id,
                                    attempt + 1,
                                    hard_attempt_cap,
                                )
                                .await;
                                attempt += 1;
                                continue;
                            }
                            // Update failed: keep the original error.
                            last_error = Some(details);
                            break;
                        }

                        last_error = Some(details);
                        break;
                    }
                }
            }

            let details = last_error.unwrap_or_else(|| {
                DownloadErrorDetails::from_code(DownloadErrorCode::DownloadProcessFailed)
            });
            Self::handle_job_failure(
                &active_jobs,
                &cancellation_tokens,
                &event_sink,
                job_id,
                details,
            )
            .await;
        });

        job
    }

    /// Updates job state for an upcoming automatic attempt and emits a warning.
    async fn handle_job_retry(
        active_jobs: &Arc<Mutex<HashMap<DownloadId, DownloadJob>>>,
        event_sink: &Option<Arc<dyn EventSink>>,
        job_id: DownloadId,
        next_attempt: u32,
        max_attempts: u32,
    ) {
        let updated = {
            let mut lock = active_jobs.lock().await;
            if let Some(job) = lock.get_mut(&job_id) {
                // A job canceled meanwhile is terminal: ignore the reset.
                job.reset_for_retry().is_ok().then(|| job.clone())
            } else {
                None
            }
        };

        if let (Some(ref sink), Some(job)) = (event_sink, updated) {
            sink.emit_queued(&job);
            sink.emit_warning(&crate::ports::event_sink::DownloadWarningEvent {
                download_id: job_id,
                code: "RETRY_ATTEMPT".to_string(),
                message: format!("Nouvelle tentative {next_attempt}/{max_attempts}"),
            });
        }
    }

    /// Explicitly cancels an in-progress or queued download job immediately.
    pub async fn cancel_download(&self, job_id: DownloadId) -> Result<DownloadJob, CoreError> {
        let (canceled_job, should_emit, token) = {
            let mut lock = self.active_jobs.lock().await;
            let job = lock
                .get_mut(&job_id)
                .ok_or(CoreError::JobNotFound(job_id))?;

            if job.status() == DownloadStatus::Canceled {
                // Idempotent: already canceled, return job without duplicate event
                return Ok(job.clone());
            }

            if job.is_terminal() {
                return Err(CoreError::InvalidState(format!(
                    "Cannot cancel job '{job_id}' in terminal state '{:?}'",
                    job.status()
                )));
            }

            job.transition_to_canceled()?;
            let job_clone = job.clone();

            let token = {
                let tokens = self.cancellation_tokens.lock().await;
                tokens.get(&job_id).cloned()
            };

            (job_clone, true, token)
        };

        if let Some(t) = token {
            t.cancel();
        }

        if should_emit {
            if let Some(ref sink) = self.event_sink {
                sink.emit_canceled(&canceled_job);
            }
        }

        Ok(canceled_job)
    }

    /// Dismisses a completed, failed, or canceled download job from the active in-memory list.
    pub async fn dismiss_download(&self, job_id: DownloadId) -> Result<(), CoreError> {
        let mut lock = self.active_jobs.lock().await;
        if let Some(job) = lock.get(&job_id) {
            if matches!(
                job.status(),
                DownloadStatus::Queued
                    | DownloadStatus::Preparing
                    | DownloadStatus::Downloading
                    | DownloadStatus::Converting
                    | DownloadStatus::Finalizing
            ) {
                return Err(CoreError::InvalidState(
                    "Cannot dismiss an active download; please cancel it first".to_string(),
                ));
            }
        }
        lock.remove(&job_id);

        let mut tokens = self.cancellation_tokens.lock().await;
        tokens.remove(&job_id);

        Ok(())
    }

    /// List all download jobs currently in memory.
    pub async fn list_downloads(&self) -> Vec<DownloadJob> {
        let lock = self.active_jobs.lock().await;
        lock.values().cloned().collect()
    }

    /// Lists persistent download history.
    pub async fn list_history(&self) -> Result<Vec<DownloadHistoryEntry>, CoreError> {
        self.history_repo.load().await
    }

    /// Removes a persistent download history entry without touching the downloaded file.
    pub async fn remove_history_entry(&self, id: HistoryEntryId) -> Result<(), CoreError> {
        self.history_repo.remove(id).await
    }

    /// Retrieves the verified canonical path of a history entry.
    ///
    /// The stored path must canonicalize to a file located inside the download
    /// directory recorded with the entry (or the current settings directory for
    /// legacy entries without one). Anything else is refused without opening it.
    pub async fn get_history_file_path(&self, id: HistoryEntryId) -> Result<PathBuf, CoreError> {
        let entries = self.history_repo.load().await?;
        let entry = entries.into_iter().find(|e| e.id() == id).ok_or_else(|| {
            let mut details =
                DownloadErrorDetails::from_code(DownloadErrorCode::OutputFileNotFound);
            details.message = format!("Entrée d'historique '{id}' introuvable.");
            CoreError::DownloadFailed(details)
        })?;

        let allowed_dir = match entry.download_dir() {
            Some(dir) => PathBuf::from(dir),
            None => {
                let settings = self.settings_repo.load().await?;
                PathBuf::from(settings.download_directory())
            }
        };

        let path = PathBuf::from(entry.destination_path());
        ensure_path_within_download_dir(&path, &allowed_dir)
    }

    /// Retrieves and validates the source URL of a history entry.
    pub async fn get_history_source_url(&self, id: HistoryEntryId) -> Result<String, CoreError> {
        let entries = self.history_repo.load().await?;
        let entry = entries.into_iter().find(|e| e.id() == id).ok_or_else(|| {
            let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::SourceUrlInvalid);
            details.message = format!("Entrée d'historique '{id}' introuvable.");
            CoreError::DownloadFailed(details)
        })?;

        let url_str = entry.source_url().as_str().to_string();
        if !url_str.starts_with("http://") && !url_str.starts_with("https://") {
            let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::SourceUrlInvalid);
            details.message = "Seules les URL HTTP et HTTPS sont autorisées.".to_string();
            return Err(CoreError::DownloadFailed(details));
        }

        Ok(url_str)
    }

    /// Retrieves and validates the source URL of an active/known download job.
    pub async fn get_download_source_url(&self, job_id: DownloadId) -> Result<String, CoreError> {
        let lock = self.active_jobs.lock().await;
        let job = lock.get(&job_id).ok_or(CoreError::JobNotFound(job_id))?;

        let url_str = job.url().as_str().to_string();
        if !url_str.starts_with("http://") && !url_str.starts_with("https://") {
            let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::SourceUrlInvalid);
            details.message = "Seules les URL HTTP et HTTPS sont autorisées.".to_string();
            return Err(CoreError::DownloadFailed(details));
        }

        Ok(url_str)
    }

    /// Retrieves the verified canonical path of a completed download job.
    pub async fn get_completed_download_path(
        &self,
        job_id: DownloadId,
    ) -> Result<PathBuf, CoreError> {
        let (path_str, allowed_dir) = {
            let lock = self.active_jobs.lock().await;
            let job = lock.get(&job_id).ok_or(CoreError::JobNotFound(job_id))?;

            if job.status() != DownloadStatus::Completed {
                let mut details =
                    DownloadErrorDetails::from_code(DownloadErrorCode::OutputFileNotFound);
                details.message = format!("Le téléchargement '{job_id}' n'est pas encore terminé.");
                return Err(CoreError::DownloadFailed(details));
            }

            let path_str = job.destination_path().map(str::to_string).ok_or_else(|| {
                let mut details =
                    DownloadErrorDetails::from_code(DownloadErrorCode::OutputFileNotFound);
                details.message =
                    format!("Chemin de destination introuvable pour le téléchargement '{job_id}'.");
                CoreError::DownloadFailed(details)
            })?;

            (path_str, job.target_dir().map(PathBuf::from))
        };

        let allowed_dir = match allowed_dir {
            Some(dir) => dir,
            None => {
                let settings = self.settings_repo.load().await?;
                PathBuf::from(settings.download_directory())
            }
        };

        ensure_path_within_download_dir(&PathBuf::from(path_str), &allowed_dir)
    }

    async fn run_pipeline(ctx: PipelineContext) -> Result<(), CoreError> {
        if ctx.cancel_token.is_cancelled() {
            return Err(CoreError::OperationCancelled);
        }

        // Step 1: Preparing
        {
            let mut lock = ctx.active_jobs.lock().await;
            if let Some(job) = lock.get_mut(&ctx.job_id) {
                job.transition_to_preparing()?;
            }
        }
        if let Some(ref sink) = ctx.event_sink {
            sink.emit_progress(&DownloadProgressEvent {
                download_id: ctx.job_id,
                phase: ProgressPhase::Preparing,
                percent: None,
                downloaded_bytes: None,
                total_bytes: None,
                speed_bytes_per_second: None,
            });
        }

        // Verify target directory can be accessed / created
        tokio::fs::create_dir_all(&ctx.target_dir)
            .await
            .map_err(|err| {
                let mut details =
                    DownloadErrorDetails::from_code(DownloadErrorCode::OutputPermissionDenied);
                details.message = format!(
                    "Impossible d'accéder au dossier de destination '{}': {err}",
                    ctx.target_dir.display()
                );
                CoreError::DownloadFailed(details)
            })?;

        let job_temp_dir = ctx.temp_root.join(format!("job_{}", ctx.job_id));
        tokio::fs::create_dir_all(&job_temp_dir)
            .await
            .map_err(|err| {
                CoreError::StorageError(format!("Failed to create temporary directory: {err}"))
            })?;

        if ctx.cancel_token.is_cancelled() {
            let _ = tokio::fs::remove_dir_all(&job_temp_dir).await;
            return Err(CoreError::OperationCancelled);
        }

        // Step 2: Downloading streams
        {
            let mut lock = ctx.active_jobs.lock().await;
            if let Some(job) = lock.get_mut(&ctx.job_id) {
                job.transition_to_downloading()?;
            }
        }

        let sink_clone = ctx.event_sink.clone();
        let jobs_clone = Arc::clone(&ctx.active_jobs);
        let job_id = ctx.job_id;
        let progress_cancel_token = ctx.cancel_token.clone();

        let progress_callback = Arc::new(move |progress: StreamProgress| {
            if progress_cancel_token.is_cancelled() {
                return;
            }

            let is_terminal = if let Ok(mut lock) = jobs_clone.try_lock() {
                if let Some(job) = lock.get_mut(&job_id) {
                    if job.is_terminal() {
                        true
                    } else {
                        let _ = job.update_progress(
                            progress.percent,
                            progress.downloaded_bytes,
                            progress.total_bytes,
                            progress.speed_bytes_per_second,
                        );
                        false
                    }
                } else {
                    true
                }
            } else {
                false
            };

            if is_terminal || progress_cancel_token.is_cancelled() {
                return;
            }

            if let Some(ref sink) = sink_clone {
                sink.emit_progress(&DownloadProgressEvent {
                    download_id: job_id,
                    phase: ProgressPhase::Downloading,
                    percent: progress.percent,
                    downloaded_bytes: progress.downloaded_bytes,
                    total_bytes: progress.total_bytes,
                    speed_bytes_per_second: progress.speed_bytes_per_second,
                });
            }
        });

        let downloaded_streams = match ctx
            .downloader
            .download_stream(
                DownloadStreamRequest {
                    url: ctx.url.clone(),
                    preset: ctx.preset,
                    temp_dir: job_temp_dir.clone(),
                    cancellation_token: Some(ctx.cancel_token.clone()),
                },
                progress_callback,
            )
            .await
        {
            Ok(streams) => streams,
            Err(err) => {
                let _ = tokio::fs::remove_dir_all(&job_temp_dir).await;
                return Err(err);
            }
        };

        if ctx.cancel_token.is_cancelled() {
            let _ = tokio::fs::remove_dir_all(&job_temp_dir).await;
            return Err(CoreError::OperationCancelled);
        }

        // Non-fatal engine obsolescence signal: yt-dlp warns even on success.
        if downloaded_streams.engine_outdated {
            if let Some(ref sink) = ctx.event_sink {
                sink.emit_warning(&crate::ports::event_sink::DownloadWarningEvent {
                    download_id: ctx.job_id,
                    code: "YTDLP_UPDATE_REQUIRED".to_string(),
                    message: "Mettez à jour le moteur.".to_string(),
                });
            }
        }

        // Cookie import problems are never silent: yt-dlp continues without
        // cookies after a failed keychain/disk-permission access.
        match downloaded_streams.cookies_diagnostic {
            Some(crate::ports::media_downloader::CookiesDiagnostic::DecryptFailed) => {
                if let Some(ref sink) = ctx.event_sink {
                    sink.emit_warning(&crate::ports::event_sink::DownloadWarningEvent {
                        download_id: ctx.job_id,
                        code: "COOKIES_DECRYPT_FAILED".to_string(),
                        message: "Impossible de déchiffrer les cookies du navigateur.".to_string(),
                    });
                }
            }
            Some(crate::ports::media_downloader::CookiesDiagnostic::PermissionDenied) => {
                if let Some(ref sink) = ctx.event_sink {
                    sink.emit_warning(&crate::ports::event_sink::DownloadWarningEvent {
                        download_id: ctx.job_id,
                        code: "COOKIES_PERMISSION_DENIED".to_string(),
                        message: "Accès refusé aux cookies du navigateur (autorisation manquante)."
                            .to_string(),
                    });
                }
            }
            _ => {}
        }

        // Update job title
        {
            let mut lock = ctx.active_jobs.lock().await;
            if let Some(job) = lock.get_mut(&ctx.job_id) {
                job.set_title(downloaded_streams.title.clone());
            }
        }

        // Step 3: Inspect artifacts using MediaInspector
        let mut video_input: Option<PathBuf> = None;
        let mut audio_input: Option<PathBuf> = None;

        for artifact in &downloaded_streams.raw_artifacts {
            if !artifact.exists() {
                continue;
            }
            if let Ok(info) = ctx.inspector.inspect(artifact).await {
                if info.has_video {
                    if video_input.is_none() {
                        video_input = Some(artifact.clone());
                    }
                    if info.has_audio && audio_input.is_none() {
                        audio_input = Some(artifact.clone());
                    }
                } else if info.has_audio && audio_input.is_none() {
                    audio_input = Some(artifact.clone());
                }
            }
        }

        // Fallback to explicit paths if raw_artifacts inspection was inconclusive
        if video_input.is_none() {
            video_input = downloaded_streams.video_path;
        }
        if audio_input.is_none() {
            audio_input = downloaded_streams.audio_path;
        }

        // Step 4: Converting / Muxing
        if ctx.cancel_token.is_cancelled() {
            let _ = tokio::fs::remove_dir_all(&job_temp_dir).await;
            return Err(CoreError::OperationCancelled);
        }
        {
            let mut lock = ctx.active_jobs.lock().await;
            if let Some(job) = lock.get_mut(&ctx.job_id) {
                if job.is_terminal() {
                    let _ = tokio::fs::remove_dir_all(&job_temp_dir).await;
                    return Err(CoreError::OperationCancelled);
                }
                job.transition_to_converting()?;
            }
        }
        if let Some(ref sink) = ctx.event_sink {
            sink.emit_progress(&DownloadProgressEvent {
                download_id: ctx.job_id,
                phase: ProgressPhase::Converting,
                percent: None,
                downloaded_bytes: None,
                total_bytes: None,
                speed_bytes_per_second: None,
            });
        }

        let ext = ctx.preset.format().extension();
        let base_filename = sanitize_filename(&downloaded_streams.title);
        let temp_output_file = job_temp_dir.join(format!("output_converted.{ext}"));

        let conversion_result = match ctx.preset {
            DownloadPreset::Video { format, .. } => {
                let v_in = video_input.ok_or_else(|| {
                    let mut details =
                        DownloadErrorDetails::from_code(DownloadErrorCode::FfmpegFailed);
                    details.message = "Missing downloaded video stream for conversion".to_string();
                    CoreError::DownloadFailed(details)
                })?;

                ctx.converter
                    .convert(
                        ConvertRequest::VideoMuxOrTranscode {
                            video_input: v_in,
                            audio_input,
                            output_path: temp_output_file.clone(),
                            format,
                            duration_seconds: downloaded_streams.duration_seconds,
                            temp_dir: job_temp_dir.clone(),
                        },
                        Some(ctx.cancel_token.clone()),
                        None,
                    )
                    .await
            }
            DownloadPreset::Mp3 { quality } => {
                let a_in = audio_input.or(video_input).ok_or_else(|| {
                    let mut details =
                        DownloadErrorDetails::from_code(DownloadErrorCode::FfmpegFailed);
                    details.message =
                        "Missing downloaded audio stream for MP3 conversion".to_string();
                    CoreError::DownloadFailed(details)
                })?;

                ctx.converter
                    .convert(
                        ConvertRequest::AudioTranscode {
                            audio_input: a_in,
                            output_path: temp_output_file.clone(),
                            codec: AudioCodec::Mp3(quality),
                            duration_seconds: downloaded_streams.duration_seconds,
                            temp_dir: job_temp_dir.clone(),
                        },
                        Some(ctx.cancel_token.clone()),
                        None,
                    )
                    .await
            }
            DownloadPreset::Flac => {
                let a_in = audio_input.or(video_input).ok_or_else(|| {
                    let mut details =
                        DownloadErrorDetails::from_code(DownloadErrorCode::FfmpegFailed);
                    details.message =
                        "Missing downloaded audio stream for FLAC conversion".to_string();
                    CoreError::DownloadFailed(details)
                })?;

                ctx.converter
                    .convert(
                        ConvertRequest::AudioTranscode {
                            audio_input: a_in,
                            output_path: temp_output_file.clone(),
                            codec: AudioCodec::Flac,
                            duration_seconds: downloaded_streams.duration_seconds,
                            temp_dir: job_temp_dir.clone(),
                        },
                        Some(ctx.cancel_token.clone()),
                        None,
                    )
                    .await
            }
        };

        let temp_converted_path = match conversion_result {
            Ok(path) => path,
            Err(err) => {
                let _ = tokio::fs::remove_dir_all(&job_temp_dir).await;
                return Err(err);
            }
        };

        if ctx.cancel_token.is_cancelled() {
            let _ = tokio::fs::remove_dir_all(&job_temp_dir).await;
            return Err(CoreError::OperationCancelled);
        }

        // Step 5: Finalizing
        {
            let mut lock = ctx.active_jobs.lock().await;
            if let Some(job) = lock.get_mut(&ctx.job_id) {
                if job.is_terminal() {
                    let _ = tokio::fs::remove_dir_all(&job_temp_dir).await;
                    return Err(CoreError::OperationCancelled);
                }
                job.transition_to_finalizing()?;
            }
        }
        if let Some(ref sink) = ctx.event_sink {
            sink.emit_progress(&DownloadProgressEvent {
                download_id: ctx.job_id,
                phase: ProgressPhase::Finalizing,
                percent: None,
                downloaded_bytes: None,
                total_bytes: None,
                speed_bytes_per_second: None,
            });
        }

        let meta = tokio::fs::metadata(&temp_converted_path)
            .await
            .map_err(|err| {
                CoreError::ConverterError(format!(
                    "Failed to access generated output file '{}': {err}",
                    temp_converted_path.display()
                ))
            })?;

        if meta.len() == 0 {
            let _ = tokio::fs::remove_dir_all(&job_temp_dir).await;
            return Err(CoreError::ConverterError(format!(
                "Generated file '{}' is empty (0 bytes)",
                temp_converted_path.display()
            )));
        }

        // Commit Point Check: do not publish if canceled right before commit
        if ctx.cancel_token.is_cancelled() {
            let _ = tokio::fs::remove_dir_all(&job_temp_dir).await;
            return Err(CoreError::OperationCancelled);
        }

        {
            let lock = ctx.active_jobs.lock().await;
            if let Some(job) = lock.get(&ctx.job_id) {
                if job.is_terminal() {
                    let _ = tokio::fs::remove_dir_all(&job_temp_dir).await;
                    return Err(CoreError::OperationCancelled);
                }
            }
        }

        // Atomically publish to final destination file without collision or overwrite
        let destination_file = publish_media_no_clobber(
            &temp_converted_path,
            &ctx.target_dir,
            &base_filename,
            ext,
            &ctx.publish_mutex,
            &ctx.event_sink,
            ctx.job_id,
        )
        .await?;

        let _ = tokio::fs::remove_dir_all(&job_temp_dir).await;

        // Step 6: Completed
        let final_path_str = destination_file.to_string_lossy().to_string();
        let completed_job = {
            let mut lock = ctx.active_jobs.lock().await;
            if let Some(job) = lock.get_mut(&ctx.job_id) {
                if job.is_terminal() {
                    None
                } else {
                    job.transition_to_completed(final_path_str.clone())?;
                    Some(job.clone())
                }
            } else {
                None
            }
        };

        // Record history entry idempotently; on failure emit non-fatal warning without failing job
        if let Some(ref job) = completed_job {
            if let Ok(history_entry) = DownloadHistoryEntry::new_with_dir(
                ctx.job_id,
                ctx.url,
                downloaded_streams.title,
                ctx.preset,
                final_path_str,
                None,
                ctx.target_dir.to_string_lossy().to_string(),
            ) {
                if let Err(err) = ctx.history_repo.append(history_entry).await {
                    if let Some(ref sink) = ctx.event_sink {
                        sink.emit_warning(&crate::ports::event_sink::DownloadWarningEvent {
                            download_id: ctx.job_id,
                            code: "HISTORY_SAVE_FAILED".to_string(),
                            message: format!("Failed to record download history: {err}"),
                        });
                    }
                }
            }

            if let Some(ref sink) = ctx.event_sink {
                sink.emit_completed(job);
            }
        }

        Ok(())
    }

    /// Handles cancellation state update and notification.
    async fn handle_job_canceled(
        active_jobs: &Arc<Mutex<HashMap<DownloadId, DownloadJob>>>,
        cancellation_tokens: &Arc<Mutex<HashMap<DownloadId, CancellationToken>>>,
        event_sink: &Option<Arc<dyn EventSink>>,
        job_id: DownloadId,
    ) {
        let canceled_job = {
            let mut lock = active_jobs.lock().await;
            if let Some(job) = lock.get_mut(&job_id) {
                if job.status() != DownloadStatus::Canceled {
                    let _ = job.transition_to_canceled();
                    Some(job.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };

        {
            let mut tokens = cancellation_tokens.lock().await;
            tokens.remove(&job_id);
        }

        if let (Some(ref sink), Some(job)) = (event_sink, canceled_job) {
            sink.emit_canceled(&job);
        }
    }

    /// Handles failure state update and notification.
    async fn handle_job_failure(
        active_jobs: &Arc<Mutex<HashMap<DownloadId, DownloadJob>>>,
        cancellation_tokens: &Arc<Mutex<HashMap<DownloadId, CancellationToken>>>,
        event_sink: &Option<Arc<dyn EventSink>>,
        job_id: DownloadId,
        error: DownloadErrorDetails,
    ) {
        let failed_job = {
            let mut lock = active_jobs.lock().await;
            if let Some(job) = lock.get_mut(&job_id) {
                if job.is_terminal() {
                    None
                } else {
                    let _ = job.transition_to_failed(error.clone());
                    Some(job.clone())
                }
            } else {
                None
            }
        };

        {
            let mut tokens = cancellation_tokens.lock().await;
            tokens.remove(&job_id);
        }

        if let (Some(ref sink), Some(job)) = (event_sink, failed_job) {
            sink.emit_failed(&job, &error);
        }
    }
}

/// Ensures `candidate` is an existing regular file whose canonical path lives
/// inside `allowed_dir` (also canonicalized). Refuses symlink escapes.
fn ensure_path_within_download_dir(
    candidate: &Path,
    allowed_dir: &Path,
) -> Result<PathBuf, CoreError> {
    let not_found = |msg: String| {
        let mut details = DownloadErrorDetails::from_code(DownloadErrorCode::OutputFileNotFound);
        details.message = msg;
        CoreError::DownloadFailed(details)
    };

    if !candidate.exists() || !candidate.is_file() {
        return Err(not_found(format!(
            "Le fichier téléchargé '{}' est introuvable sur le disque.",
            candidate.display()
        )));
    }

    let canonical_file = std::fs::canonicalize(candidate).map_err(|err| {
        not_found(format!(
            "Impossible de résoudre le chemin '{}': {err}",
            candidate.display()
        ))
    })?;

    let canonical_dir = std::fs::canonicalize(allowed_dir).map_err(|_| {
        not_found(format!(
            "Le dossier de téléchargement '{}' est introuvable.",
            allowed_dir.display()
        ))
    })?;

    if !canonical_file.starts_with(&canonical_dir) {
        return Err(not_found(format!(
            "Le fichier '{}' est en dehors du dossier de téléchargement autorisé.",
            candidate.display()
        )));
    }

    Ok(canonical_file)
}

/// Sanitizes a title into a bounded UTF-8 safe filename.
pub fn sanitize_filename(title: &str) -> String {
    let forbidden = ['/', '\\', '?', '%', '*', ':', '|', '"', '<', '>', '\0'];
    let sanitized: String = title
        .chars()
        .map(|c| {
            if c.is_control() || forbidden.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();

    let trimmed = sanitized
        .trim_end_matches(|c: char| c == '.' || c.is_whitespace())
        .trim();

    if trimmed.is_empty() {
        return "PolySaver_Media".to_string();
    }

    let upper = trimmed.to_ascii_uppercase();
    let base_candidate = if WINDOWS_RESERVED_NAMES.contains(&upper.as_str()) {
        format!("PolySaver_{trimmed}")
    } else {
        trimmed.to_string()
    };

    if base_candidate.len() <= MAX_BASE_FILENAME_BYTES {
        base_candidate
    } else {
        let mut end = MAX_BASE_FILENAME_BYTES;
        while end > 0 && !base_candidate.is_char_boundary(end) {
            end -= 1;
        }
        let truncated = &base_candidate[..end];
        let cleaned = truncated
            .trim_end_matches(|c: char| c == '.' || c.is_whitespace())
            .trim();
        if cleaned.is_empty() {
            "PolySaver_Media".to_string()
        } else {
            cleaned.to_string()
        }
    }
}

/// Maximum age before an orphaned `job_*` temporary directory is purged.
///
/// A directory younger than this may belong to a concurrently running instance,
/// so it is left alone; anything older is the debris of a crash or a SIGKILL and
/// would otherwise accumulate indefinitely (hundreds of MB have been observed).
pub const ORPHAN_TEMP_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// Removes `job_*` directories older than [`ORPHAN_TEMP_MAX_AGE`] from `temp_root`.
///
/// Returns the number of directories removed. A missing `temp_root` is not an
/// error: nothing has been downloaded yet on a fresh installation.
///
/// Only the modification time is consulted, so no inter-process lock is needed.
pub async fn purge_orphan_temp_dirs(temp_root: &Path) -> usize {
    purge_orphan_temp_dirs_older_than(temp_root, ORPHAN_TEMP_MAX_AGE).await
}

/// Age-parameterized variant of [`purge_orphan_temp_dirs`], used by tests to
/// avoid waiting a full day for the threshold.
pub async fn purge_orphan_temp_dirs_older_than(
    temp_root: &Path,
    max_age: std::time::Duration,
) -> usize {
    let Ok(mut entries) = tokio::fs::read_dir(temp_root).await else {
        return 0;
    };

    let now = std::time::SystemTime::now();
    let mut removed = 0usize;

    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("job_") {
            continue;
        }

        let Ok(metadata) = entry.metadata().await else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }

        let age = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .unwrap_or_default();

        if age >= max_age && tokio::fs::remove_dir_all(entry.path()).await.is_ok() {
            removed += 1;
        }
    }

    removed
}

/// Emits a non-fatal warning when the published file could not be flushed to disk.
fn emit_publish_sync_warning(
    event_sink: &Option<Arc<dyn EventSink>>,
    job_id: DownloadId,
    path: &Path,
    err: &std::io::Error,
) {
    if let Some(sink) = event_sink {
        sink.emit_warning(&crate::ports::event_sink::DownloadWarningEvent {
            download_id: job_id,
            code: "PUBLISH_SYNC_FAILED".to_string(),
            message: format!(
                "Impossible de synchroniser '{}' sur le disque: {err}",
                path.display()
            ),
        });
    }
}

/// Publishes the converted media file to the destination directory without overwriting any existing file.
async fn publish_media_no_clobber(
    temp_converted_file: &Path,
    target_dir: &Path,
    base_name: &str,
    ext: &str,
    publish_mutex: &Arc<Mutex<()>>,
    event_sink: &Option<Arc<dyn EventSink>>,
    job_id: DownloadId,
) -> Result<PathBuf, CoreError> {
    let _lock = publish_mutex.lock().await;

    tokio::fs::create_dir_all(target_dir).await.map_err(|err| {
        let mut details =
            DownloadErrorDetails::from_code(DownloadErrorCode::OutputPermissionDenied);
        details.message = format!(
            "Impossible d'accéder au dossier de destination '{}': {err}",
            target_dir.display()
        );
        CoreError::DownloadFailed(details)
    })?;

    let stage_name = format!(".polysaver_stage_{}.{ext}", uuid::Uuid::new_v4().simple());
    let stage_path = target_dir.join(stage_name);

    if let Err(rename_err) = tokio::fs::rename(temp_converted_file, &stage_path).await {
        tokio::fs::copy(temp_converted_file, &stage_path)
            .await
            .map_err(|copy_err| {
                let mut details =
                    DownloadErrorDetails::from_code(DownloadErrorCode::OutputPermissionDenied);
                details.message = format!(
                    "Impossible de transférer le fichier vers le dossier de destination: {rename_err} / {copy_err}"
                );
                CoreError::DownloadFailed(details)
            })?;
    }

    let mut counter = 0usize;
    loop {
        let candidate_filename = if counter == 0 {
            format!("{base_name}.{ext}")
        } else {
            format!("{base_name} ({counter}).{ext}")
        };
        let candidate_path = target_dir.join(&candidate_filename);

        match std::fs::hard_link(&stage_path, &candidate_path) {
            Ok(()) => {
                // Flush the published file before removing the staging link.
                if let Ok(file) = std::fs::File::open(&candidate_path) {
                    if let Err(err) = file.sync_all() {
                        emit_publish_sync_warning(event_sink, job_id, &candidate_path, &err);
                    }
                }
                let _ = tokio::fs::remove_file(&stage_path).await;
                return Ok(candidate_path);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                counter += 1;
                continue;
            }
            Err(_) => {
                // Fallback to exclusive file creation (create_new) and copy
                match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&candidate_path)
                {
                    Ok(mut dest_file) => {
                        let mut src_file = std::fs::File::open(&stage_path).map_err(|e| {
                            let _ = std::fs::remove_file(&candidate_path);
                            let _ = std::fs::remove_file(&stage_path);
                            CoreError::StorageError(format!("Failed to open staging file: {e}"))
                        })?;
                        if let Err(e) = std::io::copy(&mut src_file, &mut dest_file) {
                            let _ = std::fs::remove_file(&candidate_path);
                            let _ = std::fs::remove_file(&stage_path);
                            return Err(CoreError::StorageError(format!(
                                "Failed to write destination file: {e}"
                            )));
                        }
                        // Treat fsync failure as a visible (non-fatal) warning.
                        if let Err(sync_err) = dest_file.sync_all() {
                            emit_publish_sync_warning(
                                event_sink,
                                job_id,
                                &candidate_path,
                                &sync_err,
                            );
                        }
                        let _ = std::fs::remove_file(&stage_path);
                        return Ok(candidate_path);
                    }
                    Err(open_err) if open_err.kind() == std::io::ErrorKind::AlreadyExists => {
                        counter += 1;
                        continue;
                    }
                    Err(open_err) => {
                        let _ = std::fs::remove_file(&stage_path);
                        let mut details = DownloadErrorDetails::from_code(
                            DownloadErrorCode::OutputPermissionDenied,
                        );
                        details.message = format!(
                            "Impossible de créer le fichier '{}': {open_err}",
                            candidate_path.display()
                        );
                        return Err(CoreError::DownloadFailed(details));
                    }
                }
            }
        }
    }
}
