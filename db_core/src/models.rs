use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::types::Json;
use strum_macros::EnumString;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

/// Metadata associated with a booking reservation.
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, Default)]
#[serde(default)]
pub struct BookingMetadata {
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
    /// Estimated arrival time string.
    pub estimated_arrival_time: Option<String>,
    /// Whether the booking is for business travel.
    pub is_business_trip: bool,
    /// Optional corporate company name for B2B tax compliance.
    pub business_company_name: Option<String>,
    /// Optional corporate Taxpayer Registration Number (TRN) for B2B compliance.
    pub business_tax_id: Option<String>,
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

pub use common::models::UserRole;

#[derive(Debug, Serialize, Deserialize, sqlx::Type, ToSchema, Clone, Copy, PartialEq)]
#[sqlx(type_name = "user_status", rename_all = "lowercase")]
pub enum UserStatus {
    Active,
    Inactive,
}

/// Database entity representing a property reservation hold or booking.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Booking {
    /// Unique booking identifier.
    pub id: Uuid,
    /// Confirmation code.
    pub confirmation_code: String,
    /// Guest user identifier.
    pub guest_id: Uuid,
    /// Property listing identifier.
    pub listing_id: Uuid,
    /// Current booking lifecycle status.
    pub status: BookingStatus,

    /// Check-in start date.
    pub date_from: NaiveDate,
    /// Check-out departure date.
    pub date_to: NaiveDate,

    /// Checkout currency.
    pub currency: String,
    /// Daily nightly rate in base currency.
    pub daily_rate: Decimal,
    /// Total number of guest persons.
    pub number_of_persons: i32,
    /// Total duration of the stay in days.
    pub total_days: i32,

    /// Subtotal pricing before taxes and discounts.
    pub sub_total_price: Decimal,
    /// Applied discount amount if any.
    pub discount_value: Option<Decimal>,
    /// Statutory tax amount if applicable.
    pub tax_value: Option<Decimal>,

    /// Breakdown of individual fees.
    pub fee_breakdown: Json<Vec<FeeItem>>,

    /// Final total price.
    pub total_price: Decimal,
    /// Cancellation policy applied to this booking.
    pub cancellation_policy: CancellationPolicy,
    /// Additional guest preferences and arrival metadata.
    pub metadata: Json<BookingMetadata>,
    /// Smart lock door access code for confirmed stays.
    pub door_access_code: Option<String>,

    /// Host cleaning fee.
    pub cleaning_fee: Decimal,
    /// Security / damage deposit held.
    pub damage_deposit: Decimal,
    /// Platform fee.
    pub platform_fee: Decimal,
    /// Applied statutory tax rate (e.g. 0.1500).
    pub tax_rate: Decimal,
    /// Base to payment exchange rate.
    pub exchange_rate: Decimal,

    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl Booking {
    /// Calculate net accommodation price after subtracting discounts.
    pub fn accommodation_net(&self) -> Decimal {
        self.sub_total_price - self.discount_value.unwrap_or(Decimal::ZERO)
    }
}

/// Booking model paired with review submission eligibility.
pub struct BookingWithEligibility {
    /// Associated booking record.
    pub booking: Booking,
    /// Review eligibility status.
    pub review_eligibility: Option<common::models::BookingReviewEligibility>,
}

/// Lifecycle status for reservations and bookings.
#[derive(Debug, Serialize, Deserialize, sqlx::Type, ToSchema, Clone, Copy, PartialEq)]
#[sqlx(type_name = "booking_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum BookingStatus {
    /// Initial payment pending state (15-minute hold).
    Pending,
    /// Payment confirmed and reservation secured.
    Confirmed,
    /// Reservation cancelled.
    Cancelled,
    /// Reservation completed post-stay.
    Completed,
}

