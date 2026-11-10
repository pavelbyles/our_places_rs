//! Guest-portal export route tests against a mocked `listing_api`.

use super::calendar_export::{ExportCache, ExportReply, export_reply};
use common::http_client::{AuthenticatedClient, TokenProvider};
use std::sync::Arc;
use uuid::Uuid;
use web_client::TopcoatApiClient;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const ICS: &str = "BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n";

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

fn ics_path(id: Uuid) -> String {
    format!("/api/v1/listings/{id}/calendar.ics")
}

#[tokio::test]
async fn calendar_export_serves_and_caches_the_calendar() {
    let server = MockServer::start().await;
    let id = Uuid::now_v7();
    Mock::given(method("GET"))
        .and(path(ics_path(id)))
        .respond_with(|req: &Request| {
            assert!(!req.headers.contains_key("x-forwarded-authorization"));
            assert!(
                !req.headers.contains_key("if-none-match"),
                "inbound headers are never forwarded"
            );
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_body_string(ICS)
        })
        .expect(1)
        .mount(&server)
        .await;
    let client = client_for(&server);
    let cache = ExportCache::default();
    let first = export_reply(&client, &cache, &id.to_string(), None).await;
    assert_eq!(
        first,
        ExportReply::Calendar {
            body: ICS.into(),
            etag: Some("\"v1\"".into()),
            listing_id: id,
            slug: None,
        }
    );
    // Inside the TTL: no upstream call, and the subscriber's ETag gives 304.
    let second = export_reply(&client, &cache, &id.to_string(), Some("\"v1\"")).await;
    assert_eq!(
        second,
        ExportReply::NotModified {
            etag: Some("\"v1\"".into())
        }
    );
    let third = export_reply(&client, &cache, &id.to_string(), Some("\"old\"")).await;
    assert!(matches!(third, ExportReply::Calendar { .. }));
}

#[tokio::test]
async fn calendar_export_caches_404s_and_refuses_bad_ids() {
    let server = MockServer::start().await;
    let id = Uuid::now_v7();
    Mock::given(method("GET"))
        .and(path(ics_path(id)))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    let client = client_for(&server);
    let cache = ExportCache::default();
    for _ in 0..3 {
        assert_eq!(
            export_reply(&client, &cache, &id.to_string(), None).await,
            ExportReply::NotFound
        );
    }
    assert_eq!(
        export_reply(&client, &cache, "villa-azul", None).await,
        ExportReply::NotFound
    );
}

#[tokio::test]
async fn calendar_export_upstream_errors_are_503_and_not_cached() {
    let server = MockServer::start().await;
    let id = Uuid::now_v7();
    Mock::given(method("GET"))
        .and(path(ics_path(id)))
        .respond_with(ResponseTemplate::new(500))
        .expect(2)
        .mount(&server)
        .await;
    let client = client_for(&server);
    let cache = ExportCache::default();
    for _ in 0..2 {
        assert_eq!(
            export_reply(&client, &cache, &id.to_string(), None).await,
            ExportReply::Unavailable
        );
    }
}

#[tokio::test]
async fn calendar_export_inbound_user_tokens_never_reach_upstream() {
    let server = MockServer::start().await;
    let id = Uuid::now_v7();
    Mock::given(method("GET"))
        .and(path(ics_path(id)))
        .respond_with(|req: &Request| {
            let forwarded = req.headers.contains_key("x-forwarded-authorization");
            ResponseTemplate::new(if forwarded { 418 } else { 200 }).set_body_string(ICS)
        })
        .mount(&server)
        .await;
    // `export_reply` takes no inbound headers but the subscriber's ETag, so
    // nothing else can be forwarded.
    let reply = export_reply(
        &client_for(&server),
        &ExportCache::default(),
        &id.to_string(),
        None,
    )
    .await;
    assert!(matches!(reply, ExportReply::Calendar { .. }));
}
