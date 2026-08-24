// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use polysaver_core::domain::{
    AppSettings, DownloadPreset, DownloadStatus, Mp3Quality, OutputFormat, ThemeMode, VideoQuality,
};
use polysaver_core::error::DownloadErrorCode;
use polysaver_core::ports::settings_repository::SettingsRepository;
use polysaver_core::services::StartDownloadService;
use polysaver_ffmpeg::FfmpegConverter;
use polysaver_storage::{JsonDownloadHistoryRepository, JsonSettingsRepository};
use polysaver_ytdlp::YtDlpDownloader;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

/// Verifies that all bundled sidecars strictly exist and match expected versions.
fn assert_bundled_sidecars_valid(resource_bin_dir: &Path) {
    let ytdlp_path = resource_bin_dir.join("yt-dlp");
    let ffmpeg_path = resource_bin_dir.join("ffmpeg");
    let ffprobe_path = resource_bin_dir.join("ffprobe");

    assert!(
        ytdlp_path.exists(),
        "Embedded yt-dlp binary missing at: {}",
        ytdlp_path.display()
    );
    assert!(
        ffmpeg_path.exists(),
        "Embedded ffmpeg binary missing at: {}",
        ffmpeg_path.display()
    );
    assert!(
        ffprobe_path.exists(),
        "Embedded ffprobe binary missing at: {}",
        ffprobe_path.display()
    );

    // 1. Verify yt-dlp version
    let ytdlp_out = Command::new(&ytdlp_path)
        .arg("--version")
        .output()
        .expect("Failed to execute bundled yt-dlp");
    assert!(ytdlp_out.status.success());
    let ytdlp_ver = String::from_utf8_lossy(&ytdlp_out.stdout)
        .trim()
        .to_string();
    assert_eq!(
        ytdlp_ver, "2026.08.19",
        "Expected yt-dlp 2026.08.19, got: {ytdlp_ver}"
    );

    // 2. Verify ffmpeg version & configuration (including libdav1d)
    let ffmpeg_out = Command::new(&ffmpeg_path)
        .arg("-version")
        .output()
        .expect("Failed to execute bundled ffmpeg");
    assert!(ffmpeg_out.status.success());
    let ffmpeg_ver = String::from_utf8_lossy(&ffmpeg_out.stdout);
    assert!(
        ffmpeg_ver.contains("9.0.1"),
        "Expected ffmpeg 9.0.1, got: {}",
        ffmpeg_ver.lines().next().unwrap_or("")
    );
    assert!(
        ffmpeg_ver.contains("--enable-libdav1d"),
        "FFmpeg binary missing essential --enable-libdav1d configuration flag!"
    );

    // 3. Verify ffprobe version
    let ffprobe_out = Command::new(&ffprobe_path)
        .arg("-version")
        .output()
        .expect("Failed to execute bundled ffprobe");
    assert!(ffprobe_out.status.success());
    let ffprobe_ver = String::from_utf8_lossy(&ffprobe_out.stdout);
    assert!(
        ffprobe_ver.contains("9.0.1"),
        "Expected ffprobe 9.0.1, got: {}",
        ffprobe_ver.lines().next().unwrap_or("")
    );
}

/// Helper to execute a download and await its completion.
async fn run_download_test(
    service: &StartDownloadService,
    ffprobe_bin: &Path,
    url: &str,
    preset: DownloadPreset,
    test_label: &str,
) -> PathBuf {
    println!("\n=== Testing [{test_label}] URL: {url} ===");
    let job = service
        .start_download(url, Some(preset), None)
        .await
        .expect("Failed to submit download");

    let mut completed_path: Option<PathBuf> = None;
    for i in 0..240 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let list = service.list_downloads().await;
        if let Some(j) = list.iter().find(|j| j.id() == job.id()) {
            match j.status() {
                DownloadStatus::Completed => {
                    let path_str = j.destination_path().expect("Missing destination path");
                    completed_path = Some(PathBuf::from(path_str));
                    break;
                }
                DownloadStatus::Failed => {
                    panic!(
                        "Download failed for [{test_label}]: {:?}",
                        j.error_message()
                    );
                }
                _ => {
                    if i % 20 == 0 {
                        println!(
                            "[{test_label}] Progress: status={:?} percent={:?} speed={:?}",
                            j.status(),
                            j.progress_percent(),
                            j.speed_bytes_per_second()
                        );
                    }
                }
            }
        }
    }

    let final_path = completed_path.expect("Download timed out after 120s");
    assert!(
        final_path.exists(),
        "[{test_label}] Generated file does not exist: {}",
        final_path.display()
    );

    let meta = std::fs::metadata(&final_path).expect("Failed to read metadata");
    assert!(
        meta.len() > 0,
        "[{test_label}] Generated file is empty: {}",
        final_path.display()
    );

    println!(
        "[{test_label}] SUCCESS: Generated {} ({} bytes)",
        final_path.display(),
        meta.len()
    );

    // Validate with bundled ffprobe
    validate_media_with_bundled_ffprobe(ffprobe_bin, &final_path, preset, test_label);

    final_path
}

