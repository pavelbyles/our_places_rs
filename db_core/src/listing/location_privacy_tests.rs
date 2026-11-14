//! Spec 103 §1.1 and §2.1: the `location_privacy` column, its defaults, its
//! history. (Spans are checked in `tests/listing_span_privacy.rs`, a binary of
//! its own.) Each test runs in a
//! rolled-back transaction and scopes its reads by the new host's ID.

use super::{create_listing, get_listings, update_listing_with_conn};
use crate::models::{NewHostProfile, NewListing, NewUser, UpdatedListing};
use crate::user::{create_host_profile, create_user};
use common::location::LocationPrivacy;
use rust_decimal_macros::dec;
use sqlx::{Connection, PgConnection};
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
        email: format!("loc_{id}@example.com"),
        password_hash: "secret".to_string(),
        first_name: "Loc".to_string(),
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

fn new_listing(user_id: Uuid, privacy: Option<LocationPrivacy>) -> NewListing {
    NewListing {
        name: format!("Location Villa {}", Uuid::now_v7()),
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
        location_privacy: privacy,
    }
}

#[tokio::test]
async fn a_new_listing_without_privacy_is_approximate() {
    let mut conn = connect().await;
    let mut tx = match conn.begin().await {
        Ok(tx) => tx,
        Err(e) => panic!("begin: {e}"),
    };
    let user_id = host(&mut tx).await;
    let created = match create_listing(&mut *tx, &new_listing(user_id, None)).await {
        Ok(listing) => listing,
        Err(e) => panic!("create with no privacy must not hit NOT NULL: {e}"),
    };
    assert_eq!(created.location_privacy, LocationPrivacy::Approximate);

    let exact = match create_listing(
        &mut *tx,
        &new_listing(user_id, Some(LocationPrivacy::Exact)),
    )
    .await
    {
        Ok(listing) => listing,
        Err(e) => panic!("create exact: {e}"),
    };
    assert_eq!(exact.location_privacy, LocationPrivacy::Exact);
}

#[tokio::test]
async fn existing_rows_read_as_approximate() {
    let mut conn = connect().await;
    let mut tx = match conn.begin().await {
        Ok(tx) => tx,
        Err(e) => panic!("begin: {e}"),
    };
    // A row inserted without naming the column, as every listing before this
    // migration was, takes the column default.
    let user_id = host(&mut tx).await;
    let id = Uuid::now_v7();
    let inserted = sqlx::query(
        "INSERT INTO listing (id, user_id, name, listing_structure_id, country, slug, base_currency, added_at)
         VALUES ($1, $2, 'Legacy Villa', 5, 'Jamaica', $3, 'USD', now())",
    )
    .bind(id)
    .bind(user_id)
    .bind(format!("legacy-{id}"))
    .execute(&mut *tx)
    .await;
    if let Err(e) = inserted {
        panic!("insert legacy row: {e}");
    }
    let privacy: Result<LocationPrivacy, _> =
        sqlx::query_scalar("SELECT location_privacy FROM listing WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await;
    assert_eq!(privacy.ok(), Some(LocationPrivacy::Approximate));
}

#[tokio::test]
async fn update_changes_or_keeps_privacy_and_history_records_the_old_value() {
    let mut conn = connect().await;
    let mut tx = match conn.begin().await {
        Ok(tx) => tx,
        Err(e) => panic!("begin: {e}"),
    };
    let user_id = host(&mut tx).await;
    let created = match create_listing(&mut *tx, &new_listing(user_id, None)).await {
        Ok(listing) => listing,
        Err(e) => panic!("create: {e}"),
    };

    let to_exact = UpdatedListing {
        location_privacy: Some(LocationPrivacy::Exact),
        ..UpdatedListing::default()
    };
    let updated = match update_listing_with_conn(&mut tx, created.id, &to_exact).await {
        Ok(listing) => listing,
        Err(e) => panic!("update to exact: {e}"),
    };
    assert_eq!(updated.location_privacy, LocationPrivacy::Exact);

    let unchanged = UpdatedListing {
        name: Some("Renamed Villa".to_string()),
        ..UpdatedListing::default()
    };
    let kept = match update_listing_with_conn(&mut tx, created.id, &unchanged).await {
        Ok(listing) => listing,
        Err(e) => panic!("update without privacy: {e}"),
    };
    assert_eq!(kept.location_privacy, LocationPrivacy::Exact);

    let history: Result<Vec<Option<LocationPrivacy>>, _> =
        sqlx::query_scalar("SELECT location_privacy FROM listing_history WHERE listing_id = $1")
            .bind(created.id)
            .fetch_all(&mut *tx)
            .await;
    // Each history row is the listing as it was before that update: one
    // `approximate` (before the first) and one `exact` (before the second).
    // Recording the new value instead would give two `exact` rows. Both rows
    // share a transaction timestamp, so compare without order.
    let mut history: Vec<&str> = history
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.map_or("null", LocationPrivacy::as_str))
        .collect();
    history.sort_unstable();
    assert_eq!(history, vec!["approximate", "exact"]);
}

#[tokio::test]
async fn search_returns_the_column() {
    let mut conn = connect().await;
    let mut tx = match conn.begin().await {
        Ok(tx) => tx,
        Err(e) => panic!("begin: {e}"),
    };
    let user_id = host(&mut tx).await;
    if let Err(e) = create_listing(
        &mut *tx,
        &new_listing(user_id, Some(LocationPrivacy::Exact)),
    )
    .await
    {
        panic!("create: {e}");
    }
    // `get_listings` is built at run time, so only this test catches a
    // missing column there.
    let filter = common::models::ListingFilter {
        user_id: Some(user_id),
        include_inactive: true,
        ..Default::default()
    };
    let found = match get_listings(&mut *tx, 1, 10, Some(filter)).await {
        Ok(rows) => rows,
        Err(e) => panic!("search: {e}"),
    };
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].location_privacy, LocationPrivacy::Exact);
}
