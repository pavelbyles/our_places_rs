//! Stay totals in search results.

use super::tests::logs::capture_logs;
use super::tests::{create_test_user, get_test_settings};
use super::*;
use actix_web::{App, test, web};
use rust_decimal_macros::dec;
use sqlx::postgres::PgPoolOptions;
use sqlx_db_tester::TestPg;
use std::path::Path;

const GOLDEN: &str = include_str!("../../../common/testdata/stay_pricing_golden.json");
const STAY: &str = "check_in=2030-08-01&check_out=2030-08-04";

fn test_db() -> TestPg {
    dotenvy::dotenv().ok();
    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    TestPg::new(db_url, Path::new("../../db_core/migrations"))
}

fn amount(value: &serde_json::Value) -> Decimal {
    let text = value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string());
    Decimal::from_str(&text).expect("an amount")
}

fn optional_amount(value: &serde_json::Value) -> Option<Decimal> {
    (!value.is_null()).then(|| amount(value))
}

async fn host(pool: &PgPool) -> Uuid {
    let mut conn = pool.acquire().await.expect("connection");
    create_test_user(&mut conn).await
}

/// A plain listing to adjust with SQL.
async fn villa(pool: &PgPool, user_id: Uuid) -> Uuid {
    db_listing::create_listing(
        pool,
        &NewListing {
            name: format!("Total Villa {}", Uuid::now_v7()),
            user_id,
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
            commission_pct: None,
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

async fn set_rate(pool: &PgPool, target: &str, rate: Decimal) {
    sqlx::query(
        "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
         VALUES ('USD', $1, $2, NOW())",
    )
    .bind(target)
    .bind(rate)
    .execute(pool)
    .await
    .expect("insert rate");
}

macro_rules! app {
    ($pool:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new($pool.clone()))
                .app_data(web::Data::new(get_test_settings()))
                .configure(configure_routes),
        )
        .await
    };
}

macro_rules! search {
    ($app:expr, $user:expr, $query:expr) => {{
        let req = test::TestRequest::get()
            .uri(&format!(
                "/api/v1/listings?per_page=100&user_id={}&{}",
                $user, $query
            ))
            .insert_header(("Accept", "application/json"))
            .to_request();
        let found: Vec<ListingResponse> = test::call_and_read_body_json(&$app, req).await;
        found
    }};
}

#[actix_web::test]
async fn totals_match_every_golden_case() {
    let cases: Vec<serde_json::Value> = serde_json::from_str(GOLDEN).expect("golden file");
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let user_id = host(&pool).await;
        let listing = villa(&pool, user_id).await;
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
        .bind(listing)
        .execute(&pool)
        .await
        .expect("set listing fields");
        for row in case["overrides"].as_array().expect("overrides") {
            sqlx::query(
                "INSERT INTO listing_price_overrides (listing_id, start_date, end_date, nightly_rate, min_nights)
                 VALUES ($1, $2::date, $3::date, $4, $5)",
            )
            .bind(listing)
            .bind(row["start_date"].as_str().expect("start_date"))
            .bind(row["end_date"].as_str().expect("end_date"))
            .bind(amount(&row["nightly_rate"]))
            .bind(row["min_nights"].as_i64().expect("min_nights") as i32)
            .execute(&pool)
            .await
            .expect("insert override");
        }
        let currency = case["currency"].as_str().expect("currency");
        if let Some(rate) = optional_amount(&case["rate"]) {
            set_rate(&pool, currency, rate).await;
        }

        let app = app!(pool);
        let found = search!(
            app,
            user_id,
            format!(
                "check_in={}&check_out={}&guests={}&currency={currency}",
                case["check_in"].as_str().expect("check_in"),
                case["check_out"].as_str().expect("check_out"),
                case["guests"],
            )
        );
        assert_eq!(found.len(), 1, "{name}");
        let total = found[0].stay_total.as_ref().expect(name);
        let expected = &case["expected"];
        assert_eq!(total.total_amount, amount(&expected["total"]), "{name}");
        assert_eq!(
            total.damage_deposit,
            amount(&expected["damage_deposit"]),
            "{name}"
        );
        assert_eq!(
            i64::from(total.nights),
            expected["nights"].as_i64().expect("nights"),
            "{name}"
        );
        assert_eq!(total.currency, currency, "{name}");
    }
}

#[actix_web::test]
async fn guests_default_to_one_and_currency_to_the_listings_own() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let user_id = host(&pool).await;
    let listing = villa(&pool, user_id).await;
    sqlx::query("UPDATE listing SET guests_included = 1, extra_guest_fee = 50.00 WHERE id = $1")
        .bind(listing)
        .execute(&pool)
        .await
        .expect("set fee");
    let app = app!(pool);

    let found = search!(app, user_id, STAY);
    let total = found[0].stay_total.as_ref().expect("a total");
    assert_eq!(total.currency, "USD");
    // 3 nights × 200 = 600, + 15% tax = 90, + 5% fee = 30. No extra guest fee.
    assert_eq!(total.total_amount, dec!(720.00));

    let found = search!(app, user_id, format!("{STAY}&guests=2"));
    let total = found[0].stay_total.as_ref().expect("a total");
    // 600 + (50 × 3) = 750, + 112.50 tax + 37.50 fee.
    assert_eq!(total.total_amount, dec!(900.00));
}

#[actix_web::test]
async fn no_dates_means_no_total_and_the_response_is_otherwise_unchanged() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let user_id = host(&pool).await;
    villa(&pool, user_id).await;
    let app = app!(pool);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/listings?user_id={user_id}"))
        .insert_header(("Accept", "application/json"))
        .to_request();
    let found: Vec<serde_json::Value> = test::call_and_read_body_json(&app, req).await;
    assert!(found[0].get("stay_total").is_none());
}

