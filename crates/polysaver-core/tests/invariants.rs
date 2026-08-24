// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use async_trait::async_trait;
use polysaver_core::domain::download_job::DownloadId;
use polysaver_core::domain::history::{DownloadHistoryEntry, HistoryEntryId};
use polysaver_core::domain::{
    AppSettings, AppSettingsDto, DownloadJob, DownloadPreset, DownloadPresetDto, DownloadStatus,
    Language, MediaUrl, Mp3Quality, OutputFormat, ThemeMode, VideoQuality,
};
use polysaver_core::error::{CoreError, DownloadErrorDetails};
use polysaver_core::ports::event_sink::EventSink;
use polysaver_core::ports::history_repository::DownloadHistoryRepository;
use polysaver_core::ports::media_inspector::{MediaInspector, MediaStreamInfo};
use polysaver_core::ports::{
    ConvertRequest, DownloadStreamRequest, DownloadedStreams, MediaConverter, MediaDownloader,
    SettingsRepository, StreamProgress,
};
use polysaver_core::services::StartDownloadService;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

// 1. OutputFormat all 4 variants accepted
#[test]
fn test_all_output_formats_accepted() {
    assert!(OutputFormat::Mp4.is_video());
    assert!(OutputFormat::Mov.is_video());
    assert!(OutputFormat::Mp3.is_audio());
    assert!(OutputFormat::Flac.is_audio());

    assert_eq!(OutputFormat::Mp4.extension(), "mp4");
    assert_eq!(OutputFormat::Mov.extension(), "mov");
    assert_eq!(OutputFormat::Mp3.extension(), "mp3");
    assert_eq!(OutputFormat::Flac.extension(), "flac");
}

// 2. Combination MP4 + video quality accepted
#[test]
fn test_mp4_with_video_quality_accepted() {
    let preset = DownloadPreset::video(OutputFormat::Mp4, VideoQuality::P1080);
    assert!(preset.is_ok());
    let p = preset.unwrap();
    assert_eq!(p.format(), OutputFormat::Mp4);
    assert_eq!(p.video_quality(), Some(VideoQuality::P1080));
    assert_eq!(p.mp3_quality(), None);
}

// 3. Combination MOV + video quality accepted
#[test]
fn test_mov_with_video_quality_accepted() {
    let preset = DownloadPreset::video(OutputFormat::Mov, VideoQuality::Best);
    assert!(preset.is_ok());
    let p = preset.unwrap();
    assert_eq!(p.format(), OutputFormat::Mov);
    assert_eq!(p.video_quality(), Some(VideoQuality::Best));
}

// 4. Combination MP3 + MP3 quality accepted
#[test]
fn test_mp3_with_quality_accepted() {
    let preset = DownloadPreset::mp3(Mp3Quality::K320);
    assert_eq!(preset.format(), OutputFormat::Mp3);
    assert_eq!(preset.mp3_quality(), Some(Mp3Quality::K320));
    assert_eq!(preset.video_quality(), None);
}

// 5. Combination FLAC without quality accepted
#[test]
fn test_flac_without_quality_accepted() {
    let preset = DownloadPreset::flac();
    assert_eq!(preset.format(), OutputFormat::Flac);
    assert_eq!(preset.mp3_quality(), None);
    assert_eq!(preset.video_quality(), None);
}

// 6. MP3 without quality rejected via DTO
#[test]
fn test_mp3_without_quality_rejected_via_dto() {
    let dto = DownloadPresetDto {
        format: OutputFormat::Mp3,
        video_quality: None,
        mp3_quality: None,
    };
    let result = DownloadPreset::try_from(dto);
    assert!(matches!(result, Err(CoreError::InvalidSettings(_))));
}

// 7. FLAC with quality rejected via DTO
#[test]
fn test_flac_with_quality_rejected_via_dto() {
    let dto_video_q = DownloadPresetDto {
        format: OutputFormat::Flac,
        video_quality: Some(VideoQuality::P1080),
        mp3_quality: None,
    };
    assert!(DownloadPreset::try_from(dto_video_q).is_err());

    let dto_mp3_q = DownloadPresetDto {
        format: OutputFormat::Flac,
        video_quality: None,
        mp3_quality: Some(Mp3Quality::K320),
    };
    assert!(DownloadPreset::try_from(dto_mp3_q).is_err());
}

// 8. Video format with audio quality rejected via DTO
#[test]
fn test_video_format_with_audio_quality_rejected_via_dto() {
    let dto = DownloadPresetDto {
        format: OutputFormat::Mp4,
        video_quality: Some(VideoQuality::P1080),
        mp3_quality: Some(Mp3Quality::K192),
    };
    let result = DownloadPreset::try_from(dto);
    assert!(matches!(result, Err(CoreError::InvalidSettings(_))));
}

