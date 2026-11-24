use chrono::{DateTime, Duration, NaiveDate, Utc};
use common::cancellation::{
    CancellationError, CancellationPolicy, CancellationTiming, Firm, Flexible, Limited, Moderate,
    RefundCalculator, RefundSchedule, RefundableAmounts, Strict, quote_refund,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn sample_amounts() -> RefundableAmounts {
    // accommodation_net = 1000.00, cleaning = 120.00, deposit = 300.00, tax_rate = 0.15 (15%)
    // taxable_base = 1120.00, tax_value = 168.00, platform_fee = 50.00 (5% of 1000)
    // total_price = 1120.00 + 168.00 + 50.00 + 300.00 = 1638.00
    RefundableAmounts::new(
        dec!(1000.00),
        dec!(120.00),
        dec!(300.00),
        dec!(50.00),
        dec!(0.1500),
        dec!(1638.00),
    )
}

fn jamaica_check_in_utc(check_in: NaiveDate) -> DateTime<Utc> {
    // Jamaica standard check-in is 15:00 UTC-5 => 20:00 UTC
    let naive = check_in.and_hms_opt(20, 0, 0).unwrap();
    DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)
}

#[test]
fn test_flexible_policy_schedules() {
    let check_in = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
    let check_in_utc = jamaica_check_in_utc(check_in);
    let booked_at = check_in_utc - Duration::days(30);

    // >= 24h before check-in => 100% refund
    let cancelled_at_25h = check_in_utc - Duration::hours(25);
    let quote = quote_refund(
        CancellationPolicy::Flexible,
        sample_amounts(),
        booked_at,
        check_in,
        cancelled_at_25h,
    )
    .unwrap();
    assert_eq!(quote.policy_pct, dec!(1.00));
    assert_eq!(quote.accommodation_refund, dec!(1000.00));
    assert_eq!(quote.cleaning_refund, dec!(120.00));
    assert_eq!(quote.deposit_refund, dec!(300.00));
    assert_eq!(quote.tax_refund, dec!(168.00));
    assert_eq!(quote.platform_fee_refund, dec!(50.00));
    assert_eq!(quote.refund_total, dec!(1638.00));
    assert_eq!(quote.retained_total, dec!(0.00));
    assert_eq!(quote.refund_total + quote.retained_total, dec!(1638.00));

    // Exactly 24h before check-in => 100% refund (lower bound inclusive)
    let cancelled_at_24h = check_in_utc - Duration::hours(24);
    let quote_24h = quote_refund(
        CancellationPolicy::Flexible,
        sample_amounts(),
        booked_at,
        check_in,
        cancelled_at_24h,
    )
    .unwrap();
    assert_eq!(quote_24h.policy_pct, dec!(1.00));
    assert_eq!(quote_24h.refund_total, dec!(1638.00));

    // < 24h before check-in => 0% accommodation refund (100% host payout)
    let cancelled_at_23h = check_in_utc - Duration::hours(23);
    let quote_23h = quote_refund(
        CancellationPolicy::Flexible,
        sample_amounts(),
        booked_at,
        check_in,
        cancelled_at_23h,
    )
    .unwrap();
    assert_eq!(quote_23h.policy_pct, dec!(0.00));
    assert_eq!(quote_23h.accommodation_refund, dec!(0.00));
    assert_eq!(quote_23h.cleaning_refund, dec!(120.00));
    assert_eq!(quote_23h.deposit_refund, dec!(300.00));
    // Tax pro-rata on cleaning refund: 120 * 0.15 = 18.00
    assert_eq!(quote_23h.tax_refund, dec!(18.00));
    assert_eq!(quote_23h.platform_fee_refund, dec!(0.00));
    assert_eq!(quote_23h.refund_total, dec!(438.00));
    assert_eq!(quote_23h.retained_total, dec!(1200.00));
    assert_eq!(
        quote_23h.refund_total + quote_23h.retained_total,
        dec!(1638.00)
    );
}

