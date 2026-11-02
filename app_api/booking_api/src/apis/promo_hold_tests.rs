//! Redeeming a promo code through the booking API.

use super::authz_tests::{booking_request, listing, settings, tax_settings, user};
use actix_web::{App, test, web};
use db_core::models::{Booking, UserRole};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sqlx::PgPool;
use sqlx_db_tester::TestPg;
use std::path::Path;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

static DATABASE_URL_ENV: &str = "DATABASE_URL";

fn test_db() -> TestPg {
    dotenvy::dotenv().ok();
    let db_url = std::env::var(DATABASE_URL_ENV)
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".to_string());
    TestPg::new(db_url, Path::new("../../db_core/migrations"))
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

fn bearer(id: Uuid) -> (&'static str, String) {
    (
        "Authorization",
        format!("Bearer {}", api_core::auth::generate_test_jwt(id)),
    )
}

/// A 10% code covering every listing, valid now.
pub(super) async fn code(pool: &PgPool, created_by: Uuid, value: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO promo_code (code, discount_type, discount_value, scope, valid_from, valid_to,
             applies_to_all, created_by)
         VALUES ($1, 'percentage', 10, 'stay', NOW() - INTERVAL '1 day', NOW() + INTERVAL '1 day', TRUE, $2)
         RETURNING id",
    )
    .bind(value)
    .bind(created_by)
    .fetch_one(pool)
    .await
    .expect("insert promo code")
}

fn with_code(guest: Uuid, villa: Uuid, value: &str) -> serde_json::Value {
    let mut body = booking_request(guest, villa);
    body["promo_code"] = serde_json::json!(value);
    body
}

macro_rules! post_hold {
    ($app:expr, $guest:expr, $body:expr) => {
        test::call_service(
            &$app,
            test::TestRequest::post()
                .uri("/api/v1/bookings")
                .insert_header(("Accept", "application/json"))
                .insert_header(bearer($guest))
                .set_json($body)
                .to_request(),
        )
        .await
    };
}

async fn booking_of(pool: &PgPool, guest: Uuid) -> Option<Booking> {
    let id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM booking WHERE guest_id = $1 LIMIT 1")
        .bind(guest)
        .fetch_optional(pool)
        .await
        .expect("find booking");
    match id {
        Some(id) => Some(
            db_core::booking::get_booking_by_id(pool, id)
                .await
                .expect("read booking"),
        ),
        None => None,
    }
}

#[tokio::test]
async fn a_valid_code_prices_the_hold_and_is_stored() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let guest = user(&pool, UserRole::Booker).await;
    let villa = listing(&pool, host).await;
    let id = code(&pool, host, "SUMMER").await;
    let app = app!(pool);

    let resp = post_hold!(app, guest, with_code(guest, villa, " summer "));
    assert_eq!(resp.status(), 201);

    let booking = booking_of(&pool, guest).await.expect("hold created");
    assert_eq!(booking.promo_code_id, Some(id));
    assert_eq!(booking.promo_stay_discount, dec!(40.00));
    assert_eq!(booking.promo_cleaning_discount, Decimal::ZERO);
    assert_eq!(booking.accommodation_net(), dec!(360.00));
    // 360.00 + 15% GCT of 54.00 + 5% platform fee of 18.00.
    assert_eq!(booking.tax_value, Some(dec!(54.00)));
    assert_eq!(booking.platform_fee, dec!(18.00));
    assert_eq!(booking.total_price, dec!(432.00));
    assert_eq!(
        booking.promo_terms.as_ref().map(|t| t.0.code.as_str()),
        Some("SUMMER")
    );
}

#[tokio::test]
async fn no_code_or_a_blank_code_prices_as_before() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let villa = listing(&pool, host).await;
    let app = app!(pool);

    for body in [|g, v| booking_request(g, v), |g, v| with_code(g, v, "   ")] {
        let guest = user(&pool, UserRole::Booker).await;
        assert_eq!(post_hold!(app, guest, body(guest, villa)).status(), 201);
        let booking = booking_of(&pool, guest).await.expect("hold created");
        assert_eq!(booking.promo_code_id, None);
        assert_eq!(booking.total_price, dec!(480.00));
        // Free the dates for the next guest.
        sqlx::query("UPDATE booking SET status = 'cancelled' WHERE id = $1")
            .bind(booking.id)
            .execute(&pool)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn refused_codes_return_their_reason_and_create_no_hold() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let guest = user(&pool, UserRole::Booker).await;
    let villa = listing(&pool, host).await;
    let other_villa = listing(&pool, host).await;

    let deactivated = code(&pool, host, "GONE").await;
    sqlx::query("UPDATE promo_code SET deactivated_at = NOW(), deactivated_by = $2 WHERE id = $1")
        .bind(deactivated)
        .bind(host)
        .execute(&pool)
        .await
        .unwrap();
    let early = code(&pool, host, "EARLY").await;
    let late = code(&pool, host, "LATE").await;
    let elsewhere = code(&pool, host, "ELSEWHERE").await;
    let long_only = code(&pool, host, "LONGONLY").await;
    let winter = code(&pool, host, "WINTER").await;
    for (id, change) in [
        (
            early,
            "valid_from = NOW() + INTERVAL '1 day', valid_to = NOW() + INTERVAL '2 days'",
        ),
        (
            late,
            "valid_from = NOW() - INTERVAL '2 days', valid_to = NOW() - INTERVAL '1 day'",
        ),
        (elsewhere, "applies_to_all = FALSE"),
        (long_only, "min_nights = 30"),
        (winter, "stay_from = '2031-01-01'"),
    ] {
        sqlx::query(&format!("UPDATE promo_code SET {change} WHERE id = $1"))
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO promo_code_listing (promo_code_id, listing_id) VALUES ($1, $2)")
        .bind(elsewhere)
        .bind(other_villa)
        .execute(&pool)
        .await
        .unwrap();

    let app = app!(pool);
    let too_long = "A".repeat(33);
    for (typed, expected) in [
        ("SUM_MER", "PROMO_NOT_FOUND"),
        ("abc", "PROMO_NOT_FOUND"),
        (too_long.as_str(), "PROMO_NOT_FOUND"),
        ("UNKNOWN", "PROMO_NOT_FOUND"),
        ("GONE", "PROMO_DEACTIVATED"),
        ("EARLY", "PROMO_NOT_YET_VALID"),
        ("LATE", "PROMO_EXPIRED"),
        ("ELSEWHERE", "PROMO_LISTING_NOT_ELIGIBLE"),
        ("LONGONLY", "PROMO_MIN_NIGHTS_NOT_MET"),
        ("WINTER", "PROMO_STAY_DATES_NOT_ELIGIBLE"),
    ] {
        let resp = post_hold!(app, guest, with_code(guest, villa, typed));
        assert_eq!(resp.status(), 422, "{typed}");
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["code"], expected, "{typed}");
    }
    assert!(booking_of(&pool, guest).await.is_none());
}

