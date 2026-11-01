//! Currency exchange rates as stored in `currency_exchange_rates`.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The latest rate for one currency pair: 1 `base_currency` buys `rate`
/// `target_currency`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ExchangeRate {
    /// ISO 4217 code being converted from.
    pub base_currency: String,
    /// ISO 4217 code being converted to.
    pub target_currency: String,
    /// Units of `target_currency` per unit of `base_currency`.
    #[schema(value_type = String)]
    pub rate: Decimal,
    /// When this rate took effect.
    pub effective_at: DateTime<Utc>,
}

/// The latest rate for every stored currency pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ExchangeRatesResponse {
    /// Sorted by base then target currency.
    pub rates: Vec<ExchangeRate>,
}

use crate::currency::CurrencyCode;
use chrono::Duration;
use rust_decimal::prelude::Zero;
use std::str::FromStr;
use uuid::Uuid;

/// A new rate needs explicit confirmation when it moves more than this many
/// percent away from the pair's baseline rate (the typo guard).
pub const RATE_GUARD_THRESHOLD_PCT: Decimal = Decimal::from_parts(5, 0, 0, false, 0);

/// Most decimal places a stored rate may have (`NUMERIC(15,6)`).
pub const MAX_RATE_DECIMAL_PLACES: u32 = 6;

/// Largest storable rate (`NUMERIC(15,6)`): 999,999,999.999999.
pub const MAX_RATE: Decimal = Decimal::from_parts(2_764_472_319, 232_830, 0, false, 6);

/// How far ahead a rate may be scheduled.
pub const EFFECTIVE_AT_HORIZON_DAYS: i64 = 366;

/// Why a submitted rate string is not acceptable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RateError {
    /// Not a plain decimal (no sign, exponent, separators or whitespace).
    #[error("rate must be a plain decimal number such as 160.25")]
    Syntax,
    /// Zero or negative.
    #[error("rate must be greater than zero")]
    NotPositive,
    /// More than [`MAX_RATE_DECIMAL_PLACES`] significant decimal places.
    #[error("rate may have at most 6 decimal places")]
    TooPrecise,
    /// Larger than [`MAX_RATE`].
    #[error("rate is too large")]
    TooLarge,
}

/// Parses an admin-entered rate strictly: digits with an optional single
/// decimal point, no sign, exponent, separators or surrounding whitespace.
/// Trailing zeros do not count towards the decimal-place limit. Values are
/// rejected, never rounded.
pub fn parse_rate(input: &str) -> Result<Decimal, RateError> {
    let well_formed = !input.is_empty()
        && !input.starts_with('.')
        && !input.ends_with('.')
        && input.chars().filter(|c| *c == '.').count() <= 1
        && input.chars().all(|c| c.is_ascii_digit() || c == '.');
    well_formed
        .then_some(input)
        .ok_or(RateError::Syntax)
        .and_then(|s| Decimal::from_str(s).map_err(|_| RateError::Syntax))
        .map(|d| d.normalize())
        .and_then(validate_rate)
}

/// Checks an already-parsed rate against the storage and business bounds.
pub fn validate_rate(rate: Decimal) -> Result<Decimal, RateError> {
    let rate = rate.normalize();
    if rate <= Decimal::zero() {
        Err(RateError::NotPositive)
    } else if rate.scale() > MAX_RATE_DECIMAL_PLACES {
        Err(RateError::TooPrecise)
    } else if rate > MAX_RATE {
        Err(RateError::TooLarge)
    } else {
        Ok(rate)
    }
}

/// Whether moving from `baseline` to `proposed` exceeds the typo guard.
/// Computed without division so the boundary is exact: exactly 5% passes.
pub fn requires_confirmation(baseline: Decimal, proposed: Decimal) -> bool {
    (proposed - baseline).abs() * Decimal::ONE_HUNDRED > RATE_GUARD_THRESHOLD_PCT * baseline
}

/// Absolute deviation of `proposed` from `baseline` in percent, rounded to two
/// places, for display only. `None` when the baseline is zero.
pub fn deviation_pct(baseline: Decimal, proposed: Decimal) -> Option<Decimal> {
    ((proposed - baseline).abs() * Decimal::ONE_HUNDRED)
        .checked_div(baseline)
        .map(|pct| pct.round_dp(2))
}

