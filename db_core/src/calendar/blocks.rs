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
