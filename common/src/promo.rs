//! Promo code rules: normalizing a code, the checks a redemption must pass, and
//! the discount it gives. Everything here is pure, so quotes, holds and
//! reservation changes all follow the same rules.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Shortest allowed code.
pub const MIN_CODE_LEN: usize = 4;
/// Longest allowed code.
pub const MAX_CODE_LEN: usize = 32;

/// How a code's discount is expressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "promo_discount_type", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum PromoDiscountType {
    /// A percentage of the eligible amount.
    Percentage,
    /// A fixed amount in the code's stated currency.
    Fixed,
}

/// What a code's discount applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "promo_scope", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum PromoScope {
    /// The stay, including extra-guest fees, after weekly or monthly discounts.
    Stay,
    /// The stay and the cleaning fee.
    StayAndCleaning,
}

/// A code's terms, snapshotted on each booking at hold time so later changes
/// can't reprice it. Terms never change after a code is created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromoTerms {
    /// The normalized code.
    pub code: String,
    /// Whether `discount_value` is a percentage or an amount.
    pub discount_type: PromoDiscountType,
    /// The percentage, or the amount in `currency`.
    pub discount_value: Decimal,
    /// What the discount applies to.
    pub scope: PromoScope,
    /// The stated currency of a fixed discount or minimum value.
    pub currency: Option<String>,
    /// Converts `currency` to the payment currency. `1` when there is no stated
    /// currency or it is the payment currency.
    pub exchange_rate: Decimal,
    /// Fewest nights the stay must have.
    pub min_nights: Option<i32>,
    /// Least the eligible amount must be, in `currency`.
    pub min_booking_value: Option<Decimal>,
    /// First eligible night.
    pub stay_from: Option<NaiveDate>,
    /// Last eligible night.
    pub stay_to: Option<NaiveDate>,
}

/// The dates of a stay, with check-out exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromoStay {
    /// First night.
    pub check_in: NaiveDate,
    /// Departure date. The last night is the day before.
    pub check_out: NaiveDate,
}

/// The state of a code that can change after it's created, or with time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromoStatus {
    /// Whether the code has been deactivated.
    pub deactivated: bool,
    /// When the code starts being valid.
    pub valid_from: DateTime<Utc>,
    /// When the code stops being valid.
    pub valid_to: DateTime<Utc>,
}

/// A code's limits and its current usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PromoUsage {
    /// Most uses allowed across all guests.
    pub total_limit: Option<i32>,
    /// Most uses allowed per guest.
    pub guest_limit: Option<i32>,
    /// Live uses across all guests.
    pub total_used: i64,
    /// Live uses by this guest.
    pub guest_used: i64,
}

/// Why a code can't be used. Refusals are returned in a fixed order, so the
/// reason a guest sees is predictable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum PromoRefusal {
    /// The code is malformed or doesn't exist.
    #[error("That promo code isn't valid.")]
    NotFound,
    /// The code has been deactivated.
    #[error("That promo code is no longer active.")]
    Deactivated,
    /// The code's validity period hasn't started.
    #[error("That promo code isn't valid yet.")]
    NotYetValid,
    /// The code's validity period has ended.
    #[error("That promo code has expired.")]
    Expired,
    /// The code doesn't cover this listing.
    #[error("That promo code can't be used for this property.")]
    ListingNotEligible,
    /// The code's currency can't be converted to the payment currency.
    #[error("That promo code can't be used in this currency right now.")]
    FxRateUnavailable,
    /// The stay is shorter than the code requires.
    #[error("This stay is too short for that promo code.")]
    MinNightsNotMet,
    /// A night of the stay falls outside the code's stay window.
    #[error("That promo code can't be used for these dates.")]
    StayDatesNotEligible,
    /// The eligible amount is below the code's minimum.
    #[error("This booking doesn't reach the minimum value for that promo code.")]
    MinValueNotMet,
    /// The code has reached its total usage limit.
    #[error("That promo code has reached its usage limit.")]
    UsageLimitReached,
    /// This guest has reached the code's per-guest limit.
    #[error("You've already used that promo code the maximum number of times.")]
    GuestLimitReached,
}

impl PromoRefusal {
    /// Every refusal, in check order.
    pub const ALL: [Self; 11] = [
        Self::NotFound,
        Self::Deactivated,
        Self::NotYetValid,
        Self::Expired,
        Self::ListingNotEligible,
        Self::FxRateUnavailable,
        Self::MinNightsNotMet,
        Self::StayDatesNotEligible,
        Self::MinValueNotMet,
        Self::UsageLimitReached,
        Self::GuestLimitReached,
    ];

