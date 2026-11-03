//! Immutable booking history snapshots.

use crate::error::Result;
use crate::models::{Booking, BookingHistory, BookingStatus, CancellationPolicy, FeeItem};
use sqlx::PgExecutor;
use sqlx::types::Json;
use uuid::Uuid;

/// Retrieves the history of a specific booking.
#[tracing::instrument(skip(executor))]
pub async fn get_booking_history<'e, E>(
    executor: E,
    booking_id: Uuid,
) -> Result<Vec<BookingHistory>>
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
            door_access_code,
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

pub(crate) async fn record_booking_history(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    booking: &Booking,
    change_reason: &str,
    changed_by_id: Option<Uuid>,
) -> Result<Uuid> {
    let history_id = sqlx::query_scalar!(
        r#"
        INSERT INTO booking_history (
            booking_id, confirmation_code, guest_id, listing_id, status,
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown,
            total_price, cancellation_policy, metadata, door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            change_reason, changed_by_id,
            promo_code_id, promo_terms, promo_stay_discount, promo_cleaning_discount
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30)
        RETURNING id
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
        booking.door_access_code.as_deref(),
        booking.cleaning_fee,
        booking.damage_deposit,
        booking.platform_fee,
        booking.tax_rate,
        booking.exchange_rate,
        change_reason,
        changed_by_id,
        booking.promo_code_id,
        booking.promo_terms.as_ref().map(|t| Json(&t.0)) as _,
        booking.promo_stay_discount,
        booking.promo_cleaning_discount
    )
    .fetch_one(&mut **tx)
    .await?;

    Ok(history_id)
}
