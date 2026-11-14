//! Read-only booking lookups.

use crate::error::Result;
use crate::models::{Booking, BookingStatus, CancellationPolicy, FeeItem};
use chrono::NaiveDate;
use common::location::ListingLocation;
use sqlx::PgExecutor;
use sqlx::types::Json;
use std::collections::HashMap;
use uuid::Uuid;

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
            door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            promo_code_id, promo_terms as "promo_terms: Json<common::promo::PromoTerms>",
            promo_stay_discount, promo_cleaning_discount,
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

/// Retrieves bookings on listings hosted by `host_id`, newest first.
#[tracing::instrument(skip(executor))]
pub async fn get_bookings_by_host_id<'e, E>(
    executor: E,
    host_id: Uuid,
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
        SELECT b.id, b.confirmation_code, b.guest_id, b.listing_id, b.status as "status: BookingStatus",
            b.date_from, b.date_to, b.currency, b.daily_rate, b.number_of_persons, b.total_days,
            b.sub_total_price, b.discount_value, b.tax_value, b.fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            b.total_price, b.cancellation_policy as "cancellation_policy: CancellationPolicy",
            b.metadata as "metadata: Json<crate::models::BookingMetadata>",
            b.door_access_code,
            b.cleaning_fee, b.damage_deposit, b.platform_fee, b.tax_rate, b.exchange_rate,
            b.promo_code_id, b.promo_terms as "promo_terms: Json<common::promo::PromoTerms>",
            b.promo_stay_discount, b.promo_cleaning_discount,
            b.created_at, b.updated_at
        FROM booking b
        JOIN listing l ON l.id = b.listing_id
        WHERE l.user_id = $1
        ORDER BY b.created_at DESC
        LIMIT $2 OFFSET $3
        "#,
        host_id,
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
            door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            promo_code_id, promo_terms as "promo_terms: Json<common::promo::PromoTerms>",
            promo_stay_discount, promo_cleaning_discount,
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

/// Retrieves all confirmed bookings whose checkout date has passed (`date_to <= cutoff_date`).
#[tracing::instrument(skip(executor))]
pub async fn get_confirmed_bookings_past_checkout<'e, E>(
    executor: E,
    cutoff_date: NaiveDate,
) -> Result<Vec<Booking>>
where
    E: PgExecutor<'e>,
{
    let records = sqlx::query_as!(
        Booking,
        r#"
        SELECT id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus",
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy",
            metadata as "metadata: Json<crate::models::BookingMetadata>",
            door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            promo_code_id, promo_terms as "promo_terms: Json<common::promo::PromoTerms>",
            promo_stay_discount, promo_cleaning_discount,
            created_at, updated_at
        FROM booking
        WHERE status = 'confirmed'::booking_status
          AND date_to <= $1
        ORDER BY date_to ASC, id ASC
        "#,
        cutoff_date
    )
    .fetch_all(executor)
    .await?;

    Ok(records)
}

