//! One thread per guest × listing (#105, plan T20).

use crate::booking_request::fixtures::*;
use crate::booking_request::{Actor, accept_request, convert_offer_to_hold, create_request};
use crate::conversation::{NewMessage, insert_message, list_messages};
use crate::models::DbMessageSenderRole;
use chrono::Duration;
use sqlx::PgPool;
use uuid::Uuid;

async fn post(
    pool: &PgPool,
    conversation: Uuid,
    sender: Uuid,
    role: DbMessageSenderRole,
    text: &str,
) {
    insert_message(
        pool,
        &NewMessage {
            conversation_id: conversation,
            sender_id: sender,
            sender_role: role,
            sender_name: "Someone",
            text,
            booking_id: None,
            enquiry_id: None,
            request_id: None,
        },
    )
    .await
    .expect("post message");
}

#[tokio::test]
async fn conversation_tests_enquiry_request_and_booking_share_one_thread() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = listing_in_mode(&pool, "request", 0).await;
    let guest = raw_user(&pool).await;
    let enquiry = raw_enquiry(&pool, villa, guest).await;
    let conversation: Uuid =
        sqlx::query_scalar("SELECT conversation_id FROM booking_enquiry WHERE id = $1")
            .bind(enquiry)
            .fetch_one(&pool)
            .await
            .expect("conversation");
    post(
        &pool,
        conversation,
        guest,
        DbMessageSenderRole::Guest,
        "Is the pool heated?",
    )
    .await;
    post(
        &pool,
        conversation,
        host,
        DbMessageSenderRole::Host,
        "Yes it is.",
    )
    .await;

    let mut request = new_request(villa, guest, day(3, 1), day(3, 4));
    request.enquiry_id = Some(enquiry);
    request.metadata.message_to_host = Some("Booking for six.".to_string());
    let created = create_request(&pool, &request).await.expect("request");
    assert_eq!(created.request.conversation_id, conversation);
    accept_request(
        &pool,
        created.request.id,
        Actor {
            id: host,
            role: DbMessageSenderRole::Host,
        },
        48,
    )
    .await
    .expect("accept");
    let converted = convert_offer_to_hold(
        &pool,
        &crate::booking_request::OfferCheckout {
            request_id: created.request.id,
            guest_id: guest,
            quote_id: None,
            confirmation_code: format!("T{}", Uuid::new_v4().simple())[..10].to_uppercase(),
            hold_minutes: 120,
        },
    )
    .await
    .expect("convert");
    crate::booking_message::insert_booking_message(
        &pool,
        converted.booking.id,
        guest,
        DbMessageSenderRole::Guest,
        "Guest",
        "See you soon!",
    )
    .await
    .expect("booking message");

    let texts: Vec<String> =
        crate::booking_message::list_booking_messages(&pool, converted.booking.id)
            .await
            .expect("list")
            .into_iter()
            .map(|m| m.message_text)
            .collect();
    assert_eq!(
        texts,
        [
            "Is the pool heated?",
            "Yes it is.",
            "Booking for six.",
            "See you soon!"
        ]
    );
    let tagged = list_messages(&pool, conversation).await.expect("list");
    assert_eq!(tagged[2].request_id, Some(created.request.id));
    assert_eq!(tagged[2].enquiry_id, Some(enquiry));
    assert_eq!(tagged[3].booking_id, Some(converted.booking.id));
}

#[tokio::test]
async fn conversation_tests_only_the_first_host_side_post_sets_first_response() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = listing_in_mode(&pool, "request", 0).await;
    let guest = raw_user(&pool).await;
    let created = create_request(&pool, &new_request(villa, guest, day(3, 1), day(3, 4)))
        .await
        .expect("request");
    let first_response = |pool: PgPool| async move {
        sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
            "SELECT first_response_at FROM booking_request WHERE id = $1",
        )
        .bind(created.request.id)
        .fetch_one(&pool)
        .await
        .expect("first_response_at")
    };
    let conversation = created.request.conversation_id;

    post(
        &pool,
        conversation,
        guest,
        DbMessageSenderRole::Guest,
        "Hello?",
    )
    .await;
    assert_eq!(first_response(pool.clone()).await, None);
    post(&pool, conversation, host, DbMessageSenderRole::Host, "Hi!").await;
    let first = first_response(pool.clone()).await.expect("set by the host");
    sqlx::query("UPDATE booking_request SET first_response_at = first_response_at - INTERVAL '1 hour' WHERE id = $1")
        .bind(created.request.id)
        .execute(&pool)
        .await
        .expect("age it");
    post(
        &pool,
        conversation,
        host,
        DbMessageSenderRole::Admin,
        "Following up",
    )
    .await;
    assert_eq!(
        first_response(pool.clone()).await,
        Some(first - Duration::hours(1)),
        "a later post leaves it alone"
    );
}

