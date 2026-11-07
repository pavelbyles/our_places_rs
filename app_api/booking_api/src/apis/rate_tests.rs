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

/// Env var holding the test database URL.
const DATABASE_URL_ENV: &str = "DATABASE_URL";

/// Everything a rate test needs; `_db` keeps the database alive.
pub(super) struct Fixture {
    pub _db: TestPg,
    pub pool: PgPool,
    pub host_id: Uuid,
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

/// An admin user in the fixture database.
pub(super) async fn create_admin(pool: &PgPool) -> Uuid {
    let id = Uuid::now_v7();
    db_core::user::create_user(pool, &new_user(id, db_core::models::UserRole::Admin))
        .await
        .expect("create admin");
    id
}

pub(super) async fn fixture() -> Fixture {
    dotenvy::dotenv().ok();
    let db_url = std::env::var(DATABASE_URL_ENV)
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
            is_active: true,
        },
    )
    .await
    .expect("create listing");

    Fixture {
        _db: db,
        pool,
        host_id,
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
            rate_effective_grace_seconds: api_core::settings::DEFAULT_RATE_EFFECTIVE_GRACE_SECONDS,
            max_stay_nights: api_core::settings::DEFAULT_MAX_STAY_NIGHTS,
            request_response_hours: api_core::settings::DEFAULT_REQUEST_RESPONSE_HOURS,
            quote_valid_hours: api_core::settings::DEFAULT_QUOTE_VALID_HOURS,
            request_near_expiry_pct: api_core::settings::DEFAULT_REQUEST_NEAR_EXPIRY_PCT,
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

mod rate_lock_tests {
    //! The base→payment rate is locked at hold creation; payout uses the rate
    //! at confirmation and tax documents the JMD rate at issue time.

    use super::*;
    use rust_decimal::Decimal;

    async fn publish_now(pool: &PgPool, base: &str, target: &str, rate: Decimal) {
        sqlx::query(
            "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
             VALUES ($1, $2, $3, now())",
        )
        .bind(base)
        .bind(target)
        .bind(rate)
        .execute(pool)
        .await
        .expect("publish rate");
    }

    async fn hold<S>(app: &S, f: &Fixture, currency: &str) -> Uuid
    where
        S: Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error>,
    {
        let resp = test::call_service(app, hold_request(f, currency)).await;
        assert_eq!(resp.status(), 201, "hold created");
        let body: serde_json::Value = test::read_body_json(resp).await;
        body["id"]
            .as_str()
            .and_then(|id| Uuid::parse_str(id).ok())
            .expect("booking id")
    }

    async fn confirm<S>(app: &S, admin: Uuid, booking: Uuid)
    where
        S: Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error>,
    {
        let resp = test::call_service(
            app,
            test::TestRequest::patch()
                .uri(&format!("/api/v1/bookings/{booking}"))
                .insert_header(("Accept", "application/json"))
                .insert_header((
                    "Authorization",
                    format!("Bearer {}", api_core::auth::generate_test_jwt(admin)),
                ))
                .set_json(serde_json::json!({ "status": "confirmed" }))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200, "confirmation succeeds");
    }

    async fn scalar(pool: &PgPool, sql: &str, booking: Uuid) -> Decimal {
        sqlx::query_scalar(sql)
            .bind(booking)
            .fetch_one(pool)
            .await
            .expect("scalar query")
    }

    #[tokio::test]
    async fn booking_keeps_hold_rate_after_new_rate_takes_effect() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;

        let booking = hold(&app, &f, "JMD").await; // seeded USD→JMD 160
        publish_now(&f.pool, "USD", "JMD", rust_decimal_macros::dec!(170)).await;
        confirm(&app, admin, booking).await;

        let locked = scalar(
            &f.pool,
            "SELECT exchange_rate FROM booking WHERE id = $1",
            booking,
        )
        .await;
        assert_eq!(locked, rust_decimal_macros::dec!(160));
        let nightly = scalar(
            &f.pool,
            "SELECT daily_rate FROM booking WHERE id = $1",
            booking,
        )
        .await;
        assert_eq!(
            nightly,
            rust_decimal_macros::dec!(32000.00),
            "200 USD × 160"
        );
    }

    #[tokio::test]
    async fn payout_uses_host_rate_at_confirmation() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;

        let booking = hold(&app, &f, "JMD").await;
        // Host is paid in USD; an explicit JMD→USD rate takes effect after the hold.
        publish_now(&f.pool, "JMD", "USD", rust_decimal_macros::dec!(0.0062)).await;
        confirm(&app, admin, booking).await;

        let payout_rate = scalar(
            &f.pool,
            "SELECT exchange_rate FROM host_payout_ledger WHERE booking_id = $1",
            booking,
        )
        .await;
        assert_eq!(payout_rate, rust_decimal_macros::dec!(0.0062));
        let host_currency: String =
            sqlx::query_scalar(r#"SELECT default_currency FROM "user" WHERE id = $1"#)
                .bind(f.host_id)
                .fetch_one(&f.pool)
                .await
                .expect("host currency");
        assert_eq!(host_currency, "USD");
    }

    #[tokio::test]
    async fn tax_document_uses_jmd_rate_at_issue_time() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;

        let booking = hold(&app, &f, "USD").await;
        publish_now(&f.pool, "USD", "JMD", rust_decimal_macros::dec!(171.5)).await;
        confirm(&app, admin, booking).await;

        let jmd_rate = scalar(
            &f.pool,
            "SELECT jmd_exchange_rate FROM tax_document WHERE booking_id = $1 ORDER BY issue_date DESC LIMIT 1",
            booking,
        )
        .await;
        assert_eq!(jmd_rate, rust_decimal_macros::dec!(171.5));
    }

    #[tokio::test]
    async fn booking_confirms_after_currency_deactivated_and_pair_retired() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;

        let booking = hold(&app, &f, "JMD").await;
        sqlx::query(
            "UPDATE currency_exchange_rates SET retired_at = now()
             WHERE (base_currency, target_currency) IN (('USD','JMD'), ('JMD','USD'))",
        )
        .execute(&f.pool)
        .await
        .expect("retire pairs");
        sqlx::query("UPDATE currencies SET active = false WHERE code = 'JMD'")
            .execute(&f.pool)
            .await
            .expect("deactivate JMD");

        confirm(&app, admin, booking).await;

        let docs: i64 =
            sqlx::query_scalar("SELECT count(*) FROM tax_document WHERE booking_id = $1")
                .bind(booking)
                .fetch_one(&f.pool)
                .await
                .expect("count documents");
        assert!(docs >= 1, "payment receipt issued");
        let ledger: i64 =
            sqlx::query_scalar("SELECT count(*) FROM host_payout_ledger WHERE booking_id = $1")
                .bind(booking)
                .fetch_one(&f.pool)
                .await
                .expect("count ledger");
        assert_eq!(ledger, 1, "payout ledger entry created");
    }
}

