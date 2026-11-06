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

/// A user account.
#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct User {
    /// Unique identifier.
    pub id: Uuid,
    /// Email address.
    pub email: String,
    // Typically we don't return password_hash in the API model, but for the DB model it's fine.
    /// Bcrypt hash of the password.
    #[serde(skip_serializing)]
    pub password_hash: String,
    /// First name.
    pub first_name: String,
    /// Last name.
    pub last_name: String,
    /// Phone number, if given.
    pub phone_number: Option<String>,
    /// Whether the account can be used.
    pub is_active: bool,
    /// Whether the email address has been verified.
    pub is_verified: bool,
    /// Pending email verification code.
    pub verification_code: Option<String>,
    /// When the verification code stops working.
    pub verification_code_expires_at: Option<DateTime<Utc>>,
    /// When this was created.
    pub created_at: DateTime<Utc>,
    /// When this was last changed.
    pub updated_at: DateTime<Utc>,
    /// Free-form attributes, as JSON.
    pub attributes: serde_json::Value,
    /// Roles the user holds.
    pub roles: Vec<UserRole>,
    /// Preferred currency code.
    pub default_currency: String,
    /// When this was soft-deleted, if it was.
    pub deleted_at: Option<DateTime<Utc>>,
    /// Unique public handle, e.g. `jane-brown` (see `common::slug`).
    pub slug: String,
}

/// A guest's profile.
#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct BookerProfile {
    /// The guest this profile belongs to.
    pub user_id: Uuid,
    /// Emergency contacts, as JSON.
    pub emergency_contacts: Option<serde_json::Value>,
    /// Booking preferences, as JSON.
    pub booking_preferences: Option<serde_json::Value>,
    /// Loyalty details, as JSON.
    pub loyalty: Option<serde_json::Value>,
    /// When this was created.
    pub created_at: DateTime<Utc>,
    /// When this was last changed.
    pub updated_at: DateTime<Utc>,
}

/// A host's profile.
#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct HostProfile {
    /// The host this profile belongs to.
    pub user_id: Uuid,
    /// Host verification status.
    pub verified_status: Option<String>,
    /// How the host is paid, as JSON.
    pub payout_details: Option<serde_json::Value>,
    /// Bio shown to guests.
    pub description: Option<String>,
    /// When this was created.
    pub created_at: DateTime<Utc>,
    /// When this was last changed.
    pub updated_at: DateTime<Utc>,
}

/// Fields for creating a user.
#[derive(Debug)]
pub struct NewUser {
    /// Unique identifier for the new user.
    pub id: Uuid,
    /// Email address.
    pub email: String,
    /// Bcrypt hash of the password.
    pub password_hash: String,
    /// First name.
    pub first_name: String,
    /// Last name.
    pub last_name: String,
    /// Phone number, if given.
    pub phone_number: Option<String>,
    /// Whether the account can be used.
    pub is_active: bool,
    /// Whether the email address has been verified.
    pub is_verified: bool,
    /// Pending email verification code.
    pub verification_code: Option<String>,
    /// When the verification code stops working.
    pub verification_code_expires_at: Option<DateTime<Utc>>,
    /// Free-form attributes, as JSON.
    pub attributes: serde_json::Value,
    /// Roles the user holds.
    pub roles: Option<Vec<UserRole>>,
    /// Preferred currency code.
    pub default_currency: String,
}

pub use common::models::{NewBookerProfile, NewHostProfile};

/// Changes to a user. `None` leaves a field as it is.
#[derive(Debug, Default, Clone)]
pub struct UpdatedUser {
    /// Updated email address.
    pub email: Option<String>,
    /// Updated bcrypt hash of the password.
    pub password_hash: Option<String>,
    /// Updated first name.
    pub first_name: Option<String>,
    /// Updated last name.
    pub last_name: Option<String>,
    /// Updated phone number, if given.
    pub phone_number: Option<String>,
    /// Whether the account can be used.
    pub is_active: Option<bool>,
    /// Updated whether the email address has been verified.
    pub is_verified: Option<bool>,
    /// Updated pending email verification code.
    pub verification_code: Option<String>,
    /// Updated when the verification code stops working.
    pub verification_code_expires_at: Option<DateTime<Utc>>,
    /// Updated free-form attributes, as JSON.
    pub attributes: Option<serde_json::Value>,
    /// Updated roles the user holds.
    pub roles: Option<Vec<UserRole>>,
    /// Updated preferred currency code.
    pub default_currency: Option<String>,
    /// New slug, already cleaned up with `common::slug::user_slug`.
    pub slug: Option<String>,
}

pub use common::models::UserRole;

/// Whether a user account is active.
#[derive(Debug, Serialize, Deserialize, sqlx::Type, ToSchema, Clone, Copy, PartialEq)]
#[sqlx(type_name = "user_status", rename_all = "lowercase")]
pub enum UserStatus {
    /// The account can be used.
    Active,
    /// The account is disabled.
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

