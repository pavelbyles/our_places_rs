//! Spec 103 §1.2: creating a listing records no coordinates in spans.
//!
//! A binary of its own: tracing caches callsite interest process-wide, so a
//! thread-local recorder misses spans that parallel tests registered first.

use common::location::LocationPrivacy;
use db_core::listing::create_listing;
use db_core::models::{NewHostProfile, NewListing, NewUser};
use db_core::user::{create_host_profile, create_user};
use rust_decimal_macros::dec;
use sqlx::{Connection, PgConnection};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const LAT: f64 = 18.40741;
const LNG: f64 = -77.10313;

async fn connect() -> PgConnection {
    dotenvy::dotenv().ok();
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".to_string());
    match PgConnection::connect(&url).await {
        Ok(conn) => conn,
        Err(e) => panic!("connect to the test database: {e}"),
    }
}

async fn host(conn: &mut PgConnection) -> Uuid {
    let id = Uuid::now_v7();
    let user = NewUser {
        id,
        email: format!("span_{id}@example.com"),
        password_hash: "secret".to_string(),
        first_name: "Span".to_string(),
        last_name: "Host".to_string(),
        phone_number: None,
        is_active: true,
        is_verified: true,
        verification_code: None,
        verification_code_expires_at: None,
        attributes: serde_json::json!({}),
        roles: None,
        default_currency: "USD".to_string(),
    };
    if let Err(e) = create_user(&mut *conn, &user).await {
        panic!("create user: {e}");
    }
    let profile = NewHostProfile {
        description: None,
        verified_status: None,
        payout_details: None,
    };
    if let Err(e) = create_host_profile(&mut *conn, id, &profile).await {
        panic!("create host profile: {e}");
    }
    id
}

fn new_listing(user_id: Uuid) -> NewListing {
    NewListing {
        name: format!("Span Villa {}", Uuid::now_v7()),
        user_id,
        description: None,
        listing_structure_id: 5,
        country: "Jamaica".to_string(),
        price_per_night: Some(dec!(300.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 4,
        bedrooms: 2,
        beds: 2,
        full_bathrooms: 2,
        half_bathrooms: 0,
        square_meters: None,
        latitude: Some(LAT),
        longitude: Some(LNG),
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
        location_privacy: Some(LocationPrivacy::Approximate),
    }
}

/// Records every span's and event's name and fields as text.
#[derive(Clone, Default)]
struct Recorder {
    lines: Arc<Mutex<Vec<String>>>,
    next_id: Arc<Mutex<u64>>,
}

struct FieldText(String);

impl tracing::field::Visit for FieldText {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.push_str(&format!(" {}={:?}", field.name(), value));
    }
}

impl Recorder {
    fn push(&self, line: String) {
        if let Ok(mut lines) = self.lines.lock() {
            lines.push(line);
        }
    }
}

impl tracing::Subscriber for Recorder {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        let mut text = FieldText(span.metadata().name().to_string());
        span.record(&mut text);
        self.push(text.0);
        let id = match self.next_id.lock() {
            Ok(mut next) => {
                *next += 1;
                *next
            }
            Err(_) => 1,
        };
        tracing::span::Id::from_u64(id)
    }
    fn record(&self, _: &tracing::span::Id, values: &tracing::span::Record<'_>) {
        let mut text = FieldText(String::from("record"));
        values.record(&mut text);
        self.push(text.0);
    }
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut text = FieldText(String::from("event"));
        event.record(&mut text);
        self.push(text.0);
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

#[tokio::test]
async fn creating_a_listing_records_no_coordinates_in_spans() {
    let recorder = Recorder::default();
    let _guard = tracing::subscriber::set_default(recorder.clone());

    let mut conn = connect().await;
    let mut tx = match conn.begin().await {
        Ok(tx) => tx,
        Err(e) => panic!("begin: {e}"),
    };
    let user_id = host(&mut tx).await;
    if let Err(e) = create_listing(&mut *tx, &new_listing(user_id)).await {
        panic!("create: {e}");
    }

    let lines = match recorder.lines.lock() {
        Ok(lines) => lines.clone(),
        Err(_) => panic!("recorder poisoned"),
    };
    assert!(
        lines.iter().any(|s| s.starts_with("create_listing")),
        "the create_listing span was not recorded: {lines:?}"
    );
    for text in &lines {
        assert!(
            !text.contains("18.40741") && !text.contains("77.10313"),
            "coordinates in a span: {text}"
        );
    }
}
