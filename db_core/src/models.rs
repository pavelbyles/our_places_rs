use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::types::Json;
use strum_macros::EnumString;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, Default)]
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
    pub is_verified: bool,
    pub verification_code: Option<String>,
    pub verification_code_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub attributes: serde_json::Value,
    pub roles: Vec<UserRole>,
    pub default_currency: String,
    pub deleted_at: Option<DateTime<Utc>>,
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
    pub is_verified: bool,
    pub verification_code: Option<String>,
    pub verification_code_expires_at: Option<DateTime<Utc>>,
    pub attributes: serde_json::Value,
    pub roles: Option<Vec<UserRole>>,
    pub default_currency: String,
}

pub use common::models::{NewBookerProfile, NewHostProfile};

#[derive(Debug, Default, Clone)]
pub struct UpdatedUser {
    pub email: Option<String>,
    pub password_hash: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub phone_number: Option<String>,
    pub is_active: Option<bool>,
    pub is_verified: Option<bool>,
    pub verification_code: Option<String>,
    pub verification_code_expires_at: Option<DateTime<Utc>>,
    pub attributes: Option<serde_json::Value>,
    pub roles: Option<Vec<UserRole>>,
    pub default_currency: Option<String>,
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

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserRole::Booker => write!(f, "Booker"),
            UserRole::Host => write!(f, "Host"),
            UserRole::Admin => write!(f, "Admin"),
        }
    }
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

pub struct BookingWithEligibility {
    pub booking: Booking,
    pub review_eligibility: Option<common::models::BookingReviewEligibility>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::Type, ToSchema, Clone, Copy, PartialEq)]
#[sqlx(type_name = "booking_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
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
    pub metadata: Option<BookingMetadata>,
}

#[derive(Debug, FromRow, Serialize, Deserialize, Clone)]
pub struct BookingHistory {
    pub id: Uuid,
    pub booking_id: Uuid,

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

    pub changed_by_id: Option<Uuid>,
    pub change_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(
    Debug, Serialize, Deserialize, sqlx::Type, ToSchema, Clone, Copy, PartialEq, EnumString,
)]
#[sqlx(type_name = "cancellation_policy", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum CancellationPolicy {
    Flexible,
    Moderate,
    Strict,
}

pub use common::models::FeeItem;

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
    pub minimum_stay: i32,
    pub days_between_bookings: i32,
    pub commission_pct: Decimal,
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
    pub listing_details: Json<serde_json::Value>,
    pub minimum_stay: i32,
    pub days_between_bookings: i32,
    pub commission_pct: Decimal,
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
    pub minimum_stay: i32,
    pub days_between_bookings: i32,
    pub commission_pct: Option<Decimal>,
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
    pub minimum_stay: Option<i32>,
    pub days_between_bookings: Option<i32>,
    pub commission_pct: Option<Decimal>,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ListingDetails {
    pub listing: Listing,
    pub images: Vec<ListingImage>,
    pub owner_name: Option<String>,
    pub rating_summary: Option<common::models::ListingRatingSummary>,
}

#[derive(Debug, FromRow, Serialize, Deserialize, Clone, PartialEq, Eq)]
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

