//! Enquiries and requests to book: booking modes, the request lifecycle,
//! response windows and the refusals each step can return.
//!
//! Pure rules only. Storage and locking live in `db_core::booking_request`.

use crate::availability::jamaica_midnight_utc;
use crate::cancellation::CancellationPolicy;
use crate::models::FeeItem;
use crate::pricing::{BookingCalculator, Taxed};
use crate::promo::PromoTerms;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Shortest response window a new request may have, in minutes. A request
/// whose window would be capped below this by an imminent check-in is refused.
pub const MIN_REQUEST_WINDOW_MINUTES: i64 = 60;

/// How a listing sells: instantly, on request, or both.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema,
)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "booking_mode", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum BookingMode {
    /// Guests place a hold and pay straight away.
    #[default]
    Instant,
    /// Guests ask the host to approve the stay first.
    Request,
    /// Guests choose between booking now and requesting.
    Both,
}

impl BookingMode {
    /// Every mode, in display order.
    pub const ALL: [Self; 3] = [Self::Instant, Self::Request, Self::Both];

    /// Whether a guest may place an instant hold.
    pub fn allows_instant(self) -> bool {
        matches!(self, Self::Instant | Self::Both)
    }

    /// Whether a guest may submit a request to book.
    pub fn allows_request(self) -> bool {
        matches!(self, Self::Request | Self::Both)
    }

    /// The stored and form value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Instant => "instant",
            Self::Request => "request",
            Self::Both => "both",
        }
    }

    /// Parses a stored or form value.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.as_str() == value)
    }

    /// A label for the admin form.
    pub fn label(self) -> &'static str {
        match self {
            Self::Instant => "Instant book",
            Self::Request => "Request to book",
            Self::Both => "Both",
        }
    }
}

/// Where a request to book is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "booking_request_status", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    /// Waiting for the host; blocks its dates.
    Open,
    /// Accepted at the requested price; an offer that blocks its dates.
    Accepted,
    /// The host sent a custom quote; an offer that blocks its dates.
    Quoted,
    /// The host declined.
    Declined,
    /// Nobody acted before `expires_at`.
    Expired,
    /// The guest withdrew it.
    Withdrawn,
    /// The system cancelled it, for example when the guest booked instantly.
    Cancelled,
    /// The guest took up the offer and it became a `pending` booking.
    Converted,
}

impl RequestStatus {
    /// Every status.
    pub const ALL: [Self; 8] = [
        Self::Open,
        Self::Accepted,
        Self::Quoted,
        Self::Declined,
        Self::Expired,
        Self::Withdrawn,
        Self::Cancelled,
        Self::Converted,
    ];

    /// Whether the request still blocks its dates (before `expires_at`).
    pub fn is_live(self) -> bool {
        matches!(self, Self::Open | Self::Accepted | Self::Quoted)
    }

    /// Whether the guest can take it up and go to checkout.
    pub fn is_offer(self) -> bool {
        matches!(self, Self::Accepted | Self::Quoted)
    }

    /// Whether the request has reached a final state.
    pub fn is_terminal(self) -> bool {
        !self.is_live()
    }

    /// Whether moving from this status to `to` is allowed. A quote may replace
    /// a quote; terminal statuses never change.
    pub fn can_transition_to(self, to: Self) -> bool {
        use RequestStatus::{
            Accepted, Cancelled, Converted, Declined, Expired, Open, Quoted, Withdrawn,
        };
        match self {
            Open => matches!(
                to,
                Accepted | Quoted | Declined | Expired | Withdrawn | Cancelled
            ),
            Accepted => matches!(to, Declined | Expired | Withdrawn | Cancelled | Converted),
            Quoted => matches!(
                to,
                Quoted | Declined | Expired | Withdrawn | Cancelled | Converted
            ),
            Declined | Expired | Withdrawn | Cancelled | Converted => false,
        }
    }

    /// The stored value, also written to the history table.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Accepted => "accepted",
            Self::Quoted => "quoted",
            Self::Declined => "declined",
            Self::Expired => "expired",
            Self::Withdrawn => "withdrawn",
            Self::Cancelled => "cancelled",
            Self::Converted => "converted",
        }
    }

    /// A label for the portals.
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Awaiting host",
            Self::Accepted => "Accepted",
            Self::Quoted => "Quote received",
            Self::Declined => "Declined",
            Self::Expired => "Expired",
            Self::Withdrawn => "Withdrawn",
            Self::Cancelled => "Cancelled",
            Self::Converted => "Booked",
        }
    }
}

/// How a request came about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "booking_request_origin", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum RequestOrigin {
    /// The guest submitted it.
    GuestRequest,
    /// The host quoted on the guest's enquiry.
    EnquiryQuote,
}

/// Whether an enquiry is still open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "enquiry_status", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum EnquiryStatus {
    /// Open; shown in the host's inbox.
    Open,
    /// Closed; see the closed reason.
    Closed,
}

impl EnquiryStatus {
    /// The stored value, also written to the history table.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }
}

/// Why an enquiry was closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "enquiry_closed_reason", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum EnquiryClosedReason {
    /// The guest submitted a request to book from it.
    RequestSubmitted,
    /// The host sent a quote on it.
    Quoted,
    /// The host or an admin closed it.
    ClosedByHost,
    /// The guest booked the listing instantly.
    Booked,
    /// Its check-in date passed.
    Expired,
    /// A newer enquiry from the same guest on the same listing replaced it.
    Superseded,
}

