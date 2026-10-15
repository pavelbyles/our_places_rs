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
    let quote =
        common::pricing::calculate_dynamic_quote(base_rate, 3, &overrides, check_in, check_out)
            .expect("Dynamic pricing quote calculation should succeed");

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
    let deserialized: CreateSessionRequest =
        serde_json::from_str(&json).expect("Deserialization should succeed");
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
    let resp_deserialized: SessionResponse =
        serde_json::from_str(&resp_json).expect("Deserialization should succeed");
    assert_eq!(resp_deserialized.namespace, "guest");
}

#[tokio::test]
async fn test_price_breakdown_itemized_non_zero_fees() {
    use topcoat::context::Cx;
    use topcoat::view::{ViewExt, view};
    use web_app_common_tc::components::price_breakdown::price_breakdown;

    let __cx = &Cx::default();
    let v = view! {
        price_breakdown(
            nights: 5,
            effective_nightly_rate: dec!(500.00),
            subtotal: dec!(2500.00),
            discount_amount: None,
            cleaning_fee: Some(dec!(150.00)),
            damage_deposit: Some(dec!(500.00)),
            tax_amount: dec!(397.50),
            total_amount: dec!(3547.50),
            currency: "USD".to_string(),
        )
    };

    let handle = v.first().await.expect("resolving view should succeed");
    let html = handle.render(__cx);

    assert!(
        html.contains("Cleaning Fee"),
        "Should contain Cleaning Fee label"
    );
    assert!(
        html.contains("USD 150.00"),
        "Should contain Cleaning Fee amount"
    );
    assert!(
        html.contains("Refundable Damage Deposit"),
        "Should contain Damage Deposit label"
    );
    assert!(
        html.contains("USD 500.00"),
        "Should contain Damage Deposit amount"
    );
    assert!(html.contains("USD 3547.50"), "Should contain total amount");
}

#[tokio::test]
async fn test_price_breakdown_zero_fees_omitted() {
    use topcoat::context::Cx;
    use topcoat::view::{ViewExt, view};
    use web_app_common_tc::components::price_breakdown::price_breakdown;

    let __cx = &Cx::default();
    let v = view! {
        price_breakdown(
            nights: 5,
            effective_nightly_rate: dec!(500.00),
            subtotal: dec!(2500.00),
            discount_amount: None,
            cleaning_fee: Some(dec!(0.00)),
            damage_deposit: None,
            tax_amount: dec!(375.00),
            total_amount: dec!(2875.00),
            currency: "USD".to_string(),
        )
    };

    let handle = v.first().await.expect("resolving view should succeed");
    let html = handle.render(__cx);

    assert!(
        !html.contains("Cleaning Fee"),
        "Zero cleaning fee must be omitted"
    );
    assert!(
        !html.contains("Refundable Damage Deposit"),
        "None damage deposit must be omitted"
    );
    assert!(html.contains("USD 2875.00"), "Should contain total amount");
}

