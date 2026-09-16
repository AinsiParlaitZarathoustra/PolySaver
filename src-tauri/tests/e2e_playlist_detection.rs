// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! Real-network checks for the native playlist detection.
//!
//! These tests drive the actual bundled yt-dlp, so they are `#[ignore]`d and only
//! run when `POLYSAVER_RUN_NETWORK_E2E=1` is exported (same rule as the download
//! matrix). They download no media: detection asks for the listing envelope only.

use polysaver_core::domain::MediaUrl;
use polysaver_core::ports::PlaylistDetector;
use polysaver_ytdlp::YtDlpDownloader;
use std::path::PathBuf;

/// Builds a downloader that uses the bundled sidecars from the crate directory.
fn bundled_downloader() -> YtDlpDownloader {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let resource_bin_dir = manifest_dir.join("resources/bin");
    assert!(
        resource_bin_dir.join("yt-dlp").exists(),
        "bundled yt-dlp missing at {} — run scripts/prepare-sidecars.sh",
        resource_bin_dir.display()
    );
    YtDlpDownloader::with_resource_dir(manifest_dir.join("target/e2e-bin"), Some(resource_bin_dir))
}

fn network_e2e_enabled() -> bool {
    std::env::var("POLYSAVER_RUN_NETWORK_E2E").as_deref() == Ok("1")
}

#[tokio::test]
#[ignore = "Requires live network access. Run with POLYSAVER_RUN_NETWORK_E2E=1"]
async fn test_detection_classifies_real_urls() {
    if !network_e2e_enabled() {
        println!("Skipping: set POLYSAVER_RUN_NETWORK_E2E=1 to execute.");
        return;
    }

    let downloader = bundled_downloader();

    // (url, expected is_playlist, label)
    let cases: &[(&str, bool, &str)] = &[
        (
            "https://www.youtube.com/watch?v=jNQXAC9IVRw",
            false,
            "single video",
        ),
        (
            // The share button adds `list=`; yt-dlp still sees a single video.
            "https://www.youtube.com/watch?v=jNQXAC9IVRw&list=PL2SOU6wwxB0uwwH80KTQ6ht66KWxbzTIo",
            false,
            "share URL (v= plus list=)",
        ),
        (
            "https://www.youtube.com/playlist?list=PL2SOU6wwxB0uwwH80KTQ6ht66KWxbzTIo",
            true,
            "playlist",
        ),
        ("https://www.youtube.com/@MrBeast/videos", true, "channel"),
    ];

    for (raw, expected, label) in cases {
        let url = MediaUrl::parse(raw).expect("test URL must be valid");
        let detection = downloader
            .detect_playlist(&url, None)
            .await
            .unwrap_or_else(|err| panic!("[{label}] detection failed: {err}"));
        assert_eq!(
            detection.is_playlist, *expected,
            "[{label}] wrong classification for {raw}"
        );
        println!("[{label}] is_playlist={}", detection.is_playlist);
    }
}

#[tokio::test]
#[ignore = "Requires live network access. Run with POLYSAVER_RUN_NETWORK_E2E=1"]
async fn test_detection_reports_missing_playlist_as_error() {
    if !network_e2e_enabled() {
        println!("Skipping: set POLYSAVER_RUN_NETWORK_E2E=1 to execute.");
        return;
    }

    let downloader = bundled_downloader();
    // A private/missing playlist makes yt-dlp exit non-zero with `null` on stdout.
    // That must surface as an error, never as "this is a video".
    let url = MediaUrl::parse(
        "https://www.youtube.com/playlist?list=PL00000000000000000000000000000000000",
    )
    .expect("test URL must be valid");

    let result = downloader.detect_playlist(&url, None).await;
    assert!(
        result.is_err(),
        "a missing playlist must be an error, got {result:?}"
    );
    println!("[missing playlist] error = {result:?}");
}
