use actix_web::{HttpResponse, web};
use chrono::Utc;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, prelude::FromRow};
use std::str::FromStr;
use strum_macros::EnumString;
use uuid::Uuid;

#[derive(Deserialize, Serialize, Debug, FromRow)]
pub struct Listing {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub listing_structure_id: i32,
    pub country: String,
    pub price_per_night: Decimal,
    pub is_active: bool,
    pub added_at: i64,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct CreateListingResponse {
    pub id: Uuid,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ListingFormData {
    pub id: String,
    pub name: String,
    pub description: String,
    pub structure: String,
    pub country: String,
    pub price_per_night: Decimal,
    pub is_active: bool,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, sqlx::Type, EnumString)]
#[sqlx(rename_all = "snake_case")]
pub enum StructureType {
    #[strum(serialize = "Apartment")]
    Apartment,
    #[strum(serialize = "House")]
    House,
    #[strum(serialize = "Townhouse")]
    Townhouse,
    #[strum(serialize = "Studio")]
    Studio,
    #[strum(serialize = "Villa")]
    Villa,
}

impl StructureType {
    fn value(&self) -> i32 {
        match self {
            StructureType::Apartment => 1,
            StructureType::House => 2,
            StructureType::Townhouse => 3,
            StructureType::Studio => 4,
            StructureType::Villa => 5,
        }
    }
}

trait Listable {
    fn from_listing_form_data(listing: &ListingFormData) -> Self;
    // fn to_listing_form_data(&self,
}

impl Listable for Listing {
    fn from_listing_form_data(listing_form_data: &ListingFormData) -> Listing {
        let result = Uuid::try_parse(&listing_form_data.id);

        match result {
            // When there's a valid ListingFormData.Uuid -> case when there's an update to an
            // exising listing
            Ok(uuid_val) => Listing {
                id: uuid_val,
                name: listing_form_data.name.clone(),
                description: listing_form_data.description.clone(),
                listing_structure_id: StructureType::from_str(&listing_form_data.structure)
                    .unwrap()
                    .value(),
                country: listing_form_data.country.clone(),
                price_per_night: listing_form_data.price_per_night,
                is_active: listing_form_data.is_active,
                added_at: chrono::offset::Local::now().timestamp_millis(),
            },
            // When this is a new listing there will be no ListingFormData.Uuid
            Err(_err) => {
                println!("Could not parse uuid");
                Listing {
                    id: Uuid::nil(),
                    name: listing_form_data.name.clone(),
                    description: listing_form_data.description.clone(),
                    listing_structure_id: StructureType::from_str(&listing_form_data.structure)
                        .unwrap()
                        .value(),
                    country: listing_form_data.country.clone(),
                    price_per_night: listing_form_data.price_per_night,
                    is_active: listing_form_data.is_active,
                    added_at: chrono::offset::Local::now().timestamp_millis(),
                }
            }
        }
    }
}

#[allow(dead_code)]
pub async fn create_listing(
    form: web::Form<ListingFormData>,
    db_pool: web::Data<PgPool>,
) -> HttpResponse {
    let new_listing = Listing::from_listing_form_data(&form);

    let mut conn = match db_pool.get_ref().acquire().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to get database connection: {}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };

