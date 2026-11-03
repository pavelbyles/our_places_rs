//! Pending booking holds: creation with a caller-supplied hold window,
//! expiry sweeps and transfer of a hold to a promoted guest account.

use super::history::record_booking_history;
use crate::error::Result;
use crate::models::{Booking, BookingStatus, CancellationPolicy, FeeItem, NewBooking};
use chrono::Utc;
use rust_decimal::Decimal;
use sqlx::PgPool;
use sqlx::types::Json;
use uuid::Uuid;

/// Creates a `pending` booking that holds its dates for `hold_minutes`.
///
/// The hold window comes from the caller's configuration
/// (`application.pending_hold_minutes` in booking_api), so it can change per
/// deploy without a rebuild.
///
/// A promo code is checked here, under its row lock, after the guest's other
/// holds are cancelled, so a guest replacing their own hold doesn't count against
/// themselves. A refusal rolls everything back, including those cancellations.
#[tracing::instrument(
    skip(pool, new_booking),
    fields(listing_id = %new_booking.listing_id, guest_id = %new_booking.guest_id)
)]
pub async fn create_booking(
    pool: &PgPool,
    new_booking: &NewBooking,
    hold_minutes: i32,
) -> Result<Booking> {
    let mut tx = pool.begin().await?;

    // The gap is read under the lock, so it can't change before this hold commits.
    let listing = sqlx::query!(
        r#"SELECT is_active, LEAST(GREATEST(days_between_bookings, 0), 365) as "gap!"
        FROM listing WHERE id = $1 FOR UPDATE"#,
        new_booking.listing_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(crate::error::DbError::Sqlx(sqlx::Error::RowNotFound))?;
    if !listing.is_active {
        return Err(crate::error::DbError::ListingUnavailable);
    }

    // Lock order: listing, then currency. A concurrent deactivation waits for
    // this hold (and then sees it), or this hold sees the deactivation.
    crate::currency::ensure_active(&mut tx, &new_booking.currency).await?;

    let overlapping = sqlx::query!(
        r#"
        SELECT id, guest_id, status as "status: BookingStatus" FROM booking 
        WHERE listing_id = $1 
          AND (status = 'confirmed' OR (status = 'pending' AND expires_at > NOW()))
          AND date_from - $4::int4 < $3
          AND date_to + $4::int4 > $2
        "#,
        new_booking.listing_id,
        new_booking.date_from,
        new_booking.date_to,
        listing.gap
    )
    .fetch_all(&mut *tx)
    .await?;

    let has_conflict = overlapping.iter().any(|b| {
        b.status == BookingStatus::Confirmed
            || (b.status == BookingStatus::Pending && b.guest_id != new_booking.guest_id)
    });

    if has_conflict {
        return Err(crate::error::DbError::DateUnavailable);
    }

    // Cancel any existing pending holds for this guest across all listings
    let existing_guest_holds = sqlx::query_as!(
        Booking,
        r#"
        UPDATE booking
        SET status = 'cancelled', updated_at = $2
        WHERE guest_id = $1 AND status = 'pending'
        RETURNING id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus", 
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy", 
            metadata as "metadata: Json<crate::models::BookingMetadata>",
            door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            promo_code_id, promo_terms as "promo_terms: Json<common::promo::PromoTerms>",
            promo_stay_discount, promo_cleaning_discount,
            created_at, updated_at
        "#,
        new_booking.guest_id,
        Utc::now()
    )
    .fetch_all(&mut *tx)
    .await?;

    for cancelled in existing_guest_holds {
        record_booking_history(
            &mut tx,
            &cancelled,
            "Cancelled previous pending hold due to new booking placement",
            Some(new_booking.guest_id),
        )
        .await?;
    }

    if let Some(promo) = &new_booking.promo {
        let (code, usage) =
            crate::promo::lock_and_count(&mut tx, promo.promo_code_id, new_booking.guest_id)
                .await?;
        common::promo::check_status(code.status(), Utc::now())
            .and_then(|()| common::promo::check_usage(usage))
            .map_err(crate::error::DbError::PromoRefused)?;
    }

    let promo = new_booking.promo.as_ref();
    let booking = sqlx::query_as!(
        Booking,
        r#"
        INSERT INTO booking (
            id, confirmation_code, guest_id, listing_id, status,
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown,
            total_price, cancellation_policy, metadata, door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            created_at, updated_at, expires_at,
            promo_code_id, promo_terms, promo_stay_discount, promo_cleaning_discount
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26,
            NOW() + make_interval(mins => $27), $28, $29, $30, $31)
        RETURNING id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus", 
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy", 
            metadata as "metadata: Json<crate::models::BookingMetadata>",
            door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            promo_code_id, promo_terms as "promo_terms: Json<common::promo::PromoTerms>",
            promo_stay_discount, promo_cleaning_discount,
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
        new_booking.door_access_code.as_deref(),
        new_booking.cleaning_fee,
        new_booking.damage_deposit,
        new_booking.platform_fee,
        new_booking.tax_rate,
        new_booking.exchange_rate,
        Utc::now(),
        Utc::now(),
        hold_minutes,
        promo.map(|p| p.promo_code_id),
        promo.map(|p| Json(&p.terms)) as _,
        promo.map_or(Decimal::ZERO, |p| p.stay_discount),
        promo.map_or(Decimal::ZERO, |p| p.cleaning_discount)
    )
    .fetch_one(&mut *tx)
    .await?;

    if let Some(promo) = promo {
        sqlx::query!(
            "INSERT INTO promo_code_redemption (promo_code_id, booking_id) VALUES ($1, $2)",
            promo.promo_code_id,
            booking.id
        )
        .execute(&mut *tx)
        .await?;
    }

    // Record initial history
    record_booking_history(&mut tx, &booking, "Booking created", None).await?;

    tx.commit().await?;

    Ok(booking)
}

/// Cancels every `pending` hold whose `expires_at` has passed and records a
/// history snapshot for each. Rows locked by an in-flight transaction are
/// skipped and picked up on the next sweep. Returns the cancelled bookings.
///
/// Availability already ignores expired holds; this only tidies their status,
/// so it is safe to run alongside the pg_cron job that does the same.
#[tracing::instrument(skip(pool))]
pub async fn expire_stale_holds(pool: &PgPool) -> Result<Vec<Booking>> {
    let mut tx = pool.begin().await?;

    let expired = sqlx::query_as!(
        Booking,
        r#"
        WITH due AS (
            SELECT id FROM booking
            WHERE status = 'pending' AND expires_at <= NOW()
            FOR UPDATE SKIP LOCKED
        )
        UPDATE booking b
        SET status = 'cancelled', updated_at = NOW()
        FROM due
        WHERE b.id = due.id
        RETURNING b.id, b.confirmation_code, b.guest_id, b.listing_id, b.status as "status: BookingStatus",
            b.date_from, b.date_to, b.currency, b.daily_rate, b.number_of_persons, b.total_days,
            b.sub_total_price, b.discount_value, b.tax_value, b.fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            b.total_price, b.cancellation_policy as "cancellation_policy: CancellationPolicy",
            b.metadata as "metadata: Json<crate::models::BookingMetadata>",
            b.door_access_code,
            b.cleaning_fee, b.damage_deposit, b.platform_fee, b.tax_rate, b.exchange_rate,
            b.promo_code_id, b.promo_terms as "promo_terms: Json<common::promo::PromoTerms>",
            b.promo_stay_discount, b.promo_cleaning_discount,
            b.created_at, b.updated_at
        "#
    )
    .fetch_all(&mut *tx)
    .await?;

    for booking in &expired {
        record_booking_history(&mut tx, booking, "Hold expired", None).await?;
    }

    tx.commit().await?;

    Ok(expired)
}

/// Transfers a pending booking placeholder to a newly authenticated user.
#[tracing::instrument(skip(pool))]
pub async fn transfer_booking_guest(
    pool: &PgPool,
    booking_id: Uuid,
    new_guest_id: Uuid,
) -> Result<Booking> {
    let mut tx = pool.begin().await?;

    let booking = sqlx::query_as!(
        Booking,
        r#"
        UPDATE booking
        SET guest_id = $1,
            updated_at = $2
        WHERE id = $3 AND status = 'pending'
        RETURNING id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus", 
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy", 
            metadata as "metadata: Json<crate::models::BookingMetadata>",
            door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            promo_code_id, promo_terms as "promo_terms: Json<common::promo::PromoTerms>",
            promo_stay_discount, promo_cleaning_discount,
            created_at, updated_at
        "#,
        new_guest_id,
        Utc::now(),
        booking_id
    )
    .fetch_one(&mut *tx)
    .await?;

    record_booking_history(
        &mut tx,
        &booking,
        "Transferred booking from guest placeholder to authenticated user",
        Some(new_guest_id),
    )
    .await?;

    tx.commit().await?;

    Ok(booking)
}