/// Uses ONLY the bundled ffprobe to inspect streams and codecs.
fn validate_media_with_bundled_ffprobe(
    ffprobe_bin: &Path,
    file_path: &Path,
    preset: DownloadPreset,
    test_label: &str,
) {
    let output = Command::new(ffprobe_bin)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,codec_name,bit_rate",
            "-of",
            "json",
        ])
        .arg(file_path)
        .output()
        .unwrap_or_else(|e| panic!("[{test_label}] Failed to run bundled ffprobe: {e}"));

    assert!(
        output.status.success(),
        "[{test_label}] ffprobe inspection failed"
    );

    let parsed: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("[{test_label}] Failed to parse ffprobe json: {e}"));

    let streams = parsed["streams"]
        .as_array()
        .unwrap_or_else(|| panic!("[{test_label}] No streams array in ffprobe json"));

    println!(
        "[{test_label}] Streams detected: {}",
        serde_json::to_string(streams).unwrap()
    );

    match preset {
        DownloadPreset::Video { .. } => {
            let has_video = streams
                .iter()
                .any(|s| s["codec_type"].as_str() == Some("video"));
            let has_audio = streams
                .iter()
                .any(|s| s["codec_type"].as_str() == Some("audio"));
            assert!(has_video, "[{test_label}] Missing video stream in output");
            assert!(has_audio, "[{test_label}] Missing audio stream in output");
        }
        DownloadPreset::Mp3 { .. } => {
            let has_mp3 = streams
                .iter()
                .any(|s| s["codec_name"].as_str() == Some("mp3"));
            assert!(has_mp3, "[{test_label}] Codec is not MP3");
        }
        DownloadPreset::Flac => {
            let has_flac = streams
                .iter()
                .any(|s| s["codec_name"].as_str() == Some("flac"));
            assert!(has_flac, "[{test_label}] Codec is not FLAC");
        }
    }
}

