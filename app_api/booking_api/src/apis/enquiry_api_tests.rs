//! Enquiry API tests (#105, plan T23).

use super::authz_tests::{listing, settings, tax_settings, user};
use super::request_api_tests::{bearer, request_body};
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

fn enquiry_body(listing: Uuid) -> serde_json::Value {
    serde_json::json!({
        "listing_id": listing,
        "check_in": "2030-08-01",
        "check_out": "2030-08-04",
        "guests": 6,
        "currency": "usd",
        "message": "Can we hold a small wedding?",
    })
}

#[tokio::test]
async fn enquiry_api_tests_send_read_and_close() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let villa = listing(&pool, host).await;
    sqlx::query("UPDATE listing SET max_guests = 10, booking_mode = 'both' WHERE id = $1")
        .bind(villa)
        .execute(&pool)
        .await
        .expect("listing");
    let guest = user(&pool, UserRole::Booker).await;
    let other_host = user(&pool, UserRole::Host).await;
    let app = app!(pool);

    let post = |who: Uuid, uri: String, body: serde_json::Value| {
        test::TestRequest::post()
            .uri(&uri)
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(who))
            .set_json(body)
            .to_request()
    };

    let resp = test::call_service(
        &app,
        post(host, "/api/v1/enquiries".into(), enquiry_body(villa)),
    )
    .await;
    assert_eq!(
        resp.status(),
        403,
        "a host can't enquire on their own listing"
    );

    let resp = test::call_service(
        &app,
        post(guest, "/api/v1/enquiries".into(), enquiry_body(villa)),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let enquiry: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(enquiry["status"], "open");
    assert_eq!(enquiry["currency"], "USD");
    let id = enquiry["id"].as_str().expect("id").to_string();

    for (who, status) in [(guest, 200), (host, 200), (other_host, 403)] {
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/api/v1/enquiries/{id}"))
                .insert_header(("Accept", "application/json"))
                .insert_header(bearer(who))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), status, "{who}");
    }

    // A request from someone else's enquiry is refused; from the guest's own it closes it.
    let stranger = user(&pool, UserRole::Booker).await;
    let mut theirs = request_body(villa, "2030-08-01", "2030-08-04");
    theirs["enquiry_id"] = serde_json::json!(id);
    let resp = test::call_service(
        &app,
        post(stranger, "/api/v1/booking-requests".into(), theirs.clone()),
    )
    .await;
    assert_eq!(resp.status(), 403);

    // The guest closes nothing; the host does.
    let resp = test::call_service(
        &app,
        post(
            guest,
            format!("/api/v1/enquiries/{id}/close"),
            serde_json::json!({}),
        ),
    )
    .await;
    assert_eq!(resp.status(), 403);
    let resp = test::call_service(
        &app,
        post(
            host,
            format!("/api/v1/enquiries/{id}/close"),
            serde_json::json!({}),
        ),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let closed: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(closed["closed_reason"], "closed_by_host");

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/booking-requests/mine")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .to_request(),
    )
    .await;
    let mine: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(mine["enquiries"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn enquiry_api_tests_a_request_from_the_guests_enquiry_closes_it() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let villa = listing(&pool, host).await;
    sqlx::query("UPDATE listing SET max_guests = 10, booking_mode = 'request' WHERE id = $1")
        .bind(villa)
        .execute(&pool)
        .await
        .expect("listing");
    let guest = user(&pool, UserRole::Booker).await;
    let app = app!(pool);

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/enquiries")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .set_json(enquiry_body(villa))
            .to_request(),
    )
    .await;
    let enquiry: serde_json::Value = test::read_body_json(resp).await;
    let mut body = request_body(villa, "2030-08-01", "2030-08-04");
    let enquiry_id = enquiry["id"].as_str().expect("id");
    body["enquiry_id"] = enquiry_id.into();
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
    assert_eq!(resp.status(), 201);
    let request: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(request["enquiry_id"], enquiry_id);
    assert_eq!(request["conversation_id"], enquiry["conversation_id"]);

    let reason: String =
        sqlx::query_scalar("SELECT closed_reason::text FROM booking_enquiry WHERE id = $1::uuid")
            .bind(enquiry_id)
            .fetch_one(&pool)
            .await
            .expect("reason");
    assert_eq!(reason, "request_submitted");
}
