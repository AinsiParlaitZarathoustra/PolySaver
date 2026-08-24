// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::download_job::{DownloadId, DownloadJob, DownloadStatus};
use crate::domain::format::DownloadPreset;
use crate::domain::history::{DownloadHistoryEntry, HistoryEntryId};
use crate::domain::media_url::MediaUrl;
use crate::error::{CoreError, DownloadErrorCode, DownloadErrorDetails};
use crate::ports::converter::{AudioCodec, ConvertRequest, MediaConverter};
use crate::ports::event_sink::{DownloadProgressEvent, EventSink, ProgressPhase};
use crate::ports::history_repository::DownloadHistoryRepository;
use crate::ports::media_downloader::{DownloadStreamRequest, MediaDownloader, StreamProgress};
use crate::ports::media_inspector::MediaInspector;
use crate::ports::settings_repository::SettingsRepository;
use crate::services::limiter::ConcurrencyLimiter;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

const MAX_BASE_FILENAME_BYTES: usize = 180;

const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Execution context encapsulating all runtime dependencies for the download pipeline.
struct PipelineContext {
    job_id: DownloadId,
    url: MediaUrl,
    preset: DownloadPreset,
    target_dir: PathBuf,
    parallel_segments: usize,
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
}

impl StartDownloadService {
    /// Creates a new `StartDownloadService`.
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
        Self {
            downloader,
            converter,
            inspector,
            settings_repo,
            history_repo,
            event_sink,
            temp_root,
            active_jobs: Arc::new(Mutex::new(HashMap::new())),
            limiter: ConcurrencyLimiter::new(3),
            cancellation_tokens: Arc::new(Mutex::new(HashMap::new())),
            publish_mutex: Arc::new(Mutex::new(())),
        }
    }

    /// Dynamically updates the concurrency limit.
    pub fn set_max_concurrent(&self, max: usize) {
        self.limiter.set_limit(max);
    }

    /// Submits a download request, validates inputs, and launches background processing.
    pub async fn start_download(
        &self,
        raw_url: &str,
        requested_preset: Option<DownloadPreset>,
        custom_output_dir: Option<PathBuf>,
    ) -> Result<DownloadJob, CoreError> {
        let url = MediaUrl::parse(raw_url)?;
        let settings = self.settings_repo.load().await?;
        self.limiter.set_limit(settings.effective_max_concurrent());

        let preset = requested_preset.unwrap_or_else(|| settings.default_preset());

        let target_dir = if let Some(custom_dir) = custom_output_dir {
            let path_str = custom_dir.to_string_lossy().trim().to_string();
            if path_str.is_empty() {
                PathBuf::from(settings.download_directory())
            } else {
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
                custom_dir
            }
        } else {
            PathBuf::from(settings.download_directory())
        };

        let job = DownloadJob::new(url.clone(), preset);
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
        let ctx = PipelineContext {
            job_id,
            url,
            preset,
            target_dir,
            parallel_segments: settings.effective_parallel_segments(),
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

        tokio::spawn(async move {
            let permit_res = limiter.acquire(Some(&cancel_token)).await;
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

            let result = Self::run_pipeline(ctx).await;

            match result {
                Ok(()) => {
                    let mut tokens = cancellation_tokens.lock().await;
                    tokens.remove(&job_id);
                }
                Err(CoreError::OperationCancelled) => {
                    Self::handle_job_canceled(
                        &active_jobs,
                        &cancellation_tokens,
                        &event_sink,
                        job_id,
                    )
                    .await;
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
                }
            }
        });

        Ok(job)
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
    pub async fn get_history_file_path(&self, id: HistoryEntryId) -> Result<PathBuf, CoreError> {
        let entries = self.history_repo.load().await?;
        let entry = entries.into_iter().find(|e| e.id() == id).ok_or_else(|| {
            let mut details =
                DownloadErrorDetails::from_code(DownloadErrorCode::OutputFileNotFound);
            details.message = format!("Entrée d'historique '{id}' introuvable.");
            CoreError::DownloadFailed(details)
        })?;

        let path = PathBuf::from(entry.destination_path());
        if !path.exists() || !path.is_file() {
            let mut details =
                DownloadErrorDetails::from_code(DownloadErrorCode::OutputFileNotFound);
            details.message = format!(
                "Le fichier téléchargé '{}' est introuvable sur le disque.",
                path.display()
            );
            return Err(CoreError::DownloadFailed(details));
        }

        Ok(path)
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
        let lock = self.active_jobs.lock().await;
        let job = lock.get(&job_id).ok_or(CoreError::JobNotFound(job_id))?;

        if job.status() != DownloadStatus::Completed {
            let mut details =
                DownloadErrorDetails::from_code(DownloadErrorCode::OutputFileNotFound);
            details.message = format!("Le téléchargement '{job_id}' n'est pas encore terminé.");
            return Err(CoreError::DownloadFailed(details));
        }

        let path_str = job.destination_path().ok_or_else(|| {
            let mut details =
                DownloadErrorDetails::from_code(DownloadErrorCode::OutputFileNotFound);
            details.message =
                format!("Chemin de destination introuvable pour le téléchargement '{job_id}'.");
            CoreError::DownloadFailed(details)
        })?;

        let path = PathBuf::from(path_str);
        if !path.exists() || !path.is_file() {
            let mut details =
                DownloadErrorDetails::from_code(DownloadErrorCode::OutputFileNotFound);
            details.message = format!(
                "Le fichier téléchargé '{}' est introuvable sur le disque.",
                path.display()
            );
            return Err(CoreError::DownloadFailed(details));
        }

        Ok(path)
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
                    parallel_segments: ctx.parallel_segments,
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
            if let Ok(history_entry) = DownloadHistoryEntry::new(
                ctx.job_id,
                ctx.url,
                downloaded_streams.title,
                ctx.preset,
                final_path_str,
                None,
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

/// Publishes the converted media file to the destination directory without overwriting any existing file.
async fn publish_media_no_clobber(
    temp_converted_file: &Path,
    target_dir: &Path,
    base_name: &str,
    ext: &str,
    publish_mutex: &Arc<Mutex<()>>,
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
                        let _ = dest_file.sync_all();
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
