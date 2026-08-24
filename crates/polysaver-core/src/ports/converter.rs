// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::domain::format::{Mp3Quality, OutputFormat};
use crate::error::CoreError;
use std::path::PathBuf;
use std::sync::Arc;

/// Progress information emitted during conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConverterProgress {
    /// Conversion completion percentage (0 to 100), or None if indeterminate.
    pub percent: Option<u8>,
}

/// Callback for receiving conversion progress.
pub type ConverterProgressCallback = Arc<dyn Fn(ConverterProgress) + Send + Sync>;

/// Target audio codec configuration for transcoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    Mp3(Mp3Quality),
    Flac,
}

/// Typed media conversion / remuxing request.
#[derive(Debug, Clone)]
pub enum ConvertRequest {
    /// Mux video and audio streams together, transcoding only if needed.
    VideoMuxOrTranscode {
        video_input: PathBuf,
        audio_input: Option<PathBuf>,
        output_path: PathBuf,
        format: OutputFormat,
        duration_seconds: Option<u64>,
        temp_dir: PathBuf,
    },
    /// Transcode audio stream to target format (MP3 / FLAC).
    AudioTranscode {
        audio_input: PathBuf,
        output_path: PathBuf,
        codec: AudioCodec,
        duration_seconds: Option<u64>,
        temp_dir: PathBuf,
    },
}

use async_trait::async_trait;

/// Port trait for media conversion (e.g. FFmpeg sidecar).
#[async_trait]
pub trait MediaConverter: Send + Sync {
    /// Executes the conversion request asynchronously with progress reporting and cancellation.
    async fn convert(
        &self,
        request: ConvertRequest,
        cancellation_token: Option<tokio_util::sync::CancellationToken>,
        progress_callback: Option<ConverterProgressCallback>,
    ) -> Result<PathBuf, CoreError>;
}
