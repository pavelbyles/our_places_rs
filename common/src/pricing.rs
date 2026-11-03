use crate::models::{DynamicPricingQuote, FeeItem, NightlyRateBreakdown, PriceOverride};
use crate::promo::PromoTerms;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PricingError {
    #[error("Invalid date range: check-out must be after check-in")]
    InvalidDateRange,

    #[error("Minimum night stay requirement not met: required {required}, requested {provided}")]
    MinNightsNotMet { required: i32, provided: i32 },
}

pub fn calculate_dynamic_quote(
    base_nightly_rate: Decimal,
    base_min_nights: i32,
    overrides: &[PriceOverride],
    check_in: NaiveDate,
    check_out: NaiveDate,
) -> Result<DynamicPricingQuote, PricingError> {
    if check_out <= check_in {
        return Err(PricingError::InvalidDateRange);
    }

    let total_nights = (check_out - check_in).num_days() as i32;
    let mut nightly_breakdown = Vec::with_capacity(total_nights as usize);
    let mut subtotal = Decimal::ZERO;
    let mut required_min_nights = base_min_nights;

    let mut current_date = check_in;
    while current_date < check_out {
        let active_override = overrides
            .iter()
            .find(|o| current_date >= o.start_date && current_date < o.end_date);

        let (night_rate, is_override) = match active_override {
            Some(ovr) => {
                if ovr.min_nights > required_min_nights {
                    required_min_nights = ovr.min_nights;
                }
                (ovr.nightly_rate, true)
            }
            None => (base_nightly_rate, false),
        };

        subtotal += night_rate;
        nightly_breakdown.push(NightlyRateBreakdown {
            date: current_date,
            rate: night_rate,
            is_override,
        });

        current_date += chrono::Duration::days(1);
    }

    if total_nights < required_min_nights {
        return Err(PricingError::MinNightsNotMet {
            required: required_min_nights,
            provided: total_nights,
        });
    }

    let effective_daily_rate = subtotal / Decimal::from(total_nights);

    Ok(DynamicPricingQuote {
        nightly_breakdown,
        subtotal,
        effective_daily_rate,
        required_min_nights,
    })
}

/// Marker state for initial base rate calculation.
pub struct BaseRate;
/// Marker state after applying weekly or monthly stay discounts.
pub struct Discounted;
/// Marker state after the promo step, which every pricing path must pass through.
pub struct PromoApplied;
/// Marker state after applying statutory taxes.
pub struct Taxed;

/// Type-state calculator for booking accommodation, extra-guest fees, discounts, taxes, and fees.
pub struct BookingCalculator<State> {
    /// Effective nightly base rate in checkout currency.
    pub actual_daily_rate: Decimal,
    /// Number of nights in the reservation.
    pub total_days: i32,
    /// Number of extra guests above `guests_included`.
    pub extra_guests: i32,
    /// Per-night fee per extra guest in checkout currency.
    pub extra_guest_fee_per_night: Decimal,
    /// Total extra-guest fee across all nights (`extra_guests * extra_guest_fee_per_night * total_days`).
    pub extra_guest_fee_total: Decimal,
    /// Gross accommodation subtotal before discounts (includes seasonal nights + `extra_guest_fee_total`).
    pub sub_total_price: Decimal,
    /// Discount amount subtracted for weekly/monthly stays.
    pub discount_value: Option<Decimal>,
    /// Net accommodation subtotal after subtracting `discount_value` and `promo_stay_discount`.
    pub discounted_subtotal: Decimal,
    /// One-time host cleaning fee, before any promo.
    pub cleaning_fee: Decimal,
    /// Cleaning fee after subtracting `promo_cleaning_discount`.
    pub cleaning_net: Decimal,
    /// Promo discount taken from the stay.
    pub promo_stay_discount: Decimal,
    /// Promo discount taken from the cleaning fee.
    pub promo_cleaning_discount: Decimal,
    /// Refundable security damage deposit.
    pub damage_deposit: Decimal,
    /// Guest platform service fee (5% of `discounted_subtotal`).
    pub platform_fee: Decimal,
    /// Statutory tax rate (e.g., `0.15` for 15% GCT).
    pub tax_rate: Decimal,
    /// Computed statutory tax amount.
    pub tax_value: Option<Decimal>,
    /// Itemized fee breakdown for receipts and UI display.
    pub fee_breakdown: Vec<FeeItem>,
    /// Final guest total price (`discounted_subtotal + cleaning_net + tax_value + platform_fee + damage_deposit`).
    pub total_price: Decimal,
    /// Current type-state marker.
    pub state: State,
}

impl BookingCalculator<BaseRate> {
    /// Initializes a calculator from a flat nightly rate and stay duration.
    pub fn new(actual_daily_rate: Decimal, total_days: i32) -> Self {
        let sub_total_price = actual_daily_rate * Decimal::from(total_days.max(0));
        Self {
            actual_daily_rate,
            total_days,
            extra_guests: 0,
            extra_guest_fee_per_night: Decimal::ZERO,
            extra_guest_fee_total: Decimal::ZERO,
            sub_total_price,
            discount_value: None,
            discounted_subtotal: sub_total_price,
            cleaning_fee: Decimal::ZERO,
            cleaning_net: Decimal::ZERO,
            promo_stay_discount: Decimal::ZERO,
            promo_cleaning_discount: Decimal::ZERO,
            damage_deposit: Decimal::ZERO,
            platform_fee: Decimal::ZERO,
            tax_rate: Decimal::ZERO,
            tax_value: None,
            fee_breakdown: Vec::new(),
            total_price: Decimal::ZERO,
            state: BaseRate,
        }
    }

    /// Initializes a calculator with a pre-computed dynamic seasonal subtotal.
    pub fn with_subtotal(
        actual_daily_rate: Decimal,
        total_days: i32,
        sub_total_price: Decimal,
    ) -> Self {
        Self {
            actual_daily_rate,
            total_days,
            extra_guests: 0,
            extra_guest_fee_per_night: Decimal::ZERO,
            extra_guest_fee_total: Decimal::ZERO,
            sub_total_price,
            discount_value: None,
            discounted_subtotal: sub_total_price,
            cleaning_fee: Decimal::ZERO,
            cleaning_net: Decimal::ZERO,
            promo_stay_discount: Decimal::ZERO,
            promo_cleaning_discount: Decimal::ZERO,
            damage_deposit: Decimal::ZERO,
            platform_fee: Decimal::ZERO,
            tax_rate: Decimal::ZERO,
            tax_value: None,
            fee_breakdown: Vec::new(),
            total_price: Decimal::ZERO,
            state: BaseRate,
        }
    }

    /// Adds extra-guest charges (`max(0, number_of_persons - guests_included) * extra_guest_fee_per_night * total_days`)
    /// to `sub_total_price` prior to discount calculation so that weekly and monthly discounts
    /// apply to the overall stay subtotal.
    pub fn with_extra_guests(
        mut self,
        number_of_persons: i32,
        guests_included: i32,
        extra_guest_fee_per_night: Decimal,
    ) -> Self {
        let extra_guests = (number_of_persons - guests_included.max(1)).max(0);
        let fee_per_night = extra_guest_fee_per_night.max(Decimal::ZERO).round_dp(2);
        let days = Decimal::from(self.total_days.max(0));
        let extra_guest_fee_total =
            (Decimal::from(extra_guests) * fee_per_night * days).round_dp(2);

        self.extra_guests = extra_guests;
        self.extra_guest_fee_per_night = fee_per_night;
        self.extra_guest_fee_total = extra_guest_fee_total;
        self.sub_total_price += extra_guest_fee_total;
        self.discounted_subtotal = self.sub_total_price;
        self
    }

