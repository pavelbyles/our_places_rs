//! What a host is owed when a confirmed booking is cancelled with part of the
//! payment retained under the cancellation policy.

use super::unavailable_ranges_tests::{book, day, host, listing};
use super::update_booking;
use crate::models::{Booking, BookingStatus, SupplierInfo, UpdatedBooking};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sqlx::{PgPool, Row};
use sqlx_db_tester::TestPg;
use std::path::Path;
use tracing_test::traced_test;
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

fn set_status(status: BookingStatus) -> UpdatedBooking {
    UpdatedBooking {
        status: Some(status),
        ..Default::default()
    }
}

#[derive(Debug, PartialEq)]
struct Row4 {
    status: String,
    currency: String,
    gross: Decimal,
    fee: Decimal,
    net: Decimal,
}

async fn only_ledger_row(pool: &PgPool, booking_id: Uuid) -> Row4 {
    let rows = sqlx::query(
        "SELECT status::text AS status, currency, gross_amount, platform_fee_amount, net_payout_amount \
         FROM host_payout_ledger WHERE booking_id = $1",
    )
    .bind(booking_id)
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1, "a booking has one ledger row");
    Row4 {
        status: rows[0].get("status"),
        currency: rows[0].get::<String, _>("currency").trim().to_string(),
        gross: rows[0].get("gross_amount"),
        fee: rows[0].get("platform_fee_amount"),
        net: rows[0].get("net_payout_amount"),
    }
}

/// A confirmed 400.00 booking whose policy refunds 50% of the accommodation.
async fn confirmed_half_refund_booking(pool: &PgPool, host_id: Uuid) -> Booking {
    let villa = listing(pool, host_id).await;
    let hold = book(pool, villa, day(4, 1), day(4, 3)).await;
    let booking = update_booking(
        pool,
        hold.id,
        &set_status(BookingStatus::Confirmed),
        &supplier(),
    )
    .await
    .unwrap();
    sqlx::query(
        "UPDATE booking SET cancellation_policy = 'moderate', \
         date_from = CURRENT_DATE + 3, date_to = CURRENT_DATE + 5 WHERE id = $1",
    )
    .bind(booking.id)
    .execute(pool)
    .await
    .unwrap();
    booking
}

async fn cancel(pool: &PgPool, id: Uuid) {
    update_booking(pool, id, &set_status(BookingStatus::Cancelled), &supplier())
        .await
        .unwrap();
}

#[tokio::test]
async fn retained_payout_reaches_the_ledger_on_a_partial_refund() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let booking = confirmed_half_refund_booking(&pool, host(&pool).await).await;

    cancel(&pool, booking.id).await;

    assert_eq!(
        only_ledger_row(&pool, booking.id).await,
        Row4 {
            status: "pending".to_string(),
            currency: "USD".to_string(),
            gross: dec!(200.00),
            fee: dec!(20.00),
            net: dec!(180.00),
        }
    );
}

#[tokio::test]
async fn retained_payout_row_stays_cancelled_on_a_full_refund() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = listing(&pool, host(&pool).await).await;
    let hold = book(&pool, villa, day(4, 1), day(4, 3)).await;
    update_booking(
        &pool,
        hold.id,
        &set_status(BookingStatus::Confirmed),
        &supplier(),
    )
    .await
    .unwrap();

    cancel(&pool, hold.id).await;

    assert_eq!(only_ledger_row(&pool, hold.id).await.status, "cancelled");
}

#[traced_test]
#[tokio::test]
async fn retained_payout_leaves_money_already_sent_alone() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    for status in ["processing", "paid", "refunded"] {
        let booking = confirmed_half_refund_booking(&pool, host(&pool).await).await;
        sqlx::query(
            "UPDATE host_payout_ledger SET status = $1::payout_status WHERE booking_id = $2",
        )
        .bind(status)
        .bind(booking.id)
        .execute(&pool)
        .await
        .unwrap();

        cancel(&pool, booking.id).await;

        let row = only_ledger_row(&pool, booking.id).await;
        assert_eq!(row.status, status);
        assert_eq!(row.gross, dec!(400.00), "{status} row keeps its amount");
    }
    assert!(logs_contain("retained payout not recorded"));
}

#[tokio::test]
async fn retained_payout_is_inserted_when_the_booking_has_no_ledger_row() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let booking = confirmed_half_refund_booking(&pool, host(&pool).await).await;
    sqlx::query("DELETE FROM host_payout_ledger WHERE booking_id = $1")
        .bind(booking.id)
        .execute(&pool)
        .await
        .unwrap();

    cancel(&pool, booking.id).await;

    let row = only_ledger_row(&pool, booking.id).await;
    assert_eq!(row.status, "pending");
    assert_eq!(row.net, dec!(180.00));
}

#[tokio::test]
async fn retained_payout_keeps_the_rows_currency_when_the_host_changes_theirs() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host_id = host(&pool).await;
    let booking = confirmed_half_refund_booking(&pool, host_id).await;
    sqlx::query(r#"UPDATE "user" SET default_currency = 'JMD' WHERE id = $1"#)
        .bind(host_id)
        .execute(&pool)
        .await
        .unwrap();

    cancel(&pool, booking.id).await;

    let row = only_ledger_row(&pool, booking.id).await;
    assert_eq!(row.currency, "USD");
    assert_eq!(row.gross, dec!(200.00));
}

#[tokio::test]
async fn retained_payout_is_not_blocked_by_a_retired_pair() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let booking = confirmed_half_refund_booking(&pool, host(&pool).await).await;
    sqlx::query("UPDATE currency_exchange_rates SET retired_at = NOW()")
        .execute(&pool)
        .await
        .unwrap();

    cancel(&pool, booking.id).await;

    assert_eq!(only_ledger_row(&pool, booking.id).await.gross, dec!(200.00));
}

#[tokio::test]
async fn retained_payout_ledger_insert_stays_safe_to_repeat() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let booking = confirmed_half_refund_booking(&pool, host(&pool).await).await;
    let before = only_ledger_row(&pool, booking.id).await;

    // Writing the terms again re-runs the confirmation side effects' insert path.
    let payout = common::payout::PayoutRecord::new(common::payout::NewPayout {
        id: Uuid::now_v7(),
        booking_id: booking.id,
        listing_id: booking.listing_id,
        host_id: Uuid::now_v7(),
        currency: "USD".to_string(),
        gross_amount: dec!(1.00),
        platform_fee_pct: Decimal::ZERO,
        platform_fee_amount: Decimal::ZERO,
        tax_withheld_amount: Decimal::ZERO,
        exchange_rate: Decimal::ONE,
        net_payout_amount: dec!(1.00),
    });
    let mut conn = pool.acquire().await.unwrap();
    crate::payout_ledger::create_payout_ledger_entry(&mut conn, &payout)
        .await
        .unwrap();

    assert_eq!(only_ledger_row(&pool, booking.id).await, before);
}
