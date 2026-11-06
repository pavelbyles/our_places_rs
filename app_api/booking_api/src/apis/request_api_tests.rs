//! Enquiry and request-to-book API tests (#105).

use super::authz_tests::{booking_request, listing, settings, tax_settings, user};
use super::stay_pricing_pin_tests::test_db;
use actix_web::{App, test, web};
use db_core::models::UserRole;
use sqlx::PgPool;
use uuid::Uuid;

/// `Authorization` for `id`.
pub(super) fn bearer(id: Uuid) -> (&'static str, String) {
    (
        "Authorization",
        format!("Bearer {}", api_core::auth::generate_test_jwt(id)),
    )
}

/// Sets a listing's booking mode.
pub(super) async fn set_mode(pool: &PgPool, villa: Uuid, mode: &str) {
    sqlx::query("UPDATE listing SET booking_mode = $1::booking_mode WHERE id = $2")
        .bind(mode)
        .bind(villa)
        .execute(pool)
        .await
        .expect("set booking mode");
}

#[tokio::test]
async fn booking_mode_hold_an_instant_hold_on_a_request_only_listing_is_a_409() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let guest = user(&pool, UserRole::Booker).await;
    let villa = listing(&pool, host).await;
    set_mode(&pool, villa, "request").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(settings()))
            .app_data(web::Data::new(tax_settings()))
            .configure(crate::apis::configure_routes),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/bookings")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .set_json(booking_request(guest, villa))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "BOOKING_MODE_NOT_ALLOWED");

    set_mode(&pool, villa, "both").await;
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/bookings")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .set_json(booking_request(guest, villa))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);
}

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

/// A request body for `[check_in, check_out)`.
pub(super) fn request_body(listing: Uuid, check_in: &str, check_out: &str) -> serde_json::Value {
    serde_json::json!({
        "listing_id": listing,
        "check_in": check_in,
        "check_out": check_out,
        "num_adults": 2,
        "currency": "USD",
        "agreed_cancellation_policy": "Flexible",
        "message": "We're celebrating an anniversary.",
    })
}

/// A request-only listing with its host.
pub(super) async fn request_listing(pool: &PgPool) -> (Uuid, Uuid) {
    let host = user(pool, UserRole::Host).await;
    let villa = listing(pool, host).await;
    set_mode(pool, villa, "request").await;
    (villa, host)
}

#[tokio::test]
async fn request_api_tests_submit_and_read_a_request() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = request_listing(&pool).await;
    let guest = user(&pool, UserRole::Booker).await;
    let other_host = user(&pool, UserRole::Host).await;
    let stranger = user(&pool, UserRole::Booker).await;
    let admin = user(&pool, UserRole::Admin).await;
    let app = app!(pool);

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/booking-requests")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .set_json(request_body(villa, "2030-08-01", "2030-08-04"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let created: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(created["status"], "open");
    assert_eq!(created["guest_id"], guest.to_string());
    assert_eq!(created["requested_price"]["nights"], 3);
    assert!(
        created["requested_price"]["total"].is_string(),
        "amounts are strings"
    );
    assert_eq!(created["promo_applied"], false);
    let id = created["id"].as_str().expect("id").to_string();

    // The dates are now held.
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/bookings/listing/{villa}/unavailable-ranges?from=2030-07-01&to=2030-09-01"
            ))
            .insert_header(("Accept", "application/json"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let ranges: serde_json::Value = test::read_body_json(resp).await;
    assert!(
        ranges.to_string().contains("2030-08-01"),
        "request blocks the calendar: {ranges}"
    );

    // The guest, the host and an admin can read it; nobody else.
    for (who, status) in [
        (guest, 200),
        (host, 200),
        (admin, 200),
        (other_host, 403),
        (stranger, 403),
    ] {
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/api/v1/booking-requests/{id}"))
                .insert_header(("Accept", "application/json"))
                .insert_header(bearer(who))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), status, "{who}");
        if status == 403 {
            let body: serde_json::Value = test::read_body_json(resp).await;
            assert_eq!(body["code"], "NOT_REQUEST_PARTY");
        }
    }

    // `/mine` lists only the caller's.
    for (who, count) in [(guest, 1), (stranger, 0)] {
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/booking-requests/mine")
                .insert_header(("Accept", "application/json"))
                .insert_header(bearer(who))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);
        let mine: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(mine["requests"].as_array().map(Vec::len), Some(count));
    }

    // A second live request on the listing is refused.
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/booking-requests")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .set_json(request_body(villa, "2030-09-01", "2030-09-04"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "REQUEST_ALREADY_LIVE");
}

