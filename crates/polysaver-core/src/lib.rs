// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! # PolySaver Core
//!
//! Sovereign domain layer and canonical source of truth for PolySaver V2.
//!
//! - Implements pure domain models, state machines, and business invariants.
//! - Defines port interfaces (traits) for peripheral adapters.
//! - Orchestrates use case services.
//! - Zero dependencies on Tauri, frontend, external processes, or network.

pub mod domain;
pub mod error;
pub mod ports;
pub mod services;

pub use domain::{
    AppSettings, AppSettingsDto, DownloadId, DownloadJob, DownloadPreset, DownloadPresetDto,
    DownloadStatus, FormatOption, MediaUrl, Mp3Quality, OutputFormat, ProbeResult, ThemeMode,
    VideoQuality,
};
pub use error::CoreError;
pub use ports::{
    AudioCodec, ConvertRequest, DownloadProgressEvent, DownloadStreamRequest, DownloadedStreams,
    EventSink, MediaConverter, MediaDownloader, MediaProvider, ProgressPhase, SettingsRepository,
    StreamProgress,
};
pub use services::{AnalyzeUrlService, StartDownloadService};
