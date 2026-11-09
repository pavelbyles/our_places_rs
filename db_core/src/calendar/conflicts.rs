//! Conflicts between imported blocks and stays here (spec §1.5).
//!
//! Detection runs inside reconcile, under the listing lock, as one upsert on
//! `(feed_id, external_uid, booking_id)`. A conflict opens when an imported
//! block, widened by the gap, reaches a confirmed stay or live hold. It
//! reopens when it was stale and is live again, or got worse (turnover to
//! overlap, or new nights covered). Only opened and reopened conflicts send
//! an email; a sync that changes nothing touches no row.
//!
//! Staleness and the current kind are worked out when read, from the block's
//! current dates and the listing's current gap.

use super::reconcile::{CALENDAR_EMAIL_MAX_RETRIES, SyncContext};
use crate::error::{DbError, Result};
use chrono::{DateTime, NaiveDate, Utc};
use common::calendar::ConflictKind;
use common::calendar::conflict::StaleReason;
use common::calendar::dto::{
    CONFLICT_PAGE_SIZE, CalendarConflictResponse, CalendarConflictsResponse, CalendarOverview,
    CalendarOverviewListing, Page,
};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Outbox template for the conflict email.
pub const CONFLICT_TEMPLATE: &str = "calendar_conflict_host";
/// Most listings the dashboard overview lists.
pub const OVERVIEW_LISTING_LIMIT: i64 = 50;

/// Records conflicts for `feed_id`'s blocks and queues one email per conflict
/// that opened or reopened. Returns the outbox ids, to publish after commit.
pub(crate) async fn detect_for_feed(
    tx: &mut Transaction<'_, Postgres>,
    feed_id: Uuid,
    gap: i32,
    ctx: &SyncContext,
) -> Result<Vec<Uuid>> {
    // Conflicts whose block no longer reaches a live stay keep the block's
    // current dates, so moving back onto the stay reads as a reopen.
    sqlx::query!(
        "UPDATE calendar_conflict c
         SET block_date_from = k.date_from, block_date_to = k.date_to
         FROM listing_blocked_dates k, booking b
         WHERE c.feed_id = $1 AND k.id = c.block_id AND b.id = c.booking_id
           AND (c.block_date_from, c.block_date_to) IS DISTINCT FROM (k.date_from, k.date_to)
           AND NOT (
               (b.status = 'confirmed' OR (b.status = 'pending' AND b.expires_at > clock_timestamp()))
               AND k.date_from - $2::int4 < b.date_to AND k.date_to + $2::int4 > b.date_from
           )",
        feed_id,
        gap
    )
    .execute(&mut **tx)
    .await?;

    let changed = sqlx::query!(
        r#"WITH cand AS (
            SELECT k.id AS block_id, k.listing_id, k.feed_id, f.name AS feed_name,
                k.external_uid, k.date_from, k.date_to, b.id AS booking_id,
                (CASE WHEN k.date_from < b.date_to AND k.date_to > b.date_from
                    THEN 'overlap' ELSE 'turnover' END)::calendar_conflict_kind AS kind
            FROM listing_blocked_dates k
            JOIN listing_calendar_feed f ON f.id = k.feed_id
            JOIN booking b ON b.listing_id = k.listing_id
            WHERE k.feed_id = $1
              AND (b.status = 'confirmed' OR (b.status = 'pending' AND b.expires_at > clock_timestamp()))
              AND k.date_from - $2::int4 < b.date_to AND k.date_to + $2::int4 > b.date_from
        )
        INSERT INTO calendar_conflict AS cc (listing_id, feed_id, feed_name, external_uid, block_id,
            booking_id, kind, block_date_from, block_date_to)
        SELECT listing_id, feed_id, feed_name, external_uid, block_id, booking_id, kind,
            date_from, date_to
        FROM cand
        ON CONFLICT (feed_id, external_uid, booking_id) DO UPDATE SET
            block_id = EXCLUDED.block_id,
            block_date_from = EXCLUDED.block_date_from,
            block_date_to = EXCLUDED.block_date_to,
            kind = EXCLUDED.kind,
            acknowledged_at = CASE WHEN (
                    cc.block_id IS NULL
                    OR NOT (cc.block_date_from - $2::int4 < (SELECT date_to FROM booking WHERE id = EXCLUDED.booking_id)
                        AND cc.block_date_to + $2::int4 > (SELECT date_from FROM booking WHERE id = EXCLUDED.booking_id))
                    OR (cc.kind = 'turnover' AND EXCLUDED.kind = 'overlap')
                    OR EXCLUDED.block_date_from < cc.block_date_from
                    OR EXCLUDED.block_date_to > cc.block_date_to
                ) THEN NULL ELSE cc.acknowledged_at END,
            acknowledged_by = CASE WHEN (
                    cc.block_id IS NULL
                    OR NOT (cc.block_date_from - $2::int4 < (SELECT date_to FROM booking WHERE id = EXCLUDED.booking_id)
                        AND cc.block_date_to + $2::int4 > (SELECT date_from FROM booking WHERE id = EXCLUDED.booking_id))
                    OR (cc.kind = 'turnover' AND EXCLUDED.kind = 'overlap')
                    OR EXCLUDED.block_date_from < cc.block_date_from
                    OR EXCLUDED.block_date_to > cc.block_date_to
                ) THEN NULL ELSE cc.acknowledged_by END,
            reopened_at = CASE WHEN (
                    cc.block_id IS NULL
                    OR NOT (cc.block_date_from - $2::int4 < (SELECT date_to FROM booking WHERE id = EXCLUDED.booking_id)
                        AND cc.block_date_to + $2::int4 > (SELECT date_from FROM booking WHERE id = EXCLUDED.booking_id))
                    OR (cc.kind = 'turnover' AND EXCLUDED.kind = 'overlap')
                    OR EXCLUDED.block_date_from < cc.block_date_from
                    OR EXCLUDED.block_date_to > cc.block_date_to
                ) THEN now() ELSE cc.reopened_at END
        WHERE (cc.block_id, cc.block_date_from, cc.block_date_to, cc.kind)
              IS DISTINCT FROM (EXCLUDED.block_id, EXCLUDED.block_date_from, EXCLUDED.block_date_to, EXCLUDED.kind)
        RETURNING cc.id, (cc.xmax = 0) AS "inserted!",
            (cc.reopened_at IS NOT DISTINCT FROM now()) AS "reopened!""#,
        feed_id,
        gap
    )
    .fetch_all(&mut **tx)
    .await?;

    let mut outbox_ids = Vec::new();
    for row in changed.iter().filter(|r| r.inserted || r.reopened) {
        outbox_ids.push(queue_conflict_email(tx, row.id, row.reopened, ctx).await?);
    }
    if !outbox_ids.is_empty() {
        tracing::info!(
            conflicts = outbox_ids.len(),
            "calendar conflicts opened or reopened"
        );
    }
    Ok(outbox_ids)
}

