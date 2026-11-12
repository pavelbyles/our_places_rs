use super::*;
use actix_web::{App, test, web};
use api_core::settings::{Application, DatabaseSettings, Env, FeatureFlags, Log, Server, Settings};
use chrono::Utc;
use db_core::models::NewUser;
use db_core::models::{NewListing, StructureType};
use db_core::user::create_user;
use rust_decimal_macros::dec;
use sqlx_db_tester::TestPg;
use std::env;
use std::path::Path;

/// Env var holding the test database URL.
const DATABASE_URL_ENV: &str = "DATABASE_URL";

/// A fixed admin present in every test database that has a test user. The
/// write tests here act as this admin; per-role checks live in `authz_tests`.
pub(super) const TEST_ADMIN: Uuid = Uuid::from_u128(0xad41_0000_0000_0000_0000_0000_0000_0001);

/// `Authorization` for [`TEST_ADMIN`].
pub(super) fn admin_auth() -> (&'static str, String) {
    (
        "Authorization",
        format!("Bearer {}", api_core::auth::generate_test_jwt(TEST_ADMIN)),
    )
}

async fn ensure_test_admin(conn: &mut sqlx::PgConnection) {
    sqlx::query(
        r#"INSERT INTO "user" (id, email, password_hash, first_name, last_name, slug, roles, is_active)
           VALUES ($1, 'test-admin@example.com', 'x', 'Test', 'Admin', 'test-admin',
                   ARRAY['admin']::user_role[], TRUE)
           ON CONFLICT (id) DO NOTHING"#,
    )
    .bind(TEST_ADMIN)
    .execute(&mut *conn)
    .await
    .expect("test admin");
}

pub(super) async fn create_test_user(conn: &mut sqlx::PgConnection) -> Uuid {
    ensure_test_admin(&mut *conn).await;
    let id = Uuid::now_v7();
    let new_user = NewUser {
        id,
        email: format!("test_{}@example.com", id),
        password_hash: "secret".to_string(),
        first_name: "Test".to_string(),
        last_name: "User".to_string(),
        phone_number: None,
        is_active: true,
        is_verified: true,
        verification_code: None,
        verification_code_expires_at: None,
        attributes: serde_json::json!({}),
        roles: Some(vec![db_core::models::UserRole::Host]),
        default_currency: "USD".to_string(),
    };
    create_user(&mut *conn, &new_user)
        .await
        .expect("Failed to create test user");

    let host_profile = db_core::models::NewHostProfile {
        verified_status: Some("unverified".to_string()),
        payout_details: None,
        description: Some("Test Host".to_string()),
    };

    db_core::user::create_host_profile(&mut *conn, id, &host_profile)
        .await
        .expect("Failed to create host profile for test user");

    id
}