#[tokio::test]
async fn a_code_in_another_currency_uses_its_own_rate_and_keeps_it() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let guest = user(&pool, UserRole::Booker).await;
    let villa = listing(&pool, host).await;
    sqlx::query("INSERT INTO currencies (code, name) VALUES ('XTS', 'Test')")
        .execute(&pool)
        .await
        .unwrap();
    let id = code(&pool, host, "FIXEDXTS").await;
    sqlx::query(
        "UPDATE promo_code SET discount_type = 'fixed', discount_value = 25, currency = 'XTS' WHERE id = $1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    let app = app!(pool);

    // No XTS -> USD pair yet.
    let resp = post_hold!(app, guest, with_code(guest, villa, "FIXEDXTS"));
    assert_eq!(resp.status(), 422);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "FX_RATE_UNAVAILABLE");
    assert!(booking_of(&pool, guest).await.is_none());

    sqlx::query(
        "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
         VALUES ('XTS', 'USD', 2, NOW())",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        post_hold!(app, guest, with_code(guest, villa, "FIXEDXTS")).status(),
        201
    );
    let booking = booking_of(&pool, guest).await.expect("hold created");
    assert_eq!(booking.promo_stay_discount, dec!(50.00));
    assert_eq!(
        booking.promo_terms.as_ref().map(|t| t.0.exchange_rate),
        Some(dec!(2.000000))
    );

    // A new rate and a retired currency don't change the existing booking.
    sqlx::query(
        "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
         VALUES ('XTS', 'USD', 9, NOW() + INTERVAL '1 second')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE currencies SET active = FALSE WHERE code = 'XTS'")
        .execute(&pool)
        .await
        .unwrap();
    let again = booking_of(&pool, guest).await.expect("hold kept");
    assert_eq!(again.promo_stay_discount, dec!(50.00));
    assert_eq!(again.total_price, booking.total_price);

    // New holds can't use a code whose currency was deactivated.
    let other = user(&pool, UserRole::Booker).await;
    let other_villa = listing(&pool, host).await;
    let resp = post_hold!(app, other, with_code(other, other_villa, "FIXEDXTS"));
    assert_eq!(resp.status(), 422);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "FX_RATE_UNAVAILABLE");
}

#[tokio::test]
async fn stays_over_the_limit_are_refused_before_pricing() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let guest = user(&pool, UserRole::Booker).await;
    let villa = listing(&pool, host).await;
    let app = app!(pool);

    let mut body = booking_request(guest, villa);
    body["check_out"] = serde_json::json!("9999-12-31");
    let resp = post_hold!(app, guest, body);
    assert_eq!(resp.status(), 400);
    assert!(booking_of(&pool, guest).await.is_none());
}

#[tokio::test]
async fn a_non_positive_stay_limit_is_rejected_at_startup() {
    let mut application = settings().application;
    assert!(application.validate_max_stay_nights().is_ok());
    for bad in [0, -1] {
        application.max_stay_nights = bad;
        assert!(application.validate_max_stay_nights().is_err(), "{bad}");
    }
}

/// Captures everything a subscriber sees, including span fields.
#[derive(Clone, Default)]
pub(super) struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for CapturedLogs {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("log buffer").extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedLogs {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

impl CapturedLogs {
    pub(super) fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("log buffer")).into_owned()
    }
}

pub(super) fn capture_logs() -> (CapturedLogs, tracing::subscriber::DefaultGuard) {
    let logs = CapturedLogs::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_span_events(
            tracing_subscriber::fmt::format::FmtSpan::NEW
                | tracing_subscriber::fmt::format::FmtSpan::CLOSE,
        )
        .with_ansi(false)
        .with_writer(logs.clone())
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    // Other test threads have no subscriber, and `tracing` caches that a log line
    // is disabled globally. Without this, this test can miss lines at random.
    tracing::callsite::rebuild_interest_cache();
    (logs, guard)
}

#[tokio::test]
async fn plain_codes_reach_no_log_or_span_field() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let guest = user(&pool, UserRole::Booker).await;
    let other = user(&pool, UserRole::Booker).await;
    let villa = listing(&pool, host).await;
    let other_villa = listing(&pool, host).await;
    let id = code(&pool, host, "SECRETCODE").await;
    sqlx::query("UPDATE promo_code SET total_usage_limit = 1 WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let app = app!(pool);

    let (logs, guard) = capture_logs();
    // A successful hold, then a refusal under the code's lock, then a guess.
    assert_eq!(
        post_hold!(app, guest, with_code(guest, villa, "SECRETCODE")).status(),
        201
    );
    assert_eq!(
        post_hold!(app, other, with_code(other, other_villa, "SECRETCODE")).status(),
        422
    );
    assert_eq!(
        post_hold!(app, other, with_code(other, other_villa, "GUESSEDCODE")).status(),
        422
    );
    drop(guard);

    let text = logs.text();
    assert!(text.contains("promo code refused"), "refusals are logged");
    assert!(text.contains("code_hash"), "with a hash");
    assert!(text.contains("PROMO_USAGE_LIMIT_REACHED"));
    assert!(text.contains(&other.to_string()), "and the guest");
    assert!(!text.contains("SECRETCODE"), "{text}");
    assert!(!text.contains("GUESSEDCODE"), "{text}");
    assert!(
        !text.contains("failed to create booking"),
        "refusals aren't logged as failures: {text}"
    );
    assert!(
        !text
            .lines()
            .any(|line| line.contains(" ERROR ") && line.contains("promo")),
        "refusals aren't logged as errors: {text}"
    );
}

mod quote {
    use super::*;

    macro_rules! post_quote {
        ($app:expr, $guest:expr, $body:expr) => {
            test::call_service(
                &$app,
                test::TestRequest::post()
                    .uri("/api/v1/bookings/quote")
                    .insert_header(("Accept", "application/json"))
                    .insert_header(bearer($guest))
                    .set_json($body)
                    .to_request(),
            )
            .await
        };
    }

