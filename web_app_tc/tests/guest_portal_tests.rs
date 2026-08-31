use chrono::{Duration, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[test]
fn test_guest_stay_subtotal_and_statutory_gct() {
    let nightly_rate = dec!(650.00);
    let nights = 5;
    let subtotal = nightly_rate * Decimal::from(nights);
    let gct_rate = dec!(0.15); // Statutory Jamaican GCT
    let tax_amount = subtotal * gct_rate;
    let total_amount = subtotal + tax_amount;

    assert_eq!(subtotal, dec!(3250.00));
    assert_eq!(tax_amount, dec!(487.50));
    assert_eq!(total_amount, dec!(3737.50));
}

#[test]
fn test_fifteen_minute_booking_hold_expiry_calculation() {
    let now = Utc::now();
    let hold_duration = Duration::minutes(15);
    let expires_at = now + hold_duration;

    assert_eq!(expires_at - now, Duration::seconds(900));
    assert!(expires_at > now);

    // After 16 minutes, hold is expired
    let after_timeout = now + Duration::minutes(16);
    assert!(after_timeout > expires_at);
}

#[test]
fn test_guest_stay_with_length_of_stay_discount_and_gct() {
    let nightly_rate = dec!(980.00);
    let nights = 10; // 10 nights qualifies for 10% weekly discount
    let gross_subtotal = nightly_rate * Decimal::from(nights); // 9800.00
    let weekly_discount_rate = dec!(0.10);
    let discount = gross_subtotal * weekly_discount_rate; // 980.00
    let discounted_subtotal = gross_subtotal - discount; // 8820.00
    let statutory_gct = discounted_subtotal * dec!(0.15); // 1323.00
    let total = discounted_subtotal + statutory_gct; // 10143.00

    assert_eq!(gross_subtotal, dec!(9800.00));
    assert_eq!(discount, dec!(980.00));
    assert_eq!(discounted_subtotal, dec!(8820.00));
    assert_eq!(statutory_gct, dec!(1323.00));
    assert_eq!(total, dec!(10143.00));
}

#[tokio::test]
async fn test_get_listing_by_id_api() {
    let slugs = vec![
        "the-courtyard-studio-new-kingston",
        "kingston-skyline-luxury-suite",
        "the-palms-luxury-residences-kingston-6",
        "st-ann-coastal-estate-cottage-ocho-rios",
        "seaside-sanctuary-villa-runaway-bay",
        "test-from-ui1",
    ];

    for slug in slugs {
        let res = common::app_client::get_listing_by_id(slug, None).await;
        println!("API response for {}: {:?}", slug, res.as_ref().map(|d| &d.listing.name));
        assert!(res.is_ok(), "Failed to fetch listing: {}", slug);
    }
}

#[test]
fn test_tri_currency_cross_currency_checkout_settlement() {
    let base_usd = dec!(650.00);
    let nights = 5;

    // 1. JMD Settlement (1 USD = 155.50 JMD)
    let fx_jmd = dec!(155.50);
    let jmd_nightly = base_usd * fx_jmd; // 101,075.00
    let jmd_subtotal = jmd_nightly * Decimal::from(nights); // 505,375.00
    let jmd_gct = jmd_subtotal * dec!(0.15); // 75,806.25
    let jmd_total = jmd_subtotal + jmd_gct; // 581,181.25

    assert_eq!(jmd_nightly, dec!(101075.00));
    assert_eq!(jmd_subtotal, dec!(505375.00));
    assert_eq!(jmd_gct, dec!(75806.25));
    assert_eq!(jmd_total, dec!(581181.25));

    // 2. EUR Settlement (1 USD = 0.92 EUR)
    let fx_eur = dec!(0.92);
    let eur_nightly = base_usd * fx_eur; // 598.00
    let eur_subtotal = eur_nightly * Decimal::from(nights); // 2990.00
    let eur_gct = eur_subtotal * dec!(0.15); // 448.50
    let eur_total = eur_subtotal + eur_gct; // 3438.50

    assert_eq!(eur_nightly, dec!(598.00));
    assert_eq!(eur_subtotal, dec!(2990.00));
    assert_eq!(eur_gct, dec!(448.50));
    assert_eq!(eur_total, dec!(3438.50));

    // 3. CAD Settlement (1 USD = 1.36 CAD)
    let fx_cad = dec!(1.36);
    let cad_nightly = base_usd * fx_cad; // 884.00
    let cad_subtotal = cad_nightly * Decimal::from(nights); // 4420.00
    let cad_gct = cad_subtotal * dec!(0.15); // 663.00
    let cad_total = cad_subtotal + cad_gct; // 5083.00
    assert_eq!(cad_gct, dec!(663.00));
    assert_eq!(cad_total, dec!(5083.00));
}








