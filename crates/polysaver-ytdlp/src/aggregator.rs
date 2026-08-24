// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! # Multi-Stream Progress Aggregator
//!
//! Aggregates multi-stream (video + audio) download progress without artificial resets or jumps.
//! Produces strictly monotonic 0..100% progress weighted by stream sizes or stream counts.

use polysaver_core::ports::media_downloader::StreamProgress;
use std::collections::HashMap;

/// Individual stream progress snapshot.
#[derive(Debug, Clone)]
struct StreamState {
    percent: f64,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    weight: f64,
}

/// Multi-stream progress aggregator ensuring monotonic global progress.
#[derive(Debug)]
pub struct MultiStreamProgressAggregator {
    streams: HashMap<String, StreamState>,
    stream_order: Vec<String>,
    expected_stream_count: usize,
    last_emitted_percent: u8,
    total_known_bytes: Option<u64>,
}

impl MultiStreamProgressAggregator {
    /// Creates a new aggregator with an expected number of streams.
    #[must_use]
    pub fn new(expected_stream_count: usize) -> Self {
        Self {
            streams: HashMap::new(),
            stream_order: Vec::new(),
            expected_stream_count: expected_stream_count.max(1),
            last_emitted_percent: 0,
            total_known_bytes: None,
        }
    }

    /// Registers stream metadata (e.g. from before_dl header).
    pub fn register_stream_size(&mut self, stream_id: &str, size_bytes: Option<u64>) {
        if !self.streams.contains_key(stream_id) {
            self.stream_order.push(stream_id.to_string());
            self.streams.insert(
                stream_id.to_string(),
                StreamState {
                    percent: 0.0,
                    downloaded_bytes: 0,
                    total_bytes: size_bytes,
                    weight: 1.0,
                },
            );
            self.recompute_weights();
        }
    }

    /// Recomputes weights across all known streams.
    fn recompute_weights(&mut self) {
        let all_have_sizes = !self.streams.is_empty()
            && self
                .streams
                .values()
                .all(|s| s.total_bytes.is_some() && s.total_bytes.unwrap() > 0);

        if all_have_sizes {
            let total: u64 = self.streams.values().filter_map(|s| s.total_bytes).sum();
            self.total_known_bytes = Some(total);
            if total > 0 {
                for state in self.streams.values_mut() {
                    let s_bytes = state.total_bytes.unwrap_or(0);
                    state.weight = (s_bytes as f64) / (total as f64);
                }
                return;
            }
        }

        // Fallback: equal weight distribution
        let count = self.streams.len().max(self.expected_stream_count).max(1);
        let equal_weight = 1.0 / (count as f64);
        for state in self.streams.values_mut() {
            state.weight = equal_weight;
        }
    }

    /// Feeds a stream progress event and returns the aggregated global progress.
    pub fn feed(&mut self, stream_id: Option<&str>, parsed: &StreamProgress) -> StreamProgress {
        let key = stream_id
            .unwrap_or_else(|| {
                self.stream_order
                    .first()
                    .map(|s| s.as_str())
                    .unwrap_or("default")
            })
            .to_string();

        if !self.streams.contains_key(&key) {
            self.stream_order.push(key.clone());
            self.streams.insert(
                key.clone(),
                StreamState {
                    percent: 0.0,
                    downloaded_bytes: 0,
                    total_bytes: parsed.total_bytes,
                    weight: 1.0,
                },
            );
            self.recompute_weights();
        }

        if let Some(state) = self.streams.get_mut(&key) {
            if let Some(pct) = parsed.percent {
                state.percent = (pct as f64).clamp(state.percent, 100.0);
            }
            if let Some(dl) = parsed.downloaded_bytes {
                state.downloaded_bytes = dl.max(state.downloaded_bytes);
            }
            if state.total_bytes.is_none() && parsed.total_bytes.is_some() {
                state.total_bytes = parsed.total_bytes;
            }
        }

        // Calculate weighted percentage
        let mut aggregated_percent: f64 = 0.0;
        let mut total_downloaded: u64 = 0;
        let mut total_bytes_sum: u64 = 0;
        let mut all_totals_known = true;

        for state in self.streams.values() {
            aggregated_percent += state.weight * state.percent;
            total_downloaded += state.downloaded_bytes;
            if let Some(t) = state.total_bytes {
                total_bytes_sum += t;
            } else {
                all_totals_known = false;
            }
        }

        let rounded_pct = aggregated_percent.round().clamp(0.0, 100.0) as u8;
        let monotonic_pct = rounded_pct.max(self.last_emitted_percent);
        self.last_emitted_percent = monotonic_pct;

        StreamProgress {
            percent: Some(monotonic_pct),
            downloaded_bytes: if total_downloaded > 0 {
                Some(total_downloaded)
            } else {
                parsed.downloaded_bytes
            },
            total_bytes: if all_totals_known && total_bytes_sum > 0 {
                Some(total_bytes_sum)
            } else {
                parsed.total_bytes
            },
            speed_bytes_per_second: parsed.speed_bytes_per_second,
        }
    }

