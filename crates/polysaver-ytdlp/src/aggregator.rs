// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! # Multi-Stream Progress Aggregator
//!
//! Aggregates the progress of the *sequential* streams yt-dlp downloads for a
//! single job (e.g. `bestvideo+bestaudio` downloads video, then audio). Each
//! stream reports its own `downloaded`/`total`, so the aggregator commits a
//! stream when its identifier changes and reports a global percentage computed
//! from committed + current bytes.
//!
//! Guarantees:
//! - monotonic percentage (never goes backwards),
//! - 100% is never emitted before [`MultiStreamProgressAggregator::finish`],
//! - `None` percentage while totals are unknown (instead of a fake value).

use polysaver_core::ports::media_downloader::StreamProgress;

/// State of the stream currently being downloaded.
#[derive(Debug, Clone)]
struct StreamState {
    id: String,
    downloaded: u64,
    total: Option<u64>,
    is_estimate: bool,
}

/// Multi-stream progress aggregator following yt-dlp's real sequential behavior.
#[derive(Debug, Default)]
pub struct MultiStreamProgressAggregator {
    /// Sum of `downloaded` over streams already committed.
    completed_bytes: u64,
    /// Sum of `total` over streams already committed (unknown totals count 0).
    completed_total: u64,
    /// Whether every committed stream had a known total.
    completed_totals_known: bool,
    /// Stream currently being reported.
    current: Option<StreamState>,
    /// Stream metadata announced by `before_dl` (id -> advertised size).
    registered: Vec<(String, Option<u64>)>,
    /// Streams already started (so registered sizes are not double counted as pending).
    seen_ids: Vec<String>,
    /// Last percentage emitted, enforcing monotonicity.
    last_emitted_percent: u8,
}

impl MultiStreamProgressAggregator {
    /// Creates an empty aggregator. Streams are learned as they report progress.
    #[must_use]
    pub fn new() -> Self {
        Self {
            completed_bytes: 0,
            completed_total: 0,
            completed_totals_known: true,
            current: None,
            registered: Vec::new(),
            seen_ids: Vec::new(),
            last_emitted_percent: 0,
        }
    }

    /// Registers stream metadata from the `before_dl` header (weighting source).
    pub fn register_stream_size(&mut self, stream_id: &str, size_bytes: Option<u64>) {
        if !self.registered.iter().any(|(id, _)| id == stream_id) {
            self.registered
                .push((stream_id.to_string(), size_bytes.filter(|s| *s > 0)));
        }
    }

    /// Returns the size advertised for a stream, if any.
    fn registered_size(&self, id: &str) -> Option<u64> {
        self.registered
            .iter()
            .find(|(rid, _)| rid == id)
            .and_then(|(_, size)| *size)
    }

    /// Commits the current stream into the completed counters.
    fn commit_current(&mut self) {
        if let Some(state) = self.current.take() {
            self.completed_bytes = self.completed_bytes.saturating_add(state.downloaded);
            let total = state
                .total
                .or_else(|| self.registered_size(&state.id))
                .unwrap_or(state.downloaded);
            self.completed_total = self.completed_total.saturating_add(total);
            if state.total.is_none() && self.registered_size(&state.id).is_none() {
                // Total unknown and not advertised: cannot trust future percentages
                // that depend on this stream's total.
                self.completed_totals_known = false;
            }
        }
    }

    /// Feeds a parsed progress event and returns the aggregated global progress.
    ///
    /// `stream_id` identifies the stream in yt-dlp's output (`%(info.format_id)s`).
    /// `status_finished` is true when the line reports `status:finished`.
    pub fn feed_with_status(
        &mut self,
        stream_id: Option<&str>,
        parsed: &StreamProgress,
        status_finished: bool,
    ) -> StreamProgress {
        let key = stream_id.map(str::to_string);

        match (&self.current, key.as_deref()) {
            // Stream change: commit the previous one and start the new one.
            (Some(current), Some(new_id)) if current.id != new_id => {
                self.commit_current();
                self.start_stream(new_id);
            }
            (None, Some(new_id)) => {
                self.start_stream(new_id);
            }
            _ => {}
        }

        if self.current.is_none() {
            // No identifier at all: use a synthetic single stream.
            self.start_stream("default");
        }

        if let Some(current) = self.current.as_mut() {
            if let Some(dl) = parsed.downloaded_bytes {
                // downloaded_bytes is monotonic per stream, but guard anyway.
                current.downloaded = dl.max(current.downloaded);
            }
            // An exact total always wins over a previously stored estimate.
            if let Some(t) = parsed.total_bytes.filter(|t| *t > 0) {
                if current.total.is_none() || current.is_estimate {
                    current.total = Some(t);
                    current.is_estimate = false;
                }
            }
            if current.total.is_none() {
                // Fall back to the estimate only when no exact total exists.
                if let Some(t) = parsed.total_bytes_estimate.filter(|t| *t > 0) {
                    // Estimates are not monotonic; keep the highest observed one
                    // to avoid regressions in the computed percentage.
                    let keep = match current.total {
                        Some(p) if p >= t => Some(p),
                        _ => Some(t),
                    };
                    current.total = keep;
                    current.is_estimate = true;
                }
            }
            // A finished stream is complete by definition: its real total is
            // whatever was downloaded, which also rescues streams with no total.
            if status_finished && current.downloaded > 0 {
                current.total = Some(current.downloaded);
                current.is_estimate = false;
            }
        }

        let percent = self.compute_percent();
        let downloaded = self.completed_bytes + self.current.as_ref().map_or(0, |c| c.downloaded);
        let total = self.compute_total();

        StreamProgress {
            percent: Some(percent),
            downloaded_bytes: Some(downloaded),
            total_bytes: total,
            total_bytes_estimate: None,
            speed_bytes_per_second: parsed.speed_bytes_per_second,
        }
    }

