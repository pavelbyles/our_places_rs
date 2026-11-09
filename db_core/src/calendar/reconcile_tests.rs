//! Reconcile and failure recording tests (spec §1.4.2–3).

use super::feeds::{FeedChange, update};
use super::fixtures::{claim, claimed_feed, ctx, d, insert_feed, outbox_count, parsed};
use super::reconcile::{
    FEED_FAILING_TEMPLATE, FailureOutcome, NO_EVENTS_WARNING, ReconcileOutcome, SyncTrigger,
    reconcile_feed, record_failure,
};
use crate::promo_tests::{migrated_db, raw_listing, raw_user};
use chrono::NaiveDate;
use common::calendar::FeedStatus;
use sqlx::PgPool;
use uuid::Uuid;

async fn villa(pool: &PgPool) -> Uuid {
    let host = raw_user(pool).await;
    raw_listing(pool, host).await
}

async fn blocks(pool: &PgPool, feed: Uuid) -> Vec<(String, NaiveDate, NaiveDate)> {
    sqlx::query_as(
        "SELECT external_uid, date_from, date_to FROM listing_blocked_dates
         WHERE feed_id = $1 ORDER BY external_uid",
    )
    .bind(feed)
    .fetch_all(pool)
    .await
    .expect("blocks")
}

async fn updated_at(pool: &PgPool, feed: Uuid) -> Vec<chrono::DateTime<chrono::Utc>> {
    sqlx::query_scalar(
        "SELECT updated_at FROM listing_blocked_dates WHERE feed_id = $1 ORDER BY external_uid",
    )
    .bind(feed)
    .fetch_all(pool)
    .await
    .expect("updated_at")
}

async fn feed_state(
    pool: &PgPool,
    feed: Uuid,
) -> (FeedStatus, Option<String>, Option<String>, i32, i32) {
    let row: (FeedStatus, Option<String>, Option<String>, i32, i32) = sqlx::query_as(
        "SELECT last_status, last_error, last_warning, consecutive_failures, event_count
         FROM listing_calendar_feed WHERE id = $1",
    )
    .bind(feed)
    .fetch_one(pool)
    .await
    .expect("feed");
    row
}

fn manual() -> super::reconcile::SyncContext {
    ctx(SyncTrigger::Manual)
}

fn scheduled() -> super::reconcile::SyncContext {
    ctx(SyncTrigger::Scheduled)
}

#[tokio::test]
async fn inserts_changes_and_removals_are_applied_and_a_rerun_changes_nothing() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    let c = claimed_feed(&pool, v).await;

    let first = parsed(&[("a", d(1, 1), d(1, 5)), ("b", d(2, 1), d(2, 3))]);
    let out = reconcile_feed(&pool, &c, &first, &manual())
        .await
        .expect("reconcile");
    assert!(
        matches!(
            out,
            ReconcileOutcome::Applied {
                status: FeedStatus::Ok,
                events: 2,
                ..
            }
        ),
        "{out:?}"
    );
    assert_eq!(
        blocks(&pool, c.id).await,
        vec![
            ("a".into(), d(1, 1), d(1, 5)),
            ("b".into(), d(2, 1), d(2, 3))
        ]
    );

    let stamps = updated_at(&pool, c.id).await;
    reconcile_feed(&pool, &c, &first, &manual())
        .await
        .expect("rerun");
    assert_eq!(
        updated_at(&pool, c.id).await,
        stamps,
        "an unchanged feed rewrites nothing"
    );

    let second = parsed(&[("a", d(1, 2), d(1, 6)), ("c", d(3, 1), d(3, 2))]);
    reconcile_feed(&pool, &c, &second, &manual())
        .await
        .expect("reconcile");
    assert_eq!(
        blocks(&pool, c.id).await,
        vec![
            ("a".into(), d(1, 2), d(1, 6)),
            ("c".into(), d(3, 1), d(3, 2))
        ]
    );
    let (status, error, warning, failures, count) = feed_state(&pool, c.id).await;
    assert_eq!(
        (status, error, warning, failures, count),
        (FeedStatus::Ok, None, None, 0, 2)
    );
}

