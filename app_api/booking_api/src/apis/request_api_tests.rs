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

/// Submits a request as `guest` and returns its id.
pub(super) async fn submit<S, B>(app: &S, guest: Uuid, villa: Uuid, from: &str, to: &str) -> String
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = actix_web::Error,
        >,
    B: actix_web::body::MessageBody,
{
    let resp = test::call_service(
        app,
        test::TestRequest::post()
            .uri("/api/v1/booking-requests")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .set_json(request_body(villa, from, to))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let created: serde_json::Value = test::read_body_json(resp).await;
    created["id"].as_str().expect("id").to_string()
}

/// POSTs an action on a request and returns the status and JSON body.
pub(super) async fn act<S, B>(
    app: &S,
    who: Uuid,
    id: &str,
    action: &str,
    body: serde_json::Value,
) -> (u16, serde_json::Value)
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = actix_web::Error,
        >,
    B: actix_web::body::MessageBody,
{
    let resp = test::call_service(
        app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/booking-requests/{id}/{action}"))
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(who))
            .set_json(body)
            .to_request(),
    )
    .await;
    let status = resp.status().as_u16();
    let bytes = test::read_body(resp).await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
async fn request_respond_api_roles_are_enforced() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = request_listing(&pool).await;
    let guest = user(&pool, UserRole::Booker).await;
    let other_host = user(&pool, UserRole::Host).await;
    let admin = user(&pool, UserRole::Admin).await;
    let app = app!(pool);
    let decline = serde_json::json!({ "reason": "group_size", "note": "Max 8." });

    let id = submit(&app, guest, villa, "2030-08-01", "2030-08-04").await;
    // Another host can't touch it; the guest can't answer for the host.
    assert_eq!(
        act(&app, other_host, &id, "accept", serde_json::json!({}))
            .await
            .0,
        403
    );
    assert_eq!(
        act(&app, other_host, &id, "decline", decline.clone())
            .await
            .0,
        403
    );
    let (status, body) = act(&app, guest, &id, "accept", serde_json::json!({})).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (403, Some("NOT_REQUEST_PARTY"))
    );
    // A host can't withdraw the guest's request.
    assert_eq!(
        act(&app, host, &id, "withdraw", serde_json::json!({}))
            .await
            .0,
        403
    );

    let (status, body) = act(&app, host, &id, "accept", serde_json::json!({})).await;
    assert_eq!(status, 200);
    assert_eq!(body["status"], "accepted");
    let (status, body) = act(&app, host, &id, "accept", serde_json::json!({})).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (409, Some("REQUEST_NOT_OPEN"))
    );

    // An admin can decline (withdraw the offer) on any listing.
    let (status, body) = act(&app, admin, &id, "decline", decline.clone()).await;
    assert_eq!(status, 200);
    assert_eq!(body["decline_reason"], "group_size");
    assert_eq!(body["decline_note"], "Max 8.");

    // The guest withdraws their own new request.
    let id = submit(&app, guest, villa, "2030-09-01", "2030-09-04").await;
    let (status, body) = act(&app, guest, &id, "withdraw", serde_json::json!({})).await;
    assert_eq!((status, body["status"].as_str()), (200, Some("withdrawn")));

    // `other` needs a note.
    let id = submit(&app, guest, villa, "2030-10-01", "2030-10-04").await;
    let (status, body) = act(
        &app,
        host,
        &id,
        "decline",
        serde_json::json!({ "reason": "other" }),
    )
    .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (400, Some("DECLINE_NOTE_REQUIRED"))
    );
}

#[tokio::test]
async fn request_respond_api_inactive_listings_refuse_accepts_but_not_declines() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = request_listing(&pool).await;
    let guest = user(&pool, UserRole::Booker).await;
    let app = app!(pool);
    let id = submit(&app, guest, villa, "2030-08-01", "2030-08-04").await;
    sqlx::query("UPDATE listing SET is_active = FALSE WHERE id = $1")
        .bind(villa)
        .execute(&pool)
        .await
        .expect("deactivate");

    let (status, body) = act(&app, host, &id, "accept", serde_json::json!({})).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (409, Some("LISTING_UNAVAILABLE"))
    );
    let (status, _) = act(
        &app,
        host,
        &id,
        "decline",
        serde_json::json!({ "reason": "dates_unavailable" }),
    )
    .await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn request_checkout_api_an_accepted_offer_becomes_a_pending_booking() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = request_listing(&pool).await;
    let guest = user(&pool, UserRole::Booker).await;
    let app = app!(pool);
    let id = submit(&app, guest, villa, "2030-08-01", "2030-08-04").await;

    // Not yet accepted: nothing to check out.
    let (status, body) = act(&app, guest, &id, "checkout", serde_json::json!({})).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (409, Some("REQUEST_NOT_OPEN"))
    );

    assert_eq!(
        act(&app, host, &id, "accept", serde_json::json!({}))
            .await
            .0,
        200
    );
    // Only the guest can check out.
    let (status, body) = act(&app, host, &id, "checkout", serde_json::json!({})).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (403, Some("NOT_REQUEST_PARTY"))
    );

    let (status, booking) = act(&app, guest, &id, "checkout", serde_json::json!({})).await;
    assert_eq!(status, 201, "{booking}");
    assert_eq!(booking["status"], "Pending");
    let booking_id: Uuid = booking["id"].as_str().expect("id").parse().expect("uuid");
    let held = db_core::booking::get_booking_by_id(&pool, booking_id)
        .await
        .expect("booking");
    assert_eq!(held.status, db_core::models::BookingStatus::Pending);
    assert_eq!(
        (held.date_from.to_string(), held.guest_id),
        ("2030-08-01".to_string(), guest)
    );

    // The resulting hold confirms through the usual status endpoint.
    let admin = user(&pool, UserRole::Admin).await;
    let resp = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&format!("/api/v1/bookings/{booking_id}"))
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(admin))
            .set_json(serde_json::json!({ "status": "confirmed" }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);

    // A second checkout of the same offer is refused.
    let (status, body) = act(&app, guest, &id, "checkout", serde_json::json!({})).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (409, Some("REQUEST_NOT_OPEN"))
    );
}
