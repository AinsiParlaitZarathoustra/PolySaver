// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use async_trait::async_trait;
use polysaver_core::domain::download_job::DownloadId;
use polysaver_core::domain::history::{DownloadHistoryEntry, HistoryEntryId};
use polysaver_core::domain::{
    AppSettings, AppSettingsDto, DownloadJob, DownloadPreset, DownloadPresetDto, DownloadStatus,
    Language, MediaUrl, MediaUrlKind, Mp3Quality, OutputFormat, ThemeMode, VideoQuality,
    MAX_PLAYLIST_DOWNLOADS,
};
use polysaver_core::error::{CoreError, DownloadErrorDetails};
use polysaver_core::ports::event_sink::EventSink;
use polysaver_core::ports::history_repository::DownloadHistoryRepository;
use polysaver_core::ports::media_inspector::{MediaInspector, MediaStreamInfo};
use polysaver_core::ports::{
    ConvertRequest, DownloadStreamRequest, DownloadedStreams, MediaConverter, MediaDownloader,
    SettingsRepository, StreamProgress,
};
use polysaver_core::services::{RetryPolicy, StartDownloadService};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Builds an absolute path that is valid on every platform.
///
/// `/home/user/x` is absolute on Unix but **not** on Windows, so tests that feed
/// paths into validated domain objects must not hardcode a Unix literal. The
/// domain's `is_absolute` check is what makes this matter: a relative-looking
/// literal turns a passing test into a `ValidationError` on the Windows runner.
fn abs_path(suffix: &str) -> String {
    abs_path_os().join(suffix).to_string_lossy().to_string()
}