impl EnquiryClosedReason {
    /// The stored value, also written to the history table.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RequestSubmitted => "request_submitted",
            Self::Quoted => "quoted",
            Self::ClosedByHost => "closed_by_host",
            Self::Booked => "booked",
            Self::Expired => "expired",
            Self::Superseded => "superseded",
        }
    }
}

/// Why a host declined a request. Shown to the guest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "decline_reason", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum DeclineReason {
    /// The dates can't be offered.
    DatesUnavailable,
    /// The group is too large or otherwise unsuitable.
    GroupSize,
    /// The stay is too short or too long.
    StayLength,
    /// An event (such as a wedding) isn't permitted.
    EventNotPermitted,
    /// Another reason, explained in the note.
    Other,
}

impl DeclineReason {
    /// Every reason, in display order.
    pub const ALL: [Self; 5] = [
        Self::DatesUnavailable,
        Self::GroupSize,
        Self::StayLength,
        Self::EventNotPermitted,
        Self::Other,
    ];

    /// Whether a note must explain the reason.
    pub fn requires_note(self) -> bool {
        self == Self::Other
    }

    /// The stored and form value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DatesUnavailable => "dates_unavailable",
            Self::GroupSize => "group_size",
            Self::StayLength => "stay_length",
            Self::EventNotPermitted => "event_not_permitted",
            Self::Other => "other",
        }
    }

    /// Parses a stored or form value.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == value)
    }

    /// The text shown to the guest.
    pub fn label(self) -> &'static str {
        match self {
            Self::DatesUnavailable => "The dates aren't available",
            Self::GroupSize => "The group size isn't suitable",
            Self::StayLength => "The length of stay isn't suitable",
            Self::EventNotPermitted => "Events aren't permitted",
            Self::Other => "Other",
        }
    }
}

/// Validates a decline: `Other` needs a non-blank note.
pub fn check_decline(reason: DeclineReason, note: Option<&str>) -> Result<(), RequestRefusal> {
    let has_note = note.is_some_and(|n| !n.trim().is_empty());
    if reason.requires_note() && !has_note {
        Err(RequestRefusal::DeclineNoteRequired)
    } else {
        Ok(())
    }
}

/// Why an enquiry or request action was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
pub enum RequestRefusal {
    /// The listing's booking mode doesn't allow this kind of booking.
    #[error("This property doesn't accept that kind of booking.")]
    BookingModeNotAllowed,
    /// The guest already has a live request on this listing.
    #[error("You already have an open request for this property.")]
    RequestAlreadyLive,
    /// The request is no longer in a state that allows this action.
    #[error("This request is no longer open.")]
    RequestNotOpen,
    /// The request or offer ran out before the action.
    #[error("This request has expired.")]
    RequestExpired,
    /// The quote ran out before the guest accepted it.
    #[error("This quote has expired.")]
    QuoteExpired,
    /// The host has since sent a newer quote.
    #[error("The host has sent a newer quote. Please review it before continuing.")]
    QuoteSuperseded,
    /// The custom stay price is missing, not positive or too large.
    #[error("Please enter a stay price above zero with at most 2 decimal places.")]
    InvalidQuotePrice,
    /// Check-in is too close for the host to respond in time.
    #[error("Check-in is too soon for a request. Please book instantly if available.")]
    RequestWindowTooShort,
    /// A decline with reason `other` needs a note.
    #[error("Please explain the reason for declining.")]
    DeclineNoteRequired,
    /// The host tried to enquire or request on their own listing.
    #[error("You can't book your own property.")]
    OwnListing,
    /// The caller isn't the guest, the listing's host or an admin, or the
    /// action isn't theirs to take.
    #[error("You don't have access to this request.")]
    NotParty,
}

impl RequestRefusal {
    /// Every refusal.
    pub const ALL: [Self; 11] = [
        Self::BookingModeNotAllowed,
        Self::RequestAlreadyLive,
        Self::RequestNotOpen,
        Self::RequestExpired,
        Self::QuoteExpired,
        Self::QuoteSuperseded,
        Self::InvalidQuotePrice,
        Self::RequestWindowTooShort,
        Self::DeclineNoteRequired,
        Self::OwnListing,
        Self::NotParty,
    ];

    /// The API error code.
    pub fn code(self) -> &'static str {
        match self {
            Self::BookingModeNotAllowed => "BOOKING_MODE_NOT_ALLOWED",
            Self::RequestAlreadyLive => "REQUEST_ALREADY_LIVE",
            Self::RequestNotOpen => "REQUEST_NOT_OPEN",
            Self::RequestExpired => "REQUEST_EXPIRED",
            Self::QuoteExpired => "QUOTE_EXPIRED",
            Self::QuoteSuperseded => "QUOTE_SUPERSEDED",
            Self::InvalidQuotePrice => "INVALID_QUOTE_PRICE",
            Self::RequestWindowTooShort => "REQUEST_WINDOW_TOO_SHORT",
            Self::DeclineNoteRequired => "DECLINE_NOTE_REQUIRED",
            Self::OwnListing => "OWN_LISTING",
            Self::NotParty => "NOT_REQUEST_PARTY",
        }
    }

    /// The HTTP status the API returns.
    pub fn http_status(self) -> u16 {
        match self {
            Self::InvalidQuotePrice | Self::DeclineNoteRequired => 400,
            Self::OwnListing | Self::NotParty => 403,
            Self::RequestWindowTooShort => 422,
            Self::BookingModeNotAllowed
            | Self::RequestAlreadyLive
            | Self::RequestNotOpen
            | Self::RequestExpired
            | Self::QuoteExpired
            | Self::QuoteSuperseded => 409,
        }
    }

    /// The message shown to the user.
    pub fn message(self) -> String {
        self.to_string()
    }

    /// The refusal for an API error code, if it is one.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.code() == code)
    }
}

