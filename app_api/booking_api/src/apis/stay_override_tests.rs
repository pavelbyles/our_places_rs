//! Custom quote pricing (#105): a host's stay price replaces the
//! accommodation subtotal, at a locked or current rate, and drops any promo.

use super::stay_pricing::apply_stay_override;
use common::booking_request::RequestRefusal;
use common::pricing::{BookingCalculator, Discounted};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn discounted() -> BookingCalculator<Discounted> {
    BookingCalculator::with_subtotal(dec!(100.00), 7, dec!(700.00))
        .apply_discounts(None, Some(dec!(10)))
        .with_fees(Some(dec!(80.00)), Some(dec!(300.00)))
}

mod pure {
    use super::*;

    #[test]
    fn a_valid_price_replaces_the_subtotal_at_the_rate() {
        let quoted = apply_stay_override(discounted(), dec!(30000), dec!(0.0064));
        match quoted {
            Ok(calc) => {
                assert_eq!(calc.discounted_subtotal, dec!(192.00));
                assert_eq!(calc.discount_value, None);
                assert_eq!(calc.cleaning_fee, dec!(80.00));
            }
            Err(refused) => panic!("refused: {refused:?}"),
        }
    }

    #[test]
    fn zero_negative_and_three_decimal_prices_are_refused() {
        for price in [dec!(0), dec!(-1), dec!(-0.01), dec!(1.234)] {
            assert_eq!(
                apply_stay_override(discounted(), price, Decimal::ONE).err(),
                Some(RequestRefusal::InvalidQuotePrice),
                "{price}"
            );
        }
        // Trailing zeros beyond 2 places are still a 2-place amount.
        assert!(apply_stay_override(discounted(), dec!(10.500), Decimal::ONE).is_ok());
    }

    #[test]
    fn a_price_that_converts_to_zero_is_refused() {
        assert_eq!(
            apply_stay_override(discounted(), dec!(0.01), dec!(0.0064)).err(),
            Some(RequestRefusal::InvalidQuotePrice)
        );
    }
}

mod db {
    use super::super::authz_tests::{listing, settings, user};
    use super::super::stay_pricing::{
        OnRefusal, PromoRequest, StayRequest, load_listing, price_stay,
    };
    use super::super::stay_pricing_pin_tests::{amount, seed_case, test_db};
    use super::*;
    use api_core::error::ApiError;
    use chrono::NaiveDate;
    use db_core::error::DbError;
    use db_core::models::UserRole;
    use uuid::Uuid;

    const GOLDEN: &str = include_str!("../../../../common/testdata/custom_quote_golden.json");

    fn cases() -> Vec<serde_json::Value> {
        serde_json::from_str(GOLDEN).expect("golden file")
    }

    fn date(value: &serde_json::Value) -> NaiveDate {
        NaiveDate::parse_from_str(value.as_str().expect("date"), "%Y-%m-%d").expect("a date")
    }

    fn rate_of(case: &serde_json::Value) -> Decimal {
        if case["rate"].is_null() {
            Decimal::ONE
        } else {
            amount(&case["rate"])
        }
    }

    async fn quote(
        pool: &sqlx::PgPool,
        villa: Uuid,
        case: &serde_json::Value,
        locked_rate: Option<Decimal>,
        stay_price: Decimal,
        promo: Option<PromoRequest<'_>>,
    ) -> Result<super::super::stay_pricing::PricedStay, ApiError> {
        let details = load_listing(pool, villa).await.expect("listing");
        price_stay(
            pool,
            StayRequest {
                listing: &details,
                check_in: date(&case["check_in"]),
                check_out: date(&case["check_out"]),
                number_of_persons: case["guests"].as_i64().expect("guests") as i32,
                currency: case["currency"].as_str().expect("currency"),
                promo,
                max_stay_nights: settings().application.max_stay_nights,
                today: NaiveDate::from_ymd_opt(2030, 1, 1).expect("today"),
                locked_rate,
                stay_override: Some(stay_price),
            },
        )
        .await
    }