    fn quote_body(villa: Uuid, value: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "listing_id": villa,
            "check_in": "2030-08-01",
            "check_out": "2030-08-03",
            "guests": 1,
            "currency": "USD",
            "promo_code": value,
        })
    }

    fn amount(v: &serde_json::Value) -> Decimal {
        v.as_str()
            .map(str::to_string)
            .unwrap_or_else(|| v.to_string())
            .parse()
            .expect("an amount")
    }

    fn rows_add_up(q: &serde_json::Value) {
        let promo = if q["promo"].is_null() {
            Decimal::ZERO
        } else {
            amount(&q["promo"]["discount"])
        };
        assert_eq!(
            amount(&q["subtotal"]) - amount(&q["stay_discount"]) - promo
                + amount(&q["cleaning_fee"])
                + amount(&q["tax_amount"])
                + amount(&q["platform_fee"])
                + amount(&q["damage_deposit"]),
            amount(&q["total_amount"]),
            "{q}"
        );
    }

    #[tokio::test]
    async fn public_signed_in_and_hold_totals_match_with_no_code() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let villa = listing(&pool, host).await;
        sqlx::query(
            "UPDATE listing SET weekly_discount_percentage = 10, cleaning_fee = 50, damage_deposit = 100,
                 guests_included = 1, extra_guest_fee = 25 WHERE id = $1",
        )
        .bind(villa)
        .execute(&pool)
        .await
        .unwrap();
        let app = app!(pool);

        let public = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!(
                    "/api/v1/bookings/listing/{villa}/quote?check_in=2030-08-01&check_out=2030-08-08&guests=2&currency=USD"
                ))
                .to_request(),
        )
        .await;
        assert_eq!(public.status(), 200, "the public quote needs no sign-in");
        let public: serde_json::Value = test::read_body_json(public).await;
        rows_add_up(&public);

        let mut body = quote_body(villa, None);
        body["check_out"] = serde_json::json!("2030-08-08");
        body["guests"] = serde_json::json!(2);
        let signed_in = post_quote!(app, guest, body);
        assert_eq!(signed_in.status(), 200);
        let signed_in: serde_json::Value = test::read_body_json(signed_in).await;
        assert_eq!(public, signed_in);

        let mut hold = booking_request(guest, villa);
        hold["check_out"] = serde_json::json!("2030-08-08");
        hold["num_adults"] = serde_json::json!(2);
        assert_eq!(post_hold!(app, guest, hold).status(), 201);
        let booking = booking_of(&pool, guest).await.expect("hold created");
        assert_eq!(booking.total_price, amount(&public["total_amount"]));
    }

    #[tokio::test]
    async fn a_quote_with_a_code_matches_the_hold_and_its_rows_add_up() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let villa = listing(&pool, host).await;
        sqlx::query("UPDATE listing SET cleaning_fee = 100 WHERE id = $1")
            .bind(villa)
            .execute(&pool)
            .await
            .unwrap();
        for (value, change) in [
            ("BOTHPCT", "scope = 'stay_and_cleaning'"),
            ("STAYPCT", "scope = 'stay'"),
            (
                "BOTHFIXED",
                "scope = 'stay_and_cleaning', discount_type = 'fixed', discount_value = 450, currency = 'USD'",
            ),
            (
                "STAYFIXED",
                "scope = 'stay', discount_type = 'fixed', discount_value = 75, currency = 'USD'",
            ),
        ] {
            let id = code(&pool, host, value).await;
            sqlx::query(&format!("UPDATE promo_code SET {change} WHERE id = $1"))
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        let app = app!(pool);

        for value in ["BOTHPCT", "STAYPCT", "BOTHFIXED", "STAYFIXED"] {
            let resp = post_quote!(app, guest, quote_body(villa, Some(value)));
            assert_eq!(resp.status(), 200, "{value}");
            let q: serde_json::Value = test::read_body_json(resp).await;
            assert_eq!(q["promo"]["code"], value);
            assert!(q["promo_refusal"].is_null());
            rows_add_up(&q);

            assert_eq!(
                post_hold!(app, guest, with_code(guest, villa, value)).status(),
                201
            );
            let total: Decimal = sqlx::query_scalar(
                "SELECT total_price FROM booking WHERE guest_id = $1 AND status = 'pending'",
            )
            .bind(guest)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(total, amount(&q["total_amount"]), "{value}");
        }
    }

    #[tokio::test]
    async fn a_refused_code_returns_the_full_price_and_the_reason() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let villa = listing(&pool, host).await;
        let app = app!(pool);

        let resp = post_quote!(app, guest, quote_body(villa, Some("NOSUCHCODE")));
        assert_eq!(resp.status(), 200);
        let q: serde_json::Value = test::read_body_json(resp).await;
        assert!(q["promo"].is_null());
        assert_eq!(q["promo_refusal"]["code"], "PROMO_NOT_FOUND");
        assert!(q["promo_refusal"]["message"].is_string());
        assert_eq!(amount(&q["total_amount"]), dec!(480.00));
        rows_add_up(&q);
    }

    #[tokio::test]
    async fn a_missing_rate_for_the_code_is_reported_in_the_quote() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let villa = listing(&pool, host).await;
        sqlx::query("INSERT INTO currencies (code, name) VALUES ('XTS', 'Test')")
            .execute(&pool)
            .await
            .unwrap();
        let id = code(&pool, host, "NORATE").await;
        sqlx::query(
            "UPDATE promo_code SET discount_type = 'fixed', discount_value = 5, currency = 'XTS' WHERE id = $1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let app = app!(pool);

        let resp = post_quote!(app, guest, quote_body(villa, Some("NORATE")));
        assert_eq!(resp.status(), 200);
        let q: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(q["promo_refusal"]["code"], "FX_RATE_UNAVAILABLE");
        assert_eq!(amount(&q["total_amount"]), dec!(480.00));
    }

    #[tokio::test]
    async fn after_a_guests_own_hold_the_quote_still_gives_the_discount() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let villa = listing(&pool, host).await;
        let id = code(&pool, host, "ONEUSE").await;
        sqlx::query(
            "UPDATE promo_code SET total_usage_limit = 1, per_guest_limit = 1 WHERE id = $1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let app = app!(pool);

        assert_eq!(
            post_hold!(app, guest, with_code(guest, villa, "ONEUSE")).status(),
            201
        );

        let resp = post_quote!(app, guest, quote_body(villa, Some("ONEUSE")));
        let q: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(q["promo"]["code"], "ONEUSE", "{q}");

        assert_eq!(
            post_hold!(app, guest, with_code(guest, villa, "ONEUSE")).status(),
            201
        );
        let total: Decimal = sqlx::query_scalar(
            "SELECT total_price FROM booking WHERE guest_id = $1 AND status = 'pending'",
        )
        .bind(guest)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(total, amount(&q["total_amount"]));

        // Another guest is refused while that hold is live.
        let other = user(&pool, UserRole::Booker).await;
        let resp = post_quote!(app, other, quote_body(villa, Some("ONEUSE")));
        let q: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(q["promo_refusal"]["code"], "PROMO_USAGE_LIMIT_REACHED");
    }

    #[tokio::test]
    async fn a_code_deactivated_after_the_quote_refuses_the_hold_and_keeps_the_earlier_one() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let villa = listing(&pool, host).await;
        let id = code(&pool, host, "SHORTLIVED").await;
        let app = app!(pool);

        assert_eq!(
            post_hold!(app, guest, booking_request(guest, villa)).status(),
            201
        );
        let earlier = booking_of(&pool, guest).await.expect("earlier hold");

        let resp = post_quote!(app, guest, quote_body(villa, Some("SHORTLIVED")));
        let q: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(q["promo"]["code"], "SHORTLIVED");

        sqlx::query(
            "UPDATE promo_code SET deactivated_at = NOW(), deactivated_by = $2 WHERE id = $1",
        )
        .bind(id)
        .bind(host)
        .execute(&pool)
        .await
        .unwrap();

        let resp = post_hold!(app, guest, with_code(guest, villa, "SHORTLIVED"));
        assert_eq!(resp.status(), 422);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["code"], "PROMO_DEACTIVATED");

        let rows: Vec<(Uuid, String)> =
            sqlx::query_as("SELECT id, status::text FROM booking WHERE guest_id = $1")
                .bind(guest)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(rows, vec![(earlier.id, "pending".to_string())]);
    }

    /// A code failing an early and a late check gives the earlier reason.
    #[tokio::test]
    async fn refusals_follow_the_documented_order_across_groups() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let villa = listing(&pool, host).await;
        sqlx::query("INSERT INTO currencies (code, name) VALUES ('XTS', 'Test')")
            .execute(&pool)
            .await
            .unwrap();

        // 4 vs 7: expired beats minimum nights.
        let a = code(&pool, host, "ORDERA").await;
        // 6 vs 7: a missing rate beats minimum nights.
        let b = code(&pool, host, "ORDERB").await;
        // 9 vs 10: minimum value beats the usage limit.
        let c = code(&pool, host, "ORDERC").await;
        for (id, change) in [
            (
                a,
                "valid_from = NOW() - INTERVAL '2 days', valid_to = NOW() - INTERVAL '1 day', min_nights = 30",
            ),
            (
                b,
                "discount_type = 'fixed', discount_value = 5, currency = 'XTS', min_nights = 30",
            ),
            (
                c,
                "currency = 'USD', min_booking_value = 100000, total_usage_limit = 1",
            ),
        ] {
            sqlx::query(&format!("UPDATE promo_code SET {change} WHERE id = $1"))
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        let earlier = raw_redeemed_booking(&pool, host, c).await;
        assert!(!earlier.is_nil());
        let app = app!(pool);

        for (value, expected) in [
            ("ORDERA", "PROMO_EXPIRED"),
            ("ORDERB", "FX_RATE_UNAVAILABLE"),
            ("ORDERC", "PROMO_MIN_VALUE_NOT_MET"),
        ] {
            let resp = post_quote!(app, guest, quote_body(villa, Some(value)));
            let q: serde_json::Value = test::read_body_json(resp).await;
            assert_eq!(q["promo_refusal"]["code"], expected, "{value}");
        }
    }

    /// A confirmed booking redeeming `code`, using up a use.
    async fn raw_redeemed_booking(pool: &PgPool, host: Uuid, promo: Uuid) -> Uuid {
        let other_guest = user(pool, UserRole::Booker).await;
        let other_villa = listing(pool, host).await;
        let booking: Uuid = sqlx::query_scalar(
            "INSERT INTO booking (confirmation_code, guest_id, listing_id, date_from, date_to, daily_rate,
                 number_of_persons, total_days, sub_total_price, total_price, cancellation_policy, status)
             VALUES (upper(substr(md5(random()::text), 1, 10)), $1, $2, '2030-01-01', '2030-01-03', 100,
                 1, 2, 200, 200, 'flexible', 'confirmed')
             RETURNING id",
        )
        .bind(other_guest)
        .bind(other_villa)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO promo_code_redemption (promo_code_id, booking_id) VALUES ($1, $2)",
        )
        .bind(promo)
        .bind(booking)
        .execute(pool)
        .await
        .unwrap();
        booking
    }

    #[tokio::test]
    async fn absurd_date_ranges_are_refused_everywhere_but_long_stays_work() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let villa = listing(&pool, host).await;
        let app = app!(pool);

        for check_out in ["9999-12-31", "+262000-01-01"] {
            let encoded = check_out.replace('+', "%2B");
            let public = test::call_service(
                &app,
                test::TestRequest::get()
                    .uri(&format!(
                        "/api/v1/bookings/listing/{villa}/quote?check_in=2030-08-01&check_out={encoded}"
                    ))
                    .to_request(),
            )
            .await;
            assert_eq!(public.status(), 400, "{check_out}");
            let mut body = quote_body(villa, None);
            body["check_out"] = serde_json::json!(check_out);
            assert_eq!(
                post_quote!(app, guest, body.clone()).status(),
                400,
                "{check_out}"
            );
            let mut hold = booking_request(guest, villa);
            hold["check_out"] = serde_json::json!(check_out);
            assert_eq!(post_hold!(app, guest, hold).status(), 400, "{check_out}");
        }

        // 400 nights: over a year, and fine.
        let mut body = quote_body(villa, None);
        body["check_out"] = serde_json::json!("2031-09-05");
        let resp = post_quote!(app, guest, body);
        assert_eq!(resp.status(), 200);
        let q: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(q["nights"], 400);
        let mut hold = booking_request(guest, villa);
        hold["check_out"] = serde_json::json!("2031-09-05");
        assert_eq!(post_hold!(app, guest, hold).status(), 201);

        // One night over the limit.
        let mut body = quote_body(villa, None);
        let limit = api_core::settings::DEFAULT_MAX_STAY_NIGHTS + 1;
        let over = chrono::NaiveDate::from_ymd_opt(2030, 8, 1).expect("valid date")
            + chrono::Duration::days(limit);
        body["check_out"] = serde_json::json!(over.to_string());
        assert_eq!(post_quote!(app, guest, body).status(), 400);
    }

    #[tokio::test]
    async fn amounts_too_large_to_store_get_a_400_not_a_500() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let villa = listing(&pool, host).await;
        sqlx::query("UPDATE listing SET price_per_night = 90000 WHERE id = $1")
            .bind(villa)
            .execute(&pool)
            .await
            .unwrap();
        let all = code(&pool, host, "HIDESIT").await;
        sqlx::query("UPDATE promo_code SET discount_value = 100 WHERE id = $1")
            .bind(all)
            .execute(&pool)
            .await
            .unwrap();
        let app = app!(pool);
        let check_out = (chrono::NaiveDate::from_ymd_opt(2030, 8, 1).expect("valid date")
            + chrono::Duration::days(3000))
        .to_string();

        let public = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!(
                    "/api/v1/bookings/listing/{villa}/quote?check_in=2030-08-01&check_out={check_out}"
                ))
                .to_request(),
        )
        .await;
        assert_eq!(public.status(), 400);

        // A 100% code still has a gross subtotal to store.
        let mut body = quote_body(villa, Some("HIDESIT"));
        body["check_out"] = serde_json::json!(check_out);
        assert_eq!(post_quote!(app, guest, body).status(), 400);
        let mut hold = with_code(guest, villa, "HIDESIT");
        hold["check_out"] = serde_json::json!(check_out);
        assert_eq!(post_hold!(app, guest, hold).status(), 400);
    }

    #[tokio::test]
    async fn the_public_quote_ignores_any_code_and_the_signed_in_quote_needs_sign_in() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let villa = listing(&pool, host).await;
        code(&pool, host, "FORGUESTS").await;
        let app = app!(pool);

        let public = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!(
                    "/api/v1/bookings/listing/{villa}/quote?check_in=2030-08-01&check_out=2030-08-03&promo_code=FORGUESTS"
                ))
                .to_request(),
        )
        .await;
        assert_eq!(public.status(), 200);
        let q: serde_json::Value = test::read_body_json(public).await;
        assert!(q["promo"].is_null());
        assert!(q["promo_refusal"].is_null());
        assert_eq!(amount(&q["total_amount"]), dec!(480.00));

        let anonymous = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/bookings/quote")
                .insert_header(("Accept", "application/json"))
                .set_json(quote_body(villa, Some("FORGUESTS")))
                .to_request(),
        )
        .await;
        assert_eq!(anonymous.status(), 401);
    }

    #[tokio::test]
    async fn a_deactivated_account_cant_get_a_signed_in_quote() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let villa = listing(&pool, host).await;
        sqlx::query(r#"UPDATE "user" SET is_active = FALSE WHERE id = $1"#)
            .bind(guest)
            .execute(&pool)
            .await
            .unwrap();
        let app = app!(pool);

        let resp = post_quote!(app, guest, quote_body(villa, None));
        assert!(
            resp.status().is_client_error() && resp.status() != 200,
            "got {}",
            resp.status()
        );
    }

    #[tokio::test]
    async fn too_many_guests_gets_the_same_error_as_the_hold() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let villa = listing(&pool, host).await;
        let app = app!(pool);

        let mut body = quote_body(villa, None);
        body["guests"] = serde_json::json!(99);
        let quote = post_quote!(app, guest, body);
        let quote_status = quote.status();
        let quote_body_json: serde_json::Value = test::read_body_json(quote).await;

        let mut hold = booking_request(guest, villa);
        hold["num_adults"] = serde_json::json!(99);
        let held = post_hold!(app, guest, hold);
        assert_eq!(held.status(), quote_status);
        let hold_json: serde_json::Value = test::read_body_json(held).await;
        assert_eq!(hold_json, quote_body_json);
    }

    #[tokio::test]
    async fn a_refused_quote_logs_the_guest_reason_and_hash_not_the_code() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let villa = listing(&pool, host).await;
        let app = app!(pool);

        let (logs, guard) = capture_logs();
        let resp = post_quote!(app, guest, quote_body(villa, Some("QUOTEGUESS")));
        assert_eq!(resp.status(), 200);
        drop(guard);

        let text = logs.text();
        assert!(text.contains("promo code refused"), "{text}");
        assert!(text.contains(&guest.to_string()), "{text}");
        assert!(text.contains("PROMO_NOT_FOUND"), "{text}");
        assert!(text.contains("code_hash"), "{text}");
        assert!(!text.contains("QUOTEGUESS"), "{text}");
    }
}

