//! Feed storage and claiming tests (spec §1.1, §1.4.4).

use super::feeds::{
    FeedChange, NewFeed, claim_due, claim_for_listing, create, delete, get, list, release_claim,
    update,
};
use crate::error::DbError;
use crate::promo_tests::{migrated_db, raw_listing, raw_user};
use chrono::Duration;
use common::calendar::FeedStatus;
use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

const SECRET_URL: &str = "https://www.airbnb.com/calendar/ical/123.ics?s=topsecret";

fn new_feed<'a>(listing: Uuid, url: &'a str) -> NewFeed<'a> {
    NewFeed {
        listing_id: listing,
        name: "Airbnb",
        url,
        created_by: None,
    }
}

async fn villa(pool: &PgPool) -> Uuid {
    let host = raw_user(pool).await;
    raw_listing(pool, host).await
}

#[tokio::test]
async fn a_sixth_feed_is_refused() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    for i in 0..5 {
        let url = format!("https://feeds.test/{i}.ics");
        create(&pool, &new_feed(v, &url)).await.expect("feed");
    }
    let sixth = create(&pool, &new_feed(v, "https://feeds.test/6.ics")).await;
    assert!(
        matches!(sixth, Err(DbError::CalendarFeedLimit)),
        "{sixth:?}"
    );
    assert_eq!(list(&pool, v).await.expect("list").len(), 5);
}

#[tokio::test]
async fn a_duplicate_url_is_refused_but_other_listings_may_reuse_it() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    let other = villa(&pool).await;
    create(&pool, &new_feed(v, SECRET_URL)).await.expect("feed");
    let dup = create(&pool, &new_feed(v, SECRET_URL)).await;
    assert!(
        matches!(dup, Err(DbError::CalendarFeedDuplicate)),
        "{dup:?}"
    );
    create(&pool, &new_feed(other, SECRET_URL))
        .await
        .expect("other listing");
}

/// Concurrent creates serialise on the listing lock, and a racing writer's
/// FK lock conflicts with it too, so the pre-check normally wins. The unique
/// index is the backstop: a real violation still maps to `Duplicate`, and the
/// mapping happens before anything logs the error (whose detail has the URL).
#[tokio::test]
async fn a_unique_violation_maps_to_duplicate() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    create(&pool, &new_feed(v, SECRET_URL)).await.expect("feed");
    let err = sqlx::query(
        "INSERT INTO listing_calendar_feed (listing_id, name, url) VALUES ($1, 'X', $2)",
    )
    .bind(v)
    .bind(SECRET_URL)
    .execute(&pool)
    .await
    .expect_err("unique violation");
    let mapped = super::feeds::map_unique(err);
    assert!(
        matches!(mapped, DbError::CalendarFeedDuplicate),
        "{mapped:?}"
    );
    assert!(!format!("{mapped} {mapped:?}").contains("topsecret"));
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_creates_of_one_url_give_one_feed() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    let tasks: Vec<_> = (0..4)
        .map(|_| {
            let pool = pool.clone();
            tokio::spawn(async move { create(&pool, &new_feed(v, SECRET_URL)).await })
        })
        .collect();
    let mut ok = 0;
    for t in tasks {
        match t.await.expect("join") {
            Ok(_) => ok += 1,
            Err(DbError::CalendarFeedDuplicate) => {}
            Err(other) => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(ok, 1);
}

#[tokio::test]
async fn another_listings_feed_is_invisible() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let a = villa(&pool).await;
    let b = villa(&pool).await;
    let feed = create(&pool, &new_feed(a, SECRET_URL)).await.expect("feed");
    assert!(get(&pool, b, feed.id).await.expect("get").is_none());
    let change = FeedChange {
        name: Some("Hijack"),
        ..Default::default()
    };
    assert!(
        update(&pool, b, feed.id, &change)
            .await
            .expect("update")
            .is_none()
    );
    assert!(!delete(&pool, b, feed.id).await.expect("delete"));
    assert!(get(&pool, a, feed.id).await.expect("get").is_some());
}

#[tokio::test]
async fn a_url_change_bumps_the_generation_and_resets_the_status() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    let feed = create(&pool, &new_feed(v, SECRET_URL)).await.expect("feed");
    sqlx::query(
        "UPDATE listing_calendar_feed SET last_status = 'failed', last_error = 'x',
            consecutive_failures = 3, last_attempt_at = NOW() WHERE id = $1",
    )
    .bind(feed.id)
    .execute(&pool)
    .await
    .expect("seed status");

    let renamed = update(
        &pool,
        v,
        feed.id,
        &FeedChange {
            name: Some("VRBO"),
            ..Default::default()
        },
    )
    .await
    .expect("update")
    .expect("found");
    assert!(!renamed.url_changed);
    assert_eq!(renamed.feed.sync_generation, 0);
    assert!(renamed.feed.last_attempt_at.is_some());

    let moved = update(
        &pool,
        v,
        feed.id,
        &FeedChange {
            url: Some("https://feeds.test/new.ics"),
            ..Default::default()
        },
    )
    .await
    .expect("update")
    .expect("found");
    assert!(moved.url_changed);
    assert_eq!(moved.feed.sync_generation, 1);
    assert_eq!(moved.feed.last_attempt_at, None);
    assert_eq!(moved.feed.last_status, FeedStatus::Never);
    assert_eq!(moved.feed.last_error, None);
    assert_eq!(moved.feed.consecutive_failures, 0);
    assert_eq!(moved.feed.name, "VRBO");

    // Same URL again: no change, no bump.
    let same = update(
        &pool,
        v,
        feed.id,
        &FeedChange {
            url: Some("https://feeds.test/new.ics"),
            ..Default::default()
        },
    )
    .await
    .expect("update")
    .expect("found");
    assert!(!same.url_changed);
    assert_eq!(same.feed.sync_generation, 1);
}

