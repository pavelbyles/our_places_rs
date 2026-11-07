//! Who may call which booking route: anonymous, stranger, guest, host,
//! another host and admin. A route that loses its check fails here.

use actix_web::{App, test, web};
use db_core::models::UserRole;
use rust_decimal_macros::dec;
use sqlx::PgPool;
use sqlx_db_tester::TestPg;
use std::path::Path;
use uuid::Uuid;

fn new_user(id: Uuid, role: UserRole) -> db_core::models::NewUser {
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

pub(super) async fn user(pool: &PgPool, role: UserRole) -> Uuid {
    let id = Uuid::now_v7();
    db_core::user::create_user(pool, &new_user(id, role))
        .await
        .unwrap();
    if role == UserRole::Host {
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
        .unwrap();
    }
    id
}

pub(super) async fn listing(pool: &PgPool, host_id: Uuid) -> Uuid {
    db_core::listing::create_listing(
        pool,
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
            cleaning_fee: None,
            damage_deposit: None,
            guests_included: None,
            extra_guest_fee: None,
            is_active: true,
        },
    )
    .await
    .unwrap()
    .id
}

pub(super) fn settings() -> api_core::settings::Settings {
    api_core::settings::Settings {
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
    }
}

pub(super) fn tax_settings() -> crate::settings::TaxSettings {
    crate::settings::TaxSettings {
        taj_supplier_trn: "000-111-222".to_string(),
        supplier_name: "Our Places Jamaica Limited".to_string(),
        supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
    }
}

pub(super) fn booking_request(guest_id: Uuid, listing_id: Uuid) -> serde_json::Value {
    serde_json::json!({
        "guest_id": guest_id,
        "listing_id": listing_id,
        "check_in": "2030-08-01",
        "check_out": "2030-08-03",
        "num_adults": 1,
        "num_children": 0,
        "num_infants": 0,
        "num_pets": 0,
        "is_business_trip": false,
        "currency": "USD",
        "agreed_cancellation_policy": "Flexible",
    })
}