// 9. Playlist URLs rejected
#[test]
fn test_playlist_urls_rejected() {
    let list_param = MediaUrl::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PL12345");
    assert!(matches!(
        list_param,
        Err(CoreError::PlaylistNotSupported(_))
    ));
    assert_eq!(
        list_param.unwrap_err().machine_code(),
        "PLAYLIST_NOT_SUPPORTED"
    );

    let playlist_path = MediaUrl::parse("https://www.youtube.com/playlist?list=PL12345");
    assert!(matches!(
        playlist_path,
        Err(CoreError::PlaylistNotSupported(_))
    ));

    let channel_path = MediaUrl::parse("https://www.youtube.com/channel/UC12345678");
    assert!(matches!(
        channel_path,
        Err(CoreError::PlaylistNotSupported(_))
    ));
}

// 10. Valid URLs accepted
#[test]
fn test_valid_urls_accepted() {
    let http_url = MediaUrl::parse("http://example.com/video.mp4");
    assert!(http_url.is_ok());
    assert_eq!(http_url.unwrap().as_str(), "http://example.com/video.mp4");

    let https_url = MediaUrl::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
    assert!(https_url.is_ok());
}

// 11. Disallowed schemes and empty URLs rejected
#[test]
fn test_disallowed_urls_rejected() {
    assert!(matches!(MediaUrl::parse(""), Err(CoreError::InvalidUrl(_))));
    assert!(matches!(
        MediaUrl::parse("file:///etc/passwd"),
        Err(CoreError::InvalidUrl(_))
    ));
    assert!(matches!(
        MediaUrl::parse("ftp://example.com/video.mp4"),
        Err(CoreError::InvalidUrl(_))
    ));
}

// 12. Concurrency policies in AppSettings
#[test]
fn test_app_settings_concurrency_policies() {
    let parallel_off = AppSettings::new(
        "/home/user/downloads".to_string(),
        ThemeMode::Dark,
        false, // parallel disabled
        DownloadPreset::default(),
        3,
        Language::French,
    )
    .unwrap();
    assert_eq!(parallel_off.effective_max_concurrent(), 1);
    assert_eq!(parallel_off.effective_parallel_segments(), 1);

    let parallel_on = AppSettings::new(
        "/home/user/downloads".to_string(),
        ThemeMode::Light,
        true, // parallel enabled
        DownloadPreset::default(),
        5,
        Language::English,
    )
    .unwrap();
    assert_eq!(parallel_on.effective_max_concurrent(), 5);
    assert_eq!(parallel_on.effective_parallel_segments(), 8);

    // Bounds invariant: max_concurrent must be 1..=8
    assert!(AppSettings::new(
        "/home/user/downloads".to_string(),
        ThemeMode::Dark,
        true,
        DownloadPreset::default(),
        0,
        Language::French,
    )
    .is_err());
    assert!(AppSettings::new(
        "/home/user/downloads".to_string(),
        ThemeMode::Dark,
        true,
        DownloadPreset::default(),
        9,
        Language::French,
    )
    .is_err());

    // Path validation: ~ prefix, relative path, empty path, null bytes rejected
    assert!(AppSettings::new(
        "~/Downloads".to_string(),
        ThemeMode::Dark,
        true,
        DownloadPreset::default(),
        3,
        Language::French,
    )
    .is_err());
    assert!(AppSettings::new(
        "relative/path".to_string(),
        ThemeMode::Dark,
        true,
        DownloadPreset::default(),
        3,
        Language::French,
    )
    .is_err());
    assert!(AppSettings::new(
        "/path/with\0null".to_string(),
        ThemeMode::Dark,
        true,
        DownloadPreset::default(),
        3,
        Language::French,
    )
    .is_err());
}

