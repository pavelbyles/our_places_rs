//! Schema tests for the enquiries and requests migration (#105).

use crate::promo_tests::{migrated_db, raw_booking, raw_listing, raw_user};
use sqlx::migrate::Migrator;
use sqlx::{PgPool, Row};
use sqlx_db_tester::TestPg;
use std::env;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// File stem of the migration under test.
const REQUEST_MIGRATION: &str = "enquiries_and_booking_requests";

fn db_url() -> String {
    dotenvy::dotenv().ok();
    env::var(crate::DATABASE_URL_ENV)
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".to_string())
}

/// A temp copy of every migration before the one under test.
fn previous_migrations_dir() -> PathBuf {
    let dir = env::temp_dir().join(format!("op_prev_request_migrations_{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).expect("create temp migrations dir");
    let mut names: Vec<_> = std::fs::read_dir("migrations")
        .expect("read migrations dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".sql"))
        .collect();
    names.sort();
    let cutoff = names
        .iter()
        .position(|n| n.contains(REQUEST_MIGRATION))
        .expect("request migration exists");
    for name in &names[..cutoff] {
        std::fs::copy(Path::new("migrations").join(name), dir.join(name)).expect("copy migration");
    }
    dir
}

async fn old_message(pool: &PgPool, booking: Uuid, sender: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO booking_message (id, booking_id, sender_id, sender_role, sender_name, message_text)
         VALUES (gen_random_uuid(), $1, $2, 'guest', 'Guest', 'Hello')
         RETURNING id",
    )
    .bind(booking)
    .bind(sender)
    .fetch_one(pool)
    .await
    .expect("insert message")
}

async fn conversation_of(pool: &PgPool, message: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT conversation_id FROM booking_message WHERE id = $1")
        .bind(message)
        .fetch_one(pool)
        .await
        .expect("read conversation_id")
}

#[tokio::test]
async fn existing_messages_are_grouped_into_one_conversation_per_guest_and_listing() {
    let prev = previous_migrations_dir();
    let test_db = TestPg::new(db_url(), prev.clone());
    let pool = test_db.get_pool().await;

    let host = raw_user(&pool).await;
    let guest = raw_user(&pool).await;
    let other_guest = raw_user(&pool).await;
    let villa = raw_listing(&pool, host).await;
    let first_stay = raw_booking(&pool, guest, villa, "completed").await;
    let second_stay = raw_booking(&pool, guest, villa, "confirmed").await;
    let other_stay = raw_booking(&pool, other_guest, villa, "confirmed").await;
    let m1 = old_message(&pool, first_stay, guest).await;
    let m2 = old_message(&pool, second_stay, host).await;
    let m3 = old_message(&pool, other_stay, other_guest).await;

    Migrator::new(Path::new("migrations"))
        .await
        .expect("load migrations")
        .run(&pool)
        .await
        .expect("request migration applies to existing data");

    let c1 = conversation_of(&pool, m1).await;
    assert_eq!(c1, conversation_of(&pool, m2).await);
    assert_ne!(c1, conversation_of(&pool, m3).await);

    let pair: (Uuid, Uuid) =
        sqlx::query_as("SELECT listing_id, guest_id FROM conversation WHERE id = $1")
            .bind(c1)
            .fetch_one(&pool)
            .await
            .expect("conversation row");
    assert_eq!(pair, (villa, guest));

    let nullable: String = sqlx::query_scalar(
        "SELECT is_nullable FROM information_schema.columns
         WHERE table_name = 'booking_message' AND column_name = 'conversation_id'",
    )
    .fetch_one(&pool)
    .await
    .expect("column info");
    assert_eq!(nullable, "NO");

    let mode: String = sqlx::query_scalar("SELECT booking_mode::text FROM listing WHERE id = $1")
        .bind(villa)
        .fetch_one(&pool)
        .await
        .expect("listing mode");
    assert_eq!(mode, "instant");

    let _ = std::fs::remove_dir_all(prev);
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
                Uuid::now_v7(),
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
    assert_eq!(m1.booking_id, first);

    assert_eq!(
        conversation_of(&pool, m1.id).await,
        conversation_of(&pool, m2.id).await
    );
    let listed = crate::booking_message::list_booking_messages(&pool, first)
        .await
        .expect("list messages");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, m1.id);
}
