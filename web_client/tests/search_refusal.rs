//! How a search reaches the listing API, and what comes back when it's refused.

use std::sync::{Arc, Mutex};

use common::http_client::{AuthenticatedClient, TokenProvider};
use web_client::client::{ListingSearchParams, ListingSearchRefusal, TopcoatApiClient};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// Records the audience asked for, and stands in for the metadata server.
#[derive(Default)]
struct RecordingToken(Mutex<Vec<String>>);

#[async_trait::async_trait]
impl TokenProvider for RecordingToken {
    async fn get_token(&self, audience: &str) -> anyhow::Result<String> {
        if let Ok(mut audiences) = self.0.lock() {
            audiences.push(audience.to_string());
        }
        Ok("service-id-token".to_string())
    }
}

fn client_for(server: &MockServer, token: Arc<RecordingToken>) -> TopcoatApiClient {
    TopcoatApiClient::new(
        AuthenticatedClient::with_token_provider(token),
        server.uri(),
        server.uri(),
        server.uri(),
    )
}

async fn respond_with(status: u16, body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/listings"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

async fn refusal_for(status: u16, body: serde_json::Value) -> Option<ListingSearchRefusal> {
    let server = respond_with(status, body).await;
    let error = client_for(&server, Arc::default())
        .search_listings(ListingSearchParams::default())
        .await
        .err()?;
    error.downcast_ref::<ListingSearchRefusal>().cloned()
}

#[tokio::test]
async fn sends_the_search_and_leaves_out_empty_values() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/listings"))
        .and(query_param("q", "Runaway Bay"))
        .and(query_param("check_in", "2030-08-01"))
        .and(query_param("guests", "abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let found = client_for(&server, Arc::default())
        .search_listings(ListingSearchParams {
            q: Some("Runaway Bay".to_string()),
            check_in: Some("2030-08-01".to_string()),
            check_out: Some("  ".to_string()),
            guests: Some("abc".to_string()),
            ..Default::default()
        })
        .await?;
    assert!(found.is_empty());

    let requests: Vec<Request> = server.received_requests().await.unwrap_or_default();
    let query = requests
        .first()
        .and_then(|r| r.url.query().map(str::to_string))
        .unwrap_or_default();
    assert!(!query.contains("check_out"), "{query}");
    Ok(())
}

#[tokio::test]
async fn asks_for_a_token_for_the_listing_audience_not_the_url() -> anyhow::Result<()> {
    let server = respond_with(200, serde_json::json!([])).await;
    let token = Arc::new(RecordingToken::default());
    client_for(&server, token.clone())
        .search_listings(ListingSearchParams::default())
        .await?;

    let audiences = token.0.lock().map(|a| a.clone()).unwrap_or_default();
    assert_eq!(audiences, [common::app_client::listing_api_audience()]);
    assert_ne!(audiences[0], server.uri());
    Ok(())
}

#[tokio::test]
async fn a_refusal_for_guests_keeps_its_code_and_message() {
    for code in [
        "INVALID_STAY_DATES",
        "INVALID_GUESTS",
        "INVALID_SEARCH_TEXT",
        "CURRENCY_INACTIVE",
    ] {
        let refusal =
            refusal_for(400, serde_json::json!({"code": code, "message": "Shown."})).await;
        assert_eq!(
            refusal,
            Some(ListingSearchRefusal {
                code: code.to_string(),
                message: "Shown.".to_string()
            })
        );
    }
}

#[tokio::test]
async fn anything_else_stays_an_ordinary_error() {
    for (status, body) in [
        (400, serde_json::json!({"error": "from a raw input"})),
        (
            400,
            serde_json::json!({"code": "SOMETHING_ELSE", "message": "internal"}),
        ),
        (
            409,
            serde_json::json!({"code": "DATE_UNAVAILABLE", "message": "gone"}),
        ),
        (
            500,
            serde_json::json!({"code": "INVALID_GUESTS", "message": "a 5xx is never shown"}),
        ),
    ] {
        assert_eq!(
            refusal_for(status, body.clone()).await,
            None,
            "{status} {body}"
        );
    }
}
