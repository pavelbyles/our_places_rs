//! Request and enquiry sweeps (#105, plan T26).

use super::fixtures::{
    day, lapse_request, listing_in_mode, migrated_db, new_request, raw_user, request_history,
    request_status,
};
use super::{
    NewEnquiry, close_past_enquiries, create_enquiry, create_request, expire_stale_requests,
    withdraw_request,
};

#[tokio::test]
async fn request_expiry_tests_only_lapsed_live_requests_expire_and_reruns_do_nothing() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = listing_in_mode(&pool, "request", 0).await;
    let (a, b, c) = (
        raw_user(&pool).await,
        raw_user(&pool).await,
        raw_user(&pool).await,
    );
    let lapsed = create_request(&pool, &new_request(villa, a, day(3, 1), day(3, 4)))
        .await
        .expect("a");
    let live = create_request(&pool, &new_request(villa, b, day(4, 1), day(4, 4)))
        .await
        .expect("b");
    let done = create_request(&pool, &new_request(villa, c, day(5, 1), day(5, 4)))
        .await
        .expect("c");
    withdraw_request(&pool, done.request.id, c)
        .await
        .expect("withdraw");
    lapse_request(&pool, lapsed.request.id).await;
    lapse_request(&pool, done.request.id).await;

    let expired = expire_stale_requests(&pool).await.expect("sweep");
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].id, lapsed.request.id);
    assert_eq!(expired[0].host_id, host);
    assert_eq!(expired[0].from_status, "open");
    assert_eq!(request_status(&pool, lapsed.request.id).await, "expired");
    assert_eq!(request_status(&pool, live.request.id).await, "open");
    assert_eq!(request_status(&pool, done.request.id).await, "withdrawn");
    assert_eq!(
        request_history(&pool, lapsed.request.id)
            .await
            .last()
            .cloned(),
        Some((Some("open".to_string()), "expired".to_string(), None))
    );
    assert!(
        expire_stale_requests(&pool)
            .await
            .expect("rerun")
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn request_expiry_tests_a_row_locked_by_an_action_is_skipped_until_next_time() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, _) = listing_in_mode(&pool, "request", 0).await;
    let guest = raw_user(&pool).await;
    let created = create_request(&pool, &new_request(villa, guest, day(3, 1), day(3, 4)))
        .await
        .expect("request");
    lapse_request(&pool, created.request.id).await;

    let mut holder = pool.begin().await.expect("begin");
    sqlx::query("SELECT 1 FROM booking_request WHERE id = $1 FOR UPDATE")
        .bind(created.request.id)
        .execute(&mut *holder)
        .await
        .expect("lock");
    assert!(
        expire_stale_requests(&pool)
            .await
            .expect("sweep")
            .is_empty()
    );
    holder.rollback().await.expect("release");
    assert_eq!(expire_stale_requests(&pool).await.expect("sweep").len(), 1);
}

#[tokio::test]
async fn request_expiry_tests_enquiries_close_once_check_in_arrives() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, _) = listing_in_mode(&pool, "both", 0).await;
    let guest = raw_user(&pool).await;
    let created = create_enquiry(
        &pool,
        &NewEnquiry {
            listing_id: villa,
            guest_id: guest,
            sender_name: "Sam",
            date_from: day(3, 1),
            date_to: day(3, 4),
            number_of_persons: 2,
            currency: "USD",
            message: "Hi",
        },
    )
    .await
    .expect("enquiry");
    assert_eq!(
        close_past_enquiries(&pool, day(2, 28))
            .await
            .expect("before"),
        0
    );
    assert_eq!(
        close_past_enquiries(&pool, day(3, 1))
            .await
            .expect("on the day"),
        1
    );
    let reason: String =
        sqlx::query_scalar("SELECT closed_reason::text FROM booking_enquiry WHERE id = $1")
            .bind(created.enquiry.id)
            .fetch_one(&pool)
            .await
            .expect("reason");
    assert_eq!(reason, "expired");
}