/// Helper to get default test settings.
/// Returns settings with hard deletes ENABLED by default.
pub(crate) fn get_test_settings() -> Settings {
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
            database_url: Some(
                "postgres://postgres:password@localhost:5432/our_places".to_string(),
            ),
        },
        log: Log {
            level: "info".to_string(),
        },
        env: Env::Testing,
        application: Application {
            max_attempts: 1,
            pending_hold_minutes: api_core::settings::DEFAULT_PENDING_HOLD_MINUTES,
            rate_effective_grace_seconds: api_core::settings::DEFAULT_RATE_EFFECTIVE_GRACE_SECONDS,
            max_stay_nights: api_core::settings::DEFAULT_MAX_STAY_NIGHTS,
            request_response_hours: api_core::settings::DEFAULT_REQUEST_RESPONSE_HOURS,
            quote_valid_hours: api_core::settings::DEFAULT_QUOTE_VALID_HOURS,
            request_near_expiry_pct: api_core::settings::DEFAULT_REQUEST_NEAR_EXPIRY_PCT,
        },
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

    let user_id = create_test_user(&mut conn).await;
    let new_listing = NewListing {
        name: "API Test Listing".to_string(),
        user_id,
        description: None,
        listing_structure_id: 1,
        country: "Testland".to_string(),
        price_per_night: Some(dec!(100.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 2,
        bedrooms: 1,
        beds: 1,
        full_bathrooms: 1,
        half_bathrooms: 0,
        square_meters: None,
        latitude: None,
        longitude: None,
        listing_details: None,
        city: None,
        base_currency: "USD".to_string(),
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
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
        .uri(&format!("/api/v1/listings/{}", created_listing.id))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
    let body: common::models::ListingDetails = test::read_body_json(resp).await;
    assert_eq!(body.listing.id, created_listing.id);
    assert_eq!(
        body.listing.listing_structure,
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
        .uri(&format!("/api/v1/listings/{}", non_existent_id))
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
    let user_id = create_test_user(&mut conn).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    // Create a listing request with an EMPTY name
    let invalid_listing = common::models::NewListingRequest {
        name: "".to_string(), // invalid name - should not be empty
        user_id,              // Valid user ID
        description: None,
        listing_structure: "Apartment".to_string(),
        country: "Testland".to_string(),
        price_per_night: Some(dec!(100.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 2,
        bedrooms: 1,
        beds: 1,
        full_bathrooms: 1,
        half_bathrooms: 0,
        square_meters: None,
        latitude: None,
        longitude: None,
        listing_details: None,
        city: None,
        base_currency: "USD".to_string(),
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: None,
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: 1,
        extra_guest_fee: None,
        is_active: true,
    };

    let req = test::TestRequest::post()
        .insert_header(admin_auth())
        .uri("/api/v1/listings")
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

    let user_id = create_test_user(&mut conn).await;
    let new_listing = NewListing {
        name: "Delete Me Listing".to_string(),
        user_id,
        description: None,
        listing_structure_id: 1,
        country: "Testland".to_string(),
        price_per_night: Some(dec!(100.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 2,
        bedrooms: 1,
        beds: 1,
        full_bathrooms: 1,
        half_bathrooms: 0,
        square_meters: None,
        latitude: None,
        longitude: None,
        listing_details: None,
        city: None,
        base_currency: "USD".to_string(),
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
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
        .insert_header(admin_auth())
        .uri(&format!("/api/v1/listings/{}", created_listing.id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);

    // Verify it's soft deleted
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/listings/{}", created_listing.id))
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
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 2,
        bedrooms: 1,
        beds: 1,
        full_bathrooms: 1,
        half_bathrooms: 0,
        square_meters: None,
        latitude: None,
        longitude: None,
        listing_details: None,
        city: None,
        base_currency: "USD".to_string(),
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
    };
    let created_listing_2 = db_listing::create_listing(&mut *conn, &new_listing_2)
        .await
        .unwrap();

    let req = test::TestRequest::delete()
        .insert_header(admin_auth())
        .uri(&format!(
            "/api/v1/listings/{}?hard_delete=true",
            created_listing_2.id
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);

    // Verify it's gone
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/listings/{}", created_listing_2.id))
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

    let user_id = create_test_user(&mut conn).await;
    let new_listing = NewListing {
        name: "Forbidden Hard Delete".to_string(),
        user_id,
        description: None,
        listing_structure_id: 1,
        country: "Testland".to_string(),
        price_per_night: Some(dec!(100.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 2,
        bedrooms: 1,
        beds: 1,
        full_bathrooms: 1,
        half_bathrooms: 0,
        square_meters: None,
        latitude: None,
        longitude: None,
        listing_details: None,
        city: None,
        base_currency: "USD".to_string(),
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
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
        .insert_header(admin_auth())
        .uri(&format!(
            "/api/v1/listings/{}?hard_delete=true",
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
        price_unavailable_in: None,
        stay_total: None,
        id: Uuid::now_v7(),
        user_id: Uuid::now_v7(),
        name: "Test".to_string(),
        description: None,
        listing_structure: format!("{:?}", StructureType::Apartment),
        country: "Test".to_string(),
        price_per_night: Some(dec!(100)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        is_active: true,
        added_at: Utc::now(),
        owner_name: None,
        owner_slug: None,
        primary_image_url: None,
        max_guests: 2,
        bedrooms: 1,
        beds: 1,
        full_bathrooms: 1,
        half_bathrooms: 0,
        square_meters: None,
        latitude: None,
        longitude: None,
        overall_rating: None,
        city: None,
        base_currency: "USD".to_string(),
        slug: "test".to_string(),
        listing_details: None,
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: None,
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: 1,
        extra_guest_fee: None,
        booking_mode: common::booking_request::BookingMode::Instant,
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

    let user_id = create_test_user(&mut conn).await;
    let original_name = "Original Name".to_string();
    let new_listing = NewListing {
        name: original_name.clone(),
        user_id,
        description: None,
        listing_structure_id: 1,
        country: "Testland".to_string(),
        price_per_night: Some(dec!(100.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 2,
        bedrooms: 1,
        beds: 1,
        full_bathrooms: 1,
        half_bathrooms: 0,
        square_meters: None,
        latitude: None,
        longitude: None,
        listing_details: None,
        city: None,
        base_currency: "USD".to_string(),
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
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

    // 1. First Update - Name, Structure, Country, Base Currency
    let updated_name_1 = "Updated Name 1".to_string();
    let update_req_1 = UpdatedListingRequest {
        name: Some(updated_name_1.clone()),
        listing_structure: Some("Townhouse".to_string()),
        country: Some("JM".to_string()),
        base_currency: Some("JMD".to_string()),
        ..Default::default()
    };
    let req = test::TestRequest::patch()
        .insert_header(admin_auth())
        .uri(&format!("/api/v1/listings/{}", created_listing.id))
        .set_json(&update_req_1)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: ListingResponse = test::read_body_json(resp).await;
    assert_eq!(body.name, updated_name_1);
    assert_eq!(body.listing_structure, "Townhouse");
    assert_eq!(body.country, "JM");
    assert_eq!(body.base_currency, "JMD");

    // 2. Second Update
    let updated_name_2 = "Updated Name 2".to_string();
    let update_req_2 = UpdatedListingRequest {
        name: Some(updated_name_2.clone()),
        listing_structure: Some("Villa".to_string()),
        country: Some("US".to_string()),
        base_currency: Some("USD".to_string()),
        ..Default::default()
    };
    let req = test::TestRequest::patch()
        .insert_header(admin_auth())
        .uri(&format!("/api/v1/listings/{}", created_listing.id))
        .set_json(&update_req_2)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: ListingResponse = test::read_body_json(resp).await;
    assert_eq!(body.name, updated_name_2);
    assert_eq!(body.listing_structure, "Villa");
    assert_eq!(body.country, "US");
    assert_eq!(body.base_currency, "USD");
}

#[actix_web::test]
async fn test_get_listings_with_filter() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;
    let mut conn = pool.acquire().await.unwrap();

    let user_id = create_test_user(&mut conn).await;

    let listing1 = NewListing {
        name: "Apartment in Paris".to_string(),
        user_id,
        description: None,
        listing_structure_id: 1, // Apartment
        country: "France".to_string(),
        price_per_night: Some(dec!(100.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 2,
        bedrooms: 1,
        beds: 1,
        full_bathrooms: 1,
        half_bathrooms: 0,
        square_meters: None,
        latitude: None,
        longitude: None,
        listing_details: None,
        city: None,
        base_currency: "USD".to_string(),
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
    };
    db_listing::create_listing(&mut *conn, &listing1)
        .await
        .unwrap();

    let listing2 = NewListing {
        name: "House in London".to_string(),
        user_id,
        description: None,
        listing_structure_id: 2, // House
        country: "UK".to_string(),
        price_per_night: Some(dec!(200.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 4,
        bedrooms: 2,
        beds: 2,
        full_bathrooms: 2,
        half_bathrooms: 1,
        square_meters: None,
        latitude: None,
        longitude: None,
        listing_details: None,
        city: None,
        base_currency: "USD".to_string(),
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
    };
    db_listing::create_listing(&mut *conn, &listing2)
        .await
        .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    // Filter by Country (France)
    let req = test::TestRequest::get()
        .uri("/api/v1/listings?country=France")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: Vec<ListingResponse> = test::read_body_json(resp).await;
    assert_eq!(body.len(), 1);
    assert_eq!(body[0].name, "Apartment in Paris");

    // Filter by Price (< 150)
    let req = test::TestRequest::get()
        .uri("/api/v1/listings?max_price=150")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: Vec<ListingResponse> = test::read_body_json(resp).await;
    assert_eq!(body.len(), 1);
    assert_eq!(body[0].name, "Apartment in Paris");
}

#[actix_web::test]
async fn test_create_and_update_listing_with_fees_endpoint() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let mut conn = pool.acquire().await.unwrap();
    let user_id = create_test_user(&mut conn).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    // 1. Create with fees
    let listing_with_fees = common::models::NewListingRequest {
        name: "Fee Villa".to_string(),
        user_id,
        description: None,
        listing_structure: "Villa".to_string(),
        country: "Jamaica".to_string(),
        price_per_night: Some(dec!(250.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 4,
        bedrooms: 2,
        beds: 2,
        full_bathrooms: 2,
        half_bathrooms: 0,
        square_meters: None,
        latitude: None,
        longitude: None,
        listing_details: None,
        city: Some("Montego Bay".to_string()),
        base_currency: "USD".to_string(),
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: None,
        cleaning_fee: Some(dec!(75.00)),
        damage_deposit: Some(dec!(200.00)),
        guests_included: 2,
        extra_guest_fee: Some(dec!(30.00)),
        is_active: true,
    };

    let req = test::TestRequest::post()
        .insert_header(admin_auth())
        .uri("/api/v1/listings")
        .set_json(&listing_with_fees)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let created: ListingResponse = test::read_body_json(resp).await;
    assert_eq!(created.cleaning_fee, Some(dec!(75.00)));
    assert_eq!(created.damage_deposit, Some(dec!(200.00)));
    assert_eq!(created.guests_included, 2);
    assert_eq!(created.extra_guest_fee, Some(dec!(30.00)));

    // 2. Create with fees disabled / None -> defaults to 0.00 / 1
    let listing_no_fees = common::models::NewListingRequest {
        name: "Zero Fee Apartment".to_string(),
        user_id,
        description: None,
        listing_structure: "Apartment".to_string(),
        country: "Jamaica".to_string(),
        price_per_night: Some(dec!(100.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 2,
        bedrooms: 1,
        beds: 1,
        full_bathrooms: 1,
        half_bathrooms: 0,
        square_meters: None,
        latitude: None,
        longitude: None,
        listing_details: None,
        city: None,
        base_currency: "USD".to_string(),
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: None,
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: 1,
        extra_guest_fee: None,
        is_active: true,
    };

    let req = test::TestRequest::post()
        .insert_header(admin_auth())
        .uri("/api/v1/listings")
        .set_json(&listing_no_fees)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let created_no_fees: ListingResponse = test::read_body_json(resp).await;
    assert_eq!(created_no_fees.cleaning_fee, Some(dec!(0.00)));
    assert_eq!(created_no_fees.damage_deposit, Some(dec!(0.00)));
    assert_eq!(created_no_fees.guests_included, 1);
    assert_eq!(created_no_fees.extra_guest_fee, Some(dec!(0.00)));

    // 3. Update fees via PATCH
    let update_req = UpdatedListingRequest {
        cleaning_fee: Some(dec!(100.00)),
        damage_deposit: Some(dec!(350.00)),
        guests_included: Some(3),
        extra_guest_fee: Some(dec!(45.00)),
        ..Default::default()
    };
    let req = test::TestRequest::patch()
        .insert_header(admin_auth())
        .uri(&format!("/api/v1/listings/{}", created.id))
        .set_json(&update_req)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let updated: ListingResponse = test::read_body_json(resp).await;
    assert_eq!(updated.cleaning_fee, Some(dec!(100.00)));
    assert_eq!(updated.damage_deposit, Some(dec!(350.00)));
    assert_eq!(updated.guests_included, 3);
    assert_eq!(updated.extra_guest_fee, Some(dec!(45.00)));

    // 3b. Validation error: guests_included > max_guests on create
    let invalid_guests_included = common::models::NewListingRequest {
        guests_included: 5, // max_guests is 2
        ..listing_no_fees.clone()
    };
    let req = test::TestRequest::post()
        .insert_header(admin_auth())
        .uri("/api/v1/listings")
        .set_json(&invalid_guests_included)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    // 4. Validation error: negative fee
    let invalid_neg = common::models::NewListingRequest {
        cleaning_fee: Some(dec!(-10.00)),
        ..listing_no_fees.clone()
    };
    let req = test::TestRequest::post()
        .insert_header(admin_auth())
        .uri("/api/v1/listings")
        .set_json(&invalid_neg)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    // 5. Validation error: fee > 1,000,000
    let invalid_large = common::models::NewListingRequest {
        damage_deposit: Some(dec!(1000001.00)),
        ..listing_no_fees.clone()
    };
    let req = test::TestRequest::post()
        .insert_header(admin_auth())
        .uri("/api/v1/listings")
        .set_json(&invalid_large)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    // 6. Validation error: scale > 2
    let invalid_scale = common::models::NewListingRequest {
        cleaning_fee: Some(Decimal::new(50005, 3)), // 50.005
        ..listing_no_fees
    };
    let req = test::TestRequest::post()
        .insert_header(admin_auth())
        .uri("/api/v1/listings")
        .set_json(&invalid_scale)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

include!("apis_test_reviews.rs");

mod currency_conversion_tests {
    //! Listing search/detail prices in a requested currency (#100): one
    //! effective-dated lookup, no fallback rates, unavailable prices flagged.

    use super::*;
    use rust_decimal::Decimal;

    struct Ctx {
        _db: TestPg,
        pool: PgPool,
        host: Uuid,
    }

    async fn ctx() -> Ctx {
        dotenvy::dotenv().ok();
        let db_url = env::var(DATABASE_URL_ENV).expect("DATABASE_URL must be set");
        let db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
        let pool = db.get_pool().await;
        let mut conn = pool.acquire().await.expect("conn");
        let host = create_test_user(&mut conn).await;
        Ctx {
            _db: db,
            pool,
            host,
        }
    }

    async fn listing(c: &Ctx, base: &str, price: Decimal) -> Uuid {
        let mut conn = c.pool.acquire().await.expect("conn");
        db_listing::create_listing(
            &mut *conn,
            &NewListing {
                name: format!("Villa {}", Uuid::now_v7()),
                user_id: c.host,
                description: None,
                listing_structure_id: 1,
                country: "Jamaica".to_string(),
                price_per_night: Some(price),
                weekly_discount_percentage: None,
                monthly_discount_percentage: None,
                max_guests: 2,
                bedrooms: 1,
                beds: 1,
                full_bathrooms: 1,
                half_bathrooms: 0,
                square_meters: None,
                latitude: None,
                longitude: None,
                listing_details: None,
                city: None,
                base_currency: base.to_string(),
                minimum_stay: 1,
                days_between_bookings: 0,
                commission_pct: Some(dec!(0.0000)),
                cleaning_fee: None,
                damage_deposit: None,
                guests_included: None,
                extra_guest_fee: None,
                is_active: true,
            },
        )
        .await
        .expect("create listing")
        .id
    }

    async fn sql(c: &Ctx, statement: &str) {
        sqlx::query(statement)
            .execute(&c.pool)
            .await
            .expect("setup statement");
    }

    async fn get(c: &Ctx, uri: &str) -> (u16, serde_json::Value) {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(c.pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await;
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(uri)
                .insert_header(("Accept", "application/json"))
                .to_request(),
        )
        .await;
        let status = resp.status().as_u16();
        let bytes = test::read_body(resp).await;
        let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, body)
    }

    fn find(body: &serde_json::Value, id: Uuid) -> &serde_json::Value {
        let items = body
            .as_array()
            .or_else(|| body["listing"].as_array())
            .expect("listing array");
        items
            .iter()
            .find(|l| l["id"] == id.to_string())
            .expect("listing in results")
    }

    #[actix_web::test]
    async fn search_hides_inactive_listings_unless_asked() {
        let c = ctx().await;
        let active = listing(&c, "USD", dec!(100)).await;
        let inactive = listing(&c, "USD", dec!(200)).await;
        sql(
            &c,
            &format!("UPDATE listing SET is_active = FALSE WHERE id = '{inactive}'"),
        )
        .await;
        let ids = |body: &serde_json::Value| -> Vec<String> {
            body.as_array()
                .expect("listing array")
                .iter()
                .map(|l| l["id"].as_str().expect("id").to_string())
                .collect()
        };

        let (status, body) = get(&c, "/api/v1/listings").await;
        assert_eq!(status, 200);
        assert_eq!(ids(&body), [active.to_string()]);

        let (status, body) = get(&c, "/api/v1/listings?include_inactive=true").await;
        assert_eq!(status, 200);
        assert!(ids(&body).contains(&inactive.to_string()), "{body}");
        assert!(ids(&body).contains(&active.to_string()), "{body}");

        // The admin portal reads the listing it edits by id, so this stays open.
        let (status, _) = get(&c, &format!("/api/v1/listings/{inactive}")).await;
        assert_eq!(status, 200);
    }

    #[actix_web::test]
    async fn search_flags_unquotable_listing_and_converts_the_rest() {
        let c = ctx().await;
        sql(
            &c,
            "INSERT INTO currencies (code, name) VALUES ('TTD', 'Trinidad Dollar')",
        )
        .await;
        let usd = listing(&c, "USD", dec!(100)).await;
        let ttd = listing(&c, "TTD", dec!(700)).await;

        let (status, body) = get(&c, "/api/v1/listings?currency=JMD&per_page=50").await;
        assert_eq!(status, 200);
        let usd_row = find(&body, usd);
        assert_eq!(usd_row["price_per_night"], "16000.00");
        assert_eq!(usd_row["base_currency"], "JMD");
        assert!(usd_row.get("price_unavailable_in").is_none());

        let ttd_row = find(&body, ttd);
        assert!(
            ttd_row["price_per_night"].is_null(),
            "no price without a rate"
        );
        assert_eq!(ttd_row["base_currency"], "TTD", "label is never relabelled");
        assert_eq!(ttd_row["price_unavailable_in"], "JMD");
    }

    #[actix_web::test]
    async fn base_equal_to_target_is_never_flagged() {
        let c = ctx().await;
        let jmd = listing(&c, "JMD", dec!(15000)).await;
        let (status, body) = get(&c, "/api/v1/listings?currency=JMD&per_page=50").await;
        assert_eq!(status, 200);
        let row = find(&body, jmd);
        assert_eq!(row["price_per_night"], "15000.00");
        assert!(row.get("price_unavailable_in").is_none());
    }

    #[actix_web::test]
    async fn malformed_currency_is_400() {
        let c = ctx().await;
        let id = listing(&c, "USD", dec!(100)).await;
        let (search, _) = get(&c, "/api/v1/listings?currency=zz1").await;
        assert_eq!(search, 400);
        let (detail, _) = get(&c, &format!("/api/v1/listings/{id}?currency=usd")).await;
        assert_eq!(detail, 400);
    }

    #[actix_web::test]
    async fn inactive_target_is_422() {
        let c = ctx().await;
        listing(&c, "USD", dec!(100)).await;
        sql(
            &c,
            "UPDATE currencies SET active = false WHERE code = 'GBP'",
        )
        .await;
        let (status, body) = get(&c, "/api/v1/listings?currency=GBP").await;
        assert_eq!(status, 422);
        assert_eq!(body["code"], "CURRENCY_INACTIVE");
    }

    #[actix_web::test]
    async fn detail_with_unresolvable_pair_is_422() {
        let c = ctx().await;
        let id = listing(&c, "USD", dec!(100)).await;
        let (status, body) = get(&c, &format!("/api/v1/listings/{id}?currency=CAD")).await;
        assert_eq!(status, 422);
        assert_eq!(body["code"], "FX_RATE_UNAVAILABLE");
    }

    #[actix_web::test]
    async fn scheduled_rate_does_not_change_prices_yet() {
        let c = ctx().await;
        let id = listing(&c, "USD", dec!(100)).await;
        sql(
            &c,
            "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
             VALUES ('USD', 'JMD', 999, now() + interval '1 day')",
        )
        .await;
        let (status, body) = get(&c, &format!("/api/v1/listings/{id}?currency=JMD")).await;
        assert_eq!(status, 200);
        assert_eq!(body["listing"]["price_per_night"], "16000.00");
    }

    #[actix_web::test]
    async fn detail_price_matches_checkout_conversion() {
        let c = ctx().await;
        let id = listing(&c, "USD", dec!(123.45)).await;
        sql(
            &c,
            "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
             VALUES ('USD', 'JMD', 157.123456, now() - interval '1 minute')",
        )
        .await;
        // Checkout (booking_api) prices the nightly rate as
        // (price × get_rate(.., NewQuote)).round_dp(2); the detail page must agree.
        let rate = db_core::exchange_rate::get_rate_pool(
            &c.pool,
            "USD",
            "JMD",
            Utc::now(),
            db_core::exchange_rate::RateUse::NewQuote,
        )
        .await
        .expect("checkout rate");
        let expected = (dec!(123.45) * rate).round_dp(2).to_string();
        let (status, body) = get(&c, &format!("/api/v1/listings/{id}?currency=JMD")).await;
        assert_eq!(status, 200);
        assert_eq!(body["listing"]["price_per_night"], expected);
    }
}

mod currency_inactive_tests {
    //! Listing writes require an active currency (checked in the write tx).

    use super::*;

    async fn app_for(
        pool: &PgPool,
    ) -> impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    > {
        test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await
    }

    fn new_listing_request(user_id: Uuid, base: &str) -> common::models::NewListingRequest {
        common::models::NewListingRequest {
            name: format!("Villa {}", Uuid::now_v7()),
            user_id,
            description: None,
            listing_structure: "Apartment".to_string(),
            country: "Jamaica".to_string(),
            price_per_night: Some(dec!(100.00)),
            weekly_discount_percentage: None,
            monthly_discount_percentage: None,
            max_guests: 2,
            bedrooms: 1,
            beds: 1,
            full_bathrooms: 1,
            half_bathrooms: 0,
            square_meters: None,
            latitude: None,
            longitude: None,
            listing_details: None,
            city: None,
            base_currency: base.to_string(),
            minimum_stay: 1,
            days_between_bookings: 0,
            commission_pct: None,
            cleaning_fee: None,
            damage_deposit: None,
            guests_included: 1,
            extra_guest_fee: None,
            is_active: true,
        }
    }

    async fn setup() -> (TestPg, PgPool, Uuid) {
        dotenvy::dotenv().ok();
        let db_url = env::var(DATABASE_URL_ENV).expect("DATABASE_URL must be set");
        let db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
        let pool = db.get_pool().await;
        let mut conn = pool.acquire().await.expect("conn");
        let user = create_test_user(&mut conn).await;
        sqlx::query("UPDATE currencies SET active = false WHERE code = 'CAD'")
            .execute(&pool)
            .await
            .expect("deactivate CAD");
        (db, pool, user)
    }

    async fn status_and_code<S>(app: &S, req: actix_http::Request) -> (u16, serde_json::Value)
    where
        S: actix_web::dev::Service<
                actix_http::Request,
                Response = actix_web::dev::ServiceResponse,
                Error = actix_web::Error,
            >,
    {
        let resp = test::call_service(app, req).await;
        let status = resp.status().as_u16();
        let bytes = test::read_body(resp).await;
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        )
    }

    #[actix_web::test]
    async fn create_honours_is_active_and_treats_omitted_as_active() {
        let (_db, pool, user) = setup().await;
        let app = app_for(&pool).await;
        let mut omitted = serde_json::to_value(new_listing_request(user, "USD")).expect("json");
        omitted.as_object_mut().expect("object").remove("is_active");
        let mut inactive = new_listing_request(user, "USD");
        inactive.name = "Inactive villa".to_string();
        inactive.is_active = false;

        for (body, expected) in [
            (omitted, true),
            (serde_json::to_value(inactive).expect("json"), false),
        ] {
            let (status, created) = status_and_code(
                &app,
                test::TestRequest::post()
                    .insert_header(admin_auth())
                    .uri("/api/v1/listings")
                    .insert_header(("Accept", "application/json"))
                    .set_json(body)
                    .to_request(),
            )
            .await;
            assert_eq!(status, 201, "{created}");
            assert_eq!(created["is_active"], expected, "{created}");
        }
    }

    #[actix_web::test]
    async fn create_with_inactive_base_is_422() {
        let (_db, pool, user) = setup().await;
        let app = app_for(&pool).await;
        let (status, body) = status_and_code(
            &app,
            test::TestRequest::post()
                .insert_header(admin_auth())
                .uri("/api/v1/listings")
                .set_json(new_listing_request(user, "CAD"))
                .to_request(),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["code"], "CURRENCY_INACTIVE");
    }

    #[actix_web::test]
    async fn create_with_unknown_base_is_422_not_500() {
        let (_db, pool, user) = setup().await;
        let app = app_for(&pool).await;
        let (status, body) = status_and_code(
            &app,
            test::TestRequest::post()
                .insert_header(admin_auth())
                .uri("/api/v1/listings")
                .set_json(new_listing_request(user, "QQQ"))
                .to_request(),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["code"], "CURRENCY_NOT_FOUND");
    }

    #[actix_web::test]
    async fn update_to_inactive_base_is_422() {
        let (_db, pool, user) = setup().await;
        let mut conn = pool.acquire().await.expect("conn");
        let listing = db_listing::create_listing(
            &mut *conn,
            &NewListing {
                name: format!("Villa {}", Uuid::now_v7()),
                user_id: user,
                description: None,
                listing_structure_id: 1,
                country: "Jamaica".to_string(),
                price_per_night: Some(dec!(100.00)),
                weekly_discount_percentage: None,
                monthly_discount_percentage: None,
                max_guests: 2,
                bedrooms: 1,
                beds: 1,
                full_bathrooms: 1,
                half_bathrooms: 0,
                square_meters: None,
                latitude: None,
                longitude: None,
                listing_details: None,
                city: None,
                base_currency: "USD".to_string(),
                minimum_stay: 1,
                days_between_bookings: 0,
                commission_pct: None,
                cleaning_fee: None,
                damage_deposit: None,
                guests_included: None,
                extra_guest_fee: None,
                is_active: true,
            },
        )
        .await
        .expect("listing");
        let app = app_for(&pool).await;
        let (status, body) = status_and_code(
            &app,
            test::TestRequest::patch()
                .insert_header(admin_auth())
                .uri(&format!("/api/v1/listings/{}", listing.id))
                .set_json(UpdatedListingRequest {
                    base_currency: Some("CAD".to_string()),
                    ..Default::default()
                })
                .to_request(),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["code"], "CURRENCY_INACTIVE");
    }

    #[actix_web::test]
    async fn keeping_an_inactive_base_unchanged_is_allowed() {
        let (_db, pool, user) = setup().await;
        let mut conn = pool.acquire().await.expect("conn");
        // A legacy listing already priced in CAD (deactivated afterwards).
        let id: Uuid = {
            sqlx::query("UPDATE currencies SET active = true WHERE code = 'CAD'")
                .execute(&pool)
                .await
                .expect("reactivate");
            let l = db_listing::create_listing_with_active_currency(
                &pool,
                &NewListing {
                    name: format!("Villa {}", Uuid::now_v7()),
                    user_id: user,
                    description: None,
                    listing_structure_id: 1,
                    country: "Jamaica".to_string(),
                    price_per_night: Some(dec!(100.00)),
                    weekly_discount_percentage: None,
                    monthly_discount_percentage: None,
                    max_guests: 2,
                    bedrooms: 1,
                    beds: 1,
                    full_bathrooms: 1,
                    half_bathrooms: 0,
                    square_meters: None,
                    latitude: None,
                    longitude: None,
                    listing_details: None,
                    city: None,
                    base_currency: "CAD".to_string(),
                    minimum_stay: 1,
                    days_between_bookings: 0,
                    commission_pct: None,
                    cleaning_fee: None,
                    damage_deposit: None,
                    guests_included: None,
                    extra_guest_fee: None,
                    is_active: true,
                },
            )
            .await
            .expect("CAD listing");
            sqlx::query("UPDATE currencies SET active = false WHERE code = 'CAD'")
                .execute(&mut *conn)
                .await
                .expect("deactivate again");
            l.id
        };
        let app = app_for(&pool).await;
        let (status, _) = status_and_code(
            &app,
            test::TestRequest::patch()
                .insert_header(admin_auth())
                .uri(&format!("/api/v1/listings/{id}"))
                .set_json(UpdatedListingRequest {
                    name: Some("Renamed".to_string()),
                    base_currency: Some("CAD".to_string()),
                    ..Default::default()
                })
                .to_request(),
        )
        .await;
        assert_eq!(status, 200);
    }
}

mod gap_limit_tests {
    use super::*;
    use rust_decimal::Decimal;

    fn listing(
        user_id: Uuid,
        gap: i32,
        weekly: Option<Decimal>,
    ) -> common::models::NewListingRequest {
        common::models::NewListingRequest {
            name: format!("Gap Villa {}", Uuid::now_v7()),
            user_id,
            description: None,
            listing_structure: "Villa".to_string(),
            country: "Jamaica".to_string(),
            price_per_night: Some(dec!(250.00)),
            weekly_discount_percentage: weekly,
            monthly_discount_percentage: None,
            max_guests: 4,
            bedrooms: 2,
            beds: 2,
            full_bathrooms: 2,
            half_bathrooms: 0,
            square_meters: None,
            latitude: None,
            longitude: None,
            listing_details: None,
            city: None,
            base_currency: "USD".to_string(),
            minimum_stay: 1,
            days_between_bookings: gap,
            commission_pct: None,
            cleaning_fee: None,
            damage_deposit: None,
            guests_included: 1,
            extra_guest_fee: None,
            is_active: true,
        }
    }

    #[actix_web::test]
    async fn gap_and_discount_limits_apply_on_create_and_update() {
        dotenvy::dotenv().ok();
        let db_url = env::var(DATABASE_URL_ENV).expect("DATABASE_URL must be set");
        let test_db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
        let pool = test_db.get_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let user_id = create_test_user(&mut conn).await;
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await;

        let post = |body: common::models::NewListingRequest| {
            test::TestRequest::post()
                .insert_header(admin_auth())
                .uri("/api/v1/listings")
                .set_json(body)
                .to_request()
        };

        // Create: the limits are accepted, one past them is refused.
        for (gap, expected) in [(0, 201), (365, 201), (-1, 400), (366, 400)] {
            let resp = test::call_service(&app, post(listing(user_id, gap, None))).await;
            assert_eq!(resp.status(), expected, "create with gap {gap}");
        }
        for (pct, expected) in [
            (dec!(0), 201),
            (dec!(100), 201),
            (dec!(-1), 400),
            (dec!(100.01), 400),
        ] {
            let resp = test::call_service(&app, post(listing(user_id, 0, Some(pct)))).await;
            assert_eq!(resp.status(), expected, "create with discount {pct}");
        }

        let resp = test::call_service(&app, post(listing(user_id, 0, None))).await;
        let created: ListingResponse = test::read_body_json(resp).await;
        let uri = format!("/api/v1/listings/{}", created.id);

        // Update: the same limits.
        for (gap, expected) in [(0, 200), (365, 200), (-1, 400), (366, 400)] {
            let req = test::TestRequest::patch()
                .insert_header(admin_auth())
                .uri(&uri)
                .set_json(UpdatedListingRequest {
                    days_between_bookings: Some(gap),
                    ..Default::default()
                })
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), expected, "update with gap {gap}");
        }
        for (pct, expected) in [
            (dec!(0), 200),
            (dec!(100), 200),
            (dec!(-1), 400),
            (dec!(100.01), 400),
        ] {
            let req = test::TestRequest::patch()
                .insert_header(admin_auth())
                .uri(&uri)
                .set_json(UpdatedListingRequest {
                    monthly_discount_percentage: Some(pct),
                    ..Default::default()
                })
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), expected, "update with discount {pct}");
        }
    }
}

/// Captures what the current test logs.
///
/// One process-wide subscriber routes each thread's lines to that thread's
/// capture. `tracing` caches whether a log line is enabled, for all threads,
/// and a line first reached by a thread with no subscriber can be cached as
/// disabled. With a subscriber on every thread, that can't happen.
pub(super) mod logs {
    use std::cell::RefCell;
    use std::io::Write;
    use std::sync::{Arc, Mutex, Once};

    /// Everything the test's thread logged while the capture was held.
    #[derive(Clone, Default)]
    pub(in crate::apis) struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

    impl CapturedLogs {
        pub(in crate::apis) fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().expect("log buffer")).into_owned()
        }
    }

    thread_local! {
        static CURRENT: RefCell<Option<CapturedLogs>> = const { RefCell::new(None) };
    }

    /// Writes to the current thread's capture, if it has one.
    #[derive(Clone, Copy)]
    struct ToCurrentThread;

    /// The destination of one log line.
    struct Line(Option<CapturedLogs>);

    impl Write for Line {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if let Some(logs) = &self.0 {
                logs.0.lock().expect("log buffer").extend_from_slice(buf);
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for ToCurrentThread {
        type Writer = Line;
        fn make_writer(&'a self) -> Self::Writer {
            Line(CURRENT.with(|current| current.borrow().clone()))
        }
    }

    /// Ends the capture when dropped.
    pub(in crate::apis) struct Capture;

    impl Drop for Capture {
        fn drop(&mut self) {
            CURRENT.with(|current| *current.borrow_mut() = None);
        }
    }

    pub(in crate::apis) fn capture_logs() -> (CapturedLogs, Capture) {
        static INSTALL: Once = Once::new();
        INSTALL.call_once(|| {
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .with_span_events(
                    tracing_subscriber::fmt::format::FmtSpan::NEW
                        | tracing_subscriber::fmt::format::FmtSpan::CLOSE,
                )
                .with_ansi(false)
                .with_writer(ToCurrentThread)
                .init();
        });
        let logs = CapturedLogs::default();
        CURRENT.with(|current| *current.borrow_mut() = Some(logs.clone()));
        (logs, Capture)
    }
}

mod search_param_tests {
    use super::logs::capture_logs;
    use super::*;
    use api_core::tracing_utils::CustomRootSpanBuilder;
    use chrono::{Duration, TimeZone};
    use common::availability::{StayInputError, jamaica_today};
    use sqlx::postgres::PgPoolOptions;
    use tracing_actix_web::TracingLogger;

    /// Any query on this pool fails, so a 400 proves none ran.
    fn no_database() -> PgPool {
        PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_millis(200))
            .connect_lazy("postgres://postgres:password@127.0.0.1:1/none")
            .expect("lazy pool")
    }

    fn today() -> chrono::NaiveDate {
        jamaica_today(Utc::now())
    }

    async fn real_database() -> TestPg {
        dotenvy::dotenv().ok();
        let db_url = env::var(DATABASE_URL_ENV).expect("DATABASE_URL must be set");
        TestPg::new(db_url, Path::new("../../db_core/migrations"))
    }

    #[actix_web::test]
    async fn invalid_input_gets_a_typed_400_and_runs_no_query() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(no_database()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await;
        let tomorrow = today() + Duration::days(1);
        let cases = [
            (
                "check_in=abc&check_out=2099-01-02".to_string(),
                "INVALID_STAY_DATES",
                "Invalid date range: please enter valid dates.",
            ),
            (
                format!("check_in={tomorrow}"),
                "INVALID_STAY_DATES",
                "Invalid date range: choose both check-in and check-out.",
            ),
            (
                format!("check_in={tomorrow}&check_out={tomorrow}"),
                "INVALID_STAY_DATES",
                "Invalid date range: check-out must be after check-in.",
            ),
            (
                format!(
                    "check_in={}&check_out={tomorrow}",
                    today() - Duration::days(1)
                ),
                "INVALID_STAY_DATES",
                "Invalid date range: dates can't be in the past.",
            ),
            (
                "guests=0".to_string(),
                "INVALID_GUESTS",
                "Guests must be at least 1.",
            ),
            (
                "guests=abc".to_string(),
                "INVALID_GUESTS",
                "Guests must be at least 1.",
            ),
            (
                format!("q={}", "x".repeat(201)),
                "INVALID_SEARCH_TEXT",
                "Please shorten your search and use only letters, numbers and punctuation.",
            ),
            (
                "q=a%00b".to_string(),
                "INVALID_SEARCH_TEXT",
                "Please shorten your search and use only letters, numbers and punctuation.",
            ),
        ];
        for (query, code, message) in cases {
            let req = test::TestRequest::get()
                .uri(&format!("/api/v1/listings?{query}"))
                .insert_header(("Accept", "application/json"))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 400, "{query}");
            let body: serde_json::Value = test::read_body_json(resp).await;
            assert_eq!(body["code"], code, "{query}");
            assert_eq!(body["message"], message, "{query}");
        }
    }

    #[actix_web::test]
    async fn the_jamaica_date_decides_what_counts_as_past() {
        // 03:00 UTC on the 15th is still the 14th in Jamaica.
        let now = match Utc.with_ymd_and_hms(2030, 6, 15, 3, 0, 0).single() {
            Some(now) => now,
            None => panic!("bad test time"),
        };
        let query = |check_in: &str| common::models::ListingQueryParams {
            check_in: Some(check_in.to_string()),
            check_out: Some("2030-06-20".to_string()),
            ..Default::default()
        };
        assert!(read_stay_search(&query("2030-06-14"), now, 30).is_ok());
        match read_stay_search(&query("2030-06-13"), now, 30) {
            Err(ApiError::Database(db_core::error::DbError::StayInput(StayInputError::InPast))) => {
            }
            other => panic!("expected InPast, got {other:?}"),
        }
    }

    #[actix_web::test]
    async fn a_refused_search_logs_no_error_and_no_search_text() {
        let app = test::init_service(
            App::new()
                .wrap(TracingLogger::<CustomRootSpanBuilder>::new())
                .app_data(web::Data::new(no_database()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await;

        let (logs, capture) = capture_logs();
        let req = test::TestRequest::get()
            .uri("/api/v1/listings?q=SEARCHMARKER&guests=0")
            .to_request();
        let resp = test::call_service(&app, req).await;
        drop(capture);

        assert_eq!(resp.status(), 400);
        let text = logs.text();
        assert!(text.contains("search input refused"), "{text}");
        assert!(text.contains("http.target=/api/v1/listings"), "{text}");
        assert!(!text.contains("SEARCHMARKER"), "{text}");
        assert!(!text.contains(" ERROR "), "{text}");
    }

    #[actix_web::test]
    async fn a_valid_search_filters_and_logs_no_search_text() {
        let test_db = real_database().await;
        let pool = test_db.get_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let user_id = create_test_user(&mut conn).await;
        drop(conn);
        let app = test::init_service(
            App::new()
                .wrap(TracingLogger::<CustomRootSpanBuilder>::new())
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await;

        for (city, max_guests) in [("Runaway Bay", 4), ("Runaway Bay", 2), ("Kingston", 4)] {
            let listing = common::models::NewListingRequest {
                name: format!("Search Villa {}", Uuid::now_v7()),
                user_id,
                description: None,
                listing_structure: "Villa".to_string(),
                country: "Jamaica".to_string(),
                price_per_night: Some(dec!(250.00)),
                weekly_discount_percentage: None,
                monthly_discount_percentage: None,
                max_guests,
                bedrooms: 2,
                beds: 2,
                full_bathrooms: 2,
                half_bathrooms: 0,
                square_meters: None,
                latitude: None,
                longitude: None,
                listing_details: None,
                city: Some(city.to_string()),
                base_currency: "USD".to_string(),
                minimum_stay: 1,
                days_between_bookings: 0,
                commission_pct: None,
                cleaning_fee: None,
                damage_deposit: None,
                guests_included: 1,
                extra_guest_fee: None,
                is_active: true,
            };
            let req = test::TestRequest::post()
                .insert_header(admin_auth())
                .uri("/api/v1/listings")
                .set_json(&listing)
                .to_request();
            assert_eq!(test::call_service(&app, req).await.status(), 201);
        }

        let (check_in, check_out) = (today() + Duration::days(30), today() + Duration::days(33));
        let search = |query: String| {
            test::TestRequest::get()
                .uri(&format!("/api/v1/listings?user_id={user_id}&{query}"))
                .insert_header(("Accept", "application/json"))
                .to_request()
        };

        // Text combines with guests and dates.
        let found: Vec<ListingResponse> = test::call_and_read_body_json(
            &app,
            search(format!(
                "q=runaway&guests=3&check_in={check_in}&check_out={check_out}"
            )),
        )
        .await;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].city.as_deref(), Some("Runaway Bay"));
        assert_eq!(found[0].max_guests, 4);

        // No new params: everything, as today. Empty values count as absent.
        let found: Vec<ListingResponse> =
            test::call_and_read_body_json(&app, search(String::new())).await;
        assert_eq!(found.len(), 3);
        let found: Vec<ListingResponse> = test::call_and_read_body_json(
            &app,
            search("q=&check_in=&check_out=&guests=".to_string()),
        )
        .await;
        assert_eq!(found.len(), 3);

        let (logs, capture) = capture_logs();
        let resp = test::call_service(&app, search("q=SEARCHMARKER&guests=2".to_string())).await;
        drop(capture);
        assert_eq!(resp.status(), 200);
        let text = logs.text();
        assert!(text.contains("http.target=/api/v1/listings"), "{text}");
        assert!(!text.contains("SEARCHMARKER"), "{text}");
    }
}

#[actix_web::test]
async fn booking_mode_defaults_to_instant_and_round_trips_with_history() {
    dotenvy::dotenv().ok();
    let db_url = env::var(DATABASE_URL_ENV).expect("DATABASE_URL must be set");
    let test_db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
    let pool = test_db.get_pool().await;
    let mut conn = pool.acquire().await.unwrap();
    let user_id = create_test_user(&mut conn).await;
    let created = db_listing::create_listing(
        &mut *conn,
        &NewListing {
            name: "Mode Villa".to_string(),
            user_id,
            description: None,
            listing_structure_id: 1,
            country: "Jamaica".to_string(),
            price_per_night: Some(dec!(100.00)),
            weekly_discount_percentage: None,
            monthly_discount_percentage: None,
            max_guests: 2,
            bedrooms: 1,
            beds: 1,
            full_bathrooms: 1,
            half_bathrooms: 0,
            square_meters: None,
            latitude: None,
            longitude: None,
            listing_details: None,
            city: None,
            base_currency: "USD".to_string(),
            minimum_stay: 1,
            days_between_bookings: 0,
            commission_pct: Some(dec!(0.0000)),
            cleaning_fee: None,
            damage_deposit: None,
            guests_included: None,
            extra_guest_fee: None,
            is_active: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        created.booking_mode,
        common::booking_request::BookingMode::Instant
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;
    let resp = test::call_service(
        &app,
        test::TestRequest::patch()
            .insert_header(admin_auth())
            .uri(&format!("/api/v1/listings/{}", created.id))
            .set_json(serde_json::json!({ "booking_mode": "both" }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: ListingResponse = test::read_body_json(resp).await;
    assert_eq!(
        body.booking_mode,
        common::booking_request::BookingMode::Both
    );

    // The previous mode is kept in the history snapshot.
    let previous: Option<String> =
        sqlx::query_scalar("SELECT booking_mode::text FROM listing_history WHERE listing_id = $1")
            .bind(created.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(previous.as_deref(), Some("instant"));

    // A later update that doesn't mention the mode leaves it alone.
    let resp = test::call_service(
        &app,
        test::TestRequest::patch()
            .insert_header(admin_auth())
            .uri(&format!("/api/v1/listings/{}", created.id))
            .set_json(serde_json::json!({ "name": "Renamed" }))
            .to_request(),
    )
    .await;
    let body: ListingResponse = test::read_body_json(resp).await;
    assert_eq!(
        body.booking_mode,
        common::booking_request::BookingMode::Both
    );

    let resp = test::call_service(
        &app,
        test::TestRequest::patch()
            .insert_header(admin_auth())
            .uri(&format!("/api/v1/listings/{}", created.id))
            .set_json(serde_json::json!({ "booking_mode": "sometimes" }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);
}

fn listing_with_details(
    user_id: Uuid,
    details: serde_json::Value,
) -> common::models::NewListingRequest {
    common::models::NewListingRequest {
        name: "Ribbon Villa".to_string(),
        user_id,
        description: None,
        listing_structure: "Villa".to_string(),
        country: "Jamaica".to_string(),
        price_per_night: Some(dec!(100.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 6,
        bedrooms: 3,
        beds: 3,
        full_bathrooms: 2,
        half_bathrooms: 1,
        square_meters: None,
        latitude: None,
        longitude: None,
        listing_details: Some(details),
        city: None,
        base_currency: "USD".to_string(),
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: None,
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: 1,
        extra_guest_fee: None,
        is_active: true,
    }
}

#[actix_web::test]
async fn test_create_listing_refuses_an_invalid_ribbon() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let test_db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
    let pool = test_db.get_pool().await;
    let mut conn = pool.acquire().await.unwrap();
    let user_id = create_test_user(&mut conn).await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    let two_items = serde_json::json!({ "ribbon": [
        { "icon": "bed", "value": "3", "label": "Bedrooms" },
        { "icon": "bath", "value": "2.5", "label": "Bathrooms" },
    ]});
    let req = test::TestRequest::post()
        .insert_header(admin_auth())
        .uri("/api/v1/listings")
        .set_json(listing_with_details(user_id, two_items))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body = String::from_utf8(test::read_body(resp).await.to_vec()).unwrap();
    assert!(body.contains("INVALID_RIBBON"), "{body}");
    assert!(body.contains("between 3 and 6 items"), "{body}");

    let three_items = serde_json::json!({ "ribbon": [
        { "icon": "bed", "value": "3", "label": "Bedrooms" },
        { "icon": "bath", "value": "2.5", "label": "Bathrooms" },
        { "icon": "beach", "value": "Direct", "label": "Beach access" },
    ]});
    let req = test::TestRequest::post()
        .insert_header(admin_auth())
        .uri("/api/v1/listings")
        .set_json(listing_with_details(user_id, three_items))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "{}", resp.status());
    let stored: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(
        stored["listing_details"]["ribbon"][2]["value"], "Direct",
        "{stored}"
    );
}
