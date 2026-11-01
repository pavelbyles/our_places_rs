//! The redesigned price breakdown must show exactly the same figures as the
//! legacy component did (spec: tri-currency display stays identical). The
//! expected texts were captured from the legacy `web_app_common_tc`
//! component before that crate was removed.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use topcoat::context::Cx;
use topcoat::view::{ViewExt, view};
use web_ui_common::components::domain::{
    currency_selector::currency_selector,
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
    let discount = redesigned.contains(&format!("-{} ", q.currency));
    assert_eq!(discount, legacy_discount, "discount row");
}

fn quote(currency: &str, scale: Decimal) -> PriceBreakdown {
    PriceBreakdown {
        nights: 5,
        effective_nightly_rate: dec!(650.00) * scale,
        subtotal: dec!(3250.00) * scale,
        discount_amount: Some(dec!(325.125) * scale),
        cleaning_fee: Some(dec!(75.00) * scale),
        damage_deposit: Some(dec!(150.00) * scale),
        tax_amount: dec!(498.75) * scale,
        total_amount: dec!(3648.625) * scale,
        currency: currency.to_string(),
    }
}

#[tokio::test]
async fn matches_legacy_for_every_display_currency() {
    let cases = [
        (
            "USD",
            dec!(1),
            [
                "5 night(s) × USD 650.00",
                "USD 3250.00",
                "USD 75.00",
                "USD 498.75",
                "USD 150.00",
                "USD 3648.62",
            ],
        ),
        (
            "JMD",
            dec!(157.3245),
            [
                "5 night(s) × JMD 102260.92",
                "JMD 511304.62",
                "JMD 11799.33",
                "JMD 78465.59",
                "JMD 23598.67",
                "JMD 574018.10",
            ],
        ),
        (
            "EUR",
            dec!(0.9213),
            [
                "5 night(s) × EUR 598.84",
                "EUR 2994.22",
                "EUR 69.09",
                "EUR 459.49",
                "EUR 138.19",
                "EUR 3361.47",
            ],
        ),
        (
            "GBP",
            dec!(0.7891),
            [
                "5 night(s) × GBP 512.91",
                "GBP 2564.57",
                "GBP 59.18",
                "GBP 393.56",
                "GBP 118.36",
                "GBP 2879.12",
            ],
        ),
        (
            "CAD",
            dec!(1.3602),
            [
                "5 night(s) × CAD 884.13",
                "CAD 4420.65",
                "CAD 102.01",
                "CAD 678.39",
                "CAD 204.03",
                "CAD 4962.85",
            ],
        ),
    ];
    for (currency, rate, legacy) in cases {
        compare(quote(currency, rate), legacy.map(Some), true).await;
    }
}

#[tokio::test]
async fn matches_legacy_when_optional_fees_are_absent_or_zero() {
    compare(
        PriceBreakdown {
            discount_amount: None,
            cleaning_fee: Some(Decimal::ZERO),
            damage_deposit: None,
            ..quote("USD", dec!(1))
        },
        [
            Some("5 night(s) × USD 650.00"),
            Some("USD 3250.00"),
            None,
            Some("USD 498.75"),
            None,
            Some("USD 3648.62"),
        ],
        false,
    )
    .await;
}

#[tokio::test]
async fn currency_selector_links_every_currency_and_marks_the_active_one() {
    let __cx = &Cx::default();
    let out = html(
        view! { currency_selector(current_currency: Some("jmd".into())) },
        __cx,
    )
    .await;
    for c in ["USD", "JMD", "EUR", "GBP", "CAD"] {
        assert!(
            out.contains(&format!(r#"href="?currency={c}""#)),
            "{c}: {out}"
        );
    }
    assert_eq!(out.matches(r#"aria-current="true""#).count(), 1, "{out}");
    assert!(out.contains("Display currency: JMD"), "{out}");
    let unknown = html(
        view! { currency_selector(current_currency: Some("XYZ".into())) },
        __cx,
    )
    .await;
    assert!(unknown.contains("Display currency: USD"), "{unknown}");
}