mod promo_reprice {
    use super::*;

    /// A confirmed promo booking for 2 nights at 200.00 with 10% off the stay.
    async fn confirmed(pool: &PgPool, value: &str, change: &str) -> (Uuid, Uuid, Uuid) {
        let host = user(pool, UserRole::Host).await;
        let guest = user(pool, UserRole::Booker).await;
        let admin = user(pool, UserRole::Admin).await;
        let villa = listing(pool, host).await;
        let id = code(pool, host, value).await;
        if !change.is_empty() {
            sqlx::query(&format!("UPDATE promo_code SET {change} WHERE id = $1"))
                .bind(id)
                .execute(pool)
                .await
                .unwrap();
        }
        let app = app!(pool);
        assert_eq!(
            post_hold!(app, guest, with_code(guest, villa, value)).status(),
            201
        );
        let booking = booking_of(pool, guest).await.expect("hold created");
        let resp = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri(&format!("/api/v1/bookings/{}", booking.id))
                .insert_header(("Accept", "application/json"))
                .insert_header(bearer(admin))
                .set_json(serde_json::json!({ "status": "confirmed" }))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);
        (booking.id, admin, id)
    }

    macro_rules! patch_dates {
        ($app:expr, $admin:expr, $id:expr, $date_to:expr) => {
            test::call_service(
                &$app,
                test::TestRequest::patch()
                    .uri(&format!("/api/v1/bookings/{}", $id))
                    .insert_header(("Accept", "application/json"))
                    .insert_header(bearer($admin))
                    .set_json(serde_json::json!({ "date_to": $date_to }))
                    .to_request(),
            )
            .await
        };
    }

    #[tokio::test]
    async fn a_longer_stay_recalculates_a_percentage_even_after_deactivation() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let (id, admin, code_id) = confirmed(&pool, "KEPTPCT", "").await;
        sqlx::query(
            "UPDATE promo_code SET deactivated_at = NOW(), deactivated_by = created_by,
                 valid_from = NOW() - INTERVAL '2 days', valid_to = NOW() - INTERVAL '1 day',
                 total_usage_limit = 1 WHERE id = $1",
        )
        .bind(code_id)
        .execute(&pool)
        .await
        .unwrap();
        let app = app!(pool);

        let resp = patch_dates!(app, admin, id, "2030-08-05");
        assert_eq!(resp.status(), 200);

        let booking = db_core::booking::get_booking_by_id(&pool, id)
            .await
            .unwrap();
        assert_eq!(booking.promo_code_id, Some(code_id));
        assert_eq!(booking.sub_total_price, dec!(800.00));
        assert_eq!(booking.promo_stay_discount, dec!(80.00));
        assert_eq!(booking.accommodation_net(), dec!(720.00));
        // 720.00 + 15% GCT + 5% platform fee.
        assert_eq!(booking.total_price, dec!(864.00));
    }

    #[tokio::test]
    async fn a_fixed_discount_stays_and_is_capped_when_the_stay_shrinks() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let (id, admin, _) = confirmed(
            &pool,
            "KEPTFIXED",
            "discount_type = 'fixed', discount_value = 250, currency = 'USD'",
        )
        .await;
        let app = app!(pool);

        let before = db_core::booking::get_booking_by_id(&pool, id)
            .await
            .unwrap();
        assert_eq!(before.promo_stay_discount, dec!(250.00));

        // Shortened to one night at 200.00.
        let resp = patch_dates!(app, admin, id, "2030-08-02");
        assert_eq!(resp.status(), 200);
        let booking = db_core::booking::get_booking_by_id(&pool, id)
            .await
            .unwrap();
        assert_eq!(booking.promo_stay_discount, dec!(200.00));
        assert_eq!(booking.accommodation_net(), Decimal::ZERO);
        assert_eq!(booking.total_price, dec!(0.00));
    }

    #[tokio::test]
    async fn a_shorter_stay_below_the_minimum_loses_the_discount_and_frees_the_use() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let (id, admin, code_id) =
            confirmed(&pool, "NEEDSTWO", "min_nights = 2, total_usage_limit = 1").await;
        let app = app!(pool);

        let resp = patch_dates!(app, admin, id, "2030-08-02");
        assert_eq!(resp.status(), 200);

        let booking = db_core::booking::get_booking_by_id(&pool, id)
            .await
            .unwrap();
        assert_eq!(booking.promo_code_id, None);
        assert_eq!(booking.promo_stay_discount, Decimal::ZERO);
        assert_eq!(booking.accommodation_net(), dec!(200.00));
        assert_eq!(booking.total_price, dec!(240.00));

        let mut conn = pool.acquire().await.unwrap();
        let (_, usage) = db_core::promo::lock_and_count(&mut conn, code_id, Uuid::now_v7())
            .await
            .unwrap();
        assert_eq!(usage.total_used, 0);
    }

    #[tokio::test]
    async fn a_captured_update_does_not_log_the_code() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let (id, admin, _) = confirmed(&pool, "UPDATESECRET", "").await;
        let app = app!(pool);

        let (logs, guard) = capture_logs();
        let resp = patch_dates!(app, admin, id, "2030-08-05");
        assert_eq!(resp.status(), 200);
        drop(guard);

        let text = logs.text();
        assert!(
            text.contains("update_booking"),
            "the span was captured: {text}"
        );
        assert!(!text.contains("UPDATESECRET"), "{text}");
    }
}

