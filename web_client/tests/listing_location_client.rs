//! `get_listing_location_as` (spec 103 §2.2): sends the user token and maps
//! each answer to a typed result.

use std::sync::Arc;

use common::http_client::{AuthenticatedClient, TokenProvider};
use common::location::{ListingLocationResponse, LocationPrivacy};
use uuid::Uuid;
use web_client::client::{TopcoatApiClient, UserCallError};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const JWT: &str = "user-token";

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

async fn answering(status: u16, body: serde_json::Value) -> (MockServer, Uuid) {
    let server = MockServer::start().await;
    let listing = Uuid::now_v7();
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/listings/{listing}/location")))
        .and(header(
            "x-forwarded-authorization",
            format!("Bearer {JWT}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;
    (server, listing)
}

#[tokio::test]
async fn the_stored_location_comes_back_with_the_user_token() {
    let (server, listing) = answering(
        200,
        serde_json::json!({"latitude": 18.40741, "longitude": -77.10313, "location_privacy": "exact"}),
    )
    .await;
    let found = client_for(&server)
        .get_listing_location_as(listing, JWT)
        .await;
    match found {
        Ok(location) => assert_eq!(
            location,
            ListingLocationResponse {
                latitude: Some(18.40741),
                longitude: Some(-77.10313),
                location_privacy: LocationPrivacy::Exact,
            }
        ),
        Err(e) => panic!("{e}"),
    }
}

#[tokio::test]
async fn refusals_and_failures_are_typed() {
    for (status, expected) in [
        (403, "unauthorized"),
        (404, "not found"),
        (500, "status 500"),
    ] {
        let (server, listing) =
            answering(status, serde_json::json!({"code": "X", "message": "no"})).await;
        let found = client_for(&server)
            .get_listing_location_as(listing, JWT)
            .await;
        match (status, found) {
            (403, Err(UserCallError::Unauthorized))
            | (404, Err(UserCallError::NotFound))
            | (500, Err(UserCallError::Status(500))) => {}
            (_, other) => panic!("{status}: expected {expected}, got {other:?}"),
        }
    }
}
