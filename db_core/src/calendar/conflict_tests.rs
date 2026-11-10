//! Conflict detection, reopening, staleness and acknowledgement (spec §1.5).

use super::conflicts::{CONFLICT_TEMPLATE, acknowledge, list_conflicts, overview_for_user};
use super::feeds::FeedClaim;
use super::fixtures::{
    claim, claimed_feed, ctx, d, insert_booking, insert_feed, outbox_count, parsed,
};
use super::reconcile::{ReconcileOutcome, SyncTrigger, reconcile_feed};
use crate::promo_tests::{migrated_db, raw_listing, raw_user};
use chrono::NaiveDate;
use common::calendar::ConflictKind;
use common::calendar::conflict::StaleReason;
use sqlx::PgPool;
use uuid::Uuid;

const LIVE: &str = "NOW() + INTERVAL '1 hour'";
const LAPSED: &str = "NOW() - INTERVAL '1 hour'";

struct Setup {
    pool: PgPool,
    host: Uuid,
    villa: Uuid,
    claim: FeedClaim,
    _db: sqlx_db_tester::TestPg,
}

async fn setup(gap: i32) -> Setup {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let host = raw_user(&pool).await;
    let villa = raw_listing(&pool, host).await;
    sqlx::query("UPDATE listing SET days_between_bookings = $1 WHERE id = $2")
        .bind(gap)
        .bind(villa)
        .execute(&pool)
        .await
        .expect("gap");
    let claim = claimed_feed(&pool, villa).await;
    Setup {
        pool,
        host,
        villa,
        claim,
        _db: db,
    }
}

async fn sync(s: &Setup, events: &[(&str, NaiveDate, NaiveDate)]) -> i32 {
    match reconcile_feed(
        &s.pool,
        &s.claim,
        &parsed(events),
        &ctx(SyncTrigger::Manual),
    )
    .await
    .expect("reconcile")
    {
        ReconcileOutcome::Applied {
            conflicts_opened, ..
        } => conflicts_opened,
        other => panic!("{other:?}"),
    }
}

async fn emails(s: &Setup) -> i64 {
    outbox_count(&s.pool, CONFLICT_TEMPLATE).await
}

async fn ack_all(s: &Setup) {
    for c in list_conflicts(&s.pool, s.villa, 1)
        .await
        .expect("list")
        .open
    {
        assert!(
            acknowledge(&s.pool, s.villa, c.id, s.host)
                .await
                .expect("ack")
        );
    }
}

#[tokio::test]
async fn overlap_and_turnover_are_detected_and_an_expired_hold_is_not() {
    let s = setup(1).await;
    insert_booking(&s.pool, s.villa, d(1, 10), d(1, 15), "confirmed", "NULL").await;
    insert_booking(&s.pool, s.villa, d(2, 10), d(2, 15), "pending", LIVE).await;
    insert_booking(&s.pool, s.villa, d(3, 10), d(3, 15), "pending", LAPSED).await;
    let opened = sync(
        &s,
        &[
            ("over", d(1, 12), d(1, 13)),
            ("turn", d(2, 15), d(2, 17)),
            ("expired", d(3, 11), d(3, 12)),
            ("clear", d(4, 1), d(4, 3)),
        ],
    )
    .await;
    assert_eq!(opened, 2);
    let open = list_conflicts(&s.pool, s.villa, 1)
        .await
        .expect("list")
        .open;
    let mut kinds: Vec<_> = open.iter().map(|c| (c.block_date_from, c.kind)).collect();
    kinds.sort_by_key(|k| k.0);
    assert_eq!(
        kinds,
        vec![
            (d(1, 12), ConflictKind::Overlap),
            (d(2, 15), ConflictKind::Turnover)
        ]
    );
    assert!(open.iter().all(|c| c.stale_reason.is_none() && !c.reopened));
    assert_eq!(emails(&s).await, 2);

    // A re-sync of the same feed changes nothing and sends nothing.
    let again = sync(
        &s,
        &[
            ("over", d(1, 12), d(1, 13)),
            ("turn", d(2, 15), d(2, 17)),
            ("expired", d(3, 11), d(3, 12)),
            ("clear", d(4, 1), d(4, 3)),
        ],
    )
    .await;
    assert_eq!(again, 0);
    assert_eq!(emails(&s).await, 2);
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM calendar_conflict")
        .fetch_one(&s.pool)
        .await
        .expect("count");
    assert_eq!(rows, 2);
}

