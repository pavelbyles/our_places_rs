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
