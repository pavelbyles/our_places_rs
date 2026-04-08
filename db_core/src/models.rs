use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::types::Json;
use strum_macros::EnumString;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BookingMetadata {
    pub num_adults: u32,
    pub num_children: u32,
    pub num_infants: u32,
    pub num_pets: u32,
    pub message_to_host: Option<String>,
    pub estimated_arrival_time: Option<String>,
    pub is_business_trip: bool,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    // Typically we don't return password_hash in the API model, but for the DB model it's fine.
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub first_name: String,
    pub last_name: String,
    pub phone_number: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub attributes: serde_json::Value,
    pub roles: Vec<UserRole>,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct BookerProfile {
    pub user_id: Uuid,
    pub emergency_contacts: Option<serde_json::Value>,
    pub booking_preferences: Option<serde_json::Value>,
    pub loyalty: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct HostProfile {
    pub user_id: Uuid,
    pub verified_status: Option<String>,
    pub payout_details: Option<serde_json::Value>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct NewUser {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub first_name: String,
    pub last_name: String,
    pub phone_number: Option<String>,
    pub is_active: bool,
    pub attributes: serde_json::Value,
    pub roles: Option<Vec<UserRole>>,
}

pub use common::models::{NewBookerProfile, NewHostProfile};

#[derive(Debug)]
pub struct UpdatedUser {
    pub email: Option<String>,
    pub password_hash: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub phone_number: Option<String>,
    pub is_active: Option<bool>,
    pub attributes: Option<serde_json::Value>,
    pub roles: Option<Vec<UserRole>>,
}

#[derive(
    Debug, Serialize, Deserialize, sqlx::Type, ToSchema, Clone, Copy, PartialEq, EnumString,
)]
#[sqlx(type_name = "user_role", rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum UserRole {
    Booker,
    Host,
    Admin,
}

#[derive(Debug, Serialize, Deserialize, sqlx::Type, ToSchema, Clone, Copy, PartialEq)]
#[sqlx(type_name = "user_status", rename_all = "lowercase")]
pub enum UserStatus {
    Active,
    Inactive,
}

#[derive(Debug, sqlx::FromRow, Serialize, Deserialize)]
pub struct Booking {
    pub id: Uuid,
    pub confirmation_code: String,
    pub guest_id: Uuid,
    pub listing_id: Uuid,
    pub status: BookingStatus,

    pub date_from: NaiveDate,
    pub date_to: NaiveDate,

    pub currency: String,
    pub daily_rate: Decimal,
    pub number_of_persons: i32,
    pub total_days: i32,

    pub sub_total_price: Decimal,
    pub discount_value: Option<Decimal>,
    pub tax_value: Option<Decimal>,

    pub fee_breakdown: Json<Vec<FeeItem>>,

    pub total_price: Decimal,
    pub cancellation_policy: CancellationPolicy,
    pub metadata: Json<BookingMetadata>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::Type, ToSchema, Clone, Copy, PartialEq)]
#[sqlx(type_name = "booking_status", rename_all = "lowercase")]
pub enum BookingStatus {
    Pending,
    Confirmed,
    Cancelled,
    Completed,
}

#[derive(Debug, Deserialize, Serialize, Validate)]
pub struct NewBooking {
    pub confirmation_code: String,
    pub guest_id: Uuid,
    pub listing_id: Uuid,
    pub date_from: NaiveDate,
    pub date_to: NaiveDate,
    pub currency: String,
    pub daily_rate: Decimal,
    pub number_of_persons: i32,
    pub total_days: i32,
    pub sub_total_price: Decimal,
    pub discount_value: Option<Decimal>,
    pub tax_value: Option<Decimal>,
    pub fee_breakdown: Vec<FeeItem>,
    pub total_price: Decimal,
    pub cancellation_policy: CancellationPolicy,
    pub metadata: BookingMetadata,
}

#[derive(Debug, Deserialize, Serialize, Validate)]
pub struct UpdatedBooking {
    pub status: Option<BookingStatus>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::Type, ToSchema, Clone, Copy, PartialEq)]