    /// Promo code redeemed by this booking, if any.
    pub promo_code_id: Option<Uuid>,
    /// Snapshot of the code's terms at hold time.
    pub promo_terms: Option<Json<common::promo::PromoTerms>>,
    /// Promo discount taken from the stay.
    pub promo_stay_discount: Decimal,
    /// Promo discount taken from the cleaning fee.
    pub promo_cleaning_discount: Decimal,

    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl Booking {
    /// Net accommodation price after weekly or monthly discounts and any promo.
    pub fn accommodation_net(&self) -> Decimal {
        self.sub_total_price
            - self.discount_value.unwrap_or(Decimal::ZERO)
            - self.promo_stay_discount
    }

    /// Cleaning fee after any promo.
    pub fn cleaning_net(&self) -> Decimal {
        self.cleaning_fee - self.promo_cleaning_discount
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
    /// Initial payment pending state; holds its dates until `expires_at`.
    Pending,
    /// Payment confirmed and reservation secured.
    Confirmed,
    /// Reservation cancelled.
    Cancelled,
    /// Reservation completed post-stay.
    Completed,
}

impl BookingStatus {
    /// Whether a booking may move from `self` to `to`. Staying put is always
    /// allowed. Cancelled and completed are final: reviving one would skip
    /// the availability check and could double-book its dates.
    pub fn can_transition_to(self, to: BookingStatus) -> bool {
        use BookingStatus::*;
        self == to
            || matches!(
                (self, to),
                (Pending, Confirmed)
                    | (Pending, Cancelled)
                    | (Confirmed, Cancelled)
                    | (Confirmed, Completed)
            )
    }
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
    /// The promo code this booking redeems, if any.
    #[serde(default)]
    pub promo: Option<NewBookingPromo>,
}

/// A promo code being redeemed by a new booking.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NewBookingPromo {
    /// The code being redeemed.
    pub promo_code_id: Uuid,
    /// Snapshot of the code's terms.
    pub terms: common::promo::PromoTerms,
    /// Discount taken from the stay.
    pub stay_discount: Decimal,
    /// Discount taken from the cleaning fee.
    pub cleaning_discount: Decimal,
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
    /// When true, the discount fields replace the stored values, so `None`
    /// clears a discount that no longer applies. When false, `None` keeps them.
    pub replace_discounts: bool,
    /// New promo discount on the stay, when repriced.
    pub promo_stay_discount: Option<Decimal>,
    /// New promo discount on the cleaning fee, when repriced.
    pub promo_cleaning_discount: Option<Decimal>,
    /// Removes the promo and frees its use, because the new stay doesn't qualify.
    pub clear_promo: bool,
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
    /// Legal name of the supplier.
    pub supplier_name: String,
    /// Registered address of the supplier.
    pub supplier_address: String,
    /// Tax registration number (TRN) of the supplier.
    pub supplier_trn: String,
}

/// DTO for inserting a new statutory tax document into the database.
#[derive(Debug, Clone)]
pub struct NewTaxDocument {
    /// Kind of document.
    pub document_type: TaxDocumentType,
    /// Human-readable document number.
    pub document_number: String,
    /// Revision number of this document.
    pub revision: i32,
    /// Whether the document is current.
    pub status: TaxDocumentStatus,
    /// The document this one replaces, if any.
    pub supersedes_id: Option<Uuid>,
    /// The booking this belongs to.
    pub booking_id: Uuid,
    /// The booking history snapshot this was issued from, if any.
    pub booking_history_id: Option<Uuid>,
    /// The user this was issued to.
    pub recipient_id: Uuid,
    /// The listing this belongs to.
    pub listing_id: Uuid,
    /// When this was issued.
    pub issue_date: DateTime<Utc>,
    /// Legal name of the supplier.
    pub supplier_name: String,
    /// Registered address of the supplier.
    pub supplier_address: String,
    /// Tax registration number (TRN) of the supplier.
    pub supplier_trn: String,
    /// Name of the recipient.
    pub recipient_name: String,
    /// Email address of the recipient.
    pub recipient_email: String,
    /// Company of the recipient, for business stays.
    pub recipient_company_name: Option<String>,
    /// Tax registration number (TRN) of the recipient, for business stays.
    pub recipient_trn: Option<String>,
    /// Currency of the amounts.
    pub currency: String,
    /// Listing currency to payment currency, locked on the booking.
    pub exchange_rate: Decimal,
    /// Document currency to Jamaican dollars, effective at the issue date.
    pub jmd_exchange_rate: Decimal,
    /// Accommodation after discounts.
    pub accommodation_net: Decimal,
    /// One-time cleaning fee.
    pub cleaning_fee: Decimal,
    /// Amount tax is charged on.
    pub taxable_amount: Decimal,
    /// Tax rate as a fraction, such as `0.15` for 15%.
    pub tax_rate: Decimal,
    /// Tax charged.
    pub tax_amount: Decimal,
    /// Guest platform fee.
    pub platform_fee: Decimal,
    /// Refundable damage deposit.
    pub damage_deposit: Decimal,
    /// Total charged.
    pub total_amount: Decimal,
    /// Amount already paid, on a revised document.
    pub amount_previously_paid: Option<Decimal>,
    /// Amount still owed, on a revised document.
    pub balance_due: Option<Decimal>,
    /// Refund percentage, on a cancellation document.
    pub refund_pct: Option<Decimal>,
    /// Amount kept after a cancellation.
    pub retained_amount: Option<Decimal>,
    /// Tax charged, in Jamaican dollars.
    pub jmd_tax_amount: Decimal,
    /// Total charged, in Jamaican dollars.
    pub jmd_total_amount: Decimal,
    /// Invoice line items.
    pub line_items: serde_json::Value,
    /// The code the booking redeemed, if any.
    pub promo_code: Option<String>,
    /// The whole promo discount. Already reflected in the net amounts.
    pub promo_discount: Decimal,
}

/// Row entity representation of a compliance tax document in PostgreSQL.
#[derive(Debug, FromRow, Serialize, Deserialize, Clone)]
pub struct TaxDocumentRow {
    /// Unique identifier.
    pub id: Uuid,
    /// Kind of document.
    pub document_type: TaxDocumentType,
    /// Human-readable document number.
    pub document_number: String,
    /// Revision number of this document.
    pub revision: i32,
    /// Whether the document is current.
    pub status: TaxDocumentStatus,
    /// The document this one replaces, if any.
    pub supersedes_id: Option<Uuid>,
    /// The booking this belongs to.
    pub booking_id: Uuid,
    /// The booking history snapshot this was issued from, if any.
    pub booking_history_id: Option<Uuid>,
    /// The user this was issued to.
    pub recipient_id: Uuid,
    /// The listing this belongs to.
    pub listing_id: Uuid,
    /// When this was issued.
    pub issue_date: DateTime<Utc>,
    /// Legal name of the supplier.
    pub supplier_name: String,
    /// Registered address of the supplier.
    pub supplier_address: String,
    /// Tax registration number (TRN) of the supplier.
    pub supplier_trn: String,
    /// Name of the recipient.
    pub recipient_name: String,
    /// Email address of the recipient.
    pub recipient_email: String,
    /// Company of the recipient, for business stays.
    pub recipient_company_name: Option<String>,
    /// Tax registration number (TRN) of the recipient, for business stays.
    pub recipient_trn: Option<String>,
    /// Currency of the amounts.
    pub currency: String,
    /// Listing currency to payment currency, locked on the booking.
    pub exchange_rate: Decimal,
    /// Document currency to Jamaican dollars, effective at the issue date.
    pub jmd_exchange_rate: Decimal,
    /// Accommodation after discounts.
    pub accommodation_net: Decimal,
    /// One-time cleaning fee.
    pub cleaning_fee: Decimal,
    /// Amount tax is charged on.
    pub taxable_amount: Decimal,
    /// Tax rate as a fraction, such as `0.15` for 15%.
    pub tax_rate: Decimal,
    /// Tax charged.
    pub tax_amount: Decimal,
    /// Guest platform fee.
    pub platform_fee: Decimal,
    /// Refundable damage deposit.
    pub damage_deposit: Decimal,
    /// Total charged.
    pub total_amount: Decimal,
    /// Amount already paid, on a revised document.
    pub amount_previously_paid: Option<Decimal>,
    /// Amount still owed, on a revised document.
    pub balance_due: Option<Decimal>,
    /// Refund percentage, on a cancellation document.
    pub refund_pct: Option<Decimal>,
    /// Amount kept after a cancellation.
    pub retained_amount: Option<Decimal>,
    /// Tax charged, in Jamaican dollars.
    pub jmd_tax_amount: Decimal,
    /// Total charged, in Jamaican dollars.
    pub jmd_total_amount: Decimal,
    /// Invoice line items.
    pub line_items: serde_json::Value,
    /// The code the booking redeemed, if any.
    pub promo_code: Option<String>,
    /// The whole promo discount. Already reflected in the net amounts.
    pub promo_discount: Decimal,
    /// When this was created.
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
            promo_code: r.promo_code,
            promo_discount: r.promo_discount,
            created_at: r.created_at,
        }
    }
}

