//! Pins stay prices through the public quote endpoint, using figures worked out
//! by hand in `common/testdata/stay_pricing_golden.json`. `listing_api`'s search
//! totals are held to the same file, so the card and the quote can't disagree.

use super::authz_tests::{listing, settings, tax_settings, user};
use actix_web::{App, test, web};
use db_core::models::UserRole;
use rust_decimal::Decimal;
use sqlx::PgPool;
use sqlx_db_tester::TestPg;
use std::path::Path;
use std::str::FromStr;
use uuid::Uuid;

/// The cases, worked out by hand.
pub(super) const GOLDEN: &str =
    include_str!("../../../../common/testdata/stay_pricing_golden.json");

pub(super) fn test_db() -> TestPg {
    dotenvy::dotenv().ok();
    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    TestPg::new(db_url, Path::new("../../db_core/migrations"))
}

pub(super) fn golden_cases() -> Vec<serde_json::Value> {
    serde_json::from_str(GOLDEN).expect("golden file")
}

pub(super) fn amount(value: &serde_json::Value) -> Decimal {
    let text = value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string());
    Decimal::from_str(&text).expect("an amount")
}

fn optional_amount(value: &serde_json::Value) -> Option<Decimal> {
    (!value.is_null()).then(|| amount(value))
}

/// Creates the case's listing, overrides and exchange rate.
pub(super) async fn seed_case(pool: &PgPool, case: &serde_json::Value) -> Uuid {
    let host = user(pool, UserRole::Host).await;
    let villa = listing(pool, host).await;
    let fields = &case["listing"];
    sqlx::query(
        "UPDATE listing SET price_per_night = $1, cleaning_fee = $2, damage_deposit = $3,
             guests_included = $4, extra_guest_fee = $5, weekly_discount_percentage = $6,
             monthly_discount_percentage = $7
         WHERE id = $8",
    )
    .bind(amount(&fields["price_per_night"]))
    .bind(amount(&fields["cleaning_fee"]))
    .bind(amount(&fields["damage_deposit"]))
    .bind(fields["guests_included"].as_i64().expect("guests_included") as i32)
    .bind(amount(&fields["extra_guest_fee"]))
    .bind(optional_amount(&fields["weekly"]))
    .bind(optional_amount(&fields["monthly"]))
    .bind(villa)
    .execute(pool)
    .await
    .expect("set listing fields");
    for row in case["overrides"].as_array().expect("overrides") {
        sqlx::query(
            "INSERT INTO listing_price_overrides (listing_id, start_date, end_date, nightly_rate, min_nights)
             VALUES ($1, $2::date, $3::date, $4, $5)",
        )
        .bind(villa)
        .bind(row["start_date"].as_str().expect("start_date"))
        .bind(row["end_date"].as_str().expect("end_date"))
        .bind(amount(&row["nightly_rate"]))
        .bind(row["min_nights"].as_i64().expect("min_nights") as i32)
        .execute(pool)
        .await
        .expect("insert override");
    }
    if let Some(rate) = optional_amount(&case["rate"]) {
        sqlx::query(
            "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
             VALUES ('USD', $1, $2, NOW())",
        )
        .bind(case["currency"].as_str().expect("currency"))
        .bind(rate)
        .execute(pool)
        .await
        .expect("insert rate");
    }
    villa
}

#[tokio::test]
async fn the_public_quote_and_hold_match_every_golden_case() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(settings()))
            .app_data(web::Data::new(tax_settings()))
            .configure(crate::apis::configure_routes),
    )
    .await;

    for case in golden_cases() {
        let name = case["name"].as_str().expect("name");
        let currency = case["currency"].as_str().expect("currency");
        if optional_amount(&case["rate"]).is_some() {
            sqlx::query(
                "DELETE FROM currency_exchange_rates WHERE base_currency = 'USD' AND target_currency = $1",
            )
            .bind(currency)
            .execute(&pool)
            .await
            .expect("clear prior rate for golden case");
        }
        let villa = seed_case(&pool, &case).await;

        let uri = format!(
            "/api/v1/bookings/listing/{villa}/quote?check_in={}&check_out={}&guests={}&currency={currency}",
            case["check_in"].as_str().expect("check_in"),
            case["check_out"].as_str().expect("check_out"),
            case["guests"],
        );
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&uri)
                .insert_header(("Accept", "application/json"))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200, "{name}: quote");
        let quote: serde_json::Value = test::read_body_json(resp).await;

        let expected = &case["expected"];
        assert_eq!(quote["nights"], expected["nights"], "{name}: nights");
        for (field, key) in [
            ("subtotal", "subtotal"),
            ("stay_discount", "stay_discount"),
            ("cleaning_fee", "cleaning_fee"),
            ("tax_amount", "tax"),
            ("platform_fee", "platform_fee"),
            ("damage_deposit", "damage_deposit"),
            ("total_amount", "total"),
        ] {
            assert_eq!(
                amount(&quote[field]),
                amount(&expected[key]),
                "{name}: {field}"
            );
        }

        let guest = user(&pool, UserRole::Booker).await;
        let hold_resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/bookings")
                .insert_header(("Accept", "application/json"))
                .insert_header((
                    "Authorization",
                    format!("Bearer {}", api_core::auth::generate_test_jwt(guest)),
                ))
                .set_json(serde_json::json!({
                    "guest_id": guest,
                    "listing_id": villa,
                    "check_in": case["check_in"],
                    "check_out": case["check_out"],
                    "num_adults": case["guests"],
                    "num_children": 0,
                    "num_infants": 0,
                    "num_pets": 0,
                    "is_business_trip": false,
                    "currency": currency,
                    "agreed_cancellation_policy": "Flexible",
                }))
                .to_request(),
        )
        .await;
        assert_eq!(hold_resp.status(), 201, "{name}: hold");

        let id: Uuid = sqlx::query_scalar("SELECT id FROM booking WHERE guest_id = $1")
            .bind(guest)
            .fetch_one(&pool)
            .await
            .expect("the hold");
        let booking = db_core::booking::get_booking_by_id(&pool, id)
            .await
            .expect("read the hold");
        assert_eq!(
            booking.total_price,
            amount(&expected["total"]),
            "{name}: hold total"
        );
    }
}
