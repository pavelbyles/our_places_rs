use super::*;
use actix_web::{App, test, web};
use api_core::settings::{Application, DatabaseSettings, Env, FeatureFlags, Log, Server, Settings};
use chrono::Utc;
use db_core::models::NewUser;
use db_core::models::{NewListing, StructureType};
use db_core::user::create_user;
use rust_decimal_macros::dec;
use sqlx::PgExecutor;
use sqlx_db_tester::TestPg;
use std::env;
use std::path::Path;

async fn create_test_user<'e, E>(executor: E) -> Uuid
where
    E: PgExecutor<'e>,
{
    let id = Uuid::now_v7();
    let new_user = NewUser {
        id,
        email: format!("test_{}@example.com", id),
        password_hash: "secret".to_string(),
        first_name: "Test".to_string(),
        last_name: "User".to_string(),
        phone_number: None,
        is_active: true,
    };
    create_user(executor, &new_user)
        .await
        .expect("Failed to create test user");
    id
}

/// Helper to get default test settings.
/// Returns settings with hard deletes ENABLED by default.
fn get_test_settings() -> Settings {
    Settings {
        server: Server {
            host: "localhost".to_string(),
            port: 8080,
        },
        database: DatabaseSettings {
            username: "postgres".to_string(),
            password: "password".to_string(),
            port: 5432,
            host: "localhost".to_string(),
            database_name: "test".to_string(),
            cloud: false,
            instance_name: "".to_string(),
        },
        log: Log {
            level: "info".to_string(),
        },
        env: Env::Testing,
        application: Application { max_attempts: 1 },
        feature_flags: FeatureFlags {
            enable_hard_deletes: true,
        },
    }
}

#[actix_web::test]
async fn test_get_listing_by_id_success() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;
    let mut conn = pool.acquire().await.unwrap();

    let user_id = create_test_user(&mut *conn).await;
    let new_listing = NewListing {
        name: "API Test Listing".to_string(),
        user_id,
        description: None,
        listing_structure_id: 1,
        country: "Testland".to_string(),
        price_per_night: Some(dec!(100.00)),
    };
    let created_listing = db_listing::create_listing(&mut *conn, &new_listing)
        .await
        .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/listings/listing/{}", created_listing.id))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
    let body: ListingResponse = test::read_body_json(resp).await;
    assert_eq!(body.id, created_listing.id);
    assert_eq!(
        body.listing_structure,
        format!("{:?}", StructureType::Apartment)
    );
}

#[actix_web::test]
async fn test_get_listing_by_id_not_found() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");

    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let non_existent_id = Uuid::now_v7();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/listings/listing/{}", non_existent_id))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_create_listing_validation_error() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let mut conn = pool.acquire().await.unwrap();
    let user_id = create_test_user(&mut *conn).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;
    // Create a listing with an EMPTY name
    let invalid_listing = NewListingRequest {
        name: "".to_string(), // invalid name - should not be empty
        user_id,              // Valid user ID
        description: None,
        listing_structure: StructureType::Apartment,
        country: "Testland".to_string(),
        price_per_night: Some(dec!(100.00)),
    };

    let req = test::TestRequest::post()
        .uri("/api/v1/listings/")
        .set_json(&invalid_listing)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_delete_listing() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;
    let mut conn = pool.acquire().await.unwrap();

    let user_id = create_test_user(&mut *conn).await;
    let new_listing = NewListing {
        name: "Delete Me Listing".to_string(),
        user_id,
        description: None,
        listing_structure_id: 1,
        country: "Testland".to_string(),
        price_per_night: Some(dec!(100.00)),
    };
    let created_listing = db_listing::create_listing(&mut *conn, &new_listing)
        .await
        .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    // 1. Soft Delete
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/listings/listing/{}", created_listing.id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);

    // Verify it's gone from GET
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/listings/listing/{}", created_listing.id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);

    // 2. Hard Delete
    let new_listing_2 = NewListing {
        name: "Hard Delete Me".to_string(),
        user_id,
        description: None,
        listing_structure_id: 1,
        country: "Testland".to_string(),
        price_per_night: Some(dec!(100.00)),
    };
    let created_listing_2 = db_listing::create_listing(&mut *conn, &new_listing_2)
        .await
        .unwrap();

    let req = test::TestRequest::delete()
        .uri(&format!(
            "/api/v1/listings/listing/{}?hard_delete=true",
            created_listing_2.id
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);

    // Verify it's gone
    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/listings/listing/{}",
            created_listing_2.id
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_delete_listing_hard_forbidden() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;
    let mut conn = pool.acquire().await.unwrap();

    let user_id = create_test_user(&mut *conn).await;
    let new_listing = NewListing {
        name: "Forbidden Hard Delete".to_string(),
        user_id,
        description: None,
        listing_structure_id: 1,
        country: "Testland".to_string(),
        price_per_night: Some(dec!(100.00)),
    };
    let created_listing = db_listing::create_listing(&mut *conn, &new_listing)
        .await
        .unwrap();

    // Settings with hard delete DISABLED
    let mut settings = get_test_settings();
    settings.feature_flags.enable_hard_deletes = false; // Override default!

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(settings))
            .configure(configure_routes),
    )
    .await;

    let req = test::TestRequest::delete()
        .uri(&format!(
            "/api/v1/listings/listing/{}?hard_delete=true",
            created_listing.id
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_xml_serialization_of_vec() {
    use db_core::models::StructureType;
    use rust_decimal_macros::dec;
    use serde_xml_rs::to_string;

    let response = vec![ListingResponse {
        id: Uuid::now_v7(),
        user_id: Uuid::now_v7(),
        name: "Test".to_string(),
        description: None,
        listing_structure: format!("{:?}", StructureType::Apartment),
        country: "Test".to_string(),
        price_per_night: Some(dec!(100)),
        is_active: true,
        added_at: Utc::now(),
    }];

    let wrapper = ListingsWrapper { listing: response };
    let xml = to_string(&wrapper);
    assert!(xml.is_ok(), "XML serialization failed: {:?}", xml.err());
}

#[actix_web::test]
async fn test_update_listing_multiple_times() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;
    let mut conn = pool.acquire().await.unwrap();

    let user_id = create_test_user(&mut *conn).await;
    let original_name = "Original Name".to_string();
    let new_listing = NewListing {
        name: original_name.clone(),
        user_id,
        description: None,
        listing_structure_id: 1,
        country: "Testland".to_string(),
        price_per_night: Some(dec!(100.00)),
    };
    let created_listing = db_listing::create_listing(&mut *conn, &new_listing)
        .await
        .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    // 1. First Update
    let updated_name_1 = "Updated Name 1".to_string();
    let update_req_1 = UpdatedListingRequest {
        name: Some(updated_name_1.clone()),
        description: None,
        listing_structure: None,
        country: None,
        price_per_night: None,
        is_active: None,
    };
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/listings/listing/{}", created_listing.id))
        .set_json(&update_req_1)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: ListingResponse = test::read_body_json(resp).await;
    assert_eq!(body.name, updated_name_1);

    // 2. Second Update
    let updated_name_2 = "Updated Name 2".to_string();
    let update_req_2 = UpdatedListingRequest {
        name: Some(updated_name_2.clone()),
        description: None,
        listing_structure: None,
        country: None,
        price_per_night: None,
        is_active: None,
    };
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/listings/listing/{}", created_listing.id))
        .set_json(&update_req_2)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: ListingResponse = test::read_body_json(resp).await;
    assert_eq!(body.name, updated_name_2);
}
