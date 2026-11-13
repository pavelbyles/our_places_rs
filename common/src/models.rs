use std::fmt;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

pub use crate::payout::*;

/// User authorization role within the platform.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "user_role", rename_all = "lowercase")
)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    /// Guest / reservation booker account.
    Booker,
    /// Property owner / villa host account.
    Host,
    /// System administrator with elevated access.
    Admin,
}

impl fmt::Display for UserRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Booker => write!(f, "Booker"),
            Self::Host => write!(f, "Host"),
            Self::Admin => write!(f, "Admin"),
        }
    }
}

impl std::str::FromStr for UserRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "booker" => Ok(Self::Booker),
            "host" => Ok(Self::Host),
            "admin" => Ok(Self::Admin),
            other => Err(format!("Unknown user role: {other}")),
        }
    }
}

/// Fields for creating a guest profile.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct NewBookerProfile {
    /// Emergency contacts, as JSON.
    pub emergency_contacts: Option<serde_json::Value>,
    /// Booking preferences, as JSON.
    pub booking_preferences: Option<serde_json::Value>,
    /// Loyalty details, as JSON.
    pub loyalty: Option<serde_json::Value>,
}

/// Fields for creating a host profile.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct NewHostProfile {
    /// Host verification status.
    pub verified_status: Option<String>,
    /// How the host is paid, as JSON.
    pub payout_details: Option<serde_json::Value>,
    /// Bio shown to guests.
    pub description: Option<String>,
}

/// Request to create a user.
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, PartialEq)]
pub struct NewUserRequest {
    /// Email address.
    #[validate(email)]
    pub email: String,
    /// Plain-text password. Never stored or logged.
    #[validate(length(min = 8))]
    pub password: String,
    /// First name.
    #[validate(length(min = 1))]
    pub first_name: String,
    /// Last name.
    #[validate(length(min = 1))]
    pub last_name: String,
    /// Phone number, if given.
    pub phone_number: Option<String>,
    /// Whether the account can be used.
    #[serde(default)]
    pub is_active: bool,
    /// Whether the email address has been verified.
    #[serde(default)]
    pub is_verified: bool,
    /// Free-form attributes, as JSON.
    pub attributes: Option<serde_json::Value>,
    /// Role names to assign.
    pub roles: Option<Vec<String>>,
    /// Guest profile to create with the account.
    pub booker_profile: Option<NewBookerProfile>,
    /// Host profile to create with the account.
    pub host_profile: Option<NewHostProfile>,
    /// Preferred currency code. Defaults when omitted.
    pub default_currency: Option<String>,
}

/// Request to sign in.
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, PartialEq)]
pub struct LoginRequest {
    /// Email address.
    #[validate(email)]
    pub email: String,
    /// Plain-text password. Never stored or logged.
    #[validate(length(min = 1))]
    pub password: String,
}

/// Request payload for verifying a user's email with an OTP code.
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, PartialEq)]
pub struct VerifyRequest {
    /// Email address of the user to verify.
    #[validate(email)]
    pub email: String,
    /// 6-character alphanumeric verification OTP code.
    #[validate(length(min = 1))]
    pub code: String,
}

/// Request payload for resending an email verification OTP code.
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, PartialEq)]
pub struct ResendVerificationRequest {
    /// Target email address for resending verification code.
    #[validate(email)]
    pub email: String,
}

/// Request payload for updating user account details.
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct UpdateUserRequest {
    /// Updated email address.
    pub email: Option<String>,
    /// Updated password plaintext.
    pub password: Option<String>,
    /// Updated first name.
    pub first_name: Option<String>,
    /// Updated last name.
    pub last_name: Option<String>,
    /// Updated contact phone number.
    pub phone_number: Option<String>,
    /// Updated active account status.
    #[serde(default)]
    pub is_active: Option<bool>,
    /// Updated email verification status.
    #[serde(default)]
    pub is_verified: Option<bool>,
    /// Dynamic user attributes JSON.
    pub attributes: Option<serde_json::Value>,
    /// Assigned role names.
    pub roles: Option<Vec<String>>,
    /// Updated booker profile information.
    pub booker_profile: Option<NewBookerProfile>,
    /// Updated host profile information.
    pub host_profile: Option<NewHostProfile>,
    /// User's preferred default display currency.
    pub default_currency: Option<String>,
    /// New public handle; cleaned up with [`crate::slug::user_slug`].
    #[serde(default)]
    pub slug: Option<String>,
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
    /// Slug of the owner/host, for `/listings?host={slug}` links.
    #[serde(default)]
    pub owner_slug: Option<String>,
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
    /// Optional cleaning fee (base currency).
    #[serde(default)]
    pub cleaning_fee: Option<Decimal>,
    /// Optional refundable damage deposit (base currency).
    #[serde(default)]
    pub damage_deposit: Option<Decimal>,
    /// Number of guests included in the base nightly rate before extra-guest fees apply.
    #[serde(default = "default_guests_included")]
    pub guests_included: i32,
    /// Optional fee per extra guest per night above `guests_included` (base currency).
    #[serde(default)]
    pub extra_guest_fee: Option<Decimal>,
    /// How the listing sells: instantly, on request, or both.
    #[serde(default)]
    pub booking_mode: crate::booking_request::BookingMode,
    /// Set by listing search when the requested currency cannot be quoted for
    /// this listing: prices are omitted rather than shown in another currency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_unavailable_in: Option<String>,
    /// Set by listing search when dates are given and the stay can be priced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stay_total: Option<StayTotal>,
}

/// What a stay costs, as shown on a search result. The same total as a quote
/// and a hold with no promo code.
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, PartialEq)]
pub struct StayTotal {
    /// Number of nights.
    pub nights: i32,
    /// Everything the guest pays, including the refundable deposit. Serialized as a string.
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    #[schema(value_type = String, example = "720.00")]
    pub total_amount: Decimal,
    /// The refundable deposit included in the total. Serialized as a string.
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    #[schema(value_type = String, example = "0.00")]
    pub damage_deposit: Decimal,
    /// Currency of the amounts.
    pub currency: String,
}

/// Itemized fee line item in booking price breakdowns.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct FeeItem {
    /// Descriptive name of the fee.
    pub name: String,
    /// Monetary amount of the fee. Serialized as a JSON string; deserializes from string or number.
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    #[schema(value_type = String, example = "15.00")]
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
    /// Optional business company name for B2B tax invoicing.
    #[serde(default)]
    pub business_company_name: Option<String>,
    /// Optional business tax registration ID for B2B tax invoicing.
    #[serde(default)]
    pub business_tax_id: Option<String>,
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
    /// Host cleaning fee.
    #[serde(default)]
    pub cleaning_fee: Option<Decimal>,
    /// Security / damage deposit held.
    #[serde(default)]
    pub damage_deposit: Option<Decimal>,
    /// Platform fee.
    #[serde(default)]
    pub platform_fee: Option<Decimal>,
    /// Applied statutory tax rate (e.g. 0.1500).
    #[serde(default)]
    pub tax_rate: Option<Decimal>,
    /// Base to payment exchange rate.
    #[serde(default)]
    pub exchange_rate: Option<Decimal>,
    /// The promo code the booking redeemed, if any.
    #[serde(default)]
    pub promo_code: Option<String>,
    /// The whole promo discount.
    #[serde(default)]
    pub promo_discount: Decimal,
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
    /// The host's name for the photo; `None` shows "Photo N".
    #[serde(default)]
    pub name: Option<String>,
}