#[tokio::test]
async fn a_url_change_onto_another_feed_is_a_duplicate() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    create(&pool, &new_feed(v, "https://feeds.test/a.ics"))
        .await
        .expect("a");
    let b = create(&pool, &new_feed(v, "https://feeds.test/b.ics"))
        .await
        .expect("b");
    let result = update(
        &pool,
        v,
        b.id,
        &FeedChange {
            url: Some("https://feeds.test/a.ics"),
            ..Default::default()
        },
    )
    .await;
    assert!(
        matches!(result, Err(DbError::CalendarFeedDuplicate)),
        "{result:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_claims_never_share_a_feed() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let mut all = HashSet::new();
    for _ in 0..6 {
        let v = villa(&pool).await;
        for i in 0..5 {
            let url = format!("https://feeds.test/{i}.ics");
            all.insert(create(&pool, &new_feed(v, &url)).await.expect("feed").id);
        }
    }
    let claimers: Vec<_> = (0..4)
        .map(|_| {
            let pool = pool.clone();
            tokio::spawn(async move { claim_due(&pool, Duration::minutes(10), 12).await })
        })
        .collect();
    let mut seen = HashSet::new();
    for c in claimers {
        for claim in c.await.expect("join").expect("claim") {
            assert!(seen.insert(claim.id), "claimed twice: {}", claim.id);
        }
    }
    assert!(seen.is_subset(&all));
    assert!(!seen.is_empty());
}

#[tokio::test]
async fn a_claim_inside_the_interval_returns_nothing_and_release_restores_it() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    let feed = create(&pool, &new_feed(v, SECRET_URL)).await.expect("feed");

    let first = claim_due(&pool, Duration::minutes(10), 32)
        .await
        .expect("claim");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].previous_attempt_at, None);
    assert_eq!(first[0].url, SECRET_URL);
    assert!(
        claim_due(&pool, Duration::minutes(10), 32)
            .await
            .expect("claim")
            .is_empty()
    );

    assert!(release_claim(&pool, &first[0]).await.expect("release"));
    let again = claim_due(&pool, Duration::minutes(10), 32)
        .await
        .expect("claim");
    assert_eq!(
        again.iter().map(|c| c.id).collect::<Vec<_>>(),
        vec![feed.id]
    );

    // The first claim is stale now: releasing it doesn't undo the newer one.
    assert!(!release_claim(&pool, &first[0]).await.expect("release"));
    assert!(
        claim_due(&pool, Duration::minutes(10), 32)
            .await
            .expect("claim")
            .is_empty()
    );
}

#[tokio::test]
async fn a_url_change_makes_the_feed_claimable_at_once() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    let feed = create(&pool, &new_feed(v, SECRET_URL)).await.expect("feed");
    let claim = claim_for_listing(&pool, v, Duration::seconds(60))
        .await
        .expect("claim");
    assert_eq!(claim.claims.len(), 1);
    let blocked = claim_for_listing(&pool, v, Duration::seconds(60))
        .await
        .expect("claim");
    assert!(blocked.claims.is_empty());
    assert_eq!(blocked.skipped, vec![feed.id]);
    let wait = blocked.retry_after_secs.expect("retry after");
    assert!((1..=60).contains(&wait), "{wait}");

    update(
        &pool,
        v,
        feed.id,
        &FeedChange {
            url: Some("https://feeds.test/new.ics"),
            ..Default::default()
        },
    )
    .await
    .expect("update");
    let after = claim_for_listing(&pool, v, Duration::seconds(60))
        .await
        .expect("claim");
    assert_eq!(after.claims.len(), 1);
    assert_eq!(after.claims[0].sync_generation, 1);
    assert_eq!(after.claims[0].url, "https://feeds.test/new.ics");
}

#[tokio::test]
async fn deleted_listings_and_disabled_feeds_are_not_claimed() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let gone = villa(&pool).await;
    create(&pool, &new_feed(gone, SECRET_URL))
        .await
        .expect("feed");
    sqlx::query("UPDATE listing SET deleted_at = NOW() WHERE id = $1")
        .bind(gone)
        .execute(&pool)
        .await
        .expect("delete listing");
    let v = villa(&pool).await;
    let off = create(&pool, &new_feed(v, SECRET_URL)).await.expect("feed");
    update(
        &pool,
        v,
        off.id,
        &FeedChange {
            is_active: Some(false),
            ..Default::default()
        },
    )
    .await
    .expect("disable");
    assert!(
        claim_due(&pool, Duration::minutes(10), 32)
            .await
            .expect("claim")
            .is_empty()
    );
    let listing = claim_for_listing(&pool, v, Duration::seconds(60))
        .await
        .expect("claim");
    assert!(listing.claims.is_empty() && listing.skipped.is_empty());
    assert_eq!(listing.retry_after_secs, None);
}

#[tokio::test]
async fn debug_output_redacts_the_url() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    let feed = create(&pool, &new_feed(v, SECRET_URL)).await.expect("feed");
    let claims = claim_due(&pool, Duration::minutes(10), 32)
        .await
        .expect("claim");
    for text in [format!("{feed:?}"), format!("{claims:?}")] {
        assert!(text.contains("www.airbnb.com"), "{text}");
        assert!(
            !text.contains("topsecret") && !text.contains("/calendar/"),
            "{text}"
        );
    }
}
