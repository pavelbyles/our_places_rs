//! Date availability checks against confirmed bookings and live holds.

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
        SELECT count(*) as count FROM booking 
        WHERE listing_id = $1 
          AND (status = 'confirmed' OR (status = 'pending' AND expires_at > NOW()))
          AND date_from < $3 
          AND date_to > $2
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
        SELECT count(*) as count FROM booking 
        WHERE listing_id = $1 
          AND (status = 'confirmed' OR (status = 'pending' AND expires_at > NOW()))
          AND date_from < $3 
          AND date_to > $2
          AND id != $4
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

/// Returns the booked date ranges (pending holds and confirmed stays) that
/// overlap `[date_from, date_to)` for a listing, ordered by start date.
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
        SELECT date_from, date_to FROM booking
        WHERE listing_id = $1
          AND (status = 'confirmed' OR (status = 'pending' AND expires_at > NOW()))
          AND date_from < $3
          AND date_to > $2
        ORDER BY date_from
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