/// Absolute path whose text form is safe inside a JSON string literal.
///
/// A Windows path contains backslashes, which JSON would interpret as escape
/// sequences (`\p` is not valid), so the forward-slash form is used: it is still
/// an absolute path on Windows and requires no escaping.
fn abs_path_json(suffix: &str) -> String {
    abs_path_os()
        .join(suffix)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Platform-appropriate base directory for path fixtures.
fn abs_path_os() -> std::path::PathBuf {
    // `temp_dir()` is guaranteed absolute and exists on every platform, which
    // makes it a sounder fixture root than any hand-written literal.
    std::env::temp_dir().join("polysaver_test")
}

/// Polls `check` until it returns true or `timeout` elapses.
///
/// The download pipeline runs in a background task, so tests must wait for a
/// state instead of sleeping a fixed amount: a fixed sleep is flaky whenever the
/// CI runner is slower than the development machine (this is what made the Linux
/// leg fail while macOS passed).
async fn wait_until<F, Fut>(mut check: F, timeout: std::time::Duration) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if check().await {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// Waits (up to 10 s) for a job to reach any terminal state.
async fn wait_for_terminal(
    service: &StartDownloadService,
    job_id: polysaver_core::domain::DownloadId,
) {
    let reached = wait_until(
        || async {
            service
                .list_downloads()
                .await
                .iter()
                .find(|j| j.id() == job_id)
                .is_some_and(|j| j.is_terminal())
        },
        std::time::Duration::from_secs(10),
    )
    .await;
    assert!(
        reached,
        "job {job_id} did not reach a terminal state in time"
    );
}

/// Waits (up to 10 s) for the history repository to contain at least one entry.
async fn wait_for_history(service: &StartDownloadService) {
    let recorded = wait_until(
        || async { service.list_history().await.is_ok_and(|h| !h.is_empty()) },
        std::time::Duration::from_secs(10),
    )
    .await;
    assert!(recorded, "no history entry was recorded in time");
}

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

// 9. Playlist URLs are classified, not rejected
#[test]
fn test_playlist_urls_are_classified_as_playlists() {
    // A bare `list=` (no explicit video id) is a playlist.
    let list_only = MediaUrl::parse("https://www.youtube.com/playlist?list=PL12345")
        .expect("playlist URL must be accepted");
    assert_eq!(list_only.kind(), MediaUrlKind::Playlist);

    let channel_path = MediaUrl::parse("https://www.youtube.com/channel/UC12345678")
        .expect("channel URL must be accepted");
    assert_eq!(channel_path.kind(), MediaUrlKind::Playlist);

    // Channels and their sub-pages all resolve to the playlist flow.
    for raw in [
        "https://www.youtube.com/@handle",
        "https://www.youtube.com/@handle/videos",
        "https://www.youtube.com/@handle/streams",
        "https://www.youtube.com/c/SomeChannel",
        "https://www.youtube.com/user/SomeUser",
        "https://soundcloud.com/artist/sets/album",
    ] {
        let url = MediaUrl::parse(raw).expect("listing URL must be accepted");
        assert_eq!(
            url.kind(),
            MediaUrlKind::Playlist,
            "expected playlist for {raw}"
        );
    }

    // `list=` is a YouTube convention. Elsewhere it is an ordinary query parameter
    // (pagination, sorting, tracking) and must NOT switch the app into playlist
    // mode, otherwise the green button would be wrongly disabled.
    for raw in [
        "https://example.com/feed?list=abc",
        "https://vimeo.com/12345?list=xyz",
        "https://example.com/gallery?page=2&list=items",
    ] {
        let url = MediaUrl::parse(raw).expect("URL must be accepted");
        assert_eq!(
            url.kind(),
            MediaUrlKind::Single,
            "list= must not imply a playlist outside YouTube: {raw}"
        );
    }

    // A watch URL carrying a share `list=` stays a single video, and the parameter is
    // dropped so the download pipeline cannot accidentally follow the playlist.
    let shared = MediaUrl::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PL12345")
        .expect("share URL must be accepted");
    assert_eq!(shared.kind(), MediaUrlKind::Single);
    assert!(!shared.as_str().contains("list="));
    assert!(shared.as_str().contains("v=dQw4w9WgXcQ"));
    assert!(!shared.is_playlist());

    // Other query parameters are preserved by the normalization.
    let with_extra = MediaUrl::parse("https://www.youtube.com/watch?v=abc&t=42&list=PL9")
        .expect("share URL with extra params must be accepted");
    assert_eq!(with_extra.kind(), MediaUrlKind::Single);
    assert!(with_extra.as_str().contains("t=42"));
    assert!(!with_extra.as_str().contains("list="));

    // Plain videos are single.
    let plain = MediaUrl::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
        .expect("video URL must be accepted");
    assert_eq!(plain.kind(), MediaUrlKind::Single);
}

// 9b. Dangerous URLs stay rejected after the playlist change
#[test]
fn test_playlist_support_does_not_weaken_url_validation() {
    for raw in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "http://127.0.0.1/playlist?list=PL1",
        "http://localhost/playlist?list=PL1",
        "http://10.0.0.5/@handle",
        "http://[::1]/channel/UC1",
        "https://user:pass@youtube.com/playlist?list=PL1",
        "http://169.254.169.254/latest/meta-data",
        "https://youtube.com.evil.local/playlist?list=PL1",
    ] {
        assert!(
            MediaUrl::parse(raw).is_err(),
            "dangerous URL must stay rejected: {raw}"
        );
    }
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

// 11b. Local, private, and reserved hosts rejected (SSRF hardening)
#[test]
fn test_local_and_reserved_hosts_rejected() {
    let rejected = [
        "http://127.0.0.1/x",
        "http://127.1.2.3/x",
        "http://localhost/x",
        "http://localhost:8080/x",
        "http://foo.localhost/x",
        "http://printer.local/x",
        "http://service.internal/x",
        "http://0.1.2.3/x",
        "http://10.1.2.3/x",
        "http://172.16.0.1/x",
        "http://172.31.255.255/x",
        "http://192.168.1.10/x",
        "http://169.254.169.254/x",
        "http://100.64.0.1/x",
        "http://198.18.0.1/x",
        "http://192.0.2.10/x",
        "http://198.51.100.7/x",
        "http://203.0.113.9/x",
        "http://240.0.0.1/x",
        "http://224.0.0.1/x",
        "http://[::1]/x",
        "http://[::]/x",
        "http://[fc00::1]/x",
        "http://[fe80::1]/x",
        "http://[2001:db8::1]/x",
        "http://[ff02::1]/x",
        "http://[::ffff:127.0.0.1]/x",
        "http://[::ffff:10.0.0.1]/x",
        // Credentials can hide the real host; always refused.
        "https://user:pass@example.com/video",
        "https://github.com@127.0.0.1/x",
    ];
    for raw in rejected {
        assert!(
            matches!(MediaUrl::parse(raw), Err(CoreError::InvalidUrl(_))),
            "expected '{raw}' to be rejected"
        );
    }

    // Public hosts remain accepted.
    let accepted = [
        "https://www.youtube.com/watch?v=jNQXAC9IVRw",
        "http://example.com/video.mp4",
        "https://140.82.121.4/x",
        "https://[2606:50c0:8000::153]/x",
    ];
    for raw in accepted {
        assert!(
            MediaUrl::parse(raw).is_ok(),
            "expected '{raw}' to be accepted"
        );
    }
}

// 12. AppSettings path invariants
#[test]
fn test_app_settings_path_policies() {
    let video_settings = AppSettings::new(
        abs_path("home/user/downloads"),
        ThemeMode::Dark,
        DownloadPreset::default(),
        Language::French,
    )
    .unwrap();
    assert_eq!(
        video_settings.download_directory(),
        abs_path("home/user/downloads")
    );
    assert_eq!(video_settings.theme_mode(), ThemeMode::Dark);

    let audio_settings = AppSettings::new(
        abs_path("home/user/downloads"),
        ThemeMode::Light,
        DownloadPreset::mp3(Mp3Quality::K256),
        Language::English,
    )
    .unwrap();
    assert_eq!(
        audio_settings.default_preset(),
        DownloadPreset::mp3(Mp3Quality::K256)
    );
    assert_eq!(audio_settings.language(), Language::English);

    // Path validation: ~ prefix, relative path, empty path, null bytes rejected
    assert!(AppSettings::new(
        "~/Downloads".to_string(),
        ThemeMode::Dark,
        DownloadPreset::default(),
        Language::French,
    )
    .is_err());
    assert!(AppSettings::new(
        "relative/path".to_string(),
        ThemeMode::Dark,
        DownloadPreset::default(),
        Language::French,
    )
    .is_err());
    assert!(AppSettings::new(
        abs_path("path/with\0null"),
        ThemeMode::Dark,
        DownloadPreset::default(),
        Language::French,
    )
    .is_err());
    assert!(AppSettings::new(
        "   ".to_string(),
        ThemeMode::Dark,
        DownloadPreset::default(),
        Language::French,
    )
    .is_err());
}

// 12b. Cookie source and engine channel: closed enums, safe defaults, persistence
#[test]
fn test_settings_cookies_and_engine_channel() {
    use polysaver_core::domain::{CookiesBrowser, EngineChannel};

    let defaults = AppSettings::defaults_for(abs_path("downloads")).unwrap();
    assert_eq!(defaults.cookies_from_browser(), None);
    assert_eq!(defaults.engine_channel(), EngineChannel::Stable);

    // A configured browser is carried through the DTO round-trip.
    let settings = AppSettings::new(
        abs_path("downloads"),
        ThemeMode::System,
        DownloadPreset::default(),
        Language::French,
    )
    .unwrap()
    .with_cookies_from_browser(Some(CookiesBrowser::Firefox))
    .with_engine_channel(EngineChannel::Nightly);

    let dto = AppSettingsDto::from(&settings);
    assert_eq!(dto.cookies_from_browser, Some(CookiesBrowser::Firefox));
    assert_eq!(dto.engine_channel, EngineChannel::Nightly);
    assert_eq!(dto.schema_version, 1);

    let round_tripped = AppSettings::try_from(dto).unwrap();
    assert_eq!(
        round_tripped.cookies_from_browser(),
        Some(CookiesBrowser::Firefox)
    );
    assert_eq!(round_tripped.engine_channel(), EngineChannel::Nightly);

    // Every supported browser serializes to the exact yt-dlp flag value.
    let names = [
        CookiesBrowser::Brave,
        CookiesBrowser::Chrome,
        CookiesBrowser::Chromium,
        CookiesBrowser::Edge,
        CookiesBrowser::Firefox,
        CookiesBrowser::Opera,
        CookiesBrowser::Safari,
        CookiesBrowser::Vivaldi,
        CookiesBrowser::Whale,
    ];
    for browser in names {
        let json = serde_json::to_string(&browser).unwrap();
        assert_eq!(json, format!("\"{}\"", browser.as_str()));
        // Round-trip through untrusted-deserialization path.
        let parsed: CookiesBrowser = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, browser);
    }

    // Unknown values are rejected by the closed enum (no argument injection).
    assert!(serde_json::from_str::<CookiesBrowser>("\"firefox:profile\"").is_err());
    assert!(serde_json::from_str::<CookiesBrowser>("\"--exec=rm -rf\"").is_err());
    assert!(serde_json::from_str::<CookiesBrowser>("\"unknown-browser\"").is_err());
    assert!(serde_json::from_str::<EngineChannel>("\"beta\"").is_err());

    // Legacy settings.json without the new keys stays loadable.
    let legacy = format!(
        r#"{{
        "downloadDirectory": "{}",
        "themeMode": "system",
        "defaultPreset": {{ "format": "mp4", "videoQuality": "p1080" }}
    }}"#,
        abs_path_json("downloads")
    );
    let legacy_dto: AppSettingsDto = serde_json::from_str(&legacy).unwrap();
    assert_eq!(legacy_dto.cookies_from_browser, None);
    assert_eq!(legacy_dto.engine_channel, EngineChannel::Stable);
    assert_eq!(
        legacy_dto.schema_version, 0,
        "absent schema version means pre-versioning"
    );
    assert!(AppSettings::try_from(legacy_dto).is_ok());
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

    job.transition_to_completed(abs_path("output/video.mp4"))
        .unwrap();
    assert_eq!(job.status(), DownloadStatus::Completed);
    assert_eq!(job.progress_percent(), None);
    assert_eq!(
        job.destination_path(),
        Some(abs_path("output/video.mp4").as_str())
    );
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
            engine_outdated: false,
            cookies_diagnostic: None,
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
        DownloadPreset::mp3(Mp3Quality::K320),
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

    // Give the background task time to complete and record history.
    wait_for_history(&service).await;
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

    let service = StartDownloadService::new_with_retry_policy(
        Arc::new(FailingDownloader),
        Arc::new(FakeConverter),
        Arc::new(FakeInspector),
        repo,
        history_repo,
        None,
        test_dir.join("temp"),
        RetryPolicy::immediate(),
    );

    let job = service
        .start_download("https://www.youtube.com/watch?v=dQw4w9WgXcQ", None, None)
        .await
        .unwrap();

    // Wait for the background task to reach its terminal state.
    wait_for_terminal(&service, job.id()).await;

    let downloads = service.list_downloads().await;
    let found = downloads.iter().find(|j| j.id() == job.id()).unwrap();
    assert_eq!(found.status(), DownloadStatus::Failed);
    assert!(found
        .error_message()
        .unwrap()
        .contains("yt-dlp stream error"));

    let _ = tokio::fs::remove_dir_all(&test_dir).await;
}