    /// The stable API error code.
    pub fn code(self) -> &'static str {
        match self {
            Self::NotFound => "PROMO_NOT_FOUND",
            Self::Deactivated => "PROMO_DEACTIVATED",
            Self::NotYetValid => "PROMO_NOT_YET_VALID",
            Self::Expired => "PROMO_EXPIRED",
            Self::ListingNotEligible => "PROMO_LISTING_NOT_ELIGIBLE",
            Self::FxRateUnavailable => "FX_RATE_UNAVAILABLE",
            Self::MinNightsNotMet => "PROMO_MIN_NIGHTS_NOT_MET",
            Self::StayDatesNotEligible => "PROMO_STAY_DATES_NOT_ELIGIBLE",
            Self::MinValueNotMet => "PROMO_MIN_VALUE_NOT_MET",
            Self::UsageLimitReached => "PROMO_USAGE_LIMIT_REACHED",
            Self::GuestLimitReached => "PROMO_GUEST_LIMIT_REACHED",
        }
    }

    /// The message shown to the guest.
    pub fn message(self) -> String {
        self.to_string()
    }

    /// The refusal for an API error code, if it is one.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.code() == code)
    }
}

/// Normalizes a code as typed: blank means no code, and anything malformed is
/// refused, so a mistyped code can never turn into a full-price hold.
pub fn normalize_code(raw: &str) -> Result<Option<String>, PromoRefusal> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let valid_length = (MIN_CODE_LEN..=MAX_CODE_LEN).contains(&trimmed.len());
    let valid_chars = trimmed
        .chars()
        .enumerate()
        .all(|(i, c)| c.is_ascii_alphanumeric() || (i > 0 && c == '-'));
    if valid_length && valid_chars {
        Ok(Some(trimmed.to_ascii_uppercase()))
    } else {
        Err(PromoRefusal::NotFound)
    }
}

/// Checks that don't depend on the stay: deactivation, then the validity period.
pub fn check_status(status: PromoStatus, now: DateTime<Utc>) -> Result<(), PromoRefusal> {
    if status.deactivated {
        Err(PromoRefusal::Deactivated)
    } else if now < status.valid_from {
        Err(PromoRefusal::NotYetValid)
    } else if now >= status.valid_to {
        Err(PromoRefusal::Expired)
    } else {
        Ok(())
    }
}

/// Checks against the stay: minimum nights, then the stay window, then the
/// minimum value. `eligible_amount` is before the promo and tax, in the payment
/// currency: see [`PromoTerms::eligible_amount`].
pub fn check_stay_rules(
    terms: &PromoTerms,
    stay: PromoStay,
    eligible_amount: Decimal,
) -> Result<(), PromoRefusal> {
    let nights = (stay.check_out - stay.check_in).num_days();
    if terms.min_nights.is_some_and(|min| nights < i64::from(min)) {
        return Err(PromoRefusal::MinNightsNotMet);
    }
    let last_night = stay.check_out - chrono::Duration::days(1);
    let starts_too_early = terms.stay_from.is_some_and(|from| stay.check_in < from);
    let ends_too_late = terms.stay_to.is_some_and(|to| last_night > to);
    if starts_too_early || ends_too_late {
        return Err(PromoRefusal::StayDatesNotEligible);
    }
    let minimum = terms
        .min_booking_value
        .map(|min| (min * terms.exchange_rate).round_dp(2));
    if minimum.is_some_and(|min| eligible_amount < min) {
        return Err(PromoRefusal::MinValueNotMet);
    }
    Ok(())
}

/// Checks against usage: the total limit, then the per-guest limit.
pub fn check_usage(usage: PromoUsage) -> Result<(), PromoRefusal> {
    let reached = |limit: Option<i32>, used: i64| limit.is_some_and(|l| used >= i64::from(l));
    if reached(usage.total_limit, usage.total_used) {
        Err(PromoRefusal::UsageLimitReached)
    } else if reached(usage.guest_limit, usage.guest_used) {
        Err(PromoRefusal::GuestLimitReached)
    } else {
        Ok(())
    }
}

/// Largest amount a code's `NUMERIC(12,2)` columns can hold.
const MAX_CODE_AMOUNT: Decimal = Decimal::from_parts(3_567_587_327, 232, 0, false, 2);