mod currency_api_tests {
    //! `/api/v1/currencies` (public) and `/api/v1/admin/currencies` (admin only).

    use super::*;

    fn bearer(user: Uuid) -> (&'static str, String) {
        (
            "Authorization",
            format!("Bearer {}", api_core::auth::generate_test_jwt(user)),
        )
    }

    async fn call<S>(app: &S, req: actix_http::Request) -> (u16, serde_json::Value)
    where
        S: Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error>,
    {
        let resp = test::call_service(app, req).await;
        let status = resp.status().as_u16();
        let bytes = test::read_body(resp).await;
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        )
    }

    fn codes(body: &serde_json::Value) -> Vec<String> {
        body["currencies"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|c| c["code"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn public_list_needs_no_auth_and_hides_inactive() {
        let f = fixture().await;
        sqlx::query("UPDATE currencies SET active = false WHERE code = 'CAD'")
            .execute(&f.pool)
            .await
            .expect("deactivate CAD");
        let app = app(&f.pool).await;
        let (status, body) = call(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/currencies")
                .to_request(),
        )
        .await;
        assert_eq!(status, 200);
        let codes = codes(&body);
        assert!(codes.contains(&"USD".to_string()));
        assert!(!codes.contains(&"CAD".to_string()));
    }

    #[tokio::test]
    async fn admin_list_includes_inactive() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        sqlx::query("UPDATE currencies SET active = false WHERE code = 'CAD'")
            .execute(&f.pool)
            .await
            .expect("deactivate CAD");
        let app = app(&f.pool).await;
        let (status, body) = call(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/admin/currencies")
                .insert_header(bearer(admin))
                .to_request(),
        )
        .await;
        assert_eq!(status, 200);
        assert!(codes(&body).contains(&"CAD".to_string()));
    }

    #[tokio::test]
    async fn hosts_are_forbidden_on_every_admin_route() {
        let f = fixture().await;
        let app = app(&f.pool).await;
        let host = bearer(f.host_id);
        let requests = [
            test::TestRequest::get()
                .uri("/api/v1/admin/currencies")
                .insert_header(host.clone())
                .to_request(),
            test::TestRequest::post()
                .uri("/api/v1/admin/currencies")
                .insert_header(host.clone())
                .set_json(serde_json::json!({ "code": "TTD", "name": "Trinidad Dollar" }))
                .to_request(),
            test::TestRequest::patch()
                .uri("/api/v1/admin/currencies/CAD")
                .insert_header(host.clone())
                .set_json(serde_json::json!({ "active": false }))
                .to_request(),
        ];
        for req in requests {
            let (status, _) = call(&app, req).await;
            assert_eq!(status, 403);
        }
    }

    #[tokio::test]
    async fn anonymous_admin_calls_are_401() {
        let f = fixture().await;
        let app = app(&f.pool).await;
        let (status, _) = call(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/admin/currencies")
                .to_request(),
        )
        .await;
        assert_eq!(status, 401);
    }

    #[tokio::test]
    async fn create_returns_201() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;
        let (status, body) = call(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/admin/currencies")
                .insert_header(bearer(admin))
                .set_json(serde_json::json!({ "code": "TTD", "name": " Trinidad Dollar " }))
                .to_request(),
        )
        .await;
        assert_eq!(status, 201);
        assert_eq!(body["code"], "TTD");
        assert_eq!(body["name"], "Trinidad Dollar");
        assert_eq!(body["active"], true);
    }

    #[tokio::test]
    async fn malformed_codes_are_400() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;
        let (post, _) = call(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/admin/currencies")
                .insert_header(bearer(admin))
                .set_json(serde_json::json!({ "code": "ttd", "name": "Trinidad Dollar" }))
                .to_request(),
        )
        .await;
        assert_eq!(post, 400);
        let (patch, _) = call(
            &app,
            test::TestRequest::patch()
                .uri("/api/v1/admin/currencies/cad")
                .insert_header(bearer(admin))
                .set_json(serde_json::json!({ "active": false }))
                .to_request(),
        )
        .await;
        assert_eq!(patch, 400);
    }

    #[tokio::test]
    async fn blank_or_long_names_and_empty_updates_are_400() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;
        let long = "x".repeat(65);
        for body in [
            serde_json::json!({ "code": "TTD", "name": "  " }),
            serde_json::json!({ "code": "TTD", "name": long }),
        ] {
            let (status, _) = call(
                &app,
                test::TestRequest::post()
                    .uri("/api/v1/admin/currencies")
                    .insert_header(bearer(admin))
                    .set_json(body)
                    .to_request(),
            )
            .await;
            assert_eq!(status, 400);
        }
        let (empty, _) = call(
            &app,
            test::TestRequest::patch()
                .uri("/api/v1/admin/currencies/CAD")
                .insert_header(bearer(admin))
                .set_json(serde_json::json!({}))
                .to_request(),
        )
        .await;
        assert_eq!(empty, 400);
    }

    #[tokio::test]
    async fn patch_unknown_code_is_404() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;
        let (status, body) = call(
            &app,
            test::TestRequest::patch()
                .uri("/api/v1/admin/currencies/QQQ")
                .insert_header(bearer(admin))
                .set_json(serde_json::json!({ "active": false }))
                .to_request(),
        )
        .await;
        assert_eq!(status, 404);
        assert_eq!(body["code"], "CURRENCY_NOT_FOUND");
    }

    #[tokio::test]
    async fn duplicate_create_is_409() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;
        let (status, body) = call(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/admin/currencies")
                .insert_header(bearer(admin))
                .set_json(serde_json::json!({ "code": "USD", "name": "Dollar" }))
                .to_request(),
        )
        .await;
        assert_eq!(status, 409);
        assert_eq!(body["code"], "CURRENCY_EXISTS");
    }

    #[tokio::test]
    async fn deactivating_a_currency_in_use_is_409_with_listings() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;
        let (status, body) = call(
            &app,
            test::TestRequest::patch()
                .uri("/api/v1/admin/currencies/USD")
                .insert_header(bearer(admin))
                .set_json(serde_json::json!({ "active": false }))
                .to_request(),
        )
        .await;
        assert_eq!(status, 409);
        assert_eq!(body["code"], "CURRENCY_IN_USE");
        assert_eq!(body["listings"][0]["id"], f.listing_id.to_string());
    }
}