// 15b. Retry: failed jobs re-queue with same URL/preset/directory, others refused
#[tokio::test]
async fn test_retry_download_only_allows_failed_jobs() {
    let test_dir = std::env::temp_dir().join(format!("polysaver_retry_{}", uuid::Uuid::new_v4()));
    let custom_dir = test_dir.join("custom_output");
    tokio::fs::create_dir_all(&custom_dir).await.unwrap();
    let settings = AppSettings::defaults_for(test_dir.join("downloads")).unwrap();
    let repo = Arc::new(InMemorySettingsRepo {
        settings: RwLock::new(settings),
    });
    let history_repo = Arc::new(InMemoryTestHistoryRepo::default());

    let service = StartDownloadService::new_with_retry_policy(
        Arc::new(FailingDownloader),
        Arc::new(FakeConverter),
        Arc::new(FakeInspector),
        repo,
        history_repo,
        None,
        test_dir.join("temp"),
        RetryPolicy::immediate(),
    );

    // A job that is not failed (still queued/active) cannot be retried.
    let queued_job = service
        .start_download(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            Some(DownloadPreset::video(OutputFormat::Mp4, VideoQuality::P720).unwrap()),
            Some(custom_dir.clone()),
        )
        .await
        .unwrap();
    let early_retry = service.retry_download(queued_job.id()).await;
    assert!(matches!(early_retry, Err(CoreError::InvalidState(_))));

    // Unknown identifiers surface a clear not-found error.
    let unknown_retry = service
        .retry_download(polysaver_core::domain::DownloadId::new())
        .await;
    assert!(matches!(unknown_retry, Err(CoreError::JobNotFound(_))));

    // Wait for the download to fail, then retry it faithfully.
    wait_for_terminal(&service, queued_job.id()).await;
    let failed = service
        .list_downloads()
        .await
        .into_iter()
        .find(|j| j.id() == queued_job.id())
        .unwrap();
    assert_eq!(failed.status(), DownloadStatus::Failed);
    assert_eq!(failed.target_dir(), Some(custom_dir.as_path()));

    let retried = service.retry_download(failed.id()).await.unwrap();
    assert_ne!(retried.id(), failed.id());
    assert_eq!(retried.status(), DownloadStatus::Queued);
    assert_eq!(retried.url(), failed.url());
    assert_eq!(retried.preset(), failed.preset());
    assert_eq!(retried.target_dir(), Some(custom_dir.as_path()));

    // The failed entry is gone; only the new queued job remains in memory.
    let jobs = service.list_downloads().await;
    assert!(!jobs.iter().any(|j| j.id() == failed.id()));
    assert!(jobs.iter().any(|j| j.id() == retried.id()));

    let _ = tokio::fs::remove_dir_all(&test_dir).await;
}

