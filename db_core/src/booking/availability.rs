//! Date availability checks against confirmed bookings and live holds.
//!
//! A booking blocks a stay when `date_from - gap < check_out` and
//! `date_to + gap > check_in`, where `gap` is the listing's
//! `days_between_bookings` limited to 0–365. The limit stops a stored value
//! from overflowing a date or shrinking the rule. Search and the hold have
//! their own copies of this SQL, and `availability_parity_tests` keeps them
//! in step.
//!
//! Live requests to book (#105) block the same way: `status IN ('open',
//! 'accepted', 'quoted') AND expires_at > NOW()`, widened by the same gap.
//!
//! Blocked dates (#58) in `listing_blocked_dates` block a stay when
//! `k.date_from - w < check_out AND k.date_to + w > check_in`, where `w` is
//! the gap for blocks imported from a feed (`source = 'ical_feed'`) and 0 for
//! manual blocks. Every query also adds the bounding predicates
//! `k.date_from < check_out + gap AND k.date_to > check_in - gap`, a superset
//! of the rule, so `idx_blocked_dates_listing_range` can narrow the scan.
//! When an existing booking changes (`check_availability_excluding`), blocks
//! are only tested against nights the booking doesn't already hold, so a
//! conflicted stay can still be shortened or moved off a block.

use crate::error::Result;
use sqlx::PgExecutor;
use uuid::Uuid;

/// Checks if a listing is available for a specified date range.
#[tracing::instrument(skip(executor))]
pub async fn check_availability<'e, E>(
    executor: E,
    listing_id: Uuid,
    date_from: chrono::NaiveDate,
    date_to: chrono::NaiveDate,
) -> Result<bool>
where
    E: PgExecutor<'e>,
{
    if date_from >= date_to {
        return Ok(false);
    }

    let blocked = sqlx::query_scalar!(
        r#"
        SELECT (
            EXISTS (
                SELECT 1 FROM booking b
                JOIN listing l ON l.id = b.listing_id
                WHERE b.listing_id = $1
                  AND (b.status = 'confirmed' OR (b.status = 'pending' AND b.expires_at > NOW()))
                  AND b.date_from - LEAST(GREATEST(l.days_between_bookings, 0), 365) < $3
                  AND b.date_to + LEAST(GREATEST(l.days_between_bookings, 0), 365) > $2
            )
            OR EXISTS (
                SELECT 1 FROM booking_request r
                JOIN listing l ON l.id = r.listing_id
                WHERE r.listing_id = $1
                  AND r.status IN ('open', 'accepted', 'quoted') AND r.expires_at > NOW()
                  AND r.date_from - LEAST(GREATEST(l.days_between_bookings, 0), 365) < $3
                  AND r.date_to + LEAST(GREATEST(l.days_between_bookings, 0), 365) > $2
            )
            OR EXISTS (
                SELECT 1 FROM listing_blocked_dates k
                JOIN listing l ON l.id = k.listing_id
                WHERE k.listing_id = $1
                  AND k.date_from < $3 + LEAST(GREATEST(l.days_between_bookings, 0), 365)
                  AND k.date_to > $2 - LEAST(GREATEST(l.days_between_bookings, 0), 365)
                  AND k.date_from - (CASE WHEN k.source = 'ical_feed'
                        THEN LEAST(GREATEST(l.days_between_bookings, 0), 365) ELSE 0 END) < $3
                  AND k.date_to + (CASE WHEN k.source = 'ical_feed'
                        THEN LEAST(GREATEST(l.days_between_bookings, 0), 365) ELSE 0 END) > $2
            )
        ) AS "blocked!"
        "#,
        listing_id,
        date_from,
        date_to
    )
    .fetch_one(executor)
    .await?;

    Ok(!blocked)
}

