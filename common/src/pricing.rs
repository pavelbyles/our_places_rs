use crate::models::{DynamicPricingQuote, FeeItem, NightlyRateBreakdown, PriceOverride};
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

pub struct BaseRate;
pub struct Discounted;
pub struct Taxed;

pub struct BookingCalculator<State> {
    pub actual_daily_rate: Decimal,
    pub total_days: i32,
    pub sub_total_price: Decimal,
    pub discount_value: Option<Decimal>,
    pub discounted_subtotal: Decimal,
    pub tax_value: Option<Decimal>,
    pub fee_breakdown: Vec<FeeItem>,
    pub total_price: Decimal,
    pub state: State,
}

impl BookingCalculator<BaseRate> {
    pub fn new(actual_daily_rate: Decimal, total_days: i32) -> Self {
        let sub_total_price = actual_daily_rate * Decimal::from(total_days.max(0));
        Self {
            actual_daily_rate,
            total_days,
            sub_total_price,
            discount_value: None,
            discounted_subtotal: sub_total_price,
            tax_value: None,
            fee_breakdown: Vec::new(),
            total_price: Decimal::ZERO,
            state: BaseRate,
        }
    }

    pub fn with_subtotal(
        actual_daily_rate: Decimal,
        total_days: i32,
        sub_total_price: Decimal,
    ) -> Self {
        Self {
            actual_daily_rate,
            total_days,
            sub_total_price,
            discount_value: None,
            discounted_subtotal: sub_total_price,
            tax_value: None,
            fee_breakdown: Vec::new(),
            total_price: Decimal::ZERO,
            state: BaseRate,
        }
    }

    pub fn apply_discounts(
        mut self,
        monthly_pct: Option<Decimal>,
        weekly_pct: Option<Decimal>,
    ) -> BookingCalculator<Discounted> {
        if let (Some(pct), true) = (monthly_pct, self.total_days >= 28) {
            let discount = self.sub_total_price * (pct / Decimal::new(100, 0));
            self.discount_value = Some(discount);
            self.discounted_subtotal = self.sub_total_price - discount;
        } else if let (Some(pct), true) = (weekly_pct, self.total_days >= 7) {
            let discount = self.sub_total_price * (pct / Decimal::new(100, 0));
            self.discount_value = Some(discount);
            self.discounted_subtotal = self.sub_total_price - discount;
        }

        BookingCalculator {
            actual_daily_rate: self.actual_daily_rate,
            total_days: self.total_days,
            sub_total_price: self.sub_total_price,
            discount_value: self.discount_value,
            discounted_subtotal: self.discounted_subtotal,
            tax_value: self.tax_value,
            fee_breakdown: self.fee_breakdown,
            total_price: self.total_price,
            state: Discounted,
        }
    }
}

impl BookingCalculator<Discounted> {
    pub fn apply_taxes(mut self) -> BookingCalculator<Taxed> {
        let tax_value_decimal = self.discounted_subtotal * Decimal::new(10, 2);
        self.tax_value = Some(tax_value_decimal);

        BookingCalculator {
            actual_daily_rate: self.actual_daily_rate,
            total_days: self.total_days,
            sub_total_price: self.sub_total_price,
            discount_value: self.discount_value,
            discounted_subtotal: self.discounted_subtotal,
            tax_value: self.tax_value,
            fee_breakdown: self.fee_breakdown,
            total_price: self.total_price,
            state: Taxed,
        }
    }
}

impl BookingCalculator<Taxed> {
    pub fn finalize(mut self) -> Self {
        let platform_fee = self.discounted_subtotal * Decimal::new(5, 2);
        self.fee_breakdown.push(FeeItem {
            name: "Platform Fee".to_string(),
            amount: platform_fee,
        });

        let total_fees: Decimal = self.fee_breakdown.iter().map(|f| f.amount).sum();
        self.total_price =
            self.discounted_subtotal + self.tax_value.unwrap_or(Decimal::ZERO) + total_fees;

        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn test_calculator_basic_stay() {
        let daily_rate = Decimal::new(100, 0); // $100
        let calc = BookingCalculator::new(daily_rate, 3)
            .apply_discounts(None, None)
            .apply_taxes()
            .finalize();

        assert_eq!(calc.sub_total_price, Decimal::new(300, 0));
        assert_eq!(calc.discount_value, None);
        assert_eq!(calc.discounted_subtotal, Decimal::new(300, 0));
        assert_eq!(calc.tax_value, Some(Decimal::new(3000, 2))); // 10% = 30.00
        assert_eq!(calc.fee_breakdown.len(), 1);
        assert_eq!(calc.fee_breakdown[0].amount, Decimal::new(1500, 2)); // 5% platform fee = 15.00
        assert_eq!(calc.total_price, Decimal::new(34500, 2)); // 300 + 30 + 15 = 345.00
    }

    #[test]
    fn test_weekly_discount_application() {
        let daily_rate = Decimal::new(100, 0);
        let calc = BookingCalculator::new(daily_rate, 7)
            .apply_discounts(Some(Decimal::new(20, 0)), Some(Decimal::new(10, 0)))
            .apply_taxes()
            .finalize();

        assert_eq!(calc.sub_total_price, Decimal::new(700, 0));
        assert_eq!(calc.discount_value, Some(Decimal::new(70, 0)));
        assert_eq!(calc.discounted_subtotal, Decimal::new(630, 0));
    }

    #[test]
    fn test_monthly_discount_application() {
        let daily_rate = Decimal::new(100, 0);
        let calc = BookingCalculator::new(daily_rate, 30)
            .apply_discounts(Some(Decimal::new(25, 0)), Some(Decimal::new(10, 0)))
            .apply_taxes()
            .finalize();

        assert_eq!(calc.sub_total_price, Decimal::new(3000, 0));
        assert_eq!(calc.discount_value, Some(Decimal::new(750, 0)));
        assert_eq!(calc.discounted_subtotal, Decimal::new(2250, 0));
    }

    #[test]
    fn test_zero_and_negative_days_guard() {
        let daily_rate = Decimal::new(100, 0);
        let calc = BookingCalculator::new(daily_rate, -2)
            .apply_discounts(None, None)
            .apply_taxes()
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
        .apply_taxes()
        .finalize();

        assert_eq!(calc.sub_total_price, Decimal::new(900, 0)); // 3 * $300 = $900
        assert_eq!(calc.tax_value, Some(Decimal::new(9000, 2))); // 10% = $90.00
        assert_eq!(calc.fee_breakdown[0].amount, Decimal::new(4500, 2)); // 5% = $45.00
        assert_eq!(calc.total_price, Decimal::new(103500, 2)); // 900 + 90 + 45 = $1035.00
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
        .apply_taxes()
        .finalize();

        // Subtotal: 100 + 100 + 250 + 250 = 700
        assert_eq!(calc.sub_total_price, Decimal::new(700, 0));
        assert_eq!(calc.tax_value, Some(Decimal::new(7000, 2))); // $70.00
        assert_eq!(calc.fee_breakdown[0].amount, Decimal::new(3500, 2)); // $35.00
        assert_eq!(calc.total_price, Decimal::new(80500, 2)); // $805.00
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
        .apply_taxes()
        .finalize();

        // 2 nights @ $200 * 1.50 = $300 per night => Subtotal $600
        assert_eq!(calc.sub_total_price, Decimal::new(600, 0));
        assert_eq!(calc.total_price, Decimal::new(69000, 2)); // 600 + 60 + 30 = $690.00
    }
}