// 13. DownloadJob lifecycle transitions and monotonicity
#[test]
fn test_download_job_lifecycle() {
    let url = MediaUrl::parse("https://example.com/video").unwrap();
    let preset = DownloadPreset::video(OutputFormat::Mp4, VideoQuality::P720).unwrap();
    let mut job = DownloadJob::new(url, preset);

    assert_eq!(job.status(), DownloadStatus::Queued);
    assert_eq!(job.progress_percent(), None);

    job.transition_to_preparing().unwrap();
    assert_eq!(job.status(), DownloadStatus::Preparing);

    job.transition_to_downloading().unwrap();
    assert_eq!(job.status(), DownloadStatus::Downloading);

    job.update_progress(Some(40), Some(4_000_000), Some(10_000_000), Some(1_000_000))
        .unwrap();
    assert_eq!(job.progress_percent(), Some(40));
    assert_eq!(job.downloaded_bytes(), Some(4_000_000));

    // Decreasing progress rejected
    assert!(job.update_progress(Some(35), None, None, None).is_err());

    job.transition_to_converting().unwrap();
    assert_eq!(job.status(), DownloadStatus::Converting);
    assert_eq!(job.progress_percent(), None);
    assert_eq!(job.speed_bytes_per_second(), None);

    job.transition_to_finalizing().unwrap();
    assert_eq!(job.status(), DownloadStatus::Finalizing);

    job.transition_to_completed("/output/video.mp4".to_string())
        .unwrap();
    assert_eq!(job.status(), DownloadStatus::Completed);
    assert_eq!(job.progress_percent(), None);
    assert_eq!(job.destination_path(), Some("/output/video.mp4"));
    assert!(job.is_terminal());

    // Terminal job cannot be mutated
    assert!(job.transition_to_downloading().is_err());
}

// 14. StartDownloadService with default, explicit presets, and custom output dir
struct FakeDownloader;
#[async_trait]
impl MediaDownloader for FakeDownloader {
    async fn download_stream(
        &self,
        request: DownloadStreamRequest,
        _cb: Arc<dyn Fn(StreamProgress) + Send + Sync>,
    ) -> Result<DownloadedStreams, CoreError> {
        let video_file = request.temp_dir.join("video.mp4");
        tokio::fs::write(&video_file, b"fake video").await.unwrap();
        Ok(DownloadedStreams {
            raw_artifacts: vec![video_file.clone()],
            video_path: Some(video_file),
            audio_path: None,
            title: "Test Video".to_string(),
            duration_seconds: Some(100),
        })
    }
}

struct FakeConverter;
#[async_trait]
impl MediaConverter for FakeConverter {
    async fn convert(
        &self,
        request: ConvertRequest,
        _cancellation_token: Option<tokio_util::sync::CancellationToken>,
        _progress_callback: Option<polysaver_core::ports::ConverterProgressCallback>,
    ) -> Result<PathBuf, CoreError> {
        let out = match request {
            ConvertRequest::VideoMuxOrTranscode { output_path, .. } => output_path,
            ConvertRequest::AudioTranscode { output_path, .. } => output_path,
        };
        tokio::fs::write(&out, b"converted data").await.unwrap();
        Ok(out)
    }
}

struct FakeInspector;
#[async_trait]
impl MediaInspector for FakeInspector {
    async fn inspect(&self, path: &std::path::Path) -> Result<MediaStreamInfo, CoreError> {
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let has_video = matches!(ext, "mp4" | "mkv" | "webm" | "mov");
        let has_audio = true;
        Ok(MediaStreamInfo {
            has_video,
            has_audio,
            video_codec: if has_video { Some("h264".into()) } else { None },
            audio_codec: Some("aac".into()),
        })
    }
}

struct InMemorySettingsRepo {
    settings: RwLock<AppSettings>,
}

#[async_trait]
impl SettingsRepository for InMemorySettingsRepo {
    async fn load(&self) -> Result<AppSettings, CoreError> {
        Ok(self.settings.read().await.clone())
    }

    async fn save(&self, settings: &AppSettings) -> Result<(), CoreError> {
        *self.settings.write().await = settings.clone();
        Ok(())
    }
}

#[derive(Default)]
struct InMemoryTestHistoryRepo {
    entries: RwLock<Vec<DownloadHistoryEntry>>,
}

#[async_trait]
impl DownloadHistoryRepository for InMemoryTestHistoryRepo {
    async fn load(&self) -> Result<Vec<DownloadHistoryEntry>, CoreError> {
        let lock = self.entries.read().await;
        Ok(lock.clone())
    }

    async fn save(&self, entries: &[DownloadHistoryEntry]) -> Result<(), CoreError> {
        *self.entries.write().await = entries.to_vec();
        Ok(())
    }

    async fn append(&self, entry: DownloadHistoryEntry) -> Result<(), CoreError> {
        let mut lock = self.entries.write().await;
        lock.retain(|e| e.download_id() != entry.download_id());
        lock.insert(0, entry);
        Ok(())
    }

    async fn remove(&self, id: HistoryEntryId) -> Result<(), CoreError> {
        let mut lock = self.entries.write().await;
        lock.retain(|e| e.id() != id);
        Ok(())
    }
}