/// Checks if a listing is available for a specified date range, excluding a specific booking.
#[tracing::instrument(skip(executor))]
pub async fn check_availability_excluding<'e, E>(
    executor: E,
    listing_id: Uuid,
    date_from: chrono::NaiveDate,
    date_to: chrono::NaiveDate,
    exclude_booking_id: Uuid,
) -> Result<bool>
where
    E: PgExecutor<'e>,
{
    if date_from >= date_to {
        return Ok(false);
    }

    let blocked = sqlx::query_scalar!(
        r#"
        SELECT (
            EXISTS (
                SELECT 1 FROM booking b
                JOIN listing l ON l.id = b.listing_id
                WHERE b.listing_id = $1
                  AND (b.status = 'confirmed' OR (b.status = 'pending' AND b.expires_at > NOW()))
                  AND b.date_from - LEAST(GREATEST(l.days_between_bookings, 0), 365) < $3
                  AND b.date_to + LEAST(GREATEST(l.days_between_bookings, 0), 365) > $2
                  AND b.id != $4
            )
            OR EXISTS (
                SELECT 1 FROM booking_request r
                JOIN listing l ON l.id = r.listing_id
                WHERE r.listing_id = $1
                  AND r.status IN ('open', 'accepted', 'quoted') AND r.expires_at > NOW()
                  AND r.date_from - LEAST(GREATEST(l.days_between_bookings, 0), 365) < $3
                  AND r.date_to + LEAST(GREATEST(l.days_between_bookings, 0), 365) > $2
            )
            OR EXISTS (
                SELECT 1 FROM listing_blocked_dates k
                JOIN listing l ON l.id = k.listing_id
                LEFT JOIN booking old ON old.id = $4 AND old.listing_id = k.listing_id
                CROSS JOIN LATERAL (
                    SELECT
                        LEAST(GREATEST(l.days_between_bookings, 0), 365) AS g,
                        CASE WHEN k.source = 'ical_feed'
                            THEN LEAST(GREATEST(l.days_between_bookings, 0), 365) ELSE 0 END AS w,
                        LEAST($3, COALESCE(old.date_from, $3)) AS head_out,
                        GREATEST($2, COALESCE(old.date_to, $3)) AS tail_in
                ) x
                WHERE k.listing_id = $1
                  AND k.date_from < $3 + x.g
                  AND k.date_to > $2 - x.g
                  AND (
                      ($2 < x.head_out AND k.date_from - x.w < x.head_out AND k.date_to + x.w > $2)
                      OR (x.tail_in < $3 AND k.date_from - x.w < $3 AND k.date_to + x.w > x.tail_in)
                  )
            )
        ) AS "blocked!"
        "#,
        listing_id,
        date_from,
        date_to,
        exclude_booking_id
    )
    .fetch_one(executor)
    .await?;

    Ok(!blocked)
}

/// Returns the date ranges a new stay can't use that overlap
/// `[date_from, date_to)` for a listing: pending holds, confirmed stays, live
/// requests to book and imported blocks, each widened by the listing's gap,
/// plus manual blocks as they are, ordered by start date.
///
/// Uses the same status set and overlap rule as [`check_availability`], so
/// the calendar and the hold logic agree. Read-only and lock-free: it is for
/// display; placing a hold still takes the row lock.
#[tracing::instrument(skip(executor))]
pub async fn get_unavailable_ranges<'e, E>(
    executor: E,
    listing_id: Uuid,
    date_from: chrono::NaiveDate,
    date_to: chrono::NaiveDate,
) -> Result<Vec<common::availability::UnavailableRange>>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query!(
        r#"
        SELECT blocked.date_from AS "date_from!", blocked.date_to AS "date_to!"
        FROM (
            SELECT
                b.date_from - LEAST(GREATEST(l.days_between_bookings, 0), 365) AS date_from,
                b.date_to + LEAST(GREATEST(l.days_between_bookings, 0), 365) AS date_to
            FROM booking b
            JOIN listing l ON l.id = b.listing_id
            WHERE b.listing_id = $1
              AND (b.status = 'confirmed' OR (b.status = 'pending' AND b.expires_at > NOW()))
              AND b.date_from - LEAST(GREATEST(l.days_between_bookings, 0), 365) < $3
              AND b.date_to + LEAST(GREATEST(l.days_between_bookings, 0), 365) > $2
            UNION ALL
            SELECT
                r.date_from - LEAST(GREATEST(l.days_between_bookings, 0), 365),
                r.date_to + LEAST(GREATEST(l.days_between_bookings, 0), 365)
            FROM booking_request r
            JOIN listing l ON l.id = r.listing_id
            WHERE r.listing_id = $1
              AND r.status IN ('open', 'accepted', 'quoted') AND r.expires_at > NOW()
              AND r.date_from - LEAST(GREATEST(l.days_between_bookings, 0), 365) < $3
              AND r.date_to + LEAST(GREATEST(l.days_between_bookings, 0), 365) > $2
            UNION ALL
            SELECT k.date_from - x.w, k.date_to + x.w
            FROM listing_blocked_dates k
            JOIN listing l ON l.id = k.listing_id
            CROSS JOIN LATERAL (
                SELECT
                    LEAST(GREATEST(l.days_between_bookings, 0), 365) AS g,
                    CASE WHEN k.source = 'ical_feed'
                        THEN LEAST(GREATEST(l.days_between_bookings, 0), 365) ELSE 0 END AS w
            ) x
            WHERE k.listing_id = $1
              AND k.date_from < $3 + x.g
              AND k.date_to > $2 - x.g
              AND k.date_from - x.w < $3
              AND k.date_to + x.w > $2
        ) AS blocked
        ORDER BY blocked.date_from
        "#,
        listing_id,
        date_from,
        date_to
    )
    .fetch_all(executor)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| common::availability::UnavailableRange {
            date_from: r.date_from,
            date_to: r.date_to,
        })
        .collect())
}