#[tokio::test]
async fn test_booking_routes_enforce_user_and_booking_authorization() {
    dotenvy::dotenv().ok();
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".to_string());
    let test_db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
    let pool = test_db.get_pool().await;

    let host = user(&pool, UserRole::Host).await;
    let other_host = user(&pool, UserRole::Host).await;
    let guest = user(&pool, UserRole::Booker).await;
    let stranger = user(&pool, UserRole::Booker).await;
    let admin = user(&pool, UserRole::Admin).await;
    let villa = listing(&pool, host).await;
    listing(&pool, other_host).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(settings()))
            .app_data(web::Data::new(tax_settings()))
            .configure(crate::apis::configure_routes),
    )
    .await;

    let call = |method: test::TestRequest, uri: String, as_user: Option<Uuid>| {
        let req = method
            .uri(&uri)
            .insert_header(("Accept", "application/json"));
        match as_user {
            Some(id) => req.insert_header((
                "Authorization",
                format!("Bearer {}", api_core::auth::generate_test_jwt(id)),
            )),
            None => req,
        }
    };
    macro_rules! status {
        ($req:expr) => {
            test::call_service(&app, $req.to_request())
                .await
                .status()
                .as_u16()
        };
    }

    // Public routes stay open to anonymous visitors.
    assert_eq!(
        status!(call(
            test::TestRequest::get(),
            format!(
                "/api/v1/bookings/availability?listing_id={villa}&date_from=2030-08-01&date_to=2030-08-03"
            ),
            None
        )),
        200
    );
    assert_eq!(
        status!(call(
            test::TestRequest::get(),
            format!(
                "/api/v1/bookings/listing/{villa}/unavailable-ranges?from=2030-08-01&to=2030-09-01"
            ),
            None
        )),
        200
    );

    // Booking as someone else is refused (it would also cancel their holds).
    assert_eq!(
        status!(
            call(
                test::TestRequest::post(),
                "/api/v1/bookings".into(),
                Some(stranger)
            )
            .set_json(booking_request(guest, villa))
        ),
        403
    );
    assert_eq!(
        status!(
            call(test::TestRequest::post(), "/api/v1/bookings".into(), None)
                .set_json(booking_request(guest, villa))
        ),
        401
    );
    let resp = test::call_service(
        &app,
        call(
            test::TestRequest::post(),
            "/api/v1/bookings".into(),
            Some(guest),
        )
        .set_json(booking_request(guest, villa))
        .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let booking: serde_json::Value = test::read_body_json(resp).await;
    let id = booking["id"].as_str().unwrap().to_string();

    // Reading one booking: its parties and admins only; everyone else 404s.
    let one = format!("/api/v1/bookings/{id}");
    for (who, expected) in [
        (None, 401),
        (Some(stranger), 404),
        (Some(other_host), 404),
        (Some(guest), 200),
        (Some(host), 200),
        (Some(admin), 200),
    ] {
        assert_eq!(
            status!(call(test::TestRequest::get(), one.clone(), who)),
            expected,
            "GET booking as {who:?}"
        );
    }
    for (path, who, expected) in [
        (format!("{one}/messages"), stranger, 404),
        (format!("{one}/cancellation-quote"), stranger, 404),
        (format!("{one}/documents"), stranger, 404),
        (format!("{one}/messages"), guest, 200),
    ] {
        assert_eq!(
            status!(call(test::TestRequest::get(), path.clone(), Some(who))),
            expected,
            "GET {path}"
        );
    }
    assert_eq!(
        status!(
            call(test::TestRequest::patch(), one.clone(), Some(stranger))
                .set_json(serde_json::json!({ "door_access_code": "1234" }))
        ),
        404
    );

    // A user's booking list: that user or an admin.
    let by_guest = format!("/api/v1/bookings/user/{guest}");
    for (who, expected) in [(stranger, 404), (host, 404), (guest, 200), (admin, 200)] {
        assert_eq!(
            status!(call(test::TestRequest::get(), by_guest.clone(), Some(who))),
            expected,
            "GET user bookings as {who}"
        );
    }

    // A listing's bookings: its host or an admin.
    let by_listing = format!("/api/v1/bookings/listing/{villa}");
    for (who, expected) in [(guest, 404), (other_host, 404), (host, 200), (admin, 200)] {
        assert_eq!(
            status!(call(
                test::TestRequest::get(),
                by_listing.clone(),
                Some(who)
            )),
            expected,
            "GET listing bookings as {who}"
        );
    }

    // The full list is scoped server-side: admins see all, hosts their own.
    for (who, sees_it) in [
        (admin, true),
        (host, true),
        (other_host, false),
        (guest, false),
    ] {
        let resp = test::call_service(
            &app,
            call(
                test::TestRequest::get(),
                "/api/v1/bookings?per_page=100".into(),
                Some(who),
            )
            .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);
        let list: serde_json::Value = test::read_body_json(resp).await;
        let found = list
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["id"].as_str() == Some(id.as_str()));
        assert_eq!(found, sees_it, "booking list as {who}");
    }

    // Status changes by role. Only admins confirm or complete; anyone on the
    // booking may cancel; a cancelled booking can never be revived.
    let patch = |who: Uuid, body: serde_json::Value| {
        call(test::TestRequest::patch(), one.clone(), Some(who)).set_json(body)
    };
    let to = |s: &str| serde_json::json!({ "status": s });
    assert_eq!(status!(patch(guest, to("confirmed"))), 403);
    assert_eq!(status!(patch(host, to("confirmed"))), 403);
    assert_eq!(status!(patch(host, to("completed"))), 403);
    assert_eq!(status!(patch(guest, to("pending"))), 403);
    assert_eq!(
        status!(patch(
            guest,
            serde_json::json!({ "door_access_code": "1234" })
        )),
        403
    );
    assert_eq!(status!(patch(admin, to("confirmed"))), 200);
    assert_eq!(status!(patch(guest, to("cancelled"))), 200);
    let resp = test::call_service(&app, patch(admin, to("confirmed")).to_request()).await;
    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "BOOKING_NOT_MODIFIABLE");

    // Transfer and hard delete are admin-only.
    let transfer = format!("{one}/transfer");
    for who in [guest, host] {
        assert_eq!(
            status!(
                call(test::TestRequest::post(), transfer.clone(), Some(who))
                    .set_json(serde_json::json!({ "guest_id": who }))
            ),
            403
        );
        assert_eq!(
            status!(call(test::TestRequest::delete(), one.clone(), Some(who))),
            403
        );
    }
    assert_eq!(
        status!(call(test::TestRequest::delete(), one.clone(), None)),
        401
    );
    // Hard delete also needs the feature flag (off by default).
    assert_eq!(
        status!(call(test::TestRequest::delete(), one.clone(), Some(admin))),
        403
    );
    let mut hard_deletes_on = settings();
    hard_deletes_on.feature_flags.enable_hard_deletes = true;
    let flagged_app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(hard_deletes_on))
            .app_data(web::Data::new(tax_settings()))
            .configure(crate::apis::configure_routes),
    )
    .await;
    let delete = |path: &str, who: Uuid| {
        call(test::TestRequest::delete(), path.to_string(), Some(who)).to_request()
    };
    assert_eq!(
        test::call_service(&flagged_app, delete(&one, host))
            .await
            .status(),
        403
    );
    // `one` was confirmed, so it has statutory records: refused, not a 500.
    let resp = test::call_service(&flagged_app, delete(&one, admin)).await;
    assert_eq!(resp.status(), 409);
    // A hold that never got that far can be removed.
    let mut fresh = booking_request(guest, villa);
    fresh["check_in"] = "2030-09-01".into();
    fresh["check_out"] = "2030-09-03".into();
    let resp = test::call_service(
        &flagged_app,
        call(
            test::TestRequest::post(),
            "/api/v1/bookings".into(),
            Some(guest),
        )
        .set_json(fresh)
        .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let hold: serde_json::Value = test::read_body_json(resp).await;
    let hold = format!("/api/v1/bookings/{}", hold["id"].as_str().unwrap());
    assert_eq!(
        test::call_service(&flagged_app, delete(&hold, admin))
            .await
            .status(),
        204
    );
}

