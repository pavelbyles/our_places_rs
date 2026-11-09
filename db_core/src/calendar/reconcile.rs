//! Applying a fetched feed, and recording a failed fetch (spec §1.4.2–3).
//!
//! Reconcile is one transaction per feed. It locks listing → feed, checks the
//! claim's generation, upserts the events, deletes the ones that left the
//! feed, records conflicts and updates the feed. Bookings are only read.
//! A lock wait over 5 s (`55P03`) means "try again later", not a failure.
//!
//! Recording a failure locks the feed row only (no listing lock, so no lock
//! cycle) and never touches blocks.

use super::feeds::FeedClaim;
use crate::error::{DbError, Result, SQLSTATE_LOCK_NOT_AVAILABLE, sqlstate};
use common::calendar::FeedStatus;
use common::calendar::ics_parse::{FeedWarning, ParsedFeed};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Failed scheduled syncs in a row before the host is emailed.
pub const FAILURE_ALERT_THRESHOLD: i32 = 4;
/// Delivery attempts for calendar emails.
pub const CALENDAR_EMAIL_MAX_RETRIES: i32 = 3;
/// Outbox template for the failing-feed email.
pub const FEED_FAILING_TEMPLATE: &str = "calendar_feed_failing_host";
/// Warning stored when a feed that had future blocks returns no events (E2).
pub const NO_EVENTS_WARNING: &str = "feed returned no events";

/// What started a sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncTrigger {
    /// The scheduled job. Its failures count towards the alert.
    Scheduled,
    /// "Sync now", a new feed or a URL change. Failures don't count.
    Manual,
}

/// Context the emails need.
#[derive(Debug, Clone)]
pub struct SyncContext {
    /// What started the sync.
    pub trigger: SyncTrigger,
    /// The admin portal's base URL, for links in emails.
    pub admin_base_url: String,
}

impl SyncContext {
    /// The admin calendar page of `listing_id`.
    pub fn calendar_url(&self, listing_id: Uuid) -> String {
        format!(
            "{}/admin/listings/{listing_id}/calendar",
            self.admin_base_url.trim_end_matches('/')
        )
    }
}

/// What reconciling did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileOutcome {
    /// The feed was applied.
    Applied {
        /// `Ok` or `Warning`.
        status: FeedStatus,
        /// Events now imported.
        events: i32,
        /// The stored warning, if any.
        warning: Option<String>,
        /// Conflicts that opened or reopened.
        conflicts_opened: i32,
        /// Outbox rows to publish after commit.
        outbox_ids: Vec<Uuid>,
    },
    /// The claim is stale: the feed changed URL, was disabled or removed, or
    /// the listing was deleted. Nothing was written.
    Stale,
    /// A lock wait timed out; nothing was written and the claim just expires.
    Deferred,
}

/// What recording a failure did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureOutcome {
    /// The failure was recorded.
    Recorded {
        /// Scheduled failures in a row, after this one.
        consecutive_failures: i32,
        /// Outbox rows to publish after commit (the alert, at most one).
        outbox_ids: Vec<Uuid>,
    },
    /// The claim is stale; nothing was written.
    Stale,
    /// A lock wait timed out; nothing was written.
    Deferred,
}

