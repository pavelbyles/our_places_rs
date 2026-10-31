//! Hold expiry tests.

use super::unavailable_ranges_tests::{book, book_held, day, host, listing};
use super::*;
use crate::models::{BookingStatus, SupplierInfo, UpdatedBooking};
use rust_decimal_macros::dec;
use sqlx::PgPool;
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

fn confirm() -> UpdatedBooking {
    UpdatedBooking {
        status: Some(BookingStatus::Confirmed),
        ..Default::default()
    }
}

async fn expire(pool: &PgPool, id: Uuid) {
    sqlx::query!(
        "UPDATE booking SET expires_at = NOW() - INTERVAL '1 minute' WHERE id = $1",
        id
    )
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn new_hold_expires_after_the_configured_window() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = listing(&pool, host(&pool).await).await;
    let hold = book_held(&pool, villa, day(4, 1), day(4, 3), 45).await;

    let minutes = sqlx::query_scalar!(
        r#"SELECT EXTRACT(EPOCH FROM expires_at - created_at)::INT / 60 as "m!" FROM booking WHERE id = $1"#,
        hold.id
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(minutes, 45);
}

#[tokio::test]
async fn live_hold_blocks_dates_but_expired_hold_frees_them() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = listing(&pool, host(&pool).await).await;
    let hold = book(&pool, villa, day(4, 1), day(4, 3)).await;

    assert!(
        !check_availability(&pool, villa, day(4, 1), day(4, 3))
            .await
            .unwrap()
    );

    expire(&pool, hold.id).await;

    assert!(
        check_availability(&pool, villa, day(4, 1), day(4, 3))
            .await
            .unwrap()
    );
    assert!(
        get_unavailable_ranges(&pool, villa, day(4, 1), day(5, 1))
            .await
            .unwrap()
            .is_empty()
    );
    // A different guest can take the dates before any sweep has run.
    let rebooked = book(&pool, villa, day(4, 1), day(4, 3)).await;
    assert_eq!(rebooked.status, BookingStatus::Pending);
}

#[tokio::test]
async fn expired_hold_cannot_be_confirmed() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = listing(&pool, host(&pool).await).await;
    let hold = book(&pool, villa, day(4, 1), day(4, 3)).await;
    expire(&pool, hold.id).await;

    let err = update_booking(&pool, hold.id, &confirm(), &supplier())
        .await
        .unwrap_err();
    assert!(matches!(err, crate::error::DbError::HoldExpired), "{err:?}");

    let after = get_booking_by_id(&pool, hold.id).await.unwrap();
    assert_eq!(after.status, BookingStatus::Pending);
}

#[tokio::test]
async fn sweep_cancels_only_expired_holds_with_full_history_snapshot() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = listing(&pool, host(&pool).await).await;
    let stale = book(&pool, villa, day(4, 1), day(4, 3)).await;
    let live = book(&pool, villa, day(5, 1), day(5, 3)).await;
    expire(&pool, stale.id).await;

    let expired = expire_stale_holds(&pool).await.unwrap();
    assert_eq!(
        expired.iter().map(|b| b.id).collect::<Vec<_>>(),
        vec![stale.id]
    );
    assert_eq!(expired[0].status, BookingStatus::Cancelled);
    assert_eq!(
        get_booking_by_id(&pool, live.id).await.unwrap().status,
        BookingStatus::Pending
    );

    let snapshot = sqlx::query!(
        r#"SELECT status as "status: BookingStatus", tax_rate, exchange_rate, platform_fee
        FROM booking_history WHERE booking_id = $1 AND change_reason = 'Hold expired'"#,
        stale.id
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(snapshot.status, BookingStatus::Cancelled);
    assert_eq!(snapshot.tax_rate, Some(stale.tax_rate));
    assert_eq!(snapshot.exchange_rate, Some(stale.exchange_rate));
    assert_eq!(snapshot.platform_fee, Some(stale.platform_fee));

    // Idempotent: nothing left to expire.
    assert!(expire_stale_holds(&pool).await.unwrap().is_empty());
}

#[tokio::test]
async fn host_payout_converts_from_the_booking_currency() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    // USD listing and USD host, but the guest paid in JMD.
    let villa = listing(&pool, host(&pool).await).await;
    let hold = book(&pool, villa, day(4, 1), day(4, 3)).await;
    sqlx::query!("UPDATE booking SET currency = 'JMD' WHERE id = $1", hold.id)
        .execute(&pool)
        .await
        .unwrap();

    let confirmed = update_booking(&pool, hold.id, &confirm(), &supplier())
        .await
        .unwrap();

    let ledger = sqlx::query!(
        "SELECT exchange_rate, gross_amount, currency FROM host_payout_ledger WHERE booking_id = $1",
        hold.id
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    // Seeded JMD -> USD rate, not the listing's USD -> USD identity.
    assert_eq!(ledger.exchange_rate, dec!(0.006250));
    assert_eq!(
        ledger.gross_amount,
        (confirmed.accommodation_net() * dec!(0.006250)).round_dp(2)
    );
    assert_eq!(ledger.currency.trim(), "USD");
}

#[tokio::test]
async fn confirmation_fails_without_a_payout_rate_and_leaves_hold_pending() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host_id = host(&pool).await;
    let villa = listing(&pool, host_id).await;
    let hold = book(&pool, villa, day(4, 1), day(4, 3)).await;
    // JMD receipt needs no rate; JMD -> CAD has none seeded.
    sqlx::query!("UPDATE booking SET currency = 'JMD' WHERE id = $1", hold.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query!(
        r#"UPDATE "user" SET default_currency = 'CAD' WHERE id = $1"#,
        host_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let err = update_booking(&pool, hold.id, &confirm(), &supplier())
        .await
        .unwrap_err();
    assert!(
        matches!(err, crate::error::DbError::FxRateUnavailable { ref base, ref target } if base == "JMD" && target == "CAD"),
        "{err:?}"
    );
    assert_eq!(
        get_booking_by_id(&pool, hold.id).await.unwrap().status,
        BookingStatus::Pending
    );
}
