//! Export feed contents (spec §1.6.2).

use super::export::export_events;
use super::fixtures::{insert_booking, insert_feed, insert_imported_block, insert_manual_block};
use crate::promo_tests::{migrated_db, raw_listing, raw_user};
use chrono::{Duration, NaiveDate};
use common::calendar::ics_write::ExportSource;

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2030, 1, 1).expect("date")
}

fn day(n: i64) -> NaiveDate {
    today() + Duration::days(n)
}

#[tokio::test]
async fn the_export_has_stays_holds_and_blocks_but_no_gap_or_lapsed_items() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let host = raw_user(&pool).await;
    let v = raw_listing(&pool, host).await;
    sqlx::query("UPDATE listing SET days_between_bookings = 3, is_active = FALSE WHERE id = $1")
        .bind(v)
        .execute(&pool)
        .await
        .expect("gap");
    let confirmed = insert_booking(&pool, v, day(5), day(8), "confirmed", "NULL").await;
    insert_booking(
        &pool,
        v,
        day(10),
        day(12),
        "pending",
        "NOW() + INTERVAL '1 hour'",
    )
    .await;
    insert_booking(
        &pool,
        v,
        day(14),
        day(16),
        "pending",
        "NOW() - INTERVAL '1 hour'",
    )
    .await;
    insert_booking(&pool, v, day(18), day(20), "cancelled", "NULL").await;
    insert_booking(&pool, v, day(-5), day(0), "confirmed", "NULL").await;
    insert_booking(&pool, v, day(800), day(802), "confirmed", "NULL").await;
    let manual = insert_manual_block(&pool, v, day(22), day(23)).await;
    let feed = insert_feed(&pool, v).await;
    insert_imported_block(&pool, v, feed, day(30), day(33)).await;
    // A live request to book is not exported.
    let guest = raw_user(&pool).await;
    sqlx::query(
        "WITH conv AS (INSERT INTO conversation (listing_id, guest_id) VALUES ($1, $2) RETURNING id)
         INSERT INTO booking_request (conversation_id, listing_id, guest_id, origin, status,
             date_from, date_to, number_of_persons, currency, price_snapshot, expires_at)
         SELECT conv.id, $1, $2, 'guest_request', 'open', $3, $4, 2, 'USD', '{}'::jsonb,
             NOW() + INTERVAL '1 hour' FROM conv",
    )
    .bind(v)
    .bind(guest)
    .bind(day(40))
    .bind(day(42))
    .execute(&pool)
    .await
    .expect("request");

    let (name, _slug, events) = export_events(&pool, v, today())
        .await
        .expect("export")
        .expect("listing");
    assert!(name.starts_with("Villa"));
    let got: Vec<_> = events.iter().map(|e| (e.date_from, e.date_to)).collect();
    assert_eq!(
        got,
        vec![
            (day(5), day(8)),
            (day(10), day(12)),
            (day(22), day(23)),
            (day(30), day(33))
        ]
    );
    assert_eq!(events[0].uid, format!("booking-{confirmed}@ourplaces"));
    assert!(
        matches!(&events[0].source, ExportSource::Booking { confirmation_code } if !confirmation_code.is_empty())
    );
    assert_eq!(events[2].uid, format!("block-{manual}@ourplaces"));
    assert!(
        events[3].uid.ends_with("@airbnb.com"),
        "imported blocks keep their UID"
    );
    assert!(
        matches!(&events[3].source, ExportSource::Imported { feed_name } if feed_name == "Airbnb")
    );
}

#[tokio::test]
async fn a_deleted_listing_has_no_export() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let host = raw_user(&pool).await;
    let v = raw_listing(&pool, host).await;
    sqlx::query("UPDATE listing SET deleted_at = NOW() WHERE id = $1")
        .bind(v)
        .execute(&pool)
        .await
        .expect("delete");
    assert!(
        export_events(&pool, v, today())
            .await
            .expect("export")
            .is_none()
    );
    assert!(
        export_events(&pool, uuid::Uuid::now_v7(), today())
            .await
            .expect("export")
            .is_none()
    );
}
