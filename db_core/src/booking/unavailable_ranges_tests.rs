//! Unavailable-range tests and shared booking fixtures.

use super::{create_booking, get_unavailable_ranges};
use crate::models::{Booking, CancellationPolicy, NewBooking};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sqlx::PgPool;
use sqlx_db_tester::TestPg;
use std::path::Path;
use uuid::Uuid;

pub(super) fn day(m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2030, m, d).unwrap()
}

pub(super) async fn user(pool: &PgPool, role: crate::models::UserRole) -> Uuid {
    let id = Uuid::now_v7();
    let new_user = crate::models::NewUser {
        id,
        email: format!("u_{id}@example.com"),
        password_hash: "hash".to_string(),
        first_name: "Test".to_string(),
        last_name: "User".to_string(),
        phone_number: None,
        is_active: true,
        is_verified: true,
        verification_code: None,
        verification_code_expires_at: None,
        attributes: serde_json::json!({}),
        roles: Some(vec![role]),
        default_currency: "USD".to_string(),
    };
    crate::user::create_user(pool, &new_user).await.unwrap();
    id
}

pub(super) async fn host(pool: &PgPool) -> Uuid {
    let host_id = user(pool, crate::models::UserRole::Host).await;
    let profile = crate::models::NewHostProfile {
        description: None,
        verified_status: Some("pending".to_string()),
        payout_details: None,
    };
    crate::user::create_host_profile(pool, host_id, &profile)
        .await
        .unwrap();
    host_id
}

pub(super) async fn listing(pool: &PgPool, host_id: Uuid) -> Uuid {
    let new_listing = crate::models::NewListing {
        name: format!("Villa {}", Uuid::now_v7()),
        user_id: host_id,
        description: None,
        listing_structure_id: 1,
        country: "Jamaica".to_string(),
        price_per_night: Some(dec!(200.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 4,
        bedrooms: 2,
        beds: 2,
        full_bathrooms: 1,
        half_bathrooms: 0,
        square_meters: None,
        latitude: None,
        longitude: None,
        listing_details: None,
        city: None,
        base_currency: "USD".to_string(),
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: Some(dec!(0.1000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
    };
    crate::listing::create_listing(pool, &new_listing)
        .await
        .unwrap()
        .id
}

/// Hold window used by fixtures that don't test the window itself.
pub(super) const HOLD_MINUTES: i32 = 120;

/// Each booking gets its own guest: a new hold cancels the guest's
/// other pending holds.
pub(super) async fn book(
    pool: &PgPool,
    listing_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> Booking {
    book_held(pool, listing_id, from, to, HOLD_MINUTES).await
}

/// Like [`book`], with an explicit hold window in minutes.
pub(super) async fn book_held(
    pool: &PgPool,
    listing_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    hold_minutes: i32,
) -> Booking {
    let guest_id = user(pool, crate::models::UserRole::Booker).await;
    let new_booking = NewBooking {
        // UUIDv7 starts with a timestamp; take the random tail.
        confirmation_code: Uuid::now_v7().simple().to_string()[22..].to_uppercase(),
        guest_id,
        listing_id,
        date_from: from,
        date_to: to,
        currency: "USD".to_string(),
        daily_rate: dec!(200.00),
        number_of_persons: 2,
        total_days: (to - from).num_days() as i32,
        sub_total_price: dec!(400.00),
        discount_value: None,
        tax_value: None,
        fee_breakdown: vec![],
        total_price: dec!(400.00),
        cancellation_policy: CancellationPolicy::Flexible,
        metadata: crate::models::BookingMetadata {
            num_adults: 2,
            ..Default::default()
        },
        door_access_code: None,
        cleaning_fee: Decimal::ZERO,
        damage_deposit: Decimal::ZERO,
        platform_fee: Decimal::ZERO,
        tax_rate: dec!(0.1000),
        exchange_rate: Decimal::ONE,
        promo: None,
    };
    create_booking(pool, &new_booking, hold_minutes)
        .await
        .unwrap()
}

#[tokio::test]
async fn returns_only_active_overlapping_ranges_in_order() {
    dotenvy::dotenv().ok();
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".into());
    let test_db = TestPg::new(db_url, Path::new("migrations"));
    let pool = test_db.get_pool().await;

    let host = host(&pool).await;
    let villa = listing(&pool, host).await;
    let other_villa = listing(&pool, host).await;

    let late = book(&pool, villa, day(3, 20), day(3, 25)).await;
    let early = book(&pool, villa, day(3, 1), day(3, 4)).await;
    let cancelled = book(&pool, villa, day(3, 10), day(3, 12)).await;
    sqlx::query!(
        "UPDATE booking SET status = 'cancelled' WHERE id = $1",
        cancelled.id
    )
    .execute(&pool)
    .await
    .unwrap();
    // Outside the window, and on another listing: both excluded.
    book(&pool, villa, day(6, 1), day(6, 3)).await;
    book(&pool, other_villa, day(3, 5), day(3, 8)).await;

    let ranges = get_unavailable_ranges(&pool, villa, day(3, 1), day(4, 1))
        .await
        .unwrap();
    let got: Vec<_> = ranges.iter().map(|r| (r.date_from, r.date_to)).collect();
    assert_eq!(
        got,
        vec![
            (early.date_from, early.date_to),
            (late.date_from, late.date_to)
        ]
    );
}
