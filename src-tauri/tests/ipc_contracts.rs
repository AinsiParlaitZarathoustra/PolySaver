// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use polysaver::dto::media::{DownloadJobDto, StartDownloadRequestDto};
use polysaver_core::domain::{
    DownloadJob, DownloadPreset, MediaUrl, Mp3Quality, OutputFormat, VideoQuality,
};

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
        "/downloads/video.mp4".to_string(),
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
