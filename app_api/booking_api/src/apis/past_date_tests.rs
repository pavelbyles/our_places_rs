//! Past dates and over-limit stays at quote and hold.

use super::authz_tests::{booking_request, listing, settings, tax_settings, user};
use super::stay_pricing::{StayRequest, load_listing, price_stay};
use super::stay_pricing_pin_tests::test_db;
use actix_web::{App, test, web};
use api_core::error::ApiError;
use chrono::{Duration, NaiveDate};
use common::availability::{StayInputError, jamaica_today};
use db_core::error::DbError;
use db_core::models::UserRole;
use uuid::Uuid;

fn bearer(id: Uuid) -> (&'static str, String) {
    (
        "Authorization",
        format!("Bearer {}", api_core::auth::generate_test_jwt(id)),
    )
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

fn today() -> NaiveDate {
    jamaica_today(chrono::Utc::now())
}

fn refused_as_past(body: &serde_json::Value) {
    assert_eq!(body["code"], "INVALID_STAY_DATES", "{body}");
    assert_eq!(
        body["message"], "Invalid date range: dates can't be in the past.",
        "{body}"
    );
}

#[tokio::test]
async fn a_past_check_in_is_refused_everywhere_and_creates_no_hold() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let guest = user(&pool, UserRole::Booker).await;
    let villa = listing(&pool, host).await;
    let app = app!(pool);
    let check_in = today() - Duration::days(1);
    let check_out = today() + Duration::days(2);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/bookings/listing/{villa}/quote?check_in={check_in}&check_out={check_out}"
            ))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400, "public quote");
    refused_as_past(&test::read_body_json(resp).await);

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/bookings/quote")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .set_json(serde_json::json!({
                "listing_id": villa,
                "check_in": check_in,
                "check_out": check_out,
                "guests": 1,
                "currency": "USD",
            }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400, "signed-in quote");
    refused_as_past(&test::read_body_json(resp).await);

    let mut hold = booking_request(guest, villa);
    hold["check_in"] = serde_json::json!(check_in);
    hold["check_out"] = serde_json::json!(check_out);
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/bookings")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .set_json(hold)
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400, "hold");
    refused_as_past(&test::read_body_json(resp).await);

    let holds: i64 = sqlx::query_scalar("SELECT count(*) FROM booking WHERE guest_id = $1")
        .bind(guest)
        .fetch_one(&pool)
        .await
        .expect("count holds");
    assert_eq!(holds, 0);
}

#[tokio::test]
async fn check_in_today_is_accepted() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let villa = listing(&pool, host).await;
    let app = app!(pool);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/bookings/listing/{villa}/quote?check_in={}&check_out={}",
                today(),
                today() + Duration::days(1)
            ))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn over_limit_stays_get_the_typed_error() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let villa = listing(&pool, host).await;
    let app = app!(pool);
    let limit = settings().application.max_stay_nights;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/bookings/listing/{villa}/quote?check_in={}&check_out={}",
                today(),
                today() + Duration::days(limit + 1)
            ))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "INVALID_STAY_DATES");
    assert_eq!(
        body["message"],
        format!("Invalid date range: stays can be at most {limit} nights.")
    );
}

/// Uses a chosen date that differs from the real one, so this can't pass by
/// matching the UTC date for most of the day.
#[tokio::test]
async fn the_date_passed_in_decides_what_counts_as_past() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let villa = listing(&pool, host).await;
    let details = load_listing(&pool, villa).await.expect("listing");
    let chosen = today() + Duration::days(400);

    let price = |check_in: NaiveDate| {
        price_stay(
            &pool,
            StayRequest {
                listing: &details,
                check_in,
                check_out: chosen + Duration::days(3),
                number_of_persons: 1,
                currency: "USD",
                promo: None,
                max_stay_nights: 3650,
                today: chosen,
                locked_rate: None,
                stay_override: None,
            },
        )
    };

    assert!(price(chosen).await.is_ok());
    match price(chosen - Duration::days(1)).await {
        Err(ApiError::Database(DbError::StayInput(StayInputError::InPast))) => {}
        Err(other) => panic!("expected InPast, got {other:?}"),
        Ok(_) => panic!("expected InPast, got a price"),
    }
}

#[tokio::test]
async fn a_stay_below_a_seasonal_minimum_is_refused_with_the_minimum_everywhere() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let guest = user(&pool, UserRole::Booker).await;
    let villa = listing(&pool, host).await;
    let check_in = today() + Duration::days(30);
    let check_out = check_in + Duration::days(4);
    sqlx::query(
        "INSERT INTO listing_price_overrides (listing_id, start_date, end_date, nightly_rate, min_nights)
         VALUES ($1, $2, $3, 300.00, 5)",
    )
    .bind(villa)
    .bind(check_in)
    .bind(check_out + Duration::days(10))
    .execute(&pool)
    .await
    .expect("insert seasonal minimum");
    let app = app!(pool);
    let refused_as_short = |body: serde_json::Value, step: &str| {
        assert_eq!(body["code"], "MINIMUM_STAY_NOT_MET", "{step}: {body}");
        assert_eq!(
            body["message"], "These dates need a stay of at least 5 nights.",
            "{step}: {body}"
        );
        assert_eq!(body["required_nights"], 5, "{step}: {body}");
    };

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/bookings/listing/{villa}/quote?check_in={check_in}&check_out={check_out}"
            ))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400, "public quote");
    refused_as_short(test::read_body_json(resp).await, "public quote");

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/bookings/quote")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .set_json(serde_json::json!({
                "listing_id": villa,
                "check_in": check_in,
                "check_out": check_out,
                "guests": 1,
                "currency": "USD",
            }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400, "signed-in quote");
    refused_as_short(test::read_body_json(resp).await, "signed-in quote");

    let mut hold = booking_request(guest, villa);
    hold["check_in"] = serde_json::json!(check_in);
    hold["check_out"] = serde_json::json!(check_out);
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/bookings")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .set_json(hold)
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400, "hold");
    refused_as_short(test::read_body_json(resp).await, "hold");

    let holds: i64 = sqlx::query_scalar("SELECT count(*) FROM booking WHERE guest_id = $1")
        .bind(guest)
        .fetch_one(&pool)
        .await
        .expect("count holds");
    assert_eq!(holds, 0);
}
