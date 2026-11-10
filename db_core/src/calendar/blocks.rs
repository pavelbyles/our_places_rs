//! Blocked dates: the overlap check other paths share, and manual blocks.

use crate::error::Result;
use chrono::NaiveDate;
use sqlx::PgConnection;
use uuid::Uuid;

/// Whether any block on `listing_id` makes `[date_from, date_to)`
/// unavailable, with `gap` (already clamped to 0–365) widening imported
/// blocks only (spec §1.2.4). Callers hold the listing lock.
pub(crate) async fn any_block_overlaps(
    conn: &mut PgConnection,
    listing_id: Uuid,
    date_from: NaiveDate,
    date_to: NaiveDate,
    gap: i32,
) -> Result<bool> {
    let blocked = sqlx::query_scalar!(
        r#"SELECT EXISTS (
            SELECT 1 FROM listing_blocked_dates k
            WHERE k.listing_id = $1
              AND k.date_from < $3::date + $4::int4
              AND k.date_to > $2::date - $4::int4
              AND k.date_from - (CASE WHEN k.source = 'ical_feed' THEN $4::int4 ELSE 0 END) < $3::date
              AND k.date_to + (CASE WHEN k.source = 'ical_feed' THEN $4::int4 ELSE 0 END) > $2::date
        ) AS "blocked!""#,
        listing_id,
        date_from,
        date_to,
        gap
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(blocked)
}

/// Adds a manual block. Under the listing lock, refuses the exact nights of a
/// confirmed stay, live hold or live request (no gap; spec §1.2.3). Blocks may
/// overlap each other; an exact duplicate is `CalendarBlockDuplicate`. The
/// caller has validated `request` with `validate_for`.
#[tracing::instrument(skip(pool, request), fields(listing_id = %listing_id))]
pub async fn create_manual_block(
    pool: &sqlx::PgPool,
    listing_id: Uuid,
    request: &common::calendar::dto::NewBlockedDateRequest,
    created_by: Option<Uuid>,
) -> Result<common::calendar::dto::BlockedDateResponse> {
    insert_manual_block(pool, listing_id, request, created_by)
        .await
        .map_err(|e| match e {
            crate::error::DbError::Sqlx(inner) => {
                crate::error::DbError::from_sqlx_contended(inner, "manual block")
            }
            other => other,
        })
}

async fn insert_manual_block(
    pool: &sqlx::PgPool,
    listing_id: Uuid,
    request: &common::calendar::dto::NewBlockedDateRequest,
    created_by: Option<Uuid>,
) -> Result<common::calendar::dto::BlockedDateResponse> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *tx)
        .await?;
    super::feeds::lock_listing(&mut tx, listing_id).await?;
    let taken = sqlx::query_scalar!(
        r#"SELECT (
            EXISTS (
                SELECT 1 FROM booking b
                WHERE b.listing_id = $1
                  AND (b.status = 'confirmed'
                       OR (b.status = 'pending' AND b.expires_at > clock_timestamp()))
                  AND b.date_from < $3 AND b.date_to > $2
            )
            OR EXISTS (
                SELECT 1 FROM booking_request r
                WHERE r.listing_id = $1
                  AND r.status IN ('open', 'accepted', 'quoted')
                  AND r.expires_at > clock_timestamp()
                  AND r.date_from < $3 AND r.date_to > $2
            )
        ) AS "taken!""#,
        listing_id,
        request.date_from,
        request.date_to
    )
    .fetch_one(&mut *tx)
    .await?;
    if taken {
        return Err(crate::error::DbError::DateUnavailable);
    }
    let note = request
        .note
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty());
    let row = sqlx::query!(
        r#"INSERT INTO listing_blocked_dates (listing_id, source, date_from, date_to, note, created_by)
        VALUES ($1, 'manual', $2, $3, $4, $5)
        RETURNING id, created_at"#,
        listing_id,
        request.date_from,
        request.date_to,
        note,
        created_by
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match crate::error::sqlstate(&e).as_deref() {
        Some(crate::error::SQLSTATE_UNIQUE_VIOLATION) => {
            crate::error::DbError::CalendarBlockDuplicate
        }
        _ => crate::error::DbError::Sqlx(e),
    })?;
    tx.commit().await?;
    Ok(common::calendar::dto::BlockedDateResponse {
        id: row.id,
        listing_id,
        source: common::calendar::BlockSource::Manual,
        feed_id: None,
        feed_name: None,
        feed_active: None,
        date_from: request.date_from,
        date_to: request.date_to,
        note: note.map(str::to_string),
        created_at: row.created_at,
    })
}

/// Deletes a listing's manual block. `false` if it isn't the listing's;
/// `CalendarBlockManagedByFeed` for an imported block.
#[tracing::instrument(skip(pool))]
pub async fn delete_manual_block(
    pool: &sqlx::PgPool,
    listing_id: Uuid,
    block_id: Uuid,
) -> Result<bool> {
    let source = sqlx::query_scalar!(
        r#"SELECT source AS "source: common::calendar::BlockSource"
        FROM listing_blocked_dates WHERE id = $1 AND listing_id = $2"#,
        block_id,
        listing_id
    )
    .fetch_optional(pool)
    .await?;
    match source {
        None => Ok(false),
        Some(common::calendar::BlockSource::IcalFeed) => {
            Err(crate::error::DbError::CalendarBlockManagedByFeed)
        }
        Some(common::calendar::BlockSource::Manual) => {
            let deleted = sqlx::query!(
                "DELETE FROM listing_blocked_dates
                 WHERE id = $1 AND listing_id = $2 AND source = 'manual'",
                block_id,
                listing_id
            )
            .execute(pool)
            .await?;
            Ok(deleted.rows_affected() == 1)
        }
    }
}

/// A listing's blocks touching `[from, to)`, with their feed's name and
/// whether it's still synced, ordered by start date.
#[tracing::instrument(skip(pool))]
pub async fn list_blocks(
    pool: &sqlx::PgPool,
    listing_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<common::calendar::dto::BlockedDateResponse>> {
    Ok(sqlx::query_as!(
        common::calendar::dto::BlockedDateResponse,
        r#"SELECT k.id, k.listing_id, k.source AS "source: common::calendar::BlockSource",
            k.feed_id, f.name AS "feed_name?", f.is_active AS "feed_active?",
            k.date_from, k.date_to, k.note, k.created_at
        FROM listing_blocked_dates k
        LEFT JOIN listing_calendar_feed f ON f.id = k.feed_id
        WHERE k.listing_id = $1 AND k.date_to > $2 AND k.date_from < $3
        ORDER BY k.date_from, k.date_to, k.id"#,
        listing_id,
        from,
        to
    )
    .fetch_all(pool)
    .await?)
}