    let row_result = sqlx::query_as!(
        CreateListingResponse,
        r#"
        INSERT INTO listing (id, name, description, listing_structure_id, country, price_per_night, is_active, added_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id
        "#,
        Uuid::new_v4(),
        new_listing.name,
        new_listing.description,
        new_listing.listing_structure_id,
        new_listing.country,
        new_listing.price_per_night,
        new_listing.is_active,
        Utc::now()
    )
    .fetch_one(&mut *conn)
    .await;

    match row_result {
        Ok(res) => {
            println!("Created new listing");
            HttpResponse::Created().json(res)
        }
        Err(e) => {
            println!("Failed to execute query: {}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}

// --- Add this test module at the end of src/apis/listings.rs ---

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, http::StatusCode, test, web};
    use dotenv::dotenv; // To load .env file for database URL
    use rust_decimal_macros::dec;
    use sqlx::PgPool;
    use sqlx::postgres::PgPoolOptions;
    use std::env; // For creating Decimal literals easily

    // --- Helper: Database Setup ---
    // Gets a connection pool to the TEST database.
    // Assumes TEST_DATABASE_URL environment variable is set (e.g., in a .env file).
    // IMPORTANT: This requires a running PostgreSQL instance configured for testing.
    // The build.rs script seems to handle Docker setup, which is helpful.
    async fn setup_test_db_pool() -> PgPool {
        dotenv().ok(); // Load .env file if present

        let database_url = env::var("DATABASE_URL") // build.rs uses DATABASE_URL implicitly for sqlx migrate
            .expect("DATABASE_URL must be set in environment or .env file for tests");

        PgPoolOptions::new()
            .max_connections(5) // Keep test pool small
            .connect(&database_url)
            .await
            .expect("Failed to create test database pool. Is the DB running and accessible?")
    }

    // --- Test Case: Successful Listing Creation ---
    #[actix_web::test]
    async fn test_create_listing_success() {
        // --- Arrange ---
        let pool = setup_test_db_pool().await;
        // Start a transaction for this test. It will be rolled back at the end.
        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        // Create a minimal Actix App for testing this specific handler
        let app = test::init_service(
            App::new()
                // Provide the *database pool* as app data. The handler expects web::Data<PgPool>.
                // Cloning the pool is cheap.
                .app_data(web::Data::new(pool.clone()))
                .route("/listings", web::post().to(create_listing)), // Route POST /listings to our handler
        )
        .await;

        // Prepare the form data payload
        let form_data = ListingFormData {
            id: "".to_string(), // ID is ignored by create_listing, a new one is generated
            name: "Cozy Test Cabin".to_string(),
            description: "A nice place to test.".to_string(),
            structure: "House".to_string(), // Must match a variant in StructureType
            country: "Testland".to_string(),
            price_per_night: dec!(125.50),
            is_active: true,
        };

        // --- Act ---
        // Create a test request simulating a POST with form data
        let req = test::TestRequest::post()
            .uri("/listings")
            .set_form(&form_data) // Use set_form for web::Form extractor
            .to_request();

        // Call the service (our create_listing handler)
        let resp = test::call_service(&app, req).await;

        // --- Assert ---
        // 1. Check the HTTP response status code
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "Expected status code 201 Created"
        );

        // 2. Check the response body (should contain the new listing's ID)
        let body: CreateListingResponse = test::read_body_json(resp).await;
        assert!(
            !body.id.is_nil(),
            "Expected a valid UUID in the response body"
        );

        // 3. Verify the data was actually inserted into the database (within the transaction)
        let saved_listing_result = sqlx::query_as!(
            Listing, // Use the Listing struct which includes all fields
            r#"
            SELECT id, name, description, listing_structure_id, country, price_per_night, is_active, added_at
            FROM listing
            WHERE id = $1
            "#,
            body.id // Use the ID returned in the response body to fetch the record
        )
        // IMPORTANT: Execute the query against the transaction (`tx`), not the pool
        .fetch_one(&mut *tx)
        .await;

        assert!(
            saved_listing_result.is_ok(),
            "Failed to fetch the newly created listing from the DB"
        );
        let saved_listing = saved_listing_result.unwrap();

        // Compare inserted data with the input form data
        assert_eq!(saved_listing.id, body.id);
        assert_eq!(saved_listing.name, form_data.name);
        assert_eq!(saved_listing.description, form_data.description);
        // Compare the integer ID corresponding to the "House" string
        assert_eq!(
            saved_listing.listing_structure_id,
            StructureType::House.value()
        );
        assert_eq!(saved_listing.country, form_data.country);
        assert_eq!(saved_listing.price_per_night, form_data.price_per_night);
        assert_eq!(saved_listing.is_active, form_data.is_active);
        // We can't easily check the exact timestamp, but we know it should be set
        assert!(
            saved_listing.added_at > 0,
            "added_at timestamp should be set"
        );

        // --- Cleanup ---
        // Rollback the transaction to undo the INSERT, keeping the test DB clean
        tx.rollback().await.expect("Failed to rollback transaction");
    }

    // --- TODO: Add more test cases ---

    // Example: Test case for invalid structure type (if desired, though might be caught by deserialization)
    // #[actix_web::test]
    // async fn test_create_listing_invalid_structure() { ... }

    // Example: Test case simulating a database error (harder, might require specific setup
    // like violating a UNIQUE constraint if one existed on 'name', for example)
    // #[actix_web::test]
    // async fn test_create_listing_db_error() { ... }
}
