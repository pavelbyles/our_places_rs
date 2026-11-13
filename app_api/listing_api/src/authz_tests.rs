//! Server-side listing authorization (spec §1.8): every write is 401 without
//! a JWT, 404 for a deleted listing, 403 for another host, and allowed for the
//! owner and the admin.

use crate::apis::configure_routes;
use actix_web::http::{Method, StatusCode};
use actix_web::{App, test, web};
use sqlx::PgPool;
use sqlx_db_tester::TestPg;
use std::path::Path;
use uuid::Uuid;

const DATABASE_URL_ENV: &str = "DATABASE_URL";

pub(crate) fn test_db() -> TestPg {
    dotenvy::dotenv().ok();
    let url = std::env::var(DATABASE_URL_ENV)
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".into());
    TestPg::new(url, Path::new("../../db_core/migrations"))
}

/// A user with `roles` (such as `ARRAY['host']`).
pub(crate) async fn user_with(pool: &PgPool, roles: &str) -> Uuid {
    let id: Uuid = sqlx::query_scalar(&format!(
        r#"INSERT INTO "user" (email, password_hash, first_name, last_name, slug, roles, is_active)
           VALUES ('u-' || gen_random_uuid() || '@example.com', 'x', 'U', 'T',
                   'u-' || replace(gen_random_uuid()::text, '-', ''), {roles}::user_role[], TRUE)
           RETURNING id"#
    ))
    .fetch_one(pool)
    .await
    .expect("user");
    if roles.contains("host") {
        db_core::user::create_host_profile(
            pool,
            id,
            &db_core::models::NewHostProfile {
                description: None,
                verified_status: Some("pending".to_string()),
                payout_details: None,
            },
        )
        .await
        .expect("host profile");
    }
    id
}

pub(crate) async fn listing_of(pool: &PgPool, owner: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO listing (is_active, name, listing_structure_id, country, added_at, user_id, slug,
             price_per_night, base_currency)
         VALUES (TRUE, 'Villa ' || gen_random_uuid(), 1, 'Jamaica', NOW(), $1, 'v-' || replace(gen_random_uuid()::text, '-', ''),
             100, 'USD')
         RETURNING id",
    )
    .bind(owner)
    .fetch_one(pool)
    .await
    .expect("listing")
}

pub(crate) fn bearer(user: Uuid) -> (&'static str, String) {
    (
        "Authorization",
        format!("Bearer {}", api_core::auth::generate_test_jwt(user)),
    )
}

pub(crate) fn settings() -> api_core::settings::Settings {
    crate::apis::tests::get_test_settings()
}

/// Every listing write: method, path under the listing, and a valid body.
fn writes(listing: Uuid) -> Vec<(Method, String, Option<serde_json::Value>)> {
    let some = Uuid::now_v7();
    vec![
        (
            Method::PATCH,
            format!("/api/v1/listings/{listing}"),
            Some(serde_json::json!({ "name": "Renamed" })),
        ),
        (
            Method::POST,
            format!("/api/v1/listings/{listing}/images/presign"),
            Some(serde_json::json!({ "images": [] })),
        ),
        (
            Method::DELETE,
            format!("/api/v1/listings/{listing}/images/{some}"),
            None,
        ),
        (
            Method::PATCH,
            format!("/api/v1/listings/{listing}/images/{some}"),
            Some(serde_json::json!({ "name": "Pool deck" })),
        ),
        (
            Method::POST,
            format!("/api/v1/listings/{listing}/price-overrides"),
            Some(serde_json::json!({
                "start_date": "2030-01-01", "end_date": "2030-01-05",
                "nightly_rate": "150.00", "min_nights": 1
            })),
        ),
        (
            Method::PUT,
            format!("/api/v1/listings/{listing}/price-overrides/{some}"),
            Some(serde_json::json!({ "min_nights": 2 })),
        ),
        (
            Method::DELETE,
            format!("/api/v1/listings/{listing}/price-overrides/{some}"),
            None,
        ),
        // Last, because it deletes the listing.
        (Method::DELETE, format!("/api/v1/listings/{listing}"), None),
    ]
}