#[tokio::test]
async fn the_email_has_the_facts_and_no_feed_url() {
    let s = setup(0).await;
    insert_booking(&s.pool, s.villa, d(1, 10), d(1, 15), "confirmed", "NULL").await;
    sync(&s, &[("over", d(1, 12), d(1, 13))]).await;
    let payload: serde_json::Value =
        sqlx::query_scalar("SELECT payload FROM email_outbox WHERE template_id = $1")
            .bind(CONFLICT_TEMPLATE)
            .fetch_one(&s.pool)
            .await
            .expect("payload");
    assert_eq!(payload["kind"], "overlap");
    assert_eq!(payload["reopened"], "false");
    assert_eq!(payload["date_from"], "2030-01-12");
    assert_eq!(payload["feed_name"], "Airbnb");
    assert!(
        payload["confirmation_code"]
            .as_str()
            .is_some_and(|c| !c.is_empty())
    );
    assert!(!payload.to_string().contains("feeds.test"));
}

#[tokio::test]
async fn removing_then_re_adding_the_event_reopens_with_one_email() {
    let s = setup(0).await;
    insert_booking(&s.pool, s.villa, d(1, 10), d(1, 15), "confirmed", "NULL").await;
    sync(&s, &[("over", d(1, 12), d(1, 13))]).await;
    sync(&s, &[]).await;
    let open = list_conflicts(&s.pool, s.villa, 1)
        .await
        .expect("list")
        .open;
    assert_eq!(open[0].stale_reason, Some(StaleReason::EventRemoved));
    assert_eq!(emails(&s).await, 1, "going stale sends nothing");

    assert_eq!(sync(&s, &[("over", d(1, 12), d(1, 13))]).await, 1);
    assert_eq!(emails(&s).await, 2);
    let open = list_conflicts(&s.pool, s.villa, 1)
        .await
        .expect("list")
        .open;
    assert_eq!(open.len(), 1);
    assert!(open[0].reopened && open[0].stale_reason.is_none());
}

#[tokio::test]
async fn the_same_uid_on_two_feeds_gives_two_conflicts() {
    let s = setup(0).await;
    insert_booking(&s.pool, s.villa, d(1, 10), d(1, 15), "confirmed", "NULL").await;
    let other_feed = insert_feed(&s.pool, s.villa).await;
    let other = claim(&s.pool, s.villa, other_feed).await;
    sync(&s, &[("same", d(1, 12), d(1, 13))]).await;
    reconcile_feed(
        &s.pool,
        &other,
        &parsed(&[("same", d(1, 12), d(1, 13))]),
        &ctx(SyncTrigger::Manual),
    )
    .await
    .expect("reconcile");
    assert_eq!(
        list_conflicts(&s.pool, s.villa, 1)
            .await
            .expect("list")
            .open
            .len(),
        2
    );
    assert_eq!(emails(&s).await, 2);
}

#[tokio::test]
async fn turnover_becoming_overlap_reopens_an_acknowledged_conflict() {
    let s = setup(2).await;
    insert_booking(&s.pool, s.villa, d(1, 10), d(1, 15), "confirmed", "NULL").await;
    sync(&s, &[("e", d(1, 16), d(1, 18))]).await;
    ack_all(&s).await;
    assert!(
        list_conflicts(&s.pool, s.villa, 1)
            .await
            .expect("list")
            .open
            .is_empty()
    );
    // Shrinking within the old range changes the row but sends nothing.
    sync(&s, &[("e", d(1, 16), d(1, 17))]).await;
    assert_eq!(emails(&s).await, 1);
    assert!(
        list_conflicts(&s.pool, s.villa, 1)
            .await
            .expect("list")
            .open
            .is_empty()
    );

    assert_eq!(sync(&s, &[("e", d(1, 14), d(1, 17))]).await, 1);
    assert_eq!(emails(&s).await, 2);
    let open = list_conflicts(&s.pool, s.villa, 1)
        .await
        .expect("list")
        .open;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].kind, ConflictKind::Overlap);
    assert!(open[0].reopened);
    let payload: serde_json::Value = sqlx::query_scalar(
        "SELECT payload FROM email_outbox WHERE template_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(CONFLICT_TEMPLATE)
    .fetch_one(&s.pool)
    .await
    .expect("payload");
    assert_eq!(payload["reopened"], "true");
}

