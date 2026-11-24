//! One run of the scheduled calendar sync (spec §1.4.8).
//!
//! Claims pages of due feeds, syncs them, and stops starting new feeds at the
//! deadline (claims that never started are released). Then it republishes
//! calendar emails stuck in the outbox and logs a summary. Individual feed
//! failures never fail the run; only the database or the configuration can.

use anyhow::Context;
use api_core::calendar::sync::{OutboxPublisher, SyncDeps, sync_feeds};
use api_core::calendar::{FetcherConfig, GuardedFetcher};
use chrono::Duration as ChronoDuration;
use common::calendar::dto::SyncOutcome;
use db_core::calendar::feeds::claim_due;
use db_core::calendar::outbox::stuck_calendar_emails;
use db_core::calendar::reconcile::{SyncContext, SyncTrigger};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

/// Feeds claimed per page.
pub const PAGE_SIZE: i64 = 32;
/// A feed is due when not attempted for this long.
pub const DUE_AFTER_MINUTES: i64 = 10;
/// The run stops starting feeds after this long (the task timeout is 10 min).
pub const RUN_DEADLINE: Duration = Duration::from_secs(9 * 60);
/// Pending calendar emails older than this are republished.
pub const STUCK_AFTER_MINUTES: i64 = 5;
/// Most stuck emails republished per run.
pub const REPUBLISH_LIMIT: i64 = 100;

/// What a run needs to know.
#[derive(Debug, Clone)]
pub struct JobConfig {
    /// Database URL.
    pub database_url: String,
    /// Feeds fetched at once.
    pub concurrency: u16,
    /// When to stop starting feeds, from the start of the run.
    pub deadline: Duration,
    /// The admin portal's base URL, for links in emails.
    pub admin_base_url: String,
}

/// What a run is given.
pub struct JobDeps {
    /// Publishes queued emails.
    pub publisher: Arc<dyn OutboxPublisher>,
    /// The fetcher's limits and hooks.
    pub fetcher: FetcherConfig,
}

/// Counts for the run's summary log line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunSummary {
    /// Feeds synced.
    pub feeds_ok: u32,
    /// Feeds synced with a warning.
    pub feeds_warning: u32,
    /// Feeds that failed.
    pub feeds_failed: u32,
    /// Feeds deferred or released at the deadline.
    pub feeds_deferred: u32,
    /// Feeds whose claim went stale.
    pub feeds_skipped: u32,
    /// Conflicts that opened or reopened.
    pub conflicts_opened: u32,
    /// Stuck emails republished.
    pub emails_republished: u32,
    /// Wall time, in milliseconds.
    pub duration_ms: u128,
}

/// Runs one sync. `Err` only when the database or configuration is unusable.
pub async fn run(config: JobConfig, deps: JobDeps) -> anyhow::Result<RunSummary> {
    let started = Instant::now();
    let deadline = started + config.deadline;
    let concurrency = usize::from(config.concurrency.max(1));
    let pool = db_core::connection::create_connection_pool_sized(
        &config.database_url,
        u32::from(config.concurrency.max(1)) + 2,
    )
    .await
    .context("calendar sync job cannot reach the database")?;
    let fetcher = GuardedFetcher::new(deps.fetcher)
        .map_err(|e| anyhow::anyhow!("calendar fetcher misconfigured: {e}"))?;
    let sync = SyncDeps {
        pool: pool.clone(),
        publisher: deps.publisher.clone(),
        fetcher,
    };
    let ctx = SyncContext {
        trigger: SyncTrigger::Scheduled,
        admin_base_url: config.admin_base_url.clone(),
    };

    let mut summary = RunSummary::default();
    while Instant::now() < deadline {
        let claims = claim_due(&pool, ChronoDuration::minutes(DUE_AFTER_MINUTES), PAGE_SIZE)
            .await
            .context("claiming due calendar feeds")?;
        if claims.is_empty() {
            break;
        }
        let page = claims.len();
        for result in sync_feeds(&sync, claims, &ctx, concurrency, deadline).await {
            let count = match result.outcome {
                SyncOutcome::Ok => &mut summary.feeds_ok,
                SyncOutcome::Warning => &mut summary.feeds_warning,
                SyncOutcome::Failed => &mut summary.feeds_failed,
                SyncOutcome::Deferred => &mut summary.feeds_deferred,
                SyncOutcome::Skipped => &mut summary.feeds_skipped,
            };
            *count += 1;
            summary.conflicts_opened += u32::try_from(result.conflicts_opened).unwrap_or(0);
        }
        if i64::try_from(page).unwrap_or(i64::MAX) < PAGE_SIZE {
            break;
        }
    }

    let stuck = stuck_calendar_emails(
        &pool,
        ChronoDuration::minutes(STUCK_AFTER_MINUTES),
        REPUBLISH_LIMIT,
    )
    .await
    .context("finding stuck calendar emails")?;
    for id in stuck {
        match deps.publisher.publish(id).await {
            Ok(()) => summary.emails_republished += 1,
            Err(()) => tracing::error!(outbox_id = %id, "calendar email republish failed"),
        }
    }
    summary.duration_ms = started.elapsed().as_millis();
    tracing::info!(
        feeds_ok = summary.feeds_ok,
        feeds_warning = summary.feeds_warning,
        feeds_failed = summary.feeds_failed,
        feeds_deferred = summary.feeds_deferred,
        feeds_skipped = summary.feeds_skipped,
        conflicts_opened = summary.conflicts_opened,
        emails_republished = summary.emails_republished,
        duration_ms = summary.duration_ms as u64,
        "calendar sync run finished"
    );
    pool.close().await;
    Ok(summary)
}
