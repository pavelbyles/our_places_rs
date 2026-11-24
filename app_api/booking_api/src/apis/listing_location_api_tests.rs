//! Spec 103 §1.5: the booking reads carry the listing's stored point only
//! for a confirmed or completed booking, and only to its parties.

use super::authz_tests::{booking_request, listing, settings, tax_settings, user};
use super::request_api_tests::bearer;
use super::stay_pricing_pin_tests::test_db;
use actix_web::{App, test, web};
use db_core::models::UserRole;
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::{Layer, Registry};
use uuid::Uuid;

const LAT: &str = "18.012345";
const LNG: &str = "-76.790123";
const STATUSES: [&str; 4] = ["pending", "confirmed", "completed", "cancelled"];

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

macro_rules! get {
    ($app:expr, $uri:expr, $as:expr) => {{
        let resp = test::call_service(
            &$app,
            test::TestRequest::get()
                .uri(&$uri)
                .insert_header(("Accept", "application/json"))
                .insert_header(bearer($as))
                .to_request(),
        )
        .await;
        let status = resp.status().as_u16();
        let body: Value = if status == 200 {
            test::read_body_json(resp).await
        } else {
            Value::Null
        };
        (status, body)
    }};
}

/// A listing with a stored point.
async fn located(pool: &PgPool, host: Uuid) -> Uuid {
    let villa = listing(pool, host).await;
    sqlx::query(&format!(
        "UPDATE listing SET latitude = {LAT}, longitude = {LNG} WHERE id = $1"
    ))
    .bind(villa)
    .execute(pool)
    .await
    .expect("set coordinates");
    villa
}

async fn set_status(pool: &PgPool, booking: Uuid, status: &str) {
    sqlx::query("UPDATE booking SET status = $2::booking_status WHERE id = $1")
        .bind(booking)
        .bind(status)
        .execute(pool)
        .await
        .expect("set status");
}

fn location(body: &Value) -> Option<(String, String)> {
    let loc = body.get("listing_location")?;
    Some((loc["latitude"].to_string(), loc["longitude"].to_string()))
}

fn expected(status: &str) -> Option<(String, String)> {
    matches!(status, "confirmed" | "completed").then(|| (LAT.to_string(), LNG.to_string()))
}

#[actix_web::test]
async fn the_point_follows_status_and_caller_on_both_reads() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let guest = user(&pool, UserRole::Booker).await;
    let stranger = user(&pool, UserRole::Booker).await;
    let admin = user(&pool, UserRole::Admin).await;
    let app = app!(pool);

    let mut ids = Vec::new();
    for status in STATUSES {
        // One listing per booking: they all take the same dates.
        let villa = located(&pool, host).await;
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
        assert_eq!(resp.status(), 201, "create a booking to set to {status}");
        let body: Value = test::read_body_json(resp).await;
        let id: Uuid = serde_json::from_value(body["id"].clone()).expect("booking id");
        set_status(&pool, id, status).await;
        ids.push((status, id));
    }

    for (status, id) in &ids {
        let uri = format!("/api/v1/bookings/{id}");
        for (who, caller) in [("guest", guest), ("host", host), ("admin", admin)] {
            let (code, body) = get!(app, uri, caller);
            assert_eq!(code, 200, "{who} reading a {status} booking");
            assert_eq!(location(&body), expected(status), "{who}, {status}: {body}");
        }
        let (code, body) = get!(app, uri, stranger);
        assert_eq!(code, 404, "another guest, {status}");
        assert_eq!(body, Value::Null);
    }

    let list = format!("/api/v1/bookings/user/{guest}?per_page=100");
    for (who, caller) in [("guest", guest), ("admin", admin)] {
        let (code, body) = get!(app, list, caller);
        assert_eq!(code, 200, "{who} listing");
        let rows = body.as_array().expect("a list");
        assert_eq!(rows.len(), ids.len());
        for row in rows {
            let id: Uuid = serde_json::from_value(row["id"].clone()).expect("id");
            let status = ids
                .iter()
                .find(|(_, b)| *b == id)
                .map(|(s, _)| *s)
                .expect("known");
            assert_eq!(location(row), expected(status), "{who}, {status}: {row}");
        }
    }
    for (who, caller) in [("host", host), ("another guest", stranger)] {
        let (code, _) = get!(app, list, caller);
        assert_eq!(code, 404, "{who} listing the guest's bookings");
    }
    // Another guest's own list never carries this guest's points.
    let (code, body) = get!(app, format!("/api/v1/bookings/user/{stranger}"), stranger);
    assert_eq!(code, 200);
    assert!(!body.to_string().contains(LAT), "{body}");

    // An update's response leaves it out, even for a confirmed booking.
    let confirmed = ids
        .iter()
        .find(|(s, _)| *s == "confirmed")
        .map(|(_, id)| *id)
        .expect("confirmed");
    let resp = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&format!("/api/v1/bookings/{confirmed}"))
            .insert_header(("Accept", "application/json"))
            .insert_header(bearer(host))
            .set_json(serde_json::json!({ "door_access_code": "4321" }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body.get("listing_location").is_none(), "{body}");
    assert!(!body.to_string().contains(LAT), "{body}");
}

/// Counts `listing_locations` spans.
#[derive(Clone, Default)]
struct Lookups(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> Layer<S> for Lookups {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        _ctx: Context<'_, S>,
    ) {
        if attrs.metadata().name() == "listing_locations" {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[actix_web::test]
async fn the_list_looks_points_up_at_most_once_and_not_at_all_when_none_qualify() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = user(&pool, UserRole::Host).await;
    let guest = user(&pool, UserRole::Booker).await;
    let app = app!(pool);
    let mut ids = Vec::new();
    for _ in 0..3 {
        let villa = located(&pool, host).await;
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
        let body: Value = test::read_body_json(resp).await;
        ids.push(serde_json::from_value::<Uuid>(body["id"].clone()).expect("id"));
    }
    let list = format!("/api/v1/bookings/user/{guest}");

    let count = |expected_rows: usize| {
        let app = &app;
        let list = list.clone();
        async move {
            let lookups = Lookups::default();
            let _guard =
                tracing::subscriber::set_default(Registry::default().with(lookups.clone()));
            tracing::callsite::rebuild_interest_cache();
            let (code, body) = get!(app, list, guest);
            assert_eq!(code, 200);
            assert_eq!(body.as_array().map(Vec::len), Some(expected_rows));
            lookups.0.load(Ordering::SeqCst)
        }
    };

    // All pending: nothing qualifies, so no lookup.
    assert_eq!(count(3).await, 0);
    for id in &ids {
        set_status(&pool, *id, "confirmed").await;
    }
    assert_eq!(count(3).await, 1);
}
