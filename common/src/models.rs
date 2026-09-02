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
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub is_verified: bool,
    pub attributes: Option<serde_json::Value>,
    pub roles: Option<Vec<String>>,
    pub booker_profile: Option<NewBookerProfile>,
    pub host_profile: Option<NewHostProfile>,
    pub default_currency: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, PartialEq)]
pub struct LoginRequest {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 1))]
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct UpdateUserRequest {

    pub email: Option<String>,
    pub password: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub phone_number: Option<String>,
    #[serde(default)]
    pub is_active: Option<bool>,
    #[serde(default)]
    pub is_verified: Option<bool>,
    pub attributes: Option<serde_json::Value>,
    pub roles: Option<Vec<String>>,
    pub booker_profile: Option<NewBookerProfile>,
    pub host_profile: Option<NewHostProfile>,
    pub default_currency: Option<String>,
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
    pub weekly_discount_percentage: Option<Decimal>,
    pub monthly_discount_percentage: Option<Decimal>,
    pub is_active: bool,
    pub added_at: DateTime<Utc>,
    pub owner_name: Option<String>,
    pub primary_image_url: Option<String>,
    pub max_guests: i32,
    pub bedrooms: i32,
    pub beds: i32,
    pub full_bathrooms: i32,
    pub half_bathrooms: i32,
    pub square_meters: Option<i32>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub overall_rating: Option<f64>,
    pub city: Option<String>,
    pub base_currency: String,
    pub slug: String,
    pub listing_details: Option<serde_json::Value>,
    pub minimum_stay: i32,
    pub days_between_bookings: i32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct FeeItem {
    pub name: String,
    #[serde(with = "rust_decimal::serde::float")]
    pub amount: Decimal,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct BookingMetadataResponse {
    pub num_adults: u32,
    pub num_children: u32,
    pub num_infants: u32,
    pub num_pets: u32,
    pub message_to_host: Option<String>,
    pub estimated_arrival_time: Option<String>,
    pub is_business_trip: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct BookingResponse {
    pub id: Uuid,
    pub confirmation_code: String,
    pub guest_id: Uuid,
    pub listing_id: Uuid,
    pub status: String,
    pub date_from: NaiveDate,
    pub date_to: NaiveDate,
    pub currency: String,
    pub daily_rate: Decimal,
    pub number_of_persons: i32,
    pub total_days: i32,
    pub sub_total_price: Decimal,
    pub discount_value: Option<Decimal>,
    pub tax_value: Option<Decimal>,
    pub total_price: Decimal,
    pub cancellation_policy: String,
    pub metadata: BookingMetadataResponse,
    #[serde(default)]
    pub review_eligibility: Option<BookingReviewEligibility>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, PartialEq)]
pub struct ListingImageResponse {
    pub id: Uuid,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, PartialEq)]
pub struct ListingDetails {
    pub listing: ListingResponse,
    pub images: Vec<ListingImageResponse>,
    pub host_name: Option<String>,
    pub rating_summary: Option<ListingRatingSummary>,
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
    pub currency: Option<String>,
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
    pub currency: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, PartialEq)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub phone_number: Option<String>,
    pub is_active: bool,
    pub is_verified: bool,
    pub verification_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub attributes: serde_json::Value,
    pub roles: Vec<String>,
    pub default_currency: String,
    pub deleted_at: Option<DateTime<Utc>>,
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

#[derive(Debug, Serialize, Deserialize, Clone, Validate, ToSchema, Default)]
pub struct UpdatedBookingRequest {
    pub status: Option<String>,
    pub metadata: Option<BookingMetadataResponse>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Validate, ToSchema)]
pub struct TransferBookingRequest {
    #[schema(value_type = String, format = "uuid")]
    pub guest_id: Uuid,
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

    // --- NEW: Capacity & Room Breakdown ---
    #[schema(example = 2)]
    #[validate(range(min = 1, message = "Must allow at least 1 guest"))]
    pub max_guests: i32,

    #[schema(example = 1)]
    #[validate(range(min = 0, message = "Bedrooms cannot be negative"))]
    pub bedrooms: i32,