mod promo_change_quote {
    use super::*;

    /// A confirmed 2-night promo booking at 200.00 with a 100.00 cleaning fee.
    async fn confirmed(pool: &PgPool, value: &str, change: &str) -> (Uuid, Uuid, Uuid) {
        let host = user(pool, UserRole::Host).await;
        let guest = user(pool, UserRole::Booker).await;
        let admin = user(pool, UserRole::Admin).await;
        let villa = listing(pool, host).await;
        sqlx::query("UPDATE listing SET cleaning_fee = 100 WHERE id = $1")
            .bind(villa)
            .execute(pool)
            .await
            .unwrap();
        let id = code(pool, host, value).await;
        sqlx::query(&format!(
            "UPDATE promo_code SET scope = 'stay_and_cleaning' {change} WHERE id = $1"
        ))
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
        let app = app!(pool);
        assert_eq!(
            post_hold!(app, guest, with_code(guest, villa, value)).status(),
            201
        );
        let booking = booking_of(pool, guest).await.expect("hold created");
        let resp = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri(&format!("/api/v1/bookings/{}", booking.id))
                .insert_header(("Accept", "application/json"))
                .insert_header(bearer(admin))
                .set_json(serde_json::json!({ "status": "confirmed" }))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);
        (booking.id, guest, host)
    }

    macro_rules! change {
        ($app:expr, $who:expr, $id:expr, $path:expr, $date_to:expr) => {
            test::call_service(
                &$app,
                test::TestRequest::post()
                    .uri(&format!("/api/v1/bookings/{}/{}", $id, $path))
                    .insert_header(("Accept", "application/json"))
                    .insert_header(bearer($who))
                    .set_json(serde_json::json!({ "date_to": $date_to }))
                    .to_request(),
            )
            .await
        };
    }

    fn amount(v: &serde_json::Value) -> Decimal {
        v.as_str()
            .map(str::to_string)
            .unwrap_or_else(|| v.to_string())
            .parse()
            .expect("an amount")
    }

    #[tokio::test]
    async fn a_kept_promo_is_reported_and_approval_writes_the_quote() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let (id, guest, host) = confirmed(&pool, "KEPT", "").await;
        let app = app!(pool);

        let resp = change!(app, guest, id, "change-quote", "2030-08-04");
        assert_eq!(resp.status(), 200);
        let quote: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(quote["promo_outcome"]["status"], "kept");
        assert_eq!(quote["promo_outcome"]["code"], "KEPT");
        let snap = &quote["pricing_snapshot"];
        assert_eq!(snap["promo_retained"], true);
        assert!(snap["promo_lost_reason"].is_null());
        assert_eq!(amount(&snap["promo_stay_discount"]), dec!(60.00));
        assert_eq!(amount(&snap["promo_cleaning_discount"]), dec!(10.00));
        // 540.00 + 90.00 taxable, at 15%, plus a 5% platform fee on 540.00.
        assert_eq!(amount(&snap["total_price"]), dec!(751.50));
        // Commission is 10% of the (540.00 + 90.00) gross.
        assert_eq!(amount(&quote["proposed_net_payout"]), dec!(567.00));

        let resp = change!(app, guest, id, "change-requests", "2030-08-04");
        assert_eq!(resp.status(), 201);
        let request: serde_json::Value = test::read_body_json(resp).await;
        let request_id = request["id"].as_str().expect("request id").to_string();

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!(
                    "/api/v1/bookings/{id}/change-requests/{request_id}/approve"
                ))
                .insert_header(("Accept", "application/json"))
                .insert_header(bearer(host))
                .set_json(serde_json::json!({}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);

        let booking = db_core::booking::get_booking_by_id(&pool, id)
            .await
            .unwrap();
        assert_eq!(booking.promo_stay_discount, dec!(60.00));
        assert_eq!(booking.promo_cleaning_discount, dec!(10.00));
        assert_eq!(booking.total_price, dec!(751.50));
        let net: Decimal = sqlx::query_scalar(
            "SELECT net_payout_amount FROM host_payout_ledger WHERE booking_id = $1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(net, dec!(567.00), "approval writes what the quote promised");
    }

    #[tokio::test]
    async fn a_lost_promo_is_reported_with_its_reason_and_prices_at_full_rate() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let (id, guest, _) = confirmed(&pool, "LOSTONE", ", min_nights = 2").await;
        let app = app!(pool);

        let resp = change!(app, guest, id, "change-quote", "2030-08-02");
        assert_eq!(resp.status(), 200);
        let quote: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(quote["promo_outcome"]["status"], "lost");
        assert_eq!(
            quote["promo_outcome"]["reason_code"],
            "PROMO_MIN_NIGHTS_NOT_MET"
        );
        assert!(quote["promo_outcome"]["reason"].is_string());
        let snap = &quote["pricing_snapshot"];
        assert_eq!(snap["promo_retained"], false);
        assert!(snap["promo_lost_reason"].is_string());
        assert_eq!(amount(&snap["promo_stay_discount"]), Decimal::ZERO);
        // 200.00 + 100.00 taxable at 15%, plus the platform fee on 200.00.
        assert_eq!(amount(&snap["total_price"]), dec!(355.00));

        // A host's pending request carries the reason for the guest's banner.
        let resp = change!(app, guest, id, "change-requests", "2030-08-02");
        assert_eq!(resp.status(), 201);
        let request: serde_json::Value = test::read_body_json(resp).await;
        assert!(request["pricing_snapshot"]["promo_lost_reason"].is_string());
    }

    #[tokio::test]
    async fn a_booking_without_a_promo_reports_none() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let admin = user(&pool, UserRole::Admin).await;
        let villa = listing(&pool, host).await;
        let app = app!(pool);
        assert_eq!(
            post_hold!(app, guest, booking_request(guest, villa)).status(),
            201
        );
        let booking = booking_of(&pool, guest).await.expect("hold created");
        let resp = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri(&format!("/api/v1/bookings/{}", booking.id))
                .insert_header(("Accept", "application/json"))
                .insert_header(bearer(admin))
                .set_json(serde_json::json!({ "status": "confirmed" }))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);

        let resp = change!(app, guest, booking.id, "change-quote", "2030-08-04");
        let quote: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(quote["promo_outcome"]["status"], "none");
    }

    #[tokio::test]
    async fn approval_is_refused_before_settlement_when_the_promo_was_removed() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let (id, guest, host) = confirmed(&pool, "REMOVED", "").await;
        let app = app!(pool);

        let resp = change!(app, guest, id, "change-requests", "2030-08-04");
        assert_eq!(resp.status(), 201);
        let request: serde_json::Value = test::read_body_json(resp).await;
        let request_id = request["id"].as_str().expect("request id").to_string();
        sqlx::query(
            "UPDATE booking SET promo_code_id = NULL, promo_terms = NULL, promo_stay_discount = 0,
                 promo_cleaning_discount = 0 WHERE id = $1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!(
                    "/api/v1/bookings/{id}/change-requests/{request_id}/approve"
                ))
                .insert_header(("Accept", "application/json"))
                .insert_header(bearer(host))
                .set_json(serde_json::json!({}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 409);
        let settled: String = sqlx::query_scalar(
            "SELECT settlement_status::text FROM reservation_change_request WHERE id = $1::uuid",
        )
        .bind(&request_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_ne!(settled, "charged", "the guest wasn't charged");
    }
}