// 15c. Retry never writes history entries, and a failed job never reaches history
#[tokio::test]
async fn test_failed_jobs_and_retries_do_not_write_history() {
    let test_dir =
        std::env::temp_dir().join(format!("polysaver_retry_hist_{}", uuid::Uuid::new_v4()));
    let settings = AppSettings::defaults_for(test_dir.join("downloads")).unwrap();
    let repo = Arc::new(InMemorySettingsRepo {
        settings: RwLock::new(settings),
    });
    let history_repo = Arc::new(InMemoryTestHistoryRepo::default());

    let service = StartDownloadService::new_with_retry_policy(
        Arc::new(FailingDownloader),
        Arc::new(FakeConverter),
        Arc::new(FakeInspector),
        repo,
        history_repo.clone(),
        None,
        test_dir.join("temp"),
        RetryPolicy::immediate(),
    );

    let job = service
        .start_download("https://www.youtube.com/watch?v=dQw4w9WgXcQ", None, None)
        .await
        .unwrap();
    wait_for_terminal(&service, job.id()).await;
    let failed = service
        .list_downloads()
        .await
        .into_iter()
        .find(|j| j.id() == job.id())
        .unwrap();
    assert_eq!(failed.status(), DownloadStatus::Failed);

    let retried = service.retry_download(failed.id()).await.unwrap();
    wait_for_terminal(&service, retried.id()).await;

    // Failing then retrying (still failing) must not produce any history entry.
    let history = service.list_history().await.unwrap();
    assert!(
        history.is_empty(),
        "failed jobs must never be recorded in history"
    );
    assert!(service
        .list_downloads()
        .await
        .iter()
        .any(|j| j.id() == retried.id() && j.status() == DownloadStatus::Failed));

    let _ = tokio::fs::remove_dir_all(&test_dir).await;
}

// 16. AppSettingsDto TryFrom validation and default System theme mode and French language
#[test]
fn test_app_settings_dto_try_from() {
    let dto = AppSettingsDto {
        download_directory: abs_path("path/to/downloads"),
        theme_mode: ThemeMode::System,
        default_preset: DownloadPresetDto {
            format: OutputFormat::Flac,
            video_quality: None,
            mp3_quality: None,
        },
        language: Language::English,
        cookies_from_browser: None,
        engine_channel: polysaver_core::domain::EngineChannel::Stable,
        schema_version: 1,
    };

    let settings = AppSettings::try_from(dto).unwrap();
    assert_eq!(settings.download_directory(), abs_path("path/to/downloads"));
    assert_eq!(settings.theme_mode(), ThemeMode::System);
    assert_eq!(settings.default_preset(), DownloadPreset::Flac);
    assert_eq!(settings.language(), Language::English);

    // Default settings must use ThemeMode::System and Language::French
    let default_settings = AppSettings::defaults_for(abs_path("downloads")).unwrap();
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

    // Deserialization without language defaults to French.
    // Legacy `parallelDownloads` / `maxConcurrent` keys from pre-2.5 settings files
    // must be ignored gracefully for backward compatibility (no migration needed).
    let json_without_language = format!(
        r#"{{
        "downloadDirectory": "{}",
        "themeMode": "system",
        "parallelDownloads": true,
        "defaultPreset": {{
            "format": "mp3",
            "mp3Quality": "k320"
        }},
        "maxConcurrent": 3
    }}"#,
        abs_path_json("downloads")
    );
    let dto: AppSettingsDto = serde_json::from_str(&json_without_language).unwrap();
    assert_eq!(dto.language, Language::French);
    assert_eq!(dto.download_directory, abs_path_json("downloads"));
    let legacy_settings = AppSettings::try_from(dto).unwrap();
    assert_eq!(
        legacy_settings.default_preset(),
        DownloadPreset::mp3(Mp3Quality::K320)
    );
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
            tbr: None,
            note: Some("storyboard".to_string()),
        },
        FormatOption {
            format_id: "140".to_string(),
            height: None,
            has_video: false,
            has_audio: true,
            extension: "m4a".to_string(),
            filesize_approx_bytes: None,
            tbr: None,
            note: Some("audio".to_string()),
        },
        FormatOption {
            format_id: "278".to_string(),
            height: Some(144),
            has_video: true,
            has_audio: false,
            extension: "webm".to_string(),
            filesize_approx_bytes: None,
            tbr: None,
            note: None,
        },
        FormatOption {
            format_id: "247".to_string(),
            height: Some(720),
            has_video: true,
            has_audio: false,
            extension: "webm".to_string(),
            filesize_approx_bytes: None,
            tbr: None,
            note: None,
        },
        FormatOption {
            format_id: "136".to_string(),
            height: Some(720),
            has_video: true,
            has_audio: false,
            extension: "mp4".to_string(),
            filesize_approx_bytes: None,
            tbr: None,
            note: None,
        },
        FormatOption {
            format_id: "248".to_string(),
            height: Some(1080),
            has_video: true,
            has_audio: false,
            extension: "webm".to_string(),
            filesize_approx_bytes: None,
            tbr: None,
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

    // Playlist URL accepted and re-serialized identically
    let playlist_json = "\"https://youtube.com/playlist?list=PL123\"";
    let playlist_url = serde_json::from_str::<MediaUrl>(playlist_json).expect("playlist is valid");
    assert!(playlist_url.is_playlist());
    assert_eq!(serde_json::to_string(&playlist_url).unwrap(), playlist_json);

    // Local/reserved hosts are rejected through deserialization too
    // (history entries pointing there are treated as invalid lines).
    assert!(serde_json::from_str::<MediaUrl>("\"http://127.0.0.1/x\"").is_err());
    assert!(serde_json::from_str::<MediaUrl>("\"http://localhost/x\"").is_err());
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
            AppSettings::defaults_for(abs_path("downloads"))
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
        abs_path("downloads/zoo.mp4"),
        Some(123456789),
    )
    .unwrap();

    assert_eq!(entry.title(), "Me at the zoo");
    assert_eq!(entry.destination_path(), abs_path("downloads/zoo.mp4"));
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

