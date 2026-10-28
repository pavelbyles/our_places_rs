//! The redesigned domain components must show the same figures and keep the
//! element IDs of the legacy components.

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

#[tokio::test]
async fn tax_document_matches_legacy_figures_and_ids() {
    let __cx = &Cx::default();
    for d in [
        doc(
            TaxDocumentType::PaymentReceipt,
            TaxDocumentStatus::Issued,
            "USD",
        ),
        doc(
            TaxDocumentType::TaxInvoice,
            TaxDocumentStatus::Superseded,
            "EUR",
        ),
        doc(
            TaxDocumentType::RefundReceipt,
            TaxDocumentStatus::Issued,
            "GBP",
        ),
    ] {
        let l = d.clone();
        let legacy = html(
            view! { web_app_common_tc::components::tax_document_view::tax_document_view(doc: l) },
            __cx,
        )
        .await;
        let r = d.clone();
        let redesigned = html(view! { tax_document_view(doc: r) }, __cx).await;
        for id in DOC_IDS {
            assert!(text_of(&redesigned, id).is_some(), "#{id} missing");
            assert_eq!(
                text_of(&redesigned, id),
                text_of(&legacy, id),
                "#{id} for {:?}",
                d.document_type
            );
        }
        let line_cells = |h: &str| text_of(h, "tax-line-items");
        assert_eq!(line_cells(&redesigned), line_cells(&legacy), "line items");
        for phrase in [
            "Refund Due:",
            "USD 71.25",
            "50%",
            "Amount Retained by Host:",
            "SUPERSEDED DOCUMENT",
        ] {
            assert_eq!(
                redesigned.contains(phrase),
                legacy.contains(phrase),
                "{phrase:?}"
            );
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
    for src in [
        "https://storage.googleapis.com/b/villa",
        "/static/local.jpg",
    ] {
        let l = src.to_string();
        let legacy = html(view! { web_app_common_tc::components::responsive_image::responsive_image(src: l, alt: "Pool".into(), class: None) }, __cx).await;
        let r = src.to_string();
        let redesigned = html(view! { responsive_image(src: r, alt: "Pool".into()) }, __cx).await;
        let srcsets = |h: &str| {
            h.match_indices("srcset=\"")
                .map(|(i, _)| h[i..].split('"').nth(1).unwrap_or("").to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(srcsets(&redesigned), srcsets(&legacy), "{src}");
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