#[tokio::test]
#[ignore = "Requires live external network access and bundled sidecars. Run with POLYSAVER_RUN_NETWORK_E2E=1 cargo test --test e2e_real_downloads -- --ignored"]
async fn test_real_matrix_youtube_and_tiktok_mp4_mov_mp3_flac() {
    if std::env::var("POLYSAVER_RUN_NETWORK_E2E").as_deref() != Ok("1") {
        println!("Skipping real network E2E test. Set POLYSAVER_RUN_NETWORK_E2E=1 to execute.");
        return;
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let resource_bin_dir = manifest_dir.join("resources/bin");

    // Strictly enforce bundled sidecars exist and are compliant
    assert_bundled_sidecars_valid(&resource_bin_dir);

    let ffprobe_bin = resource_bin_dir.join("ffprobe");

    let test_dir = std::env::temp_dir().join(format!("polysaver_e2e_sp6_{}", uuid::Uuid::new_v4()));
    let bin_dir = test_dir.join("bin");
    let temp_dir = test_dir.join("temp");
    let config_dir = test_dir.join("config");
    let downloads_dir = test_dir.join("downloads");

    std::fs::create_dir_all(&bin_dir).unwrap();
    std::fs::create_dir_all(&temp_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&downloads_dir).unwrap();

    let initial_settings = AppSettings::new(
        downloads_dir.to_string_lossy().to_string(),
        ThemeMode::Dark,
        true,
        DownloadPreset::video(OutputFormat::Mp4, VideoQuality::Best).unwrap(),
        3,
        polysaver_core::domain::Language::French,
    )
    .unwrap();
    let settings_repo = Arc::new(JsonSettingsRepository::new(
        &config_dir,
        initial_settings.clone(),
    ));
    settings_repo.save(&initial_settings).await.unwrap();

    let downloader = Arc::new(YtDlpDownloader::with_resource_dir(
        bin_dir.clone(),
        Some(resource_bin_dir.clone()),
    ));
    let converter = Arc::new(FfmpegConverter::with_resource_dir(
        bin_dir.clone(),
        Some(resource_bin_dir.clone()),
    ));
    let history_repo = Arc::new(JsonDownloadHistoryRepository::new(&config_dir));

    let service = StartDownloadService::new(
        downloader,
        converter.clone(),
        converter,
        settings_repo,
        history_repo,
        None,
        temp_dir,
    );

    let failing_yt_url = "https://youtu.be/2vEBGzVxM7o";
    let short_yt_url = "https://www.youtube.com/watch?v=jNQXAC9IVRw&pp=ygUKYXQgdGhlIHpvbw%3D%3D";
    let tt_url = "https://www.tiktok.com/@brookehavenfootball/video/7581276453208149270?is_from_webapp=1&sender_device=pc";

    // 1. YouTube previously failing video: MP4 (Best Available)
    run_download_test(
        &service,
        &ffprobe_bin,
        failing_yt_url,
        DownloadPreset::video(OutputFormat::Mp4, VideoQuality::Best).unwrap(),
        "YouTube MP4 Best (2vEBGzVxM7o)",
    )
    .await;

    // 2. YouTube previously failing video: MP4 (720p - exact height present in probe)
    run_download_test(
        &service,
        &ffprobe_bin,
        failing_yt_url,
        DownloadPreset::video(OutputFormat::Mp4, VideoQuality::P720).unwrap(),
        "YouTube MP4 720p (2vEBGzVxM7o)",
    )
    .await;

    // 3. YouTube short video: MP4 (Best)
    run_download_test(
        &service,
        &ffprobe_bin,
        short_yt_url,
        DownloadPreset::video(OutputFormat::Mp4, VideoQuality::Best).unwrap(),
        "YouTube MP4 Best (Zoo)",
    )
    .await;

    // 4. YouTube short video: MOV (240p - real available height)
    run_download_test(
        &service,
        &ffprobe_bin,
        short_yt_url,
        DownloadPreset::video(OutputFormat::Mov, VideoQuality::P240).unwrap(),
        "YouTube MOV 240p (Zoo)",
    )
    .await;

    // 5. YouTube short video: MP3 (320k)
    run_download_test(
        &service,
        &ffprobe_bin,
        short_yt_url,
        DownloadPreset::mp3(Mp3Quality::K320),
        "YouTube MP3 320k (Zoo)",
    )
    .await;

    // 6. YouTube short video: FLAC
    run_download_test(
        &service,
        &ffprobe_bin,
        short_yt_url,
        DownloadPreset::flac(),
        "YouTube FLAC (Zoo)",
    )
    .await;

    // 7. TikTok: MP4 (Best)
    run_download_test(
        &service,
        &ffprobe_bin,
        tt_url,
        DownloadPreset::video(OutputFormat::Mp4, VideoQuality::Best).unwrap(),
        "TikTok MP4 Best",
    )
    .await;

    // 8. TikTok: MP3 (320k)
    run_download_test(
        &service,
        &ffprobe_bin,
        tt_url,
        DownloadPreset::mp3(Mp3Quality::K320),
        "TikTok MP3 320k",
    )
    .await;

    // 9. Validation of exact height rejection: requesting 4K on a 1080p video must fail with FORMAT_NOT_AVAILABLE
    let unavail_job = service
        .start_download(
            failing_yt_url,
            Some(DownloadPreset::video(OutputFormat::Mp4, VideoQuality::P2160).unwrap()),
            None,
        )
        .await
        .expect("Failed to submit download");

    for _ in 0..120 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let list = service.list_downloads().await;
        if let Some(j) = list.iter().find(|j| j.id() == unavail_job.id()) {
            if j.status() == DownloadStatus::Failed {
                let err = j.error_details().expect("Missing error details");
                assert_eq!(err.code, DownloadErrorCode::FormatNotAvailable);
                println!("SUCCESS: Exact height 2160p properly rejected with FORMAT_NOT_AVAILABLE");
                break;
            }
        }
    }

    // 10. Verify history persistence
    let history = service
        .list_history()
        .await
        .expect("Failed to load history");
    assert!(
        history.len() >= 8,
        "Expected at least 8 completed history entries, found: {}",
        history.len()
    );

    let _ = tokio::fs::remove_dir_all(&test_dir).await;
}
