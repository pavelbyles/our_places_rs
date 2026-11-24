//! `days_between_bookings` in the availability checks.

use super::unavailable_ranges_tests::{book, book_held, day, host, listing};
use super::{check_availability, check_availability_excluding, update_booking};
use crate::models::{BookingStatus, SupplierInfo, UpdatedBooking};
use chrono::NaiveDate;
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

async fn set_gap(pool: &PgPool, listing_id: Uuid, gap: i32) {
    sqlx::query("UPDATE listing SET days_between_bookings = $1 WHERE id = $2")
        .bind(gap)
        .bind(listing_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn villa_with_gap(pool: &PgPool, gap: i32) -> Uuid {
    let host = host(pool).await;
    let villa = listing(pool, host).await;
    set_gap(pool, villa, gap).await;
    villa
}

async fn is_free(pool: &PgPool, villa: Uuid, from: NaiveDate, to: NaiveDate) -> bool {
    check_availability(pool, villa, from, to).await.unwrap()
}

#[tokio::test]
async fn a_stay_must_keep_the_gap_on_either_side_of_a_booking() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = villa_with_gap(&pool, 2).await;
    book(&pool, villa, day(3, 10), day(3, 15)).await;

    // After the booking: check-in must be at least 2 days after check-out.
    assert!(!is_free(&pool, villa, day(3, 16), day(3, 20)).await);
    assert!(is_free(&pool, villa, day(3, 17), day(3, 20)).await);
    // Before it: check-out must be at least 2 days before check-in.
    assert!(!is_free(&pool, villa, day(3, 5), day(3, 9)).await);
    assert!(is_free(&pool, villa, day(3, 5), day(3, 8)).await);
    // The booking's own nights are still taken.
    assert!(!is_free(&pool, villa, day(3, 11), day(3, 12)).await);
}

#[tokio::test]
async fn no_gap_keeps_same_day_turnover() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = villa_with_gap(&pool, 0).await;
    book(&pool, villa, day(3, 10), day(3, 15)).await;

    assert!(is_free(&pool, villa, day(3, 15), day(3, 20)).await);
    assert!(is_free(&pool, villa, day(3, 5), day(3, 10)).await);
    assert!(!is_free(&pool, villa, day(3, 14), day(3, 20)).await);
    assert!(!is_free(&pool, villa, day(3, 5), day(3, 11)).await);
}

#[tokio::test]
async fn an_expired_hold_keeps_no_gap() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = villa_with_gap(&pool, 2).await;
    book_held(&pool, villa, day(3, 10), day(3, 15), -1).await;

    assert!(is_free(&pool, villa, day(3, 15), day(3, 20)).await);
}

#[tokio::test]
async fn a_stored_gap_outside_the_limits_is_read_as_the_nearest_limit() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = villa_with_gap(&pool, 0).await;
    book(&pool, villa, day(3, 10), day(3, 15)).await;

    // A negative gap doesn't shrink the rule, so an overlap stays refused.
    set_gap(&pool, villa, -5).await;
    assert!(!is_free(&pool, villa, day(3, 14), day(3, 20)).await);
    assert!(is_free(&pool, villa, day(3, 15), day(3, 20)).await);

    // A huge gap doesn't overflow the date arithmetic. It acts as 365 days.
    set_gap(&pool, villa, i32::MAX).await;
    let far = day(3, 15) + chrono::Duration::days(365);
    assert!(!is_free(&pool, villa, far - chrono::Duration::days(1), far).await);
    assert!(is_free(&pool, villa, far, far + chrono::Duration::days(1)).await);
}

#[tokio::test]
async fn a_booking_is_checked_against_its_neighbours_not_itself() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = villa_with_gap(&pool, 2).await;
    let booking = book(&pool, villa, day(3, 10), day(3, 15)).await;
    book(&pool, villa, day(3, 20), day(3, 25)).await;

    let free = |from, to| check_availability_excluding(&pool, villa, from, to, booking.id);

    // Moving within its own dates and gap is fine.
    assert!(free(day(3, 11), day(3, 16)).await.unwrap());
    // The neighbour's gap still applies: check-out must be by the 18th.
    assert!(free(day(3, 10), day(3, 18)).await.unwrap());
    assert!(!free(day(3, 10), day(3, 19)).await.unwrap());
}

#[tokio::test]
async fn raising_a_gap_leaves_a_booking_that_keeps_its_dates_alone() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = villa_with_gap(&pool, 0).await;
    let first = book(&pool, villa, day(3, 10), day(3, 15)).await;
    let second = book(&pool, villa, day(3, 15), day(3, 20)).await;

    set_gap(&pool, villa, 2).await;

    // Both live holds can still be confirmed, even though they're now too close.
    let supplier = SupplierInfo {
        supplier_name: "Our Places Jamaica Limited".to_string(),
        supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
        supplier_trn: "000-111-222".to_string(),
    };
    let confirm = UpdatedBooking {
        status: Some(BookingStatus::Confirmed),
        ..Default::default()
    };
    for id in [first.id, second.id] {
        let confirmed = update_booking(&pool, id, &confirm, &supplier)
            .await
            .unwrap();
        assert_eq!(confirmed.status, BookingStatus::Confirmed);
    }
}

#[tokio::test]
async fn a_move_that_stays_inside_a_raised_gap_is_refused_even_moving_away() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = villa_with_gap(&pool, 0).await;
    let first = book(&pool, villa, day(3, 10), day(3, 15)).await;
    book(&pool, villa, day(3, 15), day(3, 20)).await;

    set_gap(&pool, villa, 2).await;

    let free = |from, to| check_availability_excluding(&pool, villa, from, to, first.id);
    assert!(!free(day(3, 10), day(3, 14)).await.unwrap());
    assert!(free(day(3, 10), day(3, 13)).await.unwrap());
}