/// A code to create.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreatePromoCodeRequest {
    /// The code as typed.
    pub code: String,
    /// Whether `discount_value` is a percentage or an amount.
    pub discount_type: PromoDiscountType,
    /// The percentage, or the amount in `currency`.
    pub discount_value: Decimal,
    /// The stated currency of a fixed discount or minimum value.
    #[serde(default)]
    pub currency: Option<String>,
    /// What the discount applies to.
    pub scope: PromoScope,
    /// When the code starts being valid.
    pub valid_from: DateTime<Utc>,
    /// When the code stops being valid.
    pub valid_to: DateTime<Utc>,
    /// First eligible night.
    #[serde(default)]
    pub stay_from: Option<NaiveDate>,
    /// Last eligible night.
    #[serde(default)]
    pub stay_to: Option<NaiveDate>,
    /// Fewest nights the stay must have.
    #[serde(default)]
    pub min_nights: Option<i32>,
    /// Least the eligible amount must be, in `currency`.
    #[serde(default)]
    pub min_booking_value: Option<Decimal>,
    /// Most uses allowed across all guests.
    #[serde(default)]
    pub total_usage_limit: Option<i32>,
    /// Most uses allowed per guest.
    #[serde(default)]
    pub per_guest_limit: Option<i32>,
    /// Whether the code covers every listing in its owner's reach.
    pub applies_to_all: bool,
    /// The selected listings, when it doesn't.
    #[serde(default)]
    pub listing_ids: Vec<uuid::Uuid>,
}

/// A code with its usage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PromoCodeResponse {
    /// Unique identifier.
    pub id: uuid::Uuid,
    /// The normalized code.
    pub code: String,
    /// Whether `discount_value` is a percentage or an amount.
    pub discount_type: PromoDiscountType,
    /// The percentage, or the amount in `currency`.
    pub discount_value: Decimal,
    /// The stated currency of a fixed discount or minimum value.
    pub currency: Option<String>,
    /// What the discount applies to.
    pub scope: PromoScope,
    /// When the code starts being valid.
    pub valid_from: DateTime<Utc>,
    /// When the code stops being valid.
    pub valid_to: DateTime<Utc>,
    /// First eligible night.
    pub stay_from: Option<NaiveDate>,
    /// Last eligible night.
    pub stay_to: Option<NaiveDate>,
    /// Fewest nights the stay must have.
    pub min_nights: Option<i32>,
    /// Least the eligible amount must be, in `currency`.
    pub min_booking_value: Option<Decimal>,
    /// Most uses allowed across all guests.
    pub total_usage_limit: Option<i32>,
    /// Most uses allowed per guest.
    pub per_guest_limit: Option<i32>,
    /// Whether the code covers every listing in its owner's reach.
    pub applies_to_all: bool,
    /// The selected listings.
    pub listing_ids: Vec<uuid::Uuid>,
    /// The host who funds and manages the code; `None` for an admin-created code.
    pub owner_host_id: Option<uuid::Uuid>,
    /// When the code was deactivated.
    pub deactivated_at: Option<DateTime<Utc>>,
    /// Live holds using the code.
    pub active_holds: i64,
    /// Confirmed or completed bookings using the code.
    pub confirmed_uses: i64,
    /// Uses left, when the code has a total limit.
    pub remaining_uses: Option<i64>,
    /// When the code was created.
    pub created_at: DateTime<Utc>,
}

/// One page of codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PromoCodesResponse {
    /// The codes on this page.
    pub codes: Vec<PromoCodeResponse>,
    /// Codes across all pages.
    pub total: i64,
}

fn is_valid_amount(amount: Decimal) -> bool {
    amount > Decimal::ZERO && amount.normalize().scale() <= 2 && amount <= MAX_CODE_AMOUNT
}

