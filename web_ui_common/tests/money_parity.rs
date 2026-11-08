//! The price breakdown shows each figure rounded to cents (as pricing rounds,
//! not truncated), with thousands separators and currency symbols: line
//! items show only the symbol, the total names the currency too.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use topcoat::context::Cx;
use topcoat::view::{ViewExt, view};
use web_ui_common::components::domain::{
    currency_selector::currency_menu,
    price_breakdown::{PriceBreakdown, price_breakdown},
};

async fn html(v: impl topcoat::view::View, cx: &Cx) -> String {
    match v.first().await {
        Ok(handle) => handle.render(cx),
        Err(e) => panic!("render failed: {e}"),
    }
}

/// Text content of the element with `id` (the markup inside has no tags).
fn text_of(html: &str, id: &str) -> Option<String> {
    let at = html.find(&format!(r#"id="{id}""#))?;
    let start = html[at..].find('>')? + at + 1;
    let end = html[start..].find('<')? + start;
    Some(html[start..end].to_string())
}

const IDS: [&str; 6] = [
    "breakdown-nights-label",
    "breakdown-subtotal",
    "breakdown-cleaning-fee",
    "breakdown-tax",
    "breakdown-damage-deposit",
    "breakdown-total",
];

async fn compare(q: PriceBreakdown, legacy: [Option<&str>; 6], legacy_discount: bool) {
    let __cx = &Cx::default();
    let r = q.clone();
    let redesigned = html(view! { price_breakdown(quote: r) }, __cx).await;
    for (id, expected) in IDS.into_iter().zip(legacy) {
        assert_eq!(
            text_of(&redesigned, id).as_deref(),
            expected,
            "{id} differs for {}",
            q.currency
        );
    }
    let discount = redesigned.contains("Discount applied");
    assert_eq!(discount, legacy_discount, "discount row");
}

fn quote(currency: &str, scale: Decimal) -> PriceBreakdown {
    PriceBreakdown {
        nights: 5,
        effective_nightly_rate: dec!(650.00) * scale,
        subtotal: dec!(3250.00) * scale,
        discount_amount: Some(dec!(325.125) * scale),
        promo_code: None,
        promo_discount: None,
        platform_fee: None,
        cleaning_fee: Some(dec!(75.00) * scale),
        damage_deposit: Some(dec!(150.00) * scale),
        tax_amount: dec!(498.75) * scale,
        total_amount: dec!(3648.625) * scale,
        currency: currency.to_string(),
    }
}

#[tokio::test]
async fn every_display_currency_gets_its_symbol_and_separators() {
    let cases = [
        (
            "USD",
            dec!(1),
            [
                "$650.00 × 5 nights",
                "$3,250.00",
                "$75.00",
                "$498.75",
                "$150.00",
                "USD $3,648.62",
            ],
        ),
        (
            "JMD",
            dec!(157.3245),
            [
                "$102,260.92 × 5 nights",
                "$511,304.62",
                "$11,799.34",
                "$78,465.59",
                "$23,598.68",
                "JMD $574,018.10",
            ],
        ),
        (
            "EUR",
            dec!(0.9213),
            [
                "598.84 × 5 nights",
                "2,994.22",
                "69.10",
                "459.50",
                "138.20",
                "EUR 3,361.48",
            ],
        ),
        (
            "GBP",
            dec!(0.7891),
            [
                "£512.92 × 5 nights",
                "£2,564.58",
                "£59.18",
                "£393.56",
                "£118.36",
                "GBP £2,879.13",
            ],
        ),
        (
            "CAD",
            dec!(1.3602),
            [
                "$884.13 × 5 nights",
                "$4,420.65",
                "$102.02",
                "$678.40",
                "$204.03",
                "CAD $4,962.86",
            ],
        ),
    ];
    for (currency, rate, legacy) in cases {
        compare(quote(currency, rate), legacy.map(Some), true).await;
    }
}

#[tokio::test]
async fn optional_fees_that_are_absent_or_zero_are_left_off() {
    compare(
        PriceBreakdown {
            discount_amount: None,
            promo_code: None,
            promo_discount: None,
            platform_fee: None,
            cleaning_fee: Some(Decimal::ZERO),
            damage_deposit: None,
            ..quote("USD", dec!(1))
        },
        [
            Some("$650.00 × 5 nights"),
            Some("$3,250.00"),
            None,
            Some("$498.75"),
            None,
            Some("USD $3,648.62"),
        ],
        false,
    )
    .await;
}

#[test]
fn grouped_inserts_thousands_separators() {
    use web_ui_common::components::domain::price_breakdown::{grouped, total_amount};
    assert_eq!(grouped(dec!(0)), "0.00");
    assert_eq!(grouped(dec!(999.5)), "999.50");
    assert_eq!(grouped(dec!(1000)), "1,000.00");
    assert_eq!(grouped(dec!(11240)), "11,240.00");
    assert_eq!(grouped(dec!(1234567.891)), "1,234,567.89");
    assert_eq!(grouped(dec!(-325125.5)), "-325,125.50");
    assert_eq!(total_amount("USD", dec!(14133)), "USD $14,133.00");
    assert_eq!(total_amount("GBP", dec!(14133)), "GBP £14,133.00");
}

#[tokio::test]
async fn one_night_is_singular() {
    let __cx = &Cx::default();
    let q = PriceBreakdown {
        nights: 1,
        ..quote("USD", dec!(1))
    };
    let out = html(view! { price_breakdown(quote: q) }, __cx).await;
    assert_eq!(
        text_of(&out, "breakdown-nights-label").as_deref(),
        Some("$650.00 × 1 night")
    );
}

#[tokio::test]
async fn currency_selector_links_every_currency_and_marks_the_active_one() {
    let __cx = &Cx::default();
    let codes: Vec<String> = ["CAD", "EUR", "GBP", "JMD", "USD"]
        .iter()
        .map(|c| c.to_string())
        .collect();
    let menu_codes = codes.clone();
    let out = html(
        view! { currency_menu(codes: menu_codes, current_currency: Some("jmd".into())) },
        __cx,
    )
    .await;
    for c in &codes {
        assert!(
            out.contains(&format!(r#"href="/currency/{c}?next=%2F""#)),
            "{c}: {out}"
        );
    }
    assert_eq!(out.matches(r#"aria-current="true""#).count(), 1, "{out}");
    assert!(out.contains("Display currency: JMD"), "{out}");

    // A code that is no longer offered (e.g. deactivated) prompts a choice
    // instead of pretending to be USD.
    let unknown_codes = codes.clone();
    let unknown = html(
        view! { currency_menu(codes: unknown_codes, current_currency: Some("XYZ".into())) },
        __cx,
    )
    .await;
    assert!(unknown.contains("Choose a currency"), "{unknown}");
    assert_eq!(
        unknown.matches(r#"aria-current="true""#).count(),
        0,
        "{unknown}"
    );

    // No list (backend unreachable): the menu is hidden.
    let hidden = html(
        view! { currency_menu(codes: Vec::new(), current_currency: None) },
        __cx,
    )
    .await;
    assert!(!hidden.contains("<details"), "{hidden}");
}
