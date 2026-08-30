use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use web_app_common_tc::{
    theme_init_script, theme_toggle_script,
};

#[test]
fn test_theme_scripts_contain_expected_themes_and_classes() {
    let init = theme_init_script();
    assert!(init.contains("sunset"));
    assert!(init.contains("emerald"));
    assert!(init.contains("dark"));
    assert!(init.contains("data-theme"));

    let toggle = theme_toggle_script();
    assert!(toggle.contains("toggleAppTheme"));
    assert!(toggle.contains("localStorage.setItem('theme'"));
}

#[test]
fn test_statutory_tax_calculation_precision() {
    // 15% statutory GCT rate from SupportedCountry::LIST
    let jm = common::reference::SupportedCountry::LIST
        .iter()
        .find(|c| c.iso2char == "JM")
        .expect("Jamaica should be present in SupportedCountry::LIST");

    let subtotal = dec!(1000.00);
    let tax_rate = jm.tax_rate;
    let tax_amount = subtotal * tax_rate;
    let total = subtotal + tax_amount;

    assert_eq!(tax_rate, dec!(0.15));
    assert_eq!(tax_amount, dec!(150.00));
    assert_eq!(total, dec!(1150.00));
}

#[test]
fn test_discount_and_stay_totals() {
    let jm = common::reference::SupportedCountry::LIST
        .iter()
        .find(|c| c.iso2char == "JM")
        .expect("Jamaica should be present in SupportedCountry::LIST");

    let nightly_rate = dec!(250.00);
    let nights = 7;
    let subtotal = nightly_rate * Decimal::from(nights);
    let discount = dec!(100.00);
    let discounted_subtotal = subtotal - discount;
    let tax = discounted_subtotal * jm.tax_rate;
    let grand_total = discounted_subtotal + tax;

    assert_eq!(subtotal, dec!(1750.00));
    assert_eq!(discounted_subtotal, dec!(1650.00));
    assert_eq!(tax, dec!(247.50));
    assert_eq!(grand_total, dec!(1897.50));
}