mod currency_inactive_tests {
    //! The hold transaction re-checks the payment currency after locking the listing.

    use super::*;

    #[tokio::test]
    async fn hold_in_inactive_identity_currency_is_422() {
        let f = fixture().await;
        // The listing is priced in USD; USD itself is deactivated (bypassing
        // the in-use guard) so only the hold's own check can refuse it.
        sqlx::query("UPDATE currencies SET active = false WHERE code = 'USD'")
            .execute(&f.pool)
            .await
            .expect("deactivate USD");
        let app = app(&f.pool).await;
        let resp = test::call_service(&app, hold_request(&f, "USD")).await;
        assert_eq!(resp.status(), 422);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["code"], "CURRENCY_INACTIVE");
    }

    #[tokio::test]
    async fn hold_transaction_refuses_inactive_currency_directly() {
        let f = fixture().await;
        sqlx::query("UPDATE currencies SET active = false WHERE code = 'JMD'")
            .execute(&f.pool)
            .await
            .expect("deactivate JMD");
        let booking = db_core::models::NewBooking {
            confirmation_code: format!("C{}", Uuid::now_v7().simple())[..10].to_uppercase(),
            guest_id: f.guest_id,
            listing_id: f.listing_id,
            date_from: chrono::NaiveDate::from_ymd_opt(2030, 8, 1).expect("date"),
            date_to: chrono::NaiveDate::from_ymd_opt(2030, 8, 3).expect("date"),
            currency: "JMD".to_string(),
            daily_rate: dec!(32000.00),
            number_of_persons: 2,
            total_days: 2,
            sub_total_price: dec!(64000.00),
            discount_value: None,
            tax_value: None,
            fee_breakdown: vec![],
            total_price: dec!(64000.00),
            cancellation_policy: db_core::models::CancellationPolicy::Flexible,
            metadata: db_core::models::BookingMetadata {
                num_adults: 2,
                ..Default::default()
            },
            door_access_code: None,
            cleaning_fee: rust_decimal::Decimal::ZERO,
            damage_deposit: rust_decimal::Decimal::ZERO,
            platform_fee: rust_decimal::Decimal::ZERO,
            tax_rate: dec!(0.1500),
            exchange_rate: dec!(160),
            promo: None,
        };
        let r = db_core::booking::create_booking(&f.pool, &booking, 120).await;
        assert!(
            matches!(r, Err(db_core::error::DbError::CurrencyInactive(ref c)) if c == "JMD"),
            "got {r:?}"
        );
    }
}

