// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

pub mod error;
pub mod history;
pub mod media;

pub use error::IpcError;
pub use history::DownloadHistoryEntryDto;
pub use media::{
    AnalyzeUrlRequest, DownloadJobDto, FormatOptionDto, HealthResponse, ProbeResultDto,
    SetSettingsRequest, StartDownloadRequestDto,
};