    /// Applies monthly (`>= 28` nights) or weekly (`>= 7` nights) percentage discounts
    /// to the overall stay `sub_total_price` (including any extra-guest fees).
    pub fn apply_discounts(
        mut self,
        monthly_pct: Option<Decimal>,
        weekly_pct: Option<Decimal>,
    ) -> BookingCalculator<Discounted> {
        if let (Some(pct), true) = (monthly_pct, self.total_days >= 28) {
            let discount = (self.sub_total_price * (pct / Decimal::new(100, 0))).round_dp(2);
            self.discount_value = Some(discount);
            self.discounted_subtotal = self.sub_total_price - discount;
        } else if let (Some(pct), true) = (weekly_pct, self.total_days >= 7) {
            let discount = (self.sub_total_price * (pct / Decimal::new(100, 0))).round_dp(2);
            self.discount_value = Some(discount);
            self.discounted_subtotal = self.sub_total_price - discount;
        }

        BookingCalculator {
            actual_daily_rate: self.actual_daily_rate,
            total_days: self.total_days,
            extra_guests: self.extra_guests,
            extra_guest_fee_per_night: self.extra_guest_fee_per_night,
            extra_guest_fee_total: self.extra_guest_fee_total,
            sub_total_price: self.sub_total_price,
            discount_value: self.discount_value,
            discounted_subtotal: self.discounted_subtotal,
            cleaning_fee: self.cleaning_fee,
            cleaning_net: self.cleaning_net,
            promo_stay_discount: self.promo_stay_discount,
            promo_cleaning_discount: self.promo_cleaning_discount,
            damage_deposit: self.damage_deposit,
            platform_fee: self.platform_fee,
            tax_rate: self.tax_rate,
            tax_value: self.tax_value,
            fee_breakdown: self.fee_breakdown,
            total_price: self.total_price,
            state: Discounted,
        }
    }
}

impl BookingCalculator<Discounted> {
    /// Configures host cleaning fee and security damage deposit.
    pub fn with_fees(
        mut self,
        cleaning_fee: Option<Decimal>,
        damage_deposit: Option<Decimal>,
    ) -> Self {
        self.cleaning_fee = cleaning_fee.unwrap_or(Decimal::ZERO).max(Decimal::ZERO);
        self.cleaning_net = self.cleaning_fee;
        self.damage_deposit = damage_deposit.unwrap_or(Decimal::ZERO).max(Decimal::ZERO);
        self
    }

    /// Applies a promo code's discount, after weekly or monthly discounts and
    /// before tax. Pass `None` for a booking with no code. Whether the code may
    /// be used is checked before this step, by the rules in [`crate::promo`].
    pub fn apply_promo(mut self, terms: Option<&PromoTerms>) -> BookingCalculator<PromoApplied> {
        if let Some(terms) = terms {
            let (stay, cleaning) = terms.discounts(self.discounted_subtotal, self.cleaning_fee);
            self.promo_stay_discount = stay;
            self.promo_cleaning_discount = cleaning;
            self.discounted_subtotal -= stay;
            self.cleaning_net = self.cleaning_fee - cleaning;
        }

        BookingCalculator {
            actual_daily_rate: self.actual_daily_rate,
            total_days: self.total_days,
            extra_guests: self.extra_guests,
            extra_guest_fee_per_night: self.extra_guest_fee_per_night,
            extra_guest_fee_total: self.extra_guest_fee_total,
            sub_total_price: self.sub_total_price,
            discount_value: self.discount_value,
            discounted_subtotal: self.discounted_subtotal,
            cleaning_fee: self.cleaning_fee,
            cleaning_net: self.cleaning_net,
            promo_stay_discount: self.promo_stay_discount,
            promo_cleaning_discount: self.promo_cleaning_discount,
            damage_deposit: self.damage_deposit,
            platform_fee: self.platform_fee,
            tax_rate: self.tax_rate,
            tax_value: self.tax_value,
            fee_breakdown: self.fee_breakdown,
            total_price: self.total_price,
            state: PromoApplied,
        }
    }
}

impl BookingCalculator<PromoApplied> {
    /// Computes statutory tax on the taxable base (`discounted_subtotal + cleaning_net`).
    pub fn apply_taxes(mut self, tax_rate: Decimal) -> BookingCalculator<Taxed> {
        let taxable_base = self.discounted_subtotal + self.cleaning_net;
        let tax_value_decimal = (taxable_base * tax_rate).round_dp(2);
        self.tax_rate = tax_rate;
        self.tax_value = Some(tax_value_decimal);

        BookingCalculator {
            actual_daily_rate: self.actual_daily_rate,
            total_days: self.total_days,
            extra_guests: self.extra_guests,
            extra_guest_fee_per_night: self.extra_guest_fee_per_night,
            extra_guest_fee_total: self.extra_guest_fee_total,
            sub_total_price: self.sub_total_price,
            discount_value: self.discount_value,
            discounted_subtotal: self.discounted_subtotal,
            cleaning_fee: self.cleaning_fee,
            cleaning_net: self.cleaning_net,
            promo_stay_discount: self.promo_stay_discount,
            promo_cleaning_discount: self.promo_cleaning_discount,
            damage_deposit: self.damage_deposit,
            platform_fee: self.platform_fee,
            tax_rate: self.tax_rate,
            tax_value: self.tax_value,
            fee_breakdown: self.fee_breakdown,
            total_price: self.total_price,
            state: Taxed,
        }
    }
}

impl BookingCalculator<Taxed> {
    /// Finalizes the calculation by computing the 5% platform fee and total price.
    pub fn finalize(mut self) -> Self {
        let platform_fee = (self.discounted_subtotal * Decimal::new(5, 2)).round_dp(2);
        self.platform_fee = platform_fee;

        if self.cleaning_fee > Decimal::ZERO {
            self.fee_breakdown.push(FeeItem {
                name: "Cleaning Fee".to_string(),
                amount: self.cleaning_fee,
            });
        }
        if self.damage_deposit > Decimal::ZERO {
            self.fee_breakdown.push(FeeItem {
                name: "Damage Deposit".to_string(),
                amount: self.damage_deposit,
            });
        }
        self.fee_breakdown.push(FeeItem {
            name: "Platform Fee".to_string(),
            amount: platform_fee,
        });

        let taxable_base = self.discounted_subtotal + self.cleaning_net;
        self.total_price = taxable_base
            + self.tax_value.unwrap_or(Decimal::ZERO)
            + platform_fee
            + self.damage_deposit;

        self
    }
}

/// Largest amount the booking table's `NUMERIC(10,2)` money columns can hold.
const MAX_STORED_AMOUNT: Decimal = Decimal::from_parts(1_410_065_407, 2, 0, false, 2);

/// Whether every amount of a priced stay fits the booking's money columns.
pub fn fits_booking_columns(calculator: &BookingCalculator<Taxed>) -> bool {
    [
        calculator.sub_total_price,
        calculator.discount_value.unwrap_or(Decimal::ZERO),
        calculator.promo_stay_discount,
        calculator.promo_cleaning_discount,
        calculator.tax_value.unwrap_or(Decimal::ZERO),
        calculator.cleaning_fee,
        calculator.damage_deposit,
        calculator.platform_fee,
        calculator.total_price,
    ]
    .iter()
    .all(|amount| *amount <= MAX_STORED_AMOUNT)
}

