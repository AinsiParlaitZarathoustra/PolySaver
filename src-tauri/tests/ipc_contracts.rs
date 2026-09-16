// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use polysaver::dto::media::{
    DownloadJobDto, EngineUpdateResultDto, EngineUpdateStatusDto, JsRuntimeAvailability,
    JsRuntimeStatusDto, StartDownloadRequestDto,
};
use polysaver_core::domain::{
    DownloadJob, DownloadPreset, MediaUrl, Mp3Quality, OutputFormat, VideoQuality,
};

/// Builds an absolute path valid on every platform (Windows rejects Unix literals).
fn abs_path(suffix: &str) -> String {
    let base = if cfg!(windows) {
        r"C:\polysaver_test"
    } else {
        "/polysaver_test"
    };
    format!("{base}/{suffix}")
}

#[test]
fn test_download_job_dto_serialization_camel_case() {
    let url = MediaUrl::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
    let preset = DownloadPreset::video(OutputFormat::Mp4, VideoQuality::P1080).unwrap();
    let mut job = DownloadJob::new(url, preset);
    job.set_title("Rick Astley - Never Gonna Give You Up".to_string());

    let dto = DownloadJobDto::from(&job);
    let serialized = serde_json::to_string(&dto).unwrap();

    assert!(serialized.contains("\"id\""));
    assert!(serialized.contains("\"progressPercent\""));
    assert!(serialized.contains("\"downloadedBytes\""));
    assert!(serialized.contains("\"speedBytesPerSecond\""));
    assert!(serialized.contains("\"destinationPath\""));
    assert!(serialized.contains("\"errorMessage\""));
    assert!(serialized.contains("\"videoQuality\":\"p1080\""));
}

#[test]
fn test_start_download_request_deserialization() {
    let raw_json = r#"{
        "url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "preset": {
            "format": "mp3",
            "mp3Quality": "k320"
        },
        "outputDirectory": "/custom/path/downloads"
    }"#;

    let parsed: StartDownloadRequestDto = serde_json::from_str(raw_json).unwrap();
    assert_eq!(parsed.url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
    assert_eq!(
        parsed.output_directory.as_deref(),
        Some("/custom/path/downloads")
    );
    assert!(parsed.preset.is_some());

    let preset_dto = parsed.preset.unwrap();
    let preset = DownloadPreset::try_from(preset_dto).unwrap();
    assert_eq!(preset.format(), OutputFormat::Mp3);
    assert_eq!(preset.mp3_quality(), Some(Mp3Quality::K320));
}

#[test]
fn test_download_history_entry_dto_serialization_camel_case() {
    use polysaver::dto::history::DownloadHistoryEntryDto;
    use polysaver_core::domain::download_job::DownloadId;
    use polysaver_core::domain::history::DownloadHistoryEntry;

    let job_id = DownloadId::new();
    let url = MediaUrl::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
    let preset = DownloadPreset::video(OutputFormat::Mp4, VideoQuality::P1080).unwrap();
    let entry = DownloadHistoryEntry::new(
        job_id,
        url,
        "Never Gonna Give You Up".to_string(),
        preset,
        abs_path("downloads/video.mp4"),
        Some(1770000000000),
    )
    .unwrap();

    let dto = DownloadHistoryEntryDto::from(&entry);
    let serialized = serde_json::to_string(&dto).unwrap();

    assert!(serialized.contains("\"id\""));
    assert!(serialized.contains("\"downloadId\""));
    assert!(serialized.contains("\"sourceUrl\""));
    assert!(serialized.contains("\"destinationPath\""));
    assert!(serialized.contains("\"completedAt\":1770000000000"));
}

#[test]
fn test_engine_update_dto_serialization_camel_case() {
    let status = EngineUpdateStatusDto {
        current_version: Some("2026.08.19".to_string()),
        latest_version: Some("2026.09.01".to_string()),
        channel: "stable".to_string(),
        outdated: true,
        can_update: true,
        can_rollback: false,
    };
    let serialized = serde_json::to_string(&status).unwrap();
    assert!(serialized.contains("\"currentVersion\":\"2026.08.19\""));
    assert!(serialized.contains("\"latestVersion\":\"2026.09.01\""));
    assert!(serialized.contains("\"channel\":\"stable\""));
    assert!(serialized.contains("\"outdated\":true"));
    assert!(serialized.contains("\"canUpdate\":true"));
    assert!(serialized.contains("\"canRollback\":false"));

    let result = EngineUpdateResultDto {
        installed_version: "2026.09.01".to_string(),
        updated: true,
        latest_version: None,
    };
    let serialized = serde_json::to_string(&result).unwrap();
    assert!(serialized.contains("\"installedVersion\":\"2026.09.01\""));
    assert!(serialized.contains("\"updated\":true"));
    assert!(serialized.contains("\"latestVersion\":null"));
}

