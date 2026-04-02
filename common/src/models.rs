use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct NewBookerProfile {
    pub emergency_contacts: Option<serde_json::Value>,
    pub booking_preferences: Option<serde_json::Value>,
    pub loyalty: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct NewHostProfile {
    pub verified_status: Option<String>,
    pub payout_details: Option<serde_json::Value>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, PartialEq)]
pub struct NewUserRequest {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 8))]
    pub password: String,
    #[validate(length(min = 1))]
    pub first_name: String,
    #[validate(length(min = 1))]
    pub last_name: String,
    pub phone_number: Option<String>,
    pub is_active: bool,
    pub attributes: Option<serde_json::Value>,
    pub roles: Option<Vec<String>>,
    pub booker_profile: Option<NewBookerProfile>,
    pub host_profile: Option<NewHostProfile>,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct UpdateUserRequest {
    pub email: Option<String>,
    pub password: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub phone_number: Option<String>,
    pub is_active: Option<bool>,
    pub attributes: Option<serde_json::Value>,
    pub roles: Option<Vec<String>>,
    pub booker_profile: Option<NewBookerProfile>,
    pub host_profile: Option<NewHostProfile>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, PartialEq)]
pub struct ListingResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub listing_structure: String, // Simplified from enum for common compatibility if needed, or move enum here
    pub country: String,
    pub price_per_night: Option<Decimal>,
    pub is_active: bool,
    pub added_at: DateTime<Utc>,
    pub owner_name: Option<String>,
    pub primary_image_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, PartialEq)]
pub struct ListingImageResponse {
    pub id: Uuid,
    pub url: String,
}

#[derive(Debug, Deserialize, Serialize, IntoParams, ToSchema, Clone)]
pub struct ListingFilter {
    pub name: Option<String>,
    pub country: Option<String>,
    pub min_price: Option<Decimal>,
    pub max_price: Option<Decimal>,
    #[serde(default)]
    pub structure_type: Vec<String>,
    pub owner: Option<String>,
    pub resolution: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, IntoParams, ToSchema, Clone)]
pub struct ListingQueryParams {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub name: Option<String>,
    pub country: Option<String>,
    pub min_price: Option<Decimal>,
    pub max_price: Option<Decimal>,
    #[serde(default, skip_deserializing)]
    pub structure_type: Vec<String>,
    pub owner: Option<String>,
    pub resolution: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, PartialEq)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub phone_number: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub attributes: serde_json::Value,
    pub roles: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct UsersWrapper {
    pub user: Vec<UserResponse>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct ImagePresignRequest {
    pub images: Vec<PendingImageMetadata>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct PendingImageMetadata {
    pub client_file_id: String, // Added to map the file UI-side
    pub content_type: String,
    pub size_bytes: u64,
    pub display_order: i32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct ImagePresignResponse {
    pub client_file_id: String, // Mirrored back to the client
    pub file_id: uuid::Uuid,
    pub upload_url: String, // The GCS v4 Signed URL
}

#[derive(Debug, Serialize, Deserialize, Clone, Validate, ToSchema)]
pub struct NewBookingRequest {
    pub guest_id: Uuid,
    pub listing_id: Uuid,

    pub check_in: NaiveDate,
    pub check_out: NaiveDate,

    pub num_adults: u32,
    pub num_children: u32,
    pub num_infants: u32,
    pub num_pets: u32,

    // Host communication and logistics
    pub message_to_host: Option<String>,
    pub estimated_arrival_time: Option<String>,
    pub is_business_trip: bool,

    pub currency: String,

    pub agreed_cancellation_policy: String,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
pub struct NewListingRequest {
    #[schema(value_type = String, example = "Zen Loft")]
    #[validate(length(min = 1, message = "Name cannot be empty"))]
    pub name: String,

    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,

    #[serde(default)]
    #[schema(value_type = String, example = "A zen place to be")]
    #[validate(length(
        max = 2000,
        message = "Description cannot be longer than 2000 characters"
    ))]
    pub description: Option<String>,

    #[schema(value_type = String, example = "Apartment")]
    pub listing_structure: String,

    #[serde(default)]
    #[schema(value_type = String, example = "Jamaica")]
    #[validate(length(min = 1, message = "Country cannot be empty"))]
    pub country: String,

    #[serde(default)]
    #[schema(value_type = String, example = "150.00")]
    pub price_per_night: Option<Decimal>,

    #[serde(default)]
    pub weekly_discount_percentage: Option<Decimal>,

    #[serde(default)]
    pub monthly_discount_percentage: Option<Decimal>,
}