#[tokio::test]
async fn test_tax_document_view_compliance_layout_and_print_css() {
    use chrono::Utc;
    use common::tax_document::{
        InvoiceLineItem, InvoiceTaxCategory, TaxDocumentResponse, TaxDocumentStatus,
        TaxDocumentType,
    };
    use topcoat::context::Cx;
    use topcoat::view::{ViewExt, view};
    use web_app_common_tc::components::tax_document_view::tax_document_view;

    let doc = TaxDocumentResponse {
        id: uuid::Uuid::new_v4(),
        document_type: TaxDocumentType::PaymentReceipt,
        document_number: "REC-2026-BK-TEST1".to_string(),
        revision: 1,
        status: TaxDocumentStatus::Issued,
        supersedes_id: None,
        booking_id: uuid::Uuid::new_v4(),
        booking_history_id: None,
        recipient_id: uuid::Uuid::new_v4(),
        listing_id: uuid::Uuid::new_v4(),
        issue_date: Utc::now(),
        supplier_name: "Our Places Jamaica Limited".to_string(),
        supplier_address: "Main Street, Ocho Rios, St. Ann, Jamaica".to_string(),
        supplier_trn: "000-111-222".to_string(),
        recipient_name: "Jane Smith".to_string(),
        recipient_email: "jane@example.com".to_string(),
        recipient_company_name: Some("Global Vacations Inc".to_string()),
        recipient_trn: Some("888-999-000".to_string()),
        currency: "USD".to_string(),
        exchange_rate: dec!(1.000000),
        jmd_exchange_rate: dec!(155.500000),
        accommodation_net: dec!(1000.00),
        cleaning_fee: dec!(150.00),
        taxable_amount: dec!(1150.00),
        tax_rate: dec!(0.1500),
        tax_amount: dec!(172.50),
        platform_fee: dec!(100.00),
        damage_deposit: dec!(300.00),
        total_amount: dec!(1722.50),
        amount_previously_paid: None,
        balance_due: None,
        refund_pct: None,
        retained_amount: None,
        jmd_tax_amount: dec!(26823.75),
        jmd_total_amount: dec!(267848.75),
        line_items: vec![InvoiceLineItem {
            description: "5 Nights Accommodation".to_string(),
            quantity: 5,
            unit_price: dec!(200.00),
            tax_category: InvoiceTaxCategory::Gct,
            tax_rate: dec!(0.1500),
            tax_amount: dec!(150.00),
            total_amount: dec!(1150.00),
        }],
        created_at: Utc::now(),
    };

    let __cx = &Cx::default();
    let v = view! {
        tax_document_view(doc: doc)
    };

    let handle = v.first().await.expect("resolving view should succeed");
    let html = handle.render(__cx);

    // 1. Supplier TRN and Recipient details
    assert!(
        html.contains("000-111-222"),
        "Supplier TRN must be rendered"
    );
    assert!(html.contains("Our Places Jamaica Limited"));
    assert!(html.contains("Jane Smith"));
    assert!(html.contains("Global Vacations Inc"));
    assert!(
        html.contains("888-999-000"),
        "Recipient TRN must be rendered"
    );

    // 2. Document Number & Title
    assert!(html.contains("Payment Receipt"));
    assert!(html.contains("REC-2026-BK-TEST1"));
    assert!(html.contains("Issued"));

    // 3. Line items
    assert!(html.contains("5 Nights Accommodation"));
    assert!(html.contains("GCT"));

    // 4. Statutory Dual Currency JMD
    assert!(html.contains("1 USD = 155.5000 JMD"));
    assert!(html.contains("JMD 26823.75"));
    assert!(html.contains("JMD 267848.75"));

    // 5. Print CSS
    assert!(html.contains("@media print"));
    assert!(html.contains(".no-print"));
}

#[tokio::test]
async fn test_tax_document_view_superseded_and_revised_balance() {
    use chrono::Utc;
    use common::tax_document::{TaxDocumentResponse, TaxDocumentStatus, TaxDocumentType};
    use topcoat::context::Cx;
    use topcoat::view::{ViewExt, view};
    use web_app_common_tc::components::tax_document_view::tax_document_view;

    let doc = TaxDocumentResponse {
        id: uuid::Uuid::new_v4(),
        document_type: TaxDocumentType::PaymentReceipt,
        document_number: "REC-2026-BK-TEST2-R2".to_string(),
        revision: 2,
        status: TaxDocumentStatus::Superseded,
        supersedes_id: Some(uuid::Uuid::new_v4()),
        booking_id: uuid::Uuid::new_v4(),
        booking_history_id: None,
        recipient_id: uuid::Uuid::new_v4(),
        listing_id: uuid::Uuid::new_v4(),
        issue_date: Utc::now(),
        supplier_name: "Our Places Jamaica Limited".to_string(),
        supplier_address: "Main Street, Ocho Rios, St. Ann, Jamaica".to_string(),
        supplier_trn: "000-111-222".to_string(),
        recipient_name: "Bob Traveler".to_string(),
        recipient_email: "bob@example.com".to_string(),
        recipient_company_name: None,
        recipient_trn: None,
        currency: "USD".to_string(),
        exchange_rate: dec!(1.000000),
        jmd_exchange_rate: dec!(155.500000),
        accommodation_net: dec!(1200.00),
        cleaning_fee: dec!(150.00),
        taxable_amount: dec!(1350.00),
        tax_rate: dec!(0.1500),
        tax_amount: dec!(202.50),
        platform_fee: dec!(120.00),
        damage_deposit: dec!(300.00),
        total_amount: dec!(1972.50),
        amount_previously_paid: Some(dec!(1722.50)),
        balance_due: Some(dec!(250.00)),
        refund_pct: None,
        retained_amount: None,
        jmd_tax_amount: dec!(31488.75),
        jmd_total_amount: dec!(306723.75),
        line_items: vec![],
        created_at: Utc::now(),
    };

    let __cx = &Cx::default();
    let v = view! {
        tax_document_view(doc: doc)
    };

    let handle = v.first().await.expect("resolving view should succeed");
    let html = handle.render(__cx);

    assert!(html.contains("SUPERSEDED DOCUMENT"));
    assert!(html.contains("Superseded"));
    assert!(html.contains("Revision:"));
    assert!(html.contains("Amount Previously Paid:"));
    assert!(html.contains("USD 1722.50"));
    assert!(html.contains("Balance Due:"));
    assert!(html.contains("USD 250.00"));
}