/// Why a requested effective time is not acceptable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EffectiveAtError {
    /// Earlier than now minus the grace period (backdating is not allowed).
    #[error("effective time is in the past")]
    InPast,
    /// Later than [`EFFECTIVE_AT_HORIZON_DAYS`] days from now.
    #[error("effective time is too far in the future")]
    TooFarAhead,
}

/// Resolves the stored effective time of a new rate.
///
/// * `None`, or a time within `grace` before `now` up to `now` → `now`.
/// * A time after `now` (up to `horizon` ahead) → kept as scheduled.
/// * Earlier than `now - grace` → [`EffectiveAtError::InPast`].
/// * Later than `now + horizon` → [`EffectiveAtError::TooFarAhead`].
pub fn normalize_effective_at(
    requested: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    grace: Duration,
    horizon: Duration,
) -> Result<DateTime<Utc>, EffectiveAtError> {
    match requested {
        None => Ok(now),
        Some(at) if at < now - grace => Err(EffectiveAtError::InPast),
        Some(at) if at <= now => Ok(now),
        Some(at) if at > now + horizon => Err(EffectiveAtError::TooFarAhead),
        Some(at) => Ok(at),
    }
}

/// Admin request to publish a rate version (creating the pair when new).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PublishRateRequest {
    /// Currency converted from.
    pub base_currency: CurrencyCode,
    /// Currency converted to.
    pub target_currency: CurrencyCode,
    /// Units of target per unit of base, as a plain decimal string.
    pub rate: String,
    /// When the rate takes effect; omitted means now.
    #[serde(default)]
    pub effective_at: Option<DateTime<Utc>>,
    /// Confirms a change beyond the typo guard.
    #[serde(default)]
    pub confirm_large_change: bool,
}

/// Where a rate version sits relative to now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RateState {
    /// Superseded by a later effective version.
    Past,
    /// The version in effect now.
    Current,
    /// Takes effect in the future (can still be deleted).
    Scheduled,
}

/// Classifies versions given in resolution order (`effective_at DESC,
/// updated_at DESC`): the first one effective at `now` is current, later ones
/// are past, and those not yet effective are scheduled.
pub fn rate_states(effective_ats: &[DateTime<Utc>], now: DateTime<Utc>) -> Vec<RateState> {
    let current = effective_ats.iter().position(|at| *at <= now);
    effective_ats
        .iter()
        .enumerate()
        .map(|(i, at)| match current {
            _ if *at > now => RateState::Scheduled,
            Some(c) if c == i => RateState::Current,
            _ => RateState::Past,
        })
        .collect()
}

/// One stored rate version of a pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RateVersion {
    /// Row id.
    pub id: Uuid,
    /// Units of target per unit of base.
    #[schema(value_type = String)]
    pub rate: Decimal,
    /// When this version takes effect.
    pub effective_at: DateTime<Utc>,
    /// Admin who published it (absent for seeded rows).
    pub created_by: Option<Uuid>,
    /// Past, current or scheduled.
    pub state: RateState,
}

/// A pair's full version history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PairHistoryResponse {
    /// Currency converted from.
    pub base_currency: CurrencyCode,
    /// Currency converted to.
    pub target_currency: CurrencyCode,
    /// When the pair was retired, if it is.
    pub retired_at: Option<DateTime<Utc>>,
    /// Versions, newest effective first.
    pub versions: Vec<RateVersion>,
}

/// A listing base currency that cannot be quoted in an active currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RateCoverageGap {
    /// Base currency of at least one live listing.
    pub listing_base_currency: CurrencyCode,
    /// Active currency with no resolvable rate from that base.
    pub target_currency: CurrencyCode,
}

