use super::{UserResponse, configure_routes};
use actix_web::{App, test, web};
use api_core::settings::{Application, DatabaseSettings, Env, FeatureFlags, Log, Server, Settings};
use db_core::models::UserRole;
use serde_json::json;
use sqlx_db_tester::TestPg;
use std::env;
use std::path::Path;
use uuid::Uuid;

/// Env var holding the test database URL.
const DATABASE_URL_ENV: &str = "DATABASE_URL";

/// Helper to get default test settings.
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
async fn test_create_user_booker_with_profile() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    let email = format!("booker_{}@example.com", Uuid::now_v7());
    let req_body = json!({
        "email": email,
        "password": "password123",
        "first_name": "Test",
        "last_name": "Booker",
        "phone_number": "+1234567890",
        "is_active": true,
        "roles": ["booker"],
        "booker_profile": {
            "emergency_contacts": {"name": "Emergency", "phone": "911"},
            "booking_preferences": {"smoking": false},
            "loyalty": {"points": 100}
        }
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/users/")
        .set_json(&req_body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let body: UserResponse = test::read_body_json(resp).await;
    assert!(body.roles.contains(&UserRole::Booker.to_string()));
}

#[actix_web::test]
async fn test_create_user_host_with_profile() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    let email = format!("host_{}@example.com", Uuid::now_v7());
    let req_body = json!({
        "email": email,
        "password": "password123",
        "first_name": "Test",
        "last_name": "Host",
        "phone_number": "+1987654321",
        "is_active": true,
        "roles": ["host"],
        "host_profile": {
            "verified_status": "verified",
            "payout_details": {"account": "123"},
            "description": "Super host"
        }
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/users/")
        .set_json(&req_body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let body: UserResponse = test::read_body_json(resp).await;
    assert!(body.roles.contains(&UserRole::Host.to_string()));
}

#[actix_web::test]
async fn test_create_user_admin() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    let email = format!("admin_{}@example.com", Uuid::now_v7());
    let req_body = json!({
        "email": email,
        "password": "password123",
        "first_name": "Test",
        "last_name": "Admin",
        "phone_number": "+1122334455",
        "is_active": true,
        "roles": ["admin"],
        "attributes": {"access_level": "super"} // Admin profile attribute as requested
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/users/")
        .set_json(&req_body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let body: UserResponse = test::read_body_json(resp).await;
    assert!(body.roles.contains(&UserRole::Admin.to_string()));
    assert_eq!(body.attributes["access_level"], "super");
}

#[actix_web::test]
async fn test_create_user_mixed_roles() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    let email = format!("mixed_{}@example.com", Uuid::now_v7());
    let req_body = json!({
        "email": email,
        "password": "password123",
        "first_name": "Test",
        "last_name": "Mixed",
        "phone_number": "+1555666777",
        "is_active": true,
        "roles": ["booker", "host"],
        "booker_profile": {
            "loyalty": {"points": 50}
        },
        "host_profile": {
            "verified_status": "pending"
        }
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/users/")
        .set_json(&req_body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let body: UserResponse = test::read_body_json(resp).await;
    assert!(body.roles.contains(&UserRole::Booker.to_string()));
    assert!(body.roles.contains(&UserRole::Host.to_string()));
}

#[actix_web::test]
async fn test_update_user_add_role_and_profile() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    // 1. Create initial user (Booker)
    let email = format!("update_role_{}@example.com", Uuid::now_v7());
    let create_body = json!({
        "email": email,
        "password": "password123",
        "first_name": "Initial",
        "last_name": "User",
        "phone_number": "+5555555555",
        "is_active": true,
        "roles": ["booker"],
        "booker_profile": {
            "loyalty": {"points": 10}
        }
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/users/")
        .set_json(&create_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let user: UserResponse = test::read_body_json(resp).await;
    let user_id = user.id;

    // 2. Update user: Add Host role and profile
    let update_body = json!({
        "roles": ["booker", "host"], // Sending both roles
        "host_profile": {
            "verified_status": "pending",
            "description": "Now a host too"
        }
    });

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/users/user/{}", user_id))
        .set_json(&update_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let updated_user: UserResponse = test::read_body_json(resp).await;
    assert!(updated_user.roles.contains(&UserRole::Booker.to_string()));
    assert!(updated_user.roles.contains(&UserRole::Host.to_string()));

    // The successful 200 OK and roles verification is a strong signal.
}

#[actix_web::test]
async fn test_update_admin_user_password_and_protection() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    // 1. Create admin user
    let email = format!("admin_update_{}@example.com", Uuid::now_v7());
    let create_body = json!({
        "email": email,
        "password": "initial_admin_password",
        "first_name": "Initial",
        "last_name": "Admin",
        "phone_number": "+18765550100",
        "is_active": true,
        "roles": ["admin"],
        "attributes": {"access_level": "super"}
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/users/")
        .set_json(&create_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let user: UserResponse = test::read_body_json(resp).await;
    let user_id = user.id;

    // 2. Update password and profile while keeping roles & active status identical (standard admin UI flow)
    let update_body = json!({
        "password": "new_super_secret_password_2026",
        "first_name": "Updated",
        "roles": ["admin"],
        "is_active": true,
        "is_verified": true
    });

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/users/user/{}", user_id))
        .set_json(&update_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let updated_user: UserResponse = test::read_body_json(resp).await;
    assert_eq!(updated_user.first_name, "Updated");
    assert!(updated_user.roles.contains(&UserRole::Admin.to_string()));

    // 3. Attempt to deactivate the admin user -> must return 403 Forbidden
    let deactivate_body = json!({
        "is_active": false
    });
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/users/user/{}", user_id))
        .set_json(&deactivate_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    // 4. Attempt to strip the admin role -> must return 403 Forbidden
    let strip_role_body = json!({
        "roles": ["booker"]
    });
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/users/user/{}", user_id))
        .set_json(&strip_role_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    // 5. Update admin password with omitted/null roles & active status (UI modal pattern)
    let pwd_only_body = json!({
        "password": "final_admin_password_2026"
    });
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/users/user/{}", user_id))
        .set_json(&pwd_only_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 6. Verify authentication with the newly updated password succeeds
    let login_body = json!({
        "email": email,
        "password": "final_admin_password_2026"
    });
    let req = test::TestRequest::post()
        .uri("/api/v1/users/login")
        .set_json(&login_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_create_user_booker_missing_profile() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    let email = format!("fail_booker_{}@example.com", Uuid::now_v7());
    let req_body = json!({
        "email": email,
        "password": "password123",
        "first_name": "Fail",
        "last_name": "Booker",
        "phone_number": "+1234567890",
        "is_active": true,
        "roles": ["booker"]
        // Missing booker_profile
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/users/")
        .set_json(&req_body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400); // Expect Bad Request
}

#[actix_web::test]
async fn test_create_user_host_missing_profile() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    let email = format!("fail_host_{}@example.com", Uuid::now_v7());
    let req_body = json!({
        "email": email,
        "password": "password123",
        "first_name": "Fail",
        "last_name": "Host",
        "phone_number": "+1234567890",
        "is_active": true,
        "roles": ["host"]
        // Missing host_profile
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/users/")
        .set_json(&req_body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400); // Expect Bad Request
}

#[actix_web::test]
async fn test_get_all_users_with_filters() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    // Create a few users for testing
    let users_data = vec![
        ("alice@example.com", "Alice", "Wonderland"),
        ("bob@example.com", "Bob", "Builder"),
        ("charlie@example.com", "Charlie", "Chocolate"),
    ];

    for (email, first, last) in &users_data {
        let req_body = json!({
            "email": email,
            "password": "password123",
            "first_name": first,
            "last_name": last,
            "phone_number": "+1234567890",
            "is_active": true,
            "roles": ["booker"], // Minimum required
            "booker_profile": {
                "loyalty": {"points": 0}
            }
        });
        let req = test::TestRequest::post()
            .uri("/api/v1/users/")
            .set_json(&req_body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    // 1. Test get all (pagination default) - test BOTH trailing slash and without trailing slash
    let req_with_slash = test::TestRequest::get().uri("/api/v1/users/").to_request();
    let resp_with_slash = test::call_service(&app, req_with_slash).await;
    assert_eq!(resp_with_slash.status(), 200);

    let req_without_slash = test::TestRequest::get().uri("/api/v1/users").to_request();
    let resp_without_slash = test::call_service(&app, req_without_slash).await;
    assert_eq!(resp_without_slash.status(), 200);

    let body: Vec<UserResponse> = test::read_body_json(resp_without_slash).await;
    assert!(body.len() >= 3);

    // Also verify POST without trailing slash works
    let post_without_slash = test::TestRequest::post()
        .uri("/api/v1/users")
        .set_json(json!({
            "email": "admin_test_listing@example.com",
            "password": "password123",
            "first_name": "Admin",
            "last_name": "User",
            "phone_number": "+1234567891",
            "is_active": true,
            "roles": ["admin"],
        }))
        .to_request();
    let resp_post = test::call_service(&app, post_without_slash).await;
    assert_eq!(resp_post.status(), 201);

    // Verify admin user is returned in GET /api/v1/users
    let req_admin_check = test::TestRequest::get()
        .uri("/api/v1/users?search=admin_test_listing")
        .to_request();
    let resp_admin_check = test::call_service(&app, req_admin_check).await;
    assert_eq!(resp_admin_check.status(), 200);
    let admin_body: Vec<UserResponse> = test::read_body_json(resp_admin_check).await;
    assert_eq!(admin_body.len(), 1);
    assert_eq!(admin_body[0].email, "admin_test_listing@example.com");
    assert!(
        admin_body[0].roles.contains(&"Admin".to_string())
            || admin_body[0].roles.contains(&"admin".to_string())
    );

    // 2. Test search filter (email)
    let req = test::TestRequest::get()
        .uri("/api/v1/users/?search=alice")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Vec<UserResponse> = test::read_body_json(resp).await;
    assert_eq!(body.len(), 1);
    assert_eq!(body[0].email, "alice@example.com");

    // 3. Test search filter (first name) - partial match
    let req = test::TestRequest::get()
        .uri("/api/v1/users/?search=Bo")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Vec<UserResponse> = test::read_body_json(resp).await;
    assert_eq!(body.len(), 1);
    assert_eq!(body[0].first_name, "Bob");

    // 4. Test search filter (last name) - partial match
    let req = test::TestRequest::get()
        .uri("/api/v1/users/?search=Chocol")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Vec<UserResponse> = test::read_body_json(resp).await;
    assert_eq!(body.len(), 1);
    assert_eq!(body[0].last_name, "Chocolate");
}

#[actix_web::test]
async fn test_resend_verification_success() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    let email = format!("resend_{}@example.com", Uuid::now_v7());
    let req_body = json!({
        "email": email,
        "password": "password123",
        "first_name": "Test",
        "last_name": "Resend",
        "is_active": true,
        "roles": ["booker"],
        "booker_profile": { "loyalty": {"points": 0} }
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/users/")
        .set_json(&req_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let initial_user: UserResponse = test::read_body_json(resp).await;
    let initial_code = initial_user.verification_code.unwrap();

    let resend_req_body = json!({ "email": email });
    let req2 = test::TestRequest::post()
        .uri("/api/v1/users/resend-verification")
        .set_json(&resend_req_body)
        .to_request();
    let resp2 = test::call_service(&app, req2).await;
    assert_eq!(resp2.status(), 200);

    let updated_user: UserResponse = test::read_body_json(resp2).await;
    let new_code = updated_user.verification_code.unwrap();
    assert_ne!(initial_code, new_code);
}

#[actix_web::test]
async fn test_resend_verification_nonexistent_user() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    let resend_req_body = json!({ "email": "doesnt_exist@example.com" });
    let req = test::TestRequest::post()
        .uri("/api/v1/users/resend-verification")
        .set_json(&resend_req_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_resend_verification_already_verified() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    let email = format!("verified_{}@example.com", Uuid::now_v7());
    let req_body = json!({
        "email": email,
        "password": "password123",
        "first_name": "Test",
        "last_name": "Verified",
        "is_active": true,
        "roles": ["booker"],
        "booker_profile": { "loyalty": {"points": 0} }
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/users/")
        .set_json(&req_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let initial_user: UserResponse = test::read_body_json(resp).await;

    // Verify the user
    let verify_req_body =
        json!({ "email": email, "code": initial_user.verification_code.unwrap() });
    let req2 = test::TestRequest::post()
        .uri("/api/v1/users/verify")
        .set_json(&verify_req_body)
        .to_request();
    let resp2 = test::call_service(&app, req2).await;
    assert_eq!(resp2.status(), 200);

    // Try resend
    let resend_req_body = json!({ "email": email });
    let req3 = test::TestRequest::post()
        .uri("/api/v1/users/resend-verification")
        .set_json(&resend_req_body)
        .to_request();
    let resp3 = test::call_service(&app, req3).await;
    assert_eq!(resp3.status(), 401);
}

#[actix_web::test]
async fn test_verify_user_lowercase_and_whitespace() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    let email = format!("case_test_{}@example.com", Uuid::now_v7());
    let req_body = json!({
        "email": email,
        "password": "password123",
        "first_name": "Case",
        "last_name": "Test",
        "is_active": true,
        "roles": ["booker"],
        "booker_profile": { "loyalty": {"points": 0} }
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/users/")
        .set_json(&req_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let initial_user: UserResponse = test::read_body_json(resp).await;

    // Verify user using lowercase and extra surrounding whitespace
    let raw_code = initial_user.verification_code.unwrap();
    let modified_code = format!("  {}  ", raw_code.to_lowercase());

    let verify_req_body = json!({ "email": email, "code": modified_code });
    let req2 = test::TestRequest::post()
        .uri("/api/v1/users/verify")
        .set_json(&verify_req_body)
        .to_request();
    let resp2 = test::call_service(&app, req2).await;
    assert_eq!(resp2.status(), 200);
}

#[actix_web::test]
async fn test_soft_restore_hard_delete_user() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    let email = format!("to_delete_{}@example.com", Uuid::now_v7());
    let req_body = json!({
        "email": email,
        "password": "password123",
        "first_name": "Delete",
        "last_name": "Me",
        "is_active": true,
        "roles": ["booker"],
        "booker_profile": { "loyalty": {"points": 0} }
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/users/")
        .set_json(&req_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let created_user: UserResponse = test::read_body_json(resp).await;

    // Soft delete user
    let req_soft = test::TestRequest::delete()
        .uri(&format!("/api/v1/users/user/{}", created_user.id))
        .to_request();
    let resp_soft = test::call_service(&app, req_soft).await;
    assert_eq!(resp_soft.status(), 200);
    let soft_deleted: UserResponse = test::read_body_json(resp_soft).await;
    assert!(soft_deleted.deleted_at.is_some());

    // Restore user
    let req_restore = test::TestRequest::post()
        .uri(&format!("/api/v1/users/user/{}/restore", created_user.id))
        .to_request();
    let resp_restore = test::call_service(&app, req_restore).await;
    assert_eq!(resp_restore.status(), 200);
    let restored: UserResponse = test::read_body_json(resp_restore).await;
    assert!(restored.deleted_at.is_none());

    // Hard delete user
    let req_hard = test::TestRequest::delete()
        .uri(&format!("/api/v1/users/user/{}/hard", created_user.id))
        .to_request();
    let resp_hard = test::call_service(&app, req_hard).await;
    assert_eq!(resp_hard.status(), 204);
}

mod currency_inactive_tests {
    //! A user's default currency must be active when set or changed.

    use super::*;

    async fn setup() -> (TestPg, sqlx::PgPool) {
        dotenvy::dotenv().ok();
        let db_url = env::var(DATABASE_URL_ENV).expect("DATABASE_URL must be set");
        let db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
        let pool = db.get_pool().await;
        sqlx::query("UPDATE currencies SET active = false WHERE code = 'CAD'")
            .execute(&pool)
            .await
            .expect("deactivate CAD");
        (db, pool)
    }

    fn registration(currency: &str) -> serde_json::Value {
        json!({
            "email": format!("cur_{}@example.com", Uuid::now_v7()),
            "password": "password123",
            "first_name": "Cur",
            "last_name": "Rency",
            "is_active": true,
            "roles": ["booker"],
            "booker_profile": { "loyalty": {} },
            "default_currency": currency
        })
    }

    #[actix_web::test]
    async fn registration_with_inactive_currency_is_422() {
        let (_db, pool) = setup().await;
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await;
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/users/")
                .set_json(registration("CAD"))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 422);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["code"], "CURRENCY_INACTIVE");
    }

    #[actix_web::test]
    async fn changing_default_to_inactive_is_422_but_unchanged_is_fine() {
        let (_db, pool) = setup().await;
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await;
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/users/")
                .set_json(registration("USD"))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 201);
        let user: UserResponse = test::read_body_json(resp).await;

        let change = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri(&format!("/api/v1/users/user/{}", user.id))
                .set_json(json!({ "default_currency": "CAD" }))
                .to_request(),
        )
        .await;
        assert_eq!(change.status(), 422);
        let body: serde_json::Value = test::read_body_json(change).await;
        assert_eq!(body["code"], "CURRENCY_INACTIVE");

        // A user whose currency was deactivated later can still save their
        // profile without changing it.
        sqlx::query("UPDATE currencies SET active = false WHERE code = 'USD'")
            .execute(&pool)
            .await
            .expect("deactivate USD");
        let unchanged = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri(&format!("/api/v1/users/user/{}", user.id))
                .set_json(json!({ "first_name": "Renamed", "default_currency": "USD" }))
                .to_request(),
        )
        .await;
        assert_eq!(unchanged.status(), 200);
    }
}

/// Spec 103 §1.2: user_api has no offset key, so the user's listings carry no
/// coordinates and no location setting at all.
#[actix_web::test]
async fn user_listings_drop_coordinates() {
    dotenvy::dotenv().ok();
    let db_url = match env::var(DATABASE_URL_ENV) {
        Ok(url) => url,
        Err(_) => panic!("DATABASE_URL must be set"),
    };
    let test_db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
    let pool = test_db.get_pool().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes),
    )
    .await;

    let email = format!("located_{}@example.com", Uuid::now_v7());
    let created: UserResponse = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/users/")
            .set_json(json!({
                "email": email,
                "password": "password123",
                "first_name": "Located",
                "last_name": "Host",
                "is_active": true,
                "roles": ["host"],
                "host_profile": {"verified_status": "verified", "description": "Host"}
            }))
            .to_request(),
    )
    .await;

    let listing = db_core::models::NewListing {
        name: format!("Hidden Villa {}", Uuid::now_v7()),
        user_id: created.id,
        description: None,
        listing_structure_id: 5,
        country: "Jamaica".to_string(),
        price_per_night: Some(rust_decimal_macros::dec!(300.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 4,
        bedrooms: 2,
        beds: 2,
        full_bathrooms: 2,
        half_bathrooms: 0,
        square_meters: None,
        latitude: Some(18.40741),
        longitude: Some(-77.10313),
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
        location_privacy: Some(common::location::LocationPrivacy::Exact),
    };
    if let Err(e) = db_core::listing::create_listing(&pool, &listing).await {
        panic!("create listing: {e}");
    }

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/v1/users/user/{email}/listings"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body = String::from_utf8_lossy(&test::read_body(resp).await).to_string();
    let listings: Vec<serde_json::Value> = match serde_json::from_str(&body) {
        Ok(listings) => listings,
        Err(e) => panic!("{e}: {body}"),
    };
    assert_eq!(listings.len(), 1, "{body}");
    assert_eq!(listings[0]["latitude"], serde_json::Value::Null);
    assert_eq!(listings[0]["longitude"], serde_json::Value::Null);
    assert!(listings[0].get("location_privacy").is_none(), "{body}");
    assert!(listings[0].get("location_precision").is_none(), "{body}");
    assert!(
        !body.contains("18.40741") && !body.contains("77.10313"),
        "{body}"
    );
    assert_eq!(listings[0]["city"], "Ocho Rios");
}

mod password_reset_tests {
    use super::*;

    fn test_db() -> TestPg {
        dotenvy::dotenv().ok();
        let db_url = match env::var(DATABASE_URL_ENV) {
            Ok(url) => url,
            Err(_) => "postgres://postgres:password@localhost:5432/our_places".to_string(),
        };
        TestPg::new(db_url, Path::new("../../db_core/migrations"))
    }

    async fn create_active_user(
        pool: &sqlx::PgPool,
        email: &str,
        verified: bool,
    ) -> Result<Uuid, Box<dyn std::error::Error>> {
        let id = Uuid::now_v7();
        let slug = format!("rst-{}", id.simple());
        sqlx::query(
            r#"
            INSERT INTO "user" (id, email, password_hash, first_name, last_name, is_active, is_verified, roles, default_currency, slug)
            VALUES ($1, $2, '!locked!initial', 'Crewed', 'Cleaner', TRUE, $3, ARRAY['staff'::user_role], 'USD', $4)
            "#,
        )
        .bind(id)
        .bind(email)
        .bind(verified)
        .bind(slug)
        .execute(pool)
        .await?;
        Ok(id)
    }

    fn extract_token_from_url(url: &str) -> Option<String> {
        url.split("?token=").nth(1).map(ToString::to_string)
    }

    #[actix_web::test]
    async fn password_reset_non_enumerating_and_single_use_flow()
    -> Result<(), Box<dyn std::error::Error>> {
        let db = test_db();
        let pool = db.get_pool().await;
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await;

        // 1. Unknown email still returns 202 Accepted and queues no email.
        let unknown_email = format!("nobody_{}@example.com", Uuid::now_v7().simple());
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/users/password-reset/request")
                .set_json(json!({ "email": unknown_email }))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 202);
        let outbox_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM email_outbox WHERE recipient_email = $1 AND template_id = 'password_reset_link'",
        )
        .bind(&unknown_email)
        .fetch_one(&pool)
        .await?;
        assert_eq!(outbox_count, 0);

        // 2. Soft-deleted user also returns 202 with no email queued.
        let deleted_email = format!("deleted_{}@example.com", Uuid::now_v7().simple());
        let deleted_id = create_active_user(&pool, &deleted_email, true).await?;
        sqlx::query(r#"UPDATE "user" SET deleted_at = NOW(), is_active = FALSE WHERE id = $1"#)
            .bind(deleted_id)
            .execute(&pool)
            .await?;
        let del_resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/users/password-reset/request")
                .set_json(json!({ "email": deleted_email }))
                .to_request(),
        )
        .await;
        assert_eq!(del_resp.status(), 202);
        let del_outbox: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM email_outbox WHERE recipient_email = $1 AND template_id = 'password_reset_link'",
        )
        .bind(&deleted_email)
        .fetch_one(&pool)
        .await?;
        assert_eq!(del_outbox, 0);

        // 3. Active staff user receives reset link (case/whitespace normalized) and prior token is invalidated on re-request.
        let canonical_email = format!("staff_{}@example.com", Uuid::now_v7().simple());
        let staff_id = create_active_user(&pool, &canonical_email, false).await?;

        // Seed an active session to confirm password reset revokes it.
        db_core::sessions::SessionsDb::new(pool.clone())
            .save_session(
                &format!("sess_{}", Uuid::now_v7().simple()),
                Some(staff_id),
                Some(&canonical_email),
                "admin",
                b"{}",
                3600,
            )
            .await?;

        let first_req = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/users/password-reset/request")
                .set_json(json!({ "email": format!("  {}  ", canonical_email.to_uppercase()) }))
                .to_request(),
        )
        .await;
        assert_eq!(first_req.status(), 202);

        let second_req = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/users/password-reset/request")
                .set_json(json!({ "email": canonical_email }))
                .to_request(),
        )
        .await;
        assert_eq!(second_req.status(), 202);

        // Throttled: immediate duplicate request does not flood outbox or invalidate first token.
        let throttled_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM email_outbox WHERE recipient_email = $1 AND template_id = 'password_reset_link'",
        )
        .bind(&canonical_email)
        .fetch_one(&pool)
        .await?;
        assert_eq!(throttled_count, 1);

        // Once the cooldown window has elapsed, a new request supersedes the prior token.
        sqlx::query(
            "UPDATE account_action_token SET created_at = NOW() - INTERVAL '2 minutes' WHERE user_id = $1 AND purpose = 'password_reset'",
        )
        .bind(staff_id)
        .execute(&pool)
        .await?;

        let third_req = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/users/password-reset/request")
                .set_json(json!({ "email": canonical_email }))
                .to_request(),
        )
        .await;
        assert_eq!(third_req.status(), 202);

        let open_tokens: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM account_action_token WHERE user_id = $1 AND purpose = 'password_reset' AND used_at IS NULL",
        )
        .bind(staff_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(open_tokens, 1);

        let payloads: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT payload FROM email_outbox WHERE recipient_email = $1 AND template_id = 'password_reset_link' ORDER BY created_at ASC",
        )
        .bind(&canonical_email)
        .fetch_all(&pool)
        .await?;
        assert_eq!(payloads.len(), 2);

        let stale_url = payloads[0]["reset_url"]
            .as_str()
            .ok_or("missing stale reset_url")?;
        let fresh_url = payloads[1]["reset_url"]
            .as_str()
            .ok_or("missing fresh reset_url")?;
        assert!(fresh_url.contains("/reset-password?token="), "{fresh_url}");
        let stale_token = extract_token_from_url(stale_url).ok_or("missing stale token")?;
        let fresh_token = extract_token_from_url(fresh_url).ok_or("missing fresh token")?;

        // Stale token rejected.
        let stale_confirm = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/users/password-reset/confirm")
                .set_json(json!({ "token": stale_token, "new_password": "NewPassword123!" }))
                .to_request(),
        )
        .await;
        assert!(matches!(stale_confirm.status().as_u16(), 400 | 401));

        // Short password rejected with 400.
        let short_confirm = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/users/password-reset/confirm")
                .set_json(json!({ "token": fresh_token, "new_password": "short" }))
                .to_request(),
        )
        .await;
        assert_eq!(short_confirm.status(), 400);

        // Fresh token succeeds, verifies user, revokes sessions, and rejects replay.
        let ok_confirm = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/users/password-reset/confirm")
                .set_json(json!({ "token": fresh_token, "new_password": "NewPassword123!" }))
                .to_request(),
        )
        .await;
        assert_eq!(ok_confirm.status(), 200);

        let is_verified: bool =
            sqlx::query_scalar(r#"SELECT is_verified FROM "user" WHERE id = $1"#)
                .bind(staff_id)
                .fetch_one(&pool)
                .await?;
        assert!(is_verified);

        let remaining_sessions: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE user_id = $1")
                .bind(staff_id)
                .fetch_one(&pool)
                .await?;
        assert_eq!(remaining_sessions, 0);

        let replay = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/users/password-reset/confirm")
                .set_json(json!({ "token": fresh_token, "new_password": "AnotherPassword123!" }))
                .to_request(),
        )
        .await;
        assert!(matches!(replay.status().as_u16(), 400 | 401));

        // Login succeeds with the new password.
        let login_resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/users/login")
                .set_json(json!({ "email": canonical_email, "password": "NewPassword123!" }))
                .to_request(),
        )
        .await;
        assert_eq!(login_resp.status(), 200);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn password_reset_concurrent_redemption_allows_only_one_winner()
    -> Result<(), Box<dyn std::error::Error>> {
        let db = test_db();
        let pool = db.get_pool().await;
        let email = format!("race_{}@example.com", Uuid::now_v7().simple());
        let user_id = create_active_user(&pool, &email, false).await?;

        let app_a = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await;
        let app_b = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await;

        let req_resp = test::call_service(
            &app_a,
            test::TestRequest::post()
                .uri("/api/v1/users/password-reset/request")
                .set_json(json!({ "email": email }))
                .to_request(),
        )
        .await;
        assert_eq!(req_resp.status(), 202);

        let payload: serde_json::Value = sqlx::query_scalar(
            "SELECT payload FROM email_outbox WHERE recipient_email = $1 AND template_id = 'password_reset_link' ORDER BY created_at DESC LIMIT 1",
        )
        .bind(&email)
        .fetch_one(&pool)
        .await?;
        let reset_url = payload["reset_url"].as_str().ok_or("missing reset_url")?;
        let token = extract_token_from_url(reset_url).ok_or("missing token")?;

        let t1 = token.clone();
        let t2 = token;
        let (r1, r2) = tokio::join!(
            test::call_service(
                &app_a,
                test::TestRequest::post()
                    .uri("/api/v1/users/password-reset/confirm")
                    .set_json(json!({ "token": t1, "new_password": "WinnerPassword1!" }))
                    .to_request(),
            ),
            test::call_service(
                &app_b,
                test::TestRequest::post()
                    .uri("/api/v1/users/password-reset/confirm")
                    .set_json(json!({ "token": t2, "new_password": "LoserPassword2!" }))
                    .to_request(),
            ),
        );

        let mut statuses = [r1.status().as_u16(), r2.status().as_u16()];
        statuses.sort_unstable();
        assert_eq!(statuses[0], 200, "statuses: {statuses:?}");
        assert!(matches!(statuses[1], 400 | 401), "statuses: {statuses:?}");

        let used_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM account_action_token WHERE user_id = $1 AND used_at IS NOT NULL",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(used_count, 1);
        Ok(())
    }
}

mod staff_api_tests {
    use super::{configure_routes, get_test_settings};
    use actix_web::{App, test, web};
    use common::turnover::{StaffListResponse, StaffMemberSummary};
    use serde_json::json;
    use sqlx::PgPool;
    use sqlx_db_tester::TestPg;
    use std::env;
    use std::path::Path;
    use uuid::Uuid;

    static DATABASE_URL_ENV: &str = "DATABASE_URL";

    fn test_db() -> TestPg {
        dotenvy::dotenv().ok();
        let db_url = match env::var(DATABASE_URL_ENV) {
            Ok(url) => url,
            Err(_) => "postgres://postgres:password@localhost:5432/our_places".to_string(),
        };
        TestPg::new(db_url, Path::new("../../db_core/migrations"))
    }

    async fn create_user_with_roles(
        pool: &PgPool,
        email: &str,
        roles: &[&str],
    ) -> Result<Uuid, Box<dyn std::error::Error>> {
        let id = Uuid::now_v7();
        sqlx::query(
            r#"INSERT INTO "user" (
                id, email, password_hash, first_name, last_name,
                is_active, is_verified, attributes, roles, default_currency
            ) VALUES ($1, $2, 'hash', 'Test', 'Actor', TRUE, TRUE, '{}'::jsonb, $3::user_role[], 'USD')"#,
        )
        .bind(id)
        .bind(email)
        .bind(roles)
        .execute(pool)
        .await?;
        Ok(id)
    }

    async fn create_listing(
        pool: &PgPool,
        owner_id: Uuid,
        name: &str,
        deleted: bool,
    ) -> Result<Uuid, Box<dyn std::error::Error>> {
        let id = Uuid::now_v7();
        let slug = format!("listing-{}", id.simple());
        sqlx::query(
            r#"INSERT INTO listing (
                id, user_id, name, listing_structure_id, country, slug,
                base_currency, added_at, deleted_at
            ) VALUES ($1, $2, $3, 1, 'Jamaica', $4, 'USD', NOW(), CASE WHEN $5 THEN NOW() ELSE NULL END)"#,
        )
        .bind(id)
        .bind(owner_id)
        .bind(name)
        .bind(slug)
        .bind(deleted)
        .execute(pool)
        .await?;
        Ok(id)
    }

    fn auth_header(user_id: Uuid) -> (&'static str, String) {
        (
            "Authorization",
            format!("Bearer {}", api_core::auth::generate_test_jwt(user_id)),
        )
    }

    fn extract_token_from_url(url: &str) -> Option<String> {
        url.split("token=").nth(1).map(|s| s.to_string())
    }

    #[tokio::test]
    async fn staff_endpoints_reject_unauthenticated_and_booker_or_staff_callers()
    -> Result<(), Box<dyn std::error::Error>> {
        let db = test_db();
        let pool = db.get_pool().await;
        let booker = create_user_with_roles(&pool, "booker-staff@example.com", &["booker"]).await?;
        let staff = create_user_with_roles(&pool, "staff-caller@example.com", &["staff"]).await?;
        let host = create_user_with_roles(&pool, "host-owner@example.com", &["host"]).await?;
        let host_listing = create_listing(&pool, host, "Blue Lagoon", false).await?;

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await;

        let unauth = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/staff-users")
                .to_request(),
        )
        .await;
        assert_eq!(unauth.status(), 401);

        for (label, caller_id) in [("booker", booker), ("staff", staff)] {
            let list_resp = test::call_service(
                &app,
                test::TestRequest::get()
                    .uri("/api/v1/staff-users")
                    .insert_header(auth_header(caller_id))
                    .to_request(),
            )
            .await;
            assert_eq!(list_resp.status(), 403, "{label} GET /staff-users");

            let invite_resp = test::call_service(
                &app,
                test::TestRequest::post()
                    .uri("/api/v1/staff-users")
                    .insert_header(auth_header(caller_id))
                    .set_json(json!({
                        "email": "newcrew@example.com",
                        "listing_ids": [host_listing]
                    }))
                    .to_request(),
            )
            .await;
            assert_eq!(invite_resp.status(), 403, "{label} POST /staff-users");
        }

        // Empty listing_ids and soft-deleted or unowned listings are rejected for Host.
        let empty_resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/staff-users")
                .insert_header(auth_header(host))
                .set_json(json!({
                    "email": "newcrew@example.com",
                    "listing_ids": []
                }))
                .to_request(),
        )
        .await;
        assert_eq!(empty_resp.status(), 400);

        let deleted_listing = create_listing(&pool, host, "Deleted Villa", true).await?;
        let deleted_resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/staff-users")
                .insert_header(auth_header(host))
                .set_json(json!({
                    "email": "newcrew@example.com",
                    "listing_ids": [deleted_listing]
                }))
                .to_request(),
        )
        .await;
        assert_eq!(deleted_resp.status(), 400);

        let other_host = create_user_with_roles(&pool, "other-host@example.com", &["host"]).await?;
        let unowned_listing = create_listing(&pool, other_host, "Other Villa", false).await?;
        let unowned_resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/staff-users")
                .insert_header(auth_header(host))
                .set_json(json!({
                    "email": "newcrew@example.com",
                    "listing_ids": [unowned_listing]
                }))
                .to_request(),
        )
        .await;
        assert_eq!(unowned_resp.status(), 403);
        Ok(())
    }

    #[tokio::test]
    async fn host_scoping_search_escaping_multi_host_sharing_and_safe_unlink()
    -> Result<(), Box<dyn std::error::Error>> {
        let db = test_db();
        let pool = db.get_pool().await;
        let host_a = create_user_with_roles(&pool, "hosta@example.com", &["host"]).await?;
        let host_b = create_user_with_roles(&pool, "hostb@example.com", &["host"]).await?;
        let villa_a = create_listing(&pool, host_a, "Villa Alpha", false).await?;
        let villa_b1 = create_listing(&pool, host_b, "Villa Beta 1", false).await?;
        let villa_b2 = create_listing(&pool, host_b, "Villa Beta 2", false).await?;

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await;

        // Host A invites cleaner with whitespace + uppercase email.
        let invite_a = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/staff-users")
                .insert_header(auth_header(host_a))
                .set_json(json!({
                    "email": "  Shared.Cleaner@Example.com  ",
                    "first_name": "Dana",
                    "last_name": "Clean",
                    "listing_ids": [villa_a]
                }))
                .to_request(),
        )
        .await;
        assert_eq!(invite_a.status(), 202);
        let invite_a_body: common::turnover::StaffInviteResponse =
            test::read_body_json(invite_a).await;
        assert_eq!(invite_a_body.email, "shared.cleaner@example.com");
        assert_eq!(invite_a_body.message, "Invitation sent.");

        let list_a_resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/staff-users")
                .insert_header(auth_header(host_a))
                .to_request(),
        )
        .await;
        assert_eq!(list_a_resp.status(), 200);
        let list_a: StaffListResponse = test::read_body_json(list_a_resp).await;
        assert_eq!(list_a.staff.len(), 1);
        let created_a = &list_a.staff[0];
        assert_eq!(created_a.email, "shared.cleaner@example.com");
        assert!(created_a.password_pending);
        assert_eq!(created_a.listings.len(), 1);
        assert_eq!(created_a.listings[0].listing_id, villa_a);
        let staff_id = created_a.user_id;

        // Seed an admin session for the cleaner to verify final unlink revokes it.
        db_core::sessions::SessionsDb::new(pool.clone())
            .save_session(
                "staff-admin-sess-hash",
                Some(staff_id),
                Some("shared.cleaner@example.com"),
                "admin",
                b"{}",
                3600,
            )
            .await?;

        // Host B invites the same cleaner email for Host B's first listing; response is uniform
        // and does not attach until the cleaner clicks the accept link.
        let invite_b = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/staff-users")
                .insert_header(auth_header(host_b))
                .set_json(json!({
                    "email": "shared.cleaner@example.com",
                    "first_name": "OverwriteAttempt",
                    "last_name": "Ignored",
                    "listing_ids": [villa_b1]
                }))
                .to_request(),
        )
        .await;
        assert_eq!(invite_b.status(), 202);
        let invite_b_body: common::turnover::StaffInviteResponse =
            test::read_body_json(invite_b).await;
        assert_eq!(invite_b_body, invite_a_body);

        let accept_payload: serde_json::Value = sqlx::query_scalar(
            "SELECT payload FROM email_outbox WHERE recipient_email = 'shared.cleaner@example.com' AND template_id = 'staff_accept_invite' ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_one(&pool)
        .await?;
        let accept_token =
            extract_token_from_url(accept_payload["accept_url"].as_str().ok_or("accept_url")?)
                .ok_or("accept_token")?;
        let accept_resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/users/staff-invite/accept")
                .set_json(json!({ "token": accept_token }))
                .to_request(),
        )
        .await;
        assert_eq!(accept_resp.status(), 200);

        let list_b_resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/staff-users")
                .insert_header(auth_header(host_b))
                .to_request(),
        )
        .await;
        assert_eq!(list_b_resp.status(), 200);
        let list_b: StaffListResponse = test::read_body_json(list_b_resp).await;
        assert_eq!(list_b.staff.len(), 1);
        assert_eq!(list_b.staff[0].user_id, staff_id);
        assert_eq!(list_b.staff[0].first_name, "Dana");
        assert_eq!(list_b.staff[0].listings.len(), 1);
        assert_eq!(list_b.staff[0].listings[0].listing_id, villa_b1);

        // Search wildcard escaping (`q=%` must not match `Dana Clean`).
        let wildcard_resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/staff-users?q=%25")
                .insert_header(auth_header(host_b))
                .to_request(),
        )
        .await;
        assert_eq!(wildcard_resp.status(), 200);
        let wildcard_list: StaffListResponse = test::read_body_json(wildcard_resp).await;
        assert_eq!(wildcard_list.total, 0);

        // Redeem invite token so password_pending becomes false, then Host B updates listings -> queues listing_assigned email.
        let setup_payload: serde_json::Value = sqlx::query_scalar(
            "SELECT payload FROM email_outbox WHERE recipient_email = 'shared.cleaner@example.com' AND template_id = 'staff_invite' ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_one(&pool)
        .await?;
        let token = extract_token_from_url(setup_payload["setup_url"].as_str().ok_or("setup_url")?)
            .ok_or("token")?;
        let confirm_resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/users/password-reset/confirm")
                .set_json(json!({ "token": token, "new_password": "CleanerPassword1!" }))
                .to_request(),
        )
        .await;
        assert_eq!(confirm_resp.status(), 200);

        let update_b = test::call_service(
            &app,
            test::TestRequest::put()
                .uri(&format!("/api/v1/staff-users/{staff_id}/listings"))
                .insert_header(auth_header(host_b))
                .set_json(json!({ "listing_ids": [villa_b1, villa_b2] }))
                .to_request(),
        )
        .await;
        assert_eq!(update_b.status(), 200);
        let updated_b: StaffMemberSummary = test::read_body_json(update_b).await;
        assert!(!updated_b.password_pending);
        assert_eq!(updated_b.listings.len(), 2);

        let assigned_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM email_outbox WHERE recipient_email = 'shared.cleaner@example.com' AND template_id = 'staff_listing_assigned'",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(assigned_count, 1);

        // Re-seed admin session after password-reset revoked sessions, then Host B unlinks cleaner.
        db_core::sessions::SessionsDb::new(pool.clone())
            .save_session(
                "staff-admin-sess-hash-2",
                Some(staff_id),
                Some("shared.cleaner@example.com"),
                "admin",
                b"{}",
                3600,
            )
            .await?;

        let del_b = test::call_service(
            &app,
            test::TestRequest::delete()
                .uri(&format!("/api/v1/staff-users/{staff_id}"))
                .insert_header(auth_header(host_b))
                .to_request(),
        )
        .await;
        assert_eq!(del_b.status(), 200);

        // Host A still sees cleaner and Villa Alpha; 'staff' role and session stay alive.
        let list_a = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/staff-users")
                .insert_header(auth_header(host_a))
                .to_request(),
        )
        .await;
        let parsed_a: StaffListResponse = test::read_body_json(list_a).await;
        assert_eq!(parsed_a.total, 1);
        assert_eq!(parsed_a.staff[0].listings.len(), 1);
        assert_eq!(parsed_a.staff[0].listings[0].listing_id, villa_a);

        // Final unlink by Host A removes 'staff' role and revokes admin sessions.
        let del_a = test::call_service(
            &app,
            test::TestRequest::delete()
                .uri(&format!("/api/v1/staff-users/{staff_id}"))
                .insert_header(auth_header(host_a))
                .to_request(),
        )
        .await;
        assert_eq!(del_a.status(), 200);

        let roles: Vec<String> =
            sqlx::query_scalar(r#"SELECT roles::text[] FROM "user" WHERE id = $1"#)
                .bind(staff_id)
                .fetch_one(&pool)
                .await?;
        assert!(!roles.contains(&"staff".to_string()), "{roles:?}");

        let remaining_sessions: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sessions WHERE user_id = $1 AND namespace = 'admin'",
        )
        .bind(staff_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(remaining_sessions, 0);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_invites_of_same_new_email_converge_without_500()
    -> Result<(), Box<dyn std::error::Error>> {
        let db = test_db();
        let pool = db.get_pool().await;
        let host_a = create_user_with_roles(&pool, "race-ha@example.com", &["host"]).await?;
        let host_b = create_user_with_roles(&pool, "race-hb@example.com", &["host"]).await?;
        let villa_a = create_listing(&pool, host_a, "Race Villa A", false).await?;
        let villa_b = create_listing(&pool, host_b, "Race Villa B", false).await?;

        let app_a = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await;
        let app_b = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await;

        let target_email = format!("race.crew.{}@example.com", Uuid::now_v7().simple());
        let (r1, r2) = tokio::join!(
            test::call_service(
                &app_a,
                test::TestRequest::post()
                    .uri("/api/v1/staff-users")
                    .insert_header(auth_header(host_a))
                    .set_json(json!({
                        "email": target_email,
                        "first_name": "Sam",
                        "last_name": "Crew",
                        "listing_ids": [villa_a]
                    }))
                    .to_request(),
            ),
            test::call_service(
                &app_b,
                test::TestRequest::post()
                    .uri("/api/v1/staff-users")
                    .insert_header(auth_header(host_b))
                    .set_json(json!({
                        "email": format!("  {}  ", target_email.to_uppercase()),
                        "first_name": "Sam",
                        "last_name": "Crew",
                        "listing_ids": [villa_b]
                    }))
                    .to_request(),
            ),
        );

        assert_eq!(r1.status(), 202);
        assert_eq!(r2.status(), 202);

        let user_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "user" WHERE LOWER(email) = $1"#)
                .bind(&target_email)
                .fetch_one(&pool)
                .await?;
        assert_eq!(user_count, 1);
        Ok(())
    }

    #[tokio::test]
    async fn patch_user_preserves_staff_role_when_editing_via_admin_users()
    -> Result<(), Box<dyn std::error::Error>> {
        let db = test_db();
        let pool = db.get_pool().await;
        let staff_only =
            create_user_with_roles(&pool, "staff.only@example.com", &["staff"]).await?;
        let host_staff =
            create_user_with_roles(&pool, "host.staff@example.com", &["host", "staff"]).await?;
        let booker_staff =
            create_user_with_roles(&pool, "booker.staff@example.com", &["booker", "staff"]).await?;

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await;

        for (uid, sent_roles, expected_extra) in [
            (staff_only, Vec::<&str>::new(), None),
            (host_staff, vec!["host"], Some("host")),
            (booker_staff, vec!["booker"], Some("booker")),
        ] {
            let resp = test::call_service(
                &app,
                test::TestRequest::patch()
                    .uri(&format!("/api/v1/users/user/{uid}"))
                    .set_json(json!({
                        "first_name": "UpdatedName",
                        "roles": sent_roles
                    }))
                    .to_request(),
            )
            .await;
            assert_eq!(resp.status(), 200, "patch user {uid} must succeed");
            let updated: serde_json::Value = test::read_body_json(resp).await;
            let roles = updated["roles"].as_array().ok_or("roles array")?;
            assert!(
                roles
                    .iter()
                    .any(|r| r.as_str().is_some_and(|s| s.eq_ignore_ascii_case("staff"))),
                "staff role must be retained: {roles:?}"
            );
            if let Some(extra) = expected_extra {
                assert!(
                    roles
                        .iter()
                        .any(|r| r.as_str().is_some_and(|s| s.eq_ignore_ascii_case(extra))),
                    "expected {extra} in {roles:?}"
                );
            }
        }
        Ok(())
    }
}