/// The promo part of a [`StayPriceSnapshot`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotPromo {
    /// The code being redeemed.
    pub promo_code_id: Uuid,
    /// The code's terms when the stay was priced.
    pub terms: PromoTerms,
    /// Discount taken from the stay.
    pub stay_discount: Decimal,
    /// Discount taken from the cleaning fee.
    pub cleaning_discount: Decimal,
}

/// Everything a `pending` booking needs about its price, frozen when a
/// request is submitted or a quote is sent. Converting the offer copies these
/// amounts as they are, so a later rate, price or exchange-rate change can't
/// reprice what the guest accepted. Amounts serialize as strings, keeping
/// their scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StayPriceSnapshot {
    /// Payment currency.
    pub currency: String,
    /// Listing base currency to payment currency rate used.
    pub exchange_rate: Decimal,
    /// Nightly rate, for display.
    pub daily_rate: Decimal,
    /// Number of nights.
    pub total_days: i32,
    /// Accommodation subtotal before weekly or monthly discounts.
    pub sub_total_price: Decimal,
    /// Weekly or monthly discount, if any.
    pub discount_value: Option<Decimal>,
    /// Accommodation after every discount, including the promo.
    pub discounted_subtotal: Decimal,
    /// Cleaning fee before any promo discount.
    pub cleaning_fee: Decimal,
    /// Refundable damage deposit.
    pub damage_deposit: Decimal,
    /// Guest platform fee.
    pub platform_fee: Decimal,
    /// Statutory tax rate applied.
    pub tax_rate: Decimal,
    /// Statutory tax charged.
    pub tax_value: Option<Decimal>,
    /// Itemized fees.
    pub fee_breakdown: Vec<FeeItem>,
    /// Total the guest pays.
    pub total_price: Decimal,
    /// The cancellation policy the guest agreed to.
    pub cancellation_policy: CancellationPolicy,
    /// The promo code applied, if any.
    pub promo: Option<SnapshotPromo>,
}

impl StayPriceSnapshot {
    /// Freezes a priced stay. `promo` is the redeemed code's id and terms;
    /// its discounts are read from the calculator, so they can't disagree.
    pub fn from_calculator(
        calculator: &BookingCalculator<Taxed>,
        currency: String,
        exchange_rate: Decimal,
        cancellation_policy: CancellationPolicy,
        promo: Option<(Uuid, PromoTerms)>,
    ) -> Self {
        Self {
            currency,
            exchange_rate,
            daily_rate: calculator.actual_daily_rate,
            total_days: calculator.total_days,
            sub_total_price: calculator.sub_total_price,
            discount_value: calculator.discount_value,
            discounted_subtotal: calculator.discounted_subtotal,
            cleaning_fee: calculator.cleaning_fee,
            damage_deposit: calculator.damage_deposit,
            platform_fee: calculator.platform_fee,
            tax_rate: calculator.tax_rate,
            tax_value: calculator.tax_value,
            fee_breakdown: calculator.fee_breakdown.clone(),
            total_price: calculator.total_price,
            cancellation_policy,
            promo: promo.map(|(promo_code_id, terms)| SnapshotPromo {
                promo_code_id,
                terms,
                stay_discount: calculator.promo_stay_discount,
                cleaning_discount: calculator.promo_cleaning_discount,
            }),
        }
    }
}

/// A request-to-book submission from a signed-in guest.
#[derive(Debug, Clone, Serialize, Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct CreateBookingRequestBody {
    /// The listing to book.
    pub listing_id: Uuid,
    /// First night.
    pub check_in: NaiveDate,
    /// Departure date.
    pub check_out: NaiveDate,
    /// Number of adults; at least 1.
    pub num_adults: u32,
    /// Number of children.
    #[serde(default)]
    pub num_children: u32,
    /// Number of infants.
    #[serde(default)]
    pub num_infants: u32,
    /// Number of pets.
    #[serde(default)]
    pub num_pets: u32,
    /// A note to the host (user text; never logged).
    #[serde(default)]
    #[validate(length(max = 2000))]
    pub message: Option<String>,
    /// Estimated time of arrival.
    #[serde(default)]
    #[validate(length(max = 64))]
    pub estimated_arrival_time: Option<String>,
    /// Whether the stay is for business.
    #[serde(default)]
    pub is_business_trip: bool,
    /// Company name for a business stay.
    #[serde(default)]
    #[validate(length(max = 200))]
    pub business_company_name: Option<String>,
    /// Taxpayer registration number for a business stay.
    #[serde(default)]
    #[validate(length(max = 64))]
    pub business_tax_id: Option<String>,
    /// Payment currency.
    #[validate(length(min = 3, max = 3))]
    pub currency: String,
    /// The cancellation policy the guest agrees to.
    pub agreed_cancellation_policy: String,
    /// Promo code as typed. Blank means no code.
    #[serde(default)]
    #[validate(length(max = 256))]
    pub promo_code: Option<String>,
    /// The guest's enquiry this request follows from, if any.
    #[serde(default)]
    pub enquiry_id: Option<Uuid>,
}

