use crate::error::Result;
use crate::models::{Booking, NewBooking, UpdatedBooking, BookingStatus, CancellationPolicy, FeeItem};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;
use sqlx::types::Json;
use chrono::Utc;

/// Creates a new booking in the database.
pub async fn create_booking<'e, E>(executor: E, new_booking: &NewBooking) -> Result<Booking>
where
    E: PgExecutor<'e>,
{
    let booking = sqlx::query_as!(
        Booking,
        r#"
        INSERT INTO booking (
            id, confirmation_code, guest_id, listing_id, status,
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown,
            total_price, cancellation_policy, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
        RETURNING id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus", 
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy", 
            created_at, updated_at
        "#,
        Uuid::new_v4(),
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
        Utc::now(),
        Utc::now()
    )
    .fetch_one(executor)
    .await?;

    Ok(booking)
}

/// Retrieves all bookings with pagination.
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
pub async fn update_booking(
    pool: &PgPool,
    id: Uuid,
    updated_booking: &UpdatedBooking,
) -> Result<Booking> {
    
    // Check existence first or let standard update fail if not found?
    // Listing API uses transaction and SELECT FOR UPDATE, then COALESCE.
    // I'll follow that pattern.
    
    let mut tx = pool.begin().await?;

    let _current = sqlx::query!(
        r#"SELECT id FROM booking WHERE id = $1 FOR UPDATE"#,
        id
    )
    .fetch_one(&mut *tx)
    .await?;

    let booking = sqlx::query_as!(
        Booking,
        r#"
        UPDATE booking
        SET status = COALESCE($1, status), updated_at = $2
        WHERE id = $3
        RETURNING id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus", 
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy", 
            created_at, updated_at
        "#,
        updated_booking.status as Option<BookingStatus>,
        Utc::now(),
        id
    )
    .fetch_one(&mut *tx)
    .await?;
    
    tx.commit().await?;

    Ok(booking)
}

/// Deletes a booking (Hard Delete).
pub async fn delete_booking(pool: &PgPool, id: Uuid) -> Result<()> {
    let result = sqlx::query!("DELETE FROM booking WHERE id = $1", id)
        .execute(pool)
        .await?;
    
    if result.rows_affected() == 0 {
        return Err(crate::error::DbError::Sqlx(sqlx::Error::RowNotFound));
    }
    
    Ok(())
}

/// Retrieves bookings for a specific user, sorted by date_from ASC.
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