    /// Convenience wrapper for callers without status information.
    pub fn feed(&mut self, stream_id: Option<&str>, parsed: &StreamProgress) -> StreamProgress {
        self.feed_with_status(stream_id, parsed, false)
    }

    fn start_stream(&mut self, id: &str) {
        if !self.seen_ids.iter().any(|s| s == id) {
            self.seen_ids.push(id.to_string());
        }
        self.current = Some(StreamState {
            id: id.to_string(),
            downloaded: 0,
            total: self.registered_size(id),
            is_estimate: false,
        });
    }

    /// Sum of advertised sizes for registered streams not started yet.
    ///
    /// Including them in the denominator keeps the percentage meaningful (and
    /// monotonic) while the first of several streams is still running.
    fn pending_registered_total(&self) -> Option<u64> {
        let mut total = 0u64;
        for (id, size) in &self.registered {
            if self.seen_ids.iter().any(|s| s == id) {
                continue;
            }
            // A pending stream without an advertised size makes the global
            // total unknowable, so no percentage should be claimed.
            total = total.saturating_add((*size)?);
        }
        Some(total)
    }

    /// Computes the global percentage, or the last value while it cannot be trusted.
    ///
    /// The returned value never exceeds 99 until [`Self::finish`] is called, and
    /// never decreases.
    fn compute_percent(&mut self) -> u8 {
        if !self.completed_totals_known {
            return self.last_emitted_percent;
        }

        let current = match self.current.as_ref() {
            Some(c) => c,
            None => return self.last_emitted_percent,
        };

        let current_total = match current.total {
            Some(t) if t > 0 => t,
            _ => return self.last_emitted_percent,
        };

        let pending = match self.pending_registered_total() {
            Some(p) => p,
            None => return self.last_emitted_percent,
        };

        let grand_total = self
            .completed_total
            .saturating_add(current_total)
            .saturating_add(pending);
        if grand_total == 0 {
            return self.last_emitted_percent;
        }
        let grand_downloaded = self.completed_bytes.saturating_add(current.downloaded);

        let raw = ((grand_downloaded as f64 / grand_total as f64) * 100.0).round();
        let capped = raw.clamp(0.0, 99.0) as u8;
        let monotonic = capped.max(self.last_emitted_percent);
        self.last_emitted_percent = monotonic;
        monotonic
    }

    /// Returns `completed_total + current.total + pending registered totals` when fully known.
    fn compute_total(&self) -> Option<u64> {
        if !self.completed_totals_known {
            return None;
        }
        let current = self.current.as_ref()?;
        let current_total = current.total?;
        let pending = self.pending_registered_total()?;
        Some(
            self.completed_total
                .saturating_add(current_total)
                .saturating_add(pending),
        )
    }