/// A stay's price, line by line, in the payment currency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PriceBreakdown {
    /// Payment currency.
    pub currency: String,
    /// Number of nights.
    pub nights: i32,
    /// Nightly rate, for display.
    #[schema(value_type = String)]
    pub nightly_rate: Decimal,
    /// Accommodation before weekly or monthly discounts.
    #[schema(value_type = String)]
    pub subtotal: Decimal,
    /// Weekly or monthly discount.
    #[schema(value_type = String)]
    pub stay_discount: Decimal,
    /// Promo discount on the stay and cleaning fee.
    #[schema(value_type = String)]
    pub promo_discount: Decimal,
    /// Cleaning fee.
    #[schema(value_type = String)]
    pub cleaning_fee: Decimal,
    /// Statutory tax.
    #[schema(value_type = String)]
    pub tax: Decimal,
    /// Guest platform fee.
    #[schema(value_type = String)]
    pub platform_fee: Decimal,
    /// Refundable damage deposit.
    #[schema(value_type = String)]
    pub damage_deposit: Decimal,
    /// Total the guest pays.
    #[schema(value_type = String)]
    pub total: Decimal,
}

impl From<&StayPriceSnapshot> for PriceBreakdown {
    fn from(s: &StayPriceSnapshot) -> Self {
        let promo_discount = s
            .promo
            .as_ref()
            .map_or(Decimal::ZERO, |p| p.stay_discount + p.cleaning_discount);
        Self {
            currency: s.currency.clone(),
            nights: s.total_days,
            nightly_rate: s.daily_rate,
            subtotal: s.sub_total_price,
            stay_discount: s.discount_value.unwrap_or(Decimal::ZERO),
            promo_discount,
            cleaning_fee: s.cleaning_fee,
            tax: s.tax_value.unwrap_or(Decimal::ZERO),
            platform_fee: s.platform_fee,
            damage_deposit: s.damage_deposit,
            total: s.total_price,
        }
    }
}

/// A host's custom quote, as the guest and host see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct QuoteSummary {
    /// Quote id; the guest names it when accepting.
    pub id: Uuid,
    /// The stay price the host entered, in the listing's base currency.
    #[schema(value_type = String)]
    pub stay_price_base: Decimal,
    /// The host's special terms (user text).
    pub terms: String,
    /// The quoted price.
    pub price: PriceBreakdown,
    /// When the quote expires.
    pub valid_until: DateTime<Utc>,
    /// When it was sent.
    pub created_at: DateTime<Utc>,
}

/// A request to book, as the guest, host or admin sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BookingRequestResponse {
    /// Request id.
    pub id: Uuid,
    /// The guest × listing thread.
    pub conversation_id: Uuid,
    /// The listing.
    pub listing_id: Uuid,
    /// The guest.
    pub guest_id: Uuid,
    /// The enquiry it came from, if any.
    pub enquiry_id: Option<Uuid>,
    /// How it came about.
    pub origin: RequestOrigin,
    /// Where it is in its lifecycle.
    pub status: RequestStatus,
    /// First night.
    pub check_in: NaiveDate,
    /// Departure date.
    pub check_out: NaiveDate,
    /// Total guests.
    pub guests: i32,
    /// The price as requested.
    pub requested_price: PriceBreakdown,
    /// Whether the requested price includes a promo code.
    pub promo_applied: bool,
    /// The live custom quote, if the host sent one.
    pub quote: Option<QuoteSummary>,
    /// When the request or offer stops holding its dates.
    pub expires_at: DateTime<Utc>,
    /// When the host first responded.
    pub first_response_at: Option<DateTime<Utc>>,
    /// Why the host declined.
    pub decline_reason: Option<DeclineReason>,
    /// The host's note with a decline (user text).
    pub decline_note: Option<String>,
    /// The booking it became, once converted.
    pub booking_id: Option<Uuid>,
    /// When it was submitted.
    pub created_at: DateTime<Utc>,
}

/// A host's decline.
#[derive(Debug, Clone, Serialize, Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct DeclineRequestBody {
    /// Why, from the fixed list.
    pub reason: DeclineReason,
    /// More detail for the guest (user text); required when `reason` is `other`.
    #[serde(default)]
    #[validate(length(max = 1000))]
    pub note: Option<String>,
}

/// The guest takes up an offer and goes to checkout.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CheckoutRequestBody {
    /// The quote the guest saw, for a quoted request. Leave out for an
    /// accepted request.
    #[serde(default)]
    pub quote_id: Option<Uuid>,
}

/// Reads a decimal that must arrive as a JSON string, so a price never
/// passes through a float.
fn decimal_from_string<'de, D>(deserializer: D) -> Result<Decimal, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let text = String::deserialize(deserializer)?;
    text.trim()
        .parse::<Decimal>()
        .map_err(|_| serde::de::Error::custom("stay_price must be a decimal string"))
}

/// A host's custom quote.
#[derive(Debug, Clone, Serialize, Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct QuoteRequestBody {
    /// The whole stay's accommodation price, in the listing's base currency,
    /// as a decimal string (for example `"1500.00"`). Fees, GCT and the
    /// platform fee are added on top.
    #[serde(deserialize_with = "decimal_from_string")]
    #[schema(value_type = String, example = "1500.00")]
    pub stay_price: Decimal,
    /// Special terms for the guest (user text).
    #[serde(default)]
    #[validate(length(max = 2000))]
    pub terms: String,
    /// For a quote on an enquiry, the cancellation policy offered; defaults
    /// to Flexible. A quote on a request keeps the policy the guest agreed to.
    #[serde(default)]
    pub cancellation_policy: Option<String>,
}

