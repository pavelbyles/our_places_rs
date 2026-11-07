//! Request and enquiry emails (#105, plan T28).

use super::authz_tests::{listing, settings, tax_settings, user};
use super::request_api_tests::{act, bearer, request_listing, submit};
use super::stay_pricing_pin_tests::test_db;
use actix_web::{App, test, web};
use db_core::models::UserRole;
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

macro_rules! app {
    ($pool:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new($pool.clone()))
                .app_data(web::Data::new(settings()))
                .app_data(web::Data::new(tax_settings()))
                .configure(crate::apis::configure_routes),
        )
        .await
    };
}

async fn email_of(pool: &PgPool, id: Uuid) -> String {
    sqlx::query_scalar(r#"SELECT email FROM "user" WHERE id = $1"#)
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("email")
}

/// The templates queued for `recipient`, oldest first.
async fn templates_for(pool: &PgPool, recipient: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT template_id FROM email_outbox WHERE recipient_email = $1 ORDER BY created_at",
    )
    .bind(recipient)
    .fetch_all(pool)
    .await
    .expect("outbox")
}

/// Waits (without sleeping) until `recipient` has `count` emails queued.
async fn wait_for(pool: &PgPool, recipient: &str, count: usize) -> Vec<String> {
    let waited = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let queued = templates_for(pool, recipient).await;
            if queued.len() >= count {
                return queued;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    match waited {
        Ok(queued) => queued,
        Err(_) => panic!(
            "timed out: {recipient} has {:?}",
            templates_for(pool, recipient).await
        ),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn request_notification_tests_each_step_emails_both_parties() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = request_listing(&pool).await;
    let guest = user(&pool, UserRole::Booker).await;
    let admin = user(&pool, UserRole::Admin).await;
    let (guest_email, host_email) = (email_of(&pool, guest).await, email_of(&pool, host).await);
    let app = app!(pool);

    let id = submit(&app, guest, villa, "2030-08-01", "2030-08-04").await;
    wait_for(&pool, &guest_email, 1).await;
    wait_for(&pool, &host_email, 1).await;
    // The admin accepts on the host's behalf; both parties are told.
    assert_eq!(
        act(&app, admin, &id, "accept", serde_json::json!({}))
            .await
            .0,
        200
    );
    wait_for(&pool, &guest_email, 2).await;
    wait_for(&pool, &host_email, 2).await;
    assert_eq!(
        act(&app, guest, &id, "checkout", serde_json::json!({}))
            .await
            .0,
        201
    );

    let guest_mail = wait_for(&pool, &guest_email, 3).await;
    let host_mail = wait_for(&pool, &host_email, 3).await;
    assert_eq!(
        guest_mail,
        [
            "request_submitted_guest",
            "request_accepted_guest",
            "request_converted_guest"
        ]
    );
    assert_eq!(
        host_mail,
        [
            "request_received_host",
            "request_accepted_host",
            "request_converted_host"
        ]
    );
    let admin_mail = templates_for(&pool, &email_of(&pool, admin).await).await;
    assert!(
        admin_mail.is_empty(),
        "the admin isn't emailed: {admin_mail:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn request_notification_tests_enquiries_quotes_and_declines() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let villa = listing(&pool, host).await;
    sqlx::query("UPDATE listing SET booking_mode = 'both' WHERE id = $1")
        .bind(villa)
        .execute(&pool)
        .await
        .expect("mode");
    let guest = user(&pool, UserRole::Booker).await;
    let (guest_email, host_email) = (email_of(&pool, guest).await, email_of(&pool, host).await);
    let app = app!(pool);

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/enquiries")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .set_json(serde_json::json!({
                "listing_id": villa, "check_in": "2030-08-01", "check_out": "2030-08-04",
                "guests": 2, "currency": "USD", "message": "Is it quiet?",
            }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);
    // Each step's emails come from a detached task; wait for them before the
    // next step so the queued order is deterministic.
    wait_for(&pool, &guest_email, 1).await;
    wait_for(&pool, &host_email, 1).await;
    let id = submit(&app, guest, villa, "2030-09-01", "2030-09-04").await;
    wait_for(&pool, &guest_email, 2).await;
    wait_for(&pool, &host_email, 2).await;
    let quote = serde_json::json!({ "stay_price": "500.00", "terms": "No parties." });
    assert_eq!(act(&app, host, &id, "quote", quote).await.0, 201);
    wait_for(&pool, &guest_email, 3).await;
    wait_for(&pool, &host_email, 3).await;
    let decline = serde_json::json!({ "reason": "other", "note": "Changed plans." });
    assert_eq!(act(&app, host, &id, "decline", decline).await.0, 200);

    assert_eq!(
        wait_for(&pool, &guest_email, 4).await,
        [
            "enquiry_sent_guest",
            "request_submitted_guest",
            "quote_sent_guest",
            "request_declined_guest"
        ]
    );
    assert_eq!(
        wait_for(&pool, &host_email, 4).await,
        [
            "enquiry_received_host",
            "request_received_host",
            "quote_sent_host",
            "request_declined_host"
        ]
    );
    let payload: serde_json::Value = sqlx::query_scalar(
        "SELECT payload FROM email_outbox WHERE recipient_email = $1 AND template_id = 'quote_sent_guest'",
    )
    .bind(&guest_email)
    .fetch_one(&pool)
    .await
    .expect("payload");
    assert_eq!(payload["terms"], "No parties.");
    assert_eq!(payload["total"], "USD 600.00");
}

#[tokio::test(flavor = "multi_thread")]
async fn request_notification_tests_expiry_emails_go_out_once() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = request_listing(&pool).await;
    let guest = user(&pool, UserRole::Booker).await;
    let (guest_email, host_email) = (email_of(&pool, guest).await, email_of(&pool, host).await);
    let app = app!(pool);
    let id = submit(&app, guest, villa, "2030-08-01", "2030-08-04").await;
    wait_for(&pool, &host_email, 1).await;
    sqlx::query(
        "UPDATE booking_request SET expires_at = NOW() - INTERVAL '1 minute' WHERE id = $1::uuid",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .expect("lapse");

    let expired = db_core::booking_request::expire_stale_requests(&pool)
        .await
        .expect("sweep");
    let request_id: Uuid = id.parse().expect("uuid");
    assert_eq!(
        expired.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![request_id]
    );
    assert_eq!(
        super::request_notifications::notify_expired(&pool, request_id).await,
        2
    );
    // A re-run (say the sweep retried) sends nothing more.
    assert_eq!(
        super::request_notifications::notify_expired(&pool, request_id).await,
        0
    );
    assert_eq!(
        templates_for(&pool, &guest_email).await,
        ["request_submitted_guest", "request_expired_guest"]
    );
    assert_eq!(
        templates_for(&pool, &host_email).await,
        ["request_received_host", "request_expired_host"]
    );
}

/// Sets a request's window to `[now - elapsed_h, now + remaining_h)`.
async fn place_window(pool: &PgPool, id: &str, elapsed_h: i32, remaining_h: i32) {
    sqlx::query(
        "UPDATE booking_request
         SET created_at = NOW() - make_interval(hours => $2),
             expires_at = NOW() + make_interval(hours => $3)
         WHERE id = $1::uuid",
    )
    .bind(id)
    .bind(elapsed_h)
    .bind(remaining_h)
    .execute(pool)
    .await
    .expect("place window");
}

#[tokio::test(flavor = "multi_thread")]
async fn request_reminder_tests_the_host_is_reminded_once_inside_the_window() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = request_listing(&pool).await;
    let guest = user(&pool, UserRole::Booker).await;
    let host_email = email_of(&pool, host).await;
    let app = app!(pool);
    let id = submit(&app, guest, villa, "2030-08-01", "2030-08-04").await;
    wait_for(&pool, &host_email, 1).await;
    let remind = || super::request_notifications::send_host_reminders(&pool, 25);

    place_window(&pool, &id, 1, 23).await; // 96% left: too early
    assert_eq!(remind().await, 0);
    place_window(&pool, &id, 20, 4).await; // 17% left: inside 25%
    assert_eq!(remind().await, 1);
    assert_eq!(remind().await, 0, "exactly once");
    assert_eq!(
        templates_for(&pool, &host_email).await,
        ["request_received_host", "request_host_reminder"]
    );

    // A smaller share moves the window: 17% left is outside 10%.
    let other_guest = user(&pool, UserRole::Booker).await;
    let other = submit(&app, other_guest, villa, "2030-09-01", "2030-09-04").await;
    place_window(&pool, &other, 20, 4).await;
    assert_eq!(
        super::request_notifications::send_host_reminders(&pool, 10).await,
        0
    );
    assert_eq!(
        super::request_notifications::send_host_reminders(&pool, 25).await,
        1
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn request_reminder_tests_offers_and_lapsed_requests_get_no_reminder() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = request_listing(&pool).await;
    let (a, b) = (
        user(&pool, UserRole::Booker).await,
        user(&pool, UserRole::Booker).await,
    );
    let app = app!(pool);
    let accepted = submit(&app, a, villa, "2030-08-01", "2030-08-04").await;
    assert_eq!(
        act(&app, host, &accepted, "accept", serde_json::json!({}))
            .await
            .0,
        200
    );
    place_window(&pool, &accepted, 20, 4).await;
    let lapsed = submit(&app, b, villa, "2030-09-01", "2030-09-04").await;
    place_window(&pool, &lapsed, 25, -1).await;
    assert_eq!(
        super::request_notifications::send_host_reminders(&pool, 25).await,
        0
    );
}