#[tokio::test]
async fn request_api_tests_refusals_carry_their_codes() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = request_listing(&pool).await;
    let guest = user(&pool, UserRole::Booker).await;
    let instant_host = user(&pool, UserRole::Host).await;
    let instant = listing(&pool, instant_host).await;
    let app = app!(pool);

    let post = |who: Uuid, body: serde_json::Value| {
        test::TestRequest::post()
            .uri("/api/v1/booking-requests")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(who))
            .set_json(body)
            .to_request()
    };

    let tomorrow = (common::availability::jamaica_today(chrono::Utc::now())
        + chrono::Duration::days(1))
    .to_string();
    let today = common::availability::jamaica_today(chrono::Utc::now()).to_string();
    let after = (common::availability::jamaica_today(chrono::Utc::now())
        + chrono::Duration::days(4))
    .to_string();
    let soon = common::booking_request::response_window(
        chrono::Utc::now(),
        24,
        common::availability::jamaica_today(chrono::Utc::now()) + chrono::Duration::days(1),
    );

    let cases = [
        (
            host,
            request_body(villa, "2030-08-01", "2030-08-04"),
            403,
            "OWN_LISTING",
        ),
        (
            guest,
            request_body(instant, "2030-08-01", "2030-08-04"),
            409,
            "BOOKING_MODE_NOT_ALLOWED",
        ),
        (
            guest,
            request_body(villa, &today, &after),
            422,
            "REQUEST_WINDOW_TOO_SHORT",
        ),
    ];
    for (who, body, status, code) in cases {
        let resp = test::call_service(&app, post(who, body)).await;
        assert_eq!(resp.status(), status, "{code}");
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["code"], code);
    }

    // Check-in tomorrow works only while more than an hour is left before it.
    let resp = test::call_service(&app, post(guest, request_body(villa, &tomorrow, &after))).await;
    match soon {
        Ok(_) => assert_eq!(resp.status(), 201),
        Err(_) => assert_eq!(resp.status(), 422),
    }

    // Control characters in the message are refused before anything is stored.
    let mut bad = request_body(villa, "2030-10-01", "2030-10-04");
    bad["message"] = serde_json::json!("hi\u{0007}");
    let resp = test::call_service(&app, post(guest, bad)).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn request_api_tests_message_and_promo_text_stay_out_of_logs() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let (villa, _) = request_listing(&pool).await;
    let guest = user(&pool, UserRole::Booker).await;
    let app = app!(pool);

    let (logs, guard) = super::promo_hold_tests::capture_logs();
    let mut body = request_body(villa, "2030-08-01", "2030-08-04");
    body["message"] = serde_json::json!("PRIVATE-MESSAGE-TEXT");
    body["promo_code"] = serde_json::json!("NOTAREALCODE");
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/booking-requests")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .set_json(body)
            .to_request(),
    )
    .await;
    drop(guard);
    assert_eq!(resp.status(), 422);
    let text = logs.text();
    assert!(text.contains("promo code refused"), "{text}");
    assert!(!text.contains("PRIVATE-MESSAGE-TEXT"), "{text}");
    assert!(!text.contains("NOTAREALCODE"), "{text}");
}