#[actix_web::test]
async fn a_missing_rate_affects_only_that_listing() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let user_id = host(&pool).await;
    let usd = villa(&pool, user_id).await;
    let other = villa(&pool, user_id).await;
    sqlx::query("INSERT INTO currencies (code, name) VALUES ('TTD', 'Trinidad Dollar')")
        .execute(&pool)
        .await
        .expect("add currency");
    sqlx::query("UPDATE listing SET base_currency = 'TTD' WHERE id = $1")
        .bind(other)
        .execute(&pool)
        .await
        .expect("set base currency");
    set_rate(&pool, "JMD", dec!(160)).await;
    let app = app!(pool);

    let found = search!(app, user_id, format!("{STAY}&currency=JMD"));
    let row = |id: Uuid| found.iter().find(|l| l.id == id).expect("the listing");
    assert_eq!(
        row(usd).stay_total.as_ref().map(|t| t.total_amount),
        Some(dec!(115200.00))
    );
    assert_eq!(row(other).stay_total, None);
    assert_eq!(row(other).price_unavailable_in.as_deref(), Some("JMD"));
}

#[actix_web::test]
async fn a_listing_that_cant_be_priced_gets_no_total_and_a_warning() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let user_id = host(&pool).await;
    let fine = villa(&pool, user_id).await;
    let too_large = villa(&pool, user_id).await;
    let bad_discount = villa(&pool, user_id).await;
    sqlx::query("UPDATE listing SET price_per_night = 99999999.00 WHERE id = $1")
        .bind(too_large)
        .execute(&pool)
        .await
        .expect("set price");
    sqlx::query("UPDATE listing SET weekly_discount_percentage = 1000 WHERE id = $1")
        .bind(bad_discount)
        .execute(&pool)
        .await
        .expect("set discount");
    let app = app!(pool);

    let (logs, capture) = capture_logs();
    let found = search!(app, user_id, "check_in=2030-08-01&check_out=2030-08-08");
    drop(capture);

    assert_eq!(found.len(), 3);
    let total = |id: Uuid| {
        found
            .iter()
            .find(|l| l.id == id)
            .expect("the listing")
            .stay_total
            .clone()
    };
    assert!(total(fine).is_some());
    assert_eq!(total(too_large), None);
    assert_eq!(total(bad_discount), None);
    let text = logs.text();
    assert!(text.contains("search stays could not be priced"), "{text}");
    assert!(text.contains(&too_large.to_string()), "{text}");
    assert!(text.contains(&bad_discount.to_string()), "{text}");
    assert!(!text.contains(&format!("{fine}, ")), "{text}");
}

#[actix_web::test]
async fn a_failed_override_load_fails_rather_than_pricing_without_overrides() {
    let pool = PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_millis(200))
        .connect_lazy("postgres://postgres:password@127.0.0.1:1/none")
        .expect("lazy pool");
    let test_db = test_db();
    let real = test_db.get_pool().await;
    let user_id = host(&real).await;
    villa(&real, user_id).await;
    let rows = db_listing::get_listings(
        &real,
        1,
        10,
        Some(common::models::ListingFilter {
            user_id: Some(user_id),
            ..Default::default()
        }),
    )
    .await
    .expect("rows");
    let stay = SearchedStay {
        check_in: NaiveDate::from_ymd_opt(2030, 8, 1).expect("date"),
        check_out: NaiveDate::from_ymd_opt(2030, 8, 4).expect("date"),
        guests: 1,
    };

    assert!(
        search_stay_totals(&pool, &rows, stay, None, None)
            .await
            .is_err()
    );
}

#[actix_web::test]
async fn a_failed_pricing_task_gives_no_totals_and_a_logged_error() {
    let join_error = tokio::task::spawn_blocking(|| -> PricedStays { panic!("boom") }).await;
    assert!(join_error.is_err());

    let (logs, capture) = capture_logs();
    let priced = totals_or_none(join_error);
    drop(capture);

    assert!(priced.totals.is_empty());
    assert!(logs.text().contains("pricing search results failed"));
}

#[actix_web::test]
async fn query_count_doesnt_depend_on_how_many_listings_there_are() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    set_rate(&pool, "JMD", dec!(160)).await;
    let one_host = host(&pool).await;
    villa(&pool, one_host).await;
    let many_host = host(&pool).await;
    for _ in 0..50 {
        let listing = villa(&pool, many_host).await;
        sqlx::query(
            "INSERT INTO listing_price_overrides (listing_id, start_date, end_date, nightly_rate, min_nights)
             VALUES ($1, '2030-08-02', '2030-08-03', 300.00, 1)",
        )
        .bind(listing)
        .execute(&pool)
        .await
        .expect("insert override");
    }
    let app = app!(pool);
    let query = format!("{STAY}&currency=JMD");

    // Warm-ups, so connection setup and type lookups aren't counted.
    for _ in 0..3 {
        search!(app, one_host, query);
        search!(app, many_host, query);
    }

    let count = |user: Uuid, expected_rows: usize| {
        let query = query.clone();
        let app = &app;
        async move {
            let (logs, capture) = capture_logs();
            let found = search!(app, user, query);
            drop(capture);
            assert_eq!(found.len(), expected_rows);
            assert!(found.iter().all(|l| l.stay_total.is_some()));
            logs.text().matches("sqlx::query").count()
        }
    };
    let for_one = count(one_host, 1).await;
    let for_fifty = count(many_host, 50).await;
    assert!(for_one > 0, "the capture must see queries");
    assert_eq!(for_one, for_fifty);
}