/// Renames a listing photo; `None` or blank clears the name.
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, PartialEq, Eq, Default)]
pub struct RenameListingImageRequest {
    /// The new name, up to 80 characters.
    #[serde(default)]
    pub name: Option<String>,
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

/// `NewListingRequest::is_active` when the caller leaves it out.
fn default_listing_active() -> bool {
    true
}

/// What to search listings for. Absent fields don't filter.
#[derive(Debug, Deserialize, Serialize, IntoParams, ToSchema, Clone, Default)]
pub struct ListingFilter {
    /// Text the listing name must contain.
    pub name: Option<String>,
    /// Text the country must contain.
    pub country: Option<String>,
    /// Lowest nightly rate to include.
    pub min_price: Option<Decimal>,
    /// Highest nightly rate to include.
    pub max_price: Option<Decimal>,
    /// Structure types to include.
    #[serde(default)]
    pub structure_type: Vec<String>,
    /// Host email address to match, in full or in part.
    pub owner: Option<String>,
    /// Image size to return.
    pub resolution: Option<String>,
    /// Currency to convert prices into.
    pub currency: Option<String>,
    /// Optional owner user identifier filter for exact host scoping.
    #[serde(default)]
    pub user_id: Option<Uuid>,
    /// Optional host slug filter (public; matches `user.slug` exactly).
    #[serde(default)]
    pub host: Option<String>,
    /// Also return inactive listings. Off by default so guests never see them.
    /// Ignored when dates are given, because an inactive listing can't be booked.
    #[serde(default)]
    pub include_inactive: bool,
    /// First night of a stay the listing must be free for. Applies with `check_out`.
    #[serde(default)]
    pub check_in: Option<NaiveDate>,
    /// Departure date of a stay the listing must be free for. Applies with `check_in`.
    #[serde(default)]
    pub check_out: Option<NaiveDate>,
    /// Party size the listing must fit.
    #[serde(default)]
    pub guests: Option<i32>,
    /// Text matched against the listing's name, city or country.
    #[serde(default)]
    pub q: Option<String>,
}

/// Query parameters of the listing search.
#[derive(Debug, Deserialize, Serialize, IntoParams, ToSchema, Clone, Default)]
pub struct ListingQueryParams {
    /// Page number, starting at 1.
    pub page: Option<u32>,
    /// Results per page.
    pub per_page: Option<u32>,
    /// Text the listing name must contain.
    pub name: Option<String>,
    /// Text the country must contain.
    pub country: Option<String>,
    /// Lowest nightly rate to include.
    pub min_price: Option<Decimal>,
    /// Highest nightly rate to include.
    pub max_price: Option<Decimal>,
    /// Structure types to include. Read from the raw query string.
    #[serde(default, skip_deserializing)]
    pub structure_type: Vec<String>,
    /// Host email address to match, in full or in part.
    pub owner: Option<String>,
    /// Image size to return.
    pub resolution: Option<String>,
    /// Currency to convert prices into.
    pub currency: Option<String>,
    /// Optional owner user identifier query parameter for host listings scoping.
    pub user_id: Option<Uuid>,
    /// Optional host slug: only that host's listings.
    pub host: Option<String>,
    /// Also return inactive listings, for the admin portal and past-stay lookups.
    #[serde(default)]
    pub include_inactive: bool,
    /// Text matched against the listing's name, city or country.
    pub q: Option<String>,
    /// First night of the stay, `YYYY-MM-DD`. Read as text, so a bad date gets
    /// the typed refusal rather than the framework's own error.
    pub check_in: Option<String>,
    /// Departure date of the stay, `YYYY-MM-DD`. Read as text, like `check_in`.
    pub check_out: Option<String>,
    /// Party size. Read as text, like `check_in`.
    pub guests: Option<String>,
}

/// A user, as the API returns one.
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, PartialEq)]
pub struct UserResponse {
    /// Unique identifier.
    pub id: Uuid,
    /// Email address.
    pub email: String,
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
    /// When this was created.
    pub created_at: DateTime<Utc>,
    /// When this was last changed.
    pub updated_at: DateTime<Utc>,
    /// Free-form attributes, as JSON.
    pub attributes: serde_json::Value,
    /// Role names the user holds.
    pub roles: Vec<String>,
    /// Preferred currency code.
    pub default_currency: String,
    /// When this was soft-deleted, if it was.
    pub deleted_at: Option<DateTime<Utc>>,
    /// Unique public handle, e.g. `jane-brown`.
    #[serde(default)]
    pub slug: String,
}

/// XML wrapper for a list of users.
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct UsersWrapper {
    /// The users.
    pub user: Vec<UserResponse>,
}

/// Request for signed upload URLs.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct ImagePresignRequest {
    /// The images to upload.
    pub images: Vec<PendingImageMetadata>,
}

/// An image a client is about to upload.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct PendingImageMetadata {
    /// The client's own ID for the file, used to match responses to uploads.
    pub client_file_id: String, // Added to map the file UI-side
    /// MIME type.
    pub content_type: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Position among the listing's photos.
    pub display_order: i32,
}

/// Where to upload one image.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct ImagePresignResponse {
    /// The client's own ID for the file, used to match responses to uploads.
    pub client_file_id: String, // Mirrored back to the client
    /// ID of the new image record.
    pub file_id: uuid::Uuid,
    /// V4 signed URL the client uploads to directly.
    pub upload_url: String, // The GCS v4 Signed URL
    /// Extra headers the upload must send because they are part of the
    /// signature (the trace context for the image worker).
    #[serde(default)]
    pub upload_headers: std::collections::HashMap<String, String>,
}

/// Parameters required to create a new reservation checkout hold.
#[derive(Debug, Serialize, Deserialize, Clone, Validate, ToSchema)]
pub struct NewBookingRequest {
    /// Guest user identifier.
    pub guest_id: Uuid,
    /// Property listing identifier.
    pub listing_id: Uuid,

    /// Check-in start date.
    pub check_in: NaiveDate,
    /// Check-out departure date.
    pub check_out: NaiveDate,

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
    /// Estimated time of arrival.
    pub estimated_arrival_time: Option<String>,
    /// Whether this booking is for business purposes.
    pub is_business_trip: bool,
    /// Optional corporate company name for B2B tax compliance.
    #[serde(default)]
    pub business_company_name: Option<String>,
    /// Optional corporate Taxpayer Registration Number (TRN) for B2B compliance.
    #[serde(default)]
    pub business_tax_id: Option<String>,