#[tokio::test]
async fn conversation_tests_a_promoted_guest_takes_the_bookings_messages_along() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, _) = listing_in_mode(&pool, "both", 0).await;
    let placeholder = raw_user(&pool).await;
    let promoted = raw_user(&pool).await;
    let booking = raw_stay(
        &pool,
        placeholder,
        villa,
        day(3, 1),
        day(3, 4),
        "pending",
        60,
    )
    .await;
    let msg = crate::booking_message::insert_booking_message(
        &pool,
        booking,
        placeholder,
        DbMessageSenderRole::Guest,
        "Guest",
        "Arriving late",
    )
    .await
    .expect("message");

    crate::booking::transfer_booking_guest(&pool, booking, promoted)
        .await
        .expect("transfer");
    let (listing, guest): (Uuid, Uuid) = sqlx::query_as(
        "SELECT c.listing_id, c.guest_id FROM booking_message m
         JOIN conversation c ON c.id = m.conversation_id WHERE m.id = $1",
    )
    .bind(msg.id)
    .fetch_one(&pool)
    .await
    .expect("conversation");
    assert_eq!((listing, guest), (villa, promoted));
}

async fn conversation_of_enquiry(pool: &PgPool, enquiry: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT conversation_id FROM booking_enquiry WHERE id = $1")
        .bind(enquiry)
        .fetch_one(pool)
        .await
        .expect("conversation")
}

#[tokio::test]
async fn unread_guest_messages_count_only_guest_messages_on_the_hosts_listings() {
    use crate::conversation::{mark_read, unread_guest_messages};
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = listing_in_mode(&pool, "both", 0).await;
    let (cabin, other_host) = listing_in_mode(&pool, "both", 0).await;
    let guest = raw_user(&pool).await;
    let other_guest = raw_user(&pool).await;
    let admin = raw_user(&pool).await;

    let first = conversation_of_enquiry(&pool, raw_enquiry(&pool, villa, guest).await).await;
    let second = conversation_of_enquiry(&pool, raw_enquiry(&pool, villa, other_guest).await).await;
    let elsewhere = conversation_of_enquiry(&pool, raw_enquiry(&pool, cabin, guest).await).await;

    post(&pool, first, guest, DbMessageSenderRole::Guest, "One").await;
    post(&pool, first, guest, DbMessageSenderRole::Guest, "Two").await;
    post(&pool, first, host, DbMessageSenderRole::Host, "Host reply").await;
    post(
        &pool,
        first,
        admin,
        DbMessageSenderRole::Admin,
        "Admin note",
    )
    .await;
    post(
        &pool,
        second,
        other_guest,
        DbMessageSenderRole::Guest,
        "Three",
    )
    .await;
    post(
        &pool,
        elsewhere,
        guest,
        DbMessageSenderRole::Guest,
        "Other listing",
    )
    .await;

    assert_eq!(
        unread_guest_messages(&pool, Some(host))
            .await
            .expect("count"),
        3
    );
    assert_eq!(
        unread_guest_messages(&pool, Some(other_host))
            .await
            .expect("count"),
        1
    );
    assert_eq!(
        unread_guest_messages(&pool, None).await.expect("count"),
        4,
        "admins count every listing"
    );

    mark_read(&pool, first, host).await.expect("mark read");
    assert_eq!(
        unread_guest_messages(&pool, Some(host))
            .await
            .expect("count"),
        1
    );
}
