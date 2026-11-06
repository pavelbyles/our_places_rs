//! Inbox and response metrics (#105, plan T30).

use super::fixtures::*;
use super::{InboxFilter, create_request, host_response_metrics, inbox_items};
use sqlx::PgPool;
use uuid::Uuid;

async fn set_times(pool: &PgPool, id: Uuid, created_h_ago: i32, responded_after_min: Option<i32>) {
    sqlx::query(
        "UPDATE booking_request
         SET created_at = NOW() - make_interval(hours => $2),
             first_response_at = NOW() - make_interval(hours => $2)
                 + make_interval(mins => $3)
         WHERE id = $1",
    )
    .bind(id)
    .bind(created_h_ago)
    .bind(responded_after_min)
    .execute(pool)
    .await
    .expect("set times");
}

#[tokio::test]
async fn inbox_tests_hosts_see_their_own_items_and_admins_see_all() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (mine, me) = listing_in_mode(&pool, "request", 0).await;
    let (theirs, _) = listing_in_mode(&pool, "request", 0).await;
    let (a, b) = (raw_user(&pool).await, raw_user(&pool).await);
    let soon = create_request(&pool, &new_request(mine, a, day(3, 1), day(3, 4)))
        .await
        .expect("a");
    let other = create_request(&pool, &new_request(theirs, b, day(3, 1), day(3, 4)))
        .await
        .expect("b");
    raw_enquiry(&pool, mine, b).await;
    sqlx::query("UPDATE booking_request SET expires_at = NOW() + INTERVAL '1 hour' WHERE id = $1")
        .bind(soon.request.id)
        .execute(&pool)
        .await
        .expect("soon");

    let filter = InboxFilter {
        host_id: Some(me),
        ..InboxFilter::default()
    };
    let items = inbox_items(&pool, &filter, 50, 0).await.expect("inbox");
    assert_eq!(
        items.iter().map(|i| i.kind.as_str()).collect::<Vec<_>>(),
        ["request", "enquiry"],
        "requests by expiry first, then enquiries"
    );
    assert!(items.iter().all(|i| i.host_id == me));
    let all = inbox_items(&pool, &InboxFilter::default(), 500, 0)
        .await
        .expect("all");
    assert!(all.iter().any(|i| i.id == other.request.id));
    let only_enquiries = InboxFilter {
        host_id: Some(me),
        kind: Some("enquiry".to_string()),
        ..InboxFilter::default()
    };
    assert_eq!(
        inbox_items(&pool, &only_enquiries, 50, 0)
            .await
            .expect("enquiries")
            .len(),
        1
    );

    lapse_request(&pool, soon.request.id).await;
    let after = inbox_items(&pool, &filter, 50, 0).await.expect("inbox");
    assert!(
        after.iter().all(|i| i.id != soon.request.id),
        "lapsed requests leave the inbox"
    );
}

#[tokio::test]
async fn inbox_tests_the_median_first_response_and_unanswered_expiries_per_host() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = listing_in_mode(&pool, "request", 0).await;
    let mut ids = Vec::new();
    for month in 3..7 {
        let guest = raw_user(&pool).await;
        let created = create_request(
            &pool,
            &new_request(villa, guest, day(month, 1), day(month, 4)),
        )
        .await
        .expect("request");
        ids.push(created.request.id);
    }
    // Responses after 10, 30 and 60 minutes; the fourth is never answered.
    set_times(&pool, ids[0], 5, Some(10)).await;
    set_times(&pool, ids[1], 5, Some(30)).await;
    set_times(&pool, ids[2], 5, Some(60)).await;
    set_times(&pool, ids[3], 5, None).await;
    sqlx::query("UPDATE booking_request SET status = 'expired' WHERE id = $1")
        .bind(ids[3])
        .execute(&pool)
        .await
        .expect("expire");

    let metrics = host_response_metrics(&pool, Some(host))
        .await
        .expect("metrics");
    assert_eq!(metrics.len(), 1);
    let m = &metrics[0];
    assert_eq!(m.median_first_response_secs, Some(30 * 60));
    assert_eq!((m.received, m.requests, m.expired_unanswered), (4, 4, 1));

    // An even count takes the midpoint: 10, 30, 60, 90 minutes -> 45.
    let guest = raw_user(&pool).await;
    let fifth = create_request(&pool, &new_request(villa, guest, day(8, 1), day(8, 4)))
        .await
        .expect("fifth");
    set_times(&pool, fifth.request.id, 5, Some(90)).await;
    let m = &host_response_metrics(&pool, Some(host))
        .await
        .expect("metrics")[0];
    assert_eq!(m.median_first_response_secs, Some(45 * 60));

    // A host with no responses yet has no median.
    let (quiet, quiet_host) = listing_in_mode(&pool, "request", 0).await;
    let guest = raw_user(&pool).await;
    create_request(&pool, &new_request(quiet, guest, day(3, 1), day(3, 4)))
        .await
        .expect("quiet");
    let m = &host_response_metrics(&pool, Some(quiet_host))
        .await
        .expect("metrics")[0];
    assert_eq!(m.median_first_response_secs, None);
}