#[tokio::test]
async fn acknowledged_while_clear_then_back_on_the_stay_reopens() {
    let s = setup(0).await;
    insert_booking(&s.pool, s.villa, d(1, 10), d(1, 15), "confirmed", "NULL").await;
    sync(&s, &[("e", d(1, 12), d(1, 13))]).await;
    sync(&s, &[("e", d(2, 1), d(2, 3))]).await;
    let open = list_conflicts(&s.pool, s.villa, 1)
        .await
        .expect("list")
        .open;
    assert_eq!(open[0].stale_reason, Some(StaleReason::NoLongerOverlapping));
    ack_all(&s).await;
    assert_eq!(emails(&s).await, 1);

    assert_eq!(sync(&s, &[("e", d(1, 12), d(1, 13))]).await, 1);
    assert_eq!(emails(&s).await, 2);
    let open = list_conflicts(&s.pool, s.villa, 1)
        .await
        .expect("list")
        .open;
    assert_eq!(open.len(), 1);
    assert!(open[0].reopened && open[0].acknowledged_at.is_none());
}

#[tokio::test]
async fn a_released_booking_makes_the_conflict_stale() {
    let s = setup(0).await;
    let booking = insert_booking(&s.pool, s.villa, d(1, 10), d(1, 15), "pending", LIVE).await;
    sync(&s, &[("e", d(1, 12), d(1, 13))]).await;
    sqlx::query("UPDATE booking SET expires_at = NOW() - INTERVAL '1 minute' WHERE id = $1")
        .bind(booking)
        .execute(&s.pool)
        .await
        .expect("expire");
    let open = list_conflicts(&s.pool, s.villa, 1)
        .await
        .expect("list")
        .open;
    assert_eq!(open[0].stale_reason, Some(StaleReason::BookingReleased));
    sync(&s, &[("e", d(1, 12), d(1, 13))]).await;
    assert_eq!(emails(&s).await, 1, "no email when it goes stale");
}

#[tokio::test]
async fn a_gap_change_is_applied_on_read() {
    let s = setup(1).await;
    insert_booking(&s.pool, s.villa, d(1, 10), d(1, 15), "confirmed", "NULL").await;
    sync(&s, &[("e", d(1, 15), d(1, 17))]).await;
    sqlx::query("UPDATE listing SET days_between_bookings = 0 WHERE id = $1")
        .bind(s.villa)
        .execute(&s.pool)
        .await
        .expect("gap");
    let open = list_conflicts(&s.pool, s.villa, 1)
        .await
        .expect("list")
        .open;
    assert_eq!(open[0].stale_reason, Some(StaleReason::NoLongerOverlapping));
}

#[tokio::test]
async fn acknowledging_is_idempotent_and_scoped_to_the_listing() {
    let s = setup(0).await;
    insert_booking(&s.pool, s.villa, d(1, 10), d(1, 15), "confirmed", "NULL").await;
    sync(&s, &[("e", d(1, 12), d(1, 13))]).await;
    let id = list_conflicts(&s.pool, s.villa, 1)
        .await
        .expect("list")
        .open[0]
        .id;
    let other_host = raw_user(&s.pool).await;
    let other_villa = raw_listing(&s.pool, other_host).await;
    assert!(
        !acknowledge(&s.pool, other_villa, id, other_host)
            .await
            .expect("ack")
    );
    assert!(
        acknowledge(&s.pool, s.villa, id, s.host)
            .await
            .expect("ack")
    );
    let first = list_conflicts(&s.pool, s.villa, 1)
        .await
        .expect("list")
        .history
        .items[0]
        .clone();
    assert!(
        acknowledge(&s.pool, s.villa, id, other_host)
            .await
            .expect("again")
    );
    let second = list_conflicts(&s.pool, s.villa, 1)
        .await
        .expect("list")
        .history
        .items[0]
        .clone();
    assert_eq!(first.acknowledged_at, second.acknowledged_at);
    assert_eq!(second.acknowledged_by, Some(s.host));
}