    /// Selected checkout currency.
    pub currency: String,

    /// Name of agreed cancellation policy.
    pub agreed_cancellation_policy: String,

    /// Promo code to redeem, as typed. Blank means no code.
    #[serde(default)]
    #[validate(length(max = 256))]
    pub promo_code: Option<String>,
}

/// Request to price a stay for a signed-in guest.
#[derive(Debug, Serialize, Deserialize, Clone, Validate, ToSchema)]
pub struct StayQuoteRequest {
    /// Property listing identifier.
    pub listing_id: Uuid,
    /// First night.
    pub check_in: NaiveDate,
    /// Departure date.
    pub check_out: NaiveDate,
    /// Guests staying.
    pub guests: u32,
    /// Payment currency. Defaults to the listing's.
    #[serde(default)]
    pub currency: Option<String>,
    /// Promo code as typed. Blank means no code.
    #[serde(default)]
    #[validate(length(max = 256))]
    pub promo_code: Option<String>,
}

/// A promo code's effect on a quote.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, ToSchema)]
pub struct StayQuotePromo {
    /// The normalized code.
    pub code: String,
    /// The whole discount, across the stay and the cleaning fee.
    pub discount: Decimal,
}

/// Why a promo code couldn't be used.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, ToSchema)]
pub struct StayQuoteRefusal {
    /// The stable error code.
    pub code: String,
    /// The message to show the guest.
    pub message: String,
}

/// A stay's price. `subtotal - stay_discount - promo.discount + cleaning_fee +
/// tax_amount + platform_fee + damage_deposit` equals `total_amount`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, ToSchema)]
pub struct StayQuoteResponse {
    /// Number of nights.
    pub nights: i32,
    /// Payment currency.
    pub currency: String,
    /// Average nightly rate after seasonal pricing.
    pub effective_nightly_rate: Decimal,
    /// Nights and extra-guest fees, before any discount.
    pub subtotal: Decimal,
    /// Weekly or monthly discount.
    pub stay_discount: Decimal,
    /// The promo code applied, if any.
    pub promo: Option<StayQuotePromo>,
    /// Why a code wasn't applied, if one was given.
    pub promo_refusal: Option<StayQuoteRefusal>,
    /// Cleaning fee before any promo.
    pub cleaning_fee: Decimal,
    /// GCT.
    pub tax_amount: Decimal,
    /// Guest platform fee.
    pub platform_fee: Decimal,
    /// Refundable damage deposit.
    pub damage_deposit: Decimal,
    /// Amount due.
    pub total_amount: Decimal,
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
    /// Check-in start date if requesting a date change.
    pub date_from: Option<NaiveDate>,
    /// Check-out departure date if requesting a date change.
    pub date_to: Option<NaiveDate>,
    /// Updated total guest count (Admin override only).
    pub number_of_persons: Option<i32>,
}

/// Request payload to transfer a reservation to a promoted user account.
#[derive(Debug, Serialize, Deserialize, Clone, Validate, ToSchema)]
pub struct TransferBookingRequest {
    /// Recipient user identifier.
    #[schema(value_type = String, format = "uuid")]
    pub guest_id: Uuid,
}

/// Request to create a listing.
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
pub struct NewListingRequest {
    /// Whether guests can see and book the listing. Active if omitted.
    #[serde(default = "default_listing_active")]
    pub is_active: bool,

    /// Display name.
    #[schema(value_type = String, example = "Zen Loft")]
    #[validate(length(min = 1, message = "Name cannot be empty"))]
    pub name: String,

    /// The host who owns the listing.
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,

    /// Longer description.
    #[serde(default)]
    #[schema(value_type = String, example = "A zen place to be")]
    #[validate(length(
        max = 2000,
        message = "Description cannot be longer than 2000 characters"
    ))]
    pub description: Option<String>,

    /// Structure type name, such as `Villa`.
    #[schema(value_type = String, example = "Apartment")]
    pub listing_structure: String,

    /// Country.
    #[serde(default)]
    #[schema(value_type = String, example = "Jamaica")]
    #[validate(length(min = 1, message = "Country cannot be empty"))]
    pub country: String,

    /// Base nightly rate in `base_currency`.
    #[serde(default)]
    #[schema(value_type = String, example = "150.00")]
    pub price_per_night: Option<Decimal>,

    /// Discount percentage for stays of 7 nights or more.
    #[serde(default)]
    #[validate(custom(function = "validate_discount_percentage"))]
    pub weekly_discount_percentage: Option<Decimal>,

    /// Discount percentage for stays of 28 nights or more.
    #[serde(default)]
    #[validate(custom(function = "validate_discount_percentage"))]
    pub monthly_discount_percentage: Option<Decimal>,

    // --- NEW: Capacity & Room Breakdown ---
    /// Most guests allowed.
    #[schema(example = 2)]
    #[validate(range(min = 1, message = "Must allow at least 1 guest"))]
    pub max_guests: i32,

    /// Number of bedrooms.
    #[schema(example = 1)]
    #[validate(range(min = 0, message = "Bedrooms cannot be negative"))]
    pub bedrooms: i32,

    /// Number of beds.
    #[schema(example = 1)]
    #[validate(range(min = 0, message = "Beds cannot be negative"))]
    pub beds: i32,

    /// Number of full bathrooms.
    #[schema(example = 1)]
    #[validate(range(min = 0, message = "Bathrooms cannot be negative"))]
    pub full_bathrooms: i32,

    /// Number of half bathrooms.
    #[serde(default)]
    #[schema(example = 0)]
    #[validate(range(min = 0, message = "Half bathrooms cannot be negative"))]
    pub half_bathrooms: i32,

    // --- NEW: Dimensions & Location ---
    /// Floor area in square meters, if known.
    #[serde(default)]
    #[schema(example = 65)]
    pub square_meters: Option<i32>,

    /// Latitude, if known.
    #[serde(default)]
    #[schema(example = 18.2206)]
    pub latitude: Option<f64>,

    /// Longitude, if known.
    #[serde(default)]
    #[schema(example = -77.7990)]
    pub longitude: Option<f64>,

    // --- NEW: Dynamic Property Definitions (JSONB) ---
    /// Amenities and other details, as JSON. Host-edited sections are
    /// checked by [`crate::listing_details::validate_listing_details`].
    #[serde(default)]
    #[schema(value_type = Object)]
    #[validate(custom(function = "crate::listing_details::validate_listing_details"))]
    pub listing_details: Option<serde_json::Value>,

    /// City, if known.
    #[serde(default)]
    #[schema(value_type = String, example = "Kingston")]
    pub city: Option<String>,

    /// Currency the listing is priced in.
    #[serde(default = "default_base_currency")]
    #[schema(value_type = String, example = "USD")]
    pub base_currency: String,

    /// Minimum stay in nights.
    #[serde(default = "default_minimum_stay")]
    #[schema(example = 1)]
    #[validate(range(min = 1, message = "Minimum stay must be at least 1 night"))]
    pub minimum_stay: i32,

    /// Days to keep free between bookings, from 0 to 365.
    #[serde(default = "default_days_between_bookings")]
    #[schema(example = 0)]
    #[validate(range(
        min = 0,
        max = 365,
        message = "Days between bookings must be between 0 and 365"
    ))]
    pub days_between_bookings: i32,

    /// Platform commission on the host's gross, as a fraction such as `0.1000`.
    #[serde(default)]
    #[schema(value_type = String, example = "0.1000")]
    pub commission_pct: Option<Decimal>,

    /// One-time cleaning fee.
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "50.00")]
    #[validate(custom(function = "validate_fee"))]
    pub cleaning_fee: Option<Decimal>,

    /// Refundable damage deposit.
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "200.00")]
    #[validate(custom(function = "validate_fee"))]
    pub damage_deposit: Option<Decimal>,

    /// Number of guests included in the base nightly rate.
    #[serde(default = "default_guests_included")]
    #[schema(example = 1)]
    #[validate(range(min = 1, message = "Guests included must be at least 1"))]
    pub guests_included: i32,

    /// Optional fee per extra guest per night above `guests_included` (base currency).
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "25.00")]
    #[validate(custom(function = "validate_fee"))]
    pub extra_guest_fee: Option<Decimal>,
}