/// Checks a code's terms and returns the normalized code, or what's wrong with
/// them. More than 2 decimal places is rejected, not rounded.
pub fn validate_new_code(request: &CreatePromoCodeRequest) -> Result<String, String> {
    let code = normalize_code(&request.code)
        .ok()
        .flatten()
        .ok_or_else(|| {
            format!(
                "Codes are {MIN_CODE_LEN}-{MAX_CODE_LEN} letters, digits or hyphens, and can't start with a hyphen."
            )
        })?;
    if !is_valid_amount(request.discount_value) {
        return Err("The discount must be above 0, with at most 2 decimal places.".to_string());
    }
    if request.discount_type == PromoDiscountType::Percentage
        && request.discount_value > Decimal::ONE_HUNDRED
    {
        return Err("A percentage discount can be at most 100.".to_string());
    }
    let needs_currency =
        request.discount_type == PromoDiscountType::Fixed || request.min_booking_value.is_some();
    match (needs_currency, &request.currency) {
        (true, None) => {
            return Err("A fixed discount or minimum value needs a currency.".to_string());
        }
        (false, Some(_)) => {
            return Err("Only a fixed discount or minimum value has a currency.".to_string());
        }
        (true, Some(currency)) => {
            crate::currency::CurrencyCode::try_from(currency.as_str())
                .map_err(|_| "Currencies are three capital letters, e.g. USD.".to_string())?;
        }
        (false, None) => {}
    }
    if request.valid_to <= request.valid_from {
        return Err("The validity period must end after it starts.".to_string());
    }
    if let (Some(from), Some(to)) = (request.stay_from, request.stay_to)
        && to < from
    {
        return Err("The stay window must end on or after its first night.".to_string());
    }
    if request.min_nights.is_some_and(|n| n < 1) {
        return Err("Minimum nights must be at least 1.".to_string());
    }
    if request
        .min_booking_value
        .is_some_and(|value| !is_valid_amount(value))
    {
        return Err(
            "The minimum value must be above 0, with at most 2 decimal places.".to_string(),
        );
    }
    if request.total_usage_limit.is_some_and(|n| n < 1)
        || request.per_guest_limit.is_some_and(|n| n < 1)
    {
        return Err("Usage limits must be at least 1.".to_string());
    }
    if request.applies_to_all && !request.listing_ids.is_empty() {
        return Err("Choose all listings or selected listings, not both.".to_string());
    }
    if !request.applies_to_all && request.listing_ids.is_empty() {
        return Err("Select at least one listing.".to_string());
    }
    Ok(code)
}

/// What happens to a booking's promo when its terms change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PromoOutcome {
    /// The booking has no promo.
    None,
    /// The discount stays, recalculated for the new terms.
    Kept {
        /// The code.
        code: String,
    },
    /// The new stay no longer qualifies, so the discount is removed.
    Lost {
        /// The code.
        code: String,
        /// The stable refusal code.
        reason_code: String,
        /// Why, in words the guest can read.
        reason: String,
    },
}

/// Re-checks a booking's snapshotted terms against new stay terms. Only the stay
/// rules apply: validity dates, deactivation and usage limits were settled when
/// the code was redeemed. `stay_net` and `cleaning_fee` are before the promo.
pub fn reevaluate(
    terms: Option<&PromoTerms>,
    stay: PromoStay,
    stay_net: Decimal,
    cleaning_fee: Decimal,
) -> PromoOutcome {
    let Some(terms) = terms else {
        return PromoOutcome::None;
    };
    match check_stay_rules(terms, stay, terms.eligible_amount(stay_net, cleaning_fee)) {
        Ok(()) => PromoOutcome::Kept {
            code: terms.code.clone(),
        },
        Err(refusal) => PromoOutcome::Lost {
            code: terms.code.clone(),
            reason_code: refusal.code().to_string(),
            reason: refusal.message(),
        },
    }
}

impl PromoOutcome {
    /// Whether the discount stays.
    pub fn is_kept(&self) -> bool {
        matches!(self, Self::Kept { .. })
    }

    /// Why the discount is lost, if it is.
    pub fn lost_reason(&self) -> Option<String> {
        match self {
            Self::Lost { reason, .. } => Some(reason.clone()),
            Self::None | Self::Kept { .. } => None,
        }
    }
}

impl PromoTerms {
    /// The amount a minimum value is compared with: the net stay, plus the
    /// cleaning fee when the code covers it.
    pub fn eligible_amount(&self, stay_net: Decimal, cleaning_fee: Decimal) -> Decimal {
        match self.scope {
            PromoScope::Stay => stay_net,
            PromoScope::StayAndCleaning => stay_net + cleaning_fee,
        }
    }

    /// The discount on the stay and on the cleaning fee. Neither can exceed the
    /// amount it discounts. A fixed discount goes to the stay first, and only
    /// the remainder to the cleaning fee.
    pub fn discounts(&self, stay_net: Decimal, cleaning_fee: Decimal) -> (Decimal, Decimal) {
        let covers_cleaning = self.scope == PromoScope::StayAndCleaning;
        match self.discount_type {
            PromoDiscountType::Percentage => {
                let share = self.discount_value / Decimal::ONE_HUNDRED;
                let of = |amount: Decimal| (amount * share).round_dp(2).min(amount);
                let cleaning = if covers_cleaning {
                    of(cleaning_fee)
                } else {
                    Decimal::ZERO
                };
                (of(stay_net), cleaning)
            }
            PromoDiscountType::Fixed => {
                let amount = (self.discount_value * self.exchange_rate).round_dp(2);
                let stay = amount.min(stay_net);
                let cleaning = if covers_cleaning {
                    (amount - stay).min(cleaning_fee)
                } else {
                    Decimal::ZERO
                };
                (stay, cleaning)
            }
        }
    }
}

