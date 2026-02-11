use super::*;
use actix_web::{App, test, web};
use api_core::settings::{Application, DatabaseSettings, Env, FeatureFlags, Log, Server, Settings};
use db_core::models::UserRole;
use serde_json::json;
use sqlx_db_tester::TestPg;
use std::env;
use std::path::Path;
use uuid::Uuid;

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
        application: Application { max_attempts: 1 },
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
    assert!(body.roles.contains(&UserRole::Booker));
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
    assert!(body.roles.contains(&UserRole::Host));
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
    assert!(body.roles.contains(&UserRole::Admin));
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
    assert!(body.roles.contains(&UserRole::Booker));
    assert!(body.roles.contains(&UserRole::Host));
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
    assert!(updated_user.roles.contains(&UserRole::Booker));
    assert!(updated_user.roles.contains(&UserRole::Host));

    // The successful 200 OK and roles verification is a strong signal.
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

    // 1. Test get all (pagination default)
    let req = test::TestRequest::get()
        .uri("/api/v1/users/") // Use trailing slash or not? Route is "/" in scope "/api/v1/users"
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Vec<UserResponse> = test::read_body_json(resp).await;
    assert!(body.len() >= 3);

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