/// Validates that a stay discount is between 0 and 100 percent. An unbounded
/// value could overflow the discount calculation.
pub fn validate_discount_percentage(pct: &Decimal) -> Result<(), validator::ValidationError> {
    if *pct < Decimal::ZERO || *pct > Decimal::ONE_HUNDRED {
        return Err(validator::ValidationError::new("discount_out_of_range")
            .with_message("Discounts must be between 0 and 100 percent".into()));
    }
    Ok(())
}

/// Validates that a fee is between 0 and 1,000,000 and has at most 2 decimal places.
pub fn validate_fee(fee: &Decimal) -> Result<(), validator::ValidationError> {
    if *fee < Decimal::ZERO || *fee > Decimal::from(1_000_000) || fee.scale() > 2 {
        let mut err = validator::ValidationError::new("invalid_fee");
        err.message =
            Some("Fee must be between 0 and 1,000,000 with at most 2 decimal places".into());
        return Err(err);
    }
    Ok(())
}

/// Returns the default number of guests included in the base nightly rate (`1`).
pub fn default_guests_included() -> i32 {
    1
}

/// The minimum stay when the caller leaves it out.
pub fn default_minimum_stay() -> i32 {
    1
}

/// The gap between bookings when the caller leaves it out.
pub fn default_days_between_bookings() -> i32 {
    0
}

/// The base currency when the caller leaves it out.
pub fn default_base_currency() -> String {
    "USD".to_string()
}

/// Request to change a listing. Absent fields stay as they are.
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, Default)]
pub struct UpdatedListingRequest {
    /// Updated display name.
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "Zen Loft")]
    #[validate(length(min = 1, message = "Name cannot be empty"))]
    pub name: Option<String>,

    /// Updated longer description.
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "A zen place to be")]
    #[validate(length(
        max = 2000,
        message = "Description cannot be longer than 2000 characters"
    ))]
    pub description: Option<String>,

    /// Updated structure type name, such as `Villa`.
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "Apartment")]
    pub listing_structure: Option<String>,

    /// Updated country.
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "Jamaica")]
    #[validate(length(min = 1, message = "Country cannot be empty"))]
    pub country: Option<String>,

    /// Updated currency the listing is priced in.
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "USD")]
    pub base_currency: Option<String>,

    /// Updated base nightly rate in `base_currency`.
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "150.00")]
    pub price_per_night: Option<Decimal>,

    /// Updated discount percentage for stays of 7 nights or more.
    #[serde(default)]
    #[validate(custom(function = "validate_discount_percentage"))]
    pub weekly_discount_percentage: Option<Decimal>,

    /// Updated discount percentage for stays of 28 nights or more.
    #[serde(default)]
    #[validate(custom(function = "validate_discount_percentage"))]
    pub monthly_discount_percentage: Option<Decimal>,

    /// Updated most guests allowed.
    #[serde(default)]
    #[schema(example = 2)]
    #[validate(range(min = 1, message = "Must allow at least 1 guest"))]
    pub max_guests: Option<i32>,

    /// Updated number of bedrooms.
    #[serde(default)]
    #[schema(example = 1)]
    #[validate(range(min = 0, message = "Bedrooms cannot be negative"))]
    pub bedrooms: Option<i32>,

    /// Updated number of beds.
    #[serde(default)]
    #[schema(example = 1)]
    #[validate(range(min = 0, message = "Beds cannot be negative"))]
    pub beds: Option<i32>,

    /// Updated number of full bathrooms.
    #[serde(default)]
    #[schema(example = 1)]
    #[validate(range(min = 0, message = "Bathrooms cannot be negative"))]
    pub full_bathrooms: Option<i32>,

    /// Updated number of half bathrooms.
    #[serde(default)]
    #[schema(example = 0)]
    #[validate(range(min = 0, message = "Half bathrooms cannot be negative"))]
    pub half_bathrooms: Option<i32>,

    /// Updated floor area in square meters, if known.
    #[serde(default)]
    #[schema(example = 65)]
    pub square_meters: Option<i32>,

    /// Updated latitude, if known.
    #[serde(default)]
    #[schema(example = 18.2206)]
    pub latitude: Option<f64>,

    /// Updated longitude, if known.
    #[serde(default)]
    #[schema(example = -77.7990)]
    pub longitude: Option<f64>,

    /// Updated city, if known.
    #[serde(default)]
    #[schema(value_type = String, example = "Kingston")]
    pub city: Option<String>,

    /// Updated amenities and other details, as JSON. Host-edited sections
    /// are checked by [`crate::listing_details::validate_listing_details`].
    #[serde(default)]
    #[schema(value_type = Object)]
    #[validate(custom(function = "crate::listing_details::validate_listing_details"))]
    pub listing_details: Option<serde_json::Value>,

    /// Updated minimum stay in nights.
    #[serde(default)]
    #[schema(example = 1)]
    #[validate(range(min = 1, message = "Minimum stay must be at least 1 night"))]
    pub minimum_stay: Option<i32>,

    /// Updated days to keep free between bookings, from 0 to 365.
    #[serde(default)]
    #[schema(example = 0)]
    #[validate(range(
        min = 0,
        max = 365,
        message = "Days between bookings must be between 0 and 365"
    ))]
    pub days_between_bookings: Option<i32>,

    /// Updated whether guests can see and book it.
    #[serde(default)]
    pub is_active: Option<bool>,

    /// Updated platform commission on the host's gross, as a fraction such as `0.1000`.
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "0.1000")]
    pub commission_pct: Option<Decimal>,

    /// Updated one-time cleaning fee.
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "50.00")]
    #[validate(custom(function = "validate_fee"))]
    pub cleaning_fee: Option<Decimal>,

    /// Updated refundable damage deposit.
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "200.00")]
    #[validate(custom(function = "validate_fee"))]
    pub damage_deposit: Option<Decimal>,

    /// Updated number of guests included in the base nightly rate.
    #[serde(default)]
    #[schema(example = 2)]
    #[validate(range(min = 1, message = "Guests included must be at least 1"))]
    pub guests_included: Option<i32>,

    /// Updated fee per extra guest per night above `guests_included` (base currency).
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "25.00")]
    #[validate(custom(function = "validate_fee"))]
    pub extra_guest_fee: Option<Decimal>,
    /// Updated booking mode. Open requests and offers carry on either way.
    #[serde(default)]
    pub booking_mode: Option<crate::booking_request::BookingMode>,
}

