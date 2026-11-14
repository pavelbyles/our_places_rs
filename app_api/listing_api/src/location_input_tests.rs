//! Spec 103 §1.1: `location_privacy` on create and update, and coordinate
//! input validation.

use super::tests::{admin_auth, create_test_user, get_test_settings};
use super::*;
use actix_web::{App, test, web};
use common::location::LocationPrivacy;
use serde_json::{Value, json};
use sqlx_db_tester::TestPg;
use std::path::Path;

/// Env var holding the test database URL.
const DATABASE_URL_ENV: &str = "DATABASE_URL";

fn listing_body(user_id: Uuid, extra: Value) -> Value {
    let mut body = json!({
        "name": format!("Input Villa {}", Uuid::now_v7()),
        "user_id": user_id,
        "listing_structure": "Villa",
        "country": "Jamaica",
        "price_per_night": "300.00",
        "max_guests": 4,
        "bedrooms": 2,
        "beds": 2,
        "full_bathrooms": 2,
        "half_bathrooms": 0,
        "city": "Ocho Rios",
        "base_currency": "USD",
        "guests_included": 1
    });
    if let (Some(base), Some(more)) = (body.as_object_mut(), extra.as_object()) {
        for (k, v) in more {
            base.insert(k.clone(), v.clone());
        }
    }
    body
}

macro_rules! app {
    ($pool:expr) => {
        test::init_service(
            App::new()
                .app_data(crate::location_key::test_key_data())
                .app_data(web::Data::new($pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await
    };
}

async fn test_pool() -> (TestPg, sqlx::PgPool) {
    dotenvy::dotenv().ok();
    let db_url = match std::env::var(DATABASE_URL_ENV) {
        Ok(url) => url,
        Err(_) => panic!("DATABASE_URL must be set"),
    };
    let test_db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
    let pool = test_db.get_pool().await;
    (test_db, pool)
}

async fn host(pool: &sqlx::PgPool) -> Uuid {
    match pool.acquire().await {
        Ok(mut conn) => create_test_user(&mut conn).await,
        Err(e) => panic!("acquire: {e}"),
    }
}

#[actix_web::test]
async fn privacy_defaults_on_create_and_is_kept_or_changed_on_update() {
    let (_db, pool) = test_pool().await;
    let user_id = host(&pool).await;
    let app = app!(pool);

    let created: ListingResponse = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .insert_header(admin_auth())
            .uri("/api/v1/listings")
            .set_json(listing_body(user_id, json!({})))
            .to_request(),
    )
    .await;
    assert_eq!(created.location_privacy, Some(LocationPrivacy::Approximate));

    let exact: ListingResponse = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .insert_header(admin_auth())
            .uri("/api/v1/listings")
            .set_json(listing_body(user_id, json!({"location_privacy": "exact"})))
            .to_request(),
    )
    .await;
    assert_eq!(exact.location_privacy, Some(LocationPrivacy::Exact));

    let kept: ListingResponse = test::call_and_read_body_json(
        &app,
        test::TestRequest::patch()
            .insert_header(admin_auth())
            .uri(&format!("/api/v1/listings/{}", exact.id))
            .set_json(json!({"name": "Renamed Villa"}))
            .to_request(),
    )
    .await;
    assert_eq!(kept.location_privacy, Some(LocationPrivacy::Exact));

    let changed: ListingResponse = test::call_and_read_body_json(
        &app,
        test::TestRequest::patch()
            .insert_header(admin_auth())
            .uri(&format!("/api/v1/listings/{}", exact.id))
            .set_json(json!({"location_privacy": "approximate"}))
            .to_request(),
    )
    .await;
    assert_eq!(changed.location_privacy, Some(LocationPrivacy::Approximate));
}

#[actix_web::test]
async fn an_unknown_privacy_value_is_refused() {
    let (_db, pool) = test_pool().await;
    let user_id = host(&pool).await;
    let app = app!(pool);
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .insert_header(admin_auth())
            .uri("/api/v1/listings")
            .set_json(listing_body(user_id, json!({"location_privacy": "nearby"})))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);
    let body = String::from_utf8_lossy(&test::read_body(resp).await).to_string();
    assert!(body.contains("nearby"), "{body}");
}

#[actix_web::test]
async fn bad_coordinates_are_refused_naming_their_field() {
    let (_db, pool) = test_pool().await;
    let user_id = host(&pool).await;
    let app = app!(pool);

    let cases = [
        (json!({"latitude": 18.4}), "longitude"),
        (json!({"longitude": -77.1}), "latitude"),
        (json!({"latitude": 91.0, "longitude": -77.1}), "latitude"),
        (json!({"latitude": 18.4, "longitude": -181.0}), "longitude"),
    ];
    for (extra, field) in cases {
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .insert_header(admin_auth())
                .uri("/api/v1/listings")
                .set_json(listing_body(user_id, extra.clone()))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 400, "{extra}");
        let body: Value = test::read_body_json(resp).await;
        assert!(body.get(field).is_some(), "{extra}: {body}");
    }

    // The same rules apply to an update.
    let created: ListingResponse = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .insert_header(admin_auth())
            .uri("/api/v1/listings")
            .set_json(listing_body(user_id, json!({})))
            .to_request(),
    )
    .await;
    let resp = test::call_service(
        &app,
        test::TestRequest::patch()
            .insert_header(admin_auth())
            .uri(&format!("/api/v1/listings/{}", created.id))
            .set_json(json!({"latitude": 18.4}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert!(body.get("longitude").is_some(), "{body}");
}

#[actix_web::test]
async fn non_finite_coordinates_never_reach_the_database() {
    let (_db, pool) = test_pool().await;
    let user_id = host(&pool).await;
    let app = app!(pool);
    // JSON has no NaN or Infinity; a string or an out-of-range literal is
    // the nearest a client can send, and both are refused.
    for raw in [
        r#""latitude": "NaN", "longitude": -77.1"#,
        r#""latitude": 1e400, "longitude": -77.1"#,
    ] {
        let mut body = listing_body(user_id, json!({})).to_string();
        body.insert_str(1, &format!("{raw}, "));
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .insert_header(admin_auth())
                .insert_header(("Content-Type", "application/json"))
                .uri("/api/v1/listings")
                .set_payload(body)
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 400, "{raw}");
    }
    // And the validator refuses them if they ever arrive some other way.
    assert!(common::location::validate_coordinates(Some(f64::NAN), Some(-77.1)).is_err());
    assert!(common::location::validate_coordinates(Some(18.4), Some(f64::INFINITY)).is_err());
    assert!(common::location::validate_coordinates(Some(18.4), Some(-77.1)).is_ok());
    assert!(common::location::validate_coordinates(None, None).is_ok());
}
