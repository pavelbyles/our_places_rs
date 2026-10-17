//! 5-Policy typestate refund calculator and cancellation schedule engine.

use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Standard check-in hour in America/Jamaica (3:00 PM local time).
pub const STANDARD_CHECK_IN_HOUR: u32 = 15;
/// Standard check-in minute in America/Jamaica.
pub const STANDARD_CHECK_IN_MINUTE: u32 = 0;

/// Persisted cancellation policy identifier.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "cancellation_policy", rename_all = "lowercase")
)]
#[serde(rename_all = "lowercase")]
pub enum CancellationPolicy {
    /// Full refund up to 24 hours prior to check-in.
    Flexible,
    /// Full refund up to 5 days prior to check-in.
    Moderate,
    /// Full refund up to 14 days prior to check-in, 50% thereafter.
    Strict,
    /// Full refund up to 30 days prior to check-in.
    Firm,
    /// Non-refundable or minimal refund window.
    Limited,
}

impl CancellationPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Flexible => "flexible",
            Self::Moderate => "moderate",
            Self::Strict => "strict",
            Self::Firm => "firm",
            Self::Limited => "limited",
        }
    }
}

impl fmt::Display for CancellationPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CancellationPolicy {
    type Err = CancellationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "flexible" => Ok(Self::Flexible),
            "moderate" => Ok(Self::Moderate),
            "strict" => Ok(Self::Strict),
            "firm" => Ok(Self::Firm),
            "limited" => Ok(Self::Limited),
            _ => Err(CancellationError::InvalidPolicy(s.to_string())),
        }
    }
}

/// Policy marker for Flexible cancellation:
/// >= 24h before check-in: 100% accommodation refund; < 24h: 0% refund (100% host payout).
#[derive(Debug, Clone, Copy, Default)]
pub struct Flexible;

/// Policy marker for Moderate cancellation:
/// >= 5 days before check-in: 100%; 5 days to 24 hours: 50%; < 24 hours: 0%.
#[derive(Debug, Clone, Copy, Default)]
pub struct Moderate;

/// Policy marker for Strict cancellation:
/// Within 48h of booking (if booked >= 14 days out): 100%; >= 7 days before check-in: 50%; < 7 days: 0%.
#[derive(Debug, Clone, Copy, Default)]
pub struct Strict;

/// Policy marker for Firm cancellation:
/// >= 30 days before check-in (or within 48h if booked >= 14d out): 100%; 7 to 30 days: 50%; < 7 days: 0%.
#[derive(Debug, Clone, Copy, Default)]
pub struct Firm;

/// Policy marker for Limited cancellation:
/// >= 14 days before check-in: 100%; 7 to 14 days: 50%; < 7 days: 0%.
#[derive(Debug, Clone, Copy, Default)]
pub struct Limited;

/// Policy-specific refund schedule evaluated at compile time.
pub trait RefundSchedule {
    fn accommodation_refund_pct(t: &CancellationTiming) -> Decimal;
}

impl RefundSchedule for Flexible {
    fn accommodation_refund_pct(t: &CancellationTiming) -> Decimal {
        if t.hours_before_check_in >= 24 {
            Decimal::ONE
        } else {
            Decimal::ZERO
        }
    }
}

impl RefundSchedule for Moderate {
    fn accommodation_refund_pct(t: &CancellationTiming) -> Decimal {
        if t.days_before_check_in >= 5 {
            Decimal::ONE
        } else if t.hours_before_check_in >= 24 {
            Decimal::new(50, 2)
        } else {
            Decimal::ZERO
        }
    }
}

impl RefundSchedule for Strict {
    fn accommodation_refund_pct(t: &CancellationTiming) -> Decimal {
        if t.hours_since_booking <= 48 && t.days_booked_ahead >= 14 {
            Decimal::ONE
        } else if t.days_before_check_in >= 7 {
            Decimal::new(50, 2)
        } else {
            Decimal::ZERO
        }
    }
}

impl RefundSchedule for Firm {
    fn accommodation_refund_pct(t: &CancellationTiming) -> Decimal {
        if t.days_before_check_in >= 30
            || (t.hours_since_booking <= 48 && t.days_booked_ahead >= 14)
        {
            Decimal::ONE
        } else if t.days_before_check_in >= 7 {
            Decimal::new(50, 2)
        } else {
            Decimal::ZERO
        }
    }
}

