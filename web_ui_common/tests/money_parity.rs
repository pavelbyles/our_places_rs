//! The redesigned price breakdown must show exactly the same figures as the
//! legacy component (spec: tri-currency display stays identical).

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

async fn compare(q: PriceBreakdown) {
    let __cx = &Cx::default();
    let l = q.clone();
    let legacy = html(
        view! {
            web_app_common_tc::components::price_breakdown::price_breakdown(
                nights: l.nights,
                effective_nightly_rate: l.effective_nightly_rate,
                subtotal: l.subtotal,
                discount_amount: l.discount_amount,
                cleaning_fee: l.cleaning_fee,
                damage_deposit: l.damage_deposit,
                tax_amount: l.tax_amount,
                total_amount: l.total_amount,
                currency: l.currency,
            )
        },
        __cx,
    )
    .await;
    let r = q.clone();
    let redesigned = html(view! { price_breakdown(quote: r) }, __cx).await;
    for id in IDS {
        assert_eq!(
            text_of(&redesigned, id),
            text_of(&legacy, id),
            "{id} differs for {}",
            q.currency
        );
    }
    let discount = |h: &str| h.contains(&format!("-{} ", q.currency));
    assert_eq!(discount(&redesigned), discount(&legacy), "discount row");
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
    for (currency, rate) in [
        ("USD", dec!(1)),
        ("JMD", dec!(157.3245)),
        ("EUR", dec!(0.9213)),
        ("GBP", dec!(0.7891)),
        ("CAD", dec!(1.3602)),
    ] {
        compare(quote(currency, rate)).await;
    }
}

#[tokio::test]
async fn matches_legacy_when_optional_fees_are_absent_or_zero() {
    compare(PriceBreakdown {
        discount_amount: None,
        cleaning_fee: Some(Decimal::ZERO),
        damage_deposit: None,
        ..quote("USD", dec!(1))
    })
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
