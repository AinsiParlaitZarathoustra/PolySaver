// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::error::CoreError;
use serde::{Deserialize, Serialize};
use std::fmt;
use url::Url;

/// Canonical validated media URL value object.
/// Guarantees that the URL is a valid absolute HTTP or HTTPS URL, and rejects playlists/channels.
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
    /// - Host must be present.
    /// - Must not be a playlist or channel URL.
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

        // Check for playlist indicators in path or query
        let path = parsed.path().to_lowercase();
        if path.starts_with("/playlist") || path.starts_with("/channel") || path.starts_with("/c/")
        {
            return Err(CoreError::PlaylistNotSupported(
                "Playlists and channel URLs are not supported. Please provide an individual video URL."
                    .to_string(),
            ));
        }

        if let Some(query) = parsed.query() {
            let lower_query = query.to_lowercase();
            // Check for list= parameter that indicates a playlist
            for param in lower_query.split('&') {
                if param.starts_with("list=") {
                    return Err(CoreError::PlaylistNotSupported(
                        "Playlist URLs are not supported in this version. Please provide an individual video URL."
                            .to_string(),
                    ));
                }
            }
        }

        Ok(Self(parsed))
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