fn request(
    method: Method,
    uri: &str,
    body: Option<&serde_json::Value>,
    auth: Option<(&'static str, String)>,
) -> actix_http::Request {
    let mut req = test::TestRequest::default().method(method).uri(uri);
    if let Some(header) = auth {
        req = req.insert_header(header);
    }
    match body {
        Some(json) => req.set_json(json).to_request(),
        None => req.to_request(),
    }
}

#[actix_web::test]
async fn every_listing_write_checks_the_caller() {
    let db = test_db();
    let pool = db.get_pool().await;
    let owner = user_with(&pool, "ARRAY['host']").await;
    let other_host = user_with(&pool, "ARRAY['host']").await;
    let admin = user_with(&pool, "ARRAY['admin']").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(settings()))
            .configure(configure_routes),
    )
    .await;

    let listing = listing_of(&pool, owner).await;
    let deleted = listing_of(&pool, owner).await;
    sqlx::query("UPDATE listing SET deleted_at = NOW() WHERE id = $1")
        .bind(deleted)
        .execute(&pool)
        .await
        .expect("delete");

    for (method, uri, body) in writes(listing) {
        let case = format!("{method} {uri}");
        let resp =
            test::call_service(&app, request(method.clone(), &uri, body.as_ref(), None)).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "no JWT: {case}");
        let resp = test::call_service(
            &app,
            request(
                method.clone(),
                &uri,
                body.as_ref(),
                Some(bearer(other_host)),
            ),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "other host: {case}");
    }
    for (method, uri, body) in writes(deleted) {
        let resp = test::call_service(
            &app,
            request(method.clone(), &uri, body.as_ref(), Some(bearer(owner))),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "deleted listing: {method} {uri}"
        );
    }
    for actor in [owner, admin] {
        let target = listing_of(&pool, owner).await;
        for (method, uri, body) in writes(target) {
            let resp = test::call_service(
                &app,
                request(method.clone(), &uri, body.as_ref(), Some(bearer(actor))),
            )
            .await;
            let status = resp.status();
            assert!(
                status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN,
                "{}: {method} {uri} gave {status}",
                if actor == admin { "admin" } else { "owner" }
            );
        }
    }
}

#[actix_web::test]
async fn a_deactivated_account_is_401() {
    let db = test_db();
    let pool = db.get_pool().await;
    let owner = user_with(&pool, "ARRAY['host']").await;
    let listing = listing_of(&pool, owner).await;
    sqlx::query(r#"UPDATE "user" SET is_active = FALSE WHERE id = $1"#)
        .bind(owner)
        .execute(&pool)
        .await
        .expect("deactivate");
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(settings()))
            .configure(configure_routes),
    )
    .await;
    let resp = test::call_service(
        &app,
        request(
            Method::PATCH,
            &format!("/api/v1/listings/{listing}"),
            Some(&serde_json::json!({ "name": "X" })),
            Some(bearer(owner)),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

fn new_listing(owner: Uuid) -> serde_json::Value {
    serde_json::json!({
        "name": format!("Villa {}", Uuid::now_v7()), "user_id": owner, "listing_structure": "Apartment",
        "country": "Jamaica", "price_per_night": "100.00", "max_guests": 2, "bedrooms": 1,
        "beds": 1, "full_bathrooms": 1, "half_bathrooms": 0, "base_currency": "USD",
        "minimum_stay": 1, "days_between_bookings": 0, "guests_included": 1
    })
}

#[actix_web::test]
async fn creating_a_listing_needs_a_host_or_admin_and_hosts_own_what_they_create() {
    let db = test_db();
    let pool = db.get_pool().await;
    let host = user_with(&pool, "ARRAY['host']").await;
    let other_host = user_with(&pool, "ARRAY['host']").await;
    let guest = user_with(&pool, "ARRAY['booker']").await;
    let admin = user_with(&pool, "ARRAY['admin']").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(settings()))
            .configure(configure_routes),
    )
    .await;
    let post = |auth: Option<(&'static str, String)>, owner: Uuid| {
        let body = new_listing(owner);
        let mut req = test::TestRequest::post()
            .uri("/api/v1/listings")
            .insert_header(("Accept", "application/json"));
        if let Some(h) = auth {
            req = req.insert_header(h);
        }
        req.set_json(body).to_request()
    };

    let resp = test::call_service(&app, post(None, host)).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let resp = test::call_service(&app, post(Some(bearer(guest)), guest)).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // A host naming someone else still ends up as the owner.
    let resp = test::call_service(&app, post(Some(bearer(host)), other_host)).await;
    let status = resp.status();
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let id: Uuid = body["id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .expect("id");
    let owner: Uuid = sqlx::query_scalar("SELECT user_id FROM listing WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("owner");
    assert_eq!(owner, host);

    // An admin may name a host, but not a guest or a missing user.
    let resp = test::call_service(&app, post(Some(bearer(admin)), other_host)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    for bad in [guest, Uuid::now_v7()] {
        let resp = test::call_service(&app, post(Some(bearer(admin)), bad)).await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["code"], "VALIDATION_ERROR");
    }
}

#[actix_web::test]
async fn public_reads_stay_public() {
    let db = test_db();
    let pool = db.get_pool().await;
    let owner = user_with(&pool, "ARRAY['host']").await;
    let listing = listing_of(&pool, owner).await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(settings()))
            .configure(configure_routes),
    )
    .await;
    for uri in [
        format!("/api/v1/listings/{listing}"),
        format!("/api/v1/listings/{listing}/price-overrides"),
        "/api/v1/listings".to_string(),
    ] {
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&uri)
                .insert_header(("Accept", "application/json"))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK, "{uri}");
    }
}