// 19b. History path confinement: entries outside the download directory are refused
#[tokio::test]
async fn test_history_file_path_confinement() {
    use polysaver_core::domain::{AppSettings, DownloadPreset, OutputFormat, VideoQuality};
    use polysaver_core::ports::settings_repository::SettingsRepository;
    use std::sync::Arc;

    struct DummySettingsRepo;
    #[async_trait::async_trait]
    impl SettingsRepository for DummySettingsRepo {
        async fn load(&self) -> Result<AppSettings, CoreError> {
            AppSettings::defaults_for(abs_path("confinement_downloads"))
        }
        async fn save(&self, _: &AppSettings) -> Result<(), CoreError> {
            Ok(())
        }
    }

    let test_dir =
        std::env::temp_dir().join(format!("polysaver_confinement_{}", uuid::Uuid::new_v4()));
    let downloads_dir = test_dir.join("downloads");
    tokio::fs::create_dir_all(&downloads_dir).await.unwrap();

    let inside_file = downloads_dir.join("inside.mp4");
    tokio::fs::write(&inside_file, b"media").await.unwrap();

    let outside_dir = test_dir.join("elsewhere");
    tokio::fs::create_dir_all(&outside_dir).await.unwrap();
    let outside_file = outside_dir.join("outside.mp4");
    tokio::fs::write(&outside_file, b"secret").await.unwrap();

    let history_repo = Arc::new(InMemoryTestHistoryRepo::default());
    let service = StartDownloadService::new(
        Arc::new(FakeDownloader),
        Arc::new(FakeConverter),
        Arc::new(FakeInspector),
        Arc::new(DummySettingsRepo),
        history_repo.clone(),
        None,
        test_dir.join("temp"),
    );

    let url = MediaUrl::parse("https://www.youtube.com/watch?v=jNQXAC9IVRw").unwrap();
    let preset = DownloadPreset::video(OutputFormat::Mp4, VideoQuality::P720).unwrap();

    // Entry recorded with the downloads directory: the file inside is accepted.
    let inside_entry = DownloadHistoryEntry::new_with_dir(
        DownloadId::new(),
        url.clone(),
        "Inside".to_string(),
        preset,
        inside_file.to_string_lossy().to_string(),
        Some(1000),
        downloads_dir.to_string_lossy().to_string(),
    )
    .unwrap();
    history_repo.append(inside_entry.clone()).await.unwrap();
    let resolved = service
        .get_history_file_path(inside_entry.id())
        .await
        .unwrap();
    assert!(resolved.ends_with("inside.mp4"));

    // Trafiquée entry: path outside the recorded directory must be refused.
    let outside_entry = DownloadHistoryEntry::new_with_dir(
        DownloadId::new(),
        url,
        "Outside".to_string(),
        preset,
        outside_file.to_string_lossy().to_string(),
        Some(2000),
        downloads_dir.to_string_lossy().to_string(),
    )
    .unwrap();
    history_repo.append(outside_entry.clone()).await.unwrap();
    let err = service
        .get_history_file_path(outside_entry.id())
        .await
        .unwrap_err();
    assert_eq!(err.machine_code(), "OUTPUT_FILE_NOT_FOUND");

    let _ = tokio::fs::remove_dir_all(&test_dir).await;
}

// 19c. History entry path validation: absolute, no traversal
#[test]
fn test_history_entry_rejects_relative_and_traversal_paths() {
    let url = MediaUrl::parse("https://www.youtube.com/watch?v=jNQXAC9IVRw").unwrap();
    let preset = DownloadPreset::video(OutputFormat::Mp4, VideoQuality::P720).unwrap();

    assert!(DownloadHistoryEntry::new(
        DownloadId::new(),
        url.clone(),
        "T".to_string(),
        preset,
        "relative/video.mp4".to_string(),
        None,
    )
    .is_err());

    assert!(DownloadHistoryEntry::new(
        DownloadId::new(),
        url.clone(),
        "T".to_string(),
        preset,
        abs_path("downloads/../../etc/passwd"),
        None,
    )
    .is_err());

    assert!(DownloadHistoryEntry::new(
        DownloadId::new(),
        url,
        "T".to_string(),
        preset,
        abs_path("downloads/with\0null.mp4"),
        None,
    )
    .is_err());
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
            total_bytes_estimate: None,
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
            engine_outdated: false,
            cookies_diagnostic: None,
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
    let saw_cancellation = wait_until(
        || async {
            sink.canceled_count
                .load(std::sync::atomic::Ordering::SeqCst)
                == 1
        },
        std::time::Duration::from_secs(10),
    )
    .await;
    assert!(saw_cancellation, "the canceled event was never emitted");
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

    // Wait for the background worker to complete.
    wait_for_terminal(&service, job_id).await;

    // Cancellation of completed job is rejected
    let cancel_res = service.cancel_download(job_id).await;
    assert!(cancel_res.is_err());
    assert!(matches!(
        cancel_res.unwrap_err(),
        CoreError::InvalidState(_)
    ));

    let _ = tokio::fs::remove_dir_all(&test_dir).await;
}

// 20. Automatic retry: transient failures are retried, permanent ones are not
struct CountingWarningSink {
    warnings: std::sync::Mutex<Vec<String>>,
}

impl EventSink for CountingWarningSink {
    fn emit_queued(&self, _job: &DownloadJob) {}
    fn emit_progress(&self, _progress: &polysaver_core::ports::DownloadProgressEvent) {}
    fn emit_completed(&self, _job: &DownloadJob) {}
    fn emit_failed(&self, _job: &DownloadJob, _error: &DownloadErrorDetails) {}
    fn emit_canceled(&self, _job: &DownloadJob) {}
    fn emit_warning(&self, warning: &polysaver_core::ports::event_sink::DownloadWarningEvent) {
        self.warnings
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(warning.code.clone());
    }
}

/// Fails a fixed number of times before succeeding, so retry behavior is observable.
struct FlakyDownloader {
    failures_remaining: std::sync::atomic::AtomicUsize,
}

#[async_trait]
impl MediaDownloader for FlakyDownloader {
    async fn download_stream(
        &self,
        request: DownloadStreamRequest,
        _cb: Arc<dyn Fn(StreamProgress) + Send + Sync>,
    ) -> Result<DownloadedStreams, CoreError> {
        let remaining = self
            .failures_remaining
            .load(std::sync::atomic::Ordering::SeqCst);
        if remaining > 0 {
            self.failures_remaining
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            let mut details = DownloadErrorDetails::from_code(
                polysaver_core::error::DownloadErrorCode::NetworkUnavailable,
            );
            details.message = "transient network failure".to_string();
            return Err(CoreError::DownloadFailed(details));
        }

        let file = request.temp_dir.join("video.mp4");
        tokio::fs::write(&file, b"flaky success").await.unwrap();
        Ok(DownloadedStreams {
            raw_artifacts: vec![file.clone()],
            video_path: Some(file),
            audio_path: None,
            title: "Flaky Video".to_string(),
            duration_seconds: Some(10),
            engine_outdated: false,
            cookies_diagnostic: None,
        })
    }
}

/// Non-retryable error source: must fail on the first attempt.
struct PermanentFailureDownloader;

