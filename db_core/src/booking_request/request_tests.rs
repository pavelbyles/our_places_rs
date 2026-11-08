//! Submitting a request to book (#105, plan T10).

use super::create_request;
use super::fixtures::{
    day, lapse_request, listing_in_mode, migrated_db, new_request, raw_stay, raw_user,
    request_history, request_status,
};
use crate::error::DbError;
use common::booking_request::RequestRefusal;

fn refusal(result: crate::error::Result<super::CreatedRequest>) -> Option<RequestRefusal> {
    match result {
        Err(DbError::RequestRefused(r)) => Some(r),
        _ => None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn two_guests_racing_for_the_same_dates_get_one_request() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, _) = listing_in_mode(&pool, "request", 0).await;
    let (a, b) = (raw_user(&pool).await, raw_user(&pool).await);

    let first = new_request(villa, a, day(3, 1), day(3, 4));
    let second = new_request(villa, b, day(3, 2), day(3, 5));
    let (ra, rb) = tokio::join!(
        create_request(&pool, &first),
        create_request(&pool, &second)
    );
    let outcomes = [ra.is_ok(), rb.is_ok()];
    assert_eq!(outcomes.iter().filter(|ok| **ok).count(), 1, "{outcomes:?}");
    let loser = if ra.is_ok() { rb } else { ra };
    assert!(matches!(loser, Err(DbError::DateUnavailable)));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_same_guest_racing_twice_gets_one_live_request() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, _) = listing_in_mode(&pool, "both", 0).await;
    let guest = raw_user(&pool).await;

    let one = new_request(villa, guest, day(3, 1), day(3, 4));
    let two = new_request(villa, guest, day(5, 1), day(5, 4));
    let (r1, r2) = tokio::join!(create_request(&pool, &one), create_request(&pool, &two));
    assert_eq!([r1.is_ok(), r2.is_ok()].iter().filter(|ok| **ok).count(), 1);
    let loser = if r1.is_ok() { r2 } else { r1 };
    assert_eq!(refusal(loser), Some(RequestRefusal::RequestAlreadyLive));
}

#[tokio::test]
async fn a_lapsed_unswept_request_is_expired_inline_so_a_new_one_succeeds() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, _) = listing_in_mode(&pool, "request", 0).await;
    let guest = raw_user(&pool).await;

    let old = create_request(&pool, &new_request(villa, guest, day(3, 1), day(3, 4)))
        .await
        .expect("first request");
    lapse_request(&pool, old.request.id).await;

    let new = create_request(&pool, &new_request(villa, guest, day(3, 1), day(3, 4)))
        .await
        .expect("a new request once the old one lapsed");
    assert_ne!(new.request.id, old.request.id);
    assert_eq!(request_status(&pool, old.request.id).await, "expired");
    let history = request_history(&pool, old.request.id).await;
    assert_eq!(
        history
            .last()
            .map(|(from, to, actor)| (from.clone(), to.clone(), *actor)),
        Some((Some("open".to_string()), "expired".to_string(), None))
    );
}

#[tokio::test]
async fn the_guests_own_pending_hold_on_the_listing_is_superseded() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, _) = listing_in_mode(&pool, "both", 0).await;
    let guest = raw_user(&pool).await;
    let hold = raw_stay(&pool, guest, villa, day(3, 1), day(3, 4), "pending", 60).await;

    let created = create_request(&pool, &new_request(villa, guest, day(3, 1), day(3, 4)))
        .await
        .expect("request over the guest's own hold");
    assert_eq!(created.superseded_holds, vec![hold]);

    let status: String = sqlx::query_scalar("SELECT status::text FROM booking WHERE id = $1")
        .bind(hold)
        .fetch_one(&pool)
        .await
        .expect("hold status");
    assert_eq!(status, "cancelled");
    let reasons: Vec<String> =
        sqlx::query_scalar("SELECT change_reason FROM booking_history WHERE booking_id = $1")
            .bind(hold)
            .fetch_all(&pool)
            .await
            .expect("hold history");
    assert!(
        reasons
            .iter()
            .any(|r| r.contains("superseded by a request"))
    );
}