#[tokio::test]
async fn test_start_download_service_uses_default_preset_and_custom_override() {
    let test_dir = std::env::temp_dir().join(format!("polysaver_test_{}", uuid::Uuid::new_v4()));
    let download_dir = test_dir.join("downloads");
    tokio::fs::create_dir_all(&download_dir).await.unwrap();

    let settings = AppSettings::new(
        download_dir.to_string_lossy().to_string(),
        ThemeMode::Dark,
        false,
        DownloadPreset::mp3(Mp3Quality::K320),
        3,
        Language::French,
    )
    .unwrap();

    let repo = Arc::new(InMemorySettingsRepo {
        settings: RwLock::new(settings),
    });
    let history_repo = Arc::new(InMemoryTestHistoryRepo::default());

    let service = StartDownloadService::new(
        Arc::new(FakeDownloader),
        Arc::new(FakeConverter),
        Arc::new(FakeInspector),
        repo.clone(),
        history_repo.clone(),
        None,
        test_dir.join("temp"),
    );

    // When no preset passed, applies default (MP3 320k)
    let job1 = service
        .start_download("https://www.youtube.com/watch?v=dQw4w9WgXcQ", None, None)
        .await
        .unwrap();
    assert_eq!(job1.preset(), DownloadPreset::mp3(Mp3Quality::K320));

    // When explicit preset passed, overrides without modifying stored settings
    let custom_preset = DownloadPreset::video(OutputFormat::Mov, VideoQuality::P720).unwrap();
    let job2 = service
        .start_download(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            Some(custom_preset),
            None,
        )
        .await
        .unwrap();
    assert_eq!(job2.preset(), custom_preset);

    // Stored settings unchanged
    let loaded = repo.load().await.unwrap();
    assert_eq!(
        loaded.default_preset(),
        DownloadPreset::mp3(Mp3Quality::K320)
    );

    // Give background task time to complete and record history
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let history = service.list_history().await.unwrap();
    assert!(!history.is_empty());

    let _ = tokio::fs::remove_dir_all(&test_dir).await;
}

// 15. Failing downloader triggers Failed state in service
struct FailingDownloader;
#[async_trait]
impl MediaDownloader for FailingDownloader {
    async fn download_stream(
        &self,
        _request: DownloadStreamRequest,
        _cb: Arc<dyn Fn(StreamProgress) + Send + Sync>,
    ) -> Result<DownloadedStreams, CoreError> {
        Err(CoreError::ProviderError("yt-dlp stream error".to_string()))
    }
}

#[tokio::test]
async fn test_failing_downloader_transitions_to_failed() {
    let test_dir = std::env::temp_dir().join(format!("polysaver_test_{}", uuid::Uuid::new_v4()));
    let settings = AppSettings::defaults_for(test_dir.join("downloads")).unwrap();
    let repo = Arc::new(InMemorySettingsRepo {
        settings: RwLock::new(settings),
    });
    let history_repo = Arc::new(InMemoryTestHistoryRepo::default());

    let service = StartDownloadService::new(
        Arc::new(FailingDownloader),
        Arc::new(FakeConverter),
        Arc::new(FakeInspector),
        repo,
        history_repo,
        None,
        test_dir.join("temp"),
    );

    let job = service
        .start_download("https://www.youtube.com/watch?v=dQw4w9WgXcQ", None, None)
        .await
        .unwrap();

    // Give background task time to run and fail
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let downloads = service.list_downloads().await;
    let found = downloads.iter().find(|j| j.id() == job.id()).unwrap();
    assert_eq!(found.status(), DownloadStatus::Failed);
    assert!(found
        .error_message()
        .unwrap()
        .contains("yt-dlp stream error"));

    let _ = tokio::fs::remove_dir_all(&test_dir).await;
}

// 16. AppSettingsDto TryFrom validation and default System theme mode and French language
#[test]
fn test_app_settings_dto_try_from() {
    let dto = AppSettingsDto {
        download_directory: "/path/to/downloads".to_string(),
        theme_mode: ThemeMode::System,
        parallel_downloads: true,
        default_preset: DownloadPresetDto {
            format: OutputFormat::Flac,
            video_quality: None,
            mp3_quality: None,
        },
        max_concurrent: 4,
        language: Language::English,
    };

    let settings = AppSettings::try_from(dto).unwrap();
    assert_eq!(settings.download_directory(), "/path/to/downloads");
    assert_eq!(settings.theme_mode(), ThemeMode::System);
    assert!(settings.parallel_downloads());
    assert_eq!(settings.default_preset(), DownloadPreset::Flac);
    assert_eq!(settings.max_concurrent(), 4);
    assert_eq!(settings.language(), Language::English);

    // Default settings must use ThemeMode::System and Language::French
    let default_settings = AppSettings::defaults_for("/tmp/downloads").unwrap();
    assert_eq!(default_settings.theme_mode(), ThemeMode::System);
    assert_eq!(default_settings.language(), Language::French);
}