/// Changing log levels needs an admin *according to the database*: a valid
/// token for any other user is refused, whichever header carries it.
#[tokio::test]
async fn test_actuator_log_level_changes_require_a_stored_admin_role() {
    dotenvy::dotenv().ok();
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".to_string());
    let test_db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
    let pool = test_db.get_pool().await;

    let guest = user(&pool, UserRole::Booker).await;
    let host = user(&pool, UserRole::Host).await;
    let admin = user(&pool, UserRole::Admin).await;
    let unknown = Uuid::now_v7();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .configure(api_core::actuator::configure_actuator),
    )
    .await;

    let bearer = |id: Uuid| format!("Bearer {}", api_core::auth::generate_test_jwt(id));
    let post = || {
        test::TestRequest::post()
            .uri("/loggers/booking_api")
            .set_json(serde_json::json!({ "configuredLevel": "INFO" }))
    };
    let status = |resp: actix_web::dev::ServiceResponse| resp.status().as_u16();

    assert_eq!(
        status(test::call_service(&app, post().to_request()).await),
        401
    );
    for (who, expected) in [(guest, 403), (host, 403), (unknown, 401), (admin, 200)] {
        // Called directly: the user token is in `Authorization`.
        let resp = test::call_service(
            &app,
            post()
                .insert_header(("Authorization", bearer(who)))
                .to_request(),
        )
        .await;
        assert_eq!(status(resp), expected, "direct call as {who}");
        // Behind Cloud Run: service identity in `Authorization`, user token forwarded.
        let resp = test::call_service(
            &app,
            post()
                .insert_header(("Authorization", "Bearer google-id-token"))
                .insert_header(("X-Forwarded-Authorization", bearer(who)))
                .to_request(),
        )
        .await;
        assert_eq!(status(resp), expected, "forwarded call as {who}");
    }
}

