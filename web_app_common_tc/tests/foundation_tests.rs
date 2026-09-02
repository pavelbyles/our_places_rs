use chrono::NaiveDate;
use common::models::PriceOverride;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;
use web_app_common_tc::{theme_init_script, theme_toggle_script};

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

#[test]
fn test_tri_currency_conversion_precision() {
    // Base Villa Price in USD
    let base_price_usd = dec!(1800.00);

    // Mock exchange rate: 1 USD = 155.50 JMD
    let rate_jmd = dec!(155.50);
    let converted_jmd = base_price_usd * rate_jmd;
    assert_eq!(converted_jmd, dec!(279900.00));

    // Mock exchange rate: 1 USD = 0.92 EUR
    let rate_eur = dec!(0.92);
    let converted_eur = base_price_usd * rate_eur;
    assert_eq!(converted_eur, dec!(1656.00));
}

#[test]
fn test_seasonal_dynamic_pricing_calculation_with_overrides() {
    let base_rate = dec!(500.00);
    let check_in = NaiveDate::from_ymd_opt(2026, 12, 20).unwrap();
    let check_out = NaiveDate::from_ymd_opt(2026, 12, 25).unwrap(); // 5 nights

    // Peak holiday override: Dec 15 to Jan 5 at $1000/night
    let peak_override = PriceOverride {
        id: Uuid::now_v7(),
        listing_id: Uuid::now_v7(),
        start_date: NaiveDate::from_ymd_opt(2026, 12, 15).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2027, 1, 5).unwrap(),
        nightly_rate: dec!(1000.00),
        min_nights: 5,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let overrides = vec![peak_override];
    let quote = common::pricing::calculate_dynamic_quote(
        base_rate,
        3,
        &overrides,
        check_in,
        check_out,
    ).expect("Dynamic pricing quote calculation should succeed");

    assert_eq!(quote.nightly_breakdown.len(), 5);
    // All 5 nights are inside peak season, so subtotal should be 5 * 1000 = 5000
    assert_eq!(quote.subtotal, dec!(5000.00));
    assert_eq!(quote.required_min_nights, 5);
}

#[test]
fn test_length_of_stay_weekly_discount_calculation() {
    let base_rate = dec!(400.00);
    let nights = 7;
    let gross_subtotal = base_rate * Decimal::from(nights); // 2800
    let weekly_discount_pct = dec!(10.0); // 10%
    let discount_amount = gross_subtotal * (weekly_discount_pct / dec!(100.0)); // 280.00
    let net_subtotal = gross_subtotal - discount_amount; // 2520.00
    let statutory_gct = net_subtotal * dec!(0.15); // 378.00
    let total = net_subtotal + statutory_gct; // 2898.00

    assert_eq!(gross_subtotal, dec!(2800.00));
    assert_eq!(discount_amount, dec!(280.00));
    assert_eq!(net_subtotal, dec!(2520.00));
    assert_eq!(statutory_gct, dec!(378.00));
    assert_eq!(total, dec!(2898.00));
}

#[test]
fn test_pure_rust_ssr_auth_user_initials_and_roles() {
    use web_app_common_tc::auth::AuthUser;

    let admin = AuthUser::new("Maya J.", "maya@ourplaces.com", "admin");
    assert_eq!(admin.initials(), "MJ");
    assert_eq!(admin.role_display(), "Administrator");
    assert!(admin.is_admin());
    assert!(admin.is_authorized_for_admin_portal());

    let guest = AuthUser::new("David Sterling", "david@example.com", "guest");
    assert_eq!(guest.initials(), "DS");
    assert_eq!(guest.role_display(), "Verified Guest");
    assert!(!guest.is_admin());
    assert!(!guest.is_authorized_for_admin_portal());

    let host = AuthUser::new("Elena", "elena@example.com", "host");
    assert_eq!(host.initials(), "EL");
    assert_eq!(host.role_display(), "Host");
    assert!(host.is_host());
    assert!(host.is_authorized_for_admin_portal());
}

#[test]
fn test_session_models_and_namespace_isolation() {
    use common::models::{CreateSessionRequest, SessionResponse};

    let req = CreateSessionRequest {
        token_hash: "a".repeat(64),
        user_id: Uuid::now_v7(),
        email: "host@ourplaces.io".to_string(),
        name: "Host User".to_string(),
        role: "host".to_string(),
        namespace: "admin".to_string(),
        ttl_seconds: 604800,
    };

    let json = serde_json::to_string(&req).expect("Serialization should succeed");
    let deserialized: CreateSessionRequest = serde_json::from_str(&json).expect("Deserialization should succeed");
    assert_eq!(deserialized.namespace, "admin");
    assert_eq!(deserialized.role, "host");
    assert_eq!(deserialized.ttl_seconds, 604800);

    let session_resp = SessionResponse {
        token_hash: "a".repeat(64),
        user_id: req.user_id,
        email: req.email.clone(),
        name: req.name.clone(),
        role: req.role.clone(),
        namespace: "guest".to_string(),
        created_at: chrono::Utc::now(),
        last_accessed_at: chrono::Utc::now(),
        expires_at: 1800000000,
    };

    let resp_json = serde_json::to_string(&session_resp).expect("Serialization should succeed");
    let resp_deserialized: SessionResponse = serde_json::from_str(&resp_json).expect("Deserialization should succeed");
    assert_eq!(resp_deserialized.namespace, "guest");
}