impl RefundSchedule for Limited {
    fn accommodation_refund_pct(t: &CancellationTiming) -> Decimal {
        if t.days_before_check_in >= 14 {
            Decimal::ONE
        } else if t.days_before_check_in >= 7 {
            Decimal::new(50, 2)
        } else {
            Decimal::ZERO
        }
    }
}

/// Evaluated timing intervals relative to check-in and booking creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancellationTiming {
    pub days_before_check_in: i64,
    pub hours_before_check_in: i64,
    pub hours_since_booking: i64,
    pub days_booked_ahead: i64,
}

/// Typestate marker: amounts loaded, timing not yet evaluated.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pending;

/// Typestate marker: timing evaluated, ready to produce immutable quote.
#[derive(Debug, Clone, Copy, Default)]
pub struct Timed;

/// Typestate marker: quote generated.
#[derive(Debug, Clone, Copy, Default)]
pub struct Quoted;

/// Financial component snapshot required to calculate a refund.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefundableAmounts {
    pub accommodation_net: Decimal,
    pub cleaning_fee: Decimal,
    pub damage_deposit: Decimal,
    pub platform_fee: Decimal,
    pub tax_rate: Decimal,
    pub total_price: Decimal,
}

impl RefundableAmounts {
    pub fn new(
        accommodation_net: Decimal,
        cleaning_fee: Decimal,
        damage_deposit: Decimal,
        platform_fee: Decimal,
        tax_rate: Decimal,
        total_price: Decimal,
    ) -> Self {
        Self {
            accommodation_net,
            cleaning_fee,
            damage_deposit,
            platform_fee,
            tax_rate,
            total_price,
        }
    }
}

/// Immutable result of a cancellation quote calculation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RefundQuote {
    pub policy_pct: Decimal,
    pub accommodation_refund: Decimal,
    pub cleaning_refund: Decimal,
    pub deposit_refund: Decimal,
    pub tax_refund: Decimal,
    pub platform_fee_refund: Decimal,
    pub refund_total: Decimal,
    pub retained_total: Decimal,
}

impl RefundQuote {
    pub fn is_full_refund(&self) -> bool {
        self.policy_pct == Decimal::ONE
    }

    pub fn is_zero_refund(&self) -> bool {
        self.policy_pct == Decimal::ZERO
    }
}

/// Domain errors encountered during cancellation timing evaluation or quotation.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum CancellationError {
    #[error(
        "Cancellation attempted at or after check-in: cancelled_at ({cancelled_at}) >= check_in ({check_in})"
    )]
    AlreadyCheckedIn {
        cancelled_at: DateTime<Utc>,
        check_in: DateTime<Utc>,
    },
    #[error(
        "Cancellation attempted before booking time: cancelled_at ({cancelled_at}) < booked_at ({booked_at})"
    )]
    CancelledBeforeBooking {
        cancelled_at: DateTime<Utc>,
        booked_at: DateTime<Utc>,
    },
    #[error("Unknown cancellation policy: '{0}'")]
    InvalidPolicy(String),
    #[error("Invalid date or time calculation")]
    InvalidDate,
}

/// Typestate refund calculator enforcing compile-time schedule resolution.
pub struct RefundCalculator<P: RefundSchedule, S = Pending> {
    amounts: RefundableAmounts,
    timing: Option<CancellationTiming>,
    _p: PhantomData<(P, S)>,
}

impl<P: RefundSchedule> RefundCalculator<P, Pending> {
    /// Constructs a new calculator in the `Pending` state.
    pub fn new(amounts: RefundableAmounts) -> Self {
        Self {
            amounts,
            timing: None,
            _p: PhantomData,
        }
    }