    /// Marks completion: returns exactly 100% with the final byte counts.
    #[must_use]
    pub fn finish(&self) -> StreamProgress {
        let downloaded = self.completed_bytes + self.current.as_ref().map_or(0, |c| c.downloaded);
        let total = self.compute_total().or(if downloaded > 0 {
            Some(downloaded)
        } else {
            None
        });
        StreamProgress {
            percent: Some(100),
            downloaded_bytes: if downloaded > 0 {
                Some(downloaded)
            } else {
                None
            },
            total_bytes: total,
            total_bytes_estimate: None,
            speed_bytes_per_second: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(downloaded: u64, total: Option<u64>, est: Option<u64>) -> StreamProgress {
        StreamProgress {
            percent: None,
            downloaded_bytes: Some(downloaded),
            total_bytes: total,
            total_bytes_estimate: est,
            speed_bytes_per_second: Some(1_000_000),
        }
    }

    /// Single stream (audio preset): 0 -> 100%, correct totals.
    #[test]
    fn test_single_stream_audio_preset() {
        let mut agg = MultiStreamProgressAggregator::new();
        agg.register_stream_size("140", Some(5_000_000));

        let p1 = agg.feed_with_status(Some("140"), &progress(0, Some(5_000_000), None), false);
        assert_eq!(p1.percent, Some(0));
        assert_eq!(p1.downloaded_bytes, Some(0));
        assert_eq!(p1.total_bytes, Some(5_000_000));

        let p2 = agg.feed_with_status(
            Some("140"),
            &progress(2_500_000, Some(5_000_000), None),
            false,
        );
        assert_eq!(p2.percent, Some(50));
        assert_eq!(p2.downloaded_bytes, Some(2_500_000));

        let finished = agg.finish();
        assert_eq!(finished.percent, Some(100));
        assert_eq!(finished.downloaded_bytes, Some(2_500_000));
    }

    /// Two streams with known sizes: switching to stream 2 never goes backwards.
    #[test]
    fn test_two_streams_switch_never_regresses() {
        let mut agg = MultiStreamProgressAggregator::new();
        agg.register_stream_size("137", Some(80_000_000));
        agg.register_stream_size("140", Some(20_000_000));

        // Video reaches 100% of its own span; global must stay below 100.
        let p1 = agg.feed_with_status(
            Some("137"),
            &progress(80_000_000, Some(80_000_000), None),
            true,
        );
        assert_eq!(p1.percent, Some(80));
        assert!(p1.percent.unwrap() < 100, "must not hit 100% mid-way");

        // Audio starts: previous 80 MB is committed, audio contributes 0.
        let p2 = agg.feed_with_status(Some("140"), &progress(0, Some(20_000_000), None), false);
        assert_eq!(p2.percent, Some(80));
        assert_eq!(p2.downloaded_bytes, Some(80_000_000));

        // Audio at 50% -> 90% globally.
        let p3 = agg.feed_with_status(
            Some("140"),
            &progress(10_000_000, Some(20_000_000), None),
            false,
        );
        assert_eq!(p3.percent, Some(90));

        // Audio at 100% of its span: still capped below 100 until finish().
        let p4 = agg.feed_with_status(
            Some("140"),
            &progress(20_000_000, Some(20_000_000), None),
            true,
        );
        assert_eq!(p4.percent, Some(99));

        assert_eq!(agg.finish().percent, Some(100));
    }

    /// HLS stream where `total_bytes_estimate` is not monotonic.
    #[test]
    fn test_hls_estimate_non_monotonic_stays_monotonic() {
        let mut agg = MultiStreamProgressAggregator::new();

        let p1 = agg.feed_with_status(None, &progress(1_000_000, None, Some(10_000_000)), false);
        let first = p1.percent.unwrap();

        // Estimate shrinks (over-estimation corrected by yt-dlp); percent must not drop.
        let p2 = agg.feed_with_status(None, &progress(2_000_000, None, Some(6_000_000)), false);
        assert!(p2.percent.unwrap() >= first);

        // Estimate grows again.
        let p3 = agg.feed_with_status(None, &progress(3_000_000, None, Some(12_000_000)), false);
        assert!(p3.percent.unwrap() >= p2.percent.unwrap());

        // Downloaded bytes only increase.
        assert!(p3.downloaded_bytes.unwrap() >= p2.downloaded_bytes.unwrap());
    }

    /// Unknown total: no percentage is fabricated.
    #[test]
    fn test_unknown_total_emits_no_percent() {
        let mut agg = MultiStreamProgressAggregator::new();
        let p = agg.feed_with_status(None, &progress(1_234_567, None, None), false);
        assert_eq!(p.percent, Some(0));
        assert_eq!(p.downloaded_bytes, Some(1_234_567));
        assert_eq!(p.total_bytes, None);

        let finished = agg.finish();
        assert_eq!(finished.percent, Some(100));
        assert_eq!(finished.downloaded_bytes, Some(1_234_567));
    }

    /// `finish()` always reports exactly 100%.
    #[test]
    fn test_finish_reports_exactly_100() {
        let mut agg = MultiStreamProgressAggregator::new();
        let _ = agg.feed_with_status(Some("137"), &progress(1, Some(100), None), false);
        assert_eq!(agg.finish().percent, Some(100));

        let empty = MultiStreamProgressAggregator::new();
        assert_eq!(empty.finish().percent, Some(100));
    }

    /// Estimates must not overwrite a known exact total.
    #[test]
    fn test_estimate_never_overrides_exact_total() {
        let mut agg = MultiStreamProgressAggregator::new();
        let p1 = agg.feed_with_status(None, &progress(500, Some(1000), Some(10_000)), false);
        assert_eq!(p1.total_bytes, Some(1000));
        let p2 = agg.feed_with_status(None, &progress(800, Some(1000), None), false);
        assert_eq!(p2.percent, Some(80));
    }

    /// Exact totals are kept when they arrive after an estimate.
    #[test]
    fn test_exact_total_replaces_estimate() {
        let mut agg = MultiStreamProgressAggregator::new();
        let _ = agg.feed_with_status(None, &progress(500, None, Some(10_000)), false);
        let p2 = agg.feed_with_status(None, &progress(800, Some(2_000), None), false);
        assert_eq!(p2.total_bytes, Some(2_000));
        assert_eq!(p2.percent, Some(40));
    }
}