/// A guest's question about a listing, with dates and a guest count.
#[derive(Debug, Clone, Serialize, Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct CreateEnquiryBody {
    /// The listing asked about.
    pub listing_id: Uuid,
    /// Check-in date asked about.
    pub check_in: NaiveDate,
    /// Check-out date asked about.
    pub check_out: NaiveDate,
    /// Guests asked about.
    #[validate(range(min = 1, max = 100))]
    pub guests: u32,
    /// The guest's display currency, used if the host quotes on it.
    #[validate(length(min = 3, max = 3))]
    pub currency: String,
    /// The question (user text; never logged).
    #[validate(length(min = 1, max = 2000))]
    pub message: String,
}

/// An enquiry, as the guest, host or admin sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EnquiryResponse {
    /// Enquiry id.
    pub id: Uuid,
    /// The guest × listing thread.
    pub conversation_id: Uuid,
    /// The listing.
    pub listing_id: Uuid,
    /// The guest.
    pub guest_id: Uuid,
    /// Check-in date asked about.
    pub check_in: NaiveDate,
    /// Check-out date asked about.
    pub check_out: NaiveDate,
    /// Guests asked about.
    pub guests: i32,
    /// The guest's display currency.
    pub currency: String,
    /// Open or closed.
    pub status: EnquiryStatus,
    /// Why it closed.
    pub closed_reason: Option<EnquiryClosedReason>,
    /// When the host first responded.
    pub first_response_at: Option<DateTime<Utc>>,
    /// When it was sent.
    pub created_at: DateTime<Utc>,
    /// What the host could earn at standard rates. Only the host and admins
    /// get it; it is never sent to the guest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_estimate: Option<HostEarningsEstimate>,
}

/// An approximate host payout for an enquiry: the listing's standard price
/// for the dates (no promo, no custom quote) minus the platform commission,
/// in the listing's base currency. GCT, the guest platform fee and the
/// damage deposit are never part of a payout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct HostEarningsEstimate {
    /// The listing's base currency.
    pub currency: String,
    /// Nights asked about.
    pub nights: i64,
    /// Accommodation after length discounts and the extra-guest fee.
    #[schema(value_type = String)]
    pub stay_subtotal: Decimal,
    /// Cleaning fee.
    #[schema(value_type = String)]
    pub cleaning_fee: Decimal,
    /// What the host's payout is computed from.
    #[schema(value_type = String)]
    pub gross: Decimal,
    /// The listing's commission, as a fraction (0.10 is 10%).
    #[schema(value_type = String)]
    pub commission_pct: Decimal,
    /// The platform's commission.
    #[schema(value_type = String)]
    pub commission: Decimal,
    /// The estimated payout.
    #[schema(value_type = String)]
    pub net: Decimal,
}

impl HostEarningsEstimate {
    /// Builds the estimate with the shared payout formula
    /// (`host_gross`, then `calculate_host_payout`), so it matches what the
    /// payout ledger would record for a booking at these figures.
    pub fn from_stay(
        currency: String,
        nights: i64,
        stay_subtotal: Decimal,
        cleaning_fee: Decimal,
        commission_pct: Decimal,
    ) -> Self {
        let gross = crate::pricing::host_gross(stay_subtotal, cleaning_fee, Decimal::ONE);
        let (commission, net) =
            crate::pricing::calculate_host_payout(gross, commission_pct, Decimal::ZERO);
        Self {
            currency,
            nights,
            stay_subtotal,
            cleaning_fee,
            gross,
            commission_pct,
            commission,
            net,
        }
    }
}

/// A guest's requests and enquiries, newest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BookingRequestList {
    /// The requests.
    pub requests: Vec<BookingRequestResponse>,
    /// The enquiries.
    #[serde(default)]
    pub enquiries: Vec<EnquiryResponse>,
}

/// Filters for the inbox.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
pub struct InboxQuery {
    /// `enquiry` or `request`.
    #[serde(default)]
    pub kind: Option<String>,
    /// Only this listing.
    #[serde(default)]
    pub listing_id: Option<Uuid>,
    /// 1-based page.
    #[serde(default)]
    pub page: Option<u32>,
}

/// An open enquiry or live request in the inbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InboxItem {
    /// `enquiry` or `request`.
    pub kind: String,
    /// The enquiry or request id.
    pub id: Uuid,
    /// The thread.
    pub conversation_id: Uuid,
    /// The listing.
    pub listing_id: Uuid,
    /// The listing's name.
    pub listing_name: String,
    /// The guest.
    pub guest_id: Uuid,
    /// The guest's first name.
    pub guest_name: String,
    /// Check-in.
    pub check_in: NaiveDate,
    /// Check-out.
    pub check_out: NaiveDate,
    /// Guests.
    pub guests: i32,
    /// Status.
    pub status: String,
    /// When it arrived.
    pub created_at: DateTime<Utc>,
    /// Seconds since it arrived.
    pub age_secs: i64,
    /// Seconds from arrival to the host's first response, if any.
    pub first_response_secs: Option<i64>,
    /// When a request's window ends.
    pub expires_at: Option<DateTime<Utc>>,
    /// Seconds left in a request's window.
    pub time_left_secs: Option<i64>,
    /// Whether a request is in the last part of its window.
    pub near_expiry: bool,
}

