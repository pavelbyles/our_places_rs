//! Enquiries (#105, plan T22).

use super::fixtures::{day, listing_in_mode, migrated_db, raw_stay, raw_user};
use super::{Actor, NewEnquiry, close_enquiry, create_enquiry, get_enquiry};
use crate::booking::check_availability;
use crate::error::DbError;
use crate::models::DbMessageSenderRole;
use common::booking_request::{EnquiryClosedReason, EnquiryStatus, RequestRefusal};
use uuid::Uuid;

fn enquiry(listing: Uuid, guest: Uuid) -> NewEnquiry<'static> {
    NewEnquiry {
        listing_id: listing,
        guest_id: guest,
        sender_name: "Sam",
        date_from: day(3, 1),
        date_to: day(3, 4),
        number_of_persons: 6,
        currency: "USD",
        message: "Can we host a small wedding?",
    }
}

async fn enquiry_history(pool: &sqlx::PgPool, id: Uuid) -> Vec<(Option<String>, String)> {
    sqlx::query_as(
        "SELECT from_status, to_status FROM booking_request_history
         WHERE enquiry_id = $1 ORDER BY created_at, id",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .expect("history")
}

#[tokio::test]
async fn enquiry_tests_an_enquiry_on_booked_dates_succeeds_and_holds_nothing() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, _) = listing_in_mode(&pool, "instant", 0).await;
    let (owner, guest) = (raw_user(&pool).await, raw_user(&pool).await);
    raw_stay(&pool, owner, villa, day(3, 1), day(3, 4), "confirmed", 0).await;
    let (other_villa, _) = listing_in_mode(&pool, "request", 0).await;

    let created = create_enquiry(&pool, &enquiry(villa, guest))
        .await
        .expect("enquiry");
    assert_eq!(created.enquiry.status, EnquiryStatus::Open);
    let free = create_enquiry(&pool, &enquiry(other_villa, guest))
        .await
        .expect("enquiry");
    assert!(
        check_availability(&pool, other_villa, day(3, 1), day(3, 4))
            .await
            .expect("check"),
        "enquiries never hold dates"
    );
    let messages = crate::conversation::list_messages(&pool, free.enquiry.conversation_id)
        .await
        .expect("thread");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].enquiry_id, Some(free.enquiry.id));
    assert_eq!(
        enquiry_history(&pool, created.enquiry.id).await,
        vec![(None, "open".to_string())]
    );
}

#[tokio::test]
async fn enquiry_tests_a_second_enquiry_supersedes_the_first() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, _) = listing_in_mode(&pool, "both", 0).await;
    let guest = raw_user(&pool).await;
    let first = create_enquiry(&pool, &enquiry(villa, guest))
        .await
        .expect("first");
    let second = create_enquiry(&pool, &enquiry(villa, guest))
        .await
        .expect("second");
    assert_eq!(second.superseded, Some(first.enquiry.id));
    let first = get_enquiry(&pool, first.enquiry.id)
        .await
        .expect("get")
        .expect("exists");
    assert_eq!(
        first.enquiry.closed_reason,
        Some(EnquiryClosedReason::Superseded)
    );
    assert_eq!(
        first.enquiry.conversation_id,
        second.enquiry.conversation_id
    );
}

#[tokio::test]
async fn enquiry_tests_the_host_closes_once_and_owners_and_inactive_listings_are_refused() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = listing_in_mode(&pool, "both", 0).await;
    let guest = raw_user(&pool).await;
    let created = create_enquiry(&pool, &enquiry(villa, guest))
        .await
        .expect("enquiry");
    let actor = Actor {
        id: host,
        role: DbMessageSenderRole::Host,
    };
    close_enquiry(&pool, created.enquiry.id, actor)
        .await
        .expect("close");
    let closed = get_enquiry(&pool, created.enquiry.id)
        .await
        .expect("get")
        .expect("exists");
    assert_eq!(
        closed.enquiry.closed_reason,
        Some(EnquiryClosedReason::ClosedByHost)
    );
    assert!(closed.enquiry.first_response_at.is_some());
    assert!(matches!(
        close_enquiry(&pool, created.enquiry.id, actor).await,
        Err(DbError::RequestRefused(RequestRefusal::RequestNotOpen))
    ));
    assert_eq!(enquiry_history(&pool, created.enquiry.id).await.len(), 2);

    assert!(matches!(
        create_enquiry(&pool, &enquiry(villa, host)).await,
        Err(DbError::RequestRefused(RequestRefusal::OwnListing))
    ));
    sqlx::query("UPDATE listing SET is_active = FALSE WHERE id = $1")
        .bind(villa)
        .execute(&pool)
        .await
        .expect("deactivate");
    assert!(matches!(
        create_enquiry(&pool, &enquiry(villa, guest)).await,
        Err(DbError::ListingUnavailable)
    ));
}
