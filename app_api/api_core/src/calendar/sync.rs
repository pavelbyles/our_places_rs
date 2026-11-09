//! Syncing claimed feeds: shared by "Sync now" in `listing_api` and the
//! scheduled job (spec §1.4.5–8).
//!
//! - Fetches run in parallel, up to `concurrency`, and hold no connection.
//! - Reconciles run one feed at a time within a listing, so one listing never
//!   holds several connections waiting on its own lock. Listings run in
//!   parallel.
//! - The deadline cuts off fetching and parsing only. A feed not started by
//!   the deadline gets its claim released; one cut off mid-fetch is
//!   `deferred` and its claim simply expires. A reconcile that has started
//!   always finishes (its `lock_timeout` bounds it).
//! - Emails queued by a reconcile are published after commit, from a
//!   detached task. A publish failure is logged at `error`; the job
//!   republishes stuck calendar emails later.

use super::fetch::GuardedFetcher;
use common::calendar::dto::{FeedSyncResult, SyncOutcome};
use common::calendar::feed_url::redact;
use common::calendar::ics_parse::{FeedError, ParsedFeed, parse_feed};
use db_core::calendar::feeds::{FeedClaim, release_claim};
use db_core::calendar::reconcile::{
    FailureOutcome, ReconcileOutcome, SyncContext, reconcile_feed, record_failure,
};
use futures_util::StreamExt;
use sqlx::PgPool;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tracing::Instrument;
use uuid::Uuid;

/// Bodies larger than this are parsed on the blocking pool.
pub const BLOCKING_PARSE_BYTES: usize = 64 * 1024;

/// Publishes queued outbox emails. [`crate::email_publisher::EmailPublisher`]
/// is the production one; tests count calls.
#[async_trait::async_trait]
pub trait OutboxPublisher: Send + Sync {
    /// Publishes one outbox row.
    async fn publish(&self, outbox_id: Uuid) -> Result<(), ()>;
}

#[async_trait::async_trait]
impl OutboxPublisher for crate::email_publisher::EmailPublisher {
    async fn publish(&self, outbox_id: Uuid) -> Result<(), ()> {
        self.publish_email_event(outbox_id).await.map_err(|_| ())
    }
}

/// What one feed's sync needs besides its claim.
#[derive(Clone)]
pub struct SyncDeps {
    /// Database pool.
    pub pool: PgPool,
    /// Publishes queued emails.
    pub publisher: Arc<dyn OutboxPublisher>,
    /// The guarded fetcher.
    pub fetcher: GuardedFetcher,
}