/// A direct update that shortens a stay below the weekly threshold must clear
/// the weekly discount, or `accommodation_net()` understates what was charged.
#[tokio::test]
async fn stale_discount_is_cleared_when_a_direct_update_stops_it_applying() {
    dotenvy::dotenv().ok();
    let db_url = std::env::var(promo_codes::DATABASE_URL_ENV)
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".to_string());
    let test_db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
    let pool = test_db.get_pool().await;

    let host = user(&pool, UserRole::Host).await;
    let guest = user(&pool, UserRole::Booker).await;
    let admin = user(&pool, UserRole::Admin).await;
    let villa = listing(&pool, host).await;
    sqlx::query("UPDATE listing SET weekly_discount_percentage = 10 WHERE id = $1")
        .bind(villa)
        .execute(&pool)
        .await
        .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(settings()))
            .app_data(web::Data::new(tax_settings()))
            .configure(crate::apis::configure_routes),
    )
    .await;
    let bearer = |id: Uuid| {
        (
            "Authorization",
            format!("Bearer {}", api_core::auth::generate_test_jwt(id)),
        )
    };

    let mut seven_nights = booking_request(guest, villa);
    seven_nights["check_out"] = serde_json::json!("2030-08-08");
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/bookings")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .set_json(seven_nights)
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let id: Uuid = test::read_body_json::<serde_json::Value, _>(resp).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let held = db_core::booking::get_booking_by_id(&pool, id)
        .await
        .unwrap();
    assert_eq!(held.discount_value, Some(dec!(140.00)));

    sqlx::query(
        "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at) \
         VALUES ('USD', 'JMD', 160, NOW()) ON CONFLICT DO NOTHING",
    )
    .execute(&pool)
    .await
    .unwrap();
    let resp = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&format!("/api/v1/bookings/{id}"))
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(admin))
            .set_json(serde_json::json!({ "status": "confirmed" }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);

    let resp = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&format!("/api/v1/bookings/{id}"))
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(admin))
            .set_json(serde_json::json!({ "date_to": "2030-08-06" }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);

    let shortened = db_core::booking::get_booking_by_id(&pool, id)
        .await
        .unwrap();
    assert_eq!(shortened.discount_value, None);
    assert_eq!(shortened.accommodation_net(), dec!(1000.00));
    assert_eq!(shortened.sub_total_price, dec!(1000.00));
}