pub use common::models::FeeItem;

/// A rental property.
#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Listing {
    /// Unique identifier.
    pub id: Uuid,
    /// The host who owns the listing.
    pub user_id: Uuid,
    /// Display name.
    pub name: String,
    /// Longer description.
    pub description: Option<String>,
    /// Structure type ID, from `StructureType::id`.
    pub listing_structure_id: i32,
    /// Country.
    pub country: String,
    /// Base nightly rate in `base_currency`.
    pub price_per_night: Option<Decimal>,
    /// Whether guests can see and book it.
    pub is_active: bool,
    /// When the listing was added.
    pub added_at: DateTime<Utc>,
    /// When this was soft-deleted, if it was.
    pub deleted_at: Option<DateTime<Utc>>,
    /// URL of the primary photo, if any.
    pub primary_image_url: Option<String>,
    /// Discount percentage for stays of 7 nights or more.
    pub weekly_discount_percentage: Option<Decimal>,
    /// Discount percentage for stays of 28 nights or more.
    pub monthly_discount_percentage: Option<Decimal>,
    /// Unique URL handle.
    pub slug: String,
    /// Most guests allowed.
    pub max_guests: i32,
    /// Number of bedrooms.
    pub bedrooms: i32,
    /// Number of beds.
    pub beds: i32,
    /// Number of full bathrooms.
    pub full_bathrooms: i32,
    /// Number of half bathrooms.
    pub half_bathrooms: i32,
    /// Floor area in square meters, if known.
    pub square_meters: Option<i32>,
    /// Latitude, if known.
    pub latitude: Option<f64>,
    /// Longitude, if known.
    pub longitude: Option<f64>,
    /// Average overall rating, if it has reviews.
    pub overall_rating: Option<f64>,
    /// Number of reviews.
    pub review_count: i32,
    /// Amenities and other details, as JSON.
    pub listing_details: Json<serde_json::Value>,
    /// City, if known.
    pub city: Option<String>,
    /// Currency the listing is priced in.
    pub base_currency: String,
    /// Minimum stay in nights.
    pub minimum_stay: i32,
    /// Days to keep free between bookings, from 0 to 365.
    pub days_between_bookings: i32,
    /// Platform commission on the host's gross, as a fraction such as `0.1000`.
    pub commission_pct: Decimal,
    /// One-time cleaning fee.
    pub cleaning_fee: Decimal,
    /// Refundable damage deposit.
    pub damage_deposit: Decimal,
    /// Number of guests included in the base nightly rate.
    pub guests_included: i32,
    /// Fee per extra guest per night above `guests_included` (in `base_currency`).
    pub extra_guest_fee: Decimal,
}

/// A listing with its host's name, for search results.
#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct ListingWithOwner {
    /// Unique identifier.
    pub id: Uuid,
    /// The host who owns the listing.
    pub user_id: Uuid,
    /// Display name.
    pub name: String,
    /// Longer description.
    pub description: Option<String>,
    /// Structure type ID, from `StructureType::id`.
    pub listing_structure_id: i32,
    /// Country.
    pub country: String,
    /// Base nightly rate in `base_currency`.
    pub price_per_night: Option<Decimal>,
    /// Whether guests can see and book it.
    pub is_active: bool,
    /// When the listing was added.
    pub added_at: DateTime<Utc>,
    /// When this was soft-deleted, if it was.
    pub deleted_at: Option<DateTime<Utc>>,
    /// Display name of the host.
    pub owner_name: Option<String>,
    /// URL handle of the host.
    pub owner_slug: Option<String>,
    /// URL of the primary photo, if any.
    pub primary_image_url: Option<String>,
    /// Discount percentage for stays of 7 nights or more.
    pub weekly_discount_percentage: Option<Decimal>,
    /// Discount percentage for stays of 28 nights or more.
    pub monthly_discount_percentage: Option<Decimal>,
    /// Most guests allowed.
    pub max_guests: i32,
    /// Number of bedrooms.
    pub bedrooms: i32,
    /// Number of beds.
    pub beds: i32,
    /// Number of full bathrooms.
    pub full_bathrooms: i32,
    /// Number of half bathrooms.
    pub half_bathrooms: i32,
    /// Floor area in square meters, if known.
    pub square_meters: Option<i32>,
    /// Latitude, if known.
    pub latitude: Option<f64>,
    /// Longitude, if known.
    pub longitude: Option<f64>,
    /// Average overall rating, if it has reviews.
    pub overall_rating: Option<f64>,
    /// City, if known.
    pub city: Option<String>,
    /// Currency the listing is priced in.
    pub base_currency: String,
    /// Unique URL handle.
    pub slug: String,
    /// Amenities and other details, as JSON.
    pub listing_details: Json<serde_json::Value>,
    /// Minimum stay in nights.
    pub minimum_stay: i32,
    /// Days to keep free between bookings, from 0 to 365.
    pub days_between_bookings: i32,
    /// Platform commission on the host's gross, as a fraction such as `0.1000`.
    pub commission_pct: Decimal,
    /// One-time cleaning fee.
    pub cleaning_fee: Decimal,
    /// Refundable damage deposit.
    pub damage_deposit: Decimal,
    /// Number of guests included in the base nightly rate.
    pub guests_included: i32,
    /// Fee per extra guest per night above `guests_included` (in `base_currency`).
    pub extra_guest_fee: Decimal,
}

