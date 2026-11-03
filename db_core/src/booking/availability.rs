//! Date availability checks against confirmed bookings and live holds.
//!
//! A booking blocks a stay when `date_from - gap < check_out` and
//! `date_to + gap > check_in`, where `gap` is the listing's
//! `days_between_bookings` limited to 0–365. The limit stops a stored value
//! from overflowing a date or shrinking the rule. Search and the hold have
//! their own copies of this SQL, and `availability_parity_tests` keeps them
//! in step.

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

    let overlapping = sqlx::query!(
        r#"
        SELECT count(*) as count
        FROM booking b
        JOIN listing l ON l.id = b.listing_id
        WHERE b.listing_id = $1
          AND (b.status = 'confirmed' OR (b.status = 'pending' AND b.expires_at > NOW()))
          AND b.date_from - LEAST(GREATEST(l.days_between_bookings, 0), 365) < $3
          AND b.date_to + LEAST(GREATEST(l.days_between_bookings, 0), 365) > $2
        "#,
        listing_id,
        date_from,
        date_to
    )
    .fetch_one(executor)
    .await?;

    Ok(overlapping.count == Some(0))
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

    let overlapping = sqlx::query!(
        r#"
        SELECT count(*) as count
        FROM booking b
        JOIN listing l ON l.id = b.listing_id
        WHERE b.listing_id = $1
          AND (b.status = 'confirmed' OR (b.status = 'pending' AND b.expires_at > NOW()))
          AND b.date_from - LEAST(GREATEST(l.days_between_bookings, 0), 365) < $3
          AND b.date_to + LEAST(GREATEST(l.days_between_bookings, 0), 365) > $2
          AND b.id != $4
        "#,
        listing_id,
        date_from,
        date_to,
        exclude_booking_id
    )
    .fetch_one(executor)
    .await?;

    Ok(overlapping.count == Some(0))
}

/// Returns the date ranges a new stay can't use that overlap
/// `[date_from, date_to)` for a listing: pending holds and confirmed stays,
/// each widened by the listing's gap, ordered by start date.
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
        SELECT
            b.date_from - LEAST(GREATEST(l.days_between_bookings, 0), 365) AS "date_from!",
            b.date_to + LEAST(GREATEST(l.days_between_bookings, 0), 365) AS "date_to!"
        FROM booking b
        JOIN listing l ON l.id = b.listing_id
        WHERE b.listing_id = $1
          AND (b.status = 'confirmed' OR (b.status = 'pending' AND b.expires_at > NOW()))
          AND b.date_from - LEAST(GREATEST(l.days_between_bookings, 0), 365) < $3
          AND b.date_to + LEAST(GREATEST(l.days_between_bookings, 0), 365) > $2
        ORDER BY b.date_from
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
