// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::error::CoreError;
use async_trait::async_trait;
use std::path::Path;

/// Stream information returned by a media inspector sidecar (e.g. ffprobe).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaStreamInfo {
    pub has_video: bool,
    pub has_audio: bool,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
}

/// Abstract port for inspecting media files before downstream processing.
#[async_trait]
pub trait MediaInspector: Send + Sync {
    /// Inspects the media streams in the given file.
    async fn inspect(&self, path: &Path) -> Result<MediaStreamInfo, CoreError>;
}