/// Fields for creating a listing.
#[derive(Debug, Deserialize, Serialize, Validate)]
pub struct NewListing {
    /// The host who owns the listing.
    pub user_id: Uuid,
    /// Whether guests can see and book the listing.
    pub is_active: bool,
    /// Display name.
    #[validate(length(min = 1, message = "Name cannot be empty"))]
    pub name: String,

    /// Longer description.
    #[validate(length(
        max = 2000,
        message = "Description cannot be longer than 2000 characters"
    ))]
    pub description: Option<String>,

    /// Structure type ID, from `StructureType::id`.
    #[validate(range(min = 1, message = "Invalid listing structure ID"))]
    pub listing_structure_id: i32,

    /// Country.
    #[validate(length(min = 1, message = "Country cannot be empty"))]
    pub country: String,
    /// Base nightly rate in `base_currency`.
    pub price_per_night: Option<Decimal>,
    /// Discount percentage for stays of 7 nights or more.
    pub weekly_discount_percentage: Option<Decimal>,
    /// Discount percentage for stays of 28 nights or more.
    pub monthly_discount_percentage: Option<Decimal>,
    /// Most guests allowed.
    pub max_guests: i32,
    /// Number of bedrooms.
    pub bedrooms: i32,
    /// Number of beds.
    pub beds: i32,
    /// Number of full bathrooms.
    pub full_bathrooms: i32,
    /// Number of half bathrooms.
    pub half_bathrooms: i32,
    /// Floor area in square meters, if known.
    pub square_meters: Option<i32>,
    /// Latitude, if known.
    pub latitude: Option<f64>,
    /// Longitude, if known.
    pub longitude: Option<f64>,
    /// Amenities and other details, as JSON.
    pub listing_details: Option<serde_json::Value>,
    /// City, if known.
    pub city: Option<String>,
    /// Currency the listing is priced in.
    pub base_currency: String,
    /// Minimum stay in nights.
    pub minimum_stay: i32,
    /// Days to keep free between bookings, from 0 to 365.
    pub days_between_bookings: i32,
    /// Platform commission on the host's gross, as a fraction such as `0.1000`.
    pub commission_pct: Option<Decimal>,
    /// One-time cleaning fee.
    pub cleaning_fee: Option<Decimal>,
    /// Refundable damage deposit.
    pub damage_deposit: Option<Decimal>,
    /// Optional number of guests included in base rate (defaults to `1`).
    pub guests_included: Option<i32>,
    /// Optional fee per extra guest per night above `guests_included` (defaults to `0.00`).
    pub extra_guest_fee: Option<Decimal>,
}

/// Changes to a listing. `None` leaves a field as it is.
#[derive(Debug, Deserialize, Serialize, Validate, Default)]
pub struct UpdatedListing {
    /// Updated display name.
    #[validate(length(min = 1, message = "Name cannot be empty"))]
    pub name: Option<String>,

    /// Updated longer description.
    #[validate(length(
        max = 2000,
        message = "Description cannot be longer than 2000 characters"
    ))]
    pub description: Option<String>,

    /// Updated structure type ID, from `StructureType::id`.
    #[validate(range(min = 1, message = "Invalid listing structure ID"))]
    pub listing_structure_id: Option<i32>,

    /// Updated country.
    #[validate(length(min = 1, message = "Country cannot be empty"))]
    pub country: Option<String>,

    /// Updated base nightly rate in `base_currency`.
    pub price_per_night: Option<Decimal>,

    /// Updated whether guests can see and book it.
    pub is_active: Option<bool>,

    /// Updated discount percentage for stays of 7 nights or more.
    pub weekly_discount_percentage: Option<Decimal>,
    /// Updated discount percentage for stays of 28 nights or more.
    pub monthly_discount_percentage: Option<Decimal>,
    /// Updated most guests allowed.
    pub max_guests: Option<i32>,
    /// Updated number of bedrooms.
    pub bedrooms: Option<i32>,
    /// Updated number of beds.
    pub beds: Option<i32>,
    /// Updated number of full bathrooms.
    pub full_bathrooms: Option<i32>,
    /// Updated number of half bathrooms.
    pub half_bathrooms: Option<i32>,
    /// Updated floor area in square meters, if known.
    pub square_meters: Option<i32>,
    /// Updated latitude, if known.
    pub latitude: Option<f64>,
    /// Updated longitude, if known.
    pub longitude: Option<f64>,
    /// Updated amenities and other details, as JSON.
    pub listing_details: Option<serde_json::Value>,
    /// Updated city, if known.
    pub city: Option<String>,
    /// Updated currency the listing is priced in.
    pub base_currency: Option<String>,
    /// Updated minimum stay in nights.
    pub minimum_stay: Option<i32>,
    /// Updated days to keep free between bookings, from 0 to 365.
    pub days_between_bookings: Option<i32>,
    /// Updated platform commission on the host's gross, as a fraction such as `0.1000`.
    pub commission_pct: Option<Decimal>,
    /// Updated one-time cleaning fee.
    pub cleaning_fee: Option<Decimal>,
    /// Updated refundable damage deposit.
    pub damage_deposit: Option<Decimal>,
    /// Optional updated number of guests included in base rate.
    pub guests_included: Option<i32>,
    /// Optional updated fee per extra guest per night.
    pub extra_guest_fee: Option<Decimal>,
}