/// A host's response times over the last 90 days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct HostResponseMetrics {
    /// The host.
    pub host_id: Uuid,
    /// The host's first name.
    pub host_name: String,
    /// Median seconds to first response, if any were answered.
    pub median_first_response_secs: Option<i64>,
    /// Enquiries and requests received.
    pub received: i64,
    /// Requests received.
    pub requests: i64,
    /// Requests that expired with no response.
    pub expired_unanswered: i64,
    /// Share of requests that expired with no response, in whole percent.
    pub expired_unanswered_pct: i64,
}

/// The inbox page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InboxResponse {
    /// Items, soonest-expiring requests first.
    pub items: Vec<InboxItem>,
    /// Response metrics per host (only the caller's for a host).
    pub host_metrics: Vec<HostResponseMetrics>,
    /// This page, 1-based.
    pub page: u32,
    /// Items per page.
    pub per_page: u32,
}

/// When a new request's response window ends: `hours` from `now`, but never
/// after check-in day starts in Jamaica. A window capped below
/// [`MIN_REQUEST_WINDOW_MINUTES`] is refused, so the host always has time.
pub fn response_window(
    now: DateTime<Utc>,
    hours: i64,
    check_in: NaiveDate,
) -> Result<DateTime<Utc>, RequestRefusal> {
    let end = capped_end(now, hours, check_in);
    if end - now < Duration::minutes(MIN_REQUEST_WINDOW_MINUTES) {
        Err(RequestRefusal::RequestWindowTooShort)
    } else {
        Ok(end)
    }
}

/// When an offer (an accepted request or a quote) made at `now` ends:
/// `hours` from `now`, capped at check-in day in Jamaica. Refused with
/// [`RequestRefusal::RequestExpired`] once that cap has passed.
pub fn offer_window(
    now: DateTime<Utc>,
    hours: i64,
    check_in: NaiveDate,
) -> Result<DateTime<Utc>, RequestRefusal> {
    let end = capped_end(now, hours, check_in);
    if end <= now {
        Err(RequestRefusal::RequestExpired)
    } else {
        Ok(end)
    }
}

fn capped_end(now: DateTime<Utc>, hours: i64, check_in: NaiveDate) -> DateTime<Utc> {
    (now + Duration::hours(hours)).min(jamaica_midnight_utc(check_in))
}

/// Whether a live window is in its last `pct` percent: the inbox badge and
/// the host reminder use this. False once the window has ended.
pub fn is_near_expiry(
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    now: DateTime<Utc>,
    pct: i64,
) -> bool {
    if now >= expires_at {
        return false;
    }
    let window = (expires_at - created_at).num_seconds().max(0);
    let remaining = (expires_at - now).num_seconds();
    remaining.saturating_mul(100) <= window.saturating_mul(pct)
}