/// Everything needed to price a stay. Amounts are in the listing's base currency.
pub struct StayPricingInput<'a> {
    /// The listing's nightly rate.
    pub base_nightly_rate: Decimal,
    /// The listing's minimum stay.
    pub minimum_stay: i32,
    /// Overrides touching the stay.
    pub overrides: &'a [PriceOverride],
    /// Listing currency to payment currency.
    pub rate: Decimal,
    /// First night.
    pub check_in: NaiveDate,
    /// Departure date.
    pub check_out: NaiveDate,
    /// Guests staying.
    pub number_of_persons: i32,
    /// Guests the nightly rate covers.
    pub guests_included: i32,
    /// Fee per extra guest per night.
    pub extra_guest_fee: Decimal,
    /// One-time cleaning fee.
    pub cleaning_fee: Decimal,
    /// Refundable damage deposit.
    pub damage_deposit: Decimal,
    /// Discount percentage for stays of 7 nights or more.
    pub weekly_discount_percentage: Option<Decimal>,
    /// Discount percentage for stays of 28 nights or more.
    pub monthly_discount_percentage: Option<Decimal>,
}

/// Converts and prices a stay up to the promo step. Quotes, holds and search
/// totals all start here, so they can't disagree.
///
/// The nightly rate always rounds to 2 places. Other amounts are left as they
/// are at a rate of 1, and round otherwise.
pub fn price_before_promo(
    input: StayPricingInput<'_>,
) -> Result<BookingCalculator<Discounted>, PricingError> {
    let total_days = i32::try_from((input.check_out - input.check_in).num_days())
        .map_err(|_| PricingError::InvalidDateRange)?;
    let convert = |amount: Decimal| {
        if input.rate == Decimal::ONE {
            amount
        } else {
            (amount * input.rate).round_dp(2)
        }
    };
    let base_nightly_rate = (input.base_nightly_rate * input.rate).round_dp(2);
    let converted_overrides: Vec<PriceOverride> = input
        .overrides
        .iter()
        .cloned()
        .map(|mut ovr| {
            ovr.nightly_rate = convert(ovr.nightly_rate);
            ovr
        })
        .collect();

    let dynamic_quote = calculate_dynamic_quote(
        base_nightly_rate,
        input.minimum_stay,
        &converted_overrides,
        input.check_in,
        input.check_out,
    )?;

    Ok(BookingCalculator::with_subtotal(
        dynamic_quote.effective_daily_rate,
        total_days,
        dynamic_quote.subtotal,
    )
    .with_extra_guests(
        input.number_of_persons,
        input.guests_included,
        convert(input.extra_guest_fee),
    )
    .apply_discounts(
        input.monthly_discount_percentage,
        input.weekly_discount_percentage,
    )
    .with_fees(
        Some(convert(input.cleaning_fee)),
        Some(convert(input.damage_deposit)),
    ))
}

/// The one formula for a host's gross payout, in the host's currency. It covers
/// the net accommodation and the net cleaning fee. The refundable deposit, GCT
/// and the guest platform fee are never part of it.
pub fn host_gross(accommodation_net: Decimal, cleaning_net: Decimal, rate: Decimal) -> Decimal {
    ((accommodation_net + cleaning_net) * rate).round_dp(2)
}

/// Calculates host platform commission fee and net payout amount using Banker's rounding.
/// Formula:
/// fee_amount = (gross_amount * platform_fee_pct).round_dp(2)
/// net_payout = max(0, gross_amount - fee_amount - tax_withheld)
pub fn calculate_host_payout(
    gross_amount: Decimal,
    platform_fee_pct: Decimal,
    tax_withheld: Decimal,
) -> (Decimal, Decimal) {
    let fee_amount = (gross_amount * platform_fee_pct).round_dp(2);
    let net_payout = (gross_amount - fee_amount - tax_withheld).max(Decimal::ZERO);
    (fee_amount, net_payout)
}

/// Domain validation error when evaluating a proposed reservation change.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum ReservationChangeValidationError {
    /// Neither dates nor guest count were modified from the current booking.
    #[error("At least one reservation term (dates or guest count) must change")]
    NoMaterialChange,

    /// Check-out date is on or before check-in date.
    #[error("Invalid date range: check-out must be after check-in")]
    InvalidDateRange,

    /// Attempted to modify a reservation whose stay has already ended.
    #[error("Reservation cannot be modified after check-out date has arrived")]
    StayAlreadyEnded,

    /// Attempted to modify check-in date on or after the current or proposed check-in date.
    #[error("Check-in date cannot be modified once check-in has arrived")]
    CheckInAlreadyArrived,

    /// Attempted to set check-out date to today or a past date.
    #[error("Proposed check-out date must be in the future")]
    CheckOutNotInFuture,

    /// Party size or adult count is invalid or exceeds listing capacity.
    #[error("Guest count must be between 1 and {max_guests} with at least 1 adult")]
    InvalidGuestCount {
        /// Maximum allowed guests on the listing.
        max_guests: i32,
    },
}

/// Validates proposed reservation date changes against `America/Jamaica` (`today_jm`)
/// pre-stay and mid-stay rules, returning the auto-expiry cutoff date (`expires_at_date`).
pub fn validate_reservation_change_dates(
    current_from: NaiveDate,
    current_to: NaiveDate,
    proposed_from: NaiveDate,
    proposed_to: NaiveDate,
    today_jm: NaiveDate,
) -> Result<NaiveDate, ReservationChangeValidationError> {
    if current_to <= today_jm {
        return Err(ReservationChangeValidationError::StayAlreadyEnded);
    }
    if proposed_to <= proposed_from {
        return Err(ReservationChangeValidationError::InvalidDateRange);
    }
    if proposed_from != current_from && (current_from <= today_jm || proposed_from <= today_jm) {
        return Err(ReservationChangeValidationError::CheckInAlreadyArrived);
    }
    if proposed_to != current_to && proposed_to <= today_jm {
        return Err(ReservationChangeValidationError::CheckOutNotInFuture);
    }

    let expires_at_date = if proposed_from != current_from {
        current_from.min(proposed_from)
    } else {
        current_to.min(proposed_to)
    };

    Ok(expires_at_date)
}

#[cfg(test)]
mod promo_tests {
    use super::*;
    use crate::promo::{PromoDiscountType, PromoScope};
    use rust_decimal_macros::dec;

    fn terms(discount_type: PromoDiscountType, value: Decimal, scope: PromoScope) -> PromoTerms {
        PromoTerms {
            code: "SUMMER".to_string(),
            discount_type,
            discount_value: value,
            scope,
            currency: None,
            exchange_rate: Decimal::ONE,
            min_nights: None,
            min_booking_value: None,
            stay_from: None,
            stay_to: None,
        }
    }

    /// `S` 1,000 with a 100 weekly discount, 100 cleaning and 200 deposit, at 15% GCT.
    fn priced(promo: Option<&PromoTerms>, cleaning: Decimal) -> BookingCalculator<Taxed> {
        BookingCalculator::with_subtotal(dec!(142.86), 7, dec!(1000))
            .apply_discounts(None, Some(dec!(10)))
            .with_fees(Some(cleaning), Some(dec!(200)))
            .apply_promo(promo)
            .apply_taxes(dec!(0.15))
            .finalize()
    }