/// What kind of building a listing is.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, sqlx::Type, EnumString)]
#[sqlx(rename_all = "snake_case")]
pub enum StructureType {
    /// An apartment.
    #[strum(serialize = "Apartment")]
    Apartment,
    /// A house.
    #[strum(serialize = "House")]
    House,
    /// A townhouse.
    #[strum(serialize = "Townhouse")]
    Townhouse,
    /// A studio.
    #[strum(serialize = "Studio")]
    Studio,
    /// A villa.
    #[strum(serialize = "Villa")]
    Villa,
}

impl StructureType {
    /// The value stored in `listing.listing_structure_id`.
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

/// Where an image is in the upload pipeline.
#[derive(
    Debug, Serialize, Deserialize, sqlx::Type, ToSchema, Clone, Copy, PartialEq, EnumString,
)]
#[sqlx(type_name = "image_status", rename_all = "PascalCase")]
pub enum ImageStatus {
    /// A signed URL was issued, and the upload hasn't arrived.
    PendingUpload,
    /// The original has been uploaded.
    Uploaded,
    /// The image worker is resizing it.
    Processing,
    /// All sizes are ready.
    Processed,
    /// Processing failed.
    Failed,
}

/// A stored size of an image.
#[derive(
    Debug, Serialize, Deserialize, sqlx::Type, ToSchema, Clone, Copy, PartialEq, EnumString,
)]
#[sqlx(type_name = "image_resolution", rename_all = "PascalCase")]
pub enum ImageResolution {
    /// The original upload.
    Raw,
    /// 400 pixels wide.
    Thumbnail400w,
    /// 720 pixels wide.
    Mobile720w,
    /// 1280 pixels wide.
    Tablet1280w,
    /// 1920 pixels wide.
    Desktop1920w,
    /// 2560 pixels wide.
    HighRes2560w,
}

/// One stored size of a listing photo.
#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct ListingImage {
    /// Unique identifier.
    pub id: Uuid,
    /// The listing this belongs to.
    pub listing_id: Uuid,
    /// The client's own ID for the file, used to match responses to uploads.
    pub client_file_id: String,
    /// Where the image is in the upload pipeline.
    pub status: ImageStatus,
    /// Image size.
    pub resolution: ImageResolution,
    /// The original image, for a resized variant.
    pub parent_id: Option<Uuid>,
    /// URL of the image.
    pub upload_url: Option<String>,
    /// MIME type.
    pub content_type: Option<String>,
    /// File size in bytes.
    pub size_bytes: Option<i64>,
    /// Position among the listing's photos.
    pub display_order: i32,
    /// Whether this is the primary photo.
    pub is_primary: bool,
    /// When this was created.
    pub created_at: DateTime<Utc>,
    /// When this was last changed.
    pub updated_at: DateTime<Utc>,
}

/// A listing with everything its page needs.
#[derive(Debug, Serialize, Deserialize)]
pub struct ListingDetails {
    /// The listing.
    pub listing: Listing,
    /// Its photos.
    pub images: Vec<ListingImage>,
    /// Display name of the host.
    pub owner_name: Option<String>,
    /// Its review ratings, if it has any.
    pub rating_summary: Option<common::models::ListingRatingSummary>,
}

/// A nightly rate for a date range, replacing the base rate.
#[derive(Debug, FromRow, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct PriceOverride {
    /// Unique identifier.
    pub id: Uuid,
    /// The listing this belongs to.
    pub listing_id: Uuid,
    /// First night the override covers.
    pub start_date: NaiveDate,
    /// Night after the last one the override covers.
    pub end_date: NaiveDate,
    /// Nightly rate during the override.
    pub nightly_rate: Decimal,
    /// Minimum stay for a stay touching the override.
    pub min_nights: i32,
    /// When this was created.
    pub created_at: DateTime<Utc>,
    /// When this was last changed.
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

/// A single-use token that lets a guest review a stay.
#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct ReviewToken {
    /// Unique identifier.
    pub id: Uuid,
    /// Single-use review token.
    pub token: String,
    /// The booking this belongs to.
    pub booking_id: Uuid,
    /// The guest who may leave the review.
    pub guest_id: Uuid,
    /// The listing to be reviewed.
    pub listing_id: Uuid,
    /// When the token starts working.
    pub valid_from: DateTime<Utc>,
    /// When the token stops working.
    pub expires_at: DateTime<Utc>,
    /// When the token was used, if it was.
    pub used_at: Option<DateTime<Utc>>,
    /// When this was created.
    pub created_at: DateTime<Utc>,
}

/// A guest's review of a stay.
#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Review {
    /// Unique identifier.
    pub id: Uuid,
    /// The booking this belongs to.
    pub booking_id: Uuid,
    /// The listing this belongs to.
    pub listing_id: Uuid,
    /// The guest.
    pub guest_id: Uuid,
    /// Cleanliness rating, from 1 to 5.
    pub cleanliness_rating: i32,
    /// Accuracy rating, from 1 to 5.
    pub accuracy_rating: i32,
    /// Location rating, from 1 to 5.
    pub location_rating: i32,
    /// Value rating, from 1 to 5.
    pub value_rating: i32,
    /// Average of the four category ratings.
    pub overall_rating: Decimal,
    /// Review text shown publicly.
    pub public_review_text: Option<String>,
    /// Feedback only the host sees.
    pub private_host_feedback: Option<String>,
    /// The host's public reply, if any.
    pub host_reply_text: Option<String>,
    /// When the host replied, if they have.
    pub host_replied_at: Option<DateTime<Utc>>,
    /// When this was created.
    pub created_at: DateTime<Utc>,
    /// When this was last changed.
    pub updated_at: DateTime<Utc>,
}

pub use common::models::MessageSenderRole;
/// Database name for the shared message sender role.
pub type DbMessageSenderRole = MessageSenderRole;