#[tokio::test]
async fn an_empty_feed_releases_every_block_and_warns() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    let c = claimed_feed(&pool, v).await;
    reconcile_feed(&pool, &c, &parsed(&[("a", d(1, 1), d(1, 5))]), &manual())
        .await
        .expect("reconcile");
    let out = reconcile_feed(&pool, &c, &parsed(&[]), &manual())
        .await
        .expect("empty");
    assert!(blocks(&pool, c.id).await.is_empty());
    match out {
        ReconcileOutcome::Applied {
            status, warning, ..
        } => {
            assert_eq!(status, FeedStatus::Warning);
            assert_eq!(warning.as_deref(), Some(NO_EVENTS_WARNING));
        }
        other => panic!("{other:?}"),
    }
    // Empty again, with nothing left to release: a plain success.
    let out = reconcile_feed(&pool, &c, &parsed(&[]), &manual())
        .await
        .expect("empty");
    assert!(
        matches!(
            out,
            ReconcileOutcome::Applied {
                status: FeedStatus::Ok,
                ..
            }
        ),
        "{out:?}"
    );
}

#[tokio::test]
async fn a_failure_keeps_the_blocks() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    let c = claimed_feed(&pool, v).await;
    reconcile_feed(&pool, &c, &parsed(&[("a", d(1, 1), d(1, 5))]), &manual())
        .await
        .expect("reconcile");
    let out = record_failure(&pool, &c, "calendar server returned HTTP 503", &manual())
        .await
        .expect("failure");
    assert!(
        matches!(
            out,
            FailureOutcome::Recorded {
                consecutive_failures: 0,
                ..
            }
        ),
        "{out:?}"
    );
    assert_eq!(blocks(&pool, c.id).await.len(), 1);
    let (status, error, ..) = feed_state(&pool, c.id).await;
    assert_eq!(status, FeedStatus::Failed);
    assert_eq!(error.as_deref(), Some("calendar server returned HTTP 503"));
}

#[tokio::test]
async fn four_scheduled_failures_alert_once_and_a_success_rearms() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    let c = claimed_feed(&pool, v).await;

    for _ in 0..3 {
        record_failure(&pool, &c, "timeout", &manual())
            .await
            .expect("manual failure");
    }
    assert_eq!(
        feed_state(&pool, c.id).await.3,
        0,
        "manual failures don't count"
    );

    for _ in 0..6 {
        record_failure(&pool, &c, "timeout", &scheduled())
            .await
            .expect("failure");
    }
    assert_eq!(feed_state(&pool, c.id).await.3, 6);
    assert_eq!(outbox_count(&pool, FEED_FAILING_TEMPLATE).await, 1);

    reconcile_feed(&pool, &c, &parsed(&[]), &scheduled())
        .await
        .expect("success");
    assert_eq!(feed_state(&pool, c.id).await.3, 0);
    for _ in 0..4 {
        record_failure(&pool, &c, "timeout", &scheduled())
            .await
            .expect("failure");
    }
    assert_eq!(outbox_count(&pool, FEED_FAILING_TEMPLATE).await, 2);

    let payload: serde_json::Value =
        sqlx::query_scalar("SELECT payload FROM email_outbox WHERE template_id = $1 LIMIT 1")
            .bind(FEED_FAILING_TEMPLATE)
            .fetch_one(&pool)
            .await
            .expect("payload");
    assert_eq!(payload["error_code"], "timeout");
    assert!(
        payload["calendar_url"]
            .as_str()
            .is_some_and(|u| u.contains(&v.to_string()))
    );
    assert!(
        !payload.to_string().contains("feeds.test"),
        "no feed URL in the email"
    );
}