mod exchange_rate_api_tests {
    //! `/api/v1/admin/exchange-rates/*`: authz, validation and every error code.

    use super::*;

    fn bearer(user: Uuid) -> (&'static str, String) {
        (
            "Authorization",
            format!("Bearer {}", api_core::auth::generate_test_jwt(user)),
        )
    }

    async fn call<S>(app: &S, req: actix_http::Request) -> (u16, serde_json::Value)
    where
        S: Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error>,
    {
        let resp = test::call_service(app, req).await;
        let status = resp.status().as_u16();
        let bytes = test::read_body(resp).await;
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        )
    }

    fn publish(user: Uuid, body: serde_json::Value) -> actix_http::Request {
        test::TestRequest::post()
            .uri("/api/v1/admin/exchange-rates")
            .insert_header(bearer(user))
            .set_json(body)
            .to_request()
    }

    #[tokio::test]
    async fn hosts_are_forbidden_on_admin_routes_but_read_current_rates() {
        let f = fixture().await;
        let app = app(&f.pool).await;
        let host = f.host_id;
        let id = Uuid::now_v7();
        let forbidden = [
            publish(
                host,
                serde_json::json!({"base_currency":"USD","target_currency":"JMD","rate":"161"}),
            ),
            test::TestRequest::delete()
                .uri(&format!("/api/v1/admin/exchange-rates/{id}"))
                .insert_header(bearer(host))
                .to_request(),
            test::TestRequest::post()
                .uri("/api/v1/admin/exchange-rates/USD/JMD/retire")
                .insert_header(bearer(host))
                .to_request(),
            test::TestRequest::get()
                .uri("/api/v1/admin/exchange-rates/USD/JMD/history")
                .insert_header(bearer(host))
                .to_request(),
            test::TestRequest::get()
                .uri("/api/v1/admin/exchange-rates/coverage")
                .insert_header(bearer(host))
                .to_request(),
        ];
        for req in forbidden {
            let (status, _) = call(&app, req).await;
            assert_eq!(status, 403);
        }
        let (status, body) = call(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/admin/exchange-rates")
                .insert_header(bearer(host))
                .to_request(),
        )
        .await;
        assert_eq!(status, 200);
        assert!(body["rates"].as_array().is_some_and(|r| !r.is_empty()));
    }

    #[tokio::test]
    async fn guests_are_forbidden_on_the_current_table() {
        let f = fixture().await;
        let app = app(&f.pool).await;
        let (status, _) = call(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/admin/exchange-rates")
                .insert_header(bearer(f.guest_id))
                .to_request(),
        )
        .await;
        assert_eq!(status, 403);
    }

    #[tokio::test]
    async fn malformed_paths_and_rates_are_400() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;
        let (history, _) = call(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/admin/exchange-rates/usd/JMD/history")
                .insert_header(bearer(admin))
                .to_request(),
        )
        .await;
        assert_eq!(history, 400);
        let (retire, _) = call(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/admin/exchange-rates/USD/J1D/retire")
                .insert_header(bearer(admin))
                .to_request(),
        )
        .await;
        assert_eq!(retire, 400);
        for rate in ["1e5", "1,000", "-1", " 160"] {
            let (status, _) = call(
                &app,
                publish(
                    admin,
                    serde_json::json!({"base_currency":"USD","target_currency":"JMD","rate":rate}),
                ),
            )
            .await;
            assert_eq!(status, 400, "rate {rate:?}");
        }
    }

    #[tokio::test]
    async fn publish_now_and_scheduled() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;
        let (now_status, now_body) = call(
            &app,
            publish(
                admin,
                serde_json::json!({"base_currency":"USD","target_currency":"JMD","rate":"161.5"}),
            ),
        )
        .await;
        assert_eq!(now_status, 201);
        assert_eq!(now_body["state"], "current");
        let at = (chrono::Utc::now() + chrono::Duration::days(2)).to_rfc3339();
        let (sched_status, sched_body) = call(
            &app,
            publish(
                admin,
                serde_json::json!({"base_currency":"USD","target_currency":"JMD","rate":"162","effective_at":at}),
            ),
        )
        .await;
        assert_eq!(sched_status, 201);
        assert_eq!(sched_body["state"], "scheduled");
    }

    #[tokio::test]
    async fn large_change_needs_confirmation_then_succeeds() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;
        let (status, body) = call(
            &app,
            publish(
                admin,
                serde_json::json!({"base_currency":"USD","target_currency":"JMD","rate":"176"}),
            ),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["code"], "RATE_CHANGE_CONFIRMATION_REQUIRED");
        assert_eq!(body["baseline_rate"], "160.000000");
        assert_eq!(body["proposed_rate"], "176");
        assert_eq!(body["deviation_pct"], "10.00");
        let (confirmed, _) = call(
            &app,
            publish(
                admin,
                serde_json::json!({"base_currency":"USD","target_currency":"JMD","rate":"176","confirm_large_change":true}),
            ),
        )
        .await;
        assert_eq!(confirmed, 201);
    }

    #[tokio::test]
    async fn effective_time_bounds_are_422() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;
        let past = (chrono::Utc::now() - chrono::Duration::minutes(10)).to_rfc3339();
        let (s1, b1) = call(
            &app,
            publish(
                admin,
                serde_json::json!({"base_currency":"USD","target_currency":"JMD","rate":"161","effective_at":past}),
            ),
        )
        .await;
        assert_eq!(
            (s1, b1["code"].clone()),
            (422, "EFFECTIVE_AT_IN_PAST".into())
        );
        let far = (chrono::Utc::now() + chrono::Duration::days(400)).to_rfc3339();
        let (s2, b2) = call(
            &app,
            publish(
                admin,
                serde_json::json!({"base_currency":"USD","target_currency":"JMD","rate":"161","effective_at":far}),
            ),
        )
        .await;
        assert_eq!(
            (s2, b2["code"].clone()),
            (422, "EFFECTIVE_AT_TOO_FAR".into())
        );
    }

    #[tokio::test]
    async fn duplicate_version_is_409() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;
        let at = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        let body = serde_json::json!({"base_currency":"USD","target_currency":"JMD","rate":"161","effective_at":at});
        let (first, _) = call(&app, publish(admin, body.clone())).await;
        assert_eq!(first, 201);
        let (second, b) = call(&app, publish(admin, body)).await;
        assert_eq!(
            (second, b["code"].clone()),
            (409, "RATE_VERSION_EXISTS".into())
        );
    }

    #[tokio::test]
    async fn inactive_and_unknown_currencies_are_422() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        sqlx::query("UPDATE currencies SET active = false WHERE code = 'CAD'")
            .execute(&f.pool)
            .await
            .expect("deactivate");
        let app = app(&f.pool).await;
        let (s1, b1) = call(
            &app,
            publish(
                admin,
                serde_json::json!({"base_currency":"USD","target_currency":"CAD","rate":"1.36"}),
            ),
        )
        .await;
        assert_eq!((s1, b1["code"].clone()), (422, "CURRENCY_INACTIVE".into()));
        let (s2, b2) = call(
            &app,
            publish(
                admin,
                serde_json::json!({"base_currency":"USD","target_currency":"QQQ","rate":"1"}),
            ),
        )
        .await;
        assert_eq!((s2, b2["code"].clone()), (422, "CURRENCY_NOT_FOUND".into()));
    }

    #[tokio::test]
    async fn delete_rules() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;
        let at = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        let (_, created) = call(
            &app,
            publish(
                admin,
                serde_json::json!({"base_currency":"USD","target_currency":"JMD","rate":"161","effective_at":at}),
            ),
        )
        .await;
        let scheduled = created["id"].as_str().expect("id").to_string();
        let effective: Uuid = sqlx::query_scalar(
            "SELECT id FROM currency_exchange_rates WHERE base_currency='USD' AND target_currency='JMD' AND effective_at <= now() LIMIT 1",
        )
        .fetch_one(&f.pool)
        .await
        .expect("effective id");
        let del = |id: String| {
            test::TestRequest::delete()
                .uri(&format!("/api/v1/admin/exchange-rates/{id}"))
                .insert_header(bearer(admin))
                .to_request()
        };
        let (s1, b1) = call(&app, del(effective.to_string())).await;
        assert_eq!(
            (s1, b1["code"].clone()),
            (409, "RATE_ALREADY_EFFECTIVE".into())
        );
        let (s2, _) = call(&app, del(scheduled)).await;
        assert_eq!(s2, 204);
        let (s3, _) = call(&app, del(Uuid::now_v7().to_string())).await;
        assert_eq!(s3, 404);
    }

    #[tokio::test]
    async fn retire_rules_history_and_coverage() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;
        // The fixture listing is priced in USD.
        let (s1, b1) = call(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/admin/exchange-rates/USD/JMD/retire")
                .insert_header(bearer(admin))
                .to_request(),
        )
        .await;
        assert_eq!((s1, b1["code"].clone()), (409, "CURRENCY_IN_USE".into()));
        assert_eq!(b1["listings"][0]["id"], f.listing_id.to_string());

        let (s2, _) = call(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/admin/exchange-rates/EUR/CAD/retire")
                .insert_header(bearer(admin))
                .to_request(),
        )
        .await;
        assert_eq!(s2, 404, "no such pair");

        let (s3, b3) = call(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/admin/exchange-rates/USD/JMD/history")
                .insert_header(bearer(admin))
                .to_request(),
        )
        .await;
        assert_eq!(s3, 200);
        assert_eq!(b3["versions"][0]["state"], "current");

        let (s4, b4) = call(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/admin/exchange-rates/coverage")
                .insert_header(bearer(admin))
                .to_request(),
        )
        .await;
        assert_eq!(s4, 200);
        let targets: Vec<&str> = b4
            .as_array()
            .map(|gaps| {
                gaps.iter()
                    .filter_map(|g| g["target_currency"].as_str())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(targets, vec!["CAD", "EUR"]);
    }

    #[::core::prelude::v1::test]
    fn grace_setting_is_bounded() {
        let app_settings = |secs| api_core::settings::Application {
            max_attempts: 1,
            pending_hold_minutes: api_core::settings::DEFAULT_PENDING_HOLD_MINUTES,
            rate_effective_grace_seconds: secs,
            max_stay_nights: api_core::settings::DEFAULT_MAX_STAY_NIGHTS,
            request_response_hours: api_core::settings::DEFAULT_REQUEST_RESPONSE_HOURS,
            quote_valid_hours: api_core::settings::DEFAULT_QUOTE_VALID_HOURS,
            request_near_expiry_pct: api_core::settings::DEFAULT_REQUEST_NEAR_EXPIRY_PCT,
        };
        assert!(app_settings(0).rate_effective_grace().is_err());
        assert!(app_settings(301).rate_effective_grace().is_err());
        assert_eq!(
            app_settings(60)
                .rate_effective_grace()
                .map(|d| d.num_seconds()),
            Ok(60)
        );
        assert!(app_settings(300).rate_effective_grace().is_ok());
    }

    #[::core::prelude::v1::test]
    fn grace_setting_defaults_to_60_and_reads_configured_value() {
        let default: api_core::settings::Application =
            serde_json::from_value(serde_json::json!({ "max_attempts": 3 }))
                .expect("defaults apply");
        assert_eq!(default.rate_effective_grace_seconds, 60);
        let configured: api_core::settings::Application = serde_json::from_value(
            serde_json::json!({ "max_attempts": 3, "rate_effective_grace_seconds": 30 }),
        )
        .expect("configured value");
        assert_eq!(
            configured.rate_effective_grace().map(|d| d.num_seconds()),
            Ok(30)
        );
    }
}