    #[test]
    fn matches_the_worked_example_exactly() {
        let promo = terms(
            PromoDiscountType::Percentage,
            dec!(10),
            PromoScope::StayAndCleaning,
        );
        let c = priced(Some(&promo), dec!(100));

        assert_eq!(c.discount_value, Some(dec!(100.00)));
        assert_eq!(c.promo_stay_discount, dec!(90.00));
        assert_eq!(c.promo_cleaning_discount, dec!(10.00));
        assert_eq!(c.discounted_subtotal, dec!(810.00));
        assert_eq!(c.cleaning_fee, dec!(100));
        assert_eq!(c.cleaning_net, dec!(90.00));
        assert_eq!(c.tax_value, Some(dec!(135.00)));
        assert_eq!(c.platform_fee, dec!(40.50));
        assert_eq!(c.damage_deposit, dec!(200));
        assert_eq!(c.total_price, dec!(1275.50));
        assert_eq!(
            host_gross(c.discounted_subtotal, c.cleaning_net, Decimal::ONE),
            dec!(900.00)
        );
    }

    #[test]
    fn no_promo_prices_as_before() {
        let c = priced(None, dec!(100));
        assert_eq!(c.promo_stay_discount, Decimal::ZERO);
        assert_eq!(c.promo_cleaning_discount, Decimal::ZERO);
        assert_eq!(c.cleaning_net, dec!(100));
        assert_eq!(c.discounted_subtotal, dec!(900.00));
        assert_eq!(c.tax_value, Some(dec!(150.00)));
        assert_eq!(c.platform_fee, dec!(45.00));
        assert_eq!(c.total_price, dec!(1395.00));
    }

    #[test]
    fn stay_scope_leaves_the_cleaning_fee_alone() {
        let promo = terms(PromoDiscountType::Percentage, dec!(10), PromoScope::Stay);
        let c = priced(Some(&promo), dec!(100));
        assert_eq!(c.promo_cleaning_discount, Decimal::ZERO);
        assert_eq!(c.cleaning_net, dec!(100));
        assert_eq!(c.tax_value, Some(dec!(136.50)));
        assert_eq!(c.platform_fee, dec!(40.50));
        assert_eq!(c.total_price, dec!(1287.00));
    }

    #[test]
    fn a_fixed_discount_goes_to_the_stay_first_and_is_capped() {
        let over = terms(
            PromoDiscountType::Fixed,
            dec!(950),
            PromoScope::StayAndCleaning,
        );
        let c = priced(Some(&over), dec!(100));
        assert_eq!(c.promo_stay_discount, dec!(900));
        assert_eq!(c.promo_cleaning_discount, dec!(50));
        assert_eq!(c.discounted_subtotal, Decimal::ZERO);
        assert_eq!(c.cleaning_net, dec!(50));

        let huge = terms(PromoDiscountType::Fixed, dec!(99999), PromoScope::Stay);
        let c = priced(Some(&huge), dec!(100));
        assert_eq!(c.promo_stay_discount, dec!(900));
        assert_eq!(c.discounted_subtotal, Decimal::ZERO);
        assert_eq!(c.cleaning_net, dec!(100));
    }

    #[test]
    fn a_full_discount_leaves_only_the_deposit() {
        let all = terms(
            PromoDiscountType::Percentage,
            dec!(100),
            PromoScope::StayAndCleaning,
        );
        let c = priced(Some(&all), dec!(100));
        assert_eq!(c.tax_value, Some(dec!(0.00)));
        assert_eq!(c.platform_fee, dec!(0.00));
        assert_eq!(c.total_price, dec!(200));
    }

    #[test]
    fn stay_and_cleaning_with_no_cleaning_fee_takes_nothing_from_it() {
        let promo = terms(
            PromoDiscountType::Percentage,
            dec!(10),
            PromoScope::StayAndCleaning,
        );
        let c = priced(Some(&promo), Decimal::ZERO);
        assert_eq!(c.promo_cleaning_discount, Decimal::ZERO);
        assert_eq!(c.cleaning_net, Decimal::ZERO);
    }

    #[test]
    fn the_promo_applies_after_the_weekly_discount() {
        let fixed_pct = terms(PromoDiscountType::Percentage, dec!(50), PromoScope::Stay);
        let c = priced(Some(&fixed_pct), dec!(100));
        // 50% of 900, not of the 1,000 before the weekly discount.
        assert_eq!(c.promo_stay_discount, dec!(450.00));
    }
}