/// The quote a guest sees before cancelling must match what the receipt refunds.
#[tokio::test]
async fn cancellation_quote_matches_the_refund_receipt_for_a_promo_booking() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let guest = user(&pool, UserRole::Booker).await;
    let admin = user(&pool, UserRole::Admin).await;
    let villa = listing(&pool, host).await;
    sqlx::query("UPDATE listing SET cleaning_fee = 100, damage_deposit = 200 WHERE id = $1")
        .bind(villa)
        .execute(&pool)
        .await
        .unwrap();
    let id = code(&pool, host, "REFUNDED").await;
    sqlx::query("UPDATE promo_code SET scope = 'stay_and_cleaning' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let app = app!(pool);
    assert_eq!(
        post_hold!(app, guest, with_code(guest, villa, "REFUNDED")).status(),
        201
    );
    let booking = booking_of(&pool, guest).await.expect("hold created");
    let patch = |status: &str| {
        test::TestRequest::patch()
            .uri(&format!("/api/v1/bookings/{}", booking.id))
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(admin))
            .set_json(serde_json::json!({ "status": status }))
            .to_request()
    };
    assert_eq!(
        test::call_service(&app, patch("confirmed")).await.status(),
        200
    );

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/bookings/{}/cancellation-quote",
                booking.id
            ))
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(guest))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let quote: serde_json::Value = test::read_body_json(resp).await;
    let amount = |v: &serde_json::Value| -> Decimal {
        v.as_str()
            .map(str::to_string)
            .unwrap_or_else(|| v.to_string())
            .parse()
            .expect("an amount")
    };
    assert!(amount(&quote["retained_total"]) >= Decimal::ZERO, "{quote}");

    assert_eq!(
        test::call_service(&app, patch("cancelled")).await.status(),
        200
    );
    let docs = db_core::tax_document::list_for_booking(&pool, booking.id)
        .await
        .unwrap();
    let refund = docs
        .iter()
        .find(|d| d.document_type == db_core::models::TaxDocumentType::RefundReceipt)
        .expect("a refund receipt");
    assert_eq!(amount(&quote["refund_total"]), refund.total_amount);
    assert_eq!(refund.total_amount, booking.total_price);
}