async fn queue_conflict_email(
    tx: &mut Transaction<'_, Postgres>,
    conflict_id: Uuid,
    reopened: bool,
    ctx: &SyncContext,
) -> Result<Uuid> {
    let c = sqlx::query!(
        r#"SELECT c.listing_id, c.feed_name, c.block_date_from, c.block_date_to,
            c.kind AS "kind: ConflictKind", b.confirmation_code, l.name AS listing_name,
            u.email AS owner_email
        FROM calendar_conflict c
        JOIN booking b ON b.id = c.booking_id
        JOIN listing l ON l.id = c.listing_id
        JOIN "user" u ON u.id = l.user_id
        WHERE c.id = $1"#,
        conflict_id
    )
    .fetch_one(&mut **tx)
    .await?;
    let kind = match c.kind {
        ConflictKind::Overlap => "overlap",
        ConflictKind::Turnover => "turnover",
    };
    let payload = serde_json::to_value(common::email::CalendarConflictHostPayload {
        listing_name: c.listing_name.clone(),
        feed_name: c.feed_name,
        date_from: c.block_date_from.to_string(),
        date_to: c.block_date_to.to_string(),
        confirmation_code: c.confirmation_code,
        kind: kind.to_string(),
        reopened: reopened.to_string(),
        calendar_url: ctx.calendar_url(c.listing_id),
    })
    .map_err(|e| DbError::ValidationError(format!("conflict email payload: {e}")))?;
    let subject = format!("Calendar conflict at {}", c.listing_name);
    let outbox = crate::email_outbox::insert_email_outbox_tx(
        tx,
        &c.owner_email,
        &subject,
        CONFLICT_TEMPLATE,
        &payload,
        CALENDAR_EMAIL_MAX_RETRIES,
    )
    .await?;
    Ok(outbox.id)
}