#[cfg(test)]
mod rate_rules_tests {
    use super::*;
    use chrono::TimeZone;
    use rust_decimal_macros::dec;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 1, 12, 0, 0)
            .single()
            .expect("valid fixed time")
    }

    fn grace() -> Duration {
        Duration::seconds(60)
    }

    fn horizon() -> Duration {
        Duration::days(EFFECTIVE_AT_HORIZON_DAYS)
    }

    #[test]
    fn max_rate_constant_matches_numeric_15_6() {
        assert_eq!(MAX_RATE, dec!(999999999.999999));
        assert_eq!(RATE_GUARD_THRESHOLD_PCT, dec!(5));
    }

    #[test]
    fn exactly_five_percent_needs_no_confirmation() {
        assert!(!requires_confirmation(dec!(100), dec!(105)));
        assert!(!requires_confirmation(dec!(100), dec!(95)));
    }

    #[test]
    fn just_over_five_percent_needs_confirmation_both_directions() {
        assert!(requires_confirmation(dec!(100), dec!(105.000001)));
        assert!(requires_confirmation(dec!(100), dec!(94.999999)));
    }

    #[test]
    fn deviation_is_for_display_and_safe_on_zero() {
        assert_eq!(deviation_pct(dec!(160), dec!(176)), Some(dec!(10.00)));
        assert_eq!(deviation_pct(dec!(3), dec!(4)), Some(dec!(33.33)));
        assert_eq!(deviation_pct(Decimal::ZERO, dec!(1)), None);
    }

    #[test]
    fn omitted_effective_at_is_now() {
        assert_eq!(
            normalize_effective_at(None, now(), grace(), horizon()),
            Ok(now())
        );
    }

    #[test]
    fn within_grace_snaps_to_now() {
        let at = now() - Duration::seconds(59);
        assert_eq!(
            normalize_effective_at(Some(at), now(), grace(), horizon()),
            Ok(now())
        );
        let exact = now() - grace();
        assert_eq!(
            normalize_effective_at(Some(exact), now(), grace(), horizon()),
            Ok(now())
        );
    }

    #[test]
    fn beyond_grace_is_in_past() {
        let at = now() - Duration::seconds(61);
        assert_eq!(
            normalize_effective_at(Some(at), now(), grace(), horizon()),
            Err(EffectiveAtError::InPast)
        );
    }

    #[test]
    fn future_is_kept_until_horizon() {
        let soon = now() + Duration::hours(1);
        assert_eq!(
            normalize_effective_at(Some(soon), now(), grace(), horizon()),
            Ok(soon)
        );
        let edge = now() + horizon();
        assert_eq!(
            normalize_effective_at(Some(edge), now(), grace(), horizon()),
            Ok(edge)
        );
        let beyond = now() + horizon() + Duration::days(1);
        assert_eq!(
            normalize_effective_at(Some(beyond), now(), grace(), horizon()),
            Err(EffectiveAtError::TooFarAhead)
        );
    }

    #[test]
    fn parses_plain_decimals() {
        assert_eq!(parse_rate("160.5"), Ok(dec!(160.5)));
        assert_eq!(parse_rate("0.000001"), Ok(dec!(0.000001)));
        assert_eq!(parse_rate("160"), Ok(dec!(160)));
    }

    #[test]
    fn trailing_zeros_do_not_count_as_precision() {
        assert_eq!(parse_rate("160.5000000"), Ok(dec!(160.5)));
    }

    #[test]
    fn rejects_non_positive() {
        assert_eq!(parse_rate("0"), Err(RateError::NotPositive));
        assert_eq!(parse_rate("0.000000"), Err(RateError::NotPositive));
        assert_eq!(parse_rate("-1"), Err(RateError::Syntax));
    }

    #[test]
    fn rejects_loose_syntax() {
        for bad in [
            "1e3", "+160", " 160 ", "160,5", "", ".5", "5.", "1.2.3", "abc",
        ] {
            assert_eq!(parse_rate(bad), Err(RateError::Syntax), "{bad:?}");
        }
    }

    #[test]
    fn rejects_seven_significant_decimals_and_too_large() {
        assert_eq!(parse_rate("0.0000001"), Err(RateError::TooPrecise));
        assert_eq!(parse_rate("1.1234567"), Err(RateError::TooPrecise));
        assert_eq!(parse_rate("1000000000"), Err(RateError::TooLarge));
        assert_eq!(parse_rate("999999999.999999"), Ok(MAX_RATE));
    }

    #[test]
    fn classifies_rate_states_in_resolution_order() {
        let at = |h: i64| now() + Duration::hours(h);
        let states = rate_states(&[at(2), at(0), at(-5), at(-10)], now());
        assert_eq!(
            states,
            vec![
                RateState::Scheduled,
                RateState::Current,
                RateState::Past,
                RateState::Past
            ]
        );
        assert_eq!(rate_states(&[at(1)], now()), vec![RateState::Scheduled]);
    }

    #[test]
    fn publish_request_defaults() {
        let req: PublishRateRequest = serde_json::from_str(
            r#"{"base_currency":"USD","target_currency":"JMD","rate":"160.5"}"#,
        )
        .expect("minimal request");
        assert_eq!(req.effective_at, None);
        assert!(!req.confirm_large_change);
    }
}
