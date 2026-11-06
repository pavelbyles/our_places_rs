//! Instant holds and requests on the same listing (#105, plan T12).

use super::create_request;
use super::fixtures::*;
use crate::booking::{create_booking, create_booking_superseding};
use crate::error::DbError;

const HOLD_MINUTES: i32 = 120;

#[tokio::test(flavor = "multi_thread")]
async fn an_instant_hold_and_a_request_racing_for_the_same_dates_get_one_winner() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    for _ in 0..5 {
        let (villa, _) = listing_in_mode(&pool, "both", 0).await;
        let (booker, requester) = (raw_user(&pool).await, raw_user(&pool).await);
        let hold = new_hold(villa, booker, day(3, 1), day(3, 4));
        let request = new_request(villa, requester, day(3, 2), day(3, 5));
        let (h, r) = tokio::join!(
            create_booking(&pool, &hold, HOLD_MINUTES),
            create_request(&pool, &request)
        );
        assert_eq!(
            [h.is_ok(), r.is_ok()].iter().filter(|ok| **ok).count(),
            1,
            "hold {:?} / request {:?}",
            h.as_ref().err(),
            r.as_ref().err()
        );
        match (h, r) {
            (Err(DbError::DateUnavailable), Ok(_)) | (Ok(_), Err(DbError::DateUnavailable)) => {}
            (h, r) => panic!("unexpected: {:?} / {:?}", h.err(), r.err()),
        }
    }
}

#[tokio::test]
async fn another_guests_live_request_blocks_an_instant_hold_until_it_lapses() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, _) = listing_in_mode(&pool, "both", 1).await;
    let (requester, booker) = (raw_user(&pool).await, raw_user(&pool).await);
    let created = create_request(&pool, &new_request(villa, requester, day(3, 1), day(3, 4)))
        .await
        .expect("request");

    // Check-in on the request's check-out day falls inside the 1-night gap.
    let hold = new_hold(villa, booker, day(3, 4), day(3, 6));
    assert!(matches!(
        create_booking(&pool, &hold, HOLD_MINUTES).await,
        Err(DbError::DateUnavailable)
    ));
    // The day after the gap is free.
    create_booking(
        &pool,
        &new_hold(villa, booker, day(3, 5), day(3, 7)),
        HOLD_MINUTES,
    )
    .await
    .expect("free after the gap");

    lapse_request(&pool, created.request.id).await;
    let other = raw_user(&pool).await;
    create_booking(
        &pool,
        &new_hold(villa, other, day(3, 1), day(3, 3)),
        HOLD_MINUTES,
    )
    .await
    .expect("a lapsed request no longer blocks");
}

#[tokio::test]
async fn the_guests_own_request_and_enquiry_give_way_to_their_instant_hold() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, _) = listing_in_mode(&pool, "both", 0).await;
    let (other_villa, _) = listing_in_mode(&pool, "both", 0).await;
    let guest = raw_user(&pool).await;
    let mine = create_request(&pool, &new_request(villa, guest, day(3, 1), day(3, 4)))
        .await
        .expect("request");
    let elsewhere = create_request(
        &pool,
        &new_request(other_villa, guest, day(3, 1), day(3, 4)),
    )
    .await
    .expect("request on another listing");
    let enquiry = raw_enquiry(&pool, villa, guest).await;

    let (booking, superseded) = create_booking_superseding(
        &pool,
        &new_hold(villa, guest, day(3, 1), day(3, 4)),
        HOLD_MINUTES,
    )
    .await
    .expect("the guest's own request doesn't block their hold");
    assert_eq!(booking.listing_id, villa);
    assert_eq!(superseded.request, Some(mine.request.id));
    assert_eq!(superseded.enquiry, Some(enquiry));

    assert_eq!(request_status(&pool, mine.request.id).await, "cancelled");
    assert_eq!(
        request_history(&pool, mine.request.id)
            .await
            .last()
            .cloned(),
        Some((
            Some("open".to_string()),
            "cancelled".to_string(),
            Some(guest)
        ))
    );
    let closed: (String, Option<String>) = sqlx::query_as(
        "SELECT status::text, closed_reason::text FROM booking_enquiry WHERE id = $1",
    )
    .bind(enquiry)
    .fetch_one(&pool)
    .await
    .expect("enquiry");
    assert_eq!(closed, ("closed".to_string(), Some("booked".to_string())));

    // Supersession is per listing.
    assert_eq!(request_status(&pool, elsewhere.request.id).await, "open");
}

#[tokio::test]
async fn a_lapsed_own_request_is_expired_not_cancelled_by_the_hold() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, _) = listing_in_mode(&pool, "both", 0).await;
    let guest = raw_user(&pool).await;
    let mine = create_request(&pool, &new_request(villa, guest, day(3, 1), day(3, 4)))
        .await
        .expect("request");
    lapse_request(&pool, mine.request.id).await;

    let (_, superseded) = create_booking_superseding(
        &pool,
        &new_hold(villa, guest, day(5, 1), day(5, 4)),
        HOLD_MINUTES,
    )
    .await
    .expect("hold");
    assert_eq!(superseded.request, None);
    assert_eq!(request_status(&pool, mine.request.id).await, "expired");
}
