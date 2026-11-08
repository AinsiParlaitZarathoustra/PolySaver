// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! Real-network checks for the native playlist detection.
//!
//! These tests drive the actual bundled yt-dlp, so they are `#[ignore]`d and only
//! run when `POLYSAVER_RUN_NETWORK_E2E=1` is exported (same rule as the download
//! matrix). They download no media: detection asks for the listing envelope only.

use polysaver_core::domain::{MediaKind, MediaUrl, PLAYLIST_ENTRIES_LIMIT};
use polysaver_core::ports::{MediaProvider, PlaylistDetector};
use polysaver_ytdlp::YtDlpDownloader;
use std::path::PathBuf;

/// Builds a downloader that uses the bundled sidecars from the crate directory.
fn bundled_downloader() -> YtDlpDownloader {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let resource_bin_dir = manifest_dir.join("resources/bin");
    assert!(
        resource_bin_dir
            .join(format!("yt-dlp{}", std::env::consts::EXE_SUFFIX))
            .exists(),
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
        (
            "https://www.youtube.com/@MrBeast/shorts",
            true,
            "channel shorts",
        ),
        (
            // A Mix ("radio") opened from a video: still that one video.
            "https://www.youtube.com/watch?v=jNQXAC9IVRw&list=RDjNQXAC9IVRw&start_radio=1",
            false,
            "YouTube Mix",
        ),
        (
            // Shaped like a handle (`/@`), but a single post: the regression
            // that made every TikTok video link fail before 2.7.
            "https://www.tiktok.com/@brookehavenfootball/video/7581276453208149270",
            false,
            "TikTok video",
        ),
        (
            "https://www.tiktok.com/@brookehavenfootball",
            true,
            "TikTok profile",
        ),
        (
            "https://soundcloud.com/forss/sets/soulhack",
            true,
            "SoundCloud set",
        ),
        (
            "https://blazo.bandcamp.com/album/jazz-format-mixtape-vol-1",
            true,
            "Bandcamp album",
        ),
        (
            "https://www.dailymotion.com/video/x5kesuj",
            false,
            "Dailymotion video",
        ),
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

/// Listing real playlists from several sites: every entry must point to the
/// site the playlist comes from (never a rebuilt YouTube URL for another site),
/// and the listing must stay within the 100-entry bound.
#[tokio::test]
#[ignore = "Requires live network access. Run with POLYSAVER_RUN_NETWORK_E2E=1"]
async fn test_listing_entries_point_to_the_source_site() {
    if !network_e2e_enabled() {
        println!("Skipping: set POLYSAVER_RUN_NETWORK_E2E=1 to execute.");
        return;
    }

    let downloader = bundled_downloader();

    // (url, expected host suffix of every entry, label)
    let cases: &[(&str, &str, &str)] = &[
        (
            "https://www.youtube.com/playlist?list=PL2SOU6wwxB0uwwH80KTQ6ht66KWxbzTIo",
            "youtube.com",
            "YouTube playlist",
        ),
        (
            // A channel holds far more than 100 videos: the listing is bounded.
            "https://www.youtube.com/@MrBeast/videos",
            "youtube.com",
            "YouTube channel",
        ),
        (
            "https://soundcloud.com/forss/sets/soulhack",
            "soundcloud.com",
            "SoundCloud set",
        ),
        (
            "https://blazo.bandcamp.com/album/jazz-format-mixtape-vol-1",
            "bandcamp.com",
            "Bandcamp album",
        ),
    ];

    for (raw, host_suffix, label) in cases {
        let url = MediaUrl::parse(raw).expect("test URL must be valid");
        let result = downloader
            .probe(&url, None)
            .await
            .unwrap_or_else(|err| panic!("[{label}] listing failed: {err}"));

        assert_eq!(
            result.kind,
            MediaKind::Playlist,
            "[{label}] not listed as a playlist"
        );
        assert!(!result.entries.is_empty(), "[{label}] no entry listed");
        assert!(
            result.entries.len() <= PLAYLIST_ENTRIES_LIMIT,
            "[{label}] {} entries listed, above the {PLAYLIST_ENTRIES_LIMIT} bound",
            result.entries.len()
        );
        for (expected_index, entry) in result.entries.iter().enumerate() {
            let host = entry
                .url
                .to_url()
                .host_str()
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            assert!(
                host == *host_suffix || host.ends_with(&format!(".{host_suffix}")),
                "[{label}] entry {} points to another site: {}",
                entry.index,
                entry.url.as_str()
            );
            assert_eq!(
                entry.index as usize,
                expected_index + 1,
                "[{label}] positions"
            );
        }
        println!(
            "[{label}] {} entries, all on {host_suffix}",
            result.entries.len()
        );
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
