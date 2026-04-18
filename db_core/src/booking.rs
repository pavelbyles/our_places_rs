use crate::error::Result;
use crate::models::{
    Booking, BookingHistory, BookingStatus, CancellationPolicy, FeeItem, NewBooking,
    UpdatedBooking,
};
use chrono::Utc;
use sqlx::types::Json;

use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

/// Creates a new booking in the database.
#[tracing::instrument(skip(pool))]
pub async fn create_booking(pool: &PgPool, new_booking: &NewBooking) -> Result<Booking> {
    let mut tx = pool.begin().await?;

    let _listing = sqlx::query!(
        "SELECT id FROM listing WHERE id = $1 FOR UPDATE",
        new_booking.listing_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(crate::error::DbError::Sqlx(sqlx::Error::RowNotFound))?;

    let overlapping = sqlx::query!(
        r#"
        SELECT id FROM booking 
        WHERE listing_id = $1 
          AND status IN ('pending', 'confirmed') 
          AND date_from < $3 
          AND date_to > $2
        LIMIT 1
        "#,
        new_booking.listing_id,
        new_booking.date_from,
        new_booking.date_to
    )
    .fetch_optional(&mut *tx)
    .await?;

    if overlapping.is_some() {
        return Err(crate::error::DbError::ValidationError(
            "Listing is not available for the selected dates".to_string(),
        ));
    }

    let booking = sqlx::query_as!(
        Booking,
        r#"
        INSERT INTO booking (
            id, confirmation_code, guest_id, listing_id, status,
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown,
            total_price, cancellation_policy, metadata, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
        RETURNING id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus", 
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy", 
            metadata as "metadata: Json<crate::models::BookingMetadata>",
            created_at, updated_at
        "#,
        Uuid::now_v7(),
        new_booking.confirmation_code,
        new_booking.guest_id,
        new_booking.listing_id,
        BookingStatus::Pending as BookingStatus,
        new_booking.date_from,
        new_booking.date_to,
        new_booking.currency,
        new_booking.daily_rate,
        new_booking.number_of_persons,
        new_booking.total_days,
        new_booking.sub_total_price,
        new_booking.discount_value,
        new_booking.tax_value,
        Json(&new_booking.fee_breakdown) as _,
        new_booking.total_price,
        new_booking.cancellation_policy as CancellationPolicy,
        Json(&new_booking.metadata) as _,
        Utc::now(),
        Utc::now()
    )
    .fetch_one(&mut *tx)
    .await?;

    // Record initial history
    record_booking_history(&mut tx, &booking, "Booking created", None).await?;

    tx.commit().await?;

    Ok(booking)
}

/// Retrieves all bookings with pagination.
#[tracing::instrument(skip(executor))]
pub async fn get_bookings<'e, E>(executor: E, page: u32, per_page: u32) -> Result<Vec<Booking>>
where
    E: PgExecutor<'e>,
{
    let limit = per_page as i64;
    let offset = ((page.max(1) - 1) * per_page) as i64;

    let bookings = sqlx::query_as!(
        Booking,
        r#"
        SELECT id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus", 
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy", 
            metadata as "metadata: Json<crate::models::BookingMetadata>",
            created_at, updated_at
        FROM booking
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2
        "#,
        limit,
        offset
    )
    .fetch_all(executor)
    .await?;

    Ok(bookings)
}

/// Retrieves a single booking by ID.
#[tracing::instrument(skip(executor))]
pub async fn get_booking_by_id<'e, E>(executor: E, id: Uuid) -> Result<Booking>
where
    E: PgExecutor<'e>,
{
    let booking = sqlx::query_as!(
        Booking,
        r#"
        SELECT id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus", 
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy", 
            metadata as "metadata: Json<crate::models::BookingMetadata>",
            created_at, updated_at
        FROM booking
        WHERE id = $1
        "#,
        id
    )
    .fetch_one(executor)
    .await?;

    Ok(booking)
}

/// Updates a booking's status.
#[tracing::instrument(skip(pool))]
pub async fn update_booking(
    pool: &PgPool,
    id: Uuid,
    updated_booking: &UpdatedBooking,
) -> Result<Booking> {
    // Check existence first or let standard update fail if not found?
    // Listing API uses transaction and SELECT FOR UPDATE, then COALESCE.
    // I'll follow that pattern.

    let mut tx = pool.begin().await?;

    let current = sqlx::query!(
        r#"SELECT id, status as "status: BookingStatus" FROM booking WHERE id = $1 FOR UPDATE"#,
        id
    )
    .fetch_one(&mut *tx)
    .await?;

    let booking = sqlx::query_as!(
        Booking,
        r#"
        UPDATE booking
        SET status = COALESCE($1, status), 
            metadata = COALESCE($2, metadata),
            updated_at = $3
        WHERE id = $4
        RETURNING id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus", 
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy", 
            metadata as "metadata: Json<crate::models::BookingMetadata>",
            created_at, updated_at
        "#,
        updated_booking.status as Option<BookingStatus>,
        updated_booking.metadata.as_ref().map(Json) as _,
        Utc::now(),
        id
    )
    .fetch_one(&mut *tx)
    .await?;

    // Record history if anything changed
    let status_changed = updated_booking.status.map(|s| s != current.status).unwrap_or(false);
    let metadata_changed = updated_booking.metadata.is_some();

    if status_changed || metadata_changed {
        let reason = match (status_changed, metadata_changed) {
            (true, true) => "Status and metadata updated",
            (true, false) => "Status updated",
            (false, true) => "Metadata updated",
            _ => "Booking updated",
        };
        record_booking_history(&mut tx, &booking, reason, None).await?;
    }

    tx.commit().await?;

    Ok(booking)
}

/// Deletes a booking (Hard Delete).
#[tracing::instrument(skip(pool))]
pub async fn delete_booking(pool: &PgPool, id: Uuid) -> Result<()> {
    let result: sqlx::postgres::PgQueryResult =
        sqlx::query!("DELETE FROM booking WHERE id = $1", id)
            .execute(pool)
            .await?;

    if result.rows_affected() == 0 {
        return Err(crate::error::DbError::Sqlx(sqlx::Error::RowNotFound));
    }

    Ok(())
}

/// Retrieves bookings for a specific user, sorted by date_from ASC.
#[tracing::instrument(skip(executor))]
pub async fn get_bookings_by_user_id<'e, E>(
    executor: E,
    guest_id: Uuid,
    page: u32,
    per_page: u32,
) -> Result<Vec<Booking>>
where
    E: PgExecutor<'e>,
{
    let limit = per_page as i64;
    let offset = ((page.max(1) - 1) * per_page) as i64;

    let bookings = sqlx::query_as!(
        Booking,
        r#"
        SELECT id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus", 
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy", 
            metadata as "metadata: Json<crate::models::BookingMetadata>",
            created_at, updated_at
        FROM booking
        WHERE guest_id = $1
        ORDER BY date_from ASC
        LIMIT $2 OFFSET $3
        "#,
        guest_id,
        limit,
        offset
    )
    .fetch_all(executor)
    .await?;

    Ok(bookings)
}

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
          AND status IN ('pending', 'confirmed') 
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

/// Deletes pending bookings that are older than the specified minutes.
#[tracing::instrument(skip(executor))]
pub async fn cleanup_stale_bookings<'e, E>(
    executor: E,
    timeout_minutes: i64,
) -> Result<u64>
where
    E: PgExecutor<'e>,
{
    let threshold = Utc::now() - chrono::Duration::minutes(timeout_minutes);

    let result = sqlx::query!(
        r#"
        DELETE FROM booking
        WHERE status = 'pending'
          AND created_at < $1
        "#,
        threshold
    )
    .execute(executor)
    .await?;

    Ok(result.rows_affected())
}

