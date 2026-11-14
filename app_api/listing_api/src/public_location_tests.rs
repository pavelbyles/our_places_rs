//! Spec 103 §1.1.5 and §1.2: public coordinates on search and detail, stored
//! ones for managers, the echo guard, and no coordinates in logs.

use super::tests::logs::capture_logs;
use super::tests::{admin_auth, create_test_user, get_test_settings};
use super::*;
use crate::location_key::test_key_data;
use crate::public_location::public_point;
use actix_web::{App, test, web};
use api_core::models::StoredLocation;
use api_core::tracing_utils::CustomRootSpanBuilder;
use common::location::{LocationPrecision, LocationPrivacy};
use db_core::models::NewListing;
use rust_decimal_macros::dec;
use serde_json::{Value, json};
use sqlx_db_tester::TestPg;
use std::path::Path;
use tracing_actix_web::TracingLogger;

const DATABASE_URL_ENV: &str = "DATABASE_URL";
const LAT: f64 = 18.40741;
const LNG: f64 = -77.10313;
const LAT_TEXT: &str = "18.40741";
const LNG_TEXT: &str = "-77.10313";

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

async fn villa(
    pool: &sqlx::PgPool,
    privacy: LocationPrivacy,
    point: (Option<f64>, Option<f64>),
) -> (Uuid, Uuid) {
    let mut conn = match pool.acquire().await {
        Ok(conn) => conn,
        Err(e) => panic!("acquire: {e}"),
    };
    let user_id = create_test_user(&mut conn).await;
    let listing = NewListing {
        name: format!("Public Villa {}", Uuid::now_v7()),
        user_id,
        description: None,
        listing_structure_id: 5,
        country: "Jamaica".to_string(),
        price_per_night: Some(dec!(300.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 4,
        bedrooms: 2,
        beds: 2,
        full_bathrooms: 2,
        half_bathrooms: 0,
        square_meters: None,
        latitude: point.0,
        longitude: point.1,
        listing_details: None,
        city: Some("Ocho Rios".to_string()),
        base_currency: "USD".to_string(),
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: None,
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
        location_privacy: Some(privacy),
    };
    match db_listing::create_listing(&mut *conn, &listing).await {
        Ok(created) => (created.id, user_id),
        Err(e) => panic!("create: {e}"),
    }
}

/// The public point the API should send, from the same test key.
fn expected_public(id: Uuid) -> (f64, f64) {
    let stored = StoredLocation {
        latitude: Some(LAT),
        longitude: Some(LNG),
        privacy: LocationPrivacy::Approximate,
    };
    match public_point(stored, &test_key_data(), id) {
        Some((point, _)) => point,
        None => panic!("no public point"),
    }
}

/// Fails if `body` shows a stored component the public point doesn't share.
/// (A purely east-west offset can leave latitude unchanged at 5 places.)
fn assert_no_stored_point(body: &str, id: Uuid) {
    let (public_lat, public_lng) = expected_public(id);
    if public_lat != LAT {
        assert!(!body.contains(LAT_TEXT), "stored latitude leaked: {body}");
    }
    if public_lng != LNG {
        assert!(!body.contains(LNG_TEXT), "stored longitude leaked: {body}");
    }
    assert!(
        (public_lat, public_lng) != (LAT, LNG),
        "the public point is the stored one"
    );
}

fn haversine(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lat2) = (a.0.to_radians(), b.0.to_radians());
    let h = ((lat2 - lat1) / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * ((b.1 - a.1).to_radians() / 2.0).sin().powi(2);
    2.0 * common::location::EARTH_RADIUS_M * h.sqrt().asin()
}

macro_rules! app {
    ($pool:expr) => {
        test::init_service(
            App::new()
                .app_data(test_key_data())
                .wrap(TracingLogger::<CustomRootSpanBuilder>::new())
                .app_data(web::Data::new($pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await
    };
}

#[actix_web::test]
async fn search_and_detail_send_the_public_point_of_an_approximate_listing() {
    let (_db, pool) = test_pool().await;
    let (id, user_id) = villa(&pool, LocationPrivacy::Approximate, (Some(LAT), Some(LNG))).await;
    let app = app!(pool);

    let search = format!("/api/v1/listings?user_id={user_id}&per_page=100");
    let detail = format!("/api/v1/listings/{id}");
    for (uri, accept) in [
        (search.as_str(), "application/json"),
        (detail.as_str(), "application/json"),
        (search.as_str(), "application/xml"),
        (detail.as_str(), "application/xml"),
    ] {
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(uri)
                .insert_header(("Accept", accept))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200, "{uri} {accept}");
        let body = String::from_utf8_lossy(&test::read_body(resp).await).to_string();
        assert_no_stored_point(&body, id);
        assert!(!body.contains("location_privacy"), "{uri} {accept}: {body}");
    }

    let first: Value =
        test::call_and_read_body_json(&app, test::TestRequest::get().uri(&detail).to_request())
            .await;
    let second: Value =
        test::call_and_read_body_json(&app, test::TestRequest::get().uri(&detail).to_request())
            .await;
    let point = |v: &Value| {
        (
            v["listing"]["latitude"].as_f64().unwrap_or(f64::NAN),
            v["listing"]["longitude"].as_f64().unwrap_or(f64::NAN),
        )
    };
    assert_eq!(point(&first), point(&second), "the public point moved");
    assert_eq!(point(&first), expected_public(id));
    let distance = haversine((LAT, LNG), point(&first));
    assert!((149.0..=501.5).contains(&distance), "{distance} m");
    assert_eq!(first["listing"]["location_precision"], json!("approximate"));

    let found: Vec<ListingResponse> =
        test::call_and_read_body_json(&app, test::TestRequest::get().uri(&search).to_request())
            .await;
    assert_eq!(found.len(), 1);
    assert_eq!(
        (found[0].latitude, found[0].longitude),
        (Some(point(&first).0), Some(point(&first).1))
    );
    assert_eq!(
        found[0].location_precision,
        Some(LocationPrecision::Approximate)
    );
    assert_eq!(found[0].location_privacy, None);
}

#[actix_web::test]
async fn an_exact_listing_shows_its_point_and_a_half_point_shows_nothing() {
    let (_db, pool) = test_pool().await;
    let (exact, _) = villa(&pool, LocationPrivacy::Exact, (Some(LAT), Some(LNG))).await;
    let (half, _) = villa(&pool, LocationPrivacy::Exact, (Some(LAT), None)).await;
    let app = app!(pool);

    let shown: common::models::ListingDetails = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/v1/listings/{exact}"))
            .to_request(),
    )
    .await;
    assert_eq!(
        (shown.listing.latitude, shown.listing.longitude),
        (Some(LAT), Some(LNG))
    );
    assert_eq!(
        shown.listing.location_precision,
        Some(LocationPrecision::Exact)
    );

    let none: common::models::ListingDetails = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/v1/listings/{half}"))
            .to_request(),
    )
    .await;
    assert_eq!(
        (none.listing.latitude, none.listing.longitude),
        (None, None)
    );
    assert_eq!(none.listing.location_precision, None);
}

#[actix_web::test]
async fn managers_get_the_stored_point_on_create_and_update() {
    let (_db, pool) = test_pool().await;
    let user_id = match pool.acquire().await {
        Ok(mut conn) => create_test_user(&mut conn).await,
        Err(e) => panic!("acquire: {e}"),
    };
    let app = app!(pool);
    let created: ListingResponse = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .insert_header(admin_auth())
            .uri("/api/v1/listings")
            .set_json(json!({
                "name": format!("Managed Villa {}", Uuid::now_v7()),
                "user_id": user_id,
                "listing_structure": "Villa",
                "country": "Jamaica",
                "max_guests": 4, "bedrooms": 2, "beds": 2,
                "full_bathrooms": 2, "half_bathrooms": 0,
                "city": "Ocho Rios", "base_currency": "USD", "guests_included": 1,
                "latitude": LAT, "longitude": LNG
            }))
            .to_request(),
    )
    .await;
    assert_eq!(
        (created.latitude, created.longitude),
        (Some(LAT), Some(LNG))
    );
    assert_eq!(created.location_privacy, Some(LocationPrivacy::Approximate));
    assert_eq!(
        created.location_precision,
        Some(LocationPrecision::Approximate)
    );

    let updated: ListingResponse = test::call_and_read_body_json(
        &app,
        test::TestRequest::patch()
            .insert_header(admin_auth())
            .uri(&format!("/api/v1/listings/{}", created.id))
            .set_json(json!({"name": "Still Managed"}))
            .to_request(),
    )
    .await;
    assert_eq!(
        (updated.latitude, updated.longitude),
        (Some(LAT), Some(LNG))
    );
}

async fn stored(pool: &sqlx::PgPool, id: Uuid) -> (Option<f64>, Option<f64>) {
    match db_listing::get_listing_location(pool, id).await {
        Ok(Some(row)) => (row.latitude, row.longitude),
        other => panic!("location of {id}: {other:?}"),
    }
}

#[actix_web::test]
async fn the_echo_guard_keeps_the_stored_point_per_component() {
    let (_db, pool) = test_pool().await;
    let (id, _) = villa(&pool, LocationPrivacy::Approximate, (Some(LAT), Some(LNG))).await;
    let app = app!(pool);
    let (public_lat, public_lng) = expected_public(id);
    let patch = |body: Value| {
        test::TestRequest::patch()
            .insert_header(admin_auth())
            .uri(&format!("/api/v1/listings/{id}"))
            .set_json(body)
            .to_request()
    };

    // A stale form sends back the whole public point.
    let (logs, capture) = capture_logs();
    let resp = test::call_service(
        &app,
        patch(json!({"latitude": public_lat, "longitude": public_lng, "city": "Ocho Rios"})),
    )
    .await;
    drop(capture);
    assert_eq!(resp.status(), 200);
    assert_eq!(stored(&pool, id).await, (Some(LAT), Some(LNG)));
    let text = logs.text();
    assert!(text.contains("kept the stored coordinates"), "{text}");
    assert!(text.contains(" WARN "), "{text}");
    assert!(
        !text.contains(LAT_TEXT) && !text.contains(LNG_TEXT),
        "{text}"
    );

    // Only longitude is new: latitude keeps its stored value.
    let resp = test::call_service(
        &app,
        patch(json!({"latitude": public_lat, "longitude": -77.2, "city": "Ocho Rios"})),
    )
    .await;
    assert_eq!(resp.status(), 200);
    assert_eq!(stored(&pool, id).await, (Some(LAT), Some(-77.2)));

    // A genuinely new point is saved.
    let resp = test::call_service(
        &app,
        patch(json!({"latitude": 18.3, "longitude": -77.3, "city": "Ocho Rios"})),
    )
    .await;
    assert_eq!(resp.status(), 200);
    assert_eq!(stored(&pool, id).await, (Some(18.3), Some(-77.3)));
}

#[actix_web::test]
async fn no_coordinate_reaches_the_logs() {
    let (_db, pool) = test_pool().await;
    let (id, user_id) = villa(&pool, LocationPrivacy::Approximate, (Some(LAT), Some(LNG))).await;
    let app = app!(pool);
    let (public_lat, public_lng) = expected_public(id);

    let (logs, capture) = capture_logs();
    for req in [
        test::TestRequest::get()
            .uri(&format!("/api/v1/listings?user_id={user_id}"))
            .to_request(),
        test::TestRequest::get()
            .uri(&format!("/api/v1/listings/{id}"))
            .to_request(),
        test::TestRequest::patch()
            .insert_header(admin_auth())
            .uri(&format!("/api/v1/listings/{id}"))
            .set_json(json!({"name": "Logged Villa"}))
            .to_request(),
        test::TestRequest::post()
            .insert_header(admin_auth())
            .uri("/api/v1/listings")
            .set_json(json!({
                "name": format!("Logged Villa {}", Uuid::now_v7()),
                "user_id": user_id,
                "listing_structure": "Villa",
                "country": "Jamaica",
                "max_guests": 4, "bedrooms": 2, "beds": 2,
                "full_bathrooms": 2, "half_bathrooms": 0,
                "city": "Ocho Rios", "base_currency": "USD", "guests_included": 1,
                "latitude": LAT, "longitude": LNG
            }))
            .to_request(),
    ] {
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success(), "{}", resp.status());
    }
    drop(capture);
    let text = logs.text();
    assert!(
        text.contains("/api/v1/listings"),
        "nothing was captured: {text}"
    );
    for needle in [
        LAT_TEXT.to_string(),
        LNG_TEXT.to_string(),
        public_lat.to_string(),
        public_lng.to_string(),
    ] {
        assert!(!text.contains(&needle), "{needle} in logs: {text}");
    }
}
