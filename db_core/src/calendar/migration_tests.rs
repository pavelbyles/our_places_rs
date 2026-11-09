//! Schema tests for the calendar sync migration (#58).

use crate::promo_tests::{migrated_db, raw_booking, raw_listing, raw_user};
use sqlx::PgPool;
use uuid::Uuid;

async fn feed(pool: &PgPool, listing: Uuid, url: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO listing_calendar_feed (listing_id, name, url) VALUES ($1, 'Airbnb', $2) RETURNING id",
    )
    .bind(listing)
    .bind(url)
    .fetch_one(pool)
    .await
    .expect("insert feed")
}

async fn try_block(
    pool: &PgPool,
    listing: Uuid,
    source: &str,
    feed: Option<Uuid>,
    uid: Option<&str>,
    (from, to): (&str, &str),
    note: Option<&str>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO listing_blocked_dates (listing_id, source, feed_id, external_uid, date_from, date_to, note)
         VALUES ($1, $2::calendar_block_source, $3, $4, $5::date, $6::date, $7) RETURNING id",
    )
    .bind(listing)
    .bind(source)
    .bind(feed)
    .bind(uid)
    .bind(from)
    .bind(to)
    .bind(note)
    .fetch_one(pool)
    .await
}

fn is_check_or_unique_or_fk(e: &sqlx::Error) -> bool {
    matches!(
        crate::error::sqlstate(e).as_deref(),
        Some("23514" | "23505" | "23503")
    )
}

#[tokio::test]
async fn migration_applies_on_a_fresh_database() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let version: String = sqlx::query_scalar("SHOW server_version")
        .fetch_one(&pool)
        .await
        .expect("server version");
    tracing::info!(%version, "calendar migration applied");
    let tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.tables
         WHERE table_name IN ('listing_calendar_feed', 'listing_blocked_dates', 'calendar_conflict')",
    )
    .fetch_one(&pool)
    .await
    .expect("count tables");
    assert_eq!(tables, 3);
}

#[tokio::test]
async fn constraints_refuse_malformed_blocks() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let host = raw_user(&pool).await;
    let villa = raw_listing(&pool, host).await;
    let other = raw_listing(&pool, host).await;
    let f = feed(&pool, villa, "https://airbnb.example/a.ics").await;
    let other_feed = feed(&pool, other, "https://airbnb.example/b.ics").await;

    let cases: Vec<(&str, Result<Uuid, sqlx::Error>)> = vec![
        (
            "manual with feed",
            try_block(
                &pool,
                villa,
                "manual",
                Some(f),
                None,
                ("2030-01-01", "2030-01-02"),
                None,
            )
            .await,
        ),
        (
            "manual with uid",
            try_block(
                &pool,
                villa,
                "manual",
                None,
                Some("x"),
                ("2030-01-01", "2030-01-02"),
                None,
            )
            .await,
        ),
        (
            "imported without uid",
            try_block(
                &pool,
                villa,
                "ical_feed",
                Some(f),
                None,
                ("2030-01-01", "2030-01-02"),
                None,
            )
            .await,
        ),
        (
            "imported with note",
            try_block(
                &pool,
                villa,
                "ical_feed",
                Some(f),
                Some("u1"),
                ("2030-01-01", "2030-01-02"),
                Some("n"),
            )
            .await,
        ),
        (
            "empty range",
            try_block(
                &pool,
                villa,
                "manual",
                None,
                None,
                ("2030-01-02", "2030-01-02"),
                None,
            )
            .await,
        ),
        (
            "listing differs from its feed's",
            try_block(
                &pool,
                villa,
                "ical_feed",
                Some(other_feed),
                Some("u9"),
                ("2030-01-01", "2030-01-02"),
                None,
            )
            .await,
        ),
    ];
    for (name, result) in cases {
        let err = result.expect_err(name);
        assert!(is_check_or_unique_or_fk(&err), "{name}: {err}");
    }

    try_block(
        &pool,
        villa,
        "ical_feed",
        Some(f),
        Some("u1"),
        ("2030-01-01", "2030-01-02"),
        None,
    )
    .await
    .expect("first imported block");
    let dup = try_block(
        &pool,
        villa,
        "ical_feed",
        Some(f),
        Some("u1"),
        ("2030-02-01", "2030-02-02"),
        None,
    )
    .await
    .expect_err("duplicate uid");
    assert_eq!(crate::error::sqlstate(&dup).as_deref(), Some("23505"));

    try_block(
        &pool,
        villa,
        "manual",
        None,
        None,
        ("2030-03-01", "2030-03-05"),
        None,
    )
    .await
    .expect("first manual block");
    let dup = try_block(
        &pool,
        villa,
        "manual",
        None,
        None,
        ("2030-03-01", "2030-03-05"),
        Some("again"),
    )
    .await
    .expect_err("duplicate manual range");
    assert_eq!(crate::error::sqlstate(&dup).as_deref(), Some("23505"));
    try_block(
        &pool,
        villa,
        "manual",
        None,
        None,
        ("2030-03-02", "2030-03-05"),
        None,
    )
    .await
    .expect("overlapping manual blocks are allowed");
}