#[cfg(test)]
mod validation {
    use super::*;
    use chrono::TimeZone;
    use rust_decimal_macros::dec;

    fn at(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2030, 1, day, 0, 0, 0)
            .single()
            .expect("valid time")
    }

    fn valid() -> CreatePromoCodeRequest {
        CreatePromoCodeRequest {
            code: " summer-10 ".to_string(),
            discount_type: PromoDiscountType::Percentage,
            discount_value: dec!(10),
            currency: None,
            scope: PromoScope::Stay,
            valid_from: at(1),
            valid_to: at(2),
            stay_from: None,
            stay_to: None,
            min_nights: None,
            min_booking_value: None,
            total_usage_limit: None,
            per_guest_limit: None,
            applies_to_all: true,
            listing_ids: vec![],
        }
    }

    #[test]
    fn a_valid_code_is_normalized() {
        assert_eq!(validate_new_code(&valid()).as_deref(), Ok("SUMMER-10"));
    }

    #[test]
    fn valid_edge_cases_are_accepted() {
        for request in [
            CreatePromoCodeRequest {
                discount_value: dec!(100),
                ..valid()
            },
            CreatePromoCodeRequest {
                discount_value: dec!(33.33),
                ..valid()
            },
            CreatePromoCodeRequest {
                discount_type: PromoDiscountType::Fixed,
                discount_value: dec!(1000),
                currency: Some("USD".to_string()),
                ..valid()
            },
            CreatePromoCodeRequest {
                min_booking_value: Some(dec!(100)),
                currency: Some("USD".to_string()),
                stay_from: Some(at(5).date_naive()),
                stay_to: Some(at(5).date_naive()),
                min_nights: Some(1),
                total_usage_limit: Some(1),
                per_guest_limit: Some(1),
                ..valid()
            },
            CreatePromoCodeRequest {
                applies_to_all: false,
                listing_ids: vec![uuid::Uuid::nil()],
                ..valid()
            },
        ] {
            assert!(validate_new_code(&request).is_ok(), "{request:?}");
        }
    }

    #[test]
    fn every_rule_refuses_with_a_reason() {
        for (case, request) in [
            (
                "malformed code",
                CreatePromoCodeRequest {
                    code: "SUM_MER".to_string(),
                    ..valid()
                },
            ),
            (
                "blank code",
                CreatePromoCodeRequest {
                    code: "  ".to_string(),
                    ..valid()
                },
            ),
            (
                "leading hyphen",
                CreatePromoCodeRequest {
                    code: "-ABCD".to_string(),
                    ..valid()
                },
            ),
            (
                "zero",
                CreatePromoCodeRequest {
                    discount_value: Decimal::ZERO,
                    ..valid()
                },
            ),
            (
                "negative",
                CreatePromoCodeRequest {
                    discount_value: dec!(-1),
                    ..valid()
                },
            ),
            (
                "rounds to zero",
                CreatePromoCodeRequest {
                    discount_value: dec!(0.001),
                    ..valid()
                },
            ),
            (
                "too precise",
                CreatePromoCodeRequest {
                    discount_value: dec!(33.335),
                    ..valid()
                },
            ),
            (
                "over 100%",
                CreatePromoCodeRequest {
                    discount_value: dec!(100.01),
                    ..valid()
                },
            ),
            (
                "overflows the column",
                CreatePromoCodeRequest {
                    discount_type: PromoDiscountType::Fixed,
                    discount_value: dec!(100000000000),
                    currency: Some("USD".to_string()),
                    ..valid()
                },
            ),
            (
                "fixed with no currency",
                CreatePromoCodeRequest {
                    discount_type: PromoDiscountType::Fixed,
                    ..valid()
                },
            ),
            (
                "minimum with no currency",
                CreatePromoCodeRequest {
                    min_booking_value: Some(dec!(10)),
                    ..valid()
                },
            ),
            (
                "needless currency",
                CreatePromoCodeRequest {
                    currency: Some("USD".to_string()),
                    ..valid()
                },
            ),
            (
                "malformed currency",
                CreatePromoCodeRequest {
                    discount_type: PromoDiscountType::Fixed,
                    currency: Some("usd1".to_string()),
                    ..valid()
                },
            ),
            (
                "backwards period",
                CreatePromoCodeRequest {
                    valid_to: at(1),
                    ..valid()
                },
            ),
            (
                "backwards window",
                CreatePromoCodeRequest {
                    stay_from: Some(at(5).date_naive()),
                    stay_to: Some(at(4).date_naive()),
                    ..valid()
                },
            ),
            (
                "zero nights",
                CreatePromoCodeRequest {
                    min_nights: Some(0),
                    ..valid()
                },
            ),
            (
                "too precise minimum",
                CreatePromoCodeRequest {
                    min_booking_value: Some(dec!(1.001)),
                    currency: Some("USD".to_string()),
                    ..valid()
                },
            ),
            (
                "zero total limit",
                CreatePromoCodeRequest {
                    total_usage_limit: Some(0),
                    ..valid()
                },
            ),
            (
                "zero guest limit",
                CreatePromoCodeRequest {
                    per_guest_limit: Some(0),
                    ..valid()
                },
            ),
            (
                "no listings",
                CreatePromoCodeRequest {
                    applies_to_all: false,
                    ..valid()
                },
            ),
            (
                "all and selected",
                CreatePromoCodeRequest {
                    listing_ids: vec![uuid::Uuid::nil()],
                    ..valid()
                },
            ),
        ] {
            assert!(
                validate_new_code(&request).is_err(),
                "{case} must be refused"
            );
        }
    }

    #[test]
    fn the_amount_limit_matches_the_column() {
        assert_eq!(MAX_CODE_AMOUNT, dec!(9999999999.99));
    }
}

