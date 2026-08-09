#[cfg(test)]
mod tests {
    use crate::apis::BookingCalculator;
    use rust_decimal::Decimal;

    #[test]
    fn test_booking_calculator_state_machine() {
        let daily_rate = Decimal::new(100, 0);
        let total_days = 10;

        // BaseRate
        let calc = BookingCalculator::new(daily_rate, total_days);
        assert_eq!(calc.sub_total_price, Decimal::new(1000, 0));

        // BaseRate -> Discounted (Weekly discount 10%)
        let weekly_pct = Some(Decimal::new(10, 0));
        let discounted = calc.apply_discounts(None, weekly_pct);
        assert_eq!(discounted.discount_value.unwrap(), Decimal::new(100, 0));
        assert_eq!(discounted.discounted_subtotal, Decimal::new(900, 0));

        // Discounted -> Taxed (10% tax)
        let taxed = discounted.apply_taxes();
        assert_eq!(taxed.tax_value.unwrap(), Decimal::new(90, 0));

        // Taxed -> Final
        let final_calc = taxed.finalize();
        // Platform fee is 5% of discounted subtotal (900 * 0.05 = 45)
        assert_eq!(final_calc.fee_breakdown.len(), 1);
        assert_eq!(final_calc.fee_breakdown[0].amount, Decimal::new(45, 0));

        // Total price = 900 (subtotal) + 90 (tax) + 45 (fee) = 1035
        assert_eq!(final_calc.total_price, Decimal::new(1035, 0));
    }

    #[test]
    fn test_booking_calculator_with_seasonal_override() {
        use chrono::NaiveDate;
        use common::models::PriceOverride;
        use common::pricing::calculate_dynamic_quote;
        use uuid::Uuid;

        let base_rate = Decimal::new(200, 0); // $200
        let check_in = NaiveDate::from_ymd_opt(2026, 12, 20).unwrap();
        let check_out = NaiveDate::from_ymd_opt(2026, 12, 25).unwrap(); // 5 nights

        let overrides = vec![PriceOverride {
            id: Uuid::new_v4(),
            listing_id: Uuid::new_v4(),
            start_date: NaiveDate::from_ymd_opt(2026, 12, 22).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 12, 30).unwrap(),
            nightly_rate: Decimal::new(350, 0), // Peak rate $350
            min_nights: 2,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }];

        let quote = calculate_dynamic_quote(base_rate, 1, &overrides, check_in, check_out).unwrap();

        // 2 nights @ $200 (Dec 20, 21) + 3 nights @ $350 (Dec 22, 23, 24) = 400 + 1050 = 1450
        assert_eq!(quote.subtotal, Decimal::new(1450, 0));

        let calc = BookingCalculator::with_subtotal(quote.effective_daily_rate, 5, quote.subtotal)
            .apply_discounts(None, None)
            .apply_taxes()
            .finalize();

        assert_eq!(calc.sub_total_price, Decimal::new(1450, 0));
        assert_eq!(calc.tax_value, Some(Decimal::new(14500, 2))); // $145.00
        assert_eq!(calc.fee_breakdown[0].amount, Decimal::new(7250, 2)); // $72.50
        assert_eq!(calc.total_price, Decimal::new(166750, 2)); // 1450 + 145 + 72.50 = 1667.50
    }

    #[test]
    fn test_seasonal_min_nights_enforcement() {
        use chrono::NaiveDate;
        use common::models::PriceOverride;
        use common::pricing::{PricingError, calculate_dynamic_quote};
        use uuid::Uuid;

        let base_rate = Decimal::new(150, 0);
        let check_in = NaiveDate::from_ymd_opt(2026, 12, 24).unwrap();
        let check_out = NaiveDate::from_ymd_opt(2026, 12, 26).unwrap(); // 2 nights

        let overrides = vec![PriceOverride {
            id: Uuid::new_v4(),
            listing_id: Uuid::new_v4(),
            start_date: NaiveDate::from_ymd_opt(2026, 12, 20).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
            nightly_rate: Decimal::new(400, 0),
            min_nights: 3, // Requires 3 nights minimum for Christmas week
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }];

        let result = calculate_dynamic_quote(base_rate, 1, &overrides, check_in, check_out);
        assert_eq!(
            result,
            Err(PricingError::MinNightsNotMet {
                required: 3,
                provided: 2
            })
        );
    }
}