mod promo_booking_response {
    use super::*;

    #[tokio::test]
    async fn responses_carry_the_code_and_discount_and_convert_for_display() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let villa = listing(&pool, host).await;
        code(&pool, host, "RESPONSE").await;
        let app = app!(pool);

        let resp = post_hold!(app, guest, with_code(guest, villa, "RESPONSE"));
        assert_eq!(resp.status(), 201);
        let created: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(created["promo_code"], "RESPONSE");
        let amount = |v: &serde_json::Value| -> Decimal {
            v.as_str()
                .map(str::to_string)
                .unwrap_or_else(|| v.to_string())
                .parse()
                .expect("an amount")
        };
        assert_eq!(amount(&created["promo_discount"]), dec!(40.00));

        // Hosts see the code on their own bookings too, including admin-created ones.
        let id = created["id"].as_str().expect("id");
        for who in [guest, host] {
            let read = test::call_service(
                &app,
                test::TestRequest::get()
                    .uri(&format!("/api/v1/bookings/{id}"))
                    .insert_header(("Accept", "application/json"))
                    .insert_header(bearer(who))
                    .to_request(),
            )
            .await;
            assert_eq!(read.status(), 200);
            let read: serde_json::Value = test::read_body_json(read).await;
            assert_eq!(read["promo_code"], "RESPONSE");
        }

        // The seeded USD -> JMD rate is 160.
        let converted = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/api/v1/bookings/{id}?currency=JMD"))
                .insert_header(("Accept", "application/json"))
                .insert_header(bearer(guest))
                .to_request(),
        )
        .await;
        assert_eq!(converted.status(), 200);
        let converted: serde_json::Value = test::read_body_json(converted).await;
        assert_eq!(converted["currency"], "JMD");
        assert_eq!(amount(&converted["promo_discount"]), dec!(6400.00));
    }

    #[tokio::test]
    async fn a_booking_without_a_promo_has_none() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let villa = listing(&pool, host).await;
        let app = app!(pool);
        let resp = post_hold!(app, guest, booking_request(guest, villa));
        let created: serde_json::Value = test::read_body_json(resp).await;
        assert!(created["promo_code"].is_null());
    }
}