#[tokio::test]
async fn stale_claims_change_nothing() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    let events = parsed(&[("a", d(1, 1), d(1, 5))]);

    // URL changed during the fetch.
    let c = claimed_feed(&pool, v).await;
    update(
        &pool,
        v,
        c.id,
        &FeedChange {
            url: Some("https://feeds.test/moved.ics"),
            ..Default::default()
        },
    )
    .await
    .expect("move");
    assert_eq!(
        reconcile_feed(&pool, &c, &events, &manual())
            .await
            .expect("r"),
        ReconcileOutcome::Stale
    );
    assert_eq!(
        record_failure(&pool, &c, "x", &scheduled())
            .await
            .expect("f"),
        FailureOutcome::Stale
    );
    assert!(blocks(&pool, c.id).await.is_empty());
    assert_eq!(feed_state(&pool, c.id).await.0, FeedStatus::Never);

    // Disabled during the fetch.
    let c = claimed_feed(&pool, v).await;
    update(
        &pool,
        v,
        c.id,
        &FeedChange {
            is_active: Some(false),
            ..Default::default()
        },
    )
    .await
    .expect("disable");
    assert_eq!(
        reconcile_feed(&pool, &c, &events, &manual())
            .await
            .expect("r"),
        ReconcileOutcome::Stale
    );
    assert_eq!(
        record_failure(&pool, &c, "x", &scheduled())
            .await
            .expect("f"),
        FailureOutcome::Stale
    );

    // Deleted during the fetch.
    let c = claimed_feed(&pool, v).await;
    super::feeds::delete(&pool, v, c.id).await.expect("delete");
    assert_eq!(
        reconcile_feed(&pool, &c, &events, &manual())
            .await
            .expect("r"),
        ReconcileOutcome::Stale
    );
    assert_eq!(
        record_failure(&pool, &c, "x", &scheduled())
            .await
            .expect("f"),
        FailureOutcome::Stale
    );
    let leftover: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM listing_blocked_dates WHERE feed_id = $1")
            .bind(c.id)
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(leftover, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lock_held_too_long_defers() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    let c = claimed_feed(&pool, v).await;
    let mut holder = pool.begin().await.expect("tx");
    sqlx::query("SELECT id FROM listing WHERE id = $1 FOR UPDATE")
        .bind(v)
        .execute(&mut *holder)
        .await
        .expect("lock listing");
    let out = reconcile_feed(&pool, &c, &parsed(&[("a", d(1, 1), d(1, 5))]), &scheduled())
        .await
        .expect("reconcile");
    assert_eq!(out, ReconcileOutcome::Deferred);
    holder.rollback().await.expect("release");
    let (status, _, _, failures, _) = feed_state(&pool, c.id).await;
    assert_eq!((status, failures), (FeedStatus::Never, 0));
    assert!(blocks(&pool, c.id).await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn two_reconciles_of_one_feed_run_one_after_the_other() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    let c = claimed_feed(&pool, v).await;
    let events = parsed(&[("a", d(1, 1), d(1, 5)), ("b", d(2, 1), d(2, 3))]);
    let runs: Vec<_> = (0..2)
        .map(|_| {
            let (pool, c, events) = (pool.clone(), c.clone(), events.clone());
            tokio::spawn(async move { reconcile_feed(&pool, &c, &events, &manual()).await })
        })
        .collect();
    for r in runs {
        let out = r.await.expect("join").expect("reconcile");
        assert!(matches!(out, ReconcileOutcome::Applied { .. }), "{out:?}");
    }
    assert_eq!(blocks(&pool, c.id).await.len(), 2);
}

#[tokio::test]
async fn normalised_uids_never_violate_a_constraint() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let v = villa(&pool).await;
    let feed = insert_feed(&pool, v).await;
    let c = claim(&pool, v, feed).await;
    let long = "x".repeat(600);
    let body = format!(
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:{long}\r\nDTSTART;VALUE=DATE:20300101\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:dup\r\nDTSTART;VALUE=DATE:20300102\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:dup\r\nDTSTART;VALUE=DATE:20300103\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:dup#2\r\nDTSTART;VALUE=DATE:20300104\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20300105\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20300105\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    );
    let feed_parsed = common::calendar::ics_parse::parse_feed(
        body.as_bytes(),
        feed,
        NaiveDate::from_ymd_opt(2029, 12, 1).expect("today"),
    )
    .expect("parse");
    let out = reconcile_feed(&pool, &c, &feed_parsed, &manual())
        .await
        .expect("reconcile");
    assert!(
        matches!(out, ReconcileOutcome::Applied { events: 6, .. }),
        "{out:?}"
    );
}

#[test]
fn template_ids_match_the_email_templates() {
    use common::email::EmailTemplate;
    assert_eq!(
        FEED_FAILING_TEMPLATE,
        EmailTemplate::CalendarFeedFailingHost.as_str()
    );
    assert_eq!(
        super::conflicts::CONFLICT_TEMPLATE,
        EmailTemplate::CalendarConflictHost.as_str()
    );
}
