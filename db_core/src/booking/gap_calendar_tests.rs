//! `days_between_bookings` on the listing calendar.

use super::get_unavailable_ranges;
use super::unavailable_ranges_tests::{book, day, host, listing};
use common::availability::{UnavailableRange, merge_ranges};
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

async fn villa_with_gap(pool: &PgPool, gap: i32) -> Uuid {
    let host = host(pool).await;
    let villa = listing(pool, host).await;
    sqlx::query("UPDATE listing SET days_between_bookings = $1 WHERE id = $2")
        .bind(gap)
        .bind(villa)
        .execute(pool)
        .await
        .unwrap();
    villa
}

fn range(from: chrono::NaiveDate, to: chrono::NaiveDate) -> UnavailableRange {
    UnavailableRange {
        date_from: from,
        date_to: to,
    }
}

#[tokio::test]
async fn ranges_widen_by_the_gap_on_both_sides() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = villa_with_gap(&pool, 2).await;
    book(&pool, villa, day(3, 10), day(3, 15)).await;

    let ranges = get_unavailable_ranges(&pool, villa, day(3, 1), day(3, 31))
        .await
        .unwrap();
    assert_eq!(ranges, vec![range(day(3, 8), day(3, 17))]);
}

#[tokio::test]
async fn no_gap_returns_the_booked_nights_as_before() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = villa_with_gap(&pool, 0).await;
    book(&pool, villa, day(3, 10), day(3, 15)).await;

    let ranges = get_unavailable_ranges(&pool, villa, day(3, 1), day(3, 31))
        .await
        .unwrap();
    assert_eq!(ranges, vec![range(day(3, 10), day(3, 15))]);
}

#[tokio::test]
async fn a_booking_outside_the_window_shows_when_its_gap_reaches_in() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = villa_with_gap(&pool, 3).await;
    // Checks out on 28 Feb: its gap reaches 3 March.
    book(&pool, villa, day(2, 20), day(2, 28)).await;

    let reaches = get_unavailable_ranges(&pool, villa, day(3, 1), day(3, 31))
        .await
        .unwrap();
    assert_eq!(reaches, vec![range(day(2, 17), day(3, 3))]);

    // A window starting after the gap ends sees nothing.
    let past_it = get_unavailable_ranges(&pool, villa, day(3, 3), day(3, 31))
        .await
        .unwrap();
    assert_eq!(past_it, vec![]);
}

#[tokio::test]
async fn widened_ranges_merge_when_they_touch() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = villa_with_gap(&pool, 0).await;
    book(&pool, villa, day(3, 10), day(3, 12)).await;
    book(&pool, villa, day(3, 16), day(3, 18)).await;
    sqlx::query("UPDATE listing SET days_between_bookings = 2 WHERE id = $1")
        .bind(villa)
        .execute(&pool)
        .await
        .unwrap();

    let ranges = get_unavailable_ranges(&pool, villa, day(3, 1), day(3, 31))
        .await
        .unwrap();
    assert_eq!(
        merge_ranges(ranges),
        vec![range(day(3, 8), day(3, 20))],
        "14 March is where the two widened ranges meet"
    );
}

#[tokio::test]
async fn a_stored_gap_outside_the_limits_is_read_as_the_nearest_limit() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;

    let villa = villa_with_gap(&pool, -5).await;
    book(&pool, villa, day(3, 10), day(3, 15)).await;
    let ranges = get_unavailable_ranges(&pool, villa, day(3, 1), day(3, 31))
        .await
        .unwrap();
    assert_eq!(ranges, vec![range(day(3, 10), day(3, 15))]);

    let villa = villa_with_gap(&pool, i32::MAX).await;
    book(&pool, villa, day(3, 10), day(3, 15)).await;
    let ranges = get_unavailable_ranges(&pool, villa, day(3, 1), day(3, 31))
        .await
        .unwrap();
    let year = chrono::Duration::days(365);
    assert_eq!(ranges, vec![range(day(3, 10) - year, day(3, 15) + year)]);
}