#[test]
fn test_moderate_policy_schedules() {
    let check_in = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
    let check_in_utc = jamaica_check_in_utc(check_in);
    let booked_at = check_in_utc - Duration::days(30);

    // >= 5 days before check-in => 100%
    let cancelled_at_5d = check_in_utc - Duration::days(5);
    let quote_5d = quote_refund(
        CancellationPolicy::Moderate,
        sample_amounts(),
        booked_at,
        check_in,
        cancelled_at_5d,
    )
    .unwrap();
    assert_eq!(quote_5d.policy_pct, dec!(1.00));
    assert_eq!(quote_5d.refund_total, dec!(1638.00));

    // 5 days to 24 hours => 50%
    let cancelled_at_2d = check_in_utc - Duration::days(2);
    let quote_2d = quote_refund(
        CancellationPolicy::Moderate,
        sample_amounts(),
        booked_at,
        check_in,
        cancelled_at_2d,
    )
    .unwrap();
    assert_eq!(quote_2d.policy_pct, dec!(0.50));
    assert_eq!(quote_2d.accommodation_refund, dec!(500.00));
    assert_eq!(quote_2d.cleaning_refund, dec!(120.00));
    assert_eq!(quote_2d.deposit_refund, dec!(300.00));
    // Tax on (500 + 120) * 0.15 = 620 * 0.15 = 93.00
    assert_eq!(quote_2d.tax_refund, dec!(93.00));
    assert_eq!(quote_2d.platform_fee_refund, dec!(0.00));
    assert_eq!(quote_2d.refund_total, dec!(1013.00));
    assert_eq!(quote_2d.retained_total, dec!(625.00));
    assert_eq!(
        quote_2d.refund_total + quote_2d.retained_total,
        dec!(1638.00)
    );

    // < 24 hours => 0%
    let cancelled_at_12h = check_in_utc - Duration::hours(12);
    let quote_12h = quote_refund(
        CancellationPolicy::Moderate,
        sample_amounts(),
        booked_at,
        check_in,
        cancelled_at_12h,
    )
    .unwrap();
    assert_eq!(quote_12h.policy_pct, dec!(0.00));
    assert_eq!(quote_12h.refund_total, dec!(438.00));
}

#[test]
fn test_strict_table_driven_boundaries_and_grace() {
    let check_in = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
    let check_in_utc = jamaica_check_in_utc(check_in);

    // Table-driven days-out boundaries: 0, 6, 7, 13, 14, 29, 30, 31 (booked well ahead, outside 48h grace)
    let booked_at_far = check_in_utc - Duration::days(60);

    let boundaries = [
        (0, dec!(0.00)),  // day of check-in (< 7d)
        (6, dec!(0.00)),  // 6 days out (< 7d)
        (7, dec!(0.50)),  // 7 days out (>= 7d)
        (13, dec!(0.50)), // 13 days out (>= 7d)
        (14, dec!(0.50)), // 14 days out (>= 7d, outside grace)
        (29, dec!(0.50)), // 29 days out (>= 7d)
        (30, dec!(0.50)), // 30 days out (>= 7d)
        (31, dec!(0.50)), // 31 days out (>= 7d)
    ];

    for (days_out, expected_pct) in boundaries {
        let cancelled_at = if days_out == 0 {
            check_in_utc - Duration::hours(5)
        } else {
            check_in_utc - Duration::days(days_out)
        };

        let quote = quote_refund(
            CancellationPolicy::Strict,
            sample_amounts(),
            booked_at_far,
            check_in,
            cancelled_at,
        )
        .unwrap();

        assert_eq!(
            quote.policy_pct, expected_pct,
            "Strict policy at {} days out failed",
            days_out
        );
        assert_eq!(
            quote.refund_total + quote.retained_total,
            dec!(1638.00),
            "Invariant failed for Strict at {} days out",
            days_out
        );
    }

    // Strict 48-hour grace period matrix: 47h/49h x booked 13/14 days ahead
    // Case 1: booked 14 days ahead, cancelled 47h after booking => 100%
    let booked_14d = check_in_utc - Duration::days(14);
    let cancel_47h_14d = booked_14d + Duration::hours(47);
    let quote_47h_14d = quote_refund(
        CancellationPolicy::Strict,
        sample_amounts(),
        booked_14d,
        check_in,
        cancel_47h_14d,
    )
    .unwrap();
    assert_eq!(quote_47h_14d.policy_pct, dec!(1.00));

    // Case 2: booked 14 days ahead, cancelled 49h after booking => outside grace, but check days-out
    // 14 days ahead - 49h = 11 days 23h before check-in >= 7d => 50%
    let cancel_49h_14d = booked_14d + Duration::hours(49);
    let quote_49h_14d = quote_refund(
        CancellationPolicy::Strict,
        sample_amounts(),
        booked_14d,
        check_in,
        cancel_49h_14d,
    )
    .unwrap();
    assert_eq!(quote_49h_14d.policy_pct, dec!(0.50));

    // Case 3: booked 13 days ahead, cancelled 47h after booking => not booked >= 14d, so no grace
    // 13 days ahead - 47h = 11 days 1h before check-in >= 7d => 50%
    let booked_13d = check_in_utc - Duration::days(13);
    let cancel_47h_13d = booked_13d + Duration::hours(47);
    let quote_47h_13d = quote_refund(
        CancellationPolicy::Strict,
        sample_amounts(),
        booked_13d,
        check_in,
        cancel_47h_13d,
    )
    .unwrap();
    assert_eq!(quote_47h_13d.policy_pct, dec!(0.50));

    // Case 4: booked 13 days ahead, cancelled 49h after booking => no grace, >= 7d => 50%
    let cancel_49h_13d = booked_13d + Duration::hours(49);
    let quote_49h_13d = quote_refund(
        CancellationPolicy::Strict,
        sample_amounts(),
        booked_13d,
        check_in,
        cancel_49h_13d,
    )
    .unwrap();
    assert_eq!(quote_49h_13d.policy_pct, dec!(0.50));

    // Case 5: booked 6 days ahead, cancelled 20h after booking => no grace (not >= 14d), < 7d => 0%
    let booked_6d = check_in_utc - Duration::days(6);
    let cancel_20h_6d = booked_6d + Duration::hours(20);
    let quote_20h_6d = quote_refund(
        CancellationPolicy::Strict,
        sample_amounts(),
        booked_6d,
        check_in,
        cancel_20h_6d,
    )
    .unwrap();
    assert_eq!(quote_20h_6d.policy_pct, dec!(0.00));
}