#[cfg(test)]
mod host_gross_tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn host_gross_adds_the_cleaning_fee_before_converting_and_rounding_once() {
        assert_eq!(host_gross(dec!(810), dec!(90), Decimal::ONE), dec!(900));
        assert_eq!(
            host_gross(dec!(100.01), dec!(100.01), dec!(0.5)),
            dec!(100.01)
        );
    }

    #[test]
    fn host_gross_with_commission_is_charged_on_the_whole_gross() {
        let gross = host_gross(dec!(400), dec!(100), Decimal::ONE);
        assert_eq!(
            calculate_host_payout(gross, dec!(0.1), Decimal::ZERO),
            (dec!(50.0), dec!(450.0))
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn test_calculate_host_payout_precision() {
        // Standard 3% commission on $1,250.50 gross, 0 tax
        let (fee, net) =
            calculate_host_payout(Decimal::new(125050, 2), Decimal::new(3, 2), Decimal::ZERO);
        // 1250.50 * 0.03 = 37.515 => rounds to 37.52
        assert_eq!(fee, Decimal::new(3752, 2));
        assert_eq!(net, Decimal::new(121298, 2));

        // Zero commission
        let (fee_zero, net_zero) =
            calculate_host_payout(Decimal::new(50000, 2), Decimal::ZERO, Decimal::ZERO);
        assert_eq!(fee_zero, Decimal::ZERO);
        assert_eq!(net_zero, Decimal::new(50000, 2));

        // With tax withholding
        let (fee_tax, net_tax) = calculate_host_payout(
            Decimal::new(100000, 2),
            Decimal::new(5, 2),    // 5% = 50.00
            Decimal::new(1500, 2), // 15.00 tax withheld
        );
        assert_eq!(fee_tax, Decimal::new(5000, 2));
        assert_eq!(net_tax, Decimal::new(93500, 2)); // 1000 - 50 - 15 = 935.00
    }

    #[test]
    fn test_calculator_basic_stay() {
        let daily_rate = Decimal::new(100, 0); // $100
        let calc = BookingCalculator::new(daily_rate, 3)
            .apply_discounts(None, None)
            .apply_promo(None)
            .apply_taxes(Decimal::new(15, 2))
            .finalize();

        assert_eq!(calc.sub_total_price, Decimal::new(300, 0));
        assert_eq!(calc.discount_value, None);
        assert_eq!(calc.discounted_subtotal, Decimal::new(300, 0));
        assert_eq!(calc.tax_rate, Decimal::new(15, 2));
        assert_eq!(calc.tax_value, Some(Decimal::new(4500, 2))); // 15% = 45.00
        assert_eq!(calc.fee_breakdown.len(), 1);
        assert_eq!(calc.fee_breakdown[0].amount, Decimal::new(1500, 2)); // 5% platform fee = 15.00
        assert_eq!(calc.total_price, Decimal::new(36000, 2)); // 300 + 45 + 15 = 360.00
    }

    #[test]
    fn test_weekly_discount_application() {
        let daily_rate = Decimal::new(100, 0);
        let calc = BookingCalculator::new(daily_rate, 7)
            .apply_discounts(Some(Decimal::new(20, 0)), Some(Decimal::new(10, 0)))
            .apply_promo(None)
            .apply_taxes(Decimal::new(15, 2))
            .finalize();

        assert_eq!(calc.sub_total_price, Decimal::new(700, 0));
        assert_eq!(calc.discount_value, Some(Decimal::new(70, 0)));
        assert_eq!(calc.discounted_subtotal, Decimal::new(630, 0));
        assert_eq!(calc.tax_value, Some(Decimal::new(9450, 2))); // 15% of 630 = 94.50
        assert_eq!(calc.fee_breakdown[0].amount, Decimal::new(3150, 2)); // 5% of 630 = 31.50
        assert_eq!(calc.total_price, Decimal::new(75600, 2)); // 630 + 94.50 + 31.50 = 756.00
    }

    #[test]
    fn test_monthly_discount_application() {
        let daily_rate = Decimal::new(100, 0);
        let calc = BookingCalculator::new(daily_rate, 30)
            .apply_discounts(Some(Decimal::new(25, 0)), Some(Decimal::new(10, 0)))
            .apply_promo(None)
            .apply_taxes(Decimal::new(15, 2))
            .finalize();

        assert_eq!(calc.sub_total_price, Decimal::new(3000, 0));
        assert_eq!(calc.discount_value, Some(Decimal::new(750, 0)));
        assert_eq!(calc.discounted_subtotal, Decimal::new(2250, 0));
        assert_eq!(calc.tax_value, Some(Decimal::new(33750, 2))); // 15% of 2250 = 337.50
        assert_eq!(calc.fee_breakdown[0].amount, Decimal::new(11250, 2)); // 5% of 2250 = 112.50
        assert_eq!(calc.total_price, Decimal::new(270000, 2)); // 2250 + 337.50 + 112.50 = 2700.00
    }

    #[test]
    fn test_zero_and_negative_days_guard() {
        let daily_rate = Decimal::new(100, 0);
        let calc = BookingCalculator::new(daily_rate, -2)
            .apply_discounts(None, None)
            .apply_promo(None)
            .apply_taxes(Decimal::new(15, 2))
            .finalize();

        assert_eq!(calc.sub_total_price, Decimal::ZERO);
        assert_eq!(calc.total_price, Decimal::ZERO);
    }

    #[test]
    fn test_dynamic_quote_base_rate_fallback() {
        let base_rate = Decimal::new(150, 0);
        let check_in = NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();
        let check_out = NaiveDate::from_ymd_opt(2026, 12, 4).unwrap();

        let quote = calculate_dynamic_quote(base_rate, 1, &[], check_in, check_out).unwrap();

        assert_eq!(quote.subtotal, Decimal::new(450, 0));
        assert_eq!(quote.effective_daily_rate, Decimal::new(150, 0));
        assert_eq!(quote.nightly_breakdown.len(), 3);
        assert!(!quote.nightly_breakdown[0].is_override);
    }

    #[test]
    fn test_dynamic_quote_single_override() {
        let base_rate = Decimal::new(150, 0);
        let check_in = NaiveDate::from_ymd_opt(2026, 12, 20).unwrap();
        let check_out = NaiveDate::from_ymd_opt(2026, 12, 23).unwrap();

        let overrides = vec![PriceOverride {
            id: Uuid::new_v4(),
            listing_id: Uuid::new_v4(),
            start_date: NaiveDate::from_ymd_opt(2026, 12, 20).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 12, 26).unwrap(),
            nightly_rate: Decimal::new(300, 0),
            min_nights: 2,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }];

        let quote = calculate_dynamic_quote(base_rate, 1, &overrides, check_in, check_out).unwrap();

        assert_eq!(quote.subtotal, Decimal::new(900, 0)); // 3 nights @ $300
        assert_eq!(quote.effective_daily_rate, Decimal::new(300, 0));
        assert!(quote.nightly_breakdown.iter().all(|n| n.is_override));
    }

    #[test]
    fn test_dynamic_quote_partial_override() {
        let base_rate = Decimal::new(100, 0);
        let check_in = NaiveDate::from_ymd_opt(2026, 12, 18).unwrap();
        let check_out = NaiveDate::from_ymd_opt(2026, 12, 22).unwrap();

        let overrides = vec![PriceOverride {
            id: Uuid::new_v4(),
            listing_id: Uuid::new_v4(),
            start_date: NaiveDate::from_ymd_opt(2026, 12, 20).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 12, 25).unwrap(),
            nightly_rate: Decimal::new(250, 0),
            min_nights: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }];

        let quote = calculate_dynamic_quote(base_rate, 1, &overrides, check_in, check_out).unwrap();

        // Nights: Dec 18 ($100), Dec 19 ($100), Dec 20 ($250), Dec 21 ($250) = $700
        assert_eq!(quote.subtotal, Decimal::new(700, 0));
        assert_eq!(quote.effective_daily_rate, Decimal::new(175, 0));
        assert!(!quote.nightly_breakdown[0].is_override);
        assert!(!quote.nightly_breakdown[1].is_override);
        assert!(quote.nightly_breakdown[2].is_override);
        assert!(quote.nightly_breakdown[3].is_override);
    }

    #[test]
    fn test_dynamic_quote_min_nights_enforcement() {
        let base_rate = Decimal::new(150, 0);
        let check_in = NaiveDate::from_ymd_opt(2026, 12, 24).unwrap();
        let check_out = NaiveDate::from_ymd_opt(2026, 12, 26).unwrap(); // 2 nights

        let overrides = vec![PriceOverride {
            id: Uuid::new_v4(),
            listing_id: Uuid::new_v4(),
            start_date: NaiveDate::from_ymd_opt(2026, 12, 20).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 12, 30).unwrap(),
            nightly_rate: Decimal::new(400, 0),
            min_nights: 4, // Requires 4 nights
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }];

        let result = calculate_dynamic_quote(base_rate, 1, &overrides, check_in, check_out);
        assert_eq!(
            result,
            Err(PricingError::MinNightsNotMet {
                required: 4,
                provided: 2
            })
        );
    }

    #[test]
    fn test_dynamic_quote_invalid_dates() {
        let base_rate = Decimal::new(150, 0);
        let check_in = NaiveDate::from_ymd_opt(2026, 12, 25).unwrap();
        let check_out = NaiveDate::from_ymd_opt(2026, 12, 20).unwrap();

        let result = calculate_dynamic_quote(base_rate, 1, &[], check_in, check_out);
        assert_eq!(result, Err(PricingError::InvalidDateRange));
    }

    #[test]
    fn test_booking_calculator_with_dynamic_quote_single_override() {
        let base_rate = Decimal::new(150, 0);
        let check_in = NaiveDate::from_ymd_opt(2026, 12, 20).unwrap();
        let check_out = NaiveDate::from_ymd_opt(2026, 12, 23).unwrap(); // 3 nights

        let overrides = vec![PriceOverride {
            id: Uuid::new_v4(),
            listing_id: Uuid::new_v4(),
            start_date: NaiveDate::from_ymd_opt(2026, 12, 20).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 12, 26).unwrap(),
            nightly_rate: Decimal::new(300, 0), // Peak rate $300
            min_nights: 2,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }];

        let quote = calculate_dynamic_quote(base_rate, 1, &overrides, check_in, check_out).unwrap();
        let total_days = (check_out - check_in).num_days() as i32;

        let calc = BookingCalculator::with_subtotal(
            quote.effective_daily_rate,
            total_days,
            quote.subtotal,
        )
        .apply_discounts(None, None)
        .apply_promo(None)
        .apply_taxes(Decimal::new(15, 2))
        .finalize();

        assert_eq!(calc.sub_total_price, Decimal::new(900, 0)); // 3 * $300 = $900
        assert_eq!(calc.tax_value, Some(Decimal::new(13500, 2))); // 15% = $135.00
        assert_eq!(calc.fee_breakdown[0].amount, Decimal::new(4500, 2)); // 5% = $45.00
        assert_eq!(calc.total_price, Decimal::new(108000, 2)); // 900 + 135 + 45 = $1080.00
    }

    #[test]
    fn test_booking_calculator_with_dynamic_quote_partial_override() {
        let base_rate = Decimal::new(100, 0);
        let check_in = NaiveDate::from_ymd_opt(2026, 12, 18).unwrap();
        let check_out = NaiveDate::from_ymd_opt(2026, 12, 22).unwrap(); // 4 nights

        let overrides = vec![PriceOverride {
            id: Uuid::new_v4(),
            listing_id: Uuid::new_v4(),
            start_date: NaiveDate::from_ymd_opt(2026, 12, 20).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 12, 25).unwrap(),
            nightly_rate: Decimal::new(250, 0),
            min_nights: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }];

        let quote = calculate_dynamic_quote(base_rate, 1, &overrides, check_in, check_out).unwrap();
        let total_days = (check_out - check_in).num_days() as i32;

        let calc = BookingCalculator::with_subtotal(
            quote.effective_daily_rate,
            total_days,
            quote.subtotal,
        )
        .apply_discounts(None, None)
        .apply_promo(None)
        .apply_taxes(Decimal::new(15, 2))
        .finalize();

        // Subtotal: 100 + 100 + 250 + 250 = 700
        assert_eq!(calc.sub_total_price, Decimal::new(700, 0));
        assert_eq!(calc.tax_value, Some(Decimal::new(10500, 2))); // 15% of 700 = $105.00
        assert_eq!(calc.fee_breakdown[0].amount, Decimal::new(3500, 2)); // $35.00
        assert_eq!(calc.total_price, Decimal::new(84000, 2)); // 700 + 105 + 35 = $840.00
    }

    #[test]
    fn test_booking_calculator_with_exchange_rate_and_override() {
        let base_rate = Decimal::new(100, 0);
        let check_in = NaiveDate::from_ymd_opt(2026, 12, 20).unwrap();
        let check_out = NaiveDate::from_ymd_opt(2026, 12, 22).unwrap(); // 2 nights

        let conversion_rate = Decimal::new(150, 2); // 1.50 exchange rate (e.g. USD to EUR)

        let mut overrides = vec![PriceOverride {
            id: Uuid::new_v4(),
            listing_id: Uuid::new_v4(),
            start_date: NaiveDate::from_ymd_opt(2026, 12, 20).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 12, 25).unwrap(),
            nightly_rate: Decimal::new(200, 0), // $200 base currency
            min_nights: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }];

        // Apply exchange rate to overrides
        overrides.iter_mut().for_each(|o| {
            o.nightly_rate = (o.nightly_rate * conversion_rate).round_dp(2);
        });

        let converted_base_rate = (base_rate * conversion_rate).round_dp(2);

        let quote =
            calculate_dynamic_quote(converted_base_rate, 1, &overrides, check_in, check_out)
                .unwrap();
        let total_days = (check_out - check_in).num_days() as i32;

        let calc = BookingCalculator::with_subtotal(
            quote.effective_daily_rate,
            total_days,
            quote.subtotal,
        )
        .apply_discounts(None, None)
        .apply_promo(None)
        .apply_taxes(Decimal::new(15, 2))
        .finalize();

        // 2 nights @ $200 * 1.50 = $300 per night => Subtotal $600
        assert_eq!(calc.sub_total_price, Decimal::new(600, 0));
        assert_eq!(calc.tax_value, Some(Decimal::new(9000, 2))); // 15% of 600 = $90.00
        assert_eq!(calc.fee_breakdown[0].amount, Decimal::new(3000, 2)); // 5% of 600 = $30.00
        assert_eq!(calc.total_price, Decimal::new(72000, 2)); // 600 + 90 + 30 = $720.00
    }

    #[test]
    fn test_pricing_reconciliation_with_fees() {
        let daily_rate = Decimal::new(200, 0); // $200/night
        let cleaning_fee = Decimal::new(75, 0); // $75 cleaning fee
        let damage_deposit = Decimal::new(300, 0); // $300 deposit
        let tax_rate = Decimal::new(15, 2); // 15% JM GCT

        let calc = BookingCalculator::new(daily_rate, 4) // 4 nights = $800
            .apply_discounts(None, None)
            .with_fees(Some(cleaning_fee), Some(damage_deposit))
            .apply_promo(None)
            .apply_taxes(tax_rate)
            .finalize();

        let accommodation_net = calc.discounted_subtotal; // 800.00
        let taxable_base = accommodation_net + calc.cleaning_fee; // 875.00
        let tax_value = (taxable_base * tax_rate).round_dp(2); // 131.25
        let platform_fee = (accommodation_net * Decimal::new(5, 2)).round_dp(2); // 40.00

        assert_eq!(calc.sub_total_price, Decimal::new(800, 0));
        assert_eq!(calc.cleaning_fee, cleaning_fee);
        assert_eq!(calc.damage_deposit, damage_deposit);
        assert_eq!(calc.platform_fee, platform_fee);
        assert_eq!(calc.tax_value, Some(tax_value));
        assert_eq!(
            calc.total_price,
            taxable_base + tax_value + platform_fee + damage_deposit
        );
        // 875 + 131.25 + 40 + 300 = 1346.25
        assert_eq!(calc.total_price, Decimal::new(134625, 2));
    }

    #[test]
    fn test_pricing_reconciliation_invariant_randomized() {
        use rand::RngExt;
        let mut rng = rand::rng();

        for _ in 0..1000 {
            let daily_cents: i64 = rng.random_range(1000..500000); // $10.00 to $5000.00
            let daily_rate = Decimal::new(daily_cents, 2);
            let days: i32 = rng.random_range(1..60);

            let weekly_discount = if rng.random_bool(0.5) {
                Some(Decimal::new(rng.random_range(5..20), 0))
            } else {
                None
            };
            let monthly_discount = if rng.random_bool(0.5) {
                Some(Decimal::new(rng.random_range(10..35), 0))
            } else {
                None
            };

            let cleaning_fee = if rng.random_bool(0.7) {
                Some(Decimal::new(rng.random_range(0..25000), 2)) // $0 to $250
            } else {
                None
            };

            let damage_deposit = if rng.random_bool(0.7) {
                Some(Decimal::new(rng.random_range(0..100000), 2)) // $0 to $1000
            } else {
                None
            };

            let tax_rate = Decimal::new(15, 2);

            let guests_included: i32 = rng.random_range(1..=4);
            let number_of_persons: i32 = rng.random_range(1..=8);
            let extra_guest_fee = if rng.random_bool(0.6) {
                Decimal::new(rng.random_range(0..15000), 2)
            } else {
                Decimal::ZERO
            };

            let calc = BookingCalculator::new(daily_rate, days)
                .with_extra_guests(number_of_persons, guests_included, extra_guest_fee)
                .apply_discounts(monthly_discount, weekly_discount)
                .with_fees(cleaning_fee, damage_deposit)
                .apply_promo(None)
                .apply_taxes(tax_rate)
                .finalize();

            let accommodation_net = calc.discounted_subtotal;
            let taxable_base = accommodation_net + calc.cleaning_fee;
            let tax_value = calc.tax_value.expect("Tax value must be present");
            let platform_fee = calc.platform_fee;
            let damage_deposit_val = calc.damage_deposit;

            // Enforce §1.2 Reconciliation Invariant:
            // total_price = taxable_base + tax_value + platform_fee + damage_deposit
            assert_eq!(
                calc.total_price,
                taxable_base + tax_value + platform_fee + damage_deposit_val,
                "Reconciliation invariant failed for daily={}, days={}",
                daily_rate,
                days
            );
        }
    }

    #[test]
    fn test_extra_guest_fee_within_and_above_included() {
        let daily_rate = Decimal::new(200, 0); // $200/night
        let extra_fee = Decimal::new(35, 0); // $35/extra guest/night

        // Within guests_included (2 guests, 2 included) => 0 extra fee
        let within = BookingCalculator::new(daily_rate, 4)
            .with_extra_guests(2, 2, extra_fee)
            .apply_discounts(None, None)
            .apply_promo(None)
            .apply_taxes(Decimal::new(15, 2))
            .finalize();
        assert_eq!(within.extra_guests, 0);
        assert_eq!(within.extra_guest_fee_total, Decimal::ZERO);
        assert_eq!(within.sub_total_price, Decimal::new(800, 0));

        // Above guests_included (4 guests, 2 included => 2 extra * $35 * 4 nights = $280)
        let above = BookingCalculator::new(daily_rate, 4)
            .with_extra_guests(4, 2, extra_fee)
            .apply_discounts(None, None)
            .apply_promo(None)
            .apply_taxes(Decimal::new(15, 2))
            .finalize();
        assert_eq!(above.extra_guests, 2);
        assert_eq!(above.extra_guest_fee_per_night, Decimal::new(35, 0));
        assert_eq!(above.extra_guest_fee_total, Decimal::new(280, 0));
        assert_eq!(above.sub_total_price, Decimal::new(1080, 0)); // 800 + 280
        assert_eq!(above.discounted_subtotal, Decimal::new(1080, 0));
        assert_eq!(above.tax_value, Some(Decimal::new(16200, 2))); // 15% of 1080 = 162.00
        assert_eq!(above.platform_fee, Decimal::new(5400, 2)); // 5% of 1080 = 54.00
        assert_eq!(above.total_price, Decimal::new(129600, 2)); // 1080 + 162 + 54 = 1296.00
    }

    #[test]
    fn test_overall_stay_discounts_include_extra_guest_fee() {
        let daily_rate = Decimal::new(100, 0); // $100/night
        let extra_fee = Decimal::new(25, 0); // $25/extra guest/night

        // 7-night weekly stay with 2 extra guests:
        // Base = 700, Extra = 2 * 25 * 7 = 350 => sub_total_price = 1050.00
        // 10% weekly discount applies to overall stay (1050.00) => discount = 105.00, net = 945.00
        let weekly = BookingCalculator::new(daily_rate, 7)
            .with_extra_guests(4, 2, extra_fee)
            .apply_discounts(Some(Decimal::new(20, 0)), Some(Decimal::new(10, 0)))
            .apply_promo(None)
            .apply_taxes(Decimal::new(15, 2))
            .finalize();

        assert_eq!(weekly.extra_guest_fee_total, Decimal::new(350, 0));
        assert_eq!(weekly.sub_total_price, Decimal::new(1050, 0));
        assert_eq!(weekly.discount_value, Some(Decimal::new(105, 0)));
        assert_eq!(weekly.discounted_subtotal, Decimal::new(945, 0));

        // 28-night monthly stay with 1 extra guest:
        // Base = 2800, Extra = 1 * 25 * 28 = 700 => sub_total_price = 3500.00
        // 20% monthly discount applies to overall stay (3500.00) => discount = 700.00, net = 2800.00
        let monthly = BookingCalculator::new(daily_rate, 28)
            .with_extra_guests(3, 2, extra_fee)
            .apply_discounts(Some(Decimal::new(20, 0)), Some(Decimal::new(10, 0)))
            .apply_promo(None)
            .apply_taxes(Decimal::new(15, 2))
            .finalize();

        assert_eq!(monthly.extra_guest_fee_total, Decimal::new(700, 0));
        assert_eq!(monthly.sub_total_price, Decimal::new(3500, 0));
        assert_eq!(monthly.discount_value, Some(Decimal::new(700, 0)));
        assert_eq!(monthly.discounted_subtotal, Decimal::new(2800, 0));
    }

    #[test]
    fn test_validate_reservation_change_dates_pre_stay_and_mid_stay() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();

        // 1. Pre-stay: moving check-in earlier (still after today) -> allowed, expires on proposed_from
        let pre_from = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        let pre_to = NaiveDate::from_ymd_opt(2026, 11, 20).unwrap();
        let prop_from_earlier = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        assert_eq!(
            validate_reservation_change_dates(pre_from, pre_to, prop_from_earlier, pre_to, today),
            Ok(prop_from_earlier)
        );

        // 2. Pre-stay: moving check-in later -> allowed, expires on current_from
        let prop_from_later = NaiveDate::from_ymd_opt(2026, 11, 17).unwrap();
        assert_eq!(
            validate_reservation_change_dates(pre_from, pre_to, prop_from_later, pre_to, today),
            Ok(pre_from)
        );

        // 3. Pre-stay: moving check-in to today or past -> rejected (CheckInAlreadyArrived)
        assert_eq!(
            validate_reservation_change_dates(pre_from, pre_to, today, pre_to, today),
            Err(ReservationChangeValidationError::CheckInAlreadyArrived)
        );

        // 4. Mid-stay: current_from <= today < current_to
        let mid_from = NaiveDate::from_ymd_opt(2026, 11, 8).unwrap();
        let mid_to = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();

        // Mid-stay check-in modification -> rejected (CheckInAlreadyArrived)
        let new_from = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        assert_eq!(
            validate_reservation_change_dates(mid_from, mid_to, new_from, mid_to, today),
            Err(ReservationChangeValidationError::CheckInAlreadyArrived)
        );

        // Mid-stay check-out shortening (still > today) -> allowed, expires on proposed_to
        let short_to = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        assert_eq!(
            validate_reservation_change_dates(mid_from, mid_to, mid_from, short_to, today),
            Ok(short_to)
        );

        // Mid-stay check-out extension -> allowed, expires on current_to
        let ext_to = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        assert_eq!(
            validate_reservation_change_dates(mid_from, mid_to, mid_from, ext_to, today),
            Ok(mid_to)
        );

        // Mid-stay check-out set to today -> rejected (CheckOutNotInFuture)
        assert_eq!(
            validate_reservation_change_dates(mid_from, mid_to, mid_from, today, today),
            Err(ReservationChangeValidationError::CheckOutNotInFuture)
        );

        // 5. Stay already ended (current_to <= today) -> rejected (StayAlreadyEnded)
        let past_from = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        let past_to = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();
        assert_eq!(
            validate_reservation_change_dates(past_from, past_to, past_from, ext_to, today),
            Err(ReservationChangeValidationError::StayAlreadyEnded)
        );

        // 6. Invalid date range (proposed_to <= proposed_from)
        assert_eq!(
            validate_reservation_change_dates(pre_from, pre_to, pre_from, pre_from, today),
            Err(ReservationChangeValidationError::InvalidDateRange)
        );
    }
}

