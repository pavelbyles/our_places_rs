//! Spec 103 §2.2: `GET /api/v1/listings/{id}/location` gives managers the
//! stored point, and nobody else anything.

use super::configure_routes;
use super::tests::logs::capture_logs;
use crate::authz_tests::{bearer, listing_of, settings, test_db, user_with};
use actix_web::http::StatusCode;
use actix_web::{App, test, web};
use common::location::{ListingLocationResponse, LocationPrivacy};
use uuid::Uuid;

const LAT: f64 = 18.40741;
const LNG: f64 = -77.10313;

async fn place(pool: &sqlx::PgPool, listing: Uuid, lat: Option<f64>, lng: Option<f64>) {
    let updated = sqlx::query("UPDATE listing SET latitude = $2, longitude = $3 WHERE id = $1")
        .bind(listing)
        .bind(lat)
        .bind(lng)
        .execute(pool)
        .await;
    if let Err(e) = updated {
        panic!("place listing: {e}");
    }
}

#[actix_web::test]
async fn managers_get_the_stored_point_and_others_get_nothing() {
    let db = test_db();
    let pool = db.get_pool().await;
    let owner = user_with(&pool, "ARRAY['host']").await;
    let other_host = user_with(&pool, "ARRAY['host']").await;
    let admin = user_with(&pool, "ARRAY['admin']").await;
    let app = test::init_service(
        App::new()
            .app_data(crate::location_key::test_key_data())
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(settings()))
            .configure(configure_routes),
    )
    .await;

    // Approximate by default: managers still get the stored point.
    let listing = listing_of(&pool, owner).await;
    place(&pool, listing, Some(LAT), Some(LNG)).await;
    let uri = format!("/api/v1/listings/{listing}/location");

    for caller in [owner, admin] {
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&uri)
                .insert_header(bearer(caller))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get(actix_web::http::header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some("no-store")
        );
        let body: ListingLocationResponse = test::read_body_json(resp).await;
        assert_eq!(
            body,
            ListingLocationResponse {
                latitude: Some(LAT),
                longitude: Some(LNG),
                location_privacy: LocationPrivacy::Approximate,
            }
        );
    }

    let (logs, capture) = capture_logs();
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&uri)
            .insert_header(bearer(other_host))
            .to_request(),
    )
    .await;
    drop(capture);
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = String::from_utf8_lossy(&test::read_body(resp).await).to_string();
    assert!(!body.contains("18.40741"), "{body}");
    let text = logs.text();
    assert!(
        text.contains(" WARN ") && text.contains("get_listing_location"),
        "{text}"
    );

    let resp = test::call_service(&app, test::TestRequest::get().uri(&uri).to_request()).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    for missing in [Uuid::now_v7(), {
        let deleted = listing_of(&pool, owner).await;
        let gone = sqlx::query("UPDATE listing SET deleted_at = NOW() WHERE id = $1")
            .bind(deleted)
            .execute(&pool)
            .await;
        if let Err(e) = gone {
            panic!("delete: {e}");
        }
        deleted
    }] {
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/api/v1/listings/{missing}/location"))
                .insert_header(bearer(admin))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{missing}");
    }
}

#[actix_web::test]
async fn a_listing_without_a_whole_point_has_none() {
    let db = test_db();
    let pool = db.get_pool().await;
    let owner = user_with(&pool, "ARRAY['host']").await;
    let app = test::init_service(
        App::new()
            .app_data(crate::location_key::test_key_data())
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(settings()))
            .configure(configure_routes),
    )
    .await;
    for (lat, lng) in [(None, None), (Some(LAT), None)] {
        let listing = listing_of(&pool, owner).await;
        place(&pool, listing, lat, lng).await;
        let body: ListingLocationResponse = test::call_and_read_body_json(
            &app,
            test::TestRequest::get()
                .uri(&format!("/api/v1/listings/{listing}/location"))
                .insert_header(bearer(owner))
                .to_request(),
        )
        .await;
        assert_eq!((body.latitude, body.longitude), (None, None));
    }
}