#[test]
fn test_firm_table_driven_boundaries_and_grace() {
    let check_in = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
    let check_in_utc = jamaica_check_in_utc(check_in);
    let booked_at_far = check_in_utc - Duration::days(90);

    // Boundaries: 0, 6, 7, 13, 14, 29, 30, 31
    let boundaries = [
        (0, dec!(0.00)),  // < 7d
        (6, dec!(0.00)),  // < 7d
        (7, dec!(0.50)),  // 7 to 30d
        (13, dec!(0.50)), // 7 to 30d
        (14, dec!(0.50)), // 7 to 30d
        (29, dec!(0.50)), // 7 to 30d
        (30, dec!(1.00)), // >= 30d => 100%
        (31, dec!(1.00)), // >= 30d => 100%
    ];

    for (days_out, expected_pct) in boundaries {
        let cancelled_at = if days_out == 0 {
            check_in_utc - Duration::hours(5)
        } else {
            check_in_utc - Duration::days(days_out)
        };

        let quote = quote_refund(
            CancellationPolicy::Firm,
            sample_amounts(),
            booked_at_far,
            check_in,
            cancelled_at,
        )
        .unwrap();

        assert_eq!(
            quote.policy_pct, expected_pct,
            "Firm policy at {} days out failed",
            days_out
        );
        assert_eq!(
            quote.refund_total + quote.retained_total,
            dec!(1638.00),
            "Invariant failed for Firm at {} days out",
            days_out
        );
    }

    // Firm also has 48h grace if booked >= 14d out (even between 7 and 30 days)
    let booked_20d = check_in_utc - Duration::days(20);
    let cancel_40h = booked_20d + Duration::hours(40);
    let quote_grace = quote_refund(
        CancellationPolicy::Firm,
        sample_amounts(),
        booked_20d,
        check_in,
        cancel_40h,
    )
    .unwrap();
    assert_eq!(quote_grace.policy_pct, dec!(1.00));
}