/// A nightly rate for a date range, replacing the base rate.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
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

/// Request to add a price override.
#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema, PartialEq, Eq)]
pub struct CreatePriceOverrideRequest {
    /// First night the override covers.
    pub start_date: NaiveDate,
    /// Night after the last one the override covers.
    pub end_date: NaiveDate,
    /// Nightly rate during the override.
    pub nightly_rate: Decimal,
    /// Minimum stay for a stay touching the override.
    #[serde(default = "default_minimum_stay")]
    pub min_nights: i32,
}

/// Request to change a price override. Absent fields stay as they are.
#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema, PartialEq, Eq)]
pub struct UpdatePriceOverrideRequest {
    /// Updated first night the override covers.
    pub start_date: Option<NaiveDate>,
    /// Updated night after the last one the override covers.
    pub end_date: Option<NaiveDate>,
    /// Updated nightly rate during the override.
    pub nightly_rate: Option<Decimal>,
    /// Updated minimum stay for a stay touching the override.
    pub min_nights: Option<i32>,
}

/// The rate for one night of a stay.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct NightlyRateBreakdown {
    /// The night.
    pub date: NaiveDate,
    /// Rate for that night.
    pub rate: Decimal,
    /// Whether a price override set the rate.
    pub is_override: bool,
}

/// A stay's accommodation price, before fees and tax.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct DynamicPricingQuote {
    /// The rate for each night.
    pub nightly_breakdown: Vec<NightlyRateBreakdown>,
    /// Sum of the nightly rates.
    pub subtotal: Decimal,
    /// Average rate per night.
    pub effective_daily_rate: Decimal,
    /// Minimum stay that applies to these dates.
    pub required_min_nights: i32,
}

/// Request to submit a review.
#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema, PartialEq, Eq)]
pub struct NewReviewRequest {
    /// Single-use review token from the guest's email.
    pub token: String,
    /// Cleanliness rating, from 1 to 5.
    #[validate(range(min = 1, max = 5))]
    pub cleanliness_rating: i32,
    /// Accuracy rating, from 1 to 5.
    #[validate(range(min = 1, max = 5))]
    pub accuracy_rating: i32,
    /// Location rating, from 1 to 5.
    #[validate(range(min = 1, max = 5))]
    pub location_rating: i32,
    /// Value rating, from 1 to 5.
    #[validate(range(min = 1, max = 5))]
    pub value_rating: i32,
    /// Review text shown publicly.
    pub public_review_text: Option<String>,
    /// Feedback only the host sees.
    pub private_host_feedback: Option<String>,
}

impl NewReviewRequest {
    /// The average of the four category ratings, to 2 decimal places.
    pub fn calculate_overall_rating(&self) -> Decimal {
        let sum = self.cleanliness_rating
            + self.accuracy_rating
            + self.location_rating
            + self.value_rating;
        let avg = Decimal::from(sum) / Decimal::from(4);
        avg.round_dp(2)
    }
}

/// Request to reply to a review.
#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema, PartialEq, Eq)]
pub struct HostReplyRequest {
    /// The reply to show under the review.
    #[validate(length(min = 1, max = 2000))]
    pub reply_text: String,
}

/// What a review token is for, and whether it still works.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct ReviewTokenInfoResponse {
    /// Whether the token can still be used.
    pub is_valid: bool,
    /// Name of the listing to review.
    pub listing_name: String,
    /// First name of the guest.
    pub guest_first_name: String,
    /// First night of the stay.
    pub check_in: NaiveDate,
    /// Departure date of the stay.
    pub check_out: NaiveDate,
    /// When this stops being valid.
    pub expires_at: DateTime<Utc>,
    /// Why the token can't be used, if it can't.
    pub error_code: Option<String>,
}

/// A listing's review ratings.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct ListingRatingSummary {
    /// Average overall rating.
    pub overall_rating: Option<f64>,
    /// Average cleanliness rating.
    pub cleanliness_rating: Option<f64>,
    /// Average accuracy rating.
    pub accuracy_rating: Option<f64>,
    /// Average location rating.
    pub location_rating: Option<f64>,
    /// Average value rating.
    pub value_rating: Option<f64>,
    /// Number of reviews.
    pub review_count: i32,
    /// Number of reviews at each star rating.
    pub rating_distribution: std::collections::HashMap<i32, i32>,
}

/// A review, as guests see it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct ReviewResponse {
    /// Unique identifier.
    pub id: Uuid,
    /// First name of the guest.
    pub guest_first_name: String,
    /// Cleanliness rating, from 1 to 5.
    pub cleanliness_rating: i32,
    /// Accuracy rating, from 1 to 5.
    pub accuracy_rating: i32,
    /// Location rating, from 1 to 5.
    pub location_rating: i32,
    /// Value rating, from 1 to 5.
    pub value_rating: i32,
    /// Average of the four category ratings.
    pub overall_rating: f64,
    /// Review text shown publicly.
    pub public_review_text: Option<String>,
    /// The host's public reply, if any.
    pub host_reply_text: Option<String>,
    /// When the host replied, if they have.
    pub host_replied_at: Option<DateTime<Utc>>,
    /// When this was created.
    pub created_at: DateTime<Utc>,
}

/// Whether a guest can review a stay.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct BookingReviewEligibility {
    /// The booking this belongs to.
    pub booking_id: Uuid,
    /// Whether the guest can review this stay now.
    pub is_eligible: bool,
    /// The review token, when they can.
    pub token: Option<String>,
    /// Whether they already have.
    pub has_reviewed: bool,
    /// Days left to review, while the window is open.
    pub days_remaining: Option<i64>,
    /// Explanation to show the guest.
    pub status_message: String,
}