#[cfg(test)]
mod reevaluate_tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2030, 6, d).expect("valid date")
    }

    fn stay(check_in: u32, check_out: u32) -> PromoStay {
        PromoStay {
            check_in: day(check_in),
            check_out: day(check_out),
        }
    }

    fn terms() -> PromoTerms {
        PromoTerms {
            code: "SUMMER".to_string(),
            discount_type: PromoDiscountType::Percentage,
            discount_value: dec!(10),
            scope: PromoScope::Stay,
            currency: None,
            exchange_rate: Decimal::ONE,
            min_nights: None,
            min_booking_value: None,
            stay_from: None,
            stay_to: None,
        }
    }

    #[test]
    fn no_promo_stays_no_promo() {
        assert_eq!(
            reevaluate(None, stay(1, 3), dec!(100), dec!(10)),
            PromoOutcome::None
        );
    }

    #[test]
    fn a_percentage_is_recalculated_on_the_new_amounts() {
        let t = terms();
        assert!(reevaluate(Some(&t), stay(1, 9), dec!(800), dec!(0)).is_kept());
        assert_eq!(t.discounts(dec!(800), dec!(0)).0, dec!(80));
        assert_eq!(t.discounts(dec!(300), dec!(0)).0, dec!(30));
    }

    #[test]
    fn a_fixed_amount_stays_the_same_using_the_snapshotted_rate_subject_to_the_cap() {
        let t = PromoTerms {
            discount_type: PromoDiscountType::Fixed,
            discount_value: dec!(50),
            currency: Some("USD".to_string()),
            exchange_rate: dec!(160),
            ..terms()
        };
        assert!(reevaluate(Some(&t), stay(1, 9), dec!(80000), dec!(0)).is_kept());
        assert_eq!(t.discounts(dec!(80000), dec!(0)).0, dec!(8000));
        assert_eq!(t.discounts(dec!(5000), dec!(0)).0, dec!(5000));
    }

    #[test]
    fn falling_below_a_minimum_or_outside_the_window_loses_the_discount() {
        let short = PromoTerms {
            min_nights: Some(5),
            ..terms()
        };
        let outcome = reevaluate(Some(&short), stay(1, 3), dec!(400), dec!(0));
        assert!(!outcome.is_kept());
        assert!(matches!(
            &outcome,
            PromoOutcome::Lost { reason_code, .. } if reason_code == "PROMO_MIN_NIGHTS_NOT_MET"
        ));
        assert!(outcome.lost_reason().is_some());

        let cheap = PromoTerms {
            currency: Some("USD".to_string()),
            min_booking_value: Some(dec!(500)),
            ..terms()
        };
        assert!(matches!(
            reevaluate(Some(&cheap), stay(1, 3), dec!(400), dec!(0)),
            PromoOutcome::Lost { reason_code, .. } if reason_code == "PROMO_MIN_VALUE_NOT_MET"
        ));

        let window = PromoTerms {
            stay_to: Some(day(2)),
            ..terms()
        };
        assert!(matches!(
            reevaluate(Some(&window), stay(1, 5), dec!(400), dec!(0)),
            PromoOutcome::Lost { reason_code, .. } if reason_code == "PROMO_STAY_DATES_NOT_ELIGIBLE"
        ));
    }

    #[test]
    fn validity_deactivation_and_usage_are_not_consulted() {
        // The terms carry none of them, so a kept outcome can't depend on them.
        assert!(reevaluate(Some(&terms()), stay(1, 3), dec!(1), dec!(0)).is_kept());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use rust_decimal_macros::dec;

    fn day(m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2030, m, d).expect("valid date")
    }

    fn at(h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2030, 1, 1, h, 0, 0)
            .single()
            .expect("valid time")
    }

    fn terms() -> PromoTerms {
        PromoTerms {
            code: "SUMMER".to_string(),
            discount_type: PromoDiscountType::Percentage,
            discount_value: dec!(10),
            scope: PromoScope::Stay,
            currency: None,
            exchange_rate: Decimal::ONE,
            min_nights: None,
            min_booking_value: None,
            stay_from: None,
            stay_to: None,
        }
    }

    fn stay(check_in: NaiveDate, check_out: NaiveDate) -> PromoStay {
        PromoStay {
            check_in,
            check_out,
        }
    }

    #[test]
    fn normalize_trims_uppercases_and_treats_blank_as_no_code() {
        assert_eq!(normalize_code(" Summer "), Ok(Some("SUMMER".to_string())));
        assert_eq!(normalize_code("a1-b2"), Ok(Some("A1-B2".to_string())));
        assert_eq!(normalize_code(""), Ok(None));
        assert_eq!(normalize_code("   "), Ok(None));
    }

    #[test]
    fn normalize_refuses_anything_malformed() {
        let too_long = "A".repeat(MAX_CODE_LEN + 1);
        for raw in [
            "SUM_MER",
            "abc",
            "-ABC",
            "SUM MER",
            "SÜMMER",
            too_long.as_str(),
        ] {
            assert_eq!(normalize_code(raw), Err(PromoRefusal::NotFound), "{raw}");
        }
        assert!(normalize_code(&"A".repeat(MAX_CODE_LEN)).is_ok());
        assert!(normalize_code("ABCD").is_ok());
    }

    #[test]
    fn status_refuses_in_order_and_valid_to_is_exclusive() {
        let status = PromoStatus {
            deactivated: false,
            valid_from: at(10),
            valid_to: at(12),
        };
        assert_eq!(check_status(status, at(9)), Err(PromoRefusal::NotYetValid));
        assert_eq!(check_status(status, at(10)), Ok(()));
        assert_eq!(check_status(status, at(11)), Ok(()));
        assert_eq!(check_status(status, at(12)), Err(PromoRefusal::Expired));
        let deactivated = PromoStatus {
            deactivated: true,
            ..status
        };
        assert_eq!(
            check_status(deactivated, at(9)),
            Err(PromoRefusal::Deactivated)
        );
        assert_eq!(
            check_status(deactivated, at(13)),
            Err(PromoRefusal::Deactivated)
        );
    }

    #[test]
    fn stay_rules_check_minimum_nights_at_the_boundary() {
        let t = PromoTerms {
            min_nights: Some(3),
            ..terms()
        };
        assert_eq!(
            check_stay_rules(&t, stay(day(1, 1), day(1, 3)), dec!(100)),
            Err(PromoRefusal::MinNightsNotMet)
        );
        assert_eq!(
            check_stay_rules(&t, stay(day(1, 1), day(1, 4)), dec!(100)),
            Ok(())
        );
    }

    #[test]
    fn stay_rules_treat_stay_to_as_the_last_eligible_night() {
        let t = PromoTerms {
            stay_from: Some(day(1, 10)),
            stay_to: Some(day(1, 20)),
            ..terms()
        };
        assert_eq!(
            check_stay_rules(&t, stay(day(1, 10), day(1, 21)), dec!(1)),
            Ok(())
        );
        assert_eq!(
            check_stay_rules(&t, stay(day(1, 9), day(1, 21)), dec!(1)),
            Err(PromoRefusal::StayDatesNotEligible)
        );
        assert_eq!(
            check_stay_rules(&t, stay(day(1, 10), day(1, 22)), dec!(1)),
            Err(PromoRefusal::StayDatesNotEligible)
        );
    }

    #[test]
    fn stay_rules_convert_the_minimum_value_and_accept_it_exactly() {
        let t = PromoTerms {
            currency: Some("USD".to_string()),
            exchange_rate: dec!(160),
            min_booking_value: Some(dec!(100)),
            ..terms()
        };
        let s = stay(day(1, 1), day(1, 3));
        assert_eq!(check_stay_rules(&t, s, dec!(16000)), Ok(()));
        assert_eq!(
            check_stay_rules(&t, s, dec!(15999.99)),
            Err(PromoRefusal::MinValueNotMet)
        );
    }

    #[test]
    fn stay_rules_refuse_in_order() {
        let t = PromoTerms {
            min_nights: Some(5),
            stay_from: Some(day(2, 1)),
            min_booking_value: Some(dec!(100)),
            ..terms()
        };
        let s = stay(day(1, 1), day(1, 3));
        assert_eq!(
            check_stay_rules(&t, s, dec!(1)),
            Err(PromoRefusal::MinNightsNotMet)
        );
        let t = PromoTerms {
            min_nights: None,
            ..t
        };
        assert_eq!(
            check_stay_rules(&t, s, dec!(1)),
            Err(PromoRefusal::StayDatesNotEligible)
        );
    }

    #[test]
    fn usage_refuses_at_the_limit_with_the_total_first() {
        let usage = PromoUsage {
            total_limit: Some(2),
            guest_limit: Some(1),
            total_used: 1,
            guest_used: 0,
        };
        assert_eq!(check_usage(usage), Ok(()));
        assert_eq!(
            check_usage(PromoUsage {
                guest_used: 1,
                ..usage
            }),
            Err(PromoRefusal::GuestLimitReached)
        );
        assert_eq!(
            check_usage(PromoUsage {
                total_used: 2,
                guest_used: 1,
                ..usage
            }),
            Err(PromoRefusal::UsageLimitReached)
        );
        assert_eq!(check_usage(PromoUsage::default()), Ok(()));
    }

    #[test]
    fn terms_round_trip_through_json_without_losing_precision() {
        let t = PromoTerms {
            discount_type: PromoDiscountType::Fixed,
            discount_value: dec!(12.34),
            scope: PromoScope::StayAndCleaning,
            currency: Some("USD".to_string()),
            exchange_rate: dec!(160.123456),
            min_booking_value: Some(dec!(99.99)),
            stay_to: Some(day(3, 1)),
            ..terms()
        };
        let json = serde_json::to_string(&t).expect("serialize terms");
        assert_eq!(
            serde_json::from_str::<PromoTerms>(&json).expect("parse terms"),
            t
        );
    }

    #[test]
    fn refusal_codes_round_trip_and_unknown_codes_do_not_match() {
        for code in ["PROMO_NOT_FOUND", "PROMO_EXPIRED", "FX_RATE_UNAVAILABLE"] {
            assert_eq!(
                PromoRefusal::from_code(code).map(PromoRefusal::code),
                Some(code)
            );
        }
        assert_eq!(PromoRefusal::from_code("DATE_UNAVAILABLE"), None);
    }

    #[test]
    fn discounts_follow_the_worked_example_for_each_type_and_scope() {
        let both = PromoTerms {
            scope: PromoScope::StayAndCleaning,
            ..terms()
        };
        assert_eq!(both.discounts(dec!(900), dec!(100)), (dec!(90), dec!(10)));
        assert_eq!(terms().discounts(dec!(900), dec!(100)), (dec!(90), dec!(0)));
    }

    #[test]
    fn fixed_discounts_convert_cap_and_go_to_the_stay_first() {
        let fixed = PromoTerms {
            discount_type: PromoDiscountType::Fixed,
            discount_value: dec!(50),
            exchange_rate: dec!(2),
            ..terms()
        };
        assert_eq!(fixed.discounts(dec!(900), dec!(100)), (dec!(100), dec!(0)));
        assert_eq!(fixed.discounts(dec!(60), dec!(100)), (dec!(60), dec!(0)));
        let both = PromoTerms {
            scope: PromoScope::StayAndCleaning,
            ..fixed
        };
        assert_eq!(both.discounts(dec!(60), dec!(100)), (dec!(60), dec!(40)));
        assert_eq!(both.discounts(dec!(60), dec!(10)), (dec!(60), dec!(10)));
    }

    #[test]
    fn a_full_percentage_discount_takes_exactly_the_eligible_amounts() {
        let all = PromoTerms {
            discount_value: dec!(100),
            scope: PromoScope::StayAndCleaning,
            ..terms()
        };
        assert_eq!(
            all.discounts(dec!(33.33), dec!(0.01)),
            (dec!(33.33), dec!(0.01))
        );
    }
}