#[cfg(test)]
mod booking_request_tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        match Utc.with_ymd_and_hms(y, m, d, h, min, 0).single() {
            Some(t) => t,
            None => panic!("bad test time"),
        }
    }

    fn day(s: &str) -> NaiveDate {
        match NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            Ok(d) => d,
            Err(e) => panic!("bad test date {s}: {e}"),
        }
    }

    #[test]
    fn modes_allow_the_right_bookings() {
        assert!(BookingMode::Instant.allows_instant());
        assert!(!BookingMode::Instant.allows_request());
        assert!(!BookingMode::Request.allows_instant());
        assert!(BookingMode::Request.allows_request());
        assert!(BookingMode::Both.allows_instant());
        assert!(BookingMode::Both.allows_request());
        assert_eq!(BookingMode::default(), BookingMode::Instant);
        for mode in BookingMode::ALL {
            assert_eq!(BookingMode::parse(mode.as_str()), Some(mode));
        }
        assert_eq!(BookingMode::parse("sometimes"), None);
    }

    #[test]
    fn transition_table_is_exact() {
        use RequestStatus::*;
        let allowed = [
            (Open, Accepted),
            (Open, Quoted),
            (Open, Declined),
            (Open, Expired),
            (Open, Withdrawn),
            (Open, Cancelled),
            (Accepted, Declined),
            (Accepted, Expired),
            (Accepted, Withdrawn),
            (Accepted, Cancelled),
            (Accepted, Converted),
            (Quoted, Quoted),
            (Quoted, Declined),
            (Quoted, Expired),
            (Quoted, Withdrawn),
            (Quoted, Cancelled),
            (Quoted, Converted),
        ];
        for from in RequestStatus::ALL {
            for to in RequestStatus::ALL {
                let expected = allowed.contains(&(from, to));
                assert_eq!(from.can_transition_to(to), expected, "{from:?} -> {to:?}");
            }
        }
    }

    #[test]
    fn terminal_states_have_no_way_out() {
        for from in RequestStatus::ALL.into_iter().filter(|s| s.is_terminal()) {
            assert!(
                RequestStatus::ALL
                    .into_iter()
                    .all(|to| !from.can_transition_to(to)),
                "{from:?}"
            );
        }
        let live: Vec<_> = RequestStatus::ALL
            .into_iter()
            .filter(|s| s.is_live())
            .collect();
        assert_eq!(
            live,
            [
                RequestStatus::Open,
                RequestStatus::Accepted,
                RequestStatus::Quoted
            ]
        );
    }

    #[test]
    fn response_window_is_the_configured_hours_when_check_in_is_far() {
        let now = at(2030, 6, 1, 12, 0);
        assert_eq!(
            response_window(now, 24, day("2030-06-10")),
            Ok(at(2030, 6, 2, 12, 0))
        );
    }

    #[test]
    fn response_window_is_capped_at_check_in_midnight_in_jamaica() {
        // Check-in 2030-06-02 starts at 05:00 UTC.
        let now = at(2030, 6, 1, 12, 0);
        assert_eq!(
            response_window(now, 24, day("2030-06-02")),
            Ok(at(2030, 6, 2, 5, 0))
        );
    }

    #[test]
    fn response_window_under_an_hour_is_refused() {
        let check_in = day("2030-06-02");
        assert_eq!(
            response_window(at(2030, 6, 2, 4, 1), 24, check_in),
            Err(RequestRefusal::RequestWindowTooShort)
        );
        assert_eq!(
            response_window(at(2030, 6, 2, 4, 0), 24, check_in),
            Ok(at(2030, 6, 2, 5, 0))
        );
        // Check-in already started.
        assert_eq!(
            response_window(at(2030, 6, 3, 0, 0), 24, check_in),
            Err(RequestRefusal::RequestWindowTooShort)
        );
    }

    #[test]
    fn offer_window_is_capped_and_refused_once_check_in_starts() {
        let check_in = day("2030-06-02");
        assert_eq!(
            offer_window(at(2030, 5, 20, 0, 0), 48, check_in),
            Ok(at(2030, 5, 22, 0, 0))
        );
        assert_eq!(
            offer_window(at(2030, 6, 2, 4, 50), 48, check_in),
            Ok(at(2030, 6, 2, 5, 0))
        );
        assert_eq!(
            offer_window(at(2030, 6, 2, 5, 0), 48, check_in),
            Err(RequestRefusal::RequestExpired)
        );
    }

    #[test]
    fn near_expiry_is_the_last_pct_of_the_window() {
        let created = at(2030, 6, 1, 0, 0);
        let expires = at(2030, 6, 2, 0, 0); // 24h window, 25% = 6h
        assert!(!is_near_expiry(
            created,
            expires,
            at(2030, 6, 1, 17, 59),
            25
        ));
        assert!(is_near_expiry(created, expires, at(2030, 6, 1, 18, 0), 25));
        assert!(is_near_expiry(created, expires, at(2030, 6, 1, 23, 59), 25));
        assert!(!is_near_expiry(created, expires, expires, 25));
        assert!(!is_near_expiry(created, expires, at(2030, 6, 3, 0, 0), 25));
        assert!(is_near_expiry(created, expires, at(2030, 6, 1, 0, 30), 99));
        assert!(!is_near_expiry(created, expires, at(2030, 6, 1, 23, 0), 1));
        assert!(is_near_expiry(created, expires, at(2030, 6, 1, 23, 50), 1));
    }

    #[test]
    fn refusal_codes_are_unique_and_map_to_their_status() {
        let mut codes: Vec<_> = RequestRefusal::ALL.iter().map(|r| r.code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), RequestRefusal::ALL.len());
        for refusal in RequestRefusal::ALL {
            assert!(
                [400, 403, 409, 422].contains(&refusal.http_status()),
                "{refusal:?}"
            );
            assert_eq!(RequestRefusal::from_code(refusal.code()), Some(refusal));
            assert!(!refusal.message().is_empty());
        }
        assert_eq!(RequestRefusal::BookingModeNotAllowed.http_status(), 409);
        assert_eq!(RequestRefusal::RequestWindowTooShort.http_status(), 422);
        assert_eq!(RequestRefusal::InvalidQuotePrice.http_status(), 400);
        assert_eq!(RequestRefusal::NotParty.http_status(), 403);
    }

    #[test]
    fn other_needs_a_note() {
        assert_eq!(
            check_decline(DeclineReason::Other, None),
            Err(RequestRefusal::DeclineNoteRequired)
        );
        assert_eq!(
            check_decline(DeclineReason::Other, Some("   ")),
            Err(RequestRefusal::DeclineNoteRequired)
        );
        assert_eq!(
            check_decline(DeclineReason::Other, Some("Renovations")),
            Ok(())
        );
        for reason in DeclineReason::ALL
            .into_iter()
            .filter(|r| !r.requires_note())
        {
            assert_eq!(check_decline(reason, None), Ok(()));
            assert_eq!(DeclineReason::parse(reason.as_str()), Some(reason));
        }
    }
}

#[cfg(test)]
mod quote_body_tests {
    use super::QuoteRequestBody;

    #[test]
    fn stay_price_is_read_only_from_a_string() {
        let ok: Result<QuoteRequestBody, _> =
            serde_json::from_value(serde_json::json!({ "stay_price": "1500.50" }));
        assert_eq!(
            ok.map(|b| b.stay_price.to_string()).ok().as_deref(),
            Some("1500.50")
        );
        for bad in [
            serde_json::json!({ "stay_price": 1500.5 }),
            serde_json::json!({ "stay_price": "abc" }),
            serde_json::json!({}),
        ] {
            assert!(
                serde_json::from_value::<QuoteRequestBody>(bad.clone()).is_err(),
                "{bad}"
            );
        }
    }
}

#[cfg(test)]
mod price_snapshot_tests {
    use super::*;
    use crate::promo::{PromoDiscountType, PromoScope};
    use rust_decimal_macros::dec;

