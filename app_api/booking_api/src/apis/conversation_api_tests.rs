//! Conversation API tests (#105, plan T21).

use super::authz_tests::{settings, tax_settings, user};
use super::request_api_tests::{act, bearer, request_listing, submit};
use super::stay_pricing_pin_tests::test_db;
use actix_web::{App, test, web};
use db_core::models::UserRole;
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

#[tokio::test]
async fn conversation_api_tests_parties_post_and_read_and_others_are_refused() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = request_listing(&pool).await;
    let guest = user(&pool, UserRole::Booker).await;
    let other_host = user(&pool, UserRole::Host).await;
    let app = app!(pool);
    let id = submit(&app, guest, villa, "2030-08-01", "2030-08-04").await;
    let conversation: Uuid =
        sqlx::query_scalar("SELECT conversation_id FROM booking_request WHERE id = $1::uuid")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .expect("conversation");
    let uri = format!("/api/v1/conversations/{conversation}/messages");

    let post = |who: Uuid, text: &str| {
        test::TestRequest::post()
            .uri(&uri)
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(who))
            .set_json(serde_json::json!({ "message_text": text }))
            .to_request()
    };
    let get = |who: Uuid| {
        test::TestRequest::get()
            .uri(&uri)
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(who))
            .to_request()
    };

    assert_eq!(
        test::call_service(&app, post(host, "Happy to host you."))
            .await
            .status(),
        201
    );
    let resp = test::call_service(&app, post(other_host, "Hi")).await;
    assert_eq!(resp.status(), 403);
    assert_eq!(
        test::call_service(&app, get(other_host)).await.status(),
        403
    );
    assert_eq!(
        test::call_service(&app, post(guest, "   ")).await.status(),
        400
    );
    assert_eq!(
        test::call_service(&app, post(guest, "bad\u{0007}"))
            .await
            .status(),
        400
    );

    let resp = test::call_service(&app, get(guest)).await;
    assert_eq!(resp.status(), 200);
    let thread: serde_json::Value = test::read_body_json(resp).await;
    let texts: Vec<&str> = thread["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .filter_map(|m| m["message_text"].as_str())
        .collect();
    assert_eq!(
        texts,
        ["We're celebrating an anniversary.", "Happy to host you."]
    );
    assert_eq!(thread["unread_count"], 1);
    assert_eq!(thread["messages"][0]["request_id"], id);

    let resp = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&format!("{uri}/read"))
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let marked: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(marked["updated_count"], 1);
}

#[tokio::test]
async fn conversation_api_tests_the_request_thread_shows_on_the_booking_it_became() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = request_listing(&pool).await;
    let guest = user(&pool, UserRole::Booker).await;
    let app = app!(pool);
    let id = submit(&app, guest, villa, "2030-08-01", "2030-08-04").await;
    assert_eq!(
        act(&app, host, &id, "accept", serde_json::json!({}))
            .await
            .0,
        200
    );
    let (status, booking) = act(&app, guest, &id, "checkout", serde_json::json!({})).await;
    assert_eq!(status, 201);
    let booking_id = booking["id"].as_str().expect("id");

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/v1/bookings/{booking_id}/messages"))
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let thread: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(
        thread["messages"][0]["message_text"],
        "We're celebrating an anniversary."
    );
}