/// A message between a guest and a host about a booking.
#[derive(Debug, sqlx::FromRow, Serialize, Deserialize)]
pub struct DbBookingMessage {
    /// Unique identifier.
    pub id: Uuid,
    /// The booking this belongs to.
    pub booking_id: Uuid,
    /// The user who sent the message.
    pub sender_id: Uuid,
    /// Role the sender wrote as.
    pub sender_role: DbMessageSenderRole,
    /// Display name of the sender.
    pub sender_name: String,
    /// The message.
    pub message_text: String,
    /// When the message was read, if it has been.
    pub read_at: Option<DateTime<Utc>>,
    /// When this was created.
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

/// Who is on a booking, for checking access to its messages.
#[derive(Debug, sqlx::FromRow)]
pub struct BookingParties {
    /// The guest on the booking.
    pub guest_id: Uuid,
    /// The host of the listing.
    pub host_id: Uuid,
    /// Current status of the booking.
    pub status: BookingStatus,
}

pub use common::payout::PayoutStatus;
/// Database name for the shared payout status.
pub type DbPayoutStatus = PayoutStatus;

/// A row of the host payout ledger.
#[derive(Debug, sqlx::FromRow, Serialize, Deserialize)]
pub struct HostPayoutLedger {
    /// Unique identifier.
    pub id: Uuid,
    /// The booking this belongs to.
    pub booking_id: Uuid,
    /// The listing this belongs to.
    pub listing_id: Uuid,
    /// The host.
    pub host_id: Uuid,
    /// Currency of the amounts.
    pub currency: String,
    /// Host's gross for the booking, before commission.
    pub gross_amount: Decimal,
    /// Commission rate applied.
    pub platform_fee_pct: Decimal,
    /// Commission taken.
    pub platform_fee_amount: Decimal,
    /// Tax withheld from the payout.
    pub tax_withheld_amount: Decimal,
    /// Booking currency to host currency, effective at confirmation.
    pub exchange_rate: Decimal,
    /// What the host receives.
    pub net_payout_amount: Decimal,
    /// Payout status.
    pub status: DbPayoutStatus,
    /// Payment gateway reference, once sent.
    pub gateway_reference: Option<String>,
    /// Why the payout failed or was cancelled, if it did.
    pub failure_reason: Option<String>,
    /// When the payout was made, if it has been.
    pub payout_date: Option<DateTime<Utc>>,
    /// When this was created.
    pub created_at: DateTime<Utc>,
    /// When this was last changed.
    pub updated_at: DateTime<Utc>,
}

/// A ledger row with its booking and listing details.
#[derive(Debug, sqlx::FromRow, Serialize, Deserialize)]
pub struct DbPayoutLedgerEntry {
    /// Unique identifier.
    pub id: Uuid,
    /// The booking this belongs to.
    pub booking_id: Uuid,
    /// Confirmation code of the booking.
    pub booking_confirmation_code: Option<String>,
    /// The listing this belongs to.
    pub listing_id: Uuid,
    /// Name of the listing.
    pub listing_name: Option<String>,
    /// The host.
    pub host_id: Uuid,
    /// Name of the host.
    pub host_name: Option<String>,
    /// First night of the stay.
    pub check_in_date: Option<NaiveDate>,
    /// Departure date of the stay.
    pub check_out_date: Option<NaiveDate>,
    /// Currency of the amounts.
    pub currency: String,
    /// Host's gross for the booking, before commission.
    pub gross_amount: Decimal,
    /// Commission rate applied.
    pub platform_fee_pct: Decimal,
    /// Commission taken.
    pub platform_fee_amount: Decimal,
    /// Tax withheld from the payout.
    pub tax_withheld_amount: Decimal,
    /// Booking currency to host currency, effective at confirmation.
    pub exchange_rate: Decimal,
    /// What the host receives.
    pub net_payout_amount: Decimal,
    /// Part of the gross that is the cleaning fee, in this row's currency.
    pub cleaning_fee_amount: Decimal,
    /// The promo discount the host funded, in this row's currency.
    pub promo_discount_amount: Decimal,
    /// The promo code the booking redeemed, if any.
    pub promo_code: Option<String>,
    /// Payout status.
    pub status: DbPayoutStatus,
    /// Payment gateway reference, once sent.
    pub gateway_reference: Option<String>,
    /// Why the payout failed or was cancelled, if it did.
    pub failure_reason: Option<String>,
    /// When the payout was made, if it has been.
    pub payout_date: Option<DateTime<Utc>>,
    /// When this was created.
    pub created_at: DateTime<Utc>,
    /// When this was last changed.
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
            cleaning_fee_amount: e.cleaning_fee_amount,
            promo_discount_amount: e.promo_discount_amount,
            promo_code: e.promo_code,
            status: e.status,
            gateway_reference: e.gateway_reference,
            failure_reason: e.failure_reason,
            payout_date: e.payout_date,
            created_at: e.created_at,
            updated_at: e.updated_at,
        }
    }
}