#[tokio::test]
async fn deleting_a_feed_cascades_blocks_and_keeps_conflicts() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let host = raw_user(&pool).await;
    let guest = raw_user(&pool).await;
    let villa = raw_listing(&pool, host).await;
    let booking = raw_booking(&pool, guest, villa, "confirmed").await;
    let f = feed(&pool, villa, "https://airbnb.example/a.ics").await;
    let block = try_block(
        &pool,
        villa,
        "ical_feed",
        Some(f),
        Some("u1"),
        ("2030-01-01", "2030-01-02"),
        None,
    )
    .await
    .expect("imported block");
    let conflict: Uuid = sqlx::query_scalar(
        "INSERT INTO calendar_conflict (listing_id, feed_id, feed_name, external_uid, block_id, booking_id,
             kind, block_date_from, block_date_to)
         VALUES ($1, $2, 'Airbnb', 'u1', $3, $4, 'overlap', '2030-01-01', '2030-01-02') RETURNING id",
    )
    .bind(villa)
    .bind(f)
    .bind(block)
    .bind(booking)
    .fetch_one(&pool)
    .await
    .expect("insert conflict");

    sqlx::query("DELETE FROM listing_calendar_feed WHERE id = $1")
        .bind(f)
        .execute(&pool)
        .await
        .expect("delete feed");

    let blocks: i64 =
        sqlx::query_scalar("SELECT count(*) FROM listing_blocked_dates WHERE listing_id = $1")
            .bind(villa)
            .fetch_one(&pool)
            .await
            .expect("count blocks");
    assert_eq!(blocks, 0);
    let (feed_id, block_id, listing_id): (Option<Uuid>, Option<Uuid>, Uuid) =
        sqlx::query_as("SELECT feed_id, block_id, listing_id FROM calendar_conflict WHERE id = $1")
            .bind(conflict)
            .fetch_one(&pool)
            .await
            .expect("conflict survives");
    assert_eq!(feed_id, None);
    assert_eq!(block_id, None);
    assert_eq!(listing_id, villa);
}

#[tokio::test]
async fn acknowledged_by_requires_acknowledged_at() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let host = raw_user(&pool).await;
    let guest = raw_user(&pool).await;
    let villa = raw_listing(&pool, host).await;
    let booking = raw_booking(&pool, guest, villa, "confirmed").await;
    let err = sqlx::query(
        "INSERT INTO calendar_conflict (listing_id, feed_name, external_uid, booking_id, kind,
             block_date_from, block_date_to, acknowledged_by)
         VALUES ($1, 'Airbnb', 'u1', $2, 'overlap', '2030-01-01', '2030-01-02', $3)",
    )
    .bind(villa)
    .bind(booking)
    .bind(host)
    .execute(&pool)
    .await
    .expect_err("acknowledged_by without acknowledged_at");
    assert_eq!(crate::error::sqlstate(&err).as_deref(), Some("23514"));
}
