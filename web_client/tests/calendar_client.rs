//! Calendar client calls: management sends the user token; the export feed
//! sends only the service identity.

use std::sync::Arc;

use common::http_client::{AuthenticatedClient, TokenProvider};
use uuid::Uuid;
use web_client::calendar::IcsFetch;
use web_client::client::{TopcoatApiClient, UserCallError};
use wiremock::matchers::{header, header_exists, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const JWT: &str = "user-token";
const ID_TOKEN: &str = "service-id-token";

struct FixedIdToken;

#[async_trait::async_trait]
impl TokenProvider for FixedIdToken {
    async fn get_token(&self, _audience: &str) -> anyhow::Result<String> {
        Ok(ID_TOKEN.to_string())
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

#[tokio::test]
async fn management_calls_send_the_user_token() -> Result<(), UserCallError> {
    let server = MockServer::start().await;
    let listing = Uuid::now_v7();
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/listings/{listing}/calendar/feeds")))
        .and(header(
            "authorization",
            format!("Bearer {ID_TOKEN}").as_str(),
        ))
        .and(header(
            "x-forwarded-authorization",
            format!("Bearer {JWT}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/api/v1/listings/{listing}/calendar/sync")))
        .and(header(
            "x-forwarded-authorization",
            format!("Bearer {JWT}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
            "code": "CALENDAR_SYNC_RATE_LIMITED", "message": "too soon", "retry_after_secs": 40
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client = client_for(&server);
    assert!(client.list_calendar_feeds(listing, JWT).await?.is_empty());
    match client.sync_calendar(listing, JWT).await {
        Err(UserCallError::Api(rejection)) => {
            assert_eq!(rejection.status, 429);
            assert_eq!(rejection.code, "CALENDAR_SYNC_RATE_LIMITED");
        }
        other => panic!("expected a 429 rejection, got {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn the_export_feed_sends_no_user_token() -> Result<(), UserCallError> {
    let server = MockServer::start().await;
    let listing = Uuid::now_v7();
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/listings/{listing}/calendar.ics")))
        .and(header(
            "authorization",
            format!("Bearer {ID_TOKEN}").as_str(),
        ))
        .respond_with(|req: &Request| {
            assert!(
                !req.headers.contains_key("x-forwarded-authorization"),
                "no user token on the export call"
            );
            ResponseTemplate::new(200)
                .insert_header("etag", "\"abc\"")
                .set_body_string("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n")
        })
        .expect(1)
        .mount(&server)
        .await;
    let fetched = client_for(&server).get_calendar_ics(listing, None).await?;
    assert_eq!(
        fetched,
        IcsFetch::Ok {
            body: "BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n".into(),
            etag: Some("\"abc\"".into())
        }
    );
    Ok(())
}

#[tokio::test]
async fn the_export_maps_304_and_404() -> Result<(), UserCallError> {
    let server = MockServer::start().await;
    let listing = Uuid::now_v7();
    let gone = Uuid::now_v7();
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/listings/{listing}/calendar.ics")))
        .and(header_exists("if-none-match"))
        .respond_with(ResponseTemplate::new(304).insert_header("etag", "\"abc\""))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/listings/{gone}/calendar.ics")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let client = client_for(&server);
    assert_eq!(
        client.get_calendar_ics(listing, Some("\"abc\"")).await?,
        IcsFetch::NotModified {
            etag: Some("\"abc\"".into())
        }
    );
    assert_eq!(
        client.get_calendar_ics(gone, None).await?,
        IcsFetch::NotFound
    );
    Ok(())
}
