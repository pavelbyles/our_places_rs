use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

pub use crate::payout::*;

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
    /// User's preferred default display currency.
    pub default_currency: Option<String>,
}

/// DTO representing a property listing in API responses.
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, PartialEq)]
pub struct ListingResponse {
    /// Unique listing identifier.
    pub id: Uuid,
    /// Owner user identifier.
    pub user_id: Uuid,
    /// Listing display title.
    pub name: String,
    /// Detailed description of the property.
    pub description: Option<String>,
    /// Architectural structure type (e.g. villa, apartment).
    pub listing_structure: String,
    /// Country where property is located.
    pub country: String,
    /// Standard base price per night.
    pub price_per_night: Option<Decimal>,
    /// Discount percentage for stays >= 7 nights.
    pub weekly_discount_percentage: Option<Decimal>,
    /// Discount percentage for stays >= 28 nights.
    pub monthly_discount_percentage: Option<Decimal>,
    /// Whether the listing is active and bookable.
    pub is_active: bool,
    /// Timestamp when listing was added.
    pub added_at: DateTime<Utc>,
    /// Display name of the owner/host.
    pub owner_name: Option<String>,
    /// Primary hero image URL.
    pub primary_image_url: Option<String>,
    /// Maximum allowed guest count.
    pub max_guests: i32,
    /// Number of bedrooms.
    pub bedrooms: i32,
    /// Number of beds.
    pub beds: i32,
    /// Number of full bathrooms.
    pub full_bathrooms: i32,
    /// Number of half bathrooms (powder rooms).
    pub half_bathrooms: i32,
    /// Property size in square meters.
    pub square_meters: Option<i32>,
    /// Geographic latitude.
    pub latitude: Option<f64>,
    /// Geographic longitude.
    pub longitude: Option<f64>,
    /// Aggregated overall star rating.
    pub overall_rating: Option<f64>,
    /// City or locality.
    pub city: Option<String>,
    /// Base pricing currency code (e.g. USD).
    pub base_currency: String,
    /// SEO-friendly URL slug.
    pub slug: String,
    /// Structured listing metadata including amenities and house rules.
    pub listing_details: Option<serde_json::Value>,
    /// Minimum required stay in nights.
    pub minimum_stay: i32,
    /// Buffer days required between consecutive bookings.
    pub days_between_bookings: i32,
    /// Platform commission percentage.
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "0.1000")]
    pub commission_pct: Option<Decimal>,
}

/// Itemized fee line item in booking price breakdowns.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct FeeItem {
    /// Descriptive name of the fee.
    pub name: String,
    /// Monetary amount of the fee.
    #[serde(with = "rust_decimal::serde::float")]
    pub amount: Decimal,
}

/// Structured guest count and arrival metadata for bookings.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq, Default)]
#[serde(default)]
pub struct BookingMetadataResponse {
    /// Number of adults.
    pub num_adults: u32,
    /// Number of children.
    pub num_children: u32,
    /// Number of infants.
    pub num_infants: u32,
    /// Number of pets.
    pub num_pets: u32,
    /// Optional guest message to host.
    pub message_to_host: Option<String>,
    /// Estimated arrival time.
    pub estimated_arrival_time: Option<String>,
    /// Whether this reservation is for business travel.
    pub is_business_trip: bool,
}

/// DTO representing a booking reservation in API responses.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct BookingResponse {
    /// Unique booking identifier.
    pub id: Uuid,
    /// Confirmation code.
    pub confirmation_code: String,
    /// Guest user identifier.
    pub guest_id: Uuid,
    /// Property listing identifier.
    pub listing_id: Uuid,
    /// Current status string.
    pub status: String,
    /// Check-in start date.
    pub date_from: NaiveDate,
    /// Check-out departure date.
    pub date_to: NaiveDate,
    /// Payment currency.
    pub currency: String,
    /// Nightly rate.
    pub daily_rate: Decimal,
    /// Number of guests.
    pub number_of_persons: i32,
    /// Duration in days.
    pub total_days: i32,
    /// Subtotal price.
    pub sub_total_price: Decimal,
    /// Applied discount amount if any.
    pub discount_value: Option<Decimal>,
    /// Applied tax amount if any.
    pub tax_value: Option<Decimal>,
    /// Total price.
    pub total_price: Decimal,
    /// Cancellation policy name.
    pub cancellation_policy: String,
    /// Additional arrival metadata.
    pub metadata: BookingMetadataResponse,
    /// Review submission eligibility if completed.
    #[serde(default)]
    pub review_eligibility: Option<BookingReviewEligibility>,
    /// Smart door access code if available.
    #[serde(default)]
    pub door_access_code: Option<String>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// Image asset metadata attached to a listing.
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, PartialEq)]
pub struct ListingImageResponse {
    /// Image asset identifier.
    pub id: Uuid,
    /// Public image URL.
    pub url: String,
}