/// Request to create a session.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct CreateSessionRequest {
    /// Hash of the session token. The raw token is never stored.
    pub token_hash: String,
    /// The signed-in user.
    pub user_id: Uuid,
    /// Email address of the user.
    pub email: String,
    /// Display name of the user.
    pub name: String,
    /// Role the session was signed in as.
    pub role: String,
    /// Which portal the session belongs to, such as `guest`.
    pub namespace: String,
    /// How long the session lasts, in seconds.
    pub ttl_seconds: i64,
}

/// A session, as the API returns one.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct SessionResponse {
    /// Hash of the session token. The raw token is never stored.
    pub token_hash: String,
    /// The signed-in user.
    pub user_id: Uuid,
    /// Email address of the user.
    pub email: String,
    /// Display name of the user.
    pub name: String,
    /// Role the session was signed in as.
    pub role: String,
    /// Which portal the session belongs to, such as `guest`.
    pub namespace: String,
    /// When this was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last used.
    pub last_accessed_at: DateTime<Utc>,
    /// When the session expires, as a Unix timestamp in seconds.
    pub expires_at: i64,
}

/// Request to touch a session.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct RefreshSessionRequest {
    /// New length of the session in seconds. Keeps the current expiry when omitted.
    pub ttl_seconds: Option<i64>,
}

/// Sender role classification for booking correspondence.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "message_sender_role", rename_all = "lowercase")
)]
#[serde(rename_all = "lowercase")]
pub enum MessageSenderRole {
    /// Message sent by the guest/booker.
    Guest,
    /// Message sent by the villa host/owner.
    Host,
    /// Message sent by system administration.
    Admin,
}

impl fmt::Display for MessageSenderRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Guest => write!(f, "guest"),
            Self::Host => write!(f, "host"),
            Self::Admin => write!(f, "admin"),
        }
    }
}

impl std::str::FromStr for MessageSenderRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "guest" => Ok(Self::Guest),
            "host" => Ok(Self::Host),
            "admin" => Ok(Self::Admin),
            other => Err(format!("Unknown message sender role: {other}")),
        }
    }
}

/// Request to send a booking message.
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, PartialEq)]
pub struct CreateBookingMessageRequest {
    /// The message to send.
    #[validate(length(
        min = 1,
        max = 2000,
        message = "Message must be between 1 and 2000 characters"
    ))]
    pub message_text: String,
}

/// A message about a booking.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct BookingMessageResponse {
    /// Unique identifier.
    pub id: Uuid,
    /// The guest × listing conversation it belongs to.
    #[serde(default)]
    pub conversation_id: Option<Uuid>,
    /// The booking it is about, if any.
    pub booking_id: Option<Uuid>,
    /// The enquiry it is about, if any.
    #[serde(default)]
    pub enquiry_id: Option<Uuid>,
    /// The request to book it is about, if any.
    #[serde(default)]
    pub request_id: Option<Uuid>,
    /// The user who sent the message.
    pub sender_id: Uuid,
    /// Role the sender wrote as.
    pub sender_role: MessageSenderRole,
    /// Display name of the sender.
    pub sender_name: String,
    /// The message.
    pub message_text: String,
    /// When the message was read, if it has been.
    pub read_at: Option<DateTime<Utc>>,
    /// When this was created.
    pub created_at: DateTime<Utc>,
}

/// A booking's message thread.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct BookingMessagesWrapper {
    /// The messages in the thread.
    pub messages: Vec<BookingMessageResponse>,
    /// Messages the caller hasn't read.
    pub unread_count: i64,
}

/// Result of marking messages as read.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct MarkMessagesReadResponse {
    /// Messages newly marked as read.
    pub updated_count: u64,
}

/// Unread messages from guests across a host's listings (every listing for
/// an admin), for the admin portal's navigation badge.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct UnreadMessagesResponse {
    /// Unread guest-sent messages.
    pub unread_messages: i64,
}

/// Lifecycle status of a bilateral reservation change request.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "reservation_change_status", rename_all = "lowercase")
)]
#[serde(rename_all = "lowercase")]
pub enum ReservationChangeStatus {
    /// Change request is awaiting counterparty approval.
    Pending,
    /// Change request was approved by the counterparty and applied to the booking.
    Approved,
    /// Change request was declined by the counterparty.
    Declined,
    /// Change request was withdrawn by the initiator or cancelled with the booking.
    Cancelled,
    /// Change request automatically expired because its cutoff date arrived before approval.
    Expired,
}

impl fmt::Display for ReservationChangeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Approved => write!(f, "approved"),
            Self::Declined => write!(f, "declined"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Expired => write!(f, "expired"),
        }
    }
}

impl std::str::FromStr for ReservationChangeStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "declined" => Ok(Self::Declined),
            "cancelled" => Ok(Self::Cancelled),
            "expired" => Ok(Self::Expired),
            other => Err(format!("Unknown reservation change status: {other}")),
        }
    }
}

/// Role of the party who initiated a reservation change request.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "change_initiator_role", rename_all = "lowercase")
)]
#[serde(rename_all = "lowercase")]
pub enum ChangeInitiatorRole {
    /// Initiated by the booking guest (requires Host or Admin approval).
    Guest,
    /// Initiated by the listing host (requires Guest or Admin approval).
    Host,
    /// Initiated by a platform administrator.
    Admin,
}

impl fmt::Display for ChangeInitiatorRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Guest => write!(f, "guest"),
            Self::Host => write!(f, "host"),
            Self::Admin => write!(f, "admin"),
        }
    }
}

impl std::str::FromStr for ChangeInitiatorRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "guest" => Ok(Self::Guest),
            "host" => Ok(Self::Host),
            "admin" => Ok(Self::Admin),
            other => Err(format!("Unknown change initiator role: {other}")),
        }
    }
}

/// Payment settlement lifecycle status for a reservation change delta (`balance_due`).
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "change_settlement_status", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSettlementStatus {
    /// No monetary difference (`balance_due == 0.00`).
    NotRequired,
    /// Additional charge (`balance_due > 0.00`) is pending counterparty approval.
    PendingCharge,
    /// Additional charge was collected via the payment settlement gateway.
    Charged,
    /// Partial refund (`balance_due < 0.00`) is pending counterparty approval.
    PendingRefund,
    /// Partial refund was issued via the payment settlement gateway.
    Refunded,
    /// Payment gateway charge or refund attempt failed.
    Failed,
}