#[tokio::test]
async fn history_is_paginated_newest_first() {
    let s = setup(0).await;
    let mut events = Vec::new();
    for i in 0..25u32 {
        let from = d(1, 1) + chrono::Duration::days(i64::from(i) * 3);
        insert_booking(
            &s.pool,
            s.villa,
            from,
            from + chrono::Duration::days(2),
            "confirmed",
            "NULL",
        )
        .await;
        events.push((format!("e{i}"), from, from + chrono::Duration::days(1)));
    }
    let refs: Vec<(&str, NaiveDate, NaiveDate)> = events
        .iter()
        .map(|(u, f, t)| (u.as_str(), *f, *t))
        .collect();
    assert_eq!(sync(&s, &refs).await, 25);
    ack_all(&s).await;
    let p1 = list_conflicts(&s.pool, s.villa, 1)
        .await
        .expect("p1")
        .history;
    let p2 = list_conflicts(&s.pool, s.villa, 2)
        .await
        .expect("p2")
        .history;
    assert_eq!(
        (p1.items.len(), p2.items.len(), p1.total, p1.per_page),
        (20, 5, 25, 20)
    );
    assert!(
        p1.items
            .windows(2)
            .all(|w| w[0].acknowledged_at >= w[1].acknowledged_at)
    );
}

#[tokio::test]
async fn the_overview_counts_live_conflicts_and_failing_feeds_for_the_caller() {
    let s = setup(0).await;
    insert_booking(&s.pool, s.villa, d(1, 10), d(1, 15), "confirmed", "NULL").await;
    sync(
        &s,
        &[("live", d(1, 12), d(1, 13)), ("gone", d(1, 11), d(1, 12))],
    )
    .await;
    sync(&s, &[("live", d(1, 12), d(1, 13))]).await; // "gone" goes stale
    sqlx::query("UPDATE listing_calendar_feed SET consecutive_failures = 4 WHERE id = $1")
        .bind(s.claim.id)
        .execute(&s.pool)
        .await
        .expect("failing");
    let mine = overview_for_user(&s.pool, s.host, false)
        .await
        .expect("overview");
    assert_eq!((mine.open_conflicts, mine.failing_feeds), (1, 1));
    assert_eq!(mine.listings.len(), 1);
    let stranger = raw_user(&s.pool).await;
    let theirs = overview_for_user(&s.pool, stranger, false)
        .await
        .expect("overview");
    assert_eq!((theirs.open_conflicts, theirs.failing_feeds), (0, 0));
    let admin = overview_for_user(&s.pool, stranger, true)
        .await
        .expect("overview");
    assert_eq!((admin.open_conflicts, admin.failing_feeds), (1, 1));
}

/// A hold and a sync on the same nights: either the hold is refused or the
/// conflict is recorded, never neither.
#[tokio::test(flavor = "multi_thread")]
async fn a_hold_racing_a_sync_is_refused_or_recorded() {
    for _ in 0..5 {
        let s = setup(0).await;
        let guest = raw_user(&s.pool).await;
        let pool = s.pool.clone();
        let villa = s.villa;
        let hold = tokio::spawn(async move {
            crate::booking::create_booking(
                &pool,
                &crate::booking::unavailable_ranges_tests::new_booking(
                    guest,
                    villa,
                    d(1, 10),
                    d(1, 15),
                ),
                120,
            )
            .await
        });
        let pool = s.pool.clone();
        let c = s.claim.clone();
        let sync = tokio::spawn(async move {
            reconcile_feed(
                &pool,
                &c,
                &parsed(&[("e", d(1, 12), d(1, 13))]),
                &ctx(SyncTrigger::Manual),
            )
            .await
        });
        let held = hold.await.expect("join");
        sync.await.expect("join").expect("reconcile");
        let conflicts = list_conflicts(&s.pool, s.villa, 1)
            .await
            .expect("list")
            .open
            .len();
        match held {
            Ok(_) => assert_eq!(conflicts, 1, "hold won: the sync records the conflict"),
            Err(crate::error::DbError::DateUnavailable) => assert_eq!(conflicts, 0),
            Err(other) => panic!("{other:?}"),
        }
    }
}

#[tokio::test]
async fn a_conflicted_pending_hold_still_confirms() {
    let s = setup(0).await;
    let booking = insert_booking(&s.pool, s.villa, d(1, 10), d(1, 15), "pending", LIVE).await;
    sync(&s, &[("e", d(1, 12), d(1, 13))]).await;
    let confirmed = crate::booking::update_booking(
        &s.pool,
        booking,
        &crate::models::UpdatedBooking {
            status: Some(crate::models::BookingStatus::Confirmed),
            ..Default::default()
        },
        &crate::models::SupplierInfo {
            supplier_name: "Our Places".to_string(),
            supplier_address: "Ocho Rios".to_string(),
            supplier_trn: "000-111-222".to_string(),
        },
    )
    .await
    .expect("confirm");
    assert_eq!(confirmed.status, crate::models::BookingStatus::Confirmed);
}
