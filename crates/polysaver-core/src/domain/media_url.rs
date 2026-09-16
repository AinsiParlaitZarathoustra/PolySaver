// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::error::CoreError;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use url::{Host, Url};

/// Nature of a media URL, determined offline from its path and query only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaUrlKind {
    /// A single video/audio item.
    Single,
    /// A playlist, channel or other multi-video listing.
    Playlist,
}

/// Canonical validated media URL value object.
/// Guarantees that the URL is a valid absolute HTTP or HTTPS URL and refuses local,
/// private, or reserved network targets.
///
/// Note: deserialization goes through [`MediaUrl::parse`] (`serde(try_from = "String")`), so
/// history entries pointing at local/reserved hosts are now rejected as invalid lines.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MediaUrl(Url);

impl MediaUrl {
    /// Parses and strictly validates an untrusted URL string.
    ///
    /// Invariants:
    /// - Must not be empty or whitespace-only.
    /// - Must be a valid absolute URI.
    /// - Scheme must be `http` or `https`.
    /// - Host must be present, and must not be local/private/reserved.
    /// - Must not embed credentials (`user:pass@`).
    pub fn parse(raw: &str) -> Result<Self, CoreError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(CoreError::InvalidUrl("URL cannot be empty".to_string()));
        }

        let parsed = Url::parse(trimmed)
            .map_err(|err| CoreError::InvalidUrl(format!("Malformed URL: {err}")))?;

        let scheme = parsed.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(CoreError::InvalidUrl(format!(
                "Unsupported scheme '{scheme}'. Only http and https are allowed"
            )));
        }

        if parsed.host_str().is_none() {
            return Err(CoreError::InvalidUrl("URL host is missing".to_string()));
        }

        // Credentials in the URL are never legitimate for media sources and
        // enable host-confusion tricks.
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(CoreError::InvalidUrl(
                "URLs containing credentials are not allowed".to_string(),
            ));
        }

        validate_host_not_local(&parsed)?;

        Ok(Self(normalize_share_playlist_param(parsed)))
    }

    /// Offline classification of this URL as a single item or a playlist/listing.
    ///
    /// This is a *shape* heuristic used as a fast pre-classification (and as the
    /// fallback when the engine cannot be reached). The engine answer obtained
    /// through the `PlaylistDetector` port is the authoritative one, because only
    /// yt-dlp knows what an URL really resolves to.
    ///
    /// `list=` is a YouTube convention: it is appended by the "Share" button and
    /// rarely means "download the whole playlist", so [`Self::parse`] strips it
    /// whenever an explicit video id (`v=`) is present, and it is only treated as
    /// a playlist marker on YouTube hosts. On other sites `list` is an ordinary
    /// query parameter (pagination, sorting, tracking) and must not switch the UI
    /// into playlist mode.
    #[must_use]
    pub fn kind(&self) -> MediaUrlKind {
        if is_playlist_path(self.0.path()) || is_soundcloud_set(&self.0) {
            return MediaUrlKind::Playlist;
        }
        if is_youtube_host(&self.0) {
            let mut has_list = false;
            let mut has_video_id = false;
            for (key, _) in self.0.query_pairs() {
                let key = key.to_ascii_lowercase();
                if key == "list" {
                    has_list = true;
                } else if key == "v" {
                    has_video_id = true;
                }
            }
            if has_list && !has_video_id {
                return MediaUrlKind::Playlist;
            }
        }
        MediaUrlKind::Single
    }

    /// Convenience predicate for [`Self::kind`].
    #[must_use]
    pub fn is_playlist(&self) -> bool {
        self.kind() == MediaUrlKind::Playlist
    }

    /// Access the underlying URL string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Access the underlying URL object.
    #[must_use]
    pub fn to_url(&self) -> Url {
        self.0.clone()
    }
}

impl fmt::Display for MediaUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<MediaUrl> for String {
    fn from(url: MediaUrl) -> Self {
        url.0.to_string()
    }
}

