use actix_web::{HttpResponse, web};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, prelude::FromRow};
use std::str::FromStr;
use strum_macros::{Display, EnumString};
use uuid::Uuid;

#[derive(Deserialize, Serialize, Debug, FromRow)]
pub struct Listing {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub listing_structure_id: i32,
    pub country: String,
    pub price_per_night: Option<Decimal>,
    pub is_active: bool,
    pub added_at: DateTime<Utc>,
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

#[derive(Serialize, Deserialize, Debug, Display, PartialEq, Clone, sqlx::Type, EnumString)]
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
                description: Some(listing_form_data.description.clone()),
                listing_structure_id: StructureType::from_str(&listing_form_data.structure)
                    .unwrap()
                    .value(),
                country: listing_form_data.country.clone(),
                price_per_night: Some(listing_form_data.price_per_night),
                is_active: listing_form_data.is_active,
                added_at: Utc::now(),
            },
            // When this is a new listing there will be no ListingFormData.Uuid
            Err(_err) => {
                println!("Could not parse uuid");
                Listing {
                    id: Uuid::nil(),
                    name: listing_form_data.name.clone(),
                    description: Some(listing_form_data.description.clone()),
                    listing_structure_id: StructureType::from_str(&listing_form_data.structure)
                        .unwrap()
                        .value(),
                    country: listing_form_data.country.clone(),
                    price_per_night: Some(listing_form_data.price_per_night),
                    is_active: listing_form_data.is_active,
                    added_at: Utc::now(),
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
            println!("Failed to execute insert query: {}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}