async fn set_lock_timeout(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    sqlx::query("SET LOCAL lock_timeout = '5s'")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn is_lock_timeout(e: &DbError) -> bool {
    matches!(e, DbError::Sqlx(inner) if sqlstate(inner).as_deref() == Some(SQLSTATE_LOCK_NOT_AVAILABLE))
}

/// Locks the claim's feed and checks it's still live at the claimed generation.
async fn lock_claimed_feed(tx: &mut Transaction<'_, Postgres>, claim: &FeedClaim) -> Result<bool> {
    let feed = sqlx::query!(
        "SELECT sync_generation, is_active FROM listing_calendar_feed
         WHERE id = $1 AND listing_id = $2 FOR UPDATE",
        claim.id,
        claim.listing_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(feed.is_some_and(|f| f.is_active && f.sync_generation == claim.sync_generation))
}

/// Applies a parsed feed for `claim` (spec §1.4.2).
#[tracing::instrument(skip_all, fields(feed_id = %claim.id, listing_id = %claim.listing_id))]
pub async fn reconcile_feed(
    pool: &sqlx::PgPool,
    claim: &FeedClaim,
    parsed: &ParsedFeed,
    ctx: &SyncContext,
) -> Result<ReconcileOutcome> {
    match apply(pool, claim, parsed, ctx).await {
        Err(e) if is_lock_timeout(&e) => {
            tracing::warn!("calendar reconcile deferred: lock wait timed out");
            Ok(ReconcileOutcome::Deferred)
        }
        other => other,
    }
}

async fn apply(
    pool: &sqlx::PgPool,
    claim: &FeedClaim,
    parsed: &ParsedFeed,
    ctx: &SyncContext,
) -> Result<ReconcileOutcome> {
    let mut tx = pool.begin().await?;
    set_lock_timeout(&mut tx).await?;
    let Some(gap) = sqlx::query_scalar!(
        r#"SELECT LEAST(GREATEST(days_between_bookings, 0), 365) AS "gap!"
        FROM listing WHERE id = $1 AND deleted_at IS NULL FOR UPDATE"#,
        claim.listing_id
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(ReconcileOutcome::Stale);
    };
    if !lock_claimed_feed(&mut tx, claim).await? {
        return Ok(ReconcileOutcome::Stale);
    }

    let today = common::reference::listing_today(chrono::Utc::now());
    let had_future_blocks = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM listing_blocked_dates
            WHERE feed_id = $1 AND date_to > $2) AS "exists!""#,
        claim.id,
        today
    )
    .fetch_one(&mut *tx)
    .await?;

    let uids: Vec<String> = parsed.events.iter().map(|e| e.uid.clone()).collect();
    let froms: Vec<chrono::NaiveDate> = parsed.events.iter().map(|e| e.date_from).collect();
    let tos: Vec<chrono::NaiveDate> = parsed.events.iter().map(|e| e.date_to).collect();
    sqlx::query!(
        r#"INSERT INTO listing_blocked_dates (listing_id, source, feed_id, external_uid, date_from, date_to)
        SELECT $1, 'ical_feed', $2, e.uid, e.date_from, e.date_to
        FROM UNNEST($3::text[], $4::date[], $5::date[]) AS e(uid, date_from, date_to)
        ON CONFLICT (feed_id, external_uid) DO UPDATE SET
            date_from = EXCLUDED.date_from, date_to = EXCLUDED.date_to, updated_at = now()
        WHERE (listing_blocked_dates.date_from, listing_blocked_dates.date_to)
              IS DISTINCT FROM (EXCLUDED.date_from, EXCLUDED.date_to)"#,
        claim.listing_id,
        claim.id,
        &uids,
        &froms,
        &tos
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "DELETE FROM listing_blocked_dates WHERE feed_id = $1 AND NOT (external_uid = ANY($2::text[]))",
        claim.id,
        &uids
    )
    .execute(&mut *tx)
    .await?;

    let opened = super::conflicts::detect_for_feed(&mut tx, claim.id, gap, ctx).await?;

    let warning = warning_text(
        &parsed.warnings,
        parsed.events.is_empty() && had_future_blocks,
    );
    let status = match warning {
        Some(_) => FeedStatus::Warning,
        None => FeedStatus::Ok,
    };
    let events = i32::try_from(parsed.events.len()).unwrap_or(i32::MAX);
    sqlx::query!(
        "UPDATE listing_calendar_feed SET
            last_status = $2, last_success_at = now(), consecutive_failures = 0,
            failure_alerted_at = NULL, event_count = $3, last_error = NULL,
            last_warning = $4, updated_at = now()
        WHERE id = $1",
        claim.id,
        status as FeedStatus,
        events,
        warning
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(ReconcileOutcome::Applied {
        status,
        events,
        warning,
        conflicts_opened: i32::try_from(opened.len()).unwrap_or(i32::MAX),
        outbox_ids: opened,
    })
}

fn warning_text(warnings: &[FeedWarning], emptied: bool) -> Option<String> {
    let mut parts: Vec<&str> = warnings.iter().map(FeedWarning::message).collect();
    if emptied {
        parts.push(NO_EVENTS_WARNING);
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// Records a failed fetch, size check or parse for `claim` (spec §1.4.3).
/// `reason` is a sanitised one-line reason with no URL.
#[tracing::instrument(skip_all, fields(feed_id = %claim.id, listing_id = %claim.listing_id))]
pub async fn record_failure(
    pool: &sqlx::PgPool,
    claim: &FeedClaim,
    reason: &str,
    ctx: &SyncContext,
) -> Result<FailureOutcome> {
    match fail(pool, claim, reason, ctx).await {
        Err(e) if is_lock_timeout(&e) => {
            tracing::warn!("calendar failure not recorded: lock wait timed out");
            Ok(FailureOutcome::Deferred)
        }
        other => other,
    }
}

async fn fail(
    pool: &sqlx::PgPool,
    claim: &FeedClaim,
    reason: &str,
    ctx: &SyncContext,
) -> Result<FailureOutcome> {
    let mut tx = pool.begin().await?;
    set_lock_timeout(&mut tx).await?;
    if !lock_claimed_feed(&mut tx, claim).await? {
        return Ok(FailureOutcome::Stale);
    }
    let counted = ctx.trigger == SyncTrigger::Scheduled;
    let feed = sqlx::query!(
        "UPDATE listing_calendar_feed SET
            last_status = 'failed', last_error = $2,
            consecutive_failures = consecutive_failures + CASE WHEN $3 THEN 1 ELSE 0 END,
            updated_at = now()
        WHERE id = $1
        RETURNING name, consecutive_failures, failure_alerted_at",
        claim.id,
        reason,
        counted
    )
    .fetch_one(&mut *tx)
    .await?;

    let mut outbox_ids = Vec::new();
    if counted
        && feed.consecutive_failures >= FAILURE_ALERT_THRESHOLD
        && feed.failure_alerted_at.is_none()
    {
        sqlx::query!(
            "UPDATE listing_calendar_feed SET failure_alerted_at = now() WHERE id = $1",
            claim.id
        )
        .execute(&mut *tx)
        .await?;
        let owner = sqlx::query!(
            r#"SELECT u.email, l.name FROM listing l JOIN "user" u ON u.id = l.user_id
            WHERE l.id = $1"#,
            claim.listing_id
        )
        .fetch_one(&mut *tx)
        .await?;
        let payload = serde_json::json!({
            "listing_name": owner.name,
            "feed_name": feed.name,
            "error_code": reason,
            "failures": feed.consecutive_failures,
            "calendar_url": ctx.calendar_url(claim.listing_id),
        });
        let subject = format!("Calendar sync is failing for {}", owner.name);
        let outbox = crate::email_outbox::insert_email_outbox_tx(
            &mut tx,
            &owner.email,
            &subject,
            FEED_FAILING_TEMPLATE,
            &payload,
            CALENDAR_EMAIL_MAX_RETRIES,
        )
        .await?;
        tracing::info!(outbox_id = %outbox.id, "calendar feed failing alert queued");
        outbox_ids.push(outbox.id);
    }
    tx.commit().await?;
    Ok(FailureOutcome::Recorded {
        consecutive_failures: feed.consecutive_failures,
        outbox_ids,
    })
}
