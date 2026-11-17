//! Spec 102 §1.5.4: listing page metadata (U12, U13).
//!
//! A full guest page can't render under `cargo test` (the guest layout needs
//! the asset bundle), so U13 serves an explicit router: a probe layout that
//! renders `tags()`, and a probe page that uses the real `load` and
//! `listing_metadata`, against a mocked `listing_api`.

use std::sync::Arc;

use common::http_client::{AuthenticatedClient, TokenProvider};
use common::models::ListingResponse;
use tokio::net::TcpListener;
use topcoat::{
    Result,
    context::{BaseUrl, Cx},
    cookie::CookieLayer,
    metadata::Metadata,
    router::{Router, Slot, layout, page},
    session::{SessionTokenStore, cookie::CookieTokenStore},
    view::{View, view},
};
use web_client::TopcoatApiClient;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::seo::{meta_description, truncate_on_word};

// ---- U12: the meta description ----

fn listing(description: Option<&str>, city: Option<&str>, bedrooms: i32) -> ListingResponse {
    let mut json = serde_json::json!({
        "id": "0190a0a0-0000-7000-8000-000000000001",
        "user_id": "0190a0a0-0000-7000-8000-000000000002",
        "name": "Villa Azure",
        "listing_structure": "Villa",
        "country": "Jamaica",
        "is_active": true,
        "added_at": "2026-01-01T00:00:00Z",
        "max_guests": 6, "bedrooms": bedrooms, "beds": 3,
        "full_bathrooms": 2, "half_bathrooms": 0,
        "base_currency": "USD", "slug": "villa-azure",
        "minimum_stay": 2, "days_between_bookings": 0
    });
    json["description"] = serde_json::json!(description);
    json["city"] = serde_json::json!(city);
    match serde_json::from_value(json) {
        Ok(l) => l,
        Err(e) => panic!("listing fixture: {e}"),
    }
}

#[test]
fn a_short_description_is_used_with_whitespace_collapsed() {
    let l = listing(
        Some("  Sea views,\n\n  private   pool. "),
        Some("Negril"),
        3,
    );
    assert_eq!(meta_description(&l), "Sea views, private pool.");
}

#[test]
fn a_long_description_is_cut_at_a_word_within_160_chars() {
    let text = "Wake to the sound of waves ".repeat(10);
    let out = meta_description(&listing(Some(&text), Some("Negril"), 3));
    assert!(out.chars().count() <= 160, "{}", out.chars().count());
    assert!(out.ends_with('…'), "{out}");
    let body = out.trim_end_matches('…');
    assert!(text.starts_with(body), "{out}");
    assert!(!body.ends_with(' '), "{out}");
    assert!(text[body.len()..].starts_with(' '), "cut mid-word: {out}");
}

#[test]
fn multi_byte_and_emoji_text_is_cut_by_characters() {
    let text = "Été à la plage 🌴🌊 ".repeat(20);
    let out = truncate_on_word(&text, 160);
    assert!(out.chars().count() <= 160);
    assert!(out.ends_with('…'));
    let emoji_only = "🌴".repeat(200);
    let out = truncate_on_word(&emoji_only, 160);
    assert_eq!(out.chars().count(), 160);
}

#[test]
fn without_a_description_the_fallback_names_rooms_and_place() {
    assert_eq!(
        meta_description(&listing(None, Some("Negril"), 3)),
        "3-bedroom villa in Negril, Jamaica"
    );
}

#[test]
fn the_fallback_handles_a_missing_city() {
    assert_eq!(
        meta_description(&listing(Some("   "), None, 2)),
        "2-bedroom villa in Jamaica"
    );
}

#[test]
fn the_fallback_handles_zero_bedrooms() {
    assert_eq!(
        meta_description(&listing(None, Some("Negril"), 0)),
        "Villa in Negril, Jamaica"
    );
}

// ---- U13: metadata served for a listing ----

struct FixedIdToken;

#[async_trait::async_trait]
impl TokenProvider for FixedIdToken {
    async fn get_token(&self, _audience: &str) -> anyhow::Result<String> {
        Ok("service-id-token".to_string())
    }
}

#[layout("/__seo-probe")]
async fn probe_layout(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        <!DOCTYPE html>
        <html>
            <head>topcoat::metadata::tags()</head>
            <body>(slot)</body>
        </html>
    })
}

/// Loads the listing like the real page (sharing `load` with the metadata).
#[page(
    "/__seo-probe/listings/{slug}",
    with(Metadata::from_fn(super::listing_metadata))
)]
async fn probe_listing(cx: &Cx) -> Result<impl View> {
    let loaded = super::load(cx, super::slug(cx)).await?;
    let name = loaded.details.listing.name.clone();
    let structured = super::listing_structured_data(cx, &loaded.details);
    Ok(view! {
        <h1>(name)</h1>
        if let Some(structured) = structured {
            web_ui_common::components::json_ld::json_ld(value: structured)
        }
    })
}

fn details_json(slug: &str, images: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "listing": {
            "id": "0190a0a0-0000-7000-8000-000000000001",
            "user_id": "0190a0a0-0000-7000-8000-000000000002",
            "name": "Villa Azure",
            "description": "Sea views & a private pool.",
            "listing_structure": "Villa",
            "country": "Jamaica",
            "city": "Negril",
            "is_active": true,
            "added_at": "2026-01-01T00:00:00Z",
            "max_guests": 6, "bedrooms": 3, "beds": 3,
            "full_bathrooms": 2, "half_bathrooms": 0,
            "base_currency": "USD", "slug": slug,
            "minimum_stay": 2, "days_between_bookings": 0
        },
        "images": images
    })
}

const PHOTO: &str = "https://storage.googleapis.com/b/optimized/mobile/l_1.webp";