// 17. Language serialization, deserialization, and defaults
#[test]
fn test_language_serialization_and_defaults() {
    assert_eq!(serde_json::to_string(&Language::French).unwrap(), "\"fr\"");
    assert_eq!(serde_json::to_string(&Language::English).unwrap(), "\"en\"");

    assert_eq!(
        serde_json::from_str::<Language>("\"fr\"").unwrap(),
        Language::French
    );
    assert_eq!(
        serde_json::from_str::<Language>("\"en\"").unwrap(),
        Language::English
    );

    assert_eq!(Language::French.code(), "fr");
    assert_eq!(Language::English.code(), "en");
    assert_eq!(Language::default(), Language::French);

    // Deserialization without language defaults to French
    let json_without_language = r#"{
        "downloadDirectory": "/downloads",
        "themeMode": "system",
        "parallelDownloads": true,
        "defaultPreset": {
            "format": "mp3",
            "mp3Quality": "k320"
        },
        "maxConcurrent": 3
    }"#;
    let dto: AppSettingsDto = serde_json::from_str(json_without_language).unwrap();
    assert_eq!(dto.language, Language::French);
}

// 18. ThemeMode serialization and deserialization
#[test]
fn test_theme_mode_serialization() {
    assert_eq!(
        serde_json::to_string(&ThemeMode::Light).unwrap(),
        "\"light\""
    );
    assert_eq!(serde_json::to_string(&ThemeMode::Dark).unwrap(), "\"dark\"");
    assert_eq!(
        serde_json::to_string(&ThemeMode::System).unwrap(),
        "\"system\""
    );

    assert_eq!(
        serde_json::from_str::<ThemeMode>("\"light\"").unwrap(),
        ThemeMode::Light
    );
    assert_eq!(
        serde_json::from_str::<ThemeMode>("\"dark\"").unwrap(),
        ThemeMode::Dark
    );
    assert_eq!(
        serde_json::from_str::<ThemeMode>("\"system\"").unwrap(),
        ThemeMode::System
    );
}

// 19. DownloadErrorCode and DownloadErrorDetails serialization and invariants
#[test]
fn test_download_error_code_and_details() {
    use polysaver_core::error::{DownloadErrorCode, DownloadErrorDetails};

    let codes = [
        (
            DownloadErrorCode::YtdlpNotFound,
            "\"YTDLP_NOT_FOUND\"",
            false,
        ),
        (
            DownloadErrorCode::YtdlpStartFailed,
            "\"YTDLP_START_FAILED\"",
            false,
        ),
        (
            DownloadErrorCode::YtdlpUpdateRequired,
            "\"YTDLP_UPDATE_REQUIRED\"",
            false,
        ),
        (
            DownloadErrorCode::NetworkUnavailable,
            "\"NETWORK_UNAVAILABLE\"",
            true,
        ),
        (
            DownloadErrorCode::VideoUnavailable,
            "\"VIDEO_UNAVAILABLE\"",
            false,
        ),
        (
            DownloadErrorCode::AuthenticationRequired,
            "\"AUTHENTICATION_REQUIRED\"",
            false,
        ),
        (DownloadErrorCode::RateLimited, "\"RATE_LIMITED\"", true),
        (
            DownloadErrorCode::FormatNotAvailable,
            "\"FORMAT_NOT_AVAILABLE\"",
            false,
        ),
        (
            DownloadErrorCode::FfmpegNotFound,
            "\"FFMPEG_NOT_FOUND\"",
            false,
        ),
        (DownloadErrorCode::FfmpegFailed, "\"FFMPEG_FAILED\"", false),
        (
            DownloadErrorCode::OutputPermissionDenied,
            "\"OUTPUT_PERMISSION_DENIED\"",
            false,
        ),
        (
            DownloadErrorCode::OutputFileNotFound,
            "\"OUTPUT_FILE_NOT_FOUND\"",
            false,
        ),
        (
            DownloadErrorCode::SourceUrlInvalid,
            "\"SOURCE_URL_INVALID\"",
            false,
        ),
        (
            DownloadErrorCode::HistorySaveFailed,
            "\"HISTORY_SAVE_FAILED\"",
            true,
        ),
        (DownloadErrorCode::DiskFull, "\"DISK_FULL\"", false),
        (
            DownloadErrorCode::DownloadCanceled,
            "\"DOWNLOAD_CANCELED\"",
            false,
        ),
        (
            DownloadErrorCode::DownloadProcessFailed,
            "\"DOWNLOAD_PROCESS_FAILED\"",
            true,
        ),
    ];

    for (code, expected_json, expected_retryable) in codes {
        assert_eq!(serde_json::to_string(&code).unwrap(), expected_json);
        assert_eq!(code.is_retryable(), expected_retryable);
        assert!(!code.default_user_message().is_empty());
    }

    let details = DownloadErrorDetails::from_code(DownloadErrorCode::NetworkUnavailable);
    let serialized = serde_json::to_string(&details).unwrap();
    assert!(serialized.contains("\"code\":\"NETWORK_UNAVAILABLE\""));
    assert!(serialized.contains("\"retryable\":true"));
}