mod inactive_account_tests {
    //! A deactivated or deleted account's tokens stop working immediately.

    use super::*;

    fn publish(user: Uuid) -> actix_http::Request {
        test::TestRequest::post()
            .uri("/api/v1/admin/exchange-rates")
            .insert_header((
                "Authorization",
                format!("Bearer {}", api_core::auth::generate_test_jwt(user)),
            ))
            .set_json(serde_json::json!({
                "base_currency": "USD", "target_currency": "JMD", "rate": "161"
            }))
            .to_request()
    }

    #[tokio::test]
    async fn deactivated_admin_cannot_publish_rates() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        sqlx::query(r#"UPDATE "user" SET is_active = false WHERE id = $1"#)
            .bind(admin)
            .execute(&f.pool)
            .await
            .expect("deactivate admin");
        let app = app(&f.pool).await;
        let resp = test::call_service(&app, publish(admin)).await;
        assert_eq!(resp.status(), 401);
    }

    #[tokio::test]
    async fn deleted_admin_cannot_publish_rates() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        sqlx::query(r#"UPDATE "user" SET deleted_at = now() WHERE id = $1"#)
            .bind(admin)
            .execute(&f.pool)
            .await
            .expect("soft-delete admin");
        let app = app(&f.pool).await;
        let resp = test::call_service(&app, publish(admin)).await;
        assert_eq!(resp.status(), 401);
    }

    #[tokio::test]
    async fn active_admin_still_publishes_and_hosts_cannot_change_payouts() {
        let f = fixture().await;
        let admin = create_admin(&f.pool).await;
        let app = app(&f.pool).await;
        let resp = test::call_service(&app, publish(admin)).await;
        assert_eq!(resp.status(), 201);

        // Payout status is admin-only: a host now gets 403 (was 401).
        let resp = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri(&format!("/api/v1/admin/ledger/{}/status", Uuid::now_v7()))
                .insert_header((
                    "Authorization",
                    format!("Bearer {}", api_core::auth::generate_test_jwt(f.host_id)),
                ))
                .set_json(serde_json::json!({ "status": "paid" }))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 403);
    }
}