#[test]
fn test_limited_policy_schedules() {
    let check_in = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
    let check_in_utc = jamaica_check_in_utc(check_in);
    let booked_at = check_in_utc - Duration::days(60);

    // >= 14d => 100%
    let quote_14d = quote_refund(
        CancellationPolicy::Limited,
        sample_amounts(),
        booked_at,
        check_in,
        check_in_utc - Duration::days(14),
    )
    .unwrap();
    assert_eq!(quote_14d.policy_pct, dec!(1.00));

    // 7d to 14d => 50%
    let quote_10d = quote_refund(
        CancellationPolicy::Limited,
        sample_amounts(),
        booked_at,
        check_in,
        check_in_utc - Duration::days(10),
    )
    .unwrap();
    assert_eq!(quote_10d.policy_pct, dec!(0.50));

    // < 7d => 0%
    let quote_5d = quote_refund(
        CancellationPolicy::Limited,
        sample_amounts(),
        booked_at,
        check_in,
        check_in_utc - Duration::days(5),
    )
    .unwrap();
    assert_eq!(quote_5d.policy_pct, dec!(0.00));
}

#[test]
fn test_cancelled_at_or_after_check_in_returns_error() {
    let check_in = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
    let check_in_utc = jamaica_check_in_utc(check_in);
    let booked_at = check_in_utc - Duration::days(10);

    // Exactly at check-in time => Err(AlreadyCheckedIn)
    let res_exact = quote_refund(
        CancellationPolicy::Flexible,
        sample_amounts(),
        booked_at,
        check_in,
        check_in_utc,
    );
    assert!(matches!(
        res_exact,
        Err(CancellationError::AlreadyCheckedIn { .. })
    ));

    // After check-in time => Err(AlreadyCheckedIn)
    let res_after = quote_refund(
        CancellationPolicy::Flexible,
        sample_amounts(),
        booked_at,
        check_in,
        check_in_utc + Duration::hours(2),
    );
    assert!(matches!(
        res_after,
        Err(CancellationError::AlreadyCheckedIn { .. })
    ));
}

#[test]
fn test_cancelled_before_booked_at_returns_error() {
    let check_in = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
    let check_in_utc = jamaica_check_in_utc(check_in);
    let booked_at = check_in_utc - Duration::days(10);

    let res = quote_refund(
        CancellationPolicy::Flexible,
        sample_amounts(),
        booked_at,
        check_in,
        booked_at - Duration::hours(1),
    );
    assert!(matches!(
        res,
        Err(CancellationError::CancelledBeforeBooking { .. })
    ));
}

#[test]
fn test_compile_time_typestate_calculator() {
    let check_in = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
    let check_in_utc = jamaica_check_in_utc(check_in);
    let booked_at = check_in_utc - Duration::days(40);
    let cancelled_at = check_in_utc - Duration::days(35);

    // Pending -> at() -> Timed -> quote() -> RefundQuote
    let calc = RefundCalculator::<Firm>::new(sample_amounts());
    let timed = calc.at(booked_at, check_in, cancelled_at).unwrap();
    assert_eq!(timed.timing().days_before_check_in, 35);
    let quote = timed.quote();
    assert_eq!(quote.policy_pct, dec!(1.00));
    assert!(quote.is_full_refund());
    assert!(!quote.is_zero_refund());

    // Test other compile-time typestate policy markers
    let timed_flex = RefundCalculator::<Flexible>::new(sample_amounts())
        .at(booked_at, check_in, cancelled_at)
        .unwrap();
    assert_eq!(timed_flex.quote().policy_pct, dec!(1.00));

    let timed_mod = RefundCalculator::<Moderate>::new(sample_amounts())
        .at(booked_at, check_in, cancelled_at)
        .unwrap();
    assert_eq!(timed_mod.quote().policy_pct, dec!(1.00));

    let timed_strict = RefundCalculator::<Strict>::new(sample_amounts())
        .at(booked_at, check_in, cancelled_at)
        .unwrap();
    assert_eq!(timed_strict.quote().policy_pct, dec!(0.50));

    let timed_limited = RefundCalculator::<Limited>::new(sample_amounts())
        .at(booked_at, check_in, cancelled_at)
        .unwrap();
    assert_eq!(timed_limited.quote().policy_pct, dec!(1.00));
}