#[async_trait]
impl MediaDownloader for PermanentFailureDownloader {
    async fn download_stream(
        &self,
        _request: DownloadStreamRequest,
        _cb: Arc<dyn Fn(StreamProgress) + Send + Sync>,
    ) -> Result<DownloadedStreams, CoreError> {
        let mut details = DownloadErrorDetails::from_code(
            polysaver_core::error::DownloadErrorCode::VideoUnavailable,
        );
        details.message = "video unavailable".to_string();
        Err(CoreError::DownloadFailed(details))
    }
}

#[tokio::test]
async fn test_transient_failures_are_retried_until_success() {
    let test_dir =
        std::env::temp_dir().join(format!("polysaver_retry_ok_{}", uuid::Uuid::new_v4()));
    let settings = AppSettings::defaults_for(test_dir.join("downloads")).unwrap();
    let repo = Arc::new(InMemorySettingsRepo {
        settings: RwLock::new(settings),
    });
    let history_repo = Arc::new(InMemoryTestHistoryRepo::default());
    let sink = Arc::new(CountingWarningSink {
        warnings: std::sync::Mutex::new(Vec::new()),
    });

    // Fail twice, then succeed on the third attempt.
    let service = StartDownloadService::new_with_retry_policy(
        Arc::new(FlakyDownloader {
            failures_remaining: std::sync::atomic::AtomicUsize::new(2),
        }),
        Arc::new(FakeConverter),
        Arc::new(FakeInspector),
        repo,
        history_repo,
        Some(sink.clone()),
        test_dir.join("temp"),
        RetryPolicy::immediate(),
    );

    let job = service
        .start_download("https://www.youtube.com/watch?v=dQw4w9WgXcQ", None, None)
        .await
        .unwrap();

    wait_for_terminal(&service, job.id()).await;

    let finished = service
        .list_downloads()
        .await
        .into_iter()
        .find(|j| j.id() == job.id())
        .unwrap();
    assert_eq!(finished.status(), DownloadStatus::Completed);
    assert_eq!(
        finished.retry_count(),
        2,
        "two automatic retries were performed"
    );

    let warnings = sink
        .warnings
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    assert_eq!(warnings.iter().filter(|c| *c == "RETRY_ATTEMPT").count(), 2);

    // Only the successful attempt writes history.
    let history = service.list_history().await.unwrap();
    assert_eq!(history.len(), 1);

    // No job_<id> temporary workspace survives the retry cycle.
    let temp_root = test_dir.join("temp");
    if temp_root.exists() {
        let mut entries = tokio::fs::read_dir(&temp_root).await.unwrap();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(
                !name.starts_with("job_"),
                "temporary directory '{name}' leaked after retries"
            );
        }
    }

    let _ = tokio::fs::remove_dir_all(&test_dir).await;
}

#[tokio::test]
async fn test_non_retryable_failure_is_immediate() {
    let test_dir = std::env::temp_dir().join(format!("polysaver_noretry_{}", uuid::Uuid::new_v4()));
    let settings = AppSettings::defaults_for(test_dir.join("downloads")).unwrap();
    let repo = Arc::new(InMemorySettingsRepo {
        settings: RwLock::new(settings),
    });
    let history_repo = Arc::new(InMemoryTestHistoryRepo::default());
    let sink = Arc::new(CountingWarningSink {
        warnings: std::sync::Mutex::new(Vec::new()),
    });

    let service = StartDownloadService::new_with_retry_policy(
        Arc::new(PermanentFailureDownloader),
        Arc::new(FakeConverter),
        Arc::new(FakeInspector),
        repo,
        history_repo,
        Some(sink.clone()),
        test_dir.join("temp"),
        RetryPolicy::immediate(),
    );

    let job = service
        .start_download("https://www.youtube.com/watch?v=dQw4w9WgXcQ", None, None)
        .await
        .unwrap();

    wait_for_terminal(&service, job.id()).await;

    let finished = service
        .list_downloads()
        .await
        .into_iter()
        .find(|j| j.id() == job.id())
        .unwrap();
    assert_eq!(finished.status(), DownloadStatus::Failed);
    assert_eq!(
        finished.retry_count(),
        0,
        "no retry for a permanent failure"
    );
    assert!(sink
        .warnings
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .all(|c| c != "RETRY_ATTEMPT"));

    let _ = tokio::fs::remove_dir_all(&test_dir).await;
}

#[tokio::test]
async fn test_exhausted_retries_end_in_failed_with_original_error() {
    let test_dir = std::env::temp_dir().join(format!("polysaver_exhaust_{}", uuid::Uuid::new_v4()));
    let settings = AppSettings::defaults_for(test_dir.join("downloads")).unwrap();
    let repo = Arc::new(InMemorySettingsRepo {
        settings: RwLock::new(settings),
    });
    let history_repo = Arc::new(InMemoryTestHistoryRepo::default());

    let service = StartDownloadService::new_with_retry_policy(
        Arc::new(FailingDownloader),
        Arc::new(FakeConverter),
        Arc::new(FakeInspector),
        repo,
        history_repo,
        None,
        test_dir.join("temp"),
        RetryPolicy::immediate(),
    );

    let job = service
        .start_download("https://www.youtube.com/watch?v=dQw4w9WgXcQ", None, None)
        .await
        .unwrap();

    wait_for_terminal(&service, job.id()).await;

    let finished = service
        .list_downloads()
        .await
        .into_iter()
        .find(|j| j.id() == job.id())
        .unwrap();
    assert_eq!(finished.status(), DownloadStatus::Failed);
    assert_eq!(finished.retry_count(), 2, "1 initial attempt + 2 retries");
    assert!(finished
        .error_message()
        .unwrap()
        .contains("yt-dlp stream error"));
    assert!(service.list_history().await.unwrap().is_empty());

    let _ = tokio::fs::remove_dir_all(&test_dir).await;
}

// 21. YtdlpUpdateRequired triggers a single engine update followed by one retry
struct StaleEngineDownloader {
    stale_remaining: std::sync::atomic::AtomicUsize,
}

