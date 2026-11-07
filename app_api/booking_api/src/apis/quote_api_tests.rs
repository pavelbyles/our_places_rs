//! Custom quote API tests (#105, plan T25).

use super::authz_tests::{listing, settings, tax_settings, user};
use super::request_api_tests::{act, bearer, request_listing, submit};
use super::stay_pricing_pin_tests::test_db;
use actix_web::{App, test, web};
use db_core::models::UserRole;

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

fn quote(price: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "stay_price": price, "terms": "Wedding allowed until 10pm." })
}

#[tokio::test]
async fn quote_api_tests_preview_matches_the_sent_quote_and_the_hand_figures() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = request_listing(&pool).await;
    let guest = user(&pool, UserRole::Booker).await;
    let app = app!(pool);
    let id = submit(&app, guest, villa, "2030-08-01", "2030-08-04").await;

    let (status, preview) = act(&app, host, &id, "quote-preview", quote("600.00".into())).await;
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["subtotal"], "600.00");
    assert_eq!(preview["tax"], "90.00");
    assert_eq!(preview["platform_fee"], "30.00");
    assert_eq!(preview["total"], "720.00");

    let (status, sent) = act(&app, host, &id, "quote", quote("600.00".into())).await;
    assert_eq!(status, 201, "{sent}");
    assert_eq!(sent["status"], "quoted");
    assert_eq!(sent["quote"]["price"], preview);
    assert_eq!(sent["quote"]["terms"], "Wedding allowed until 10pm.");
    let quote_id = sent["quote"]["id"].as_str().expect("quote id").to_string();

    // The guest accepts the quote they saw and gets a hold at its total.
    let (status, booking) = act(
        &app,
        guest,
        &id,
        "checkout",
        serde_json::json!({ "quote_id": quote_id }),
    )
    .await;
    assert_eq!(status, 201, "{booking}");
    let total: rust_decimal::Decimal = booking["total_price"]
        .as_str()
        .expect("total is a string")
        .parse()
        .expect("a decimal");
    assert_eq!(total.to_string(), "720.00");
}

#[tokio::test]
async fn quote_api_tests_prices_must_be_strings_and_only_the_host_side_may_quote() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = request_listing(&pool).await;
    let guest = user(&pool, UserRole::Booker).await;
    let other_host = user(&pool, UserRole::Host).await;
    let app = app!(pool);
    let id = submit(&app, guest, villa, "2030-08-01", "2030-08-04").await;

    assert_eq!(
        act(&app, host, &id, "quote", quote(600.5.into())).await.0,
        400
    );
    let (status, body) = act(&app, host, &id, "quote", quote("0".into())).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (400, Some("INVALID_QUOTE_PRICE"))
    );
    assert_eq!(
        act(&app, other_host, &id, "quote-preview", quote("600".into()))
            .await
            .0,
        403
    );
    assert_eq!(
        act(&app, other_host, &id, "quote", quote("600".into()))
            .await
            .0,
        403
    );
    assert_eq!(
        act(&app, guest, &id, "quote", quote("600".into())).await.0,
        403
    );

    sqlx::query(
        "UPDATE booking_request SET expires_at = NOW() - INTERVAL '1 minute' WHERE id = $1::uuid",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .expect("lapse");
    let (status, body) = act(&app, host, &id, "quote", quote("600".into())).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (409, Some("REQUEST_EXPIRED"))
    );
}

#[tokio::test]
async fn quote_api_tests_a_quote_on_an_enquiry_becomes_a_quoted_request() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let villa = listing(&pool, host).await;
    let guest = user(&pool, UserRole::Booker).await;
    let app = app!(pool);

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/enquiries")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .set_json(serde_json::json!({
                "listing_id": villa,
                "check_in": "2030-08-01",
                "check_out": "2030-08-04",
                "guests": 2,
                "currency": "USD",
                "message": "Can you do a better price?",
            }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let enquiry: serde_json::Value = test::read_body_json(resp).await;
    let uri = format!(
        "/api/v1/enquiries/{}/quote",
        enquiry["id"].as_str().expect("id")
    );
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&uri)
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(host))
            .set_json(quote("500.00".into()))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let request: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(request["status"], "quoted");
    assert_eq!(request["origin"], "enquiry_quote");
    assert_eq!(request["quote"]["price"]["total"], "600.00");
}
