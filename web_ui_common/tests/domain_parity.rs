//! The redesigned domain components must show the same figures and keep the
//! element IDs of the legacy components. The expected texts were captured
//! from the legacy `web_app_common_tc` components before that crate was
//! removed.

use chrono::{TimeZone, Utc};
use common::tax_document::{
    InvoiceLineItem, InvoiceTaxCategory, TaxDocumentResponse, TaxDocumentStatus, TaxDocumentType,
};
use rust_decimal_macros::dec;
use topcoat::context::Cx;
use topcoat::view::{ViewExt, view};
use uuid::Uuid;
use web_ui_common::components::domain::{
    listing_card::{ListingSummary, listing_card},
    responsive_image::responsive_image,
    tax_document_view::tax_document_view,
};

async fn html(v: impl topcoat::view::View, cx: &Cx) -> String {
    match v.first().await {
        Ok(handle) => handle.render(cx),
        Err(e) => panic!("render failed: {e}"),
    }
}

/// Text inside the element with `id`, with nested tags removed.
fn text_of(html: &str, id: &str) -> Option<String> {
    let at = html.find(&format!(r#"id="{id}""#))?;
    let open_end = html[at..].find('>')? + at + 1;
    let tag_start = html[..at].rfind('<')?;
    let tag: String = html[tag_start + 1..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    let close = html[open_end..].find(&format!("</{tag}>"))? + open_end;
    let mut text = String::new();
    let mut in_tag = false;
    for c in html[open_end..close].chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    Some(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

const DOC_IDS: [&str; 20] = [
    "tax-doc-title",
    "tax-doc-status",
    "tax-doc-number",
    "tax-doc-issue-date",
    "supplier-name",
    "supplier-address",
    "supplier-trn",
    "recipient-name",
    "recipient-email",
    "recipient-company",
    "recipient-trn",
    "statutory-fx-rate",
    "statutory-jmd-tax",
    "statutory-jmd-total",
    "tot-acc",
    "tot-cleaning",
    "tot-taxable",
    "tot-tax",
    "tot-platform",
    "tot-total",
];

fn doc(kind: TaxDocumentType, status: TaxDocumentStatus, currency: &str) -> TaxDocumentResponse {
    TaxDocumentResponse {
        id: Uuid::nil(),
        document_type: kind,
        document_number: "REC-2026-OP-E2E-7788".into(),
        revision: 2,
        status,
        supersedes_id: None,
        booking_id: Uuid::nil(),
        booking_history_id: None,
        recipient_id: Uuid::nil(),
        listing_id: Uuid::nil(),
        issue_date: Utc
            .with_ymd_and_hms(2026, 9, 12, 15, 0, 0)
            .single()
            .unwrap_or_default(),
        supplier_name: "Our Places Jamaica Limited".into(),
        supplier_address: "Main Street, Ocho Rios".into(),
        supplier_trn: "000-111-222".into(),
        recipient_name: "Jordan Tester".into(),
        recipient_email: "guest.tester@example.com".into(),
        recipient_company_name: Some("Tester & Co".into()),
        recipient_trn: Some("123-456-789".into()),
        currency: currency.into(),
        exchange_rate: dec!(1),
        jmd_exchange_rate: dec!(160.1234),
        accommodation_net: dec!(1950.00),
        cleaning_fee: dec!(75.00),
        taxable_amount: dec!(2025.00),
        tax_rate: dec!(0.1500),
        tax_amount: dec!(303.75),
        platform_fee: dec!(50.00),
        damage_deposit: dec!(150.00),
        total_amount: dec!(2528.75),
        amount_previously_paid: Some(dec!(2600.00)),
        balance_due: Some(dec!(-71.25)),
        refund_pct: Some(dec!(0.5)),
        retained_amount: Some(dec!(975.00)),
        jmd_tax_amount: dec!(48637.68),
        jmd_total_amount: dec!(404912.04),
        line_items: vec![InvoiceLineItem {
            description: "Accommodation (3 nights)".into(),
            quantity: 3,
            unit_price: dec!(650.00),
            tax_category: InvoiceTaxCategory::Gct,
            tax_rate: dec!(0.1500),
            tax_amount: dec!(292.50),
            total_amount: dec!(1950.00),
        }],
        created_at: Utc
            .with_ymd_and_hms(2026, 9, 12, 15, 0, 0)
            .single()
            .unwrap_or_default(),
    }
}

/// Legacy texts for `DOC_IDS`, in order, for a document in `currency`.
fn legacy_doc_texts(title: &str, status: &str, currency: &str) -> Vec<String> {
    let money = |amount: &str| format!("{currency} {amount}");
    let mut texts = vec![
        title.to_string(),
        status.to_string(),
        "REC-2026-OP-E2E-7788".into(),
        "September 12, 2026".into(),
        "Our Places Jamaica Limited".into(),
        "Main Street, Ocho Rios".into(),
        "000-111-222".into(),
        "Jordan Tester".into(),
        "guest.tester@example.com".into(),
        "Company: Tester &amp; Co".into(),
        "Recipient TRN: 123-456-789".into(),
        format!("1 {currency} = 160.1234 JMD"),
        "JMD 48637.68".into(),
        "JMD 404912.04".into(),
    ];
    texts.extend(["1950.00", "75.00", "2025.00", "303.75", "50.00", "2528.75"].map(money));
    texts
}

#[tokio::test]
async fn tax_document_matches_legacy_figures_and_ids() {
    let __cx = &Cx::default();
    let cases = [
        (
            doc(
                TaxDocumentType::PaymentReceipt,
                TaxDocumentStatus::Issued,
                "USD",
            ),
            legacy_doc_texts("Payment Receipt", "Issued", "USD"),
            [true, true, true, true, false],
        ),
        (
            doc(
                TaxDocumentType::TaxInvoice,
                TaxDocumentStatus::Superseded,
                "EUR",
            ),
            legacy_doc_texts("Statutory Tax Invoice", "Superseded", "EUR"),
            [true, false, true, true, true],
        ),
        (
            doc(
                TaxDocumentType::RefundReceipt,
                TaxDocumentStatus::Issued,
                "GBP",
            ),
            legacy_doc_texts("Refund Receipt", "Issued", "GBP"),
            [true, false, true, true, false],
        ),
    ];
    for (d, legacy, legacy_phrases) in cases {
        let r = d.clone();
        let redesigned = html(view! { tax_document_view(doc: r) }, __cx).await;
        for (id, expected) in DOC_IDS.into_iter().zip(&legacy) {
            assert_eq!(
                text_of(&redesigned, id).as_deref(),
                Some(expected.as_str()),
                "#{id} for {:?}",
                d.document_type
            );
        }
        let c = &d.currency;
        assert_eq!(
            text_of(&redesigned, "tax-line-items"),
            Some(format!(
                "Accommodation (3 nights)3{c} 650.00GCT15.0%{c} 292.50{c} 1950.00"
            )),
            "line items"
        );
        let phrases = [
            "Refund Due:",
            "USD 71.25",
            "50%",
            "Amount Retained by Host:",
            "SUPERSEDED DOCUMENT",
        ];
        for (phrase, expected) in phrases.into_iter().zip(legacy_phrases) {
            assert_eq!(redesigned.contains(phrase), expected, "{phrase:?}");
        }
        assert!(redesigned.contains("tax-document-container"));
        assert!(
            !redesigned.contains("<style"),
            "print CSS lives in the stylesheet"
        );
    }
}

#[tokio::test]
async fn responsive_image_srcset_matches_legacy() {
    let __cx = &Cx::default();
    let cases: [(&str, &[&str]); 2] = [
        (
            "https://storage.googleapis.com/b/villa",
            &[
                "https://storage.googleapis.com/b/villa_1920.webp",
                "https://storage.googleapis.com/b/villa_1024.webp",
                "https://storage.googleapis.com/b/villa_640.webp",
            ],
        ),
        ("/static/local.jpg", &[]),
    ];
    for (src, legacy) in cases {
        let r = src.to_string();
        let redesigned = html(view! { responsive_image(src: r, alt: "Pool".into()) }, __cx).await;
        let srcsets = redesigned
            .match_indices("srcset=\"")
            .map(|(i, _)| redesigned[i..].split('"').nth(1).unwrap_or("").to_string())
            .collect::<Vec<_>>();
        assert_eq!(srcsets, legacy, "{src}");
        assert!(redesigned.contains(r#"alt="Pool""#) && redesigned.contains(r#"loading="lazy""#));
    }
}

#[tokio::test]
async fn listing_card_keeps_the_texts_the_e2e_suite_checks() {
    let __cx = &Cx::default();
    let listing = ListingSummary {
        slug: "the-courtyard-studio-new-kingston".into(),
        name: "The Courtyard Studio".into(),
        image_url: None,
        price: "650".into(),
        currency: "USD".into(),
        country: "Jamaica".into(),
        city: Some("New Kingston".into()),
        max_guests: 4,
        bedrooms: 2,
        full_bathrooms: 2,
        rating: None,
        review_count: None,
    };
    let out = html(view! { listing_card(listing: listing) }, __cx).await;
    for text in [
        "USD 650",
        "New Kingston, Jamaica",
        "4 Guests",
        "View Listing",
        "The Courtyard Studio",
    ] {
        assert!(out.contains(text), "{text}: {out}");
    }
    assert!(out.contains(r#"href="/listings/the-courtyard-studio-new-kingston""#));
    assert!(
        out.contains("New") && !out.contains("4.95") && !out.contains("Infinity Pool"),
        "no invented data"
    );
}