/// Parameters for creating a new booking hold.
#[derive(Debug, Deserialize, Serialize, Validate)]
pub struct NewBooking {
    /// Human-readable confirmation code.
    pub confirmation_code: String,
    /// Guest user identifier.
    pub guest_id: Uuid,
    /// Property listing identifier.
    pub listing_id: Uuid,
    /// Check-in date.
    pub date_from: NaiveDate,
    /// Check-out date.
    pub date_to: NaiveDate,
    /// Payment currency.
    pub currency: String,
    /// Daily nightly rate.
    pub daily_rate: Decimal,
    /// Total persons.
    pub number_of_persons: i32,
    /// Total stay days.
    pub total_days: i32,
    /// Subtotal price.
    pub sub_total_price: Decimal,
    /// Discount amount.
    pub discount_value: Option<Decimal>,
    /// Tax amount.
    pub tax_value: Option<Decimal>,
    /// Itemized fee breakdown.
    pub fee_breakdown: Vec<FeeItem>,
    /// Total price.
    pub total_price: Decimal,
    /// Cancellation policy.
    pub cancellation_policy: CancellationPolicy,
    /// Guest preferences and arrival details.
    pub metadata: BookingMetadata,
    /// Optional smart door access code.
    pub door_access_code: Option<String>,
    /// Host cleaning fee.
    pub cleaning_fee: Decimal,
    /// Security / damage deposit held.
    pub damage_deposit: Decimal,
    /// Platform fee.
    pub platform_fee: Decimal,
    /// Applied statutory tax rate (e.g. 0.1500).
    pub tax_rate: Decimal,
    /// Base to payment exchange rate.
    pub exchange_rate: Decimal,
}

/// Parameters for mutating an existing booking.
#[derive(Debug, Deserialize, Serialize, Validate, Default, Clone)]
pub struct UpdatedBooking {
    /// Updated status if changing.
    pub status: Option<BookingStatus>,
    /// Updated arrival and guest metadata.
    pub metadata: Option<BookingMetadata>,
    /// Updated smart door access code.
    pub door_access_code: Option<String>,
    /// Check-in start date if requesting a date change.
    pub date_from: Option<chrono::NaiveDate>,
    /// Check-out departure date if requesting a date change.
    pub date_to: Option<chrono::NaiveDate>,
    /// Updated total guest count if modified.
    pub number_of_persons: Option<i32>,
    /// Daily rate if re-priced.
    pub daily_rate: Option<Decimal>,
    /// Total days if dates changed.
    pub total_days: Option<i32>,
    /// Subtotal price.
    pub sub_total_price: Option<Decimal>,
    /// Discount value.
    pub discount_value: Option<Decimal>,
    /// Tax value.
    pub tax_value: Option<Decimal>,
    /// Itemized fee breakdown.
    pub fee_breakdown: Option<Vec<FeeItem>>,
    /// Total price.
    pub total_price: Option<Decimal>,
    /// Cleaning fee.
    pub cleaning_fee: Option<Decimal>,
    /// Damage deposit.
    pub damage_deposit: Option<Decimal>,
    /// Platform fee.
    pub platform_fee: Option<Decimal>,
    /// Tax rate.
    pub tax_rate: Option<Decimal>,
    /// Exchange rate.
    pub exchange_rate: Option<Decimal>,
}

/// Immutable audit log entry capturing booking status transitions.
#[derive(Debug, FromRow, Serialize, Deserialize, Clone)]
pub struct BookingHistory {
    /// Unique audit log entry identifier.
    pub id: Uuid,
    /// Booking identifier.
    pub booking_id: Uuid,

    /// Confirmation code at the time of change.
    pub confirmation_code: String,
    /// Guest identifier.
    pub guest_id: Uuid,
    /// Property listing identifier.
    pub listing_id: Uuid,
    /// Recorded booking status.
    pub status: BookingStatus,
    /// Check-in date.
    pub date_from: NaiveDate,
    /// Check-out date.
    pub date_to: NaiveDate,
    /// Currency.
    pub currency: String,
    /// Daily rate.
    pub daily_rate: Decimal,
    /// Number of persons.
    pub number_of_persons: i32,
    /// Duration in days.
    pub total_days: i32,
    /// Subtotal price.
    pub sub_total_price: Decimal,
    /// Discount amount.
    pub discount_value: Option<Decimal>,
    /// Tax amount.
    pub tax_value: Option<Decimal>,
    /// Itemized fees.
    pub fee_breakdown: Json<Vec<FeeItem>>,
    /// Total price.
    pub total_price: Decimal,
    /// Cancellation policy.
    pub cancellation_policy: CancellationPolicy,
    /// Associated booking metadata.
    pub metadata: Json<BookingMetadata>,
    /// Smart door access code.
    pub door_access_code: Option<String>,