/// Detailed listing view incorporating images and host information.
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, PartialEq)]
pub struct ListingDetails {
    /// Listing core fields.
    pub listing: ListingResponse,
    /// Attached photo gallery images.
    pub images: Vec<ListingImageResponse>,
    /// Owner host display name.
    pub host_name: Option<String>,
    /// Aggregated reviews and ratings summary.
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

    /// Optional guest message to host.
    pub message_to_host: Option<String>,
    /// Estimated time of arrival.
    pub estimated_arrival_time: Option<String>,
    /// Whether this booking is for business purposes.
    pub is_business_trip: bool,

    /// Selected checkout currency.
    pub currency: String,

    /// Name of agreed cancellation policy.
    pub agreed_cancellation_policy: String,
}

/// Request payload to update an existing booking reservation.
#[derive(Debug, Serialize, Deserialize, Clone, Validate, ToSchema, Default)]
pub struct UpdatedBookingRequest {
    /// New lifecycle status if updating.
    pub status: Option<String>,
    /// Updated arrival or guest metadata.
    pub metadata: Option<BookingMetadataResponse>,
    /// Smart lock door access code.
    pub door_access_code: Option<String>,
}

/// Request payload to transfer a reservation to a promoted user account.
#[derive(Debug, Serialize, Deserialize, Clone, Validate, ToSchema)]
pub struct TransferBookingRequest {
    /// Recipient user identifier.
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

    #[serde(default)]
    #[schema(value_type = String, example = "0.1000")]
    pub commission_pct: Option<Decimal>,
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

    #[serde(default)]
    #[schema(value_type = Option<String>, example = "0.1000")]
    pub commission_pct: Option<Decimal>,
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

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageSenderRole {
    Guest,
    Host,
    Admin,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, PartialEq)]
pub struct CreateBookingMessageRequest {
    #[validate(length(
        min = 1,
        max = 2000,
        message = "Message must be between 1 and 2000 characters"
    ))]
    pub message_text: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct BookingMessageResponse {
    pub id: Uuid,
    pub booking_id: Uuid,
    pub sender_id: Uuid,
    pub sender_role: MessageSenderRole,
    pub sender_name: String,
    pub message_text: String,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct BookingMessagesWrapper {
    pub messages: Vec<BookingMessageResponse>,
    pub unread_count: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct MarkMessagesReadResponse {
    pub updated_count: u64,
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
    fn test_booking_message_validation() {
        let valid = CreateBookingMessageRequest {
            message_text: "Hello, host!".to_string(),
        };
        assert!(valid.validate().is_ok());

        let too_short = CreateBookingMessageRequest {
            message_text: "".to_string(),
        };
        assert!(too_short.validate().is_err());

        let too_long = CreateBookingMessageRequest {
            message_text: "a".repeat(2001),
        };
        assert!(too_long.validate().is_err());
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
        assert!(response2.door_access_code.is_none());
    }

    #[test]
    fn test_is_material_booking_change() {
        use chrono::NaiveDate;
        use rust_decimal::Decimal;

        let d1 = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
        let d3 = NaiveDate::from_ymd_opt(2026, 11, 6).unwrap();

        let price1 = Decimal::new(1000, 0);
        let price2 = Decimal::new(1200, 0);

        let base_terms = BookingMaterialTerms {
            date_from: d1,
            date_to: d2,
            number_of_persons: 2,
            total_price: price1,
        };

        // No change
        assert!(!is_material_booking_change(&base_terms, &base_terms));

        // Date changed
        let date_changed = BookingMaterialTerms {
            date_to: d3,
            ..base_terms
        };
        assert!(is_material_booking_change(&base_terms, &date_changed));

        // Party size changed
        let guests_changed = BookingMaterialTerms {
            number_of_persons: 3,
            ..base_terms
        };
        assert!(is_material_booking_change(&base_terms, &guests_changed));

        // Price changed
        let price_changed = BookingMaterialTerms {
            total_price: price2,
            ..base_terms
        };
        assert!(is_material_booking_change(&base_terms, &price_changed));
    }
}

/// Key material terms of a booking used to evaluate whether changes require transactional notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookingMaterialTerms {
    /// Booking arrival date.
    pub date_from: chrono::NaiveDate,
    /// Booking departure date.
    pub date_to: chrono::NaiveDate,
    /// Number of guests.
    pub number_of_persons: i32,
    /// Total price.
    pub total_price: rust_decimal::Decimal,
}

impl BookingMaterialTerms {
    /// Returns true if this instance differs in dates, party size, or total price from another.
    pub fn is_materially_different(&self, other: &Self) -> bool {
        self != other
    }
}

/// Determines if a booking change is material (requiring transactional notification to both guest and host).
/// Material changes include modifications to:
/// - Dates (`date_from`, `date_to`)
/// - Party size (number of guests / adults + children)
/// - Total price
pub fn is_material_booking_change(
    old_terms: &BookingMaterialTerms,
    new_terms: &BookingMaterialTerms,
) -> bool {
    old_terms.is_materially_different(new_terms)
}