#[async_trait]
impl MediaDownloader for StaleEngineDownloader {
    async fn download_stream(
        &self,
        request: DownloadStreamRequest,
        _cb: Arc<dyn Fn(StreamProgress) + Send + Sync>,
    ) -> Result<DownloadedStreams, CoreError> {
        let remaining = self
            .stale_remaining
            .load(std::sync::atomic::Ordering::SeqCst);
        if remaining > 0 {
            self.stale_remaining
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            let mut details = DownloadErrorDetails::from_code(
                polysaver_core::error::DownloadErrorCode::YtdlpUpdateRequired,
            );
            details.message = "engine too old".to_string();
            return Err(CoreError::DownloadFailed(details));
        }

        let file = request.temp_dir.join("video.mp4");
        tokio::fs::write(&file, b"updated engine success")
            .await
            .unwrap();
        Ok(DownloadedStreams {
            raw_artifacts: vec![file.clone()],
            video_path: Some(file),
            audio_path: None,
            title: "After Update".to_string(),
            duration_seconds: Some(5),
            engine_outdated: false,
            cookies_diagnostic: None,
        })
    }
}

struct CountingEngineUpdater {
    calls: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl polysaver_core::services::EngineUpdater for CountingEngineUpdater {
    async fn update_engine(&self) -> Result<String, CoreError> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok("2026.09.01".to_string())
    }
}

#[tokio::test]
async fn test_stale_engine_updates_once_then_retries() {
    let test_dir = std::env::temp_dir().join(format!("polysaver_stale_{}", uuid::Uuid::new_v4()));
    let settings = AppSettings::defaults_for(test_dir.join("downloads")).unwrap();
    let repo = Arc::new(InMemorySettingsRepo {
        settings: RwLock::new(settings),
    });
    let history_repo = Arc::new(InMemoryTestHistoryRepo::default());
    let sink = Arc::new(CountingWarningSink {
        warnings: std::sync::Mutex::new(Vec::new()),
    });
    let updater = Arc::new(CountingEngineUpdater {
        calls: std::sync::atomic::AtomicUsize::new(0),
    });

    let service = StartDownloadService::new_with_retry_policy(
        Arc::new(StaleEngineDownloader {
            stale_remaining: std::sync::atomic::AtomicUsize::new(1),
        }),
        Arc::new(FakeConverter),
        Arc::new(FakeInspector),
        repo,
        history_repo,
        Some(sink.clone()),
        test_dir.join("temp"),
        RetryPolicy::immediate(),
    )
    .with_engine_updater(updater.clone());

    let job = service
        .start_download("https://www.youtube.com/watch?v=dQw4w9WgXcQ", None, None)
        .await
        .unwrap();

    wait_for_terminal(&service, job.id()).await;

    let finished = service
        .list_downloads()
        .await
        .into_iter()
        .find(|j| j.id() == job.id())
        .unwrap();
    assert_eq!(finished.status(), DownloadStatus::Completed);
    assert_eq!(
        updater.calls.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the engine must be updated exactly once"
    );
    assert_eq!(finished.retry_count(), 1);

    let warnings = sink
        .warnings
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    assert!(warnings.iter().any(|c| c == "ENGINE_UPDATE_RETRY"));

    let _ = tokio::fs::remove_dir_all(&test_dir).await;
}

#[tokio::test]
async fn test_failed_engine_update_keeps_original_error() {
    let test_dir =
        std::env::temp_dir().join(format!("polysaver_stale_fail_{}", uuid::Uuid::new_v4()));
    let settings = AppSettings::defaults_for(test_dir.join("downloads")).unwrap();
    let repo = Arc::new(InMemorySettingsRepo {
        settings: RwLock::new(settings),
    });
    let history_repo = Arc::new(InMemoryTestHistoryRepo::default());

    struct OfflineUpdater;
    #[async_trait::async_trait]
    impl polysaver_core::services::EngineUpdater for OfflineUpdater {
        async fn update_engine(&self) -> Result<String, CoreError> {
            Err(CoreError::ProviderError("offline".to_string()))
        }
    }

    let service = StartDownloadService::new_with_retry_policy(
        Arc::new(StaleEngineDownloader {
            stale_remaining: std::sync::atomic::AtomicUsize::new(5),
        }),
        Arc::new(FakeConverter),
        Arc::new(FakeInspector),
        repo,
        history_repo,
        None,
        test_dir.join("temp"),
        RetryPolicy::immediate(),
    )
    .with_engine_updater(Arc::new(OfflineUpdater));

    let job = service
        .start_download("https://www.youtube.com/watch?v=dQw4w9WgXcQ", None, None)
        .await
        .unwrap();

    wait_for_terminal(&service, job.id()).await;

    let finished = service
        .list_downloads()
        .await
        .into_iter()
        .find(|j| j.id() == job.id())
        .unwrap();
    assert_eq!(finished.status(), DownloadStatus::Failed);
    // The original engine error is reported, not the update failure.
    assert_eq!(
        finished.error_details().map(|d| d.code),
        Some(polysaver_core::error::DownloadErrorCode::YtdlpUpdateRequired)
    );

    let _ = tokio::fs::remove_dir_all(&test_dir).await;
}

