//! Exchange-rate behaviour at the booking API boundary (#100).
//!
//! A shared fixture spins up an isolated database (TestPg), a host, a guest,
//! a USD listing and the booking routes.

use actix_web::dev::{Service, ServiceResponse};
use actix_web::{App, test, web};
use rust_decimal_macros::dec;
use sqlx::PgPool;
use sqlx_db_tester::TestPg;
use std::path::Path;
use uuid::Uuid;

/// Everything a rate test needs; `_db` keeps the database alive.
pub(super) struct Fixture {
    pub _db: TestPg,
    pub pool: PgPool,
    pub guest_id: Uuid,
    pub listing_id: Uuid,
}

fn new_user(id: Uuid, role: db_core::models::UserRole) -> db_core::models::NewUser {
    db_core::models::NewUser {
        id,
        email: format!("u_{id}@example.com"),
        password_hash: "hash".to_string(),
        first_name: "Test".to_string(),
        last_name: "User".to_string(),
        phone_number: None,
        is_active: true,
        is_verified: true,
        verification_code: None,
        verification_code_expires_at: None,
        attributes: serde_json::json!({}),
        roles: Some(vec![role]),
        default_currency: "USD".to_string(),
    }
}

pub(super) async fn fixture() -> Fixture {
    dotenvy::dotenv().ok();
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".to_string());
    let db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
    let pool = db.get_pool().await;

    let host_id = Uuid::now_v7();
    db_core::user::create_user(&pool, &new_user(host_id, db_core::models::UserRole::Host))
        .await
        .expect("create host");
    db_core::user::create_host_profile(
        &pool,
        host_id,
        &db_core::models::NewHostProfile {
            description: None,
            verified_status: Some("pending".to_string()),
            payout_details: None,
        },
    )
    .await
    .expect("create host profile");
    let guest_id = Uuid::now_v7();
    db_core::user::create_user(
        &pool,
        &new_user(guest_id, db_core::models::UserRole::Booker),
    )
    .await
    .expect("create guest");
    let listing = db_core::listing::create_listing(
        &pool,
        &db_core::models::NewListing {
            name: format!("Villa {}", Uuid::now_v7()),
            user_id: host_id,
            description: None,
            listing_structure_id: 1,
            country: "Jamaica".to_string(),
            price_per_night: Some(dec!(200.00)),
            weekly_discount_percentage: None,
            monthly_discount_percentage: None,
            max_guests: 4,
            bedrooms: 2,
            beds: 2,
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
            commission_pct: Some(dec!(0.1000)),
            cleaning_fee: Some(dec!(50.00)),
            damage_deposit: None,
            guests_included: None,
            extra_guest_fee: None,
        },
    )
    .await
    .expect("create listing");

    Fixture {
        _db: db,
        pool,
        guest_id,
        listing_id: listing.id,
    }
}

/// The booking API routes over `pool`.
pub(super) async fn app(
    pool: &PgPool,
) -> impl Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error> {
    let settings = api_core::settings::Settings {
        server: api_core::settings::Server {
            host: "127.0.0.1".to_string(),
            port: 0,
        },
        database: api_core::settings::DatabaseSettings {
            username: String::new(),
            password: String::new(),
            port: 0,
            host: String::new(),
            database_name: String::new(),
            cloud: false,
            instance_name: String::new(),
            database_url: None,
        },
        log: api_core::settings::Log {
            level: "info".to_string(),
        },
        env: api_core::settings::Env::Testing,
        application: api_core::settings::Application {
            max_attempts: 3,
            pending_hold_minutes: api_core::settings::DEFAULT_PENDING_HOLD_MINUTES,
        },
        feature_flags: api_core::settings::FeatureFlags {
            enable_hard_deletes: false,
        },
    };
    test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(settings))
            .app_data(web::Data::new(crate::settings::TaxSettings {
                taj_supplier_trn: "000-111-222".to_string(),
                supplier_name: "Our Places Jamaica Limited".to_string(),
                supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
            }))
            .configure(crate::apis::configure_routes),
    )
    .await
}

/// `POST /api/v1/bookings` as the fixture guest.
pub(super) fn hold_request(f: &Fixture, currency: &str) -> actix_http::Request {
    test::TestRequest::post()
        .uri("/api/v1/bookings")
        .insert_header(("Accept", "application/json"))
        .insert_header((
            "Authorization",
            format!("Bearer {}", api_core::auth::generate_test_jwt(f.guest_id)),
        ))
        .set_json(serde_json::json!({
            "guest_id": f.guest_id,
            "listing_id": f.listing_id,
            "check_in": "2030-07-01",
            "check_out": "2030-07-03",
            "num_adults": 2,
            "num_children": 0,
            "num_infants": 0,
            "num_pets": 0,
            "is_business_trip": false,
            "currency": currency,
            "agreed_cancellation_policy": "flexible",
        }))
        .to_request()
}

mod hold_rate_use_tests {
    //! Hold creation resolves its rate as a new quote.

    use super::*;

    #[tokio::test]
    async fn hold_on_retired_pair_is_fx_rate_unavailable() {
        let f = fixture().await;
        // The seeded USD→JMD and JMD→USD pairs are both retired: no rate may
        // be quoted, directly or derived.
        sqlx::query(
            "UPDATE currency_exchange_rates SET retired_at = now()
             WHERE (base_currency, target_currency) IN (('USD','JMD'), ('JMD','USD'))",
        )
        .execute(&f.pool)
        .await
        .expect("retire USD/JMD");
        let app = app(&f.pool).await;

        let resp = test::call_service(&app, hold_request(&f, "JMD")).await;
        assert_eq!(resp.status(), 422);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["code"], "FX_RATE_UNAVAILABLE");
    }

    #[tokio::test]
    async fn hold_in_inactive_currency_is_currency_inactive() {
        let f = fixture().await;
        sqlx::query("UPDATE currencies SET active = false WHERE code = 'JMD'")
            .execute(&f.pool)
            .await
            .expect("deactivate JMD");
        let app = app(&f.pool).await;

        let resp = test::call_service(&app, hold_request(&f, "JMD")).await;
        assert_eq!(resp.status(), 422);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["code"], "CURRENCY_INACTIVE");
    }
}
