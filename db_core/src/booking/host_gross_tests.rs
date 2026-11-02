//! Host gross includes the cleaning fee in every future calculation, and a
//! revision keeps to the ledger row's own currency, rate and commission.

use super::unavailable_ranges_tests::{book, day, host, listing};
use super::update_booking;
use crate::models::{Booking, BookingStatus, SupplierInfo, UpdatedBooking};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sqlx::{PgPool, Row};
use sqlx_db_tester::TestPg;
use std::path::Path;
use uuid::Uuid;

fn test_db() -> TestPg {
    dotenvy::dotenv().ok();
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".into());
    TestPg::new(db_url, Path::new("migrations"))
}

fn supplier() -> SupplierInfo {
    SupplierInfo {
        supplier_name: "Our Places Jamaica Limited".to_string(),
        supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
        supplier_trn: "000-111-222".to_string(),
    }
}

/// A confirmed 400.00 stay with a 100.00 cleaning fee at 10% commission.
async fn confirmed_with_cleaning(pool: &PgPool, host_id: Uuid) -> Booking {
    let villa = listing(pool, host_id).await;
    let hold = book(pool, villa, day(4, 1), day(4, 3)).await;
    sqlx::query("UPDATE booking SET cleaning_fee = 100 WHERE id = $1")
        .bind(hold.id)
        .execute(pool)
        .await
        .unwrap();
    update_booking(
        pool,
        hold.id,
        &UpdatedBooking {
            status: Some(BookingStatus::Confirmed),
            ..Default::default()
        },
        &supplier(),
    )
    .await
    .unwrap()
}

/// New terms for the booking: one more night at 600.00, same 100.00 cleaning.
fn longer_stay() -> UpdatedBooking {
    UpdatedBooking {
        date_to: Some(day(4, 4)),
        total_days: Some(3),
        sub_total_price: Some(dec!(600.00)),
        replace_discounts: true,
        ..Default::default()
    }
}

async fn ledger(pool: &PgPool, booking_id: Uuid) -> (String, Decimal, Decimal, Decimal) {
    let row = sqlx::query(
        "SELECT currency, gross_amount, platform_fee_amount, net_payout_amount \
         FROM host_payout_ledger WHERE booking_id = $1",
    )
    .bind(booking_id)
    .fetch_one(pool)
    .await
    .unwrap();
    (
        row.get::<String, _>("currency").trim().to_string(),
        row.get("gross_amount"),
        row.get("platform_fee_amount"),
        row.get("net_payout_amount"),
    )
}

#[tokio::test]
async fn host_gross_includes_the_cleaning_fee_at_confirmation() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let booking = confirmed_with_cleaning(&pool, host(&pool).await).await;

    assert_eq!(
        ledger(&pool, booking.id).await,
        ("USD".to_string(), dec!(500.00), dec!(50.00), dec!(450.00))
    );
}

#[tokio::test]
async fn host_gross_revising_an_older_row_uses_the_new_formula() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let booking = confirmed_with_cleaning(&pool, host(&pool).await).await;
    // A row written before the cleaning fee counted towards gross.
    sqlx::query(
        "UPDATE host_payout_ledger \
         SET gross_amount = 400, platform_fee_amount = 40, net_payout_amount = 360 \
         WHERE booking_id = $1",
    )
    .bind(booking.id)
    .execute(&pool)
    .await
    .unwrap();

    update_booking(&pool, booking.id, &longer_stay(), &supplier())
        .await
        .unwrap();

    assert_eq!(
        ledger(&pool, booking.id).await,
        ("USD".to_string(), dec!(700.00), dec!(70.00), dec!(630.00))
    );
}

#[tokio::test]
async fn host_gross_revision_keeps_the_rows_currency_rate_and_commission() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host_id = host(&pool).await;
    let booking = confirmed_with_cleaning(&pool, host_id).await;
    sqlx::query(r#"UPDATE "user" SET default_currency = 'JMD' WHERE id = $1"#)
        .bind(host_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE listing SET commission_pct = 0.2 WHERE id = $1")
        .bind(booking.listing_id)
        .execute(&pool)
        .await
        .unwrap();

    update_booking(&pool, booking.id, &longer_stay(), &supplier())
        .await
        .unwrap();

    assert_eq!(
        ledger(&pool, booking.id).await,
        ("USD".to_string(), dec!(700.00), dec!(70.00), dec!(630.00))
    );
}

#[tokio::test]
async fn host_gross_revision_with_no_ledger_row_writes_nothing_and_succeeds() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let booking = confirmed_with_cleaning(&pool, host(&pool).await).await;
    sqlx::query("DELETE FROM host_payout_ledger WHERE booking_id = $1")
        .bind(booking.id)
        .execute(&pool)
        .await
        .unwrap();

    update_booking(&pool, booking.id, &longer_stay(), &supplier())
        .await
        .unwrap();

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM host_payout_ledger WHERE booking_id = $1")
            .bind(booking.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn host_gross_retained_payout_stays_accommodation_only() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let booking = confirmed_with_cleaning(&pool, host(&pool).await).await;
    sqlx::query(
        "UPDATE booking SET cancellation_policy = 'moderate', \
         date_from = CURRENT_DATE + 3, date_to = CURRENT_DATE + 5 WHERE id = $1",
    )
    .bind(booking.id)
    .execute(&pool)
    .await
    .unwrap();

    update_booking(
        &pool,
        booking.id,
        &UpdatedBooking {
            status: Some(BookingStatus::Cancelled),
            ..Default::default()
        },
        &supplier(),
    )
    .await
    .unwrap();

    // Half of the 400.00 stay; the cleaning fee went back to the guest.
    assert_eq!(
        ledger(&pool, booking.id).await,
        ("USD".to_string(), dec!(200.00), dec!(20.00), dec!(180.00))
    );
}
