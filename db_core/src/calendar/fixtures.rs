//! Test fixtures for calendar feeds and blocked dates.

use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

/// Inserts an active feed on `listing` with a unique URL.
pub(crate) async fn insert_feed(pool: &PgPool, listing: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO listing_calendar_feed (listing_id, name, url)
         VALUES ($1, 'Airbnb', 'https://feeds.test/' || gen_random_uuid() || '.ics')
         RETURNING id",
    )
    .bind(listing)
    .fetch_one(pool)
    .await
    .expect("insert feed")
}

/// Inserts a block imported from `feed` (on `listing`).
pub(crate) async fn insert_imported_block(
    pool: &PgPool,
    listing: Uuid,
    feed: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO listing_blocked_dates (listing_id, source, feed_id, external_uid, date_from, date_to)
         VALUES ($1, 'ical_feed', $2, gen_random_uuid()::text || '@airbnb.com', $3, $4)
         RETURNING id",
    )
    .bind(listing)
    .bind(feed)
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await
    .expect("insert imported block")
}

/// Inserts a manual block on `listing`.
pub(crate) async fn insert_manual_block(
    pool: &PgPool,
    listing: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO listing_blocked_dates (listing_id, source, date_from, date_to)
         VALUES ($1, 'manual', $2, $3)
         RETURNING id",
    )
    .bind(listing)
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await
    .expect("insert manual block")
}

/// Which kind of block a test seeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlockKind {
    /// From a feed: widened by the gap.
    Imported,
    /// By hand: never widened.
    Manual,
}

/// Inserts a block of `kind`, creating a feed for imported ones.
pub(crate) async fn insert_block(
    pool: &PgPool,
    listing: Uuid,
    kind: BlockKind,
    from: NaiveDate,
    to: NaiveDate,
) -> Uuid {
    match kind {
        BlockKind::Imported => {
            let feed = insert_feed(pool, listing).await;
            insert_imported_block(pool, listing, feed, from, to).await
        }
        BlockKind::Manual => insert_manual_block(pool, listing, from, to).await,
    }
}