#[cfg(test)]
mod pricing_core_tests {
    use super::*;
    use crate::reference::SupportedCountry;
    use rust_decimal_macros::dec;
    use std::str::FromStr;
    use uuid::Uuid;

    const GOLDEN: &str = include_str!("../testdata/stay_pricing_golden.json");

    fn amount(value: &serde_json::Value) -> Decimal {
        let text = value.as_str().map(str::to_string).unwrap_or_default();
        match Decimal::from_str(&text) {
            Ok(amount) => amount,
            Err(e) => panic!("bad amount {value}: {e}"),
        }
    }

    fn optional(value: &serde_json::Value) -> Option<Decimal> {
        (!value.is_null()).then(|| amount(value))
    }

    fn date(value: &serde_json::Value) -> NaiveDate {
        match NaiveDate::parse_from_str(value.as_str().unwrap_or_default(), "%Y-%m-%d") {
            Ok(date) => date,
            Err(e) => panic!("bad date {value}: {e}"),
        }
    }

    fn whole(value: &serde_json::Value) -> i32 {
        match value.as_i64().and_then(|n| i32::try_from(n).ok()) {
            Some(n) => n,
            None => panic!("bad number {value}"),
        }
    }

    fn override_row(
        rate: Decimal,
        from: NaiveDate,
        to: NaiveDate,
        min_nights: i32,
    ) -> PriceOverride {
        PriceOverride {
            id: Uuid::new_v4(),
            listing_id: Uuid::new_v4(),
            start_date: from,
            end_date: to,
            nightly_rate: rate,
            min_nights,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn finish(calculator: BookingCalculator<Discounted>) -> BookingCalculator<Taxed> {
        calculator
            .apply_promo(None)
            .apply_taxes(SupportedCountry::tax_rate_for("Jamaica"))
            .finalize()
    }

    #[test]
    fn every_golden_case_prices_as_worked_out_by_hand() {
        let cases: Vec<serde_json::Value> = match serde_json::from_str(GOLDEN) {
            Ok(cases) => cases,
            Err(e) => panic!("golden file: {e}"),
        };
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap_or_default();
            let fields = &case["listing"];
            let overrides: Vec<PriceOverride> = case["overrides"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|row| {
                    override_row(
                        amount(&row["nightly_rate"]),
                        date(&row["start_date"]),
                        date(&row["end_date"]),
                        whole(&row["min_nights"]),
                    )
                })
                .collect();
            let priced = price_before_promo(StayPricingInput {
                base_nightly_rate: amount(&fields["price_per_night"]),
                minimum_stay: 1,
                overrides: &overrides,
                rate: optional(&case["rate"]).unwrap_or(Decimal::ONE),
                check_in: date(&case["check_in"]),
                check_out: date(&case["check_out"]),
                number_of_persons: whole(&case["guests"]),
                guests_included: whole(&fields["guests_included"]),
                extra_guest_fee: amount(&fields["extra_guest_fee"]),
                cleaning_fee: amount(&fields["cleaning_fee"]),
                damage_deposit: amount(&fields["damage_deposit"]),
                weekly_discount_percentage: optional(&fields["weekly"]),
                monthly_discount_percentage: optional(&fields["monthly"]),
            })
            .map(finish);
            let priced = match priced {
                Ok(priced) => priced,
                Err(e) => panic!("{name}: {e}"),
            };
            let expected = &case["expected"];
            assert_eq!(priced.total_days, whole(&expected["nights"]), "{name}");
            assert_eq!(
                priced.sub_total_price,
                amount(&expected["subtotal"]),
                "{name}"
            );
            assert_eq!(
                priced.discount_value.unwrap_or(Decimal::ZERO),
                amount(&expected["stay_discount"]),
                "{name}"
            );
            assert_eq!(
                priced.cleaning_fee,
                amount(&expected["cleaning_fee"]),
                "{name}"
            );
            assert_eq!(priced.tax_value, Some(amount(&expected["tax"])), "{name}");
            assert_eq!(
                priced.platform_fee,
                amount(&expected["platform_fee"]),
                "{name}"
            );
            assert_eq!(
                priced.damage_deposit,
                amount(&expected["damage_deposit"]),
                "{name}"
            );
            assert_eq!(priced.total_price, amount(&expected["total"]), "{name}");
        }
    }

    fn three_decimal_input(rate: Decimal, overrides: &[PriceOverride]) -> StayPricingInput<'_> {
        let day = |d| NaiveDate::from_ymd_opt(2030, 8, d).unwrap_or_default();
        StayPricingInput {
            base_nightly_rate: dec!(100.125),
            minimum_stay: 1,
            overrides,
            rate,
            check_in: day(1),
            check_out: day(3),
            number_of_persons: 1,
            guests_included: 1,
            extra_guest_fee: Decimal::ZERO,
            cleaning_fee: dec!(33.335),
            damage_deposit: dec!(10.005),
            weekly_discount_percentage: None,
            monthly_discount_percentage: None,
        }
    }