/// A conflict row joined with its booking and current block.
struct ConflictRow {
    id: Uuid,
    listing_id: Uuid,
    feed_id: Option<Uuid>,
    feed_name: String,
    block_id: Option<Uuid>,
    booking_id: Uuid,
    confirmation_code: String,
    booking_date_from: NaiveDate,
    booking_date_to: NaiveDate,
    kind: ConflictKind,
    block_date_from: NaiveDate,
    block_date_to: NaiveDate,
    detected_at: DateTime<Utc>,
    reopened_at: Option<DateTime<Utc>>,
    acknowledged_at: Option<DateTime<Utc>>,
    acknowledged_by: Option<Uuid>,
    stale_reason: Option<String>,
}

impl From<ConflictRow> for CalendarConflictResponse {
    fn from(r: ConflictRow) -> Self {
        CalendarConflictResponse {
            id: r.id,
            listing_id: r.listing_id,
            feed_id: r.feed_id,
            feed_name: r.feed_name,
            block_id: r.block_id,
            booking_id: r.booking_id,
            confirmation_code: r.confirmation_code,
            booking_date_from: r.booking_date_from,
            booking_date_to: r.booking_date_to,
            kind: r.kind,
            block_date_from: r.block_date_from,
            block_date_to: r.block_date_to,
            detected_at: r.detected_at,
            reopened: r.reopened_at.is_some(),
            reopened_at: r.reopened_at,
            acknowledged_at: r.acknowledged_at,
            acknowledged_by: r.acknowledged_by,
            stale_reason: r.stale_reason.as_deref().and_then(StaleReason::from_code),
        }
    }
}

/// A listing's conflicts: open ones (live first, then stale), and one page of
/// acknowledged history, newest first. `page` is 1-based.
#[tracing::instrument(skip(pool))]
pub async fn list_conflicts(
    pool: &PgPool,
    listing_id: Uuid,
    page: i64,
) -> Result<CalendarConflictsResponse> {
    let page = page.max(1);
    let open = sqlx::query_as!(
        ConflictRow,
        r#"SELECT c.id, c.listing_id, c.feed_id, c.feed_name, c.block_id, c.booking_id,
            b.confirmation_code, b.date_from AS booking_date_from, b.date_to AS booking_date_to,
            COALESCE(CASE WHEN k.id IS NOT NULL THEN
                (CASE WHEN k.date_from < b.date_to AND k.date_to > b.date_from
                    THEN 'overlap' ELSE 'turnover' END)::calendar_conflict_kind END,
                c.kind) AS "kind!: ConflictKind",
            c.block_date_from, c.block_date_to, c.detected_at, c.reopened_at,
            c.acknowledged_at, c.acknowledged_by,
            CASE
                WHEN c.block_id IS NULL THEN 'event_removed'
                WHEN NOT (b.status = 'confirmed' OR (b.status = 'pending' AND b.expires_at > now()))
                    THEN 'booking_released'
                WHEN NOT (k.date_from - LEAST(GREATEST(l.days_between_bookings, 0), 365) < b.date_to
                      AND k.date_to + LEAST(GREATEST(l.days_between_bookings, 0), 365) > b.date_from)
                    THEN 'no_longer_overlapping'
            END AS stale_reason
        FROM calendar_conflict c
        JOIN booking b ON b.id = c.booking_id
        JOIN listing l ON l.id = c.listing_id
        LEFT JOIN listing_blocked_dates k ON k.id = c.block_id
        WHERE c.listing_id = $1 AND c.acknowledged_at IS NULL
        ORDER BY 17 NULLS FIRST, c.detected_at DESC, c.id"#,
        listing_id
    )
    .fetch_all(pool)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!" FROM calendar_conflict
        WHERE listing_id = $1 AND acknowledged_at IS NOT NULL"#,
        listing_id
    )
    .fetch_one(pool)
    .await?;
    let history = sqlx::query_as!(
        ConflictRow,
        r#"SELECT c.id, c.listing_id, c.feed_id, c.feed_name, c.block_id, c.booking_id,
            b.confirmation_code, b.date_from AS booking_date_from, b.date_to AS booking_date_to,
            c.kind AS "kind!: ConflictKind",
            c.block_date_from, c.block_date_to, c.detected_at, c.reopened_at,
            c.acknowledged_at, c.acknowledged_by, NULL::text AS stale_reason
        FROM calendar_conflict c
        JOIN booking b ON b.id = c.booking_id
        WHERE c.listing_id = $1 AND c.acknowledged_at IS NOT NULL
        ORDER BY c.acknowledged_at DESC, c.id
        LIMIT $2 OFFSET $3"#,
        listing_id,
        CONFLICT_PAGE_SIZE,
        (page - 1).saturating_mul(CONFLICT_PAGE_SIZE)
    )
    .fetch_all(pool)
    .await?;
    Ok(CalendarConflictsResponse {
        open: open.into_iter().map(Into::into).collect(),
        history: Page {
            items: history.into_iter().map(Into::into).collect(),
            page,
            per_page: CONFLICT_PAGE_SIZE,
            total,
        },
    })
}

