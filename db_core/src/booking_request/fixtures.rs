//! Shared fixtures for enquiry and request tests.

use super::NewRequest;
use crate::models::{BookingMetadata, NewBooking};
pub(crate) use crate::promo_tests::{migrated_db, raw_listing, raw_user};
use chrono::{Duration, NaiveDate, Utc};
use common::booking_request::StayPriceSnapshot;
use common::cancellation::CancellationPolicy;
use rust_decimal_macros::dec;
use sqlx::PgPool;
use uuid::Uuid;

/// A night in 2030.
pub(crate) fn day(m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2030, m, d).expect("a 2030 date")
}

/// A listing in `mode` with a `gap`-night gap, and its host.
pub(crate) async fn listing_in_mode(pool: &PgPool, mode: &str, gap: i32) -> (Uuid, Uuid) {
    let host = raw_user(pool).await;
    let villa = raw_listing(pool, host).await;
    sqlx::query(
        "UPDATE listing SET booking_mode = $1::booking_mode, days_between_bookings = $2,
             price_per_night = 100 WHERE id = $3",
    )
    .bind(mode)
    .bind(gap)
    .bind(villa)
    .execute(pool)
    .await
    .expect("set booking mode");
    (villa, host)
}

/// A plain USD snapshot.
pub(crate) fn snapshot() -> StayPriceSnapshot {
    StayPriceSnapshot {
        currency: "USD".to_string(),
        exchange_rate: dec!(1),
        daily_rate: dec!(100.00),
        total_days: 3,
        sub_total_price: dec!(300.00),
        discount_value: None,
        discounted_subtotal: dec!(300.00),
        cleaning_fee: dec!(0.00),
        damage_deposit: dec!(0.00),
        platform_fee: dec!(15.00),
        tax_rate: dec!(0.15),
        tax_value: Some(dec!(45.00)),
        fee_breakdown: vec![],
        total_price: dec!(360.00),
        cancellation_policy: CancellationPolicy::Flexible,
        promo: None,
    }
}

/// A request for `[from, to)` whose window ends in a day.
pub(crate) fn new_request(
    listing: Uuid,
    guest: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> NewRequest {
    NewRequest {
        listing_id: listing,
        guest_id: guest,
        enquiry_id: None,
        date_from: from,
        date_to: to,
        number_of_persons: 2,
        snapshot: snapshot(),
        metadata: BookingMetadata {
            num_adults: 2,
            ..BookingMetadata::default()
        },
        expires_at: Utc::now() + Duration::hours(24),
    }
}

/// A booking row in `status`, holding for `hold_minutes` from now (negative
/// for an already lapsed hold).
pub(crate) async fn raw_stay(
    pool: &PgPool,
    guest: Uuid,
    listing: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    status: &str,
    hold_minutes: i32,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO booking (confirmation_code, guest_id, listing_id, date_from, date_to, daily_rate,
             number_of_persons, total_days, sub_total_price, total_price, cancellation_policy,
             status, expires_at)
         VALUES (upper(substr(md5(random()::text), 1, 10)), $1, $2, $3, $4, 100, 1, $4 - $3, 100,
             100, 'flexible', $5::booking_status, NOW() + make_interval(mins => $6))
         RETURNING id",
    )
    .bind(guest)
    .bind(listing)
    .bind(from)
    .bind(to)
    .bind(status)
    .bind(hold_minutes)
    .fetch_one(pool)
    .await
    .expect("insert booking")
}

/// Moves a request's window into the past without running any sweep.
pub(crate) async fn lapse_request(pool: &PgPool, request: Uuid) {
    sqlx::query(
        "UPDATE booking_request SET expires_at = NOW() - INTERVAL '1 minute' WHERE id = $1",
    )
    .bind(request)
    .execute(pool)
    .await
    .expect("lapse request");
}

/// A request's status as stored.
pub(crate) async fn request_status(pool: &PgPool, request: Uuid) -> String {
    sqlx::query_scalar("SELECT status::text FROM booking_request WHERE id = $1")
        .bind(request)
        .fetch_one(pool)
        .await
        .expect("request status")
}

/// The `(from_status, to_status, actor_id)` history of a request, in order.
pub(crate) async fn request_history(
    pool: &PgPool,
    request: Uuid,
) -> Vec<(Option<String>, String, Option<Uuid>)> {
    sqlx::query_as(
        "SELECT from_status, to_status, actor_id FROM booking_request_history
         WHERE request_id = $1 ORDER BY created_at, id",
    )
    .bind(request)
    .fetch_all(pool)
    .await
    .expect("request history")
}

/// An instant hold for `[from, to)` by `guest`.
pub(crate) fn new_hold(listing: Uuid, guest: Uuid, from: NaiveDate, to: NaiveDate) -> NewBooking {
    NewBooking {
        confirmation_code: format!("H{}", Uuid::new_v4().simple())[..10].to_uppercase(),
        guest_id: guest,
        listing_id: listing,
        date_from: from,
        date_to: to,
        currency: "USD".to_string(),
        daily_rate: dec!(100.00),
        number_of_persons: 2,
        total_days: i32::try_from((to - from).num_days()).unwrap_or(1),
        sub_total_price: dec!(300.00),
        discount_value: None,
        tax_value: Some(dec!(45.00)),
        fee_breakdown: vec![],
        total_price: dec!(360.00),
        cancellation_policy: CancellationPolicy::Flexible,
        metadata: BookingMetadata::default(),
        door_access_code: None,
        cleaning_fee: dec!(0),
        damage_deposit: dec!(0),
        platform_fee: dec!(15.00),
        tax_rate: dec!(0.15),
        exchange_rate: dec!(1),
        promo: None,
    }
}

/// An open enquiry by `guest` on `listing`.
pub(crate) async fn raw_enquiry(pool: &PgPool, listing: Uuid, guest: Uuid) -> Uuid {
    sqlx::query_scalar(
        "WITH conv AS (
             INSERT INTO conversation (listing_id, guest_id) VALUES ($1, $2)
             ON CONFLICT (listing_id, guest_id) DO UPDATE SET last_message_at = NOW()
             RETURNING id
         )
         INSERT INTO booking_enquiry (conversation_id, listing_id, guest_id, date_from, date_to,
             number_of_persons, currency)
         SELECT conv.id, $1, $2, '2030-03-01', '2030-03-04', 2, 'USD' FROM conv
         RETURNING id",
    )
    .bind(listing)
    .bind(guest)
    .fetch_one(pool)
    .await
    .expect("insert enquiry")
}
