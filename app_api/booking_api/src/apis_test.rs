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
}