/// A processed original photo on `listing`.
async fn photo_on(pool: &PgPool, listing: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO listing_image (listing_id, client_file_id, status, upload_url, display_order)
         VALUES ($1, 'f', 'Processed', 'https://storage.googleapis.com/b/listings/x', 0)
         RETURNING id",
    )
    .bind(listing)
    .fetch_one(pool)
    .await
    .expect("photo")
}

#[actix_web::test]
async fn a_host_names_clears_and_is_refused_long_photo_names() {
    let db = test_db();
    let pool = db.get_pool().await;
    let host = user_with(&pool, "ARRAY['host']").await;
    let listing = listing_of(&pool, host).await;
    let photo = photo_on(&pool, listing).await;
    let other_listing = listing_of(&pool, host).await;
    let other_photo = photo_on(&pool, other_listing).await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(settings()))
            .configure(configure_routes),
    )
    .await;
    let uri = format!("/api/v1/listings/{listing}/images/{photo}");
    let stored = |pool: PgPool| async move {
        sqlx::query_scalar::<_, Option<String>>("SELECT name FROM listing_image WHERE id = $1")
            .bind(photo)
            .fetch_one(&pool)
            .await
            .expect("name")
    };

    let body = serde_json::json!({ "name": "  Ocean terrace " });
    let resp = test::call_service(
        &app,
        request(Method::PATCH, &uri, Some(&body), Some(bearer(host))),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(stored(pool.clone()).await.as_deref(), Some("Ocean terrace"));

    let long = serde_json::json!({ "name": "x".repeat(81) });
    let resp = test::call_service(
        &app,
        request(Method::PATCH, &uri, Some(&long), Some(bearer(host))),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let text = String::from_utf8(test::read_body(resp).await.to_vec()).unwrap_or_default();
    assert!(text.contains("INVALID_PHOTO_NAME"), "{text}");
    assert_eq!(
        stored(pool.clone()).await.as_deref(),
        Some("Ocean terrace"),
        "unchanged"
    );

    let blank = serde_json::json!({ "name": "   " });
    let resp = test::call_service(
        &app,
        request(Method::PATCH, &uri, Some(&blank), Some(bearer(host))),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(stored(pool.clone()).await, None, "blank clears the name");

    // A photo of another listing can't be renamed through this one.
    let wrong = format!("/api/v1/listings/{listing}/images/{other_photo}");
    let resp = test::call_service(
        &app,
        request(Method::PATCH, &wrong, Some(&body), Some(bearer(host))),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // The listing page carries the name.
    let named = serde_json::json!({ "name": "Pool deck" });
    test::call_service(
        &app,
        request(Method::PATCH, &uri, Some(&named), Some(bearer(host))),
    )
    .await;
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/v1/listings/{listing}"))
            .to_request(),
    )
    .await;
    let details: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(details["images"][0]["name"], "Pool deck", "{details}");
}
