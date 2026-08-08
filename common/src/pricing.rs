use crate::models::FeeItem;
use rust_decimal::Decimal;

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

        // Weekly discount 10% on $700 subtotal = $70
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

        // Monthly discount 25% on $3000 subtotal = $750
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
}