    /// Marks completion of all streams at exactly 100%.
    #[must_use]
    pub fn finish(&self) -> StreamProgress {
        StreamProgress {
            percent: Some(100),
            downloaded_bytes: self.total_known_bytes,
            total_bytes: self.total_known_bytes,
            speed_bytes_per_second: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_stream_progress_monotonicity() {
        let mut agg = MultiStreamProgressAggregator::new(1);
        let p1 = agg.feed(
            Some("stream1"),
            &StreamProgress {
                percent: Some(20),
                downloaded_bytes: Some(200),
                total_bytes: Some(1000),
                speed_bytes_per_second: Some(500),
            },
        );
        assert_eq!(p1.percent, Some(20));

        // Duplicate or lower percent cannot regress
        let p2 = agg.feed(
            Some("stream1"),
            &StreamProgress {
                percent: Some(15),
                downloaded_bytes: Some(200),
                total_bytes: Some(1000),
                speed_bytes_per_second: Some(500),
            },
        );
        assert_eq!(p2.percent, Some(20));
    }

    #[test]
    fn test_dual_stream_equal_weight_progress() {
        let mut agg = MultiStreamProgressAggregator::new(2);
        agg.register_stream_size("video", None);
        agg.register_stream_size("audio", None);

        // Stream 1 (video) 0 -> 100%
        let p1 = agg.feed(
            Some("video"),
            &StreamProgress {
                percent: Some(50),
                downloaded_bytes: None,
                total_bytes: None,
                speed_bytes_per_second: None,
            },
        );
        // 50% * 0.5 = 25%
        assert_eq!(p1.percent, Some(25));

        let p2 = agg.feed(
            Some("video"),
            &StreamProgress {
                percent: Some(100),
                downloaded_bytes: None,
                total_bytes: None,
                speed_bytes_per_second: None,
            },
        );
        // 100% * 0.5 = 50%
        assert_eq!(p2.percent, Some(50));

        // Stream 2 (audio) starts at 0% - global progress must not drop below 50%
        let p3 = agg.feed(
            Some("audio"),
            &StreamProgress {
                percent: Some(0),
                downloaded_bytes: None,
                total_bytes: None,
                speed_bytes_per_second: None,
            },
        );
        assert_eq!(p3.percent, Some(50));

        // Stream 2 reaches 100% -> global 100%
        let p4 = agg.feed(
            Some("audio"),
            &StreamProgress {
                percent: Some(100),
                downloaded_bytes: None,
                total_bytes: None,
                speed_bytes_per_second: None,
            },
        );
        assert_eq!(p4.percent, Some(100));
    }

    #[test]
    fn test_dual_stream_unequal_sizes_weighted() {
        let mut agg = MultiStreamProgressAggregator::new(2);
        // Video: 80 MB, Audio: 20 MB -> Total: 100 MB (weights: 0.8 and 0.2)
        agg.register_stream_size("video", Some(80_000_000));
        agg.register_stream_size("audio", Some(20_000_000));

        // Video at 50% -> 50% * 0.8 = 40%
        let p1 = agg.feed(
            Some("video"),
            &StreamProgress {
                percent: Some(50),
                downloaded_bytes: Some(40_000_000),
                total_bytes: Some(80_000_000),
                speed_bytes_per_second: None,
            },
        );
        assert_eq!(p1.percent, Some(40));

        // Video complete (100% * 0.8 = 80%)
        let p2 = agg.feed(
            Some("video"),
            &StreamProgress {
                percent: Some(100),
                downloaded_bytes: Some(80_000_000),
                total_bytes: Some(80_000_000),
                speed_bytes_per_second: None,
            },
        );
        assert_eq!(p2.percent, Some(80));

        // Audio at 50% -> 80% + (50% * 0.2) = 90%
        let p3 = agg.feed(
            Some("audio"),
            &StreamProgress {
                percent: Some(50),
                downloaded_bytes: Some(10_000_000),
                total_bytes: Some(20_000_000),
                speed_bytes_per_second: None,
            },
        );
        assert_eq!(p3.percent, Some(90));

        // Audio complete -> 100%
        let p4 = agg.feed(
            Some("audio"),
            &StreamProgress {
                percent: Some(100),
                downloaded_bytes: Some(20_000_000),
                total_bytes: Some(20_000_000),
                speed_bytes_per_second: None,
            },
        );
        assert_eq!(p4.percent, Some(100));
    }
}