    /// The nightly rate always rounds, and other amounts only round when
    /// converted. Stored values have 2 places, so this can only show here.
    #[test]
    fn the_nightly_rate_always_rounds_and_other_amounts_only_when_converted() {
        let at_one = match price_before_promo(three_decimal_input(Decimal::ONE, &[])) {
            Ok(priced) => priced,
            Err(e) => panic!("{e}"),
        };
        assert_eq!(at_one.sub_total_price, dec!(200.24), "100.12 × 2 nights");
        assert_eq!(at_one.cleaning_fee, dec!(33.335));
        assert_eq!(at_one.damage_deposit, dec!(10.005));

        let converted = match price_before_promo(three_decimal_input(dec!(2), &[])) {
            Ok(priced) => priced,
            Err(e) => panic!("{e}"),
        };
        assert_eq!(converted.sub_total_price, dec!(400.50), "200.25 × 2 nights");
        assert_eq!(converted.cleaning_fee, dec!(66.67));
        assert_eq!(converted.damage_deposit, dec!(20.01));
    }

    #[test]
    fn an_override_rate_is_left_as_it_is_at_a_rate_of_one() {
        let day = |d| NaiveDate::from_ymd_opt(2030, 8, d).unwrap_or_default();
        let overrides = [override_row(dec!(150.125), day(1), day(3), 1)];
        let priced = match price_before_promo(three_decimal_input(Decimal::ONE, &overrides)) {
            Ok(priced) => priced,
            Err(e) => panic!("{e}"),
        };
        assert_eq!(priced.sub_total_price, dec!(300.250));
    }

