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

/// Inserts a booking with `status` (`expires_at` as SQL, such as `NULL`).
pub(crate) async fn insert_booking(
    pool: &PgPool,
    listing: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    status: &str,
    expires_at: &str,
) -> Uuid {
    let guest = crate::promo_tests::raw_user(pool).await;
    sqlx::query_scalar(&format!(
        r#"INSERT INTO booking (
            confirmation_code, guest_id, listing_id, status, date_from, date_to, currency,
            daily_rate, number_of_persons, total_days, sub_total_price, total_price,
            cancellation_policy, expires_at)
        VALUES (upper(substr(md5(random()::text), 1, 10)), $1, $2, '{status}'::booking_status,
            $3, $4, 'USD', 100, 2, ($4 - $3), 200, 200, 'flexible', {expires_at})
        RETURNING id"#
    ))
    .bind(guest)
    .bind(listing)
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await
    .expect("insert booking")
}

/// Creates a feed on `listing` and claims it for a manual sync.
pub(crate) async fn claimed_feed(
    pool: &PgPool,
    listing: Uuid,
) -> crate::calendar::feeds::FeedClaim {
    let feed = insert_feed(pool, listing).await;
    claim(pool, listing, feed).await
}

/// Claims `feed` again, whatever its last attempt.
pub(crate) async fn claim(
    pool: &PgPool,
    listing: Uuid,
    feed: Uuid,
) -> crate::calendar::feeds::FeedClaim {
    sqlx::query("UPDATE listing_calendar_feed SET last_attempt_at = NULL WHERE id = $1")
        .bind(feed)
        .execute(pool)
        .await
        .expect("reset attempt");
    crate::calendar::feeds::claim_for_listing(pool, listing, chrono::Duration::seconds(60))
        .await
        .expect("claim")
        .claims
        .into_iter()
        .find(|c| c.id == feed)
        .expect("feed claimed")
}

/// Calendar context for tests.
pub(crate) fn ctx(
    trigger: crate::calendar::reconcile::SyncTrigger,
) -> crate::calendar::reconcile::SyncContext {
    crate::calendar::reconcile::SyncContext {
        trigger,
        admin_base_url: "https://admin.test".to_string(),
    }
}

/// Outbox rows queued with `template`.
pub(crate) async fn outbox_count(pool: &PgPool, template: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM email_outbox WHERE template_id = $1")
        .bind(template)
        .fetch_one(pool)
        .await
        .expect("count outbox")
}

/// A parsed feed with `(uid, from, to)` events.
pub(crate) fn parsed(
    events: &[(&str, NaiveDate, NaiveDate)],
) -> common::calendar::ics_parse::ParsedFeed {
    common::calendar::ics_parse::ParsedFeed {
        events: events
            .iter()
            .map(
                |(uid, from, to)| common::calendar::ics_parse::ExternalEvent {
                    uid: (*uid).to_string(),
                    date_from: *from,
                    date_to: *to,
                },
            )
            .collect(),
        warnings: Vec::new(),
    }
}

/// A date in 2030.
pub(crate) fn d(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2030, month, day).expect("date")
}