/// Retrieves the history of a specific booking.
#[tracing::instrument(skip(executor))]
pub async fn get_booking_history<'e, E>(executor: E, booking_id: Uuid) -> Result<Vec<BookingHistory>>
where
    E: PgExecutor<'e>,
{
    let history = sqlx::query_as!(
        BookingHistory,
        r#"
        SELECT id, booking_id, confirmation_code, guest_id, listing_id, 
            status as "status: BookingStatus", date_from, date_to, currency, 
            daily_rate, number_of_persons, total_days, sub_total_price, 
            discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>", 
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy", 
            metadata as "metadata: Json<crate::models::BookingMetadata>", 
            changed_by_id, change_reason, created_at
        FROM booking_history
        WHERE booking_id = $1
        ORDER BY created_at ASC
        "#,
        booking_id
    )
    .fetch_all(executor)
    .await?;

    Ok(history)
}

async fn record_booking_history(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    booking: &Booking,
    change_reason: &str,
    changed_by_id: Option<Uuid>,
) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO booking_history (
            booking_id, confirmation_code, guest_id, listing_id, status,
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown,
            total_price, cancellation_policy, metadata, change_reason, changed_by_id
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
        "#,
        booking.id,
        booking.confirmation_code,
        booking.guest_id,
        booking.listing_id,
        booking.status as BookingStatus,
        booking.date_from,
        booking.date_to,
        booking.currency,
        booking.daily_rate,
        booking.number_of_persons,
        booking.total_days,
        booking.sub_total_price,
        booking.discount_value,
        booking.tax_value,
        Json(&booking.fee_breakdown.0) as _,
        booking.total_price,
        booking.cancellation_policy as CancellationPolicy,
        Json(&booking.metadata.0) as _,
        change_reason,
        changed_by_id
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}
