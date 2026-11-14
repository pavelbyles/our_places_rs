//! Spec 103 §1.5: a booking reveals its listing's stored point only when it
//! is confirmed or completed, decided in the same read.

use super::listing_locations;
use super::unavailable_ranges_tests::{book, day, host, listing};
use common::location::ListingLocation;
use sqlx::PgPool;
use sqlx_db_tester::TestPg;
use std::path::Path;
use uuid::Uuid;

const POINT: ListingLocation = ListingLocation {
    latitude: 18.012345,
    longitude: -76.790123,
};

fn test_db() -> TestPg {
    dotenvy::dotenv().ok();
    let db_url = std::env::var(crate::DATABASE_URL_ENV)
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".into());
    TestPg::new(db_url, Path::new("migrations"))
}

async fn located_listing(pool: &PgPool) -> Uuid {
    let villa = listing(pool, host(pool).await).await;
    sqlx::query("UPDATE listing SET latitude = $2, longitude = $3 WHERE id = $1")
        .bind(villa)
        .bind(POINT.latitude)
        .bind(POINT.longitude)
        .execute(pool)
        .await
        .unwrap();
    villa
}

/// A booking at `status`, set directly: these tests are about the read.
async fn booking_with(pool: &PgPool, villa: Uuid, month: u32, status: &str) -> Uuid {
    let b = book(pool, villa, day(month, 1), day(month, 3)).await;
    sqlx::query("UPDATE booking SET status = $2::booking_status WHERE id = $1")
        .bind(b.id)
        .bind(status)
        .execute(pool)
        .await
        .unwrap();
    b.id
}

#[tokio::test]
async fn only_confirmed_and_completed_bookings_get_the_point() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let villa = located_listing(&pool).await;
    let pending = book(&pool, villa, day(1, 1), day(1, 3)).await.id;
    let confirmed = booking_with(&pool, villa, 2, "confirmed").await;
    let completed = booking_with(&pool, villa, 3, "completed").await;
    let cancelled = booking_with(&pool, villa, 4, "cancelled").await;

    let found = listing_locations(&pool, &[pending, confirmed, completed, cancelled])
        .await
        .unwrap();

    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found.get(&confirmed), Some(&POINT));
    assert_eq!(found.get(&completed), Some(&POINT));
    assert!(!found.contains_key(&pending));
    assert!(!found.contains_key(&cancelled));
}

#[tokio::test]
async fn a_confirmed_then_cancelled_booking_gets_nothing() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let villa = located_listing(&pool).await;
    let id = booking_with(&pool, villa, 2, "confirmed").await;
    assert!(
        listing_locations(&pool, &[id])
            .await
            .unwrap()
            .contains_key(&id)
    );
    sqlx::query("UPDATE booking SET status = 'cancelled' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(listing_locations(&pool, &[id]).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_soft_deleted_listing_still_shows_a_confirmed_stay() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let villa = located_listing(&pool).await;
    let id = booking_with(&pool, villa, 2, "confirmed").await;
    crate::listing::delete_listing(&pool, villa, false)
        .await
        .unwrap();
    assert_eq!(
        listing_locations(&pool, &[id]).await.unwrap().get(&id),
        Some(&POINT)
    );
}

#[tokio::test]
async fn a_missing_coordinate_gives_nothing() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let villa = located_listing(&pool).await;
    let id = booking_with(&pool, villa, 2, "confirmed").await;
    sqlx::query("UPDATE listing SET longitude = NULL WHERE id = $1")
        .bind(villa)
        .execute(&pool)
        .await
        .unwrap();
    assert!(listing_locations(&pool, &[id]).await.unwrap().is_empty());
}

#[tokio::test]
async fn unknown_and_empty_ids_give_nothing() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    assert!(listing_locations(&pool, &[]).await.unwrap().is_empty());
    assert!(
        listing_locations(&pool, &[Uuid::now_v7()])
            .await
            .unwrap()
            .is_empty()
    );
}

#[test]
fn the_status_filter_is_an_allowlist() {
    // A denylist (`<> 'pending'`, `NOT IN (...)`) would let a status added
    // to the enum later through.
    let src = include_str!("queries.rs");
    let at = src.find("pub async fn listing_locations").unwrap();
    let body = &src[at..];
    assert!(
        body.contains("b.status IN ('confirmed', 'completed')"),
        "{body}"
    );
    assert!(!body.contains("NOT IN") && !body.contains("<>") && !body.contains("!="));
}