impl TryFrom<&str> for MediaUrl {
    type Error = CoreError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<String> for MediaUrl {
    type Error = CoreError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

/// Returns true for URL paths that identify a multi-video listing rather than a
/// single item: YouTube playlists and channel pages (`/channel`, `/c/`, `/user/`,
/// `/@handle`, including their `/videos`, `/streams` and `/shorts` sub-pages).
///
/// `list=` alone is handled separately by [`MediaUrl::kind`]: it is a YouTube
/// convention and only means "playlist" there.
#[must_use]
pub fn is_playlist_path(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path.starts_with("/playlist")
        || path.starts_with("/channel")
        || path.starts_with("/c/")
        || path.starts_with("/user/")
        || path.starts_with("/@")
}

/// Returns true when the URL points at a SoundCloud set (their playlist form).
#[must_use]
pub fn is_soundcloud_set(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let is_soundcloud = host == "soundcloud.com" || host.ends_with(".soundcloud.com");
    is_soundcloud && url.path().to_ascii_lowercase().contains("/sets/")
}

/// Returns true when the URL points at a YouTube host (including `m.`/`music.`
/// subdomains and the `youtu.be` shortener).
#[must_use]
pub fn is_youtube_host(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "youtu.be"
        || host == "youtube-nocookie.com"
        || host.ends_with(".youtube-nocookie.com")
}

/// Strips the `list=` query parameter when an explicit `v=` video id is present.
///
/// `list=` is added by YouTube's "Share" button and almost never means "I want the
/// whole playlist"; treating `watch?v=X&list=Y` as a single video is the least
/// surprising behaviour. This runs in the single entry point (`parse`), so analysis,
/// downloads and history all see the same normalized URL, and the `--no-playlist`
/// flag already passed to yt-dlp then covers the `v=`+`list=` combination.
fn normalize_share_playlist_param(mut url: Url) -> Url {
    let mut has_list = false;
    let mut has_video_id = false;
    for (key, _) in url.query_pairs() {
        let key = key.to_ascii_lowercase();
        if key == "list" {
            has_list = true;
        } else if key == "v" {
            has_video_id = true;
        }
    }
    if !has_list || !has_video_id {
        return url;
    }

    let retained: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| !key.eq_ignore_ascii_case("list"))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();

    if retained.is_empty() {
        url.set_query(None);
    } else {
        let mut serializer = url.query_pairs_mut();
        serializer.clear();
        for (key, value) in retained {
            serializer.append_pair(&key, &value);
        }
    }
    url
}

/// Rejects local/private/reserved targets, operating on the *parsed* host
/// (never the raw string, which can hide hosts in credentials).
fn validate_host_not_local(url: &Url) -> Result<(), CoreError> {
    let host = url
        .host()
        .ok_or_else(|| CoreError::InvalidUrl("URL host is missing".to_string()))?;

    match host {
        Host::Domain(name) => {
            let lower = name.trim_end_matches('.').to_ascii_lowercase();
            if lower == "localhost"
                || lower.ends_with(".localhost")
                || lower.ends_with(".local")
                || lower.ends_with(".internal")
            {
                return Err(CoreError::InvalidUrl(format!(
                    "Local network host '{name}' is not allowed"
                )));
            }
            Ok(())
        }
        Host::Ipv4(ip) => {
            if is_public_ipv4(ip) {
                Ok(())
            } else {
                Err(CoreError::InvalidUrl(format!(
                    "Local or reserved IP address '{ip}' is not allowed"
                )))
            }
        }
        Host::Ipv6(ip) => {
            // Unwrap IPv4-mapped addresses (::ffff:127.0.0.1) and re-validate.
            if let Some(mapped) = ip.to_ipv4_mapped() {
                if is_public_ipv4(mapped) {
                    return Ok(());
                }
                return Err(CoreError::InvalidUrl(format!(
                    "Local or reserved IP address '{ip}' is not allowed"
                )));
            }
            if is_public_ipv6(ip) {
                Ok(())
            } else {
                Err(CoreError::InvalidUrl(format!(
                    "Local or reserved IP address '{ip}' is not allowed"
                )))
            }
        }
    }
}

/// Rejects loopback, private, link-local, CGNAT, benchmarking, documentation,
/// reserved, and multicast IPv4 addresses.
fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    if ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
    {
        return false;
    }
    let octets = ip.octets();
    // 0.0.0.0/8 "this network"
    if octets[0] == 0 {
        return false;
    }
    // 100.64.0.0/10 CGNAT
    if octets[0] == 100 && (64..=127).contains(&octets[1]) {
        return false;
    }
    // 198.18.0.0/15 benchmarking
    if octets[0] == 198 && (18..=19).contains(&octets[1]) {
        return false;
    }
    // 240.0.0.0/4 reserved (broadcast already rejected above)
    if octets[0] >= 240 {
        return false;
    }
    true
}

/// Rejects loopback, unspecified, ULA, link-local, documentation, and multicast IPv6 addresses.
fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_unique_local()
        || ip.is_unicast_link_local()
        || ip.is_multicast()
    {
        return false;
    }
    let segments = ip.segments();
    // 2001:db8::/32 documentation
    if segments[0] == 0x2001 && segments[1] == 0x0db8 {
        return false;
    }
    true
}

/// Standalone host check used by callers that already parsed a URL.
#[must_use]
pub fn is_host_allowed_for_media(host: &str) -> bool {
    match Url::parse(&format!("https://{host}")) {
        Ok(url) => validate_host_not_local(&url).is_ok(),
        Err(_) => false,
    }
}

/// Validates an IP address literal against the local/reserved policy.
#[must_use]
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_ipv4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(mapped) => is_public_ipv4(mapped),
            None => is_public_ipv6(v6),
        },
    }
}