/// Acknowledges a listing's conflict. Repeating it changes nothing. `false`
/// if the conflict isn't the listing's.
#[tracing::instrument(skip(pool))]
pub async fn acknowledge(
    pool: &PgPool,
    listing_id: Uuid,
    conflict_id: Uuid,
    user_id: Uuid,
) -> Result<bool> {
    let row = sqlx::query_scalar!(
        "UPDATE calendar_conflict SET
            acknowledged_by = CASE WHEN acknowledged_at IS NULL THEN $3 ELSE acknowledged_by END,
            acknowledged_at = COALESCE(acknowledged_at, now())
         WHERE id = $1 AND listing_id = $2
         RETURNING id",
        conflict_id,
        listing_id,
        user_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

/// Open, non-stale conflicts and failing feeds across the caller's listings
/// (every listing for the admin), for the dashboard.
#[tracing::instrument(skip(pool))]
pub async fn overview_for_user(
    pool: &PgPool,
    user_id: Uuid,
    is_admin: bool,
) -> Result<CalendarOverview> {
    let rows = sqlx::query!(
        r#"WITH mine AS (
            SELECT l.id, l.name, LEAST(GREATEST(l.days_between_bookings, 0), 365) AS gap
            FROM listing l
            WHERE l.deleted_at IS NULL AND ($2 OR l.user_id = $1)
        ),
        open_conflicts AS (
            SELECT c.listing_id, COUNT(*) AS n
            FROM calendar_conflict c
            JOIN mine m ON m.id = c.listing_id
            JOIN booking b ON b.id = c.booking_id
            JOIN listing_blocked_dates k ON k.id = c.block_id
            WHERE c.acknowledged_at IS NULL
              AND (b.status = 'confirmed' OR (b.status = 'pending' AND b.expires_at > now()))
              AND k.date_from - m.gap < b.date_to AND k.date_to + m.gap > b.date_from
            GROUP BY c.listing_id
        ),
        failing AS (
            SELECT f.listing_id, COUNT(*) AS n
            FROM listing_calendar_feed f JOIN mine m ON m.id = f.listing_id
            WHERE f.is_active AND f.consecutive_failures >= $3
            GROUP BY f.listing_id
        )
        SELECT m.id AS "listing_id!", m.name AS "listing_name!",
            COALESCE(oc.n, 0) AS "open_conflicts!", COALESCE(fl.n, 0) AS "failing_feeds!"
        FROM mine m
        LEFT JOIN open_conflicts oc ON oc.listing_id = m.id
        LEFT JOIN failing fl ON fl.listing_id = m.id
        WHERE oc.n IS NOT NULL OR fl.n IS NOT NULL
        ORDER BY COALESCE(oc.n, 0) + COALESCE(fl.n, 0) DESC, m.name
        LIMIT $4"#,
        user_id,
        is_admin,
        super::reconcile::FAILURE_ALERT_THRESHOLD,
        OVERVIEW_LISTING_LIMIT
    )
    .fetch_all(pool)
    .await
    .map_err(DbError::from)?;
    let listings: Vec<CalendarOverviewListing> = rows
        .into_iter()
        .map(|r| CalendarOverviewListing {
            listing_id: r.listing_id,
            listing_name: r.listing_name,
            open_conflicts: r.open_conflicts,
            failing_feeds: r.failing_feeds,
        })
        .collect();
    Ok(CalendarOverview {
        open_conflicts: listings.iter().map(|l| l.open_conflicts).sum(),
        failing_feeds: listings.iter().map(|l| l.failing_feeds).sum(),
        listings,
    })
}