    fn terms() -> PromoTerms {
        PromoTerms {
            code: "SUMMER-10".to_string(),
            discount_type: PromoDiscountType::Percentage,
            discount_value: dec!(10),
            scope: PromoScope::StayAndCleaning,
            currency: None,
            exchange_rate: Decimal::ONE,
            min_nights: None,
            min_booking_value: None,
            stay_from: None,
            stay_to: None,
        }
    }

    fn priced(promo: Option<&PromoTerms>) -> BookingCalculator<Taxed> {
        BookingCalculator::with_subtotal(dec!(150.00), 3, dec!(450.00))
            .apply_discounts(None, None)
            .with_fees(Some(dec!(60.00)), Some(dec!(250.00)))
            .apply_promo(promo)
            .apply_taxes(dec!(0.15))
            .finalize()
    }

    fn snapshot(promo: bool) -> (BookingCalculator<Taxed>, StayPriceSnapshot) {
        let terms = terms();
        let calculator = priced(promo.then_some(&terms));
        let snapshot = StayPriceSnapshot::from_calculator(
            &calculator,
            "USD".to_string(),
            dec!(1.000000),
            CancellationPolicy::Moderate,
            promo.then(|| (Uuid::nil(), terms)),
        );
        (calculator, snapshot)
    }

    #[test]
    fn snapshot_matches_the_calculator_field_for_field() {
        let (calc, snap) = snapshot(true);
        assert_eq!(snap.daily_rate, calc.actual_daily_rate);
        assert_eq!(snap.total_days, calc.total_days);
        assert_eq!(snap.sub_total_price, calc.sub_total_price);
        assert_eq!(snap.discount_value, calc.discount_value);
        assert_eq!(snap.discounted_subtotal, calc.discounted_subtotal);
        assert_eq!(snap.cleaning_fee, calc.cleaning_fee);
        assert_eq!(snap.damage_deposit, calc.damage_deposit);
        assert_eq!(snap.platform_fee, calc.platform_fee);
        assert_eq!(snap.tax_rate, calc.tax_rate);
        assert_eq!(snap.tax_value, calc.tax_value);
        assert_eq!(snap.fee_breakdown, calc.fee_breakdown);
        assert_eq!(snap.total_price, calc.total_price);
        let promo = snap
            .promo
            .as_ref()
            .map(|p| (p.stay_discount, p.cleaning_discount));
        assert_eq!(
            promo,
            Some((calc.promo_stay_discount, calc.promo_cleaning_discount))
        );
        assert!(calc.promo_stay_discount > Decimal::ZERO);
    }

    #[test]
    fn snapshot_without_a_promo_has_none() {
        let (_, snap) = snapshot(false);
        assert_eq!(snap.promo, None);
    }

    #[test]
    fn json_round_trip_keeps_every_amount_and_its_scale() {
        let (_, snap) = snapshot(true);
        let json = match serde_json::to_value(&snap) {
            Ok(json) => json,
            Err(e) => panic!("serialize: {e}"),
        };
        // Amounts are strings, so JSONB can't turn them into floats.
        assert_eq!(
            json["total_price"],
            serde_json::json!(snap.total_price.to_string())
        );
        assert_eq!(json["exchange_rate"], serde_json::json!("1.000000"));
        let back: StayPriceSnapshot = match serde_json::from_value(json) {
            Ok(back) => back,
            Err(e) => panic!("deserialize: {e}"),
        };
        assert_eq!(back, snap);
        assert_eq!(back.exchange_rate.scale(), 6);
        assert_eq!(back.total_price.to_string(), snap.total_price.to_string());
        assert_eq!(back.cleaning_fee.to_string(), "60.00");
    }
}

#[cfg(test)]
mod host_earnings_tests {
    use super::HostEarningsEstimate;
    use rust_decimal_macros::dec;

    fn estimate(
        subtotal: rust_decimal::Decimal,
        pct: rust_decimal::Decimal,
    ) -> HostEarningsEstimate {
        HostEarningsEstimate::from_stay("USD".into(), 3, subtotal, dec!(80.00), pct)
    }

    #[test]
    fn host_earnings_follow_the_payout_formula() {
        let e = estimate(dec!(900.00), dec!(0.1000));
        assert_eq!(e.gross, dec!(980.00));
        assert_eq!(e.commission, dec!(98.00));
        assert_eq!(e.net, dec!(882.00));
        let (commission, net) = crate::pricing::calculate_host_payout(
            crate::pricing::host_gross(dec!(900.00), dec!(80.00), rust_decimal::Decimal::ONE),
            dec!(0.1000),
            rust_decimal::Decimal::ZERO,
        );
        assert_eq!((e.commission, e.net), (commission, net));
    }

    #[test]
    fn host_earnings_at_zero_commission_are_the_gross() {
        let e = estimate(dec!(333.33), dec!(0));
        assert_eq!(e.commission, dec!(0));
        assert_eq!(e.net, dec!(413.33));
    }

    #[test]
    fn host_earnings_never_go_negative() {
        let e = estimate(dec!(20.00), dec!(1.5));
        assert_eq!(e.net, dec!(0));
    }

    #[test]
    fn host_earnings_amounts_serialize_as_strings() {
        let json = serde_json::to_value(estimate(dec!(900.00), dec!(0.1000))).expect("json");
        assert_eq!(json["net"], "882.00");
        assert_eq!(json["commission_pct"], "0.1000");
    }
}
