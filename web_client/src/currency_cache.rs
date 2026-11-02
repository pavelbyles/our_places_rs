//! In-process cache of the active currency list (#100).
//!
//! Every currency selector renders from this list, so it is cached per portal
//! instance for `CURRENCY_CACHE_TTL_SECONDS` (default 60; 0 disables). Refresh
//! is single-flight: concurrent misses after expiry share one fetch, which is
//! bounded by a timeout (default 2s) so a slow backend cannot stall every page.
//! When a refresh fails or times out the previous list is served with a
//! warning (it is display data, never money) and kept for another TTL before
//! the next attempt; with nothing cached the error is returned.

use common::currency::Currency;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// Longest a refresh may take before the stale list (or an error) is used.
pub const DEFAULT_FETCH_TIMEOUT: Duration = Duration::from_secs(2);

/// Why a refresh produced no list.
#[derive(Debug)]
pub enum RefreshError<E> {
    /// The fetch itself failed.
    Fetch(E),
    /// The fetch took longer than the cache's timeout.
    TimedOut(Duration),
}

impl<E: std::fmt::Display> std::fmt::Display for RefreshError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefreshError::Fetch(e) => write!(f, "{e}"),
            RefreshError::TimedOut(after) => write!(f, "timed out after {after:?}"),
        }
    }
}

/// Cached active currencies with a TTL.
pub struct CurrencyCache {
    ttl: Duration,
    fetch_timeout: Duration,
    clock: Clock,
    state: Mutex<Option<(Instant, Arc<Vec<Currency>>)>>,
}

impl std::fmt::Debug for CurrencyCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CurrencyCache")
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

impl CurrencyCache {
    /// A cache with the given TTL and the system clock.
    pub fn new(ttl: Duration) -> Self {
        Self::with_clock(ttl, Arc::new(Instant::now))
    }

    /// A cache with an injected clock (tests).
    pub fn with_clock(ttl: Duration, clock: Clock) -> Self {
        Self {
            ttl,
            fetch_timeout: DEFAULT_FETCH_TIMEOUT,
            clock,
            state: Mutex::new(None),
        }
    }

    /// The same cache with a different refresh timeout.
    pub fn with_fetch_timeout(mut self, timeout: Duration) -> Self {
        self.fetch_timeout = timeout;
        self
    }

    /// The cached list while fresh, otherwise the result of `fetch`.
    ///
    /// The lock is held across the (time-limited) fetch, so concurrent callers
    /// wait for one refresh instead of each calling the backend. A failed or
    /// timed-out refresh keeps serving the previous list for another TTL.
    pub async fn get_or_refresh<F, Fut, E>(
        &self,
        fetch: F,
    ) -> Result<Arc<Vec<Currency>>, RefreshError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Vec<Currency>, E>>,
        E: std::fmt::Display,
    {
        let mut state = self.state.lock().await;
        let now = (self.clock)();
        if let Some((fetched_at, list)) = state.as_ref()
            && !self.ttl.is_zero()
            && now.saturating_duration_since(*fetched_at) < self.ttl
        {
            return Ok(Arc::clone(list));
        }
        let outcome = match tokio::time::timeout(self.fetch_timeout, fetch()).await {
            Ok(Ok(list)) => Ok(list),
            Ok(Err(e)) => Err(RefreshError::Fetch(e)),
            Err(_) => Err(RefreshError::TimedOut(self.fetch_timeout)),
        };
        match outcome {
            Ok(list) => {
                let list = Arc::new(list);
                *state = Some((now, Arc::clone(&list)));
                Ok(list)
            }
            Err(e) => match state.take() {
                Some((_, stale)) => {
                    tracing::warn!(error = %e, "currency refresh failed; serving the previous list for another TTL");
                    *state = Some((now, Arc::clone(&stale)));
                    Ok(stale)
                }
                None => {
                    tracing::error!(error = %e, "currency list unavailable and nothing cached");
                    Err(e)
                }
            },
        }
    }

    /// Drops the cached list so the next read refetches (after admin edits).
    pub async fn invalidate(&self) {
        *self.state.lock().await = None;
    }
}

#[cfg(test)]
mod currency_cache_tests {
    use super::*;
    use common::currency::CurrencyCode;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    fn list(codes: &[&str]) -> Vec<Currency> {
        codes
            .iter()
            .map(|c| Currency {
                code: CurrencyCode::try_from(*c).expect("code"),
                name: (*c).to_string(),
                active: true,
            })
            .collect()
    }

    /// A clock that advances only when the test says so.
    fn manual_clock() -> (Clock, Arc<AtomicU64>) {
        let base = Instant::now();
        let offset = Arc::new(AtomicU64::new(0));
        let o = Arc::clone(&offset);
        let clock: Clock = Arc::new(move || base + Duration::from_secs(o.load(Ordering::SeqCst)));
        (clock, offset)
    }

    async fn fetch_counting(calls: &AtomicUsize, codes: &[&str]) -> Result<Vec<Currency>, String> {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(list(codes))
    }

    #[tokio::test]
    async fn serves_cached_list_within_ttl() {
        let (clock, offset) = manual_clock();
        let cache = CurrencyCache::with_clock(Duration::from_secs(60), clock);
        let calls = AtomicUsize::new(0);
        cache
            .get_or_refresh(|| fetch_counting(&calls, &["USD"]))
            .await
            .expect("first");
        offset.store(59, Ordering::SeqCst);
        let second = cache
            .get_or_refresh(|| fetch_counting(&calls, &["USD", "JMD"]))
            .await
            .expect("second");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(second.len(), 1);
    }