/// Why a fetched body couldn't be applied.
enum Fetched {
    Parsed(ParsedFeed),
    Failed { code: &'static str, reason: String },
    TimedOut,
}

/// Syncs `claims` and returns one result per claim, in claim order.
#[tracing::instrument(skip_all, fields(feeds = claims.len()))]
pub async fn sync_feeds(
    deps: &SyncDeps,
    claims: Vec<FeedClaim>,
    ctx: &SyncContext,
    concurrency: usize,
    deadline: Instant,
) -> Vec<FeedSyncResult> {
    let order: Vec<Uuid> = claims.iter().map(|c| c.id).collect();
    let mut by_listing: BTreeMap<Uuid, Vec<FeedClaim>> = BTreeMap::new();
    for claim in claims {
        by_listing.entry(claim.listing_id).or_default().push(claim);
    }
    let permits = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut results: Vec<FeedSyncResult> = futures_util::stream::iter(by_listing)
        .map(|(listing_id, claims)| {
            let permits = permits.clone();
            async move { sync_listing(deps, listing_id, claims, ctx, permits, deadline).await }
        })
        .buffer_unordered(concurrency.max(1))
        .flat_map(futures_util::stream::iter)
        .collect()
        .await;
    results.sort_by_key(|r| order.iter().position(|id| *id == r.feed_id));
    results
}

#[tracing::instrument(skip_all, fields(listing_id = %listing_id, feeds = claims.len()))]
async fn sync_listing(
    deps: &SyncDeps,
    listing_id: Uuid,
    claims: Vec<FeedClaim>,
    ctx: &SyncContext,
    permits: Arc<Semaphore>,
    deadline: Instant,
) -> Vec<FeedSyncResult> {
    let mut results = Vec::with_capacity(claims.len());
    let mut fetches = tokio::task::JoinSet::new();
    for claim in claims {
        if Instant::now() >= deadline {
            release(&deps.pool, &claim).await;
            results.push(result(claim.id, SyncOutcome::Deferred));
            continue;
        }
        let fetcher = deps.fetcher.clone();
        let permits = permits.clone();
        let span = feed_span(&claim);
        fetches.spawn(
            async move {
                let fetched = fetch_and_parse(&fetcher, &claim, permits, deadline).await;
                (claim, fetched)
            }
            .instrument(span),
        );
    }
    // Reconcile each feed as its fetch finishes, one at a time.
    while let Some(joined) = fetches.join_next().await {
        let (claim, fetched) = match joined {
            Ok(done) => done,
            Err(e) => {
                tracing::error!(error = %e, "calendar fetch task failed");
                continue;
            }
        };
        let span = feed_span(&claim);
        let outcome = apply(deps, &claim, fetched, ctx).instrument(span).await;
        results.push(outcome);
    }
    results
}

fn feed_span(claim: &FeedClaim) -> tracing::Span {
    let host = url::Url::parse(&claim.url)
        .map(|u| redact(&u))
        .unwrap_or_else(|_| "<invalid>".to_string());
    tracing::info_span!("calendar_feed", feed_id = %claim.id, host = %host)
}

async fn release(pool: &PgPool, claim: &FeedClaim) {
    if let Err(e) = release_claim(pool, claim).await {
        tracing::warn!(feed_id = %claim.id, error = ?e, "calendar claim not released");
    }
}

async fn fetch_and_parse(
    fetcher: &GuardedFetcher,
    claim: &FeedClaim,
    permits: Arc<Semaphore>,
    deadline: Instant,
) -> Fetched {
    let work = async {
        let _permit = permits.acquire_owned().await.ok();
        let url = match url::Url::parse(&claim.url) {
            Ok(url) => url,
            Err(_) => {
                return Fetched::Failed {
                    code: "fetch_invalid_url",
                    reason: "calendar URL is not allowed".to_string(),
                };
            }
        };
        let body = match fetcher.fetch(&url).await {
            Ok(body) => body,
            Err(e) => {
                tracing::warn!(code = e.code(), "calendar fetch failed");
                return Fetched::Failed {
                    code: e.code(),
                    reason: e.to_string(),
                };
            }
        };
        match parse(body, claim.id).await {
            Ok(parsed) => Fetched::Parsed(parsed),
            Err(e) => {
                tracing::warn!(code = e.code(), "calendar feed refused");
                Fetched::Failed {
                    code: e.code(),
                    reason: e.to_string(),
                }
            }
        }
    };
    tokio::time::timeout_at(deadline, work)
        .await
        .unwrap_or(Fetched::TimedOut)
}

async fn parse(body: Vec<u8>, feed_id: Uuid) -> Result<ParsedFeed, FeedError> {
    let today = common::reference::listing_today(chrono::Utc::now());
    match body.len() > BLOCKING_PARSE_BYTES {
        true => tokio::task::spawn_blocking(move || parse_feed(&body, feed_id, today))
            .await
            .unwrap_or(Err(FeedError::Truncated)),
        false => parse_feed(&body, feed_id, today),
    }
}

fn result(feed_id: Uuid, outcome: SyncOutcome) -> FeedSyncResult {
    FeedSyncResult {
        feed_id,
        outcome,
        events: None,
        error_code: None,
        conflicts_opened: 0,
    }
}

async fn apply(
    deps: &SyncDeps,
    claim: &FeedClaim,
    fetched: Fetched,
    ctx: &SyncContext,
) -> FeedSyncResult {
    match fetched {
        Fetched::TimedOut => {
            tracing::info!("calendar sync deferred: deadline reached");
            result(claim.id, SyncOutcome::Deferred)
        }
        Fetched::Failed { code, reason } => {
            match record_failure(&deps.pool, claim, &reason, ctx).await {
                Ok(FailureOutcome::Recorded { outbox_ids, .. }) => {
                    publish_detached(deps.publisher.clone(), outbox_ids);
                    FeedSyncResult {
                        error_code: Some(code.to_string()),
                        ..result(claim.id, SyncOutcome::Failed)
                    }
                }
                Ok(FailureOutcome::Stale) => result(claim.id, SyncOutcome::Skipped),
                Ok(FailureOutcome::Deferred) => result(claim.id, SyncOutcome::Deferred),
                Err(e) => {
                    tracing::error!(error = ?e, "calendar failure not recorded");
                    FeedSyncResult {
                        error_code: Some(code.to_string()),
                        ..result(claim.id, SyncOutcome::Failed)
                    }
                }
            }
        }
        Fetched::Parsed(parsed) => match reconcile_feed(&deps.pool, claim, &parsed, ctx).await {
            Ok(ReconcileOutcome::Applied {
                status,
                events,
                conflicts_opened,
                outbox_ids,
                ..
            }) => {
                publish_detached(deps.publisher.clone(), outbox_ids);
                let warned = status == common::calendar::FeedStatus::Warning;
                FeedSyncResult {
                    feed_id: claim.id,
                    outcome: if warned {
                        SyncOutcome::Warning
                    } else {
                        SyncOutcome::Ok
                    },
                    events: Some(events),
                    error_code: warned.then(|| {
                        parsed
                            .warnings
                            .first()
                            .map(|w| w.code())
                            .unwrap_or("feed_returned_no_events")
                            .to_string()
                    }),
                    conflicts_opened,
                }
            }
            Ok(ReconcileOutcome::Stale) => result(claim.id, SyncOutcome::Skipped),
            Ok(ReconcileOutcome::Deferred) => result(claim.id, SyncOutcome::Deferred),
            Err(e) => {
                tracing::error!(error = ?e, "calendar reconcile failed");
                FeedSyncResult {
                    error_code: Some("reconcile_failed".to_string()),
                    ..result(claim.id, SyncOutcome::Failed)
                }
            }
        },
    }
}

/// Publishes committed outbox rows without holding up the caller.
fn publish_detached(publisher: Arc<dyn OutboxPublisher>, outbox_ids: Vec<Uuid>) {
    if outbox_ids.is_empty() {
        return;
    }
    tokio::spawn(
        async move {
            for id in outbox_ids {
                if publisher.publish(id).await.is_err() {
                    tracing::error!(outbox_id = %id, "calendar email publish failed; the job republishes it");
                }
            }
        }
        .instrument(tracing::Span::current()),
    );
}
