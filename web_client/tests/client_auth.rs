//! Tests for backend calls made on behalf of a signed-in user.

use common::http_client::AuthenticatedClient;
use uuid::Uuid;
use web_client::client::{TopcoatApiClient, UserCallError};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const JWT: &str = "user-token";

fn client_for(server: &MockServer) -> TopcoatApiClient {
    TopcoatApiClient::new(
        AuthenticatedClient::new(false),
        server.uri(),
        server.uri(),
        server.uri(),
    )
}

#[tokio::test]
async fn user_bookings_send_bearer_to_user_path() -> Result<(), UserCallError> {
    let server = MockServer::start().await;
    let user = Uuid::now_v7();
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/bookings/user/{user}")))
        .and(header("authorization", format!("Bearer {JWT}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let bookings = client_for(&server).get_user_bookings_as(user, JWT).await?;
    assert!(bookings.is_empty());
    Ok(())
}

#[tokio::test]
async fn cancellation_quote_sends_bearer_and_decodes() -> Result<(), UserCallError> {
    let server = MockServer::start().await;
    let id = Uuid::now_v7();
    let body = serde_json::json!({
        "policy_pct": "50", "accommodation_refund": "100.00", "cleaning_refund": "0",
        "deposit_refund": "200.00", "tax_refund": "15.00", "platform_fee_refund": "0",
        "refund_total": "315.00", "retained_total": "115.00"
    });
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/bookings/{id}/cancellation-quote")))
        .and(header("authorization", format!("Bearer {JWT}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;

    let quote = client_for(&server)
        .get_cancellation_quote_as(id, JWT)
        .await?;
    assert_eq!(quote.refund_total.to_string(), "315.00");
    Ok(())
}

#[tokio::test]
async fn booking_lookup_maps_404_to_not_found() {
    let server = MockServer::start().await;
    let id = Uuid::now_v7();
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/bookings/{id}")))
        .and(header("authorization", format!("Bearer {JWT}").as_str()))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;

    let result = client_for(&server).get_booking_as(id, JWT).await;
    assert!(matches!(result, Err(UserCallError::NotFound)));
}

#[tokio::test]
async fn unauthorized_and_server_errors_are_distinct() {
    let server = MockServer::start().await;
    let user = Uuid::now_v7();
    Mock::given(path(format!("/api/v1/bookings/user/{user}")))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let id = Uuid::now_v7();
    Mock::given(path(format!("/api/v1/bookings/{id}")))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let c = client_for(&server);
    assert!(matches!(
        c.get_user_bookings_as(user, JWT).await,
        Err(UserCallError::Unauthorized)
    ));
    assert!(matches!(
        c.get_booking_as(id, JWT).await,
        Err(UserCallError::Status(503))
    ));
}

#[tokio::test]
async fn unavailable_ranges_are_public_and_decoded() -> Result<(), UserCallError> {
    use chrono::NaiveDate;
    let server = MockServer::start().await;
    let listing = Uuid::now_v7();
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v1/bookings/listing/{listing}/unavailable-ranges"
        )))
        .and(wiremock::matchers::query_param("from", "2027-01-01"))
        .and(wiremock::matchers::query_param("to", "2027-03-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ranges": [{"date_from": "2027-01-10", "date_to": "2027-01-14"}]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let d = |m, day| NaiveDate::from_ymd_opt(2027, m, day).unwrap_or_default();
    let ranges = client_for(&server)
        .get_unavailable_ranges(listing, d(1, 1), d(3, 1))
        .await?;
    assert_eq!(ranges.len(), 1);
    assert_eq!(
        (ranges[0].date_from, ranges[0].date_to),
        (d(1, 10), d(1, 14))
    );
    Ok(())
}

#[tokio::test]
async fn user_profile_uses_the_user_route() -> Result<(), UserCallError> {
    let server = MockServer::start().await;
    let id = Uuid::now_v7();
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/users/user/{id}")))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    let result = client_for(&server).get_user_profile(id).await;
    assert!(matches!(result, Err(UserCallError::NotFound)));
    Ok(())
}