/// The change quote includes the cleaning fee and uses the ledger row's
/// commission, so what it promises is what approval writes.
#[tokio::test]
async fn reservation_change_payout_quote_matches_what_approval_writes() {
    dotenvy::dotenv().ok();
    let db_url = std::env::var(promo_codes::DATABASE_URL_ENV)
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".to_string());
    let test_db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
    let pool = test_db.get_pool().await;

    let host = user(&pool, UserRole::Host).await;
    let guest = user(&pool, UserRole::Booker).await;
    let admin = user(&pool, UserRole::Admin).await;
    let villa = listing(&pool, host).await;
    sqlx::query("UPDATE listing SET cleaning_fee = 100 WHERE id = $1")
        .bind(villa)
        .execute(&pool)
        .await
        .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(settings()))
            .app_data(web::Data::new(tax_settings()))
            .configure(crate::apis::configure_routes),
    )
    .await;
    let bearer = |id: Uuid| {
        (
            "Authorization",
            format!("Bearer {}", api_core::auth::generate_test_jwt(id)),
        )
    };

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/bookings")
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .set_json(booking_request(guest, villa))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let id: Uuid = test::read_body_json::<serde_json::Value, _>(resp).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let resp = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&format!("/api/v1/bookings/{id}"))
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(admin))
            .set_json(serde_json::json!({ "status": "confirmed" }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);

    // The listing's commission moves after the ledger row was written at 10%.
    sqlx::query("UPDATE listing SET commission_pct = 0.2 WHERE id = $1")
        .bind(villa)
        .execute(&pool)
        .await
        .unwrap();

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/bookings/{id}/change-quote"))
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .set_json(serde_json::json!({ "date_to": "2030-08-04" }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let quote: serde_json::Value = test::read_body_json(resp).await;
    let amount = |v: &serde_json::Value| {
        v.to_string()
            .trim_matches('"')
            .parse::<rust_decimal::Decimal>()
            .unwrap()
    };

    // 2 nights at 200.00 plus 100.00 cleaning, less 10%.
    assert_eq!(amount(&quote["previous_net_payout"]), dec!(450.00));
    // 3 nights at 200.00 plus 100.00 cleaning, less the row's 10%, not the listing's 20%.
    assert_eq!(amount(&quote["proposed_net_payout"]), dec!(630.00));
    assert_eq!(
        amount(&quote["pricing_snapshot"]["host_gross_amount"]),
        dec!(700.00)
    );
}

mod promo_codes {
    use super::*;

    pub(super) static DATABASE_URL_ENV: &str = "DATABASE_URL";

    fn test_db() -> TestPg {
        dotenvy::dotenv().ok();
        let db_url = std::env::var(DATABASE_URL_ENV).unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/our_places".to_string()
        });
        TestPg::new(db_url, Path::new("../../db_core/migrations"))
    }

    fn body(code: &str, listing_ids: Vec<Uuid>) -> serde_json::Value {
        serde_json::json!({
            "code": code,
            "discount_type": "percentage",
            "discount_value": "10",
            "scope": "stay",
            "valid_from": "2030-01-01T00:00:00Z",
            "valid_to": "2030-12-31T00:00:00Z",
            "applies_to_all": listing_ids.is_empty(),
            "listing_ids": listing_ids,
        })
    }

    macro_rules! call {
        ($app:expr, $req:expr, $who:expr) => {{
            let req = $req.insert_header(("Accept", "application/json"));
            let req = match $who {
                Some(id) => req.insert_header((
                    "Authorization",
                    format!("Bearer {}", api_core::auth::generate_test_jwt(id)),
                )),
                None => req,
            };
            test::call_service(&$app, req.to_request()).await
        }};
    }

    macro_rules! app {
        ($pool:expr) => {
            test::init_service(
                App::new()
                    .app_data(web::Data::new($pool.clone()))
                    .app_data(web::Data::new(settings()))
                    .app_data(web::Data::new(tax_settings()))
                    .configure(crate::apis::configure_routes),
            )
            .await
        };
    }

    const URI: &str = "/api/v1/admin/promo-codes";

    #[tokio::test]
    async fn only_the_right_people_can_create_view_and_deactivate() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let admin = user(&pool, UserRole::Admin).await;
        let host = user(&pool, UserRole::Host).await;
        let other_host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let villa = listing(&pool, host).await;
        let other_villa = listing(&pool, other_host).await;
        let app = app!(pool);

        // Anonymous callers get 401 and guests 403, on every route.
        let id = Uuid::now_v7();
        let routes = || {
            [
                test::TestRequest::get().uri(URI),
                test::TestRequest::post()
                    .uri(URI)
                    .set_json(body("ANYONE", vec![])),
                test::TestRequest::get().uri(&format!("{URI}/{id}")),
                test::TestRequest::post().uri(&format!("{URI}/{id}/deactivate")),
            ]
        };
        for req in routes() {
            assert_eq!(call!(app, req, None::<Uuid>).status(), 401);
        }
        for req in routes() {
            assert_eq!(call!(app, req, Some(guest)).status(), 403);
        }

        // A host can't select another host's listing.
        let resp = call!(
            app,
            test::TestRequest::post()
                .uri(URI)
                .set_json(body("NOTMINE", vec![other_villa])),
            Some(host)
        );
        assert_eq!(resp.status(), 403);

        // A host's code is theirs; an admin's has no owner and can select any listing.
        let resp = call!(
            app,
            test::TestRequest::post()
                .uri(URI)
                .set_json(body(" mine ", vec![villa])),
            Some(host)
        );
        assert_eq!(resp.status(), 201);
        let mine: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(mine["code"], "MINE");
        assert_eq!(mine["owner_host_id"], host.to_string());
        let mine_id = mine["id"].as_str().expect("id").to_string();

        let resp = call!(
            app,
            test::TestRequest::post()
                .uri(URI)
                .set_json(body("FROMADMIN", vec![other_villa])),
            Some(admin)
        );
        assert_eq!(resp.status(), 201);
        let from_admin: serde_json::Value = test::read_body_json(resp).await;
        assert!(from_admin["owner_host_id"].is_null());
        let admin_id = from_admin["id"].as_str().expect("id").to_string();

        // Reads and writes by a host who doesn't own the code are refused.
        for (who, code_id, expected) in [
            (other_host, mine_id.as_str(), 403),
            (host, admin_id.as_str(), 403),
            (host, mine_id.as_str(), 200),
            (admin, mine_id.as_str(), 200),
        ] {
            let resp = call!(
                app,
                test::TestRequest::get().uri(&format!("{URI}/{code_id}")),
                Some(who)
            );
            assert_eq!(resp.status(), expected, "GET {code_id} as {who}");
        }
        for (who, code_id) in [(other_host, mine_id.as_str()), (host, admin_id.as_str())] {
            let resp = call!(
                app,
                test::TestRequest::post().uri(&format!("{URI}/{code_id}/deactivate")),
                Some(who)
            );
            assert_eq!(resp.status(), 403);
        }

        // Lists: hosts see their own, admins everything.
        let resp = call!(app, test::TestRequest::get().uri(URI), Some(host));
        let list: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(list["total"], 1);
        assert_eq!(list["codes"][0]["code"], "MINE");
        let resp = call!(app, test::TestRequest::get().uri(URI), Some(admin));
        let list: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(list["total"], 2);

        // The actor comes from the token, and a second deactivation is refused.
        let resp = call!(
            app,
            test::TestRequest::post().uri(&format!("{URI}/{mine_id}/deactivate")),
            Some(host)
        );
        assert_eq!(resp.status(), 200);
        let by: Option<Uuid> =
            sqlx::query_scalar("SELECT deactivated_by FROM promo_code WHERE code = 'MINE'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(by, Some(host));
        let resp = call!(
            app,
            test::TestRequest::post().uri(&format!("{URI}/{mine_id}/deactivate")),
            Some(host)
        );
        assert_eq!(resp.status(), 409);
        let err: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(err["code"], "PROMO_ALREADY_DEACTIVATED");
    }

    #[tokio::test]
    async fn create_returns_typed_errors() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let admin = user(&pool, UserRole::Admin).await;
        sqlx::query("UPDATE currencies SET active = FALSE WHERE code = 'CAD'")
            .execute(&pool)
            .await
            .unwrap();
        let app = app!(pool);

        // A case-variant duplicate.
        let resp = call!(
            app,
            test::TestRequest::post()
                .uri(URI)
                .set_json(body("SUMMER", vec![])),
            Some(admin)
        );
        assert_eq!(resp.status(), 201);
        let resp = call!(
            app,
            test::TestRequest::post()
                .uri(URI)
                .set_json(body("summer", vec![])),
            Some(admin)
        );
        assert_eq!(resp.status(), 409);
        let err: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(err["code"], "PROMO_CODE_EXISTS");

        // Invalid terms.
        let mut too_precise = body("PRECISE", vec![]);
        too_precise["discount_value"] = serde_json::json!("33.335");
        let resp = call!(
            app,
            test::TestRequest::post().uri(URI).set_json(too_precise),
            Some(admin)
        );
        assert_eq!(resp.status(), 400);

        // A missing listing, and an inactive currency.
        let resp = call!(
            app,
            test::TestRequest::post()
                .uri(URI)
                .set_json(body("GHOST", vec![Uuid::now_v7()])),
            Some(admin)
        );
        assert_eq!(resp.status(), 400);
        let mut inactive = body("INACTIVE", vec![]);
        inactive["discount_type"] = serde_json::json!("fixed");
        inactive["currency"] = serde_json::json!("CAD");
        let resp = call!(
            app,
            test::TestRequest::post().uri(URI).set_json(inactive),
            Some(admin)
        );
        assert_eq!(resp.status(), 422);
        let err: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(err["code"], "CURRENCY_INACTIVE");

        // An unknown ID.
        let resp = call!(
            app,
            test::TestRequest::get().uri(&format!("{URI}/{}", Uuid::now_v7())),
            Some(admin)
        );
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    async fn the_list_reports_remaining_uses() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let admin = user(&pool, UserRole::Admin).await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let villa = listing(&pool, host).await;
        let app = app!(pool);

        let mut limited = body("LIMITED", vec![]);
        limited["total_usage_limit"] = serde_json::json!(3);
        limited["valid_from"] = serde_json::json!("2020-01-01T00:00:00Z");
        limited["valid_to"] = serde_json::json!("2040-01-01T00:00:00Z");
        assert_eq!(
            call!(
                app,
                test::TestRequest::post().uri(URI).set_json(limited),
                Some(admin)
            )
            .status(),
            201
        );
        let mut hold = booking_request(guest, villa);
        hold["promo_code"] = serde_json::json!("LIMITED");
        assert_eq!(
            call!(
                app,
                test::TestRequest::post()
                    .uri("/api/v1/bookings")
                    .set_json(hold),
                Some(guest)
            )
            .status(),
            201
        );

        let resp = call!(app, test::TestRequest::get().uri(URI), Some(admin));
        let list: serde_json::Value = test::read_body_json(resp).await;
        let code = &list["codes"][0];
        assert_eq!(code["active_holds"], 1);
        assert_eq!(code["confirmed_uses"], 0);
        assert_eq!(code["remaining_uses"], 2);
    }
}