/// Ledger totals. Sums are `None` when no rows match.
#[derive(Debug, sqlx::FromRow, Serialize, Deserialize)]
pub struct DbPayoutSummary {
    /// Sum of gross amounts.
    pub total_gross: Option<Decimal>,
    /// Sum of commission taken.
    pub total_platform_fee: Option<Decimal>,
    /// Sum of tax withheld.
    pub total_tax_withheld: Option<Decimal>,
    /// Sum of net payouts.
    pub total_net: Option<Decimal>,
    /// Sum of net payouts already paid.
    pub total_paid: Option<Decimal>,
    /// Sum of net payouts still pending.
    pub total_pending: Option<Decimal>,
    /// Sum of net payouts being processed.
    pub total_processing: Option<Decimal>,
    /// Number of ledger entries.
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
/// Database name for the shared email status.
pub type DbEmailStatus = EmailStatus;

/// An email waiting to be sent, or already sent.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct EmailOutbox {
    /// Unique identifier.
    pub id: Uuid,
    /// Who the email goes to.
    pub recipient_email: String,
    /// Subject line.
    pub subject: String,
    /// Template to render.
    pub template_id: String,
    /// Template data, as JSON.
    pub payload: serde_json::Value,
    /// Delivery status.
    pub status: DbEmailStatus,
    /// Delivery attempts so far.
    pub attempts: i32,
    /// Attempts allowed before giving up.
    pub max_retries: i32,
    /// Error from the latest attempt, if any.
    pub last_error: Option<String>,
    /// When it was sent, if it has been.
    pub sent_at: Option<DateTime<Utc>>,
    /// When this was created.
    pub created_at: DateTime<Utc>,
    /// When this was last changed.
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

// ============================================================================
// Enquiries and requests to book (#105)
// ============================================================================

/// The thread between one guest and one listing's host. Enquiries, requests,
/// quotes and bookings for the pair all post into it.
#[derive(Debug, Clone, FromRow, PartialEq, Eq)]
pub struct DbConversation {
    /// Unique identifier.
    pub id: Uuid,
    /// The listing.
    pub listing_id: Uuid,
    /// The guest.
    pub guest_id: Uuid,
    /// When the thread started.
    pub created_at: DateTime<Utc>,
    /// When the last message was posted, for inbox ordering.
    pub last_message_at: DateTime<Utc>,
}

/// A guest's question with dates and a guest count. Never blocks dates.
#[derive(Debug, Clone, FromRow)]
pub struct DbBookingEnquiry {
    /// Unique identifier.
    pub id: Uuid,
    /// The guest × listing thread.
    pub conversation_id: Uuid,
    /// The listing asked about.
    pub listing_id: Uuid,
    /// The guest asking.
    pub guest_id: Uuid,
    /// Check-in date asked about.
    pub date_from: NaiveDate,
    /// Check-out date asked about.
    pub date_to: NaiveDate,
    /// Guests asked about.
    pub number_of_persons: i32,
    /// The guest's display currency, used if the host quotes on it.
    pub currency: String,
    /// Open or closed.
    pub status: common::booking_request::EnquiryStatus,
    /// Why it closed, once closed.
    pub closed_reason: Option<common::booking_request::EnquiryClosedReason>,
    /// When the host or an admin first responded.
    pub first_response_at: Option<DateTime<Utc>>,
    /// When it was sent.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

/// A request to book, priced when submitted. Blocks its dates while live.
///
/// `Debug` includes the snapshot's promo terms, which hold the code string:
/// never log this struct, only its ids.
#[derive(Debug, Clone, FromRow)]
pub struct DbBookingRequest {
    /// Unique identifier.
    pub id: Uuid,
    /// The guest × listing thread.
    pub conversation_id: Uuid,
    /// The listing requested.
    pub listing_id: Uuid,
    /// The guest requesting.
    pub guest_id: Uuid,
    /// The enquiry it came from, if any.
    pub enquiry_id: Option<Uuid>,
    /// How it came about.
    pub origin: common::booking_request::RequestOrigin,
    /// Where it is in its lifecycle.
    pub status: common::booking_request::RequestStatus,
    /// Check-in date.
    pub date_from: NaiveDate,
    /// Check-out date.
    pub date_to: NaiveDate,
    /// Total guests.
    pub number_of_persons: i32,
    /// Payment currency.
    pub currency: String,
    /// The price as requested.
    pub price_snapshot: Json<common::booking_request::StayPriceSnapshot>,
    /// Guest breakdown and arrival details, copied to the booking.
    pub metadata: Json<BookingMetadata>,
    /// The promo code whose use this request reserves, while open or accepted.
    pub promo_code_id: Option<Uuid>,
    /// The live custom quote, once quoted.
    pub current_quote_id: Option<Uuid>,
    /// When the request or offer stops blocking its dates.
    pub expires_at: DateTime<Utc>,
    /// When the host or an admin first responded.
    pub first_response_at: Option<DateTime<Utc>>,
    /// Why the host declined, once declined.
    pub decline_reason: Option<common::booking_request::DeclineReason>,
    /// The host's note with a decline.
    pub decline_note: Option<String>,
    /// The booking it became, once converted.
    pub booking_id: Option<Uuid>,
    /// When it was submitted.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

impl DbBookingRequest {
    /// The `pending` booking this request becomes when the guest takes up the
    /// offer, priced exactly as `snapshot` (the request's own, or its current
    /// quote's).
    pub fn to_new_booking(
        &self,
        snapshot: &common::booking_request::StayPriceSnapshot,
        confirmation_code: String,
    ) -> NewBooking {
        NewBooking {
            confirmation_code,
            guest_id: self.guest_id,
            listing_id: self.listing_id,
            date_from: self.date_from,
            date_to: self.date_to,
            currency: snapshot.currency.clone(),
            daily_rate: snapshot.daily_rate,
            number_of_persons: self.number_of_persons,
            total_days: snapshot.total_days,
            sub_total_price: snapshot.sub_total_price,
            discount_value: snapshot.discount_value,
            tax_value: snapshot.tax_value,
            fee_breakdown: snapshot.fee_breakdown.clone(),
            total_price: snapshot.total_price,
            cancellation_policy: snapshot.cancellation_policy,
            metadata: self.metadata.0.clone(),
            door_access_code: None,
            cleaning_fee: snapshot.cleaning_fee,
            damage_deposit: snapshot.damage_deposit,
            platform_fee: snapshot.platform_fee,
            tax_rate: snapshot.tax_rate,
            exchange_rate: snapshot.exchange_rate,
            promo: snapshot.promo.as_ref().map(|p| NewBookingPromo {
                promo_code_id: p.promo_code_id,
                terms: p.terms.clone(),
                stay_discount: p.stay_discount,
                cleaning_discount: p.cleaning_discount,
            }),
        }
    }
}

/// A host's custom quote on a request. A newer quote supersedes it.
#[derive(Debug, Clone, FromRow)]
pub struct DbRequestQuote {
    /// Unique identifier.
    pub id: Uuid,
    /// The request quoted on.
    pub request_id: Uuid,
    /// The stay price the host entered, in the listing's base currency.
    pub stay_price_base: Decimal,
    /// The host's special terms (user text).
    pub terms: String,
    /// The quoted price, promo-free.
    pub price_snapshot: Json<common::booking_request::StayPriceSnapshot>,
    /// When the quote expires.
    pub valid_until: DateTime<Utc>,
    /// When a newer quote replaced it.
    pub superseded_at: Option<DateTime<Utc>>,
    /// The host or admin who sent it.
    pub created_by: Uuid,
    /// When it was sent.
    pub created_at: DateTime<Utc>,
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
            promo_code_id: None,
            promo_terms: None,
            promo_stay_discount: Decimal::ZERO,
            promo_cleaning_discount: Decimal::ZERO,
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

