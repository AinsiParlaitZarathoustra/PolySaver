// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! Bounded exponential backoff policy for automatic download retries.

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio_util::sync::CancellationToken;

/// Retry policy applied to transient download failures.
///
/// Defaults: 3 attempts total (1 initial + 2 retries) with delays 2 s and 8 s,
/// each jittered by ±25 %. Tests inject a zero-delay policy to stay fast.
///
/// The default attempt count is mirrored in the frontend as `MAX_RETRY_ATTEMPTS`
/// (`src/ipc/contracts.ts`) for the "Attempt n/max" chip; keep both in sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Total number of attempts, including the initial one. Clamped to 1..=4.
    pub max_attempts: u32,
    /// Base delay before the first retry; doubled for each subsequent retry.
    pub base_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_secs(2),
        }
    }
}

impl RetryPolicy {
    /// Creates a policy, clamping the attempt count to the supported range.
    #[must_use]
    pub fn new(max_attempts: u32, base_delay: Duration) -> Self {
        Self {
            max_attempts: max_attempts.clamp(1, 4),
            base_delay,
        }
    }

    /// Policy used in tests: 3 attempts, no waiting.
    #[must_use]
    pub fn immediate() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::ZERO,
        }
    }

    /// Delay before attempt `attempt + 1`, applying exponential growth and jitter.
    ///
    /// With the default 2 s base the sequence is 2 s, 8 s, 32 s (each delay ×4).
    #[must_use]
    pub fn delay_before(&self, attempt: u32) -> Duration {
        if self.base_delay.is_zero() || attempt == 0 {
            return Duration::ZERO;
        }
        let multiplier = 1u32 << (2 * (attempt - 1).min(4));
        let base = self.base_delay.saturating_mul(multiplier);
        apply_jitter(base)
    }

    /// Waits before the next attempt, returning `false` if cancellation fired.
    ///
    /// Cancellation interrupts the sleep immediately (no residual wait).
    pub async fn sleep_before_retry(&self, attempt: u32, token: &CancellationToken) -> bool {
        let delay = self.delay_before(attempt);
        if token.is_cancelled() {
            return false;
        }
        if delay.is_zero() {
            return true;
        }
        tokio::select! {
            _ = token.cancelled() => false,
            _ = tokio::time::sleep(delay) => !token.is_cancelled(),
        }
    }
}

/// Applies ±25 % jitter using the clock as an entropy source (no `rand` dependency).
fn apply_jitter(base: Duration) -> Duration {
    if base.is_zero() {
        return base;
    }
    let micros = base.as_micros() as u64;
    let entropy = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::from(d.subsec_nanos()))
        .unwrap_or(0);
    // Map entropy into [-25 %, +25 %].
    let span = micros / 4;
    if span == 0 {
        return base;
    }
    let offset = entropy % (2 * span + 1);
    let jittered = micros.saturating_sub(span).saturating_add(offset);
    Duration::from_micros(jittered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy_shape() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.base_delay, Duration::from_secs(2));
        assert_eq!(RetryPolicy::new(99, Duration::from_secs(1)).max_attempts, 4);
        assert_eq!(RetryPolicy::new(0, Duration::from_secs(1)).max_attempts, 1);
    }

    /// Pins the value mirrored by `MAX_RETRY_ATTEMPTS` in `src/ipc/contracts.ts`.
    #[test]
    fn test_default_max_attempts_is_the_ui_contract_value() {
        assert_eq!(RetryPolicy::default().max_attempts, 3);
    }

    #[test]
    fn test_exponential_backoff_within_jitter_bounds() {
        let policy = RetryPolicy::new(4, Duration::from_secs(2));

        // No delay before the first attempt.
        assert_eq!(policy.delay_before(0), Duration::ZERO);

        // Attempt 1 => ~2 s, attempt 2 => ~8 s, attempt 3 => ~32 s (all ±25 %).
        let d1 = policy.delay_before(1).as_millis();
        assert!((1500..=2500).contains(&d1), "d1 = {d1} ms");

        let d2 = policy.delay_before(2).as_millis();
        assert!((6000..=10000).contains(&d2), "d2 = {d2} ms");

        let d3 = policy.delay_before(3).as_millis();
        assert!((24000..=40000).contains(&d3), "d3 = {d3} ms");
    }

    #[test]
    fn test_immediate_policy_has_no_delay() {
        let policy = RetryPolicy::immediate();
        assert_eq!(policy.delay_before(1), Duration::ZERO);
        assert_eq!(policy.delay_before(3), Duration::ZERO);
    }

    #[tokio::test]
    async fn test_sleep_returns_immediately_when_cancelled() {
        let policy = RetryPolicy::new(3, Duration::from_secs(60));
        let token = CancellationToken::new();
        token.cancel();
        let start = std::time::Instant::now();
        let proceed = policy.sleep_before_retry(1, &token).await;
        assert!(!proceed);
        assert!(
            start.elapsed() < Duration::from_millis(50),
            "cancellation must not wait for the backoff delay"
        );
    }

    #[tokio::test]
    async fn test_sleep_interrupted_mid_wait() {
        let policy = RetryPolicy::new(3, Duration::from_secs(30));
        let token = CancellationToken::new();
        let child = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            child.cancel();
        });
        let start = std::time::Instant::now();
        let proceed = policy.sleep_before_retry(1, &token).await;
        assert!(!proceed);
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "cancellation must interrupt the ongoing wait"
        );
    }
}
