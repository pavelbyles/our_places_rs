//! Manual block tests (spec §1.2.2–3).

use super::blocks::{create_manual_block, delete_manual_block, list_blocks};
use super::fixtures::{d, insert_booking, insert_feed, insert_imported_block};
use crate::error::DbError;
use crate::promo_tests::{migrated_db, raw_listing, raw_user};
use chrono::NaiveDate;
use common::calendar::BlockSource;
use common::calendar::dto::NewBlockedDateRequest;
use sqlx::PgPool;
use uuid::Uuid;

fn req(from: NaiveDate, to: NaiveDate) -> NewBlockedDateRequest {
    NewBlockedDateRequest {
        date_from: from,
        date_to: to,
        note: Some("  owner stay  ".into()),
    }
}

async fn villa(pool: &PgPool, gap: i32) -> Uuid {
    let host = raw_user(pool).await;
    let v = raw_listing(pool, host).await;
    sqlx::query("UPDATE listing SET days_between_bookings = $1 WHERE id = $2")
        .bind(gap)
        .bind(v)
        .execute(pool)
        .await
        .expect("gap");
    v
}

async fn live_request(pool: &PgPool, listing: Uuid, from: NaiveDate, to: NaiveDate) {
    let guest = raw_user(pool).await;
    sqlx::query(
        "WITH conv AS (
            INSERT INTO conversation (listing_id, guest_id) VALUES ($1, $2) RETURNING id)
         INSERT INTO booking_request (conversation_id, listing_id, guest_id, origin, status,
             date_from, date_to, number_of_persons, currency, price_snapshot, expires_at)
         SELECT conv.id, $1, $2, 'guest_request', 'open', $3, $4, 2, 'USD', '{}'::jsonb,
             NOW() + INTERVAL '1 hour'
         FROM conv",
    )
    .bind(listing)
    .bind(guest)
    .bind(from)
    .bind(to)
    .execute(pool)
    .await
    .expect("request");
}

#[tokio::test]
async fn blocks_over_stays_holds_and_requests_are_refused() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool, 2).await;
    insert_booking(&pool, v, d(1, 10), d(1, 15), "confirmed", "NULL").await;
    insert_booking(
        &pool,
        v,
        d(2, 10),
        d(2, 15),
        "pending",
        "NOW() + INTERVAL '1 hour'",
    )
    .await;
    live_request(&pool, v, d(3, 10), d(3, 15)).await;
    for (from, to) in [
        (d(1, 14), d(1, 16)),
        (d(2, 1), d(2, 11)),
        (d(3, 12), d(3, 13)),
    ] {
        let result = create_manual_block(&pool, v, &req(from, to), None).await;
        assert!(
            matches!(result, Err(DbError::DateUnavailable)),
            "{from}: {result:?}"
        );
    }
}

#[tokio::test]
async fn blocks_over_gap_days_expired_holds_and_other_blocks_are_allowed() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool, 2).await;
    insert_booking(&pool, v, d(1, 10), d(1, 15), "confirmed", "NULL").await;
    insert_booking(
        &pool,
        v,
        d(2, 10),
        d(2, 15),
        "pending",
        "NOW() - INTERVAL '1 hour'",
    )
    .await;
    // Gap days on both sides of the stay (E22).
    create_manual_block(&pool, v, &req(d(1, 15), d(1, 17)), None)
        .await
        .expect("after");
    create_manual_block(&pool, v, &req(d(1, 8), d(1, 10)), None)
        .await
        .expect("before");
    create_manual_block(&pool, v, &req(d(2, 10), d(2, 15)), None)
        .await
        .expect("expired hold");
    // Overlapping another block.
    let feed = insert_feed(&pool, v).await;
    insert_imported_block(&pool, v, feed, d(4, 1), d(4, 10)).await;
    let block = create_manual_block(&pool, v, &req(d(4, 5), d(4, 12)), None)
        .await
        .expect("overlap");
    assert_eq!(block.note.as_deref(), Some("owner stay"));
    assert_eq!(block.source, BlockSource::Manual);
}

#[tokio::test]
async fn an_exact_duplicate_is_refused() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool, 0).await;
    create_manual_block(&pool, v, &req(d(5, 1), d(5, 4)), None)
        .await
        .expect("first");
    let dup = create_manual_block(&pool, v, &req(d(5, 1), d(5, 4)), None).await;
    assert!(
        matches!(dup, Err(DbError::CalendarBlockDuplicate)),
        "{dup:?}"
    );
}

#[tokio::test]
async fn deleting_checks_the_source_and_the_listing() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool, 0).await;
    let other = villa(&pool, 0).await;
    let manual = create_manual_block(&pool, v, &req(d(5, 1), d(5, 4)), None)
        .await
        .expect("block");
    let feed = insert_feed(&pool, v).await;
    let imported = insert_imported_block(&pool, v, feed, d(6, 1), d(6, 3)).await;

    let result = delete_manual_block(&pool, v, imported).await;
    assert!(
        matches!(result, Err(DbError::CalendarBlockManagedByFeed)),
        "{result:?}"
    );
    assert!(
        !delete_manual_block(&pool, other, manual.id)
            .await
            .expect("other listing")
    );
    assert!(
        delete_manual_block(&pool, v, manual.id)
            .await
            .expect("delete")
    );
    assert!(
        !delete_manual_block(&pool, v, manual.id)
            .await
            .expect("gone")
    );
}

#[tokio::test]
async fn listing_shows_feed_names_and_disabled_feeds() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool, 0).await;
    let feed = insert_feed(&pool, v).await;
    insert_imported_block(&pool, v, feed, d(6, 1), d(6, 3)).await;
    create_manual_block(&pool, v, &req(d(5, 1), d(5, 4)), None)
        .await
        .expect("block");
    sqlx::query("UPDATE listing_calendar_feed SET is_active = FALSE WHERE id = $1")
        .bind(feed)
        .execute(&pool)
        .await
        .expect("disable");
    let blocks = list_blocks(&pool, v, d(1, 1), d(12, 31))
        .await
        .expect("list");
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].source, BlockSource::Manual);
    assert_eq!(blocks[0].feed_name, None);
    assert_eq!(blocks[1].feed_name.as_deref(), Some("Airbnb"));
    assert_eq!(blocks[1].feed_active, Some(false));
    assert!(
        list_blocks(&pool, v, d(7, 1), d(8, 1))
            .await
            .expect("list")
            .is_empty()
    );
}
