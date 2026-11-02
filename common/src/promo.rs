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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
        [
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
        ]
        .into_iter()
        .find(|r| r.code() == code)
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