    /// Evaluates cancellation timing relative to check-in in America/Jamaica.
    ///
    /// Check-in is evaluated at standard 15:00 America/Jamaica local time (UTC-5).
    /// Returns `Err(CancellationError::AlreadyCheckedIn)` if `cancelled_at >= check_in`.
    pub fn at(
        self,
        booked_at: DateTime<Utc>,
        check_in: NaiveDate,
        cancelled_at: DateTime<Utc>,
    ) -> Result<RefundCalculator<P, Timed>, CancellationError> {
        let jamaica_offset =
            FixedOffset::west_opt(5 * 3600).ok_or(CancellationError::InvalidDate)?;

        let check_in_naive = check_in
            .and_hms_opt(STANDARD_CHECK_IN_HOUR, STANDARD_CHECK_IN_MINUTE, 0)
            .ok_or(CancellationError::InvalidDate)?;

        // Jamaica is UTC-5 constantly without daylight saving time.
        // 15:00 Jamaica is 20:00 UTC.
        let check_in_utc = jamaica_offset
            .from_local_datetime(&check_in_naive)
            .single()
            .ok_or(CancellationError::InvalidDate)?
            .with_timezone(&Utc);

        if cancelled_at >= check_in_utc {
            return Err(CancellationError::AlreadyCheckedIn {
                cancelled_at,
                check_in: check_in_utc,
            });
        }

        if cancelled_at < booked_at {
            return Err(CancellationError::CancelledBeforeBooking {
                cancelled_at,
                booked_at,
            });
        }

        let duration_before_check_in = check_in_utc - cancelled_at;
        let hours_before_check_in = duration_before_check_in.num_hours();
        let days_before_check_in = hours_before_check_in / 24;

        let duration_since_booking = cancelled_at - booked_at;
        let hours_since_booking = duration_since_booking.num_hours();

        let booking_duration = check_in_utc - booked_at;
        let days_booked_ahead = booking_duration.num_hours() / 24;

        let timing = CancellationTiming {
            days_before_check_in,
            hours_before_check_in,
            hours_since_booking,
            days_booked_ahead,
        };

        Ok(RefundCalculator {
            amounts: self.amounts,
            timing: Some(timing),
            _p: PhantomData,
        })
    }
}

impl<P: RefundSchedule> RefundCalculator<P, Timed> {
    /// Returns a reference to the calculated timing parameters.
    pub fn timing(&self) -> &CancellationTiming {
        const DEFAULT_TIMING: CancellationTiming = CancellationTiming {
            days_before_check_in: 0,
            hours_before_check_in: 0,
            hours_since_booking: 0,
            days_booked_ahead: 0,
        };
        self.timing.as_ref().unwrap_or(&DEFAULT_TIMING)
    }

    /// Returns a reference to the snapshot amounts.
    pub fn amounts(&self) -> &RefundableAmounts {
        &self.amounts
    }

    /// Evaluates the policy schedule and calculates the watertight refund component breakdown.
    ///
    /// Invariant: `refund_total + retained_total == total_price`
    pub fn quote(self) -> RefundQuote {
        let timing = self.timing();
        let pct = P::accommodation_refund_pct(timing);

        let accommodation_refund = (self.amounts.accommodation_net * pct).round_dp(2);
        let cleaning_refund = self.amounts.cleaning_fee;
        let deposit_refund = self.amounts.damage_deposit;
        let taxable_base_refund = accommodation_refund + cleaning_refund;
        let tax_refund = (taxable_base_refund * self.amounts.tax_rate).round_dp(2);
        let platform_fee_refund = if pct == Decimal::ONE {
            self.amounts.platform_fee
        } else {
            Decimal::ZERO
        };

        let refund_total = accommodation_refund
            + cleaning_refund
            + deposit_refund
            + tax_refund
            + platform_fee_refund;
        let retained_total = self.amounts.total_price - refund_total;

        RefundQuote {
            policy_pct: pct,
            accommodation_refund,
            cleaning_refund,
            deposit_refund,
            tax_refund,
            platform_fee_refund,
            refund_total,
            retained_total,
        }
    }
}

/// Runtime dispatch evaluating cancellation refund quote from persisted policy enum.
pub fn quote_refund(
    policy: CancellationPolicy,
    amounts: RefundableAmounts,
    booked_at: DateTime<Utc>,
    check_in: NaiveDate,
    cancelled_at: DateTime<Utc>,
) -> Result<RefundQuote, CancellationError> {
    match policy {
        CancellationPolicy::Flexible => RefundCalculator::<Flexible>::new(amounts)
            .at(booked_at, check_in, cancelled_at)
            .map(|c| c.quote()),
        CancellationPolicy::Moderate => RefundCalculator::<Moderate>::new(amounts)
            .at(booked_at, check_in, cancelled_at)
            .map(|c| c.quote()),
        CancellationPolicy::Strict => RefundCalculator::<Strict>::new(amounts)
            .at(booked_at, check_in, cancelled_at)
            .map(|c| c.quote()),
        CancellationPolicy::Firm => RefundCalculator::<Firm>::new(amounts)
            .at(booked_at, check_in, cancelled_at)
            .map(|c| c.quote()),
        CancellationPolicy::Limited => RefundCalculator::<Limited>::new(amounts)
            .at(booked_at, check_in, cancelled_at)
            .map(|c| c.quote()),
    }
}
