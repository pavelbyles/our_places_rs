//! `/sitemap.xml` (spec 102 §1.2) served over HTTP, built with Topcoat's
//! `Sitemap`, against a mocked `listing_api`.

use std::sync::Arc;

use common::http_client::{AuthenticatedClient, TokenProvider};
use tokio::net::TcpListener;
use topcoat::{
    context::BaseUrl,
    router::{Router, RouterBuilderDiscoverExt},
};
use web_client::TopcoatApiClient;
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ORIGIN: &str = "https://ourplaces.io";

struct FixedIdToken;

#[async_trait::async_trait]
impl TokenProvider for FixedIdToken {
    async fn get_token(&self, _audience: &str) -> anyhow::Result<String> {
        Ok("service-id-token".to_string())
    }
}

fn client_for(server: &MockServer) -> TopcoatApiClient {
    TopcoatApiClient::new(
        AuthenticatedClient::with_token_provider(Arc::new(FixedIdToken)),
        server.uri(),
        server.uri(),
        server.uri(),
    )
}

/// The guest router (pages and routes) with the mock API and the origin.
async fn serve(api: &MockServer) -> String {
    let _ = crate::pages::seo::sitemap;
    let router = match BaseUrl::new(ORIGIN) {
        Ok(base) => Router::builder()
            .discover()
            .with(base)
            .with(client_for(api))
            .build(),
        Err(e) => panic!("origin: {e}"),
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.ok();
    let addr = listener.as_ref().and_then(|l| l.local_addr().ok());
    if let Some(listener) = listener {
        tokio::spawn(async move {
            let _ = topcoat::serve(listener, router).await;
        });
    }
    addr.map(|a| format!("http://{a}")).unwrap_or_default()
}

/// A listing as `GET /api/v1/listings` returns it.
fn listing(slug: &str, last_modified: Option<&str>) -> serde_json::Value {
    let mut json = serde_json::json!({
        "id": uuid::Uuid::now_v7(),
        "user_id": uuid::Uuid::now_v7(),
        "name": format!("Villa {slug}"),
        "listing_structure": "Villa",
        "country": "Jamaica",
        "is_active": true,
        "added_at": "2026-01-01T00:00:00Z",
        "max_guests": 6, "bedrooms": 3, "beds": 3,
        "full_bathrooms": 2, "half_bathrooms": 0,
        "base_currency": "USD", "slug": slug,
        "minimum_stay": 2, "days_between_bookings": 0
    });
    if let Some(at) = last_modified {
        json["last_modified_at"] = serde_json::json!(at);
    }
    json
}

fn page_of(n: usize, prefix: &str) -> serde_json::Value {
    serde_json::Value::Array(
        (0..n)
            .map(|i| listing(&format!("{prefix}-{i}"), None))
            .collect(),
    )
}

async fn mock_page(api: &MockServer, page: u32, body: serde_json::Value, times: u64) {
    Mock::given(method("GET"))
        .and(path("/api/v1/listings"))
        .and(query_param("page", page.to_string()))
        .and(query_param("per_page", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(times)
        .mount(api)
        .await;
}

struct Reply {
    status: u16,
    content_type: String,
    cache_control: String,
    retry_after: Option<String>,
    body: String,
}

async fn get_sitemap(api: &MockServer) -> Reply {
    let base = serve(api).await;
    let res = match reqwest::get(format!("{base}/sitemap.xml")).await {
        Ok(r) => r,
        Err(e) => panic!("GET /sitemap.xml: {e}"),
    };
    let header = |name: &str| {
        res.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let (status, content_type, cache_control, retry_after) = (
        res.status().as_u16(),
        header("content-type").unwrap_or_default(),
        header("cache-control").unwrap_or_default(),
        header("retry-after"),
    );
    Reply {
        status,
        content_type,
        cache_control,
        retry_after,
        body: res.text().await.unwrap_or_default(),
    }
}

/// Every `<loc>` in document order (entities resolved), failing on
/// malformed XML. quick-xml reports `&amp;` etc. as separate events.
fn locs(xml: &str) -> Vec<String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    let (mut out, mut current): (Vec<String>, Option<String>) = (Vec::new(), None);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == b"loc" => current = Some(String::new()),
            Ok(Event::Text(t)) => {
                if let Some(loc) = current.as_mut() {
                    loc.push_str(&t.decode().map(|s| s.into_owned()).unwrap_or_default());
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if let Some(loc) = current.as_mut() {
                    let name = r.decode().map(|s| s.into_owned()).unwrap_or_default();
                    match quick_xml::escape::resolve_predefined_entity(&name) {
                        Some(text) => loc.push_str(text),
                        None => panic!("unknown entity &{name}; in {xml}"),
                    }
                }
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"loc" => out.extend(current.take()),
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => panic!("malformed sitemap XML: {e}\n{xml}"),
        }
    }
    out
}

const STATIC_LOCS: [&str; 7] = [
    "https://ourplaces.io/",
    "https://ourplaces.io/listings",
    "https://ourplaces.io/about",
    "https://ourplaces.io/terms",
    "https://ourplaces.io/cancellation-policy",
    "https://ourplaces.io/privacy-policy",
    "https://ourplaces.io/house-rules",
];

#[tokio::test]
async fn an_empty_site_lists_the_static_pages() {
    let api = MockServer::start().await;
    mock_page(&api, 1, page_of(0, "none"), 1).await;
    let reply = get_sitemap(&api).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert!(
        reply.content_type.starts_with("application/xml"),
        "{}",
        reply.content_type
    );
    assert_eq!(reply.cache_control, "public, max-age=3600");
    assert!(
        reply
            .body
            .contains("http://www.sitemaps.org/schemas/sitemap/0.9")
    );
    assert_eq!(locs(&reply.body), STATIC_LOCS);
}

#[tokio::test]
async fn listings_are_absolute_sorted_deduplicated_and_dated() {
    let api = MockServer::start().await;
    let body = serde_json::json!([
        listing("villa-b", None),
        listing("villa-a", Some("2026-11-16T12:30:00Z")),
        listing("villa-b", None),
    ]);
    mock_page(&api, 1, body, 1).await;
    let reply = get_sitemap(&api).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let found = locs(&reply.body);
    assert_eq!(
        &found[STATIC_LOCS.len()..],
        [
            "https://ourplaces.io/listings/villa-a",
            "https://ourplaces.io/listings/villa-b"
        ]
    );
    assert!(
        reply.body.contains(
            "<loc>https://ourplaces.io/listings/villa-a</loc><lastmod>2026-11-16T12:30:00Z</lastmod>"
        ),
        "{}",
        reply.body
    );
    assert!(
        reply
            .body
            .contains("<loc>https://ourplaces.io/listings/villa-b</loc></url>"),
        "villa-b has no lastmod: {}",
        reply.body
    );
}

#[tokio::test]
async fn a_hostile_slug_is_escaped() {
    let api = MockServer::start().await;
    mock_page(&api, 1, serde_json::json!([listing("a&b<c", None)]), 1).await;
    let reply = get_sitemap(&api).await;
    assert!(
        reply.body.contains("/listings/a&amp;b&lt;c</loc>"),
        "{}",
        reply.body
    );
    assert!(locs(&reply.body).contains(&"https://ourplaces.io/listings/a&b<c".to_string()));
}

#[tokio::test]
async fn a_full_page_fetches_the_next_one() {
    let api = MockServer::start().await;
    mock_page(&api, 1, page_of(100, "p1"), 1).await;
    mock_page(&api, 2, page_of(1, "p2"), 1).await;
    let reply = get_sitemap(&api).await;
    assert_eq!(reply.status, 200);
    assert_eq!(locs(&reply.body).len(), STATIC_LOCS.len() + 101);
}

#[tokio::test]
async fn the_sitemap_asks_only_for_public_listings() {
    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/listings"))
        .and(query_param_is_missing("include_inactive"))
        .and(query_param_is_missing("user_id"))
        .and(query_param_is_missing("host"))
        .and(query_param_is_missing("owner"))
        .and(query_param_is_missing("currency"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page_of(0, "x")))
        .expect(1)
        .mount(&api)
        .await;
    assert_eq!(get_sitemap(&api).await.status, 200);
}

#[tokio::test]
async fn an_api_failure_is_a_503_never_a_partial_sitemap() {
    let api = MockServer::start().await;
    mock_page(&api, 1, page_of(100, "p1"), 1).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/listings"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&api)
        .await;
    let reply = get_sitemap(&api).await;
    assert_eq!(reply.status, 503);
    assert_eq!(reply.retry_after.as_deref(), Some("300"));
    assert_eq!(reply.cache_control, "no-store");
    assert!(!reply.body.contains("<urlset"), "{}", reply.body);
}

#[tokio::test]
async fn reaching_the_page_cap_is_a_503() {
    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/listings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page_of(100, "full")))
        .expect(50)
        .mount(&api)
        .await;
    let reply = get_sitemap(&api).await;
    assert_eq!(reply.status, 503);
    assert!(!reply.body.contains("<urlset"));
}