#[test]
fn test_js_runtime_dto_serialization_camel_case() {
    let availability = JsRuntimeAvailability {
        is_ready: true,
        kind: Some("deno".to_string()),
        version: Some("2.9.6".to_string()),
        binary_path: Some("/usr/local/bin/deno".to_string()),
        status_message: "deno 2.9.6 detected".to_string(),
    };
    let serialized = serde_json::to_string(&availability).unwrap();
    assert!(serialized.contains("\"isReady\":true"));
    assert!(serialized.contains("\"kind\":\"deno\""));
    assert!(serialized.contains("\"version\":\"2.9.6\""));
    assert!(serialized.contains("\"binaryPath\":\"/usr/local/bin/deno\""));

    let status = JsRuntimeStatusDto {
        kind: Some("node".to_string()),
        version: Some("24.21.0".to_string()),
        path: Some("/app/bin/node/node".to_string()),
        is_ready: true,
        version_too_old: false,
    };
    let serialized = serde_json::to_string(&status).unwrap();
    assert!(serialized.contains("\"isReady\":true"));
    assert!(serialized.contains("\"versionTooOld\":false"));

    // A missing runtime serializes deterministically.
    let missing = JsRuntimeStatusDto {
        kind: None,
        version: None,
        path: None,
        is_ready: false,
        version_too_old: false,
    };
    let serialized = serde_json::to_string(&missing).unwrap();
    assert!(serialized.contains("\"kind\":null"));
    assert!(serialized.contains("\"isReady\":false"));
}

#[test]
fn test_probe_result_dto_carries_playlist_shape() {
    use polysaver::dto::media::{PlaylistEntryDto, ProbeResultDto};
    use polysaver_core::domain::{MediaKind, PlaylistEntry, ProbeResult};

    let url = MediaUrl::parse("https://www.youtube.com/playlist?list=PL123").unwrap();
    let entries = vec![
        PlaylistEntry {
            index: 1,
            url: MediaUrl::parse("https://www.youtube.com/watch?v=aaa").unwrap(),
            title: "Première".to_string(),
            duration_seconds: Some(61),
            thumbnail_url: Some("https://i.ytimg.com/vi/aaa/hqdefault.jpg".to_string()),
            available: true,
        },
        PlaylistEntry {
            index: 2,
            url: MediaUrl::parse("https://www.youtube.com/watch?v=bbb").unwrap(),
            title: "[Private video]".to_string(),
            duration_seconds: None,
            thumbnail_url: None,
            available: false,
        },
    ];
    let probe = ProbeResult::new_playlist(
        url,
        "Ma playlist".to_string(),
        Some("https://i.ytimg.com/vi/aaa/hqdefault.jpg".to_string()),
        Some("Une chaîne".to_string()),
        entries,
        Some(12000),
    );

    let dto = ProbeResultDto::from(&probe);
    let serialized = serde_json::to_string(&dto).unwrap();

    assert!(serialized.contains("\"kind\":\"playlist\""));
    assert!(serialized.contains("\"playlistTotal\":12000"));
    assert!(serialized.contains("\"entriesLimit\":200"));
    assert!(serialized.contains("\"title\":\"Première\""));
    assert!(serialized.contains("\"durationSeconds\":61"));
    assert!(serialized.contains("\"available\":false"));
    // The entry DTO keeps the camelCase contract of the frontend type.
    let entry = PlaylistEntryDto::from(&probe.entries[1]);
    let serialized_entry = serde_json::to_string(&entry).unwrap();
    assert!(serialized_entry.contains("\"thumbnailUrl\":null"));

    // A single result keeps the retro-compatible shape: empty entries, kind single.
    let single = ProbeResult::new(
        MediaUrl::parse("https://www.youtube.com/watch?v=abc").unwrap(),
        "Vidéo".to_string(),
        Some(19),
        None,
        None,
        Vec::new(),
    );
    assert_eq!(single.kind, MediaKind::Single);
    let single_dto = ProbeResultDto::from(&single);
    let serialized_single = serde_json::to_string(&single_dto).unwrap();
    assert!(serialized_single.contains("\"kind\":\"single\""));
    assert!(serialized_single.contains("\"entries\":[]"));
}

#[test]
fn test_start_playlist_download_request_deserialization() {
    use polysaver::dto::media::StartPlaylistDownloadRequestDto;

    let raw_json = r#"{
        "url": "https://www.youtube.com/playlist?list=PL123",
        "preset": { "format": "mp4", "videoQuality": "p720" },
        "outputDirectory": null,
        "selectedUrls": [
            "https://www.youtube.com/watch?v=aaa",
            "https://www.youtube.com/watch?v=bbb"
        ]
    }"#;

    let parsed: StartPlaylistDownloadRequestDto = serde_json::from_str(raw_json).unwrap();
    assert_eq!(parsed.url, "https://www.youtube.com/playlist?list=PL123");
    assert_eq!(parsed.selected_urls.len(), 2);
    assert!(parsed.output_directory.is_none());

    // Every entry re-validates through the same gate as a single download.
    for raw in &parsed.selected_urls {
        assert!(MediaUrl::parse(raw).is_ok());
    }
    assert!(MediaUrl::parse("file:///etc/passwd").is_err());
}
