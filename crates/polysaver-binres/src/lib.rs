// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! # PolySaver Binary Resolver Crate
//!
//! Autonomous, cached sidecar binary locator for `yt-dlp`, `ffmpeg`, and `ffprobe`.
//! Strictly isolated infrastructure crate without dependencies on Tauri or Core domain.

pub mod error;
pub mod resolver;

pub use error::{BinResError, BinaryKind};
pub use resolver::{query_binary_version, BinaryResolver, ResolvedBinary};