    /// User who performed the change.
    pub changed_by_id: Option<Uuid>,
    /// Reason provided for the change.
    pub change_reason: Option<String>,
    /// Timestamp of transition.
    pub created_at: DateTime<Utc>,
}

/// Deduplication and audit record for transactional emails and notifications sent per booking.
#[derive(Debug, FromRow, Serialize, Deserialize, Clone)]
pub struct BookingNotificationLog {
    /// Unique log identifier.
    pub id: Uuid,
    /// Associated booking identifier.
    pub booking_id: Uuid,
    /// Notification type discriminator.
    pub notification_type: String,
    /// Recipient user identifier.
    pub recipient_user_id: Uuid,
    /// Dispatch timestamp.
    pub sent_at: DateTime<Utc>,
}

pub use common::cancellation::CancellationPolicy;

pub use common::tax_document::{TaxDocumentStatus, TaxDocumentType};

/// Information identifying the Merchant of Record supplier issuing compliance documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierInfo {
    pub supplier_name: String,
    pub supplier_address: String,
    pub supplier_trn: String,
}

/// DTO for inserting a new statutory tax document into the database.
#[derive(Debug, Clone)]
pub struct NewTaxDocument {
    pub document_type: TaxDocumentType,
    pub document_number: String,
    pub revision: i32,
    pub status: TaxDocumentStatus,
    pub supersedes_id: Option<Uuid>,
    pub booking_id: Uuid,
    pub booking_history_id: Option<Uuid>,
    pub recipient_id: Uuid,
    pub listing_id: Uuid,
    pub issue_date: DateTime<Utc>,
    pub supplier_name: String,
    pub supplier_address: String,
    pub supplier_trn: String,
    pub recipient_name: String,
    pub recipient_email: String,
    pub recipient_company_name: Option<String>,
    pub recipient_trn: Option<String>,
    pub currency: String,
    pub exchange_rate: Decimal,
    pub jmd_exchange_rate: Decimal,
    pub accommodation_net: Decimal,
    pub cleaning_fee: Decimal,
    pub taxable_amount: Decimal,
    pub tax_rate: Decimal,
    pub tax_amount: Decimal,
    pub platform_fee: Decimal,
    pub damage_deposit: Decimal,
    pub total_amount: Decimal,
    pub amount_previously_paid: Option<Decimal>,
    pub balance_due: Option<Decimal>,
    pub refund_pct: Option<Decimal>,
    pub retained_amount: Option<Decimal>,
    pub jmd_tax_amount: Decimal,
    pub jmd_total_amount: Decimal,
    pub line_items: serde_json::Value,
}