#[test]
fn test_available_video_qualities_filtering_and_sorting() {
    use polysaver_core::domain::probe::{FormatOption, ProbeResult};

    let formats = vec![
        FormatOption {
            format_id: "sb0".to_string(),
            height: Some(90),
            has_video: false,
            has_audio: false,
            extension: "mhtml".to_string(),
            filesize_approx_bytes: None,
            note: Some("storyboard".to_string()),
        },
        FormatOption {
            format_id: "140".to_string(),
            height: None,
            has_video: false,
            has_audio: true,
            extension: "m4a".to_string(),
            filesize_approx_bytes: None,
            note: Some("audio".to_string()),
        },
        FormatOption {
            format_id: "278".to_string(),
            height: Some(144),
            has_video: true,
            has_audio: false,
            extension: "webm".to_string(),
            filesize_approx_bytes: None,
            note: None,
        },
        FormatOption {
            format_id: "247".to_string(),
            height: Some(720),
            has_video: true,
            has_audio: false,
            extension: "webm".to_string(),
            filesize_approx_bytes: None,
            note: None,
        },
        FormatOption {
            format_id: "136".to_string(),
            height: Some(720),
            has_video: true,
            has_audio: false,
            extension: "mp4".to_string(),
            filesize_approx_bytes: None,
            note: None,
        },
        FormatOption {
            format_id: "248".to_string(),
            height: Some(1080),
            has_video: true,
            has_audio: false,
            extension: "webm".to_string(),
            filesize_approx_bytes: None,
            note: None,
        },
    ];

    let probe = ProbeResult::new(
        MediaUrl::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap(),
        "Rick Astley".to_string(),
        Some(212),
        None,
        Some("RickAstleyVEVO".to_string()),
        formats,
    );

    assert!(probe.has_video_stream());
    let available = &probe.available_video_qualities;

    assert_eq!(
        available,
        &vec![VideoQuality::P1080, VideoQuality::P720, VideoQuality::P144]
    );
}

// 20. MediaUrl strict Serde deserialization invariants
#[test]
fn test_media_url_serde_strict_deserialization() {
    // Valid URL deserializes properly
    let valid_json = "\"https://www.youtube.com/watch?v=dQw4w9WgXcQ\"";
    let url: MediaUrl = serde_json::from_str(valid_json).unwrap();
    assert_eq!(url.as_str(), "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
    assert_eq!(serde_json::to_string(&url).unwrap(), valid_json);

    // Empty string rejected
    assert!(serde_json::from_str::<MediaUrl>("\"\"").is_err());
    assert!(serde_json::from_str::<MediaUrl>("\"   \"").is_err());

    // Relative path rejected
    assert!(serde_json::from_str::<MediaUrl>("\"/video.mp4\"").is_err());

    // Javascript scheme rejected
    assert!(serde_json::from_str::<MediaUrl>("\"javascript:alert(1)\"").is_err());

    // Playlist URL rejected
    assert!(
        serde_json::from_str::<MediaUrl>("\"https://youtube.com/playlist?list=PL123\"").is_err()
    );
}

#[tokio::test]
async fn test_get_completed_download_path_validation() {
    use polysaver_core::domain::{AppSettings, DownloadPreset, OutputFormat, VideoQuality};
    use polysaver_core::ports::settings_repository::SettingsRepository;
    use polysaver_core::services::StartDownloadService;
    use std::sync::Arc;

    struct DummySettingsRepo;
    #[async_trait::async_trait]
    impl SettingsRepository for DummySettingsRepo {
        async fn load(&self) -> Result<AppSettings, CoreError> {
            AppSettings::defaults_for("/tmp/downloads")
        }
        async fn save(&self, _: &AppSettings) -> Result<(), CoreError> {
            Ok(())
        }
    }

    let service = StartDownloadService::new(
        Arc::new(FakeDownloader),
        Arc::new(FakeConverter),
        Arc::new(FakeInspector),
        Arc::new(DummySettingsRepo),
        Arc::new(InMemoryTestHistoryRepo::default()),
        None,
        std::env::temp_dir(),
    );

    let job = service
        .start_download(
            "https://www.youtube.com/watch?v=jNQXAC9IVRw",
            Some(DownloadPreset::video(OutputFormat::Mp4, VideoQuality::P720).unwrap()),
            None,
        )
        .await
        .unwrap();

    let job_id = job.id();

    let result = service.get_completed_download_path(job_id).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.machine_code(), "OUTPUT_FILE_NOT_FOUND");
}