    #[schema(example = 1)]
    #[validate(range(min = 0, message = "Beds cannot be negative"))]
    pub beds: i32,

    #[schema(example = 1)]
    #[validate(range(min = 0, message = "Bathrooms cannot be negative"))]
    pub full_bathrooms: i32,

    #[serde(default)]
    #[schema(example = 0)]
    #[validate(range(min = 0, message = "Half bathrooms cannot be negative"))]
    pub half_bathrooms: i32,

    // --- NEW: Dimensions & Location ---
    #[serde(default)]
    #[schema(example = 65)]
    pub square_meters: Option<i32>,

    #[serde(default)]
    #[schema(example = 18.2206)]
    pub latitude: Option<f64>,

    #[serde(default)]
    #[schema(example = -77.7990)]
    pub longitude: Option<f64>,

    // --- NEW: Dynamic Property Definitions (JSONB) ---
    #[serde(default)]
    #[schema(value_type = Object)]
    pub listing_details: Option<serde_json::Value>,

    #[serde(default)]
    #[schema(value_type = String, example = "Kingston")]
    pub city: Option<String>,

    #[serde(default = "default_base_currency")]
    #[schema(value_type = String, example = "USD")]
    pub base_currency: String,

    #[serde(default = "default_minimum_stay")]
    #[schema(example = 1)]
    #[validate(range(min = 1, message = "Minimum stay must be at least 1 night"))]
    pub minimum_stay: i32,

    #[serde(default = "default_days_between_bookings")]
    #[schema(example = 0)]
    #[validate(range(min = 0, message = "Days between bookings cannot be negative"))]
    pub days_between_bookings: i32,
}

pub fn default_minimum_stay() -> i32 {
    1
}

pub fn default_days_between_bookings() -> i32 {
    0
}

pub fn default_base_currency() -> String {
    "USD".to_string()
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, Default)]
pub struct UpdatedListingRequest {
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "Zen Loft")]
    #[validate(length(min = 1, message = "Name cannot be empty"))]
    pub name: Option<String>,

    #[serde(default)]
    #[schema(value_type = Option<String>, example = "A zen place to be")]
    #[validate(length(
        max = 2000,
        message = "Description cannot be longer than 2000 characters"
    ))]
    pub description: Option<String>,

    #[serde(default)]
    #[schema(value_type = Option<String>, example = "Apartment")]
    pub listing_structure: Option<String>,

    #[serde(default)]
    #[schema(value_type = Option<String>, example = "Jamaica")]
    #[validate(length(min = 1, message = "Country cannot be empty"))]
    pub country: Option<String>,

    #[serde(default)]
    #[schema(value_type = Option<String>, example = "USD")]
    pub base_currency: Option<String>,

    #[serde(default)]
    #[schema(value_type = Option<String>, example = "150.00")]
    pub price_per_night: Option<Decimal>,

    #[serde(default)]
    pub weekly_discount_percentage: Option<Decimal>,

    #[serde(default)]
    pub monthly_discount_percentage: Option<Decimal>,

    #[serde(default)]
    #[schema(example = 2)]
    #[validate(range(min = 1, message = "Must allow at least 1 guest"))]
    pub max_guests: Option<i32>,

    #[serde(default)]
    #[schema(example = 1)]
    #[validate(range(min = 0, message = "Bedrooms cannot be negative"))]
    pub bedrooms: Option<i32>,

    #[serde(default)]
    #[schema(example = 1)]
    #[validate(range(min = 0, message = "Beds cannot be negative"))]
    pub beds: Option<i32>,

    #[serde(default)]
    #[schema(example = 1)]
    #[validate(range(min = 0, message = "Bathrooms cannot be negative"))]
    pub full_bathrooms: Option<i32>,

    #[serde(default)]
    #[schema(example = 0)]
    #[validate(range(min = 0, message = "Half bathrooms cannot be negative"))]
    pub half_bathrooms: Option<i32>,

    #[serde(default)]
    #[schema(example = 65)]
    pub square_meters: Option<i32>,

    #[serde(default)]
    #[schema(example = 18.2206)]
    pub latitude: Option<f64>,

    #[serde(default)]
    #[schema(example = -77.7990)]
    pub longitude: Option<f64>,

    #[serde(default)]
    #[schema(value_type = String, example = "Kingston")]
    pub city: Option<String>,

    #[serde(default)]
    #[schema(value_type = Object)]
    pub listing_details: Option<serde_json::Value>,

    #[serde(default)]
    #[schema(example = 1)]
    #[validate(range(min = 1, message = "Minimum stay must be at least 1 night"))]
    pub minimum_stay: Option<i32>,

    #[serde(default)]
    #[schema(example = 0)]
    #[validate(range(min = 0, message = "Days between bookings cannot be negative"))]
    pub days_between_bookings: Option<i32>,

    #[serde(default)]
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct PriceOverride {
    pub id: Uuid,
    pub listing_id: Uuid,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub nightly_rate: Decimal,
    pub min_nights: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema, PartialEq, Eq)]