#[tokio::test]
async fn confirmed_stays_other_guests_holds_and_requests_block_with_the_gap() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, _) = listing_in_mode(&pool, "request", 2).await;
    let (owner, holder, other, guest) = (
        raw_user(&pool).await,
        raw_user(&pool).await,
        raw_user(&pool).await,
        raw_user(&pool).await,
    );
    raw_stay(&pool, owner, villa, day(3, 10), day(3, 12), "confirmed", 0).await;
    // A different guest from the requester below, whose request would
    // otherwise supersede this hold.
    raw_stay(&pool, holder, villa, day(4, 10), day(4, 12), "pending", 60).await;
    create_request(&pool, &new_request(villa, other, day(5, 10), day(5, 12)))
        .await
        .expect("other guest's request");

    // Each touches a block only through the 2-night gap.
    for (from, to) in [
        (day(3, 13), day(3, 15)),
        (day(4, 6), day(4, 9)),
        (day(5, 13), day(5, 15)),
    ] {
        let result = create_request(&pool, &new_request(villa, guest, from, to)).await;
        assert!(
            matches!(result, Err(DbError::DateUnavailable)),
            "{from}..{to}: {result:?}"
        );
    }
    // A lapsed hold no longer blocks.
    raw_stay(&pool, holder, villa, day(6, 10), day(6, 12), "pending", -1).await;
    create_request(&pool, &new_request(villa, guest, day(6, 10), day(6, 12)))
        .await
        .expect("lapsed hold doesn't block");
}

#[tokio::test]
async fn mode_activity_and_ownership_are_checked() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let guest = raw_user(&pool).await;

    let (instant, _) = listing_in_mode(&pool, "instant", 0).await;
    let result = create_request(&pool, &new_request(instant, guest, day(3, 1), day(3, 4))).await;
    assert_eq!(refusal(result), Some(RequestRefusal::BookingModeNotAllowed));

    let (inactive, _) = listing_in_mode(&pool, "request", 0).await;
    sqlx::query("UPDATE listing SET is_active = FALSE WHERE id = $1")
        .bind(inactive)
        .execute(&pool)
        .await
        .expect("deactivate");
    let result = create_request(&pool, &new_request(inactive, guest, day(3, 1), day(3, 4))).await;
    assert!(matches!(result, Err(DbError::ListingUnavailable)));

    let (own, host) = listing_in_mode(&pool, "request", 0).await;
    let result = create_request(&pool, &new_request(own, host, day(3, 1), day(3, 4))).await;
    assert_eq!(refusal(result), Some(RequestRefusal::OwnListing));
}

#[tokio::test]
async fn creation_writes_one_history_row_by_the_guest() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, _) = listing_in_mode(&pool, "request", 0).await;
    let guest = raw_user(&pool).await;
    let created = create_request(&pool, &new_request(villa, guest, day(3, 1), day(3, 4)))
        .await
        .expect("request");
    assert_eq!(
        created.request.status,
        common::booking_request::RequestStatus::Open
    );
    assert_eq!(
        request_history(&pool, created.request.id).await,
        vec![(None, "open".to_string(), Some(guest))]
    );
    let conversation: (uuid::Uuid, uuid::Uuid) =
        sqlx::query_as("SELECT listing_id, guest_id FROM conversation WHERE id = $1")
            .bind(created.request.conversation_id)
            .fetch_one(&pool)
            .await
            .expect("conversation");
    assert_eq!(conversation, (villa, guest));
}

#[tokio::test]
async fn a_request_from_an_enquiry_closes_it_and_someone_elses_is_refused() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, _) = listing_in_mode(&pool, "request", 0).await;
    let (guest, stranger) = (raw_user(&pool).await, raw_user(&pool).await);
    let conversation: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO conversation (listing_id, guest_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(villa)
    .bind(guest)
    .fetch_one(&pool)
    .await
    .expect("conversation");
    let enquiry: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO booking_enquiry (conversation_id, listing_id, guest_id, date_from, date_to,
             number_of_persons, currency)
         VALUES ($1, $2, $3, '2030-03-01', '2030-03-04', 2, 'USD') RETURNING id",
    )
    .bind(conversation)
    .bind(villa)
    .bind(guest)
    .fetch_one(&pool)
    .await
    .expect("enquiry");

    let mut theirs = new_request(villa, stranger, day(3, 1), day(3, 4));
    theirs.enquiry_id = Some(enquiry);
    assert_eq!(
        refusal(create_request(&pool, &theirs).await),
        Some(RequestRefusal::NotParty)
    );

    let mut mine = new_request(villa, guest, day(3, 1), day(3, 4));
    mine.enquiry_id = Some(enquiry);
    let created = create_request(&pool, &mine)
        .await
        .expect("request from enquiry");
    assert_eq!(created.closed_enquiry, Some(enquiry));
    let closed: (String, Option<String>) = sqlx::query_as(
        "SELECT status::text, closed_reason::text FROM booking_enquiry WHERE id = $1",
    )
    .bind(enquiry)
    .fetch_one(&pool)
    .await
    .expect("enquiry status");
    assert_eq!(
        closed,
        ("closed".to_string(), Some("request_submitted".to_string()))
    );
}