#[tokio::test]
async fn test_download_history_entry_invariants_and_actions() {
    let job_id = DownloadId::new();
    let url = MediaUrl::parse("https://www.youtube.com/watch?v=jNQXAC9IVRw").unwrap();
    let preset = DownloadPreset::video(OutputFormat::Mp4, VideoQuality::P720).unwrap();

    // 1. Valid entry construction
    let entry = DownloadHistoryEntry::new(
        job_id,
        url.clone(),
        "  Me at the zoo  ".to_string(),
        preset,
        "/downloads/zoo.mp4".to_string(),
        Some(123456789),
    )
    .unwrap();

    assert_eq!(entry.title(), "Me at the zoo");
    assert_eq!(entry.destination_path(), "/downloads/zoo.mp4");
    assert_eq!(entry.completed_at(), 123456789);
    assert_eq!(entry.download_id(), job_id);

    // 2. Empty destination path rejected
    let empty_dest = DownloadHistoryEntry::new(
        job_id,
        url.clone(),
        "Title".to_string(),
        preset,
        "   ".to_string(),
        None,
    );
    assert!(matches!(empty_dest, Err(CoreError::ValidationError(_))));
}

#[test]
fn test_sanitize_filename_bounded_utf8_and_windows_reserved() {
    use polysaver_core::services::start_download::sanitize_filename;

    // Normal title
    assert_eq!(sanitize_filename("Simple Title"), "Simple Title");

    // Forbidden characters sanitized to underscore
    assert_eq!(
        sanitize_filename("What? A / B \\ C * D < E > F : G \" H | I % J"),
        "What_ A _ B _ C _ D _ E _ F _ G _ H _ I _ J"
    );

    // Windows reserved names protected
    assert_eq!(sanitize_filename("CON"), "PolySaver_CON");
    assert_eq!(sanitize_filename("prn"), "PolySaver_prn");
    assert_eq!(sanitize_filename("aux"), "PolySaver_aux");
    assert_eq!(sanitize_filename("NUL"), "PolySaver_NUL");
    assert_eq!(sanitize_filename("COM1"), "PolySaver_COM1");
    assert_eq!(sanitize_filename("lpt1"), "PolySaver_lpt1");

    // Trailing dots and spaces trimmed
    assert_eq!(
        sanitize_filename("Title with dots...   "),
        "Title with dots"
    );

    // Empty fallback
    assert_eq!(sanitize_filename(""), "PolySaver_Media");
    assert_eq!(sanitize_filename("..."), "PolySaver_Media");

    // Extremely long multi-byte UTF-8 string bounded cleanly at char boundary <= 180 bytes
    let long_japanese = "こんにちは世界".repeat(20); // 20 * 7 * 3 = 420 bytes
    let sanitized_long = sanitize_filename(&long_japanese);
    assert!(sanitized_long.len() <= 180);
    assert!(std::str::from_utf8(sanitized_long.as_bytes()).is_ok());
}

#[derive(Default)]
struct TestEventSink {
    queued_count: std::sync::atomic::AtomicUsize,
    progress_count: std::sync::atomic::AtomicUsize,
    completed_count: std::sync::atomic::AtomicUsize,
    failed_count: std::sync::atomic::AtomicUsize,
    canceled_count: std::sync::atomic::AtomicUsize,
}