#[sqlx(type_name = "cancellation_policy", rename_all = "lowercase")]
pub enum CancellationPolicy {
    Flexible,
    Moderate,
    Strict,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct FeeItem {
    pub name: String,
    #[serde(with = "rust_decimal::serde::float")]
    pub amount: Decimal,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Listing {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub listing_structure_id: i32,
    pub country: String,
    pub price_per_night: Option<Decimal>,
    pub is_active: bool,
    pub added_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub primary_image_url: Option<String>,
    pub weekly_discount_percentage: Option<Decimal>,
    pub monthly_discount_percentage: Option<Decimal>,
    pub slug: String,
    pub max_guests: i32,
    pub bedrooms: i32,
    pub beds: i32,
    pub full_bathrooms: i32,
    pub half_bathrooms: i32,
    pub square_meters: Option<i32>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub overall_rating: Option<f64>,
    pub review_count: i32,
    pub listing_details: Json<serde_json::Value>,
    pub city: Option<String>,
    pub base_currency: String,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct ListingWithOwner {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub listing_structure_id: i32,
    pub country: String,
    pub price_per_night: Option<Decimal>,
    pub is_active: bool,
    pub added_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub owner_name: Option<String>,
    pub primary_image_url: Option<String>,
    pub weekly_discount_percentage: Option<Decimal>,
    pub monthly_discount_percentage: Option<Decimal>,
    pub max_guests: i32,
    pub bedrooms: i32,
    pub full_bathrooms: i32,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub overall_rating: Option<f64>,
    pub city: Option<String>,
    pub base_currency: String,
}

#[derive(Debug, Deserialize, Serialize, Validate)]
pub struct NewListing {
    pub user_id: Uuid,
    #[validate(length(min = 1, message = "Name cannot be empty"))]
    pub name: String,

    #[validate(length(
        max = 2000,
        message = "Description cannot be longer than 2000 characters"
    ))]
    pub description: Option<String>,

    #[validate(range(min = 1, message = "Invalid listing structure ID"))]
    pub listing_structure_id: i32,

    #[validate(length(min = 1, message = "Country cannot be empty"))]
    pub country: String,
    pub price_per_night: Option<Decimal>,
    pub weekly_discount_percentage: Option<Decimal>,
    pub monthly_discount_percentage: Option<Decimal>,
    pub max_guests: i32,
    pub bedrooms: i32,
    pub beds: i32,
    pub full_bathrooms: i32,
    pub half_bathrooms: i32,
    pub square_meters: Option<i32>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub listing_details: Option<serde_json::Value>,
    pub city: Option<String>,
    pub base_currency: String,
}

#[derive(Debug, Deserialize, Serialize, Validate)]
pub struct UpdatedListing {
    #[validate(length(min = 1, message = "Name cannot be empty"))]
    pub name: Option<String>,

    #[validate(length(
        max = 2000,
        message = "Description cannot be longer than 2000 characters"
    ))]
    pub description: Option<String>,

    #[validate(range(min = 1, message = "Invalid listing structure ID"))]
    pub listing_structure_id: Option<i32>,

    #[validate(length(min = 1, message = "Country cannot be empty"))]
    pub country: Option<String>,

    pub price_per_night: Option<Decimal>,

    pub is_active: Option<bool>,

    pub weekly_discount_percentage: Option<Decimal>,
    pub monthly_discount_percentage: Option<Decimal>,
    pub max_guests: Option<i32>,
    pub bedrooms: Option<i32>,
    pub beds: Option<i32>,
    pub full_bathrooms: Option<i32>,
    pub half_bathrooms: Option<i32>,
    pub square_meters: Option<i32>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub listing_details: Option<serde_json::Value>,
    pub city: Option<String>,
    pub base_currency: Option<String>,
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
    pub fn id(&self) -> i32 {
        match self {
            StructureType::Apartment => 1,
            StructureType::House => 2,
            StructureType::Townhouse => 3,
            StructureType::Studio => 4,
            StructureType::Villa => 5,
        }
    }
}

#[derive(
    Debug, Serialize, Deserialize, sqlx::Type, ToSchema, Clone, Copy, PartialEq, EnumString,
)]
#[sqlx(type_name = "image_status", rename_all = "PascalCase")]
pub enum ImageStatus {
    PendingUpload,
    Uploaded,
    Processing,
    Processed,
    Failed,
}

#[derive(
    Debug, Serialize, Deserialize, sqlx::Type, ToSchema, Clone, Copy, PartialEq, EnumString,
)]
#[sqlx(type_name = "image_resolution", rename_all = "PascalCase")]
pub enum ImageResolution {
    Raw,
    Thumbnail400w,
    Mobile720w,
    Tablet1280w,
    Desktop1920w,
    HighRes2560w,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct ListingImage {
    pub id: Uuid,
    pub listing_id: Uuid,
    pub client_file_id: String,
    pub status: ImageStatus,
    pub resolution: ImageResolution,
    pub parent_id: Option<Uuid>,
    pub upload_url: Option<String>,
    pub content_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub display_order: i32,
    pub is_primary: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