    #[test]
    fn a_stay_below_the_override_minimum_is_refused() {
        let day = |d| NaiveDate::from_ymd_opt(2030, 8, d).unwrap_or_default();
        let overrides = [override_row(dec!(150), day(1), day(3), 5)];
        let refused = price_before_promo(three_decimal_input(Decimal::ONE, &overrides)).err();
        assert_eq!(
            refused,
            Some(PricingError::MinNightsNotMet {
                required: 5,
                provided: 2
            })
        );
    }

    #[test]
    fn the_stored_amount_limit_matches_the_column() {
        assert_eq!(MAX_STORED_AMOUNT, dec!(99999999.99));
    }

    #[test]
    fn amounts_too_large_for_the_booking_columns_are_flagged() {
        let day = |d| NaiveDate::from_ymd_opt(2030, 8, d).unwrap_or_default();
        let priced = |nightly: Decimal| {
            let input = StayPricingInput {
                base_nightly_rate: nightly,
                ..three_decimal_input(Decimal::ONE, &[])
            };
            assert_eq!(input.check_in, day(1));
            match price_before_promo(input) {
                Ok(calculator) => finish(calculator),
                Err(e) => panic!("{e}"),
            }
        };
        assert!(fits_booking_columns(&priced(dec!(100))));
        assert!(!fits_booking_columns(&priced(dec!(99999999))));
    }
}
