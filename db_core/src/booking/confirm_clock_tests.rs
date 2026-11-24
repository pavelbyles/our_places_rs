//! Confirming a hold reads the clock, not the transaction's start time.

use super::lock_booking;
use crate::booking::unavailable_ranges_tests::{book, day, host, listing};
use sqlx_db_tester::TestPg;
use std::path::Path;

fn test_db() -> TestPg {
    dotenvy::dotenv().ok();
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".into());
    TestPg::new(db_url, Path::new("migrations"))
}

/// `expires_at` is later than this transaction's `NOW()` and earlier than the
/// next `clock_timestamp()`, which is what a confirmation sees when it waited
/// on the listing lock while its hold ran out.
#[tokio::test]
async fn a_hold_that_expired_during_the_transaction_is_not_active() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = host(&pool).await;
    let villa = listing(&pool, host).await;
    let hold = book(&pool, villa, day(3, 10), day(3, 15)).await;

    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE booking SET expires_at = clock_timestamp() WHERE id = $1")
        .bind(hold.id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let current = lock_booking(&mut tx, hold.id).await.unwrap();
    assert!(!current.hold_active);
}

#[tokio::test]
async fn a_hold_with_time_left_is_active() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = host(&pool).await;
    let villa = listing(&pool, host).await;
    let hold = book(&pool, villa, day(3, 10), day(3, 15)).await;

    let mut conn = pool.acquire().await.unwrap();
    let current = lock_booking(&mut conn, hold.id).await.unwrap();
    assert!(current.hold_active);
}