pub struct CreatePriceOverrideRequest {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub nightly_rate: Decimal,
    #[serde(default = "default_minimum_stay")]
    pub min_nights: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema, PartialEq, Eq)]
pub struct UpdatePriceOverrideRequest {
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub nightly_rate: Option<Decimal>,
    pub min_nights: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct NightlyRateBreakdown {
    pub date: NaiveDate,
    pub rate: Decimal,
    pub is_override: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct DynamicPricingQuote {
    pub nightly_breakdown: Vec<NightlyRateBreakdown>,
    pub subtotal: Decimal,
    pub effective_daily_rate: Decimal,
    pub required_min_nights: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema, PartialEq, Eq)]
pub struct NewReviewRequest {
    pub token: String,
    #[validate(range(min = 1, max = 5))]
    pub cleanliness_rating: i32,
    #[validate(range(min = 1, max = 5))]
    pub accuracy_rating: i32,
    #[validate(range(min = 1, max = 5))]
    pub location_rating: i32,
    #[validate(range(min = 1, max = 5))]
    pub value_rating: i32,
    pub public_review_text: Option<String>,
    pub private_host_feedback: Option<String>,
}

impl NewReviewRequest {
    pub fn calculate_overall_rating(&self) -> Decimal {
        let sum = self.cleanliness_rating
            + self.accuracy_rating
            + self.location_rating
            + self.value_rating;
        let avg = Decimal::from(sum) / Decimal::from(4);
        avg.round_dp(2)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema, PartialEq, Eq)]
pub struct HostReplyRequest {
    #[validate(length(min = 1, max = 2000))]
    pub reply_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct ReviewTokenInfoResponse {
    pub is_valid: bool,
    pub listing_name: String,
    pub guest_first_name: String,
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
    pub expires_at: DateTime<Utc>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct ListingRatingSummary {
    pub overall_rating: Option<f64>,
    pub cleanliness_rating: Option<f64>,
    pub accuracy_rating: Option<f64>,
    pub location_rating: Option<f64>,
    pub value_rating: Option<f64>,
    pub review_count: i32,
    pub rating_distribution: std::collections::HashMap<i32, i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct ReviewResponse {
    pub id: Uuid,
    pub guest_first_name: String,
    pub cleanliness_rating: i32,
    pub accuracy_rating: i32,
    pub location_rating: i32,
    pub value_rating: i32,
    pub overall_rating: f64,
    pub public_review_text: Option<String>,
    pub host_reply_text: Option<String>,
    pub host_replied_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct BookingReviewEligibility {
    pub booking_id: Uuid,
    pub is_eligible: bool,
    pub token: Option<String>,
    pub has_reviewed: bool,
    pub days_remaining: Option<i64>,
    pub status_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct CreateSessionRequest {
    pub token_hash: String,
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub namespace: String,
    pub ttl_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct SessionResponse {
    pub token_hash: String,
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub namespace: String,
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: DateTime<Utc>,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct RefreshSessionRequest {
    pub ttl_seconds: Option<i64>,
}


#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    #[test]
    fn test_overall_rating_calculation() {
        let req1 = NewReviewRequest {
            token: "xyz".to_string(),
            cleanliness_rating: 5,
            accuracy_rating: 4,
            location_rating: 5,
            value_rating: 4,
            public_review_text: None,
            private_host_feedback: None,
        };
        assert_eq!(
            req1.calculate_overall_rating(),
            Decimal::from_str("4.50").unwrap()
        );

        let req2 = NewReviewRequest {
            token: "xyz".to_string(),
            cleanliness_rating: 5,
            accuracy_rating: 5,
            location_rating: 4,
            value_rating: 5,
            public_review_text: None,
            private_host_feedback: None,
        };
        assert_eq!(
            req2.calculate_overall_rating(),
            Decimal::from_str("4.75").unwrap()
        );

        let req3 = NewReviewRequest {
            token: "xyz".to_string(),
            cleanliness_rating: 3,
            accuracy_rating: 3,
            location_rating: 3,
            value_rating: 4,
            public_review_text: None,
            private_host_feedback: None,
        };
        assert_eq!(
            req3.calculate_overall_rating(),
            Decimal::from_str("3.25").unwrap()
        );
    }

    #[test]
    fn test_booking_response_serde_with_eligibility() {
        let json_with_eligibility = r#"{
            "id": "00000000-0000-0000-0000-000000000000",
            "confirmation_code": "ABCDEF",
            "guest_id": "00000000-0000-0000-0000-000000000000",
            "listing_id": "00000000-0000-0000-0000-000000000000",
            "status": "Completed",
            "date_from": "2026-08-01",
            "date_to": "2026-08-05",
            "currency": "USD",
            "daily_rate": "100.00",
            "number_of_persons": 2,
            "total_days": 4,
            "sub_total_price": "400.00",
            "discount_value": null,
            "tax_value": "40.00",
            "total_price": "440.00",
            "cancellation_policy": "Flexible",
            "metadata": {
                "num_adults": 2,
                "num_children": 0,
                "num_infants": 0,
                "num_pets": 0,
                "message_to_host": null,
                "estimated_arrival_time": null,
                "is_business_trip": false
            },
            "review_eligibility": {
                "booking_id": "00000000-0000-0000-0000-000000000000",
                "is_eligible": true,
                "token": "tok123",
                "has_reviewed": false,
                "days_remaining": 10,
                "status_message": "Eligible for review"
            },
            "created_at": "2026-08-01T12:00:00Z",
            "updated_at": "2026-08-01T12:00:00Z"
        }"#;

        let response: crate::models::BookingResponse =
            serde_json::from_str(json_with_eligibility).unwrap();
        assert!(response.review_eligibility.is_some());
        let eligibility = response.review_eligibility.unwrap();
        assert_eq!(eligibility.token.as_deref(), Some("tok123"));
        assert!(eligibility.is_eligible);

        let json_without_eligibility = r#"{
            "id": "00000000-0000-0000-0000-000000000000",
            "confirmation_code": "ABCDEF",
            "guest_id": "00000000-0000-0000-0000-000000000000",
            "listing_id": "00000000-0000-0000-0000-000000000000",
            "status": "Completed",
            "date_from": "2026-08-01",
            "date_to": "2026-08-05",
            "currency": "USD",
            "daily_rate": "100.00",
            "number_of_persons": 2,
            "total_days": 4,
            "sub_total_price": "400.00",
            "discount_value": null,
            "tax_value": "40.00",
            "total_price": "440.00",
            "cancellation_policy": "Flexible",
            "metadata": {
                "num_adults": 2,
                "num_children": 0,
                "num_infants": 0,
                "num_pets": 0,
                "message_to_host": null,
                "estimated_arrival_time": null,
                "is_business_trip": false
            },
            "review_eligibility": null,
            "created_at": "2026-08-01T12:00:00Z",
            "updated_at": "2026-08-01T12:00:00Z"
        }"#;

        let response2: crate::models::BookingResponse =
            serde_json::from_str(json_without_eligibility).unwrap();
        assert!(response2.review_eligibility.is_none());
    }
}