    #[test]
    fn booking_status_transitions_are_one_way() {
        use BookingStatus::*;
        let all = [Pending, Confirmed, Cancelled, Completed];
        let allowed = [
            (Pending, Confirmed),
            (Pending, Cancelled),
            (Confirmed, Cancelled),
            (Confirmed, Completed),
        ];
        for from in all {
            for to in all {
                let expected = from == to || allowed.contains(&(from, to));
                assert_eq!(from.can_transition_to(to), expected, "{from:?} -> {to:?}");
            }
        }
    }
}

#[cfg(test)]
mod request_model_tests {
    use super::*;
    use common::booking_request::{RequestOrigin, RequestStatus, SnapshotPromo, StayPriceSnapshot};
    use common::cancellation::CancellationPolicy;
    use common::models::FeeItem;
    use common::promo::{PromoDiscountType, PromoScope, PromoTerms};
    use rust_decimal_macros::dec;

    fn snapshot() -> StayPriceSnapshot {
        StayPriceSnapshot {
            currency: "EUR".to_string(),
            exchange_rate: dec!(0.920000),
            daily_rate: dec!(92.86),
            total_days: 7,
            sub_total_price: dec!(700.00),
            discount_value: Some(dec!(70.00)),
            discounted_subtotal: dec!(567.00),
            cleaning_fee: dec!(80.00),
            damage_deposit: dec!(300.00),
            platform_fee: dec!(28.35),
            tax_rate: dec!(0.15),
            tax_value: Some(dec!(97.05)),
            fee_breakdown: vec![FeeItem {
                name: "Cleaning Fee".to_string(),
                amount: dec!(80.00),
            }],
            total_price: dec!(1072.40),
            cancellation_policy: CancellationPolicy::Strict,
            promo: Some(SnapshotPromo {
                promo_code_id: Uuid::nil(),
                terms: PromoTerms {
                    code: "SUMMER-10".to_string(),
                    discount_type: PromoDiscountType::Percentage,
                    discount_value: dec!(10),
                    scope: PromoScope::Stay,
                    currency: None,
                    exchange_rate: Decimal::ONE,
                    min_nights: None,
                    min_booking_value: None,
                    stay_from: None,
                    stay_to: None,
                },
                stay_discount: dec!(63.00),
                cleaning_discount: dec!(0),
            }),
        }
    }

    fn request(snapshot: StayPriceSnapshot) -> DbBookingRequest {
        let now = Utc::now();
        DbBookingRequest {
            id: Uuid::now_v7(),
            conversation_id: Uuid::now_v7(),
            listing_id: Uuid::now_v7(),
            guest_id: Uuid::now_v7(),
            enquiry_id: None,
            origin: RequestOrigin::GuestRequest,
            status: RequestStatus::Accepted,
            date_from: NaiveDate::from_ymd_opt(2030, 8, 1).unwrap_or_default(),
            date_to: NaiveDate::from_ymd_opt(2030, 8, 8).unwrap_or_default(),
            number_of_persons: 4,
            currency: "EUR".to_string(),
            price_snapshot: Json(snapshot),
            metadata: Json(BookingMetadata {
                num_adults: 3,
                num_children: 1,
                message_to_host: Some("Anniversary trip".to_string()),
                ..BookingMetadata::default()
            }),
            promo_code_id: Some(Uuid::nil()),
            current_quote_id: None,
            expires_at: now,
            first_response_at: Some(now),
            decline_reason: None,
            decline_note: None,
            booking_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn the_new_booking_carries_every_snapshot_amount_exactly() {
        let snap = snapshot();
        let req = request(snap.clone());
        let booking = req.to_new_booking(&snap, "ABC123".to_string());
        assert_eq!(booking.confirmation_code, "ABC123");
        assert_eq!(booking.guest_id, req.guest_id);
        assert_eq!(booking.listing_id, req.listing_id);
        assert_eq!(
            (booking.date_from, booking.date_to),
            (req.date_from, req.date_to)
        );
        assert_eq!(booking.number_of_persons, 4);
        assert_eq!(booking.currency, "EUR");
        assert_eq!(booking.exchange_rate, dec!(0.920000));
        assert_eq!(booking.daily_rate, snap.daily_rate);
        assert_eq!(booking.total_days, 7);
        assert_eq!(booking.sub_total_price, snap.sub_total_price);
        assert_eq!(booking.discount_value, snap.discount_value);
        assert_eq!(booking.tax_value, snap.tax_value);
        assert_eq!(booking.tax_rate, snap.tax_rate);
        assert_eq!(booking.cleaning_fee, snap.cleaning_fee);
        assert_eq!(booking.damage_deposit, snap.damage_deposit);
        assert_eq!(booking.platform_fee, snap.platform_fee);
        assert_eq!(booking.total_price, snap.total_price);
        assert_eq!(booking.fee_breakdown, snap.fee_breakdown);
        assert_eq!(booking.cancellation_policy, CancellationPolicy::Strict);
        assert_eq!(booking.metadata.num_adults, 3);
        assert_eq!(
            booking.metadata.message_to_host.as_deref(),
            Some("Anniversary trip")
        );
        assert_eq!(booking.door_access_code, None);
        let promo = booking
            .promo
            .as_ref()
            .map(|p| (p.promo_code_id, p.stay_discount));
        assert_eq!(promo, Some((Uuid::nil(), dec!(63.00))));
    }

    #[test]
    fn a_quote_snapshot_without_a_promo_gives_a_booking_without_one() {
        let req = request(snapshot());
        let quoted = StayPriceSnapshot {
            promo: None,
            total_price: dec!(999.00),
            ..snapshot()
        };
        let booking = req.to_new_booking(&quoted, "Q1".to_string());
        assert!(booking.promo.is_none());
        assert_eq!(booking.total_price, dec!(999.00));
    }
}