    #[tokio::test]
    async fn every_custom_quote_golden_case_prices_through_price_stay() {
        for case in cases() {
            let name = case["name"].as_str().expect("name");
            let test_db = test_db();
            let pool = test_db.get_pool().await;
            let villa = seed_case(&pool, &case).await;
            let priced = quote(
                &pool,
                villa,
                &case,
                Some(rate_of(&case)),
                amount(&case["custom_stay_price"]),
                None,
            )
            .await
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let calc = &priced.calculator;
            let expected = &case["expected"];
            assert_eq!(
                calc.actual_daily_rate,
                amount(&expected["daily_rate"]),
                "{name}"
            );
            assert_eq!(
                calc.sub_total_price,
                amount(&expected["subtotal"]),
                "{name}"
            );
            assert_eq!(
                calc.cleaning_fee,
                amount(&expected["cleaning_fee"]),
                "{name}"
            );
            assert_eq!(calc.tax_value, Some(amount(&expected["tax"])), "{name}");
            assert_eq!(
                calc.platform_fee,
                amount(&expected["platform_fee"]),
                "{name}"
            );
            assert_eq!(
                calc.damage_deposit,
                amount(&expected["damage_deposit"]),
                "{name}"
            );
            assert_eq!(calc.total_price, amount(&expected["total"]), "{name}");
            assert_eq!(priced.conversion_rate, rate_of(&case), "{name}");
            assert!(priced.promo.is_none(), "{name}");
        }
    }

    #[tokio::test]
    async fn a_locked_rate_ignores_a_newer_rate_version() {
        let case = cases().remove(1);
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let villa = seed_case(&pool, &case).await;
        sqlx::query(
            "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
             VALUES ('USD', 'EUR', 0.0070, clock_timestamp())",
        )
        .execute(&pool)
        .await
        .expect("newer rate");

        let locked = quote(&pool, villa, &case, Some(dec!(0.0064)), dec!(30000), None)
            .await
            .expect("locked quote");
        assert_eq!(locked.calculator.total_price, dec!(267.20));
        let current = quote(&pool, villa, &case, None, dec!(30000), None)
            .await
            .expect("current quote");
        assert_eq!(current.conversion_rate, dec!(0.0070));
        assert_ne!(current.calculator.total_price, dec!(267.20));
    }

    #[tokio::test]
    async fn a_custom_quote_never_evaluates_a_promo_code() {
        let case = cases().remove(0);
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let villa = seed_case(&pool, &case).await;
        // An unknown code with OnRefusal::Fail would refuse an ordinary stay.
        let promo = PromoRequest {
            raw_code: "NO-SUCH-CODE",
            guest_id: Uuid::now_v7(),
            on_refusal: OnRefusal::Fail,
        };
        let priced = quote(&pool, villa, &case, None, dec!(650.00), Some(promo))
            .await
            .expect("quote ignores the promo");
        assert!(priced.promo.is_none());
        assert!(priced.promo_refusal.is_none());
        assert_eq!(priced.calculator.total_price, dec!(1172.00));
    }

    #[tokio::test]
    async fn invalid_custom_prices_are_400_invalid_quote_price() {
        let case = cases().remove(2);
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let villa = seed_case(&pool, &case).await;
        for price in [dec!(0), dec!(-5), dec!(1.234), dec!(99999999999.99)] {
            match quote(&pool, villa, &case, None, price, None).await {
                Err(ApiError::Database(DbError::RequestRefused(
                    RequestRefusal::InvalidQuotePrice,
                ))) => {}
                Err(other) => panic!("{price}: expected INVALID_QUOTE_PRICE, got {other:?}"),
                Ok(_) => panic!("{price}: expected INVALID_QUOTE_PRICE, got a price"),
            }
        }
    }

    #[tokio::test]
    async fn ordinary_stays_still_price_with_the_current_rate_and_no_override() {
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let host = user(&pool, UserRole::Host).await;
        let villa = listing(&pool, host).await;
        let details = load_listing(&pool, villa).await.expect("listing");
        let priced = price_stay(
            &pool,
            StayRequest {
                listing: &details,
                check_in: NaiveDate::from_ymd_opt(2030, 8, 1).expect("date"),
                check_out: NaiveDate::from_ymd_opt(2030, 8, 4).expect("date"),
                number_of_persons: 1,
                currency: "USD",
                promo: None,
                max_stay_nights: 3650,
                today: NaiveDate::from_ymd_opt(2030, 1, 1).expect("today"),
                locked_rate: None,
                stay_override: None,
            },
        )
        .await
        .expect("price");
        assert_eq!(priced.conversion_rate, Decimal::ONE);
        assert!(priced.calculator.sub_total_price > Decimal::ZERO);
    }
}