#[test]
fn test_refund_schedule_direct_trait_calls() {
    let t = CancellationTiming {
        days_before_check_in: 10,
        hours_before_check_in: 240,
        hours_since_booking: 50,
        days_booked_ahead: 20,
    };

    assert_eq!(Flexible::accommodation_refund_pct(&t), Decimal::ONE);
    assert_eq!(Moderate::accommodation_refund_pct(&t), Decimal::ONE);
    assert_eq!(Strict::accommodation_refund_pct(&t), Decimal::new(50, 2));
    assert_eq!(Firm::accommodation_refund_pct(&t), Decimal::new(50, 2));
    assert_eq!(Limited::accommodation_refund_pct(&t), Decimal::new(50, 2));
}

#[test]
fn test_cancellation_policy_enum_display_and_parsing() {
    use std::str::FromStr;

    let policies = [
        (CancellationPolicy::Flexible, "flexible"),
        (CancellationPolicy::Moderate, "moderate"),
        (CancellationPolicy::Strict, "strict"),
        (CancellationPolicy::Firm, "firm"),
        (CancellationPolicy::Limited, "limited"),
    ];

    for (policy, name) in policies {
        assert_eq!(policy.as_str(), name);
        assert_eq!(policy.to_string(), name);
        assert_eq!(CancellationPolicy::from_str(name).unwrap(), policy);
        assert_eq!(
            CancellationPolicy::from_str(&name.to_uppercase()).unwrap(),
            policy
        );
    }

    assert!(matches!(
        CancellationPolicy::from_str("nonexistent"),
        Err(CancellationError::InvalidPolicy(_))
    ));
}

#[test]
fn test_reconciliation_invariant_across_various_fees_and_decimals() {
    let test_cases = [
        // accommodation, cleaning, deposit, platform_fee, tax_rate
        (dec!(0.00), dec!(0.00), dec!(0.00), dec!(0.00), dec!(0.15)),
        (dec!(100.00), dec!(0.00), dec!(0.00), dec!(5.00), dec!(0.15)),
        (
            dec!(733.33),
            dec!(85.50),
            dec!(250.00),
            dec!(36.67),
            dec!(0.15),
        ),
        (
            dec!(1999.99),
            dec!(150.00),
            dec!(500.00),
            dec!(100.00),
            dec!(0.10),
        ),
        (
            dec!(50.25),
            dec!(25.10),
            dec!(75.00),
            dec!(2.51),
            dec!(0.125),
        ),
    ];

    let check_in = NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();
    let check_in_utc = jamaica_check_in_utc(check_in);
    let booked_at = check_in_utc - Duration::days(45);

    // Test across all 5 policies and multiple cancellation timing intervals
    let cancel_times = [
        check_in_utc - Duration::days(35),  // 100% on Firm, Moderate, etc.
        check_in_utc - Duration::days(10),  // 50% on Firm, Strict, Limited
        check_in_utc - Duration::hours(10), // 0% on Flexible, Moderate, etc.
    ];

    let all_policies = [
        CancellationPolicy::Flexible,
        CancellationPolicy::Moderate,
        CancellationPolicy::Strict,
        CancellationPolicy::Firm,
        CancellationPolicy::Limited,
    ];

    for (acc, clean, dep, plat, tax_rate) in test_cases {
        let taxable_base = acc + clean;
        let tax_val = (taxable_base * tax_rate).round_dp(2);
        let total = taxable_base + tax_val + plat + dep;

        let amounts = RefundableAmounts::new(acc, clean, dep, plat, tax_rate, total);

        for policy in all_policies {
            for cancel_at in cancel_times {
                let quote =
                    quote_refund(policy, amounts.clone(), booked_at, check_in, cancel_at).unwrap();

                // Core invariant: refund + retained == total
                assert_eq!(
                    quote.refund_total + quote.retained_total,
                    total,
                    "Invariant violated for policy {:?} with total {}",
                    policy,
                    total
                );

                // Deposit and cleaning fee must always be 100% refunded
                assert_eq!(quote.cleaning_refund, clean);
                assert_eq!(quote.deposit_refund, dep);

                // If full refund, platform fee refunded; else 0
                if quote.policy_pct == Decimal::ONE {
                    assert_eq!(quote.platform_fee_refund, plat);
                    assert_eq!(quote.retained_total, Decimal::ZERO);
                } else {
                    assert_eq!(quote.platform_fee_refund, Decimal::ZERO);
                }
            }
        }
    }
}
