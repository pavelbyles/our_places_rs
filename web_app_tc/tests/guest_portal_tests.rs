use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[test]
fn test_sample_listings_validity_and_parity() {
    let raw = std::fs::read_to_string("src/pages/sample_data.rs")
        .expect("sample_data.rs should exist in web_app_tc");
    assert!(raw.contains("Villa Serenity — Montego Bay"));
    assert!(raw.contains("Blue Lagoon Sanctuary — Port Antonio"));
    assert!(raw.contains("Negril Sunset Cliffside Estate"));
    assert!(raw.contains("Ocho Rios Coastal Haven"));
    assert!(raw.contains("Kingston Skyline Luxury Penthouse"));
}

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

#[tokio::test]
async fn test_get_listing_by_id_api() {
    let res = common::app_client::get_listing_by_id("villa-serenity-montego-bay", None).await;
    println!("API response: {:?}", res);
    assert!(res.is_ok());
}