impl fmt::Display for ChangeSettlementStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRequired => write!(f, "not_required"),
            Self::PendingCharge => write!(f, "pending_charge"),
            Self::Charged => write!(f, "charged"),
            Self::PendingRefund => write!(f, "pending_refund"),
            Self::Refunded => write!(f, "refunded"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for ChangeSettlementStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "not_required" => Ok(Self::NotRequired),
            "pending_charge" => Ok(Self::PendingCharge),
            "charged" => Ok(Self::Charged),
            "pending_refund" => Ok(Self::PendingRefund),
            "refunded" => Ok(Self::Refunded),
            "failed" => Ok(Self::Failed),
            other => Err(format!("Unknown change settlement status: {other}")),
        }
    }
}

/// Immutable pricing snapshot persisted on a reservation change request.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct ReservationChangePricingSnapshot {
    /// Effective nightly rate in booking currency.
    pub daily_rate: Decimal,
    /// Proposed duration in nights.
    pub total_days: i32,
    /// Number of extra guests above `guests_included`.
    pub extra_guests: i32,
    /// Per-night fee per extra guest in booking currency.
    pub extra_guest_fee_per_night: Decimal,
    /// Total extra-guest fee across all nights in booking currency.
    pub extra_guest_fee_total: Decimal,
    /// Accommodation subtotal before discounts (includes `extra_guest_fee_total`).
    pub sub_total_price: Decimal,
    /// Weekly or monthly discount amount if applicable.
    pub discount_value: Option<Decimal>,
    /// Net accommodation subtotal after discount (`sub_total_price - discount_value`).
    pub discounted_subtotal: Decimal,
    /// One-time host cleaning fee, before any promo.
    pub cleaning_fee: Decimal,
    /// Promo discount on the stay.
    #[serde(default)]
    pub promo_stay_discount: Decimal,
    /// Promo discount on the cleaning fee.
    #[serde(default)]
    pub promo_cleaning_discount: Decimal,
    /// Cleaning fee after any promo.
    #[serde(default)]
    pub cleaning_net: Decimal,
    /// Whether the booking keeps its promo on these terms.
    #[serde(default)]
    pub promo_retained: bool,
    /// Why the booking would lose its promo on these terms, if it would.
    #[serde(default)]
    pub promo_lost_reason: Option<String>,
    /// Refundable security damage deposit.
    pub damage_deposit: Decimal,
    /// Guest platform service fee (5% of `discounted_subtotal`).
    pub platform_fee: Decimal,
    /// Statutory tax rate (e.g., `0.15`).
    pub tax_rate: Decimal,
    /// Statutory tax amount.
    pub tax_value: Option<Decimal>,
    /// Itemized fee breakdown.
    pub fee_breakdown: Vec<FeeItem>,
    /// Final guest total price in booking currency.
    pub total_price: Decimal,
    /// Host payout gross amount in host currency.
    pub host_gross_amount: Decimal,
    /// Host platform commission fee amount in host currency.
    pub host_platform_fee_amount: Decimal,
    /// Host net payout amount in host currency.
    pub host_net_payout_amount: Decimal,
}

/// Request payload to preview pricing and payout deltas for a reservation modification.
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, PartialEq, Default)]
pub struct ReservationChangeQuoteRequest {
    /// Proposed check-in date (defaults to current booking `date_from` if omitted).
    pub date_from: Option<NaiveDate>,
    /// Proposed check-out date (defaults to current booking `date_to` if omitted).
    pub date_to: Option<NaiveDate>,
    /// Proposed number of adults (defaults to current booking `metadata.num_adults` if omitted).
    pub num_adults: Option<u32>,
    /// Proposed number of children (defaults to current booking `metadata.num_children` if omitted).
    pub num_children: Option<u32>,
    /// Proposed number of infants (defaults to current booking `metadata.num_infants` if omitted).
    pub num_infants: Option<u32>,
    /// Proposed number of pets (defaults to current booking `metadata.num_pets` if omitted).
    pub num_pets: Option<u32>,
}

/// Response payload detailing pricing and payout deltas for a proposed reservation change.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct ReservationChangeQuoteResponse {
    /// Target booking identifier.
    pub booking_id: Uuid,
    /// Current check-in date.
    pub previous_date_from: NaiveDate,
    /// Current check-out date.
    pub previous_date_to: NaiveDate,
    /// Proposed check-in date.
    pub proposed_date_from: NaiveDate,
    /// Proposed check-out date.
    pub proposed_date_to: NaiveDate,
    /// Current total guest count.
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
    /// Current total price paid by the guest.
    pub previous_total_price: Decimal,
    /// Proposed total price for the modified reservation.
    pub proposed_total_price: Decimal,
    /// Difference owed by (`> 0`) or refunded to (`< 0`) the guest.
    pub balance_due: Decimal,
    /// Current net payout to the host.
    pub previous_net_payout: Decimal,
    /// Proposed net payout to the host.
    pub proposed_net_payout: Decimal,
    /// Net host payout delta (`proposed_net_payout - previous_net_payout`).
    pub host_payout_delta: Decimal,
    /// Detailed breakdown of the proposed pricing calculation.
    pub pricing_snapshot: ReservationChangePricingSnapshot,
    /// What happens to the booking's promo on these terms.
    pub promo_outcome: crate::promo::PromoOutcome,
    /// Date on which a pending request for these terms will auto-expire.
    pub expires_at_date: NaiveDate,
}

/// Request payload to submit a bilateral reservation change request for counterparty approval.
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, PartialEq, Default)]
pub struct CreateReservationChangeRequest {
    /// Proposed check-in date (defaults to current booking `date_from` if omitted).
    pub date_from: Option<NaiveDate>,
    /// Proposed check-out date (defaults to current booking `date_to` if omitted).
    pub date_to: Option<NaiveDate>,
    /// Proposed number of adults (defaults to current booking `metadata.num_adults` if omitted).
    pub num_adults: Option<u32>,
    /// Proposed number of children (defaults to current booking `metadata.num_children` if omitted).
    pub num_children: Option<u32>,
    /// Proposed number of infants (defaults to current booking `metadata.num_infants` if omitted).
    pub num_infants: Option<u32>,
    /// Proposed number of pets (defaults to current booking `metadata.num_pets` if omitted).
    pub num_pets: Option<u32>,
    /// Optional note explaining the reason for the requested change.
    #[validate(length(max = 1000, message = "Reason cannot exceed 1000 characters"))]
    pub reason: Option<String>,
}

/// Request payload for approving or declining a pending reservation change request.
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, PartialEq, Default)]
pub struct RespondReservationChangeRequest {
    /// Optional note from the counterparty explaining the approval or decline.
    #[validate(length(max = 1000, message = "Response note cannot exceed 1000 characters"))]
    pub response_note: Option<String>,
}

