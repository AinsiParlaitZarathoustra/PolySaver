// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

use crate::error::CoreError;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
struct LimiterState {
    limit: usize,
    active_count: usize,
}

/// Dynamic concurrency limiter with FIFO notification and cancellation support.
#[derive(Debug, Clone)]
pub struct ConcurrencyLimiter {
    state: Arc<Mutex<LimiterState>>,
    notify: Arc<Notify>,
}

impl ConcurrencyLimiter {
    /// Creates a limiter with the initial concurrency limit bounded to `1..=8`.
    #[must_use]
    pub fn new(initial_limit: usize) -> Self {
        let limit = initial_limit.clamp(1, 8);
        Self {
            state: Arc::new(Mutex::new(LimiterState {
                limit,
                active_count: 0,
            })),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Dynamically updates the maximum concurrent limit (clamped between 1 and 8).
    pub fn set_limit(&self, new_limit: usize) {
        let clamped = new_limit.clamp(1, 8);
        let notify_count = {
            let mut lock = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            lock.limit = clamped;
            clamped.saturating_sub(lock.active_count)
        };
        for _ in 0..notify_count {
            self.notify.notify_one();
        }
    }

    /// Returns the current concurrency limit.
    #[must_use]
    pub fn current_limit(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .limit
    }

    /// Returns the number of currently active jobs holding a permit.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .active_count
    }

    /// Asynchronously acquires a permit, waiting if active count reaches limit, or failing if cancelled.
    pub async fn acquire(
        &self,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<ConcurrencyPermit, CoreError> {
        loop {
            if let Some(token) = cancel_token {
                if token.is_cancelled() {
                    return Err(CoreError::OperationCancelled);
                }
            }

            let notified = self.notify.notified();

            {
                let mut lock = self
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if lock.active_count < lock.limit {
                    lock.active_count += 1;
                    return Ok(ConcurrencyPermit {
                        limiter: self.clone(),
                    });
                }
            }

            if let Some(token) = cancel_token {
                tokio::select! {
                    _ = notified => {}
                    _ = token.cancelled() => {
                        return Err(CoreError::OperationCancelled);
                    }
                }
            } else {
                notified.await;
            }
        }
    }

    fn release(&self) {
        {
            let mut lock = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if lock.active_count > 0 {
                lock.active_count -= 1;
            }
        }
        self.notify.notify_one();
    }
}

/// RAII permit that decrements the active count and wakes a waiting job on drop.
pub struct ConcurrencyPermit {
    limiter: ConcurrencyLimiter,
}

impl Drop for ConcurrencyPermit {
    fn drop(&mut self) {
        self.limiter.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_limiter_acquires_up_to_limit() {
        let limiter = ConcurrencyLimiter::new(2);
        assert_eq!(limiter.current_limit(), 2);
        assert_eq!(limiter.active_count(), 0);

        let permit1 = limiter.acquire(None).await.unwrap();
        assert_eq!(limiter.active_count(), 1);

        let permit2 = limiter.acquire(None).await.unwrap();
        assert_eq!(limiter.active_count(), 2);

        drop(permit1);
        assert_eq!(limiter.active_count(), 1);

        drop(permit2);
        assert_eq!(limiter.active_count(), 0);
    }

    #[tokio::test]
    async fn test_limiter_dynamic_limit_change() {
        let limiter = ConcurrencyLimiter::new(1);
        let _permit1 = limiter.acquire(None).await.unwrap();

        limiter.set_limit(3);
        assert_eq!(limiter.current_limit(), 3);

        let _permit2 = limiter.acquire(None).await.unwrap();
        let _permit3 = limiter.acquire(None).await.unwrap();
        assert_eq!(limiter.active_count(), 3);
    }

    #[tokio::test]
    async fn test_limiter_cancellation_during_wait() {
        let limiter = ConcurrencyLimiter::new(1);
        let _permit1 = limiter.acquire(None).await.unwrap();

        let cancel_token = CancellationToken::new();
        let token_clone = cancel_token.clone();
        let limiter_clone = limiter.clone();

        let handle = tokio::spawn(async move { limiter_clone.acquire(Some(&token_clone)).await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel_token.cancel();

        let result = handle.await.unwrap();
        assert!(matches!(result, Err(CoreError::OperationCancelled)));
    }
}