impl EventSink for TestEventSink {
    fn emit_queued(&self, _job: &DownloadJob) {
        self.queued_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    fn emit_progress(&self, _progress: &polysaver_core::ports::DownloadProgressEvent) {
        self.progress_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    fn emit_completed(&self, _job: &DownloadJob) {
        self.completed_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    fn emit_failed(&self, _job: &DownloadJob, _error: &DownloadErrorDetails) {
        self.failed_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    fn emit_canceled(&self, _job: &DownloadJob) {
        self.canceled_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

struct BlockingFakeDownloader;
#[async_trait]
impl MediaDownloader for BlockingFakeDownloader {
    async fn download_stream(
        &self,
        request: DownloadStreamRequest,
        cb: Arc<dyn Fn(StreamProgress) + Send + Sync>,
    ) -> Result<DownloadedStreams, CoreError> {
        cb(StreamProgress {
            percent: Some(25),
            downloaded_bytes: Some(250),
            total_bytes: Some(1000),
            speed_bytes_per_second: Some(100),
        });

        if let Some(token) = request.cancellation_token {
            token.cancelled().await;
            return Err(CoreError::OperationCancelled);
        }

        let video_file = request.temp_dir.join("video.mp4");
        tokio::fs::write(&video_file, b"video").await.unwrap();
        Ok(DownloadedStreams {
            raw_artifacts: vec![video_file.clone()],
            video_path: Some(video_file),
            audio_path: None,
            title: "Test Video".to_string(),
            duration_seconds: Some(100),
        })
    }
}

#[tokio::test]
async fn test_cancel_and_dismiss_download_state_validation() {
    let test_dir = std::env::temp_dir().join(format!("polysaver_cancel_{}", uuid::Uuid::new_v4()));
    let settings = AppSettings::defaults_for(test_dir.join("downloads")).unwrap();
    let repo = Arc::new(InMemorySettingsRepo {
        settings: RwLock::new(settings),
    });
    let history_repo = Arc::new(InMemoryTestHistoryRepo::default());
    let sink = Arc::new(TestEventSink::default());

    let service = StartDownloadService::new(
        Arc::new(BlockingFakeDownloader),
        Arc::new(FakeConverter),
        Arc::new(FakeInspector),
        repo,
        history_repo,
        Some(sink.clone()),
        test_dir.join("temp"),
    );

    let job = service
        .start_download("https://www.youtube.com/watch?v=dQw4w9WgXcQ", None, None)
        .await
        .unwrap();

    let job_id = job.id();

    // 1. Instant cancellation returns Canceled status immediately
    let canceled_job = service.cancel_download(job_id).await.unwrap();
    assert_eq!(canceled_job.status(), DownloadStatus::Canceled);

    // 2. Active list reflects Canceled immediately
    let listed = service.list_downloads().await;
    let found = listed.iter().find(|j| j.id() == job_id).unwrap();
    assert_eq!(found.status(), DownloadStatus::Canceled);

    // 3. Second cancellation is idempotent, returns Canceled without error
    let second_cancel = service.cancel_download(job_id).await.unwrap();
    assert_eq!(second_cancel.status(), DownloadStatus::Canceled);

    // 4. Verify sink received exactly 1 canceled event and 0 completed or failed
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(
        sink.canceled_count
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(
        sink.completed_count
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    assert_eq!(
        sink.failed_count.load(std::sync::atomic::Ordering::SeqCst),
        0
    );

    // 5. Dismiss succeeds on canceled job
    let dismiss_res = service.dismiss_download(job_id).await;
    assert!(dismiss_res.is_ok());

    let _ = tokio::fs::remove_dir_all(&test_dir).await;
}

#[tokio::test]
async fn test_cannot_cancel_completed_job() {
    let test_dir = std::env::temp_dir().join(format!(
        "polysaver_completed_cancel_{}",
        uuid::Uuid::new_v4()
    ));
    let settings = AppSettings::defaults_for(test_dir.join("downloads")).unwrap();
    let repo = Arc::new(InMemorySettingsRepo {
        settings: RwLock::new(settings),
    });
    let history_repo = Arc::new(InMemoryTestHistoryRepo::default());

    let service = StartDownloadService::new(
        Arc::new(FakeDownloader),
        Arc::new(FakeConverter),
        Arc::new(FakeInspector),
        repo,
        history_repo,
        None,
        test_dir.join("temp"),
    );

    let job = service
        .start_download("https://www.youtube.com/watch?v=dQw4w9WgXcQ", None, None)
        .await
        .unwrap();

    let job_id = job.id();

    // Wait for background worker to complete
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Cancellation of completed job is rejected
    let cancel_res = service.cancel_download(job_id).await;
    assert!(cancel_res.is_err());
    assert!(matches!(
        cancel_res.unwrap_err(),
        CoreError::InvalidState(_)
    ));

    let _ = tokio::fs::remove_dir_all(&test_dir).await;
}