/// API response representing a reservation change request and its financial/approval status.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct ReservationChangeRequestResponse {
    /// Unique change request identifier.
    pub id: Uuid,
    /// Associated booking identifier.
    pub booking_id: Uuid,
    /// Associated listing identifier.
    pub listing_id: Uuid,
    /// User ID of the party who initiated the request.
    pub initiated_by_id: Uuid,
    /// Role of the party who initiated the request (`guest`, `host`, or `admin`).
    pub initiated_by_role: ChangeInitiatorRole,
    /// Current lifecycle status of the change request.
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
    /// Guest balance due (`> 0` charge, `< 0` refund, `0` none).
    pub balance_due: Decimal,
    /// Host net payout prior to the change.
    pub previous_net_payout: Decimal,
    /// Proposed host net payout.
    pub proposed_net_payout: Decimal,
    /// Host net payout delta.
    pub host_payout_delta: Decimal,
    /// Full pricing calculation snapshot.
    pub pricing_snapshot: ReservationChangePricingSnapshot,
    /// Payment settlement lifecycle status.
    pub settlement_status: ChangeSettlementStatus,
    /// External payment gateway transaction reference if settled.
    pub gateway_reference: Option<String>,
    /// Optional initiator reason note.
    pub reason: Option<String>,
    /// User ID of the counterparty who approved or declined the request.
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

    #[test]
    fn test_fee_item_serde_string_and_number_compatibility() {
        let item = FeeItem {
            name: "Cleaning Fee".into(),
            amount: Decimal::new(5000, 2), // 50.00
        };

        // 1. Serializes to string
        let json = serde_json::to_string(&item).expect("serialization should succeed");
        assert!(json.contains(r#""amount":"50.00""#));

        // 2. Deserializes from string format
        let from_str: FeeItem =
            serde_json::from_str(&json).expect("deserialization from str should succeed");
        assert_eq!(from_str, item);

        // 3. Deserializes from legacy number format
        let legacy_json = r#"{"name":"Cleaning Fee","amount":50.00}"#;
        let from_legacy: FeeItem =
            serde_json::from_str(legacy_json).expect("deserialization from number should succeed");
        assert_eq!(from_legacy.name, "Cleaning Fee");
        assert_eq!(from_legacy.amount, Decimal::new(5000, 2));
    }

    #[test]
    fn test_validate_fee() {
        assert!(validate_fee(&Decimal::ZERO).is_ok());
        assert!(validate_fee(&Decimal::new(5000, 2)).is_ok()); // 50.00
        assert!(validate_fee(&Decimal::from(1_000_000)).is_ok());

        // Negative
        assert!(validate_fee(&Decimal::new(-1, 0)).is_err());
        // > 1,000,000
        assert!(validate_fee(&Decimal::from(1_000_001)).is_err());
        // More than 2 decimal places
        assert!(validate_fee(&Decimal::new(50001, 3)).is_err()); // 50.001
    }

    #[test]
    fn test_user_role_display_and_parsing() {
        use std::str::FromStr;
        assert_eq!(UserRole::Booker.to_string(), "Booker");
        assert_eq!(UserRole::Host.to_string(), "Host");
        assert_eq!(UserRole::Admin.to_string(), "Admin");

        assert_eq!(UserRole::from_str("booker").unwrap(), UserRole::Booker);
        assert_eq!(UserRole::from_str("Host").unwrap(), UserRole::Host);
        assert_eq!(UserRole::from_str("ADMIN").unwrap(), UserRole::Admin);
        assert!(UserRole::from_str("unknown").is_err());

        let json = serde_json::to_string(&UserRole::Host).unwrap();
        assert_eq!(json, "\"host\"");
        let parsed: UserRole = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, UserRole::Host);
    }

    #[test]
    fn test_message_sender_role_display_and_parsing() {
        use std::str::FromStr;
        assert_eq!(MessageSenderRole::Guest.to_string(), "guest");
        assert_eq!(MessageSenderRole::Host.to_string(), "host");
        assert_eq!(MessageSenderRole::Admin.to_string(), "admin");

        assert_eq!(
            MessageSenderRole::from_str("guest").unwrap(),
            MessageSenderRole::Guest
        );
        assert_eq!(
            MessageSenderRole::from_str("Host").unwrap(),
            MessageSenderRole::Host
        );
        assert_eq!(
            MessageSenderRole::from_str("ADMIN").unwrap(),
            MessageSenderRole::Admin
        );
        assert!(MessageSenderRole::from_str("unknown").is_err());

        let json = serde_json::to_string(&MessageSenderRole::Guest).unwrap();
        assert_eq!(json, "\"guest\"");
        let parsed: MessageSenderRole = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, MessageSenderRole::Guest);
    }

    #[test]
    fn test_reservation_change_enums_and_validation() {
        assert_eq!(ReservationChangeStatus::Pending.to_string(), "pending");
        assert_eq!(ReservationChangeStatus::Approved.to_string(), "approved");
        assert_eq!(ReservationChangeStatus::Declined.to_string(), "declined");
        assert_eq!(ReservationChangeStatus::Cancelled.to_string(), "cancelled");
        assert_eq!(ReservationChangeStatus::Expired.to_string(), "expired");
        assert_eq!(
            ReservationChangeStatus::from_str("APPROVED").unwrap(),
            ReservationChangeStatus::Approved
        );
        assert!(ReservationChangeStatus::from_str("invalid").is_err());

        assert_eq!(ChangeInitiatorRole::Guest.to_string(), "guest");
        assert_eq!(ChangeInitiatorRole::Host.to_string(), "host");
        assert_eq!(ChangeInitiatorRole::Admin.to_string(), "admin");
        assert_eq!(
            ChangeInitiatorRole::from_str("HOST").unwrap(),
            ChangeInitiatorRole::Host
        );
        assert!(ChangeInitiatorRole::from_str("invalid").is_err());

        assert_eq!(
            ChangeSettlementStatus::NotRequired.to_string(),
            "not_required"
        );
        assert_eq!(
            ChangeSettlementStatus::PendingCharge.to_string(),
            "pending_charge"
        );
        assert_eq!(ChangeSettlementStatus::Charged.to_string(), "charged");
        assert_eq!(
            ChangeSettlementStatus::PendingRefund.to_string(),
            "pending_refund"
        );
        assert_eq!(ChangeSettlementStatus::Refunded.to_string(), "refunded");
        assert_eq!(ChangeSettlementStatus::Failed.to_string(), "failed");
        assert_eq!(
            ChangeSettlementStatus::from_str("PENDING_REFUND").unwrap(),
            ChangeSettlementStatus::PendingRefund
        );
        assert!(ChangeSettlementStatus::from_str("invalid").is_err());

        let valid_req = CreateReservationChangeRequest {
            reason: Some("Need two extra nights".to_string()),
            ..Default::default()
        };
        assert!(valid_req.validate().is_ok());

        let invalid_req = CreateReservationChangeRequest {
            reason: Some("x".repeat(1001)),
            ..Default::default()
        };
        assert!(invalid_req.validate().is_err());

        let invalid_resp = RespondReservationChangeRequest {
            response_note: Some("y".repeat(1001)),
        };
        assert!(invalid_resp.validate().is_err());
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
