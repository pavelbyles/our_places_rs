use actix_web::{web, HttpResponse};
use chrono::Utc;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{prelude::FromRow, PgPool};
use std::str::FromStr;
use strum_macros::EnumString;
use uuid::Uuid;

#[derive(Deserialize, Serialize, Debug, FromRow)]
pub struct Listing {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub structure_type_id: i32,
    pub country: String,
    pub price_per_night: Decimal,
    pub is_active: bool,
    pub added_at: i64,
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
                structure_type_id: StructureType::from_str(&listing_form_data.structure)
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
                    structure_type_id: StructureType::from_str(&listing_form_data.structure)
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

    match sqlx::query!(
        r#"
        INSERT INTO listing (id, name, description, listing_structure_id, country, price_per_night, is_active, added_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
        Uuid::new_v4(),
        new_listing.name,
        new_listing.description,
        new_listing.structure_type_id,
        new_listing.country,
        new_listing.price_per_night,
        new_listing.is_active,
        Utc::now()
    )
    .execute(db_pool.get_ref())
    .await
    {
        Ok(res) => {
            println!("Created {0} new listing(s)", res.rows_affected());
            HttpResponse::Ok().finish()
        },
        Err(e) => {
            println!("Failed to execute query: {}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}