/// Retrieves bookings for a specific user, sorted by date_from ASC.
#[tracing::instrument(skip(executor))]
pub async fn get_bookings_by_user_id<'e, E>(
    executor: E,
    guest_id: Uuid,
    page: u32,
    per_page: u32,
) -> Result<Vec<crate::models::BookingWithEligibility>>
where
    E: PgExecutor<'e>,
{
    let limit = per_page as i64;
    let offset = ((page.max(1) - 1) * per_page) as i64;

    let records = sqlx::query!(
        r#"
        SELECT b.id, b.confirmation_code, b.guest_id, b.listing_id, b.status as "status: BookingStatus", 
            b.date_from, b.date_to, b.currency, b.daily_rate, b.number_of_persons, b.total_days,
            b.sub_total_price, b.discount_value, b.tax_value, b.fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            b.total_price, b.cancellation_policy as "cancellation_policy: CancellationPolicy", 
            b.metadata as "metadata: Json<crate::models::BookingMetadata>",
            json_build_object(
                'booking_id', b.id,
                'is_eligible', 
                    CASE 
                        WHEN b.status = 'cancelled' THEN false
                        WHEN r.id IS NOT NULL THEN false
                        WHEN CURRENT_DATE < b.date_to THEN false
                        WHEN CURRENT_DATE > (b.date_to + 15) THEN false
                        ELSE true
                    END,
                'token', rt.token,
                'has_reviewed', CASE WHEN r.id IS NOT NULL THEN true ELSE false END,
                'days_remaining', 
                    CASE 
                        WHEN CURRENT_DATE > (b.date_to + 15) THEN 0
                        ELSE (b.date_to + 15) - CURRENT_DATE
                    END,
                'status_message', 
                    CASE 
                        WHEN b.status = 'cancelled' THEN 'Cancelled bookings are not eligible for review'
                        WHEN r.id IS NOT NULL THEN 'You have already submitted a review for this stay'
                        WHEN CURRENT_DATE < b.date_to THEN 'Reviews can only be submitted after your stay has ended'
                        WHEN CURRENT_DATE > (b.date_to + 15) THEN 'The 15-day review period for this stay has expired'
                        ELSE 'Eligible for review'
                    END
            ) as "review_eligibility!",
            b.created_at, b.updated_at,
            b.door_access_code,
            b.cleaning_fee, b.damage_deposit, b.platform_fee, b.tax_rate, b.exchange_rate,
            b.promo_code_id, b.promo_terms as "promo_terms: Json<common::promo::PromoTerms>",
            b.promo_stay_discount, b.promo_cleaning_discount
        FROM booking b
        LEFT JOIN review r ON r.booking_id = b.id
        LEFT JOIN LATERAL (
            SELECT token FROM review_token 
            WHERE booking_id = b.id AND used_at IS NULL AND expires_at > NOW() 
            ORDER BY created_at DESC LIMIT 1
        ) rt ON true
        WHERE b.guest_id = $1
        ORDER BY b.date_from ASC
        LIMIT $2 OFFSET $3
        "#,
        guest_id,
        limit,
        offset
    )
    .fetch_all(executor)
    .await?;

    let mut bookings = Vec::with_capacity(records.len());
    for r in records {
        let booking = Booking {
            id: r.id,
            confirmation_code: r.confirmation_code,
            guest_id: r.guest_id,
            listing_id: r.listing_id,
            status: r.status,
            date_from: r.date_from,
            date_to: r.date_to,
            currency: r.currency,
            daily_rate: r.daily_rate,
            number_of_persons: r.number_of_persons,
            total_days: r.total_days,
            sub_total_price: r.sub_total_price,
            discount_value: r.discount_value,
            tax_value: r.tax_value,
            fee_breakdown: r.fee_breakdown,
            total_price: r.total_price,
            cancellation_policy: r.cancellation_policy,
            metadata: r.metadata,
            door_access_code: r.door_access_code,
            cleaning_fee: r.cleaning_fee,
            damage_deposit: r.damage_deposit,
            platform_fee: r.platform_fee,
            tax_rate: r.tax_rate,
            exchange_rate: r.exchange_rate,
            promo_code_id: r.promo_code_id,
            promo_terms: r.promo_terms,
            promo_stay_discount: r.promo_stay_discount,
            promo_cleaning_discount: r.promo_cleaning_discount,
            created_at: r.created_at,
            updated_at: r.updated_at,
        };

        let eligibility = serde_json::from_value::<common::models::BookingReviewEligibility>(
            r.review_eligibility,
        )
        .ok();

        bookings.push(crate::models::BookingWithEligibility {
            booking,
            review_eligibility: eligibility,
        });
    }

    Ok(bookings)
}

/// Retrieves bookings for a specific listing, sorted by date_from DESC, created_at DESC.
#[tracing::instrument(skip(executor))]
pub async fn get_bookings_by_listing_id<'e, E>(
    executor: E,
    listing_id: Uuid,
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
            door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            promo_code_id, promo_terms as "promo_terms: Json<common::promo::PromoTerms>",
            promo_stay_discount, promo_cleaning_discount,
            created_at, updated_at
        FROM booking
        WHERE listing_id = $1
        ORDER BY date_from DESC, created_at DESC
        LIMIT $2 OFFSET $3
        "#,
        listing_id,
        limit,
        offset
    )
    .fetch_all(executor)
    .await?;

    Ok(bookings)
}

/// The stored point of each booking's listing, keyed by booking ID, for the
/// bookings whose guest may see it (spec 103 §1.5): only `confirmed` and
/// `completed` ones, checked in this same read so a status change between
/// two reads can't reveal it. The statuses are an allowlist, so a status
/// added later stays excluded. Soft-deleted listings count: the guest may be
/// on their way. A listing missing either coordinate gives nothing.
///
/// One query for any number of bookings, and none for an empty slice.
#[tracing::instrument(skip_all, fields(bookings = booking_ids.len()))]
pub async fn listing_locations<'e, E>(
    executor: E,
    booking_ids: &[Uuid],
) -> Result<HashMap<Uuid, ListingLocation>>
where
    E: PgExecutor<'e>,
{
    if booking_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query!(
        r#"
        SELECT b.id, l.latitude AS "latitude!", l.longitude AS "longitude!"
        FROM booking b
        JOIN listing l ON l.id = b.listing_id
        WHERE b.id = ANY($1)
          AND b.status IN ('confirmed', 'completed')
          AND l.latitude IS NOT NULL
          AND l.longitude IS NOT NULL
        "#,
        booking_ids
    )
    .fetch_all(executor)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.id,
                ListingLocation {
                    latitude: r.latitude,
                    longitude: r.longitude,
                },
            )
        })
        .collect())
}