/// Row entity representation of a compliance tax document in PostgreSQL.
#[derive(Debug, FromRow, Serialize, Deserialize, Clone)]
pub struct TaxDocumentRow {
    pub id: Uuid,
    pub document_type: TaxDocumentType,
    pub document_number: String,
    pub revision: i32,
    pub status: TaxDocumentStatus,
    pub supersedes_id: Option<Uuid>,
    pub booking_id: Uuid,
    pub booking_history_id: Option<Uuid>,
    pub recipient_id: Uuid,
    pub listing_id: Uuid,
    pub issue_date: DateTime<Utc>,
    pub supplier_name: String,
    pub supplier_address: String,
    pub supplier_trn: String,
    pub recipient_name: String,
    pub recipient_email: String,
    pub recipient_company_name: Option<String>,
    pub recipient_trn: Option<String>,
    pub currency: String,
    pub exchange_rate: Decimal,
    pub jmd_exchange_rate: Decimal,
    pub accommodation_net: Decimal,
    pub cleaning_fee: Decimal,
    pub taxable_amount: Decimal,
    pub tax_rate: Decimal,
    pub tax_amount: Decimal,
    pub platform_fee: Decimal,
    pub damage_deposit: Decimal,
    pub total_amount: Decimal,
    pub amount_previously_paid: Option<Decimal>,
    pub balance_due: Option<Decimal>,
    pub refund_pct: Option<Decimal>,
    pub retained_amount: Option<Decimal>,
    pub jmd_tax_amount: Decimal,
    pub jmd_total_amount: Decimal,
    pub line_items: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl From<TaxDocumentRow> for common::tax_document::TaxDocumentResponse {
    fn from(r: TaxDocumentRow) -> Self {
        let line_items: Vec<common::tax_document::InvoiceLineItem> =
            serde_json::from_value(r.line_items).unwrap_or_default();
        Self {
            id: r.id,
            document_type: r.document_type,
            document_number: r.document_number,
            revision: r.revision,
            status: r.status,
            supersedes_id: r.supersedes_id,
            booking_id: r.booking_id,
            booking_history_id: r.booking_history_id,
            recipient_id: r.recipient_id,
            listing_id: r.listing_id,
            issue_date: r.issue_date,
            supplier_name: r.supplier_name,
            supplier_address: r.supplier_address,
            supplier_trn: r.supplier_trn,
            recipient_name: r.recipient_name,
            recipient_email: r.recipient_email,
            recipient_company_name: r.recipient_company_name,
            recipient_trn: r.recipient_trn,
            currency: r.currency,
            exchange_rate: r.exchange_rate,
            jmd_exchange_rate: r.jmd_exchange_rate,
            accommodation_net: r.accommodation_net,
            cleaning_fee: r.cleaning_fee,
            taxable_amount: r.taxable_amount,
            tax_rate: r.tax_rate,
            tax_amount: r.tax_amount,
            platform_fee: r.platform_fee,
            damage_deposit: r.damage_deposit,
            total_amount: r.total_amount,
            amount_previously_paid: r.amount_previously_paid,
            balance_due: r.balance_due,
            refund_pct: r.refund_pct,
            retained_amount: r.retained_amount,
            jmd_tax_amount: r.jmd_tax_amount,
            jmd_total_amount: r.jmd_total_amount,
            line_items,
            created_at: r.created_at,
        }
    }
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
    pub cleaning_fee: Decimal,
    pub damage_deposit: Decimal,
    /// Number of guests included in the base nightly rate.
    pub guests_included: i32,
    /// Fee per extra guest per night above `guests_included` (in `base_currency`).
    pub extra_guest_fee: Decimal,
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
    pub cleaning_fee: Decimal,
    pub damage_deposit: Decimal,
    /// Number of guests included in the base nightly rate.
    pub guests_included: i32,
    /// Fee per extra guest per night above `guests_included` (in `base_currency`).
    pub extra_guest_fee: Decimal,
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
    pub cleaning_fee: Option<Decimal>,
    pub damage_deposit: Option<Decimal>,
    /// Optional number of guests included in base rate (defaults to `1`).
    pub guests_included: Option<i32>,
    /// Optional fee per extra guest per night above `guests_included` (defaults to `0.00`).
    pub extra_guest_fee: Option<Decimal>,
}

#[derive(Debug, Deserialize, Serialize, Validate, Default)]
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
    pub cleaning_fee: Option<Decimal>,
    pub damage_deposit: Option<Decimal>,
    /// Optional updated number of guests included in base rate.
    pub guests_included: Option<i32>,
    /// Optional updated fee per extra guest per night.
    pub extra_guest_fee: Option<Decimal>,
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

pub use common::models::MessageSenderRole;
pub type DbMessageSenderRole = MessageSenderRole;

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
            sender_role: msg.sender_role,
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

pub use common::payout::PayoutStatus;
pub type DbPayoutStatus = PayoutStatus;

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
            status: e.status,
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

pub use common::email::EmailStatus;
pub type DbEmailStatus = EmailStatus;

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

pub use common::models::{
    ChangeInitiatorRole, ChangeSettlementStatus, ReservationChangePricingSnapshot,
    ReservationChangeRequestResponse, ReservationChangeStatus,
};

/// Parameters required to insert a new `reservation_change_request` row into PostgreSQL.
#[derive(Debug, Clone)]
pub struct NewReservationChangeRequest {
    /// Associated booking identifier.
    pub booking_id: Uuid,
    /// Associated listing identifier.
    pub listing_id: Uuid,
    /// User ID of the initiator.
    pub initiated_by_id: Uuid,
    /// Role of the initiator (`guest`, `host`, or `admin`).
    pub initiated_by_role: ChangeInitiatorRole,
    /// Check-in date prior to the change.
    pub previous_date_from: NaiveDate,
    /// Check-out date prior to the change.
    pub previous_date_to: NaiveDate,
    /// Proposed check-in date.
    pub proposed_date_from: NaiveDate,
    /// Proposed check-out date.
    pub proposed_date_to: NaiveDate,
    /// Total guest count prior to the change.
    pub previous_number_of_persons: i32,
    /// Proposed total guest count.
    pub proposed_number_of_persons: i32,
    /// Proposed adult count.
    pub proposed_num_adults: i32,
    /// Proposed children count.
    pub proposed_num_children: i32,
    /// Proposed infant count.
    pub proposed_num_infants: i32,
    /// Proposed pet count.
    pub proposed_num_pets: i32,
    /// Booking payment currency code.
    pub currency: String,
    /// Total price prior to the change.
    pub previous_total_price: Decimal,
    /// Proposed total price.
    pub proposed_total_price: Decimal,
    /// Difference owed by (`> 0`) or refunded to (`< 0`) the guest.
    pub balance_due: Decimal,
    /// Host net payout prior to the change.
    pub previous_net_payout: Decimal,
    /// Proposed host net payout.
    pub proposed_net_payout: Decimal,
    /// Host net payout delta.
    pub host_payout_delta: Decimal,
    /// Full pricing snapshot for atomic booking, receipt, and payout updates.
    pub pricing_snapshot: ReservationChangePricingSnapshot,
    /// Initial payment settlement status (`not_required`, `pending_charge`, or `pending_refund`).
    pub settlement_status: ChangeSettlementStatus,
    /// Optional initiator reason note.
    pub reason: Option<String>,
    /// Date on which the pending request auto-expires if unapproved.
    pub expires_at_date: NaiveDate,
}

/// Database row entity representing a `reservation_change_request` record.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ReservationChangeRequestRow {
    /// Unique change request identifier.
    pub id: Uuid,
    /// Associated booking identifier.
    pub booking_id: Uuid,
    /// Associated listing identifier.
    pub listing_id: Uuid,
    /// User ID of the initiator.
    pub initiated_by_id: Uuid,
    /// Role of the initiator.
    pub initiated_by_role: ChangeInitiatorRole,
    /// Current lifecycle status.
    pub status: ReservationChangeStatus,
    /// Check-in date prior to the change.
    pub previous_date_from: NaiveDate,
    /// Check-out date prior to the change.
    pub previous_date_to: NaiveDate,
    /// Proposed check-in date.
    pub proposed_date_from: NaiveDate,
    /// Proposed check-out date.
    pub proposed_date_to: NaiveDate,
    /// Total guest count prior to the change.
    pub previous_number_of_persons: i32,
    /// Proposed total guest count.
    pub proposed_number_of_persons: i32,
    /// Proposed adult count.
    pub proposed_num_adults: i32,
    /// Proposed children count.
    pub proposed_num_children: i32,
    /// Proposed infant count.
    pub proposed_num_infants: i32,
    /// Proposed pet count.
    pub proposed_num_pets: i32,
    /// Booking payment currency code.
    pub currency: String,
    /// Total price prior to the change.
    pub previous_total_price: Decimal,
    /// Proposed total price.
    pub proposed_total_price: Decimal,
    /// Guest balance due.
    pub balance_due: Decimal,
    /// Host net payout prior to the change.
    pub previous_net_payout: Decimal,
    /// Proposed host net payout.
    pub proposed_net_payout: Decimal,
    /// Host net payout delta.
    pub host_payout_delta: Decimal,
    /// JSONB pricing calculation snapshot.
    pub pricing_snapshot: Json<ReservationChangePricingSnapshot>,
    /// Payment settlement status.
    pub settlement_status: ChangeSettlementStatus,
    /// Payment gateway reference string if settled.
    pub gateway_reference: Option<String>,
    /// Optional initiator reason note.
    pub reason: Option<String>,
    /// User ID of the counterparty who approved or declined.
    pub responder_id: Option<Uuid>,
    /// Optional counterparty response note.
    pub response_note: Option<String>,
    /// Timestamp when the counterparty responded.
    pub responded_at: Option<DateTime<Utc>>,
    /// Date on which the pending request auto-expires if unapproved.
    pub expires_at_date: NaiveDate,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl From<ReservationChangeRequestRow> for ReservationChangeRequestResponse {
    fn from(r: ReservationChangeRequestRow) -> Self {
        Self {
            id: r.id,
            booking_id: r.booking_id,
            listing_id: r.listing_id,
            initiated_by_id: r.initiated_by_id,
            initiated_by_role: r.initiated_by_role,
            status: r.status,
            previous_date_from: r.previous_date_from,
            previous_date_to: r.previous_date_to,
            proposed_date_from: r.proposed_date_from,
            proposed_date_to: r.proposed_date_to,
            previous_number_of_persons: r.previous_number_of_persons,
            proposed_number_of_persons: r.proposed_number_of_persons,
            proposed_num_adults: r.proposed_num_adults,
            proposed_num_children: r.proposed_num_children,
            proposed_num_infants: r.proposed_num_infants,
            proposed_num_pets: r.proposed_num_pets,
            currency: r.currency.trim().to_string(),
            previous_total_price: r.previous_total_price,
            proposed_total_price: r.proposed_total_price,
            balance_due: r.balance_due,
            previous_net_payout: r.previous_net_payout,
            proposed_net_payout: r.proposed_net_payout,
            host_payout_delta: r.host_payout_delta,
            pricing_snapshot: r.pricing_snapshot.0,
            settlement_status: r.settlement_status,
            gateway_reference: r.gateway_reference,
            reason: r.reason,
            responder_id: r.responder_id,
            response_note: r.response_note,
            responded_at: r.responded_at,
            expires_at_date: r.expires_at_date,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_booking_accommodation_net_with_and_without_discount() {
        let mut booking = Booking {
            id: Uuid::new_v4(),
            confirmation_code: "ABCDEF12".to_string(),
            guest_id: Uuid::new_v4(),
            listing_id: Uuid::new_v4(),
            status: BookingStatus::Confirmed,
            date_from: chrono::NaiveDate::from_ymd_opt(2026, 12, 1).unwrap(),
            date_to: chrono::NaiveDate::from_ymd_opt(2026, 12, 5).unwrap(),
            currency: "USD".to_string(),
            daily_rate: dec!(250.00),
            number_of_persons: 2,
            total_days: 4,
            sub_total_price: dec!(1000.00),
            discount_value: Some(dec!(150.00)),
            tax_value: Some(dec!(127.50)),
            fee_breakdown: sqlx::types::Json(vec![]),
            total_price: dec!(1077.50),
            cancellation_policy: CancellationPolicy::Flexible,
            metadata: sqlx::types::Json(BookingMetadata::default()),
            door_access_code: None,
            cleaning_fee: dec!(100.00),
            damage_deposit: dec!(200.00),
            platform_fee: dec!(50.00),
            tax_rate: dec!(0.15),
            exchange_rate: dec!(1.0),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // With discount
        assert_eq!(booking.accommodation_net(), dec!(850.00));

        // Without discount (None)
        booking.discount_value = None;
        assert_eq!(booking.accommodation_net(), dec!(1000.00));

        // With zero discount
        booking.discount_value = Some(Decimal::ZERO);
        assert_eq!(booking.accommodation_net(), dec!(1000.00));
    }
}