async fn serve(api: &MockServer) -> String {
    let client = TopcoatApiClient::new(
        AuthenticatedClient::with_token_provider(Arc::new(FixedIdToken)),
        api.uri(),
        api.uri(),
        api.uri(),
    );
    let router = match BaseUrl::new("https://ourplaces.io") {
        Ok(base) => Router::builder()
            .layout(probe_layout)
            .page(probe_listing)
            .layer(CookieLayer::new())
            .with(SessionTokenStore::new(
                CookieTokenStore::new().name("op_guest_session"),
            ))
            .with(base)
            .with(client)
            .build(),
        Err(e) => panic!("origin: {e}"),
    };
    Mock::given(method("GET"))
        .and(path("/api/v1/currencies"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "currencies": [] })),
        )
        .mount(api)
        .await;
    let listener = TcpListener::bind("127.0.0.1:0").await.ok();
    let addr = listener.as_ref().and_then(|l| l.local_addr().ok());
    if let Some(listener) = listener {
        tokio::spawn(async move {
            let _ = topcoat::serve(listener, router).await;
        });
    }
    addr.map(|a| format!("http://{a}")).unwrap_or_default()
}

async fn get(api: &MockServer, slug: &str) -> (u16, String) {
    let base = serve(api).await;
    match reqwest::get(format!("{base}/__seo-probe/listings/{slug}")).await {
        Ok(res) => {
            let status = res.status().as_u16();
            (status, res.text().await.unwrap_or_default())
        }
        Err(e) => panic!("GET listing: {e}"),
    }
}

#[tokio::test]
async fn listing_metadata_shares_one_api_call_with_the_page() {
    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/listings/villa-azure"))
        .respond_with(ResponseTemplate::new(200).set_body_json(details_json(
            "villa-azure",
            serde_json::json!([{ "id": "0190a0a0-0000-7000-8000-0000000000aa", "url": PHOTO, "name": "Pool deck" }]),
        )))
        .expect(1)
        .mount(&api)
        .await;
    let (status, html) = get(&api, "villa-azure").await;
    assert_eq!(status, 200, "{html}");
    assert!(
        html.contains("<title>Villa Azure · Our Places</title>"),
        "{html}"
    );
    assert!(
        html.contains(r#"<meta name="description" content="Sea views &amp; a private pool.">"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<link rel="canonical" href="https://ourplaces.io/listings/villa-azure">"#),
        "{html}"
    );
    assert!(
        html.contains(
            r#"<meta property="og:image" content="https://storage.googleapis.com/b/optimized/desktop/l_1.webp">"#
        ),
        "{html}"
    );
    assert!(
        html.contains(r#"<meta property="og:image:alt" content="Pool deck">"#),
        "{html}"
    );
}

#[tokio::test]
async fn a_uuid_url_declares_the_slug_canonical() {
    let api = MockServer::start().await;
    let id = "0190a0a0-0000-7000-8000-000000000001";
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/listings/{id}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(details_json("villa-azure", serde_json::json!([]))),
        )
        .mount(&api)
        .await;
    let (status, html) = get(&api, id).await;
    assert_eq!(status, 200, "{html}");
    assert!(
        html.contains(r#"<link rel="canonical" href="https://ourplaces.io/listings/villa-azure">"#),
        "{html}"
    );
    assert!(!html.contains(&format!("listings/{id}\"")), "{html}");
}

#[tokio::test]
async fn an_unknown_listing_is_still_a_404() {
    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/listings/nope"))
        .respond_with(ResponseTemplate::new(404).set_body_json(
            serde_json::json!({ "code": "NOT_FOUND", "message": "Listing not found" }),
        ))
        .mount(&api)
        .await;
    assert_eq!(get(&api, "nope").await.0, 404);
}

#[tokio::test]
async fn an_api_failure_keeps_its_error_status() {
    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/listings/villa-azure"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&api)
        .await;
    let (status, _) = get(&api, "villa-azure").await;
    assert!(status >= 500, "{status}");
}

/// The single JSON-LD block of `html`, parsed.
fn structured_data(html: &str) -> serde_json::Value {
    let open = r#"<script type="application/ld+json">"#;
    assert_eq!(html.matches(open).count(), 1, "{html}");
    let body = html
        .split(open)
        .nth(1)
        .and_then(|rest| rest.split("</script>").next())
        .unwrap_or_default();
    match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => panic!("JSON-LD does not parse: {e}: {body}"),
    }
}

#[tokio::test]
async fn structured_data_is_rendered_once_with_the_canonical_url() {
    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/listings/villa-azure"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(details_json("villa-azure", serde_json::json!([]))),
        )
        .expect(1)
        .mount(&api)
        .await;
    let (status, html) = get(&api, "villa-azure").await;
    assert_eq!(status, 200, "{html}");
    let ld = structured_data(&html);
    assert_eq!(ld["@type"], "VacationRental");
    assert_eq!(ld["url"], "https://ourplaces.io/listings/villa-azure");
    assert_eq!(ld["description"], "Sea views & a private pool.");
}

#[tokio::test]
async fn structured_data_on_a_uuid_url_uses_the_slug_canonical() {
    let api = MockServer::start().await;
    let id = "0190a0a0-0000-7000-8000-000000000001";
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/listings/{id}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(details_json("villa-azure", serde_json::json!([]))),
        )
        .mount(&api)
        .await;
    let (_, html) = get(&api, id).await;
    let ld = structured_data(&html);
    assert_eq!(ld["url"], "https://ourplaces.io/listings/villa-azure");
    assert_eq!(
        ld["@id"],
        "https://ourplaces.io/listings/villa-azure#rental"
    );
}
