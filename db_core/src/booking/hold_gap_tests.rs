//! `days_between_bookings` when placing a hold.

use super::create_booking;
use super::unavailable_ranges_tests::{
    HOLD_MINUTES, book, book_held, day, host, listing, new_booking, user,
};
use crate::error::DbError;
use crate::models::{Booking, BookingStatus, UserRole};
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

async fn hold(
    pool: &PgPool,
    villa: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> crate::error::Result<Booking> {
    let guest = user(pool, UserRole::Booker).await;
    create_booking(pool, &new_booking(guest, villa, from, to), HOLD_MINUTES).await
}

async fn confirm(pool: &PgPool, id: Uuid) {
    sqlx::query("UPDATE booking SET status = 'confirmed' WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

fn assert_refused(result: crate::error::Result<Booking>) {
    match result {
        Err(DbError::DateUnavailable) => {}
        other => panic!("expected DateUnavailable, got {other:?}"),
    }
}

#[tokio::test]
async fn a_hold_must_keep_the_gap_around_a_confirmed_booking() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = villa_with_gap(&pool, 2).await;
    let booking = book(&pool, villa, day(3, 10), day(3, 15)).await;
    confirm(&pool, booking.id).await;

    assert_refused(hold(&pool, villa, day(3, 16), day(3, 20)).await);
    assert_refused(hold(&pool, villa, day(3, 5), day(3, 9)).await);
    hold(&pool, villa, day(3, 17), day(3, 20)).await.unwrap();
    hold(&pool, villa, day(3, 5), day(3, 8)).await.unwrap();
}

#[tokio::test]
async fn a_hold_must_keep_the_gap_around_a_live_hold_but_not_an_expired_one() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;

    let villa = villa_with_gap(&pool, 2).await;
    book(&pool, villa, day(3, 10), day(3, 15)).await;
    assert_refused(hold(&pool, villa, day(3, 16), day(3, 20)).await);

    let villa = villa_with_gap(&pool, 2).await;
    book_held(&pool, villa, day(3, 10), day(3, 15), -1).await;
    hold(&pool, villa, day(3, 16), day(3, 20)).await.unwrap();
}

#[tokio::test]
async fn an_overlap_is_refused_with_no_gap_and_same_day_turnover_is_not() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = villa_with_gap(&pool, 0).await;
    book(&pool, villa, day(3, 10), day(3, 15)).await;

    assert_refused(hold(&pool, villa, day(3, 14), day(3, 20)).await);
    hold(&pool, villa, day(3, 15), day(3, 20)).await.unwrap();
}

#[tokio::test]
async fn a_stored_gap_outside_the_limits_is_read_as_the_nearest_limit() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;

    let villa = villa_with_gap(&pool, -5).await;
    book(&pool, villa, day(3, 10), day(3, 15)).await;
    assert_refused(hold(&pool, villa, day(3, 14), day(3, 20)).await);

    let villa = villa_with_gap(&pool, i32::MAX).await;
    book(&pool, villa, day(3, 10), day(3, 15)).await;
    let far = day(3, 15) + chrono::Duration::days(365);
    assert_refused(hold(&pool, villa, far - chrono::Duration::days(1), far).await);
    hold(&pool, villa, far, far + chrono::Duration::days(1))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_holds_never_end_up_inside_each_others_gap() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = villa_with_gap(&pool, 2).await;

    // Each stay is free alone, but they're too close to both be held.
    let stays = [
        (day(3, 10), day(3, 15)),
        (day(3, 16), day(3, 20)),
        (day(3, 5), day(3, 9)),
    ];
    let mut tasks = Vec::new();
    for (from, to) in stays {
        let guest = user(&pool, UserRole::Booker).await;
        let pool = pool.clone();
        tasks.push(tokio::spawn(async move {
            create_booking(&pool, &new_booking(guest, villa, from, to), HOLD_MINUTES).await
        }));
    }
    let mut placed = Vec::new();
    for task in tasks {
        match task.await.unwrap() {
            Ok(booking) => placed.push(booking),
            Err(DbError::DateUnavailable) => {}
            Err(other) => panic!("expected DateUnavailable, got {other:?}"),
        }
    }
    // The 5–9 and 16–20 stays don't conflict with each other, so either one
    // success (the 10–15 stay won) or two (it lost to both) is possible. What
    // can never happen is two stays inside each other's gap.
    assert!(!placed.is_empty());
    for a in &placed {
        for b in &placed {
            if a.id != b.id {
                let clear = a.date_to + chrono::Duration::days(2) <= b.date_from
                    || b.date_to + chrono::Duration::days(2) <= a.date_from;
                assert!(clear, "{a:?} and {b:?} are inside each other's gap");
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn two_racing_holds_for_the_same_gap_give_exactly_one_success() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = villa_with_gap(&pool, 2).await;

    let mut tasks = Vec::new();
    for (from, to) in [(day(3, 10), day(3, 15)), (day(3, 16), day(3, 20))] {
        let guest = user(&pool, UserRole::Booker).await;
        let pool = pool.clone();
        tasks.push(tokio::spawn(async move {
            create_booking(&pool, &new_booking(guest, villa, from, to), HOLD_MINUTES).await
        }));
    }
    let mut placed = 0;
    for task in tasks {
        match task.await.unwrap() {
            Ok(_) => placed += 1,
            Err(DbError::DateUnavailable) => {}
            Err(other) => panic!("expected DateUnavailable, got {other:?}"),
        }
    }
    assert_eq!(placed, 1);
}

#[tokio::test]
async fn a_guest_can_replace_their_own_live_hold_with_one_inside_its_gap() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let villa = villa_with_gap(&pool, 2).await;
    let guest = user(&pool, UserRole::Booker).await;

    let first = create_booking(
        &pool,
        &new_booking(guest, villa, day(3, 10), day(3, 15)),
        HOLD_MINUTES,
    )
    .await
    .unwrap();
    let second = create_booking(
        &pool,
        &new_booking(guest, villa, day(3, 16), day(3, 20)),
        HOLD_MINUTES,
    )
    .await
    .unwrap();

    assert_eq!(second.status, BookingStatus::Pending);
    let old = super::get_booking_by_id(&pool, first.id).await.unwrap();
    assert_eq!(old.status, BookingStatus::Cancelled);
}
