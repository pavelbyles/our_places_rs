//! Schema constraint tests for enquiries and booking requests (#105).

use crate::promo_tests::{migrated_db, raw_booking, raw_listing, raw_user};
use sqlx::{PgPool, Row};
use uuid::Uuid;

async fn conversation_of(pool: &PgPool, message: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT conversation_id FROM booking_message WHERE id = $1")
        .bind(message)
        .fetch_one(pool)
        .await
        .expect("read conversation_id")
}

async fn raw_conversation(pool: &PgPool, listing: Uuid, guest: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO conversation (listing_id, guest_id) VALUES ($1, $2)
         ON CONFLICT (listing_id, guest_id) DO UPDATE SET last_message_at = NOW()
         RETURNING id",
    )
    .bind(listing)
    .bind(guest)
    .fetch_one(pool)
    .await
    .expect("insert conversation")
}

async fn raw_request(pool: &PgPool, listing: Uuid, guest: Uuid) -> Result<Uuid, sqlx::Error> {
    let conversation = raw_conversation(pool, listing, guest).await;
    sqlx::query_scalar(
        "INSERT INTO booking_request (conversation_id, listing_id, guest_id, origin, date_from,
             date_to, number_of_persons, currency, price_snapshot, expires_at)
         VALUES ($1, $2, $3, 'guest_request', '2030-02-01', '2030-02-04', 2, 'USD', '{}'::jsonb,
             NOW() + INTERVAL '1 day')
         RETURNING id",
    )
    .bind(conversation)
    .bind(listing)
    .bind(guest)
    .fetch_one(pool)
    .await
}

#[tokio::test]
async fn only_one_live_request_per_guest_and_listing() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let host = raw_user(&pool).await;
    let guest = raw_user(&pool).await;
    let villa = raw_listing(&pool, host).await;

    let first = raw_request(&pool, villa, guest)
        .await
        .expect("first request");
    let second = raw_request(&pool, villa, guest).await;
    match second {
        Err(sqlx::Error::Database(e)) => {
            assert_eq!(e.constraint(), Some("uq_request_live_pair"));
        }
        other => panic!("expected a unique violation, got {other:?}"),
    }

    sqlx::query("UPDATE booking_request SET status = 'expired' WHERE id = $1")
        .bind(first)
        .execute(&pool)
        .await
        .expect("expire first");
    raw_request(&pool, villa, guest)
        .await
        .expect("a new request once the first is no longer live");

    // Another guest on the same listing is unaffected.
    let other = raw_user(&pool).await;
    raw_request(&pool, villa, other)
        .await
        .expect("another guest's request");
}

#[tokio::test]
async fn history_rows_need_exactly_one_subject() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let host = raw_user(&pool).await;
    let guest = raw_user(&pool).await;
    let villa = raw_listing(&pool, host).await;
    let request = raw_request(&pool, villa, guest).await.expect("request");

    let insert = |enquiry: Option<Uuid>, request: Option<Uuid>| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO booking_request_history (enquiry_id, request_id, to_status, reason)
                 VALUES ($1, $2, 'open', 'test')",
            )
            .bind(enquiry)
            .bind(request)
            .execute(&pool)
            .await
        }
    };

    assert!(insert(None, None).await.is_err());
    assert!(insert(None, Some(request)).await.is_ok());

    let conversation = raw_conversation(&pool, villa, guest).await;
    let enquiry: Uuid = sqlx::query_scalar(
        "INSERT INTO booking_enquiry (conversation_id, listing_id, guest_id, date_from, date_to,
             number_of_persons, currency)
         VALUES ($1, $2, $3, '2030-02-01', '2030-02-04', 2, 'USD') RETURNING id",
    )
    .bind(conversation)
    .bind(villa)
    .bind(guest)
    .fetch_one(&pool)
    .await
    .expect("enquiry");
    assert!(insert(Some(enquiry), Some(request)).await.is_err());
    assert!(insert(Some(enquiry), None).await.is_ok());

    let rows: i64 = sqlx::query("SELECT COUNT(*) AS n FROM booking_request_history")
        .fetch_one(&pool)
        .await
        .expect("count")
        .get("n");
    assert_eq!(rows, 2);
}

#[tokio::test]
async fn a_closed_enquiry_needs_a_reason_and_an_open_one_has_none() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let host = raw_user(&pool).await;
    let guest = raw_user(&pool).await;
    let villa = raw_listing(&pool, host).await;
    let conversation = raw_conversation(&pool, villa, guest).await;

    let insert = |status: &'static str, reason: Option<&'static str>| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO booking_enquiry (conversation_id, listing_id, guest_id, date_from,
                     date_to, number_of_persons, currency, status, closed_reason)
                 VALUES ($1, $2, $3, '2030-02-01', '2030-02-04', 2, 'USD',
                     $4::enquiry_status, $5::enquiry_closed_reason)",
            )
            .bind(conversation)
            .bind(villa)
            .bind(guest)
            .bind(status)
            .bind(reason)
            .execute(&pool)
            .await
        }
    };
    assert!(insert("closed", None).await.is_err());
    assert!(insert("open", Some("quoted")).await.is_err());
    assert!(insert("closed", Some("quoted")).await.is_ok());
    assert!(insert("open", None).await.is_ok());
    // A second open enquiry for the same pair is refused.
    assert!(insert("open", None).await.is_err());
}

#[tokio::test]
async fn new_booking_messages_join_the_guest_and_listing_conversation() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let host = raw_user(&pool).await;
    let guest = raw_user(&pool).await;
    let villa = raw_listing(&pool, host).await;
    let first = raw_booking(&pool, guest, villa, "completed").await;
    let second = raw_booking(&pool, guest, villa, "confirmed").await;

    let send = |booking: Uuid, sender: Uuid| {
        let pool = pool.clone();
        async move {
            crate::booking_message::insert_booking_message(
                &pool,
                booking,
                sender,
                crate::models::DbMessageSenderRole::Guest,
                "Guest",
                "Hello",
            )
            .await
            .expect("insert message")
        }
    };
    let m1 = send(first, guest).await;
    let m2 = send(second, host).await;
    assert_eq!(m1.booking_id, Some(first));

    assert_eq!(
        conversation_of(&pool, m1.id).await,
        conversation_of(&pool, m2.id).await
    );
    let listed = crate::booking_message::list_booking_messages(&pool, first)
        .await
        .expect("list messages");
    // A booking shows its whole guest × listing conversation.
    let ids: Vec<Uuid> = listed.iter().map(|m| m.id).collect();
    assert_eq!(ids, vec![m1.id, m2.id]);
}