impl From<PriceOverride> for common::models::PriceOverride {
    fn from(p: PriceOverride) -> Self {
        common::models::PriceOverride {
            id: p.id,
            listing_id: p.listing_id,
            start_date: p.start_date,
            end_date: p.end_date,
            nightly_rate: p.nightly_rate,
            min_nights: p.min_nights,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

impl From<common::models::PriceOverride> for PriceOverride {
    fn from(p: common::models::PriceOverride) -> Self {
        PriceOverride {
            id: p.id,
            listing_id: p.listing_id,
            start_date: p.start_date,
            end_date: p.end_date,
            nightly_rate: p.nightly_rate,
            min_nights: p.min_nights,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct ReviewToken {
    pub id: Uuid,
    pub token: String,
    pub booking_id: Uuid,
    pub guest_id: Uuid,
    pub listing_id: Uuid,
    pub valid_from: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Review {
    pub id: Uuid,
    pub booking_id: Uuid,
    pub listing_id: Uuid,
    pub guest_id: Uuid,
    pub cleanliness_rating: i32,
    pub accuracy_rating: i32,
    pub location_rating: i32,
    pub value_rating: i32,
    pub overall_rating: Decimal,
    pub public_review_text: Option<String>,
    pub private_host_feedback: Option<String>,
    pub host_reply_text: Option<String>,
    pub host_replied_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::Type, Clone, Copy, PartialEq, Eq, EnumString)]
#[sqlx(type_name = "message_sender_role", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum DbMessageSenderRole {
    Guest,
    Host,
    Admin,
}

impl From<DbMessageSenderRole> for common::models::MessageSenderRole {
    fn from(r: DbMessageSenderRole) -> Self {
        match r {
            DbMessageSenderRole::Guest => common::models::MessageSenderRole::Guest,
            DbMessageSenderRole::Host => common::models::MessageSenderRole::Host,
            DbMessageSenderRole::Admin => common::models::MessageSenderRole::Admin,
        }
    }
}

impl From<common::models::MessageSenderRole> for DbMessageSenderRole {
    fn from(r: common::models::MessageSenderRole) -> Self {
        match r {
            common::models::MessageSenderRole::Guest => DbMessageSenderRole::Guest,
            common::models::MessageSenderRole::Host => DbMessageSenderRole::Host,
            common::models::MessageSenderRole::Admin => DbMessageSenderRole::Admin,
        }
    }
}

#[derive(Debug, sqlx::FromRow, Serialize, Deserialize)]
pub struct DbBookingMessage {
    pub id: Uuid,
    pub booking_id: Uuid,
    pub sender_id: Uuid,
    pub sender_role: DbMessageSenderRole,
    pub sender_name: String,
    pub message_text: String,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<DbBookingMessage> for common::models::BookingMessageResponse {
    fn from(msg: DbBookingMessage) -> Self {
        common::models::BookingMessageResponse {
            id: msg.id,
            booking_id: msg.booking_id,
            sender_id: msg.sender_id,
            sender_role: msg.sender_role.into(),
            sender_name: msg.sender_name,
            message_text: msg.message_text,
            read_at: msg.read_at,
            created_at: msg.created_at,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct BookingParties {
    pub guest_id: Uuid,
    pub host_id: Uuid,
    pub status: BookingStatus,
}

#[derive(Debug, Serialize, Deserialize, sqlx::Type, Clone, Copy, PartialEq, Eq, EnumString)]
#[sqlx(type_name = "payout_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum DbPayoutStatus {
    Pending,
    Processing,
    Paid,
    Cancelled,
    Refunded,
}

impl From<DbPayoutStatus> for common::payout::PayoutStatus {
    fn from(s: DbPayoutStatus) -> Self {
        match s {
            DbPayoutStatus::Pending => common::payout::PayoutStatus::Pending,
            DbPayoutStatus::Processing => common::payout::PayoutStatus::Processing,
            DbPayoutStatus::Paid => common::payout::PayoutStatus::Paid,
            DbPayoutStatus::Cancelled => common::payout::PayoutStatus::Cancelled,
            DbPayoutStatus::Refunded => common::payout::PayoutStatus::Refunded,
        }
    }
}

impl From<common::payout::PayoutStatus> for DbPayoutStatus {
    fn from(s: common::payout::PayoutStatus) -> Self {
        match s {
            common::payout::PayoutStatus::Pending => DbPayoutStatus::Pending,
            common::payout::PayoutStatus::Processing => DbPayoutStatus::Processing,
            common::payout::PayoutStatus::Paid => DbPayoutStatus::Paid,
            common::payout::PayoutStatus::Cancelled => DbPayoutStatus::Cancelled,
            common::payout::PayoutStatus::Refunded => DbPayoutStatus::Refunded,
        }
    }
}

#[derive(Debug, sqlx::FromRow, Serialize, Deserialize)]
pub struct HostPayoutLedger {
    pub id: Uuid,
    pub booking_id: Uuid,
    pub listing_id: Uuid,
    pub host_id: Uuid,
    pub currency: String,
    pub gross_amount: Decimal,
    pub platform_fee_pct: Decimal,
    pub platform_fee_amount: Decimal,
    pub tax_withheld_amount: Decimal,
    pub exchange_rate: Decimal,
    pub net_payout_amount: Decimal,
    pub status: DbPayoutStatus,
    pub gateway_reference: Option<String>,
    pub failure_reason: Option<String>,
    pub payout_date: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow, Serialize, Deserialize)]
pub struct DbPayoutLedgerEntry {
    pub id: Uuid,
    pub booking_id: Uuid,
    pub booking_confirmation_code: Option<String>,
    pub listing_id: Uuid,
    pub listing_name: Option<String>,
    pub host_id: Uuid,
    pub host_name: Option<String>,
    pub check_in_date: Option<NaiveDate>,
    pub check_out_date: Option<NaiveDate>,
    pub currency: String,
    pub gross_amount: Decimal,
    pub platform_fee_pct: Decimal,
    pub platform_fee_amount: Decimal,
    pub tax_withheld_amount: Decimal,
    pub exchange_rate: Decimal,
    pub net_payout_amount: Decimal,
    pub status: DbPayoutStatus,
    pub gateway_reference: Option<String>,
    pub failure_reason: Option<String>,
    pub payout_date: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<DbPayoutLedgerEntry> for common::payout::PayoutLedgerEntry {
    fn from(e: DbPayoutLedgerEntry) -> Self {
        common::payout::PayoutLedgerEntry {
            id: e.id,
            booking_id: e.booking_id,
            booking_confirmation_code: e.booking_confirmation_code,
            listing_id: e.listing_id,
            listing_name: e.listing_name,
            host_id: e.host_id,
            host_name: e.host_name,
            check_in_date: e.check_in_date,
            check_out_date: e.check_out_date,
            currency: e.currency,
            gross_amount: e.gross_amount,
            platform_fee_pct: e.platform_fee_pct,
            platform_fee_amount: e.platform_fee_amount,
            tax_withheld_amount: e.tax_withheld_amount,
            exchange_rate: e.exchange_rate,
            net_payout_amount: e.net_payout_amount,
            status: e.status.into(),
            gateway_reference: e.gateway_reference,
            failure_reason: e.failure_reason,
            payout_date: e.payout_date,
            created_at: e.created_at,
            updated_at: e.updated_at,
        }
    }
}

#[derive(Debug, sqlx::FromRow, Serialize, Deserialize)]
pub struct DbPayoutSummary {
    pub total_gross: Option<Decimal>,
    pub total_platform_fee: Option<Decimal>,
    pub total_tax_withheld: Option<Decimal>,
    pub total_net: Option<Decimal>,
    pub total_paid: Option<Decimal>,
    pub total_pending: Option<Decimal>,
    pub total_processing: Option<Decimal>,
    pub count_entries: Option<i64>,
}

impl From<DbPayoutSummary> for common::payout::PayoutSummary {
    fn from(s: DbPayoutSummary) -> Self {
        common::payout::PayoutSummary {
            total_gross: s.total_gross.unwrap_or(Decimal::ZERO),
            total_platform_fee: s.total_platform_fee.unwrap_or(Decimal::ZERO),
            total_tax_withheld: s.total_tax_withheld.unwrap_or(Decimal::ZERO),
            total_net: s.total_net.unwrap_or(Decimal::ZERO),
            total_paid: s.total_paid.unwrap_or(Decimal::ZERO),
            total_pending: s.total_pending.unwrap_or(Decimal::ZERO),
            total_processing: s.total_processing.unwrap_or(Decimal::ZERO),
            count_entries: s.count_entries.unwrap_or(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, EnumString)]
#[sqlx(type_name = "email_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum DbEmailStatus {
    Pending,
    Processing,
    Sent,
    Failed,
}

impl From<DbEmailStatus> for common::email::EmailStatus {
    fn from(s: DbEmailStatus) -> Self {
        match s {
            DbEmailStatus::Pending => common::email::EmailStatus::Pending,
            DbEmailStatus::Processing => common::email::EmailStatus::Processing,
            DbEmailStatus::Sent => common::email::EmailStatus::Sent,
            DbEmailStatus::Failed => common::email::EmailStatus::Failed,
        }
    }
}

impl From<common::email::EmailStatus> for DbEmailStatus {
    fn from(s: common::email::EmailStatus) -> Self {
        match s {
            common::email::EmailStatus::Pending => DbEmailStatus::Pending,
            common::email::EmailStatus::Processing => DbEmailStatus::Processing,
            common::email::EmailStatus::Sent => DbEmailStatus::Sent,
            common::email::EmailStatus::Failed => DbEmailStatus::Failed,
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct EmailOutbox {
    pub id: Uuid,
    pub recipient_email: String,
    pub subject: String,
    pub template_id: String,
    pub payload: serde_json::Value,
    pub status: DbEmailStatus,
    pub attempts: i32,
    pub max_retries: i32,
    pub last_error: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