// 22. Orphaned temp workspaces from a crash are purged, recent ones are kept
#[tokio::test]
async fn test_purge_orphan_temp_dirs_only_removes_old_entries() {
    use polysaver_core::services::start_download::purge_orphan_temp_dirs_older_than;

    let temp_root = std::env::temp_dir().join(format!("polysaver_purge_{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_root).await.unwrap();

    // An old leftover from a killed run (contains a partial download).
    let old_job = temp_root.join("job_11111111-1111-1111-1111-111111111111");
    tokio::fs::create_dir_all(&old_job).await.unwrap();
    tokio::fs::write(old_job.join("stream.part"), b"partial data")
        .await
        .unwrap();

    // A recent workspace, possibly belonging to a concurrent instance.
    let recent_job = temp_root.join("job_22222222-2222-2222-2222-222222222222");
    tokio::fs::create_dir_all(&recent_job).await.unwrap();

    // Unrelated directory names must never be touched.
    let unrelated = temp_root.join("not_a_job_dir");
    tokio::fs::create_dir_all(&unrelated).await.unwrap();

    // A stray file named like a job directory is not a directory and stays.
    let stray_file = temp_root.join("job_33333333-3333-3333-3333-333333333333");
    tokio::fs::write(&stray_file, b"file, not dir")
        .await
        .unwrap();

    // Zero threshold: every `job_*` directory qualifies as old, isolating the
    // name/type logic from the age comparison.
    let removed = purge_orphan_temp_dirs_older_than(&temp_root, std::time::Duration::ZERO).await;
    assert_eq!(removed, 2, "only the two job_* directories are candidates");
    assert!(!old_job.exists());
    assert!(!recent_job.exists());
    assert!(
        stray_file.exists(),
        "a file is never removed as a directory"
    );
    assert!(unrelated.exists(), "non job_* entries are left alone");

    // With the real 24 h threshold, fresh directories survive.
    let fresh = temp_root.join("job_44444444-4444-4444-4444-444444444444");
    tokio::fs::create_dir_all(&fresh).await.unwrap();
    let removed =
        polysaver_core::services::start_download::purge_orphan_temp_dirs(&temp_root).await;
    assert_eq!(removed, 0, "a just-created workspace must survive startup");
    assert!(fresh.exists());

    // A missing temp root is not an error.
    let missing = temp_root.join("does_not_exist");
    let removed = purge_orphan_temp_dirs_older_than(&missing, std::time::Duration::ZERO).await;
    assert_eq!(removed, 0);

    let _ = tokio::fs::remove_dir_all(&temp_root).await;
}

// 34. Playlist download batches
/// Downloader that never finishes, so batch tests observe queue state without racing
/// against the fake pipeline's fast completion.
struct HangingDownloader;

#[async_trait]
impl MediaDownloader for HangingDownloader {
    async fn download_stream(
        &self,
        _request: DownloadStreamRequest,
        _cb: Arc<dyn Fn(StreamProgress) + Send + Sync>,
    ) -> Result<DownloadedStreams, CoreError> {
        tokio::time::sleep(std::time::Duration::from_secs(600)).await;
        Err(CoreError::OperationCancelled)
    }
}

async fn batch_test_service(
    downloader: Arc<dyn MediaDownloader>,
) -> (StartDownloadService, Arc<InMemoryTestHistoryRepo>, PathBuf) {
    let dir = std::env::temp_dir().join(format!("polysaver_batch_{}", uuid::Uuid::new_v4()));
    let download_dir = dir.join("downloads");
    tokio::fs::create_dir_all(&download_dir).await.unwrap();
    let settings = AppSettings::new(
        download_dir.to_string_lossy().to_string(),
        ThemeMode::Dark,
        DownloadPreset::video(OutputFormat::Mp4, VideoQuality::P1080).unwrap(),
        Language::French,
    )
    .unwrap();
    let history = Arc::new(InMemoryTestHistoryRepo::default());
    let service = StartDownloadService::new(
        downloader,
        Arc::new(FakeConverter),
        Arc::new(FakeInspector),
        Arc::new(InMemorySettingsRepo {
            settings: RwLock::new(settings),
        }),
        history.clone(),
        None,
        dir.join("temp"),
    );
    (service, history, dir)
}

fn playlist_entry_urls(count: usize) -> Vec<MediaUrl> {
    (0..count)
        .map(|i| {
            MediaUrl::parse(&format!("https://www.youtube.com/watch?v=entry{i}"))
                .expect("generated URL must be valid")
        })
        .collect()
}

#[tokio::test]
async fn test_start_download_batch_rejects_empty_and_oversized_selections() {
    let (service, _history, _dir) = batch_test_service(Arc::new(HangingDownloader)).await;

    let empty = service
        .start_download_batch(Vec::new(), None, None)
        .await
        .expect_err("an empty selection must be refused");
    assert!(matches!(empty, CoreError::InvalidSettings(_)));

    let too_many = playlist_entry_urls(MAX_PLAYLIST_DOWNLOADS + 1);
    let refused = service
        .start_download_batch(too_many, None, None)
        .await
        .expect_err("more than the cap must be refused");
    assert!(matches!(refused, CoreError::InvalidSettings(_)));

    // Nothing was queued by the two rejected calls.
    assert!(service.list_downloads().await.is_empty());
}

#[tokio::test]
async fn test_start_download_batch_queues_one_independent_job_per_url() {
    let (service, history, _dir) = batch_test_service(Arc::new(HangingDownloader)).await;
    let urls = playlist_entry_urls(3);
    let expected: Vec<String> = urls.iter().map(|u| u.as_str().to_string()).collect();

    let jobs = service
        .start_download_batch(urls, None, None)
        .await
        .expect("batch must be accepted");
    assert_eq!(jobs.len(), 3);

    // Distinct identifiers: each video is its own job with its own progress,
    // cancellation, retry and history entry.
    let mut ids: Vec<String> = jobs.iter().map(|j| j.id().to_string()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 3, "each entry must get its own job id");

    let listed = service.list_downloads().await;
    assert_eq!(listed.len(), 3);
    let mut listed_urls: Vec<String> = listed
        .iter()
        .map(|j| j.url().as_str().to_string())
        .collect();
    listed_urls.sort();
    let mut sorted_expected = expected.clone();
    sorted_expected.sort();
    assert_eq!(listed_urls, sorted_expected);

    // The default preset from settings is applied to every job of the batch.
    for job in &listed {
        assert_eq!(
            job.preset(),
            DownloadPreset::video(OutputFormat::Mp4, VideoQuality::P1080).unwrap()
        );
    }

    // No video finished yet: history stays untouched.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(history.load().await.unwrap().is_empty());
}

#[tokio::test]
async fn test_single_download_refuses_playlist_urls() {
    let (service, _history, _dir) = batch_test_service(Arc::new(FakeDownloader)).await;

    let refused = service
        .start_download("https://www.youtube.com/playlist?list=PL123", None, None)
        .await
        .expect_err("playlist URLs must not go through the single-video pipeline");
    assert!(matches!(refused, CoreError::InvalidState(_)));

    let channel = service
        .start_download("https://www.youtube.com/@handle/videos", None, None)
        .await;
    assert!(matches!(channel, Err(CoreError::InvalidState(_))));

    assert!(service.list_downloads().await.is_empty());

    // A share URL (v= plus list=) stays a normal single video.
    let job = service
        .start_download(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PL123",
            None,
            None,
        )
        .await
        .expect("share URLs must still download as a single video");
    assert!(!job.url().as_str().contains("list="));
}