mod agreement {
    use super::*;

    /// One promo booking's discount, net amounts and tax agree everywhere they
    /// appear, and later changes to the code or the rate leave them alone.
    #[tokio::test]
    async fn figures_agree_across_every_surface_and_later_changes_leave_them_alone() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let guest = user(&pool, UserRole::Booker).await;
        let admin = user(&pool, UserRole::Admin).await;
        let villa = listing(&pool, host).await;
        sqlx::query(
            "UPDATE listing SET cleaning_fee = 100, damage_deposit = 200, weekly_discount_percentage = 10
             WHERE id = $1",
        )
        .bind(villa)
        .execute(&pool)
        .await
        .unwrap();
        let code_id = code(&pool, host, "AGREEMENT").await;
        sqlx::query("UPDATE promo_code SET scope = 'stay_and_cleaning' WHERE id = $1")
            .bind(code_id)
            .execute(&pool)
            .await
            .unwrap();
        let app = app!(pool);
        let amount = |v: &serde_json::Value| -> Decimal {
            v.as_str()
                .map(str::to_string)
                .unwrap_or_else(|| v.to_string())
                .parse()
                .expect("an amount")
        };

        // Quote.
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/bookings/quote")
                .insert_header(("Accept", "application/json"))
                .insert_header(bearer(guest))
                .set_json(serde_json::json!({
                    "listing_id": villa,
                    "check_in": "2030-08-01",
                    "check_out": "2030-08-08",
                    "guests": 1,
                    "currency": "USD",
                    "promo_code": "AGREEMENT",
                }))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);
        let quote: serde_json::Value = test::read_body_json(resp).await;

        // Booking.
        let mut hold = with_code(guest, villa, "AGREEMENT");
        hold["check_out"] = serde_json::json!("2030-08-08");
        assert_eq!(post_hold!(app, guest, hold).status(), 201);
        let held = booking_of(&pool, guest).await.expect("hold created");
        let resp = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri(&format!("/api/v1/bookings/{}", held.id))
                .insert_header(("Accept", "application/json"))
                .insert_header(bearer(admin))
                .set_json(serde_json::json!({ "status": "confirmed" }))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);
        let booking = db_core::booking::get_booking_by_id(&pool, held.id)
            .await
            .unwrap();

        // 1,400 stay less 140 weekly is 1,260; 10% off gives 126 and 10.
        let discount = dec!(136.00);
        let stay_net = dec!(1134.00);
        let cleaning_net = dec!(90.00);
        let tax = dec!(183.60);
        // 1,224.00 taxable + 183.60 GCT + 56.70 platform fee + 200.00 deposit.
        let total = dec!(1664.30);

        assert_eq!(amount(&quote["promo"]["discount"]), discount);
        assert_eq!(amount(&quote["tax_amount"]), tax);
        assert_eq!(amount(&quote["total_amount"]), total);

        assert_eq!(
            booking.promo_stay_discount + booking.promo_cleaning_discount,
            discount
        );
        assert_eq!(booking.accommodation_net(), stay_net);
        assert_eq!(booking.cleaning_net(), cleaning_net);
        assert_eq!(booking.tax_value, Some(tax));
        assert_eq!(booking.total_price, total);

        // Email payload.
        let email = super::super::notifications::guest_confirmation_payload(
            &booking,
            "Villa".to_string(),
            "Guest".to_string(),
            "https://example.test/receipt".to_string(),
        );
        assert_eq!(email.promo_discount, Some(discount));
        assert_eq!(email.accommodation_net, Some(stay_net));
        assert_eq!(email.cleaning_fee, Some(cleaning_net));
        assert_eq!(email.tax_amount, Some(tax));
        assert_eq!(email.total_price, total);

        // Tax document.
        let docs = db_core::tax_document::list_for_booking(&pool, booking.id)
            .await
            .unwrap();
        let receipt = docs
            .iter()
            .find(|d| d.document_type == db_core::models::TaxDocumentType::PaymentReceipt)
            .expect("a receipt");
        assert_eq!(receipt.promo_discount, discount);
        assert_eq!(receipt.accommodation_net, stay_net);
        assert_eq!(receipt.cleaning_fee, cleaning_net);
        assert_eq!(receipt.tax_amount, tax);
        assert_eq!(receipt.total_amount, total);

        // Ledger: the seeded host is paid in USD at 10% commission.
        let ledger = db_core::payout_ledger::get_booking_payout(&pool, booking.id)
            .await
            .unwrap()
            .expect("a ledger row");
        assert_eq!(ledger.promo_discount_amount, discount);
        assert_eq!(ledger.net_payout_amount, dec!(1101.60));

        // Deactivating the code and publishing a rate leave everything alone.
        sqlx::query(
            "UPDATE promo_code SET deactivated_at = NOW(), deactivated_by = $2 WHERE id = $1",
        )
        .bind(code_id)
        .bind(admin)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
             VALUES ('USD', 'JMD', 999, NOW())",
        )
        .execute(&pool)
        .await
        .unwrap();

        let after = db_core::booking::get_booking_by_id(&pool, booking.id)
            .await
            .unwrap();
        assert_eq!(after.total_price, total);
        assert_eq!(after.promo_stay_discount, booking.promo_stay_discount);
        let docs_after = db_core::tax_document::list_for_booking(&pool, booking.id)
            .await
            .unwrap();
        assert_eq!(docs_after.len(), docs.len());
        assert_eq!(docs_after[0].total_amount, docs[0].total_amount);
        assert_eq!(docs_after[0].jmd_total_amount, docs[0].jmd_total_amount);
        let ledger_after = db_core::payout_ledger::get_booking_payout(&pool, booking.id)
            .await
            .unwrap()
            .expect("a ledger row");
        assert_eq!(ledger_after, ledger);
    }
}

/// The generated document lists every new route, so the API contract can't lose one.
#[tokio::test]
async fn openapi_includes_promo_routes() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let app = app!(pool);
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api-docs/openapi.json")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let doc: serde_json::Value = test::read_body_json(resp).await;
    for (path, method) in [
        ("/api/v1/bookings/listing/{id}/quote", "get"),
        ("/api/v1/bookings/quote", "post"),
        ("/api/v1/admin/promo-codes", "get"),
        ("/api/v1/admin/promo-codes", "post"),
        ("/api/v1/admin/promo-codes/{id}", "get"),
        ("/api/v1/admin/promo-codes/{id}/deactivate", "post"),
    ] {
        assert!(doc["paths"][path][method].is_object(), "{method} {path}");
    }
}