    #[tokio::test]
    async fn refetches_after_ttl_and_after_invalidate() {
        let (clock, offset) = manual_clock();
        let cache = CurrencyCache::with_clock(Duration::from_secs(60), clock);
        let calls = AtomicUsize::new(0);
        cache
            .get_or_refresh(|| fetch_counting(&calls, &["USD"]))
            .await
            .expect("first");
        offset.store(60, Ordering::SeqCst);
        let fresh = cache
            .get_or_refresh(|| fetch_counting(&calls, &["USD", "JMD"]))
            .await
            .expect("after ttl");
        assert_eq!(fresh.len(), 2);
        cache.invalidate().await;
        cache
            .get_or_refresh(|| fetch_counting(&calls, &["USD"]))
            .await
            .expect("after invalidate");
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn zero_ttl_always_fetches() {
        let cache = CurrencyCache::new(Duration::ZERO);
        let calls = AtomicUsize::new(0);
        for _ in 0..3 {
            cache
                .get_or_refresh(|| fetch_counting(&calls, &["USD"]))
                .await
                .expect("fetch");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn serves_stale_list_when_refresh_fails() {
        let (clock, offset) = manual_clock();
        let cache = CurrencyCache::with_clock(Duration::from_secs(60), clock);
        let calls = AtomicUsize::new(0);
        cache
            .get_or_refresh(|| fetch_counting(&calls, &["USD", "JMD"]))
            .await
            .expect("first");
        offset.store(120, Ordering::SeqCst);
        let stale = cache
            .get_or_refresh(|| async { Err::<Vec<Currency>, _>("booking_api down".to_string()) })
            .await
            .expect("stale list served");
        assert_eq!(stale.len(), 2);
    }

    #[tokio::test]
    async fn errors_when_nothing_is_cached() {
        let cache = CurrencyCache::new(Duration::from_secs(60));
        let r = cache
            .get_or_refresh(|| async { Err::<Vec<Currency>, _>("down".to_string()) })
            .await;
        assert!(
            matches!(r, Err(RefreshError::Fetch(ref e)) if e == "down"),
            "got {r:?}"
        );
    }

    #[tokio::test]
    async fn concurrent_misses_share_one_fetch() {
        let cache = Arc::new(CurrencyCache::new(Duration::from_secs(60)));
        let calls = Arc::new(AtomicUsize::new(0));
        let tasks: Vec<_> = (0..8)
            .map(|_| {
                let cache = Arc::clone(&cache);
                let calls = Arc::clone(&calls);
                tokio::spawn(async move {
                    cache
                        .get_or_refresh(|| async {
                            calls.fetch_add(1, Ordering::SeqCst);
                            tokio::time::sleep(Duration::from_millis(20)).await;
                            Ok::<_, String>(list(&["USD"]))
                        })
                        .await
                })
            })
            .collect();
        for t in tasks {
            t.await.expect("join").expect("list");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn slow_refresh_times_out_and_serves_the_previous_list() {
        let (clock, offset) = manual_clock();
        let cache = CurrencyCache::with_clock(Duration::from_secs(60), clock)
            .with_fetch_timeout(Duration::from_millis(50));
        let calls = AtomicUsize::new(0);
        cache
            .get_or_refresh(|| fetch_counting(&calls, &["USD"]))
            .await
            .expect("first");
        offset.store(120, Ordering::SeqCst);
        let started = Instant::now();
        let stale = cache
            .get_or_refresh(|| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok::<_, String>(list(&["USD", "JMD"]))
            })
            .await
            .expect("stale list served");
        assert_eq!(stale.len(), 1);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "bounded by the timeout"
        );
    }

    #[tokio::test]
    async fn slow_first_fetch_times_out_with_an_error() {
        let cache = CurrencyCache::new(Duration::from_secs(60))
            .with_fetch_timeout(Duration::from_millis(50));
        let r = cache
            .get_or_refresh(|| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok::<_, String>(list(&["USD"]))
            })
            .await;
        assert!(matches!(r, Err(RefreshError::TimedOut(_))), "got {r:?}");
    }

    #[tokio::test]
    async fn a_failed_refresh_is_not_retried_until_the_next_ttl() {
        let (clock, offset) = manual_clock();
        let cache = CurrencyCache::with_clock(Duration::from_secs(60), clock);
        let calls = AtomicUsize::new(0);
        cache
            .get_or_refresh(|| fetch_counting(&calls, &["USD"]))
            .await
            .expect("first");
        offset.store(120, Ordering::SeqCst);
        let failing = AtomicUsize::new(0);
        let fail = || async {
            failing.fetch_add(1, Ordering::SeqCst);
            Err::<Vec<Currency>, _>("down".to_string())
        };
        cache
            .get_or_refresh(fail)
            .await
            .expect("stale after failure");
        offset.store(150, Ordering::SeqCst);
        cache
            .get_or_refresh(fail)
            .await
            .expect("still stale, no retry");
        assert_eq!(
            failing.load(Ordering::SeqCst),
            1,
            "one attempt per TTL during an outage"
        );
        offset.store(181, Ordering::SeqCst);
        cache
            .get_or_refresh(|| fetch_counting(&calls, &["USD", "JMD"]))
            .await
            .expect("recovered");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
