//! Enquiries, requests to book, conversations and the inbox (#105).
//!
//! Failed calls return an [`ApiCallError`] inside the `anyhow::Error`, so the
//! portals can read the typed `code` with [`api_error_code`] and show a
//! friendly message instead of the raw text.

use super::{booking_api_audience, booking_api_url, get_client};
use crate::booking_request::{
    BookingRequestList, BookingRequestResponse, CheckoutRequestBody, CreateBookingRequestBody,
    CreateEnquiryBody, DeclineRequestBody, EnquiryResponse, InboxQuery, InboxResponse,
    PriceBreakdown, QuoteRequestBody,
};
use crate::models::{
    BookingMessageResponse, BookingMessagesWrapper, BookingResponse, CreateBookingMessageRequest,
    MarkMessagesReadResponse,
};
use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;

/// A refused or failed API call, with the typed error code when the API
/// gave one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ApiCallError {
    /// HTTP status.
    pub status: u16,
    /// The API's error code, such as `DATE_UNAVAILABLE`.
    pub code: Option<String>,
    /// The API's message, or the raw body.
    pub message: String,
}

impl ApiCallError {
    /// Builds the error from a failed response's status and body text.
    pub fn from_body(status: u16, body: &str) -> Self {
        let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
        let field = |name: &str| {
            parsed
                .as_ref()
                .and_then(|v| v.get(name))
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
        };
        Self {
            status,
            code: field("code"),
            message: field("message").unwrap_or_else(|| body.to_string()),
        }
    }
}

/// The typed error code of a failed call made through this module, if any.
pub fn api_error_code(err: &anyhow::Error) -> Option<&str> {
    err.downcast_ref::<ApiCallError>()
        .and_then(|e| e.code.as_deref())
}

async fn call<T: DeserializeOwned>(
    method: reqwest::Method,
    path: &str,
    body: Option<&(impl Serialize + ?Sized)>,
    user_jwt: &str,
) -> Result<T> {
    let url = format!("{}{path}", booking_api_url());
    let mut request = get_client()
        .user_request(method, &url, &booking_api_audience(), Some(user_jwt))
        .await?
        .header("Accept", "application/json");
    if let Some(body) = body {
        request = request.json(body);
    }
    let res = request
        .send()
        .await
        .context("Failed to connect to booking service")?;
    let status = res.status();
    if !status.is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(ApiCallError::from_body(status.as_u16(), &text).into());
    }
    res.json::<T>()
        .await
        .with_context(|| format!("Failed to parse response from {path}"))
}

const NO_BODY: Option<&()> = None;

/// Submits a request to book.
pub async fn create_booking_request(
    body: &CreateBookingRequestBody,
    user_jwt: &str,
) -> Result<BookingRequestResponse> {
    call(
        reqwest::Method::POST,
        "/api/v1/booking-requests",
        Some(body),
        user_jwt,
    )
    .await
}

/// A request, for its guest, the host or an admin.
pub async fn get_booking_request(id: Uuid, user_jwt: &str) -> Result<BookingRequestResponse> {
    call(
        reqwest::Method::GET,
        &format!("/api/v1/booking-requests/{id}"),
        NO_BODY,
        user_jwt,
    )
    .await
}

/// The signed-in guest's requests and enquiries.
pub async fn my_booking_requests(user_jwt: &str) -> Result<BookingRequestList> {
    call(
        reqwest::Method::GET,
        "/api/v1/booking-requests/mine",
        NO_BODY,
        user_jwt,
    )
    .await
}

/// The host or an admin accepts a request.
pub async fn accept_booking_request(id: Uuid, user_jwt: &str) -> Result<BookingRequestResponse> {
    let path = format!("/api/v1/booking-requests/{id}/accept");
    call(
        reqwest::Method::POST,
        &path,
        Some(&serde_json::json!({})),
        user_jwt,
    )
    .await
}

/// The host or an admin declines a request, or withdraws an offer.
pub async fn decline_booking_request(
    id: Uuid,
    body: &DeclineRequestBody,
    user_jwt: &str,
) -> Result<BookingRequestResponse> {
    let path = format!("/api/v1/booking-requests/{id}/decline");
    call(reqwest::Method::POST, &path, Some(body), user_jwt).await
}

/// The guest withdraws their request or offer.
pub async fn withdraw_booking_request(id: Uuid, user_jwt: &str) -> Result<BookingRequestResponse> {
    let path = format!("/api/v1/booking-requests/{id}/withdraw");
    call(
        reqwest::Method::POST,
        &path,
        Some(&serde_json::json!({})),
        user_jwt,
    )
    .await
}

/// The guest takes up an offer; it becomes a pending hold.
pub async fn checkout_booking_request(
    id: Uuid,
    quote_id: Option<Uuid>,
    user_jwt: &str,
) -> Result<BookingResponse> {
    let path = format!("/api/v1/booking-requests/{id}/checkout");
    call(
        reqwest::Method::POST,
        &path,
        Some(&CheckoutRequestBody { quote_id }),
        user_jwt,
    )
    .await
}

/// Prices a custom quote without sending it.
pub async fn preview_quote(
    id: Uuid,
    body: &QuoteRequestBody,
    user_jwt: &str,
) -> Result<PriceBreakdown> {
    let path = format!("/api/v1/booking-requests/{id}/quote-preview");
    call(reqwest::Method::POST, &path, Some(body), user_jwt).await
}

/// Sends a custom quote on a request.
pub async fn send_quote(
    id: Uuid,
    body: &QuoteRequestBody,
    user_jwt: &str,
) -> Result<BookingRequestResponse> {
    let path = format!("/api/v1/booking-requests/{id}/quote");
    call(reqwest::Method::POST, &path, Some(body), user_jwt).await
}

/// Sends an enquiry.
pub async fn create_enquiry(body: &CreateEnquiryBody, user_jwt: &str) -> Result<EnquiryResponse> {
    call(
        reqwest::Method::POST,
        "/api/v1/enquiries",
        Some(body),
        user_jwt,
    )
    .await
}

/// An enquiry, for its guest, the host or an admin.
pub async fn get_enquiry(id: Uuid, user_jwt: &str) -> Result<EnquiryResponse> {
    call(
        reqwest::Method::GET,
        &format!("/api/v1/enquiries/{id}"),
        NO_BODY,
        user_jwt,
    )
    .await
}

/// The host or an admin closes an enquiry.
pub async fn close_enquiry(id: Uuid, user_jwt: &str) -> Result<EnquiryResponse> {
    let path = format!("/api/v1/enquiries/{id}/close");
    call(
        reqwest::Method::POST,
        &path,
        Some(&serde_json::json!({})),
        user_jwt,
    )
    .await
}

/// The host or an admin quotes straight on an enquiry.
pub async fn quote_enquiry(
    id: Uuid,
    body: &QuoteRequestBody,
    user_jwt: &str,
) -> Result<BookingRequestResponse> {
    let path = format!("/api/v1/enquiries/{id}/quote");
    call(reqwest::Method::POST, &path, Some(body), user_jwt).await
}

/// A conversation's messages.
pub async fn get_conversation_messages(id: Uuid, user_jwt: &str) -> Result<BookingMessagesWrapper> {
    let path = format!("/api/v1/conversations/{id}/messages");
    call(reqwest::Method::GET, &path, NO_BODY, user_jwt).await
}

/// Posts a message to a conversation.
pub async fn post_conversation_message(
    id: Uuid,
    text: &str,
    user_jwt: &str,
) -> Result<BookingMessageResponse> {
    let path = format!("/api/v1/conversations/{id}/messages");
    let body = CreateBookingMessageRequest {
        message_text: text.to_string(),
    };
    call(reqwest::Method::POST, &path, Some(&body), user_jwt).await
}

/// Marks the other party's messages read.
pub async fn mark_conversation_read(id: Uuid, user_jwt: &str) -> Result<MarkMessagesReadResponse> {
    let path = format!("/api/v1/conversations/{id}/messages/read");
    call(reqwest::Method::PATCH, &path, NO_BODY, user_jwt).await
}

/// The inbox (a host's own listings, or all for an admin).
pub async fn get_inbox(query: &InboxQuery, user_jwt: &str) -> Result<InboxResponse> {
    let mut params = Vec::new();
    if let Some(kind) = &query.kind {
        params.push(format!("kind={kind}"));
    }
    if let Some(listing) = query.listing_id {
        params.push(format!("listing_id={listing}"));
    }
    if let Some(page) = query.page {
        params.push(format!("page={page}"));
    }
    let path = if params.is_empty() {
        "/api/v1/inbox".to_string()
    } else {
        format!("/api/v1/inbox?{}", params.join("&"))
    };
    call(reqwest::Method::GET, &path, NO_BODY, user_jwt).await
}

#[cfg(test)]
mod app_client_requests {
    use super::*;
    use crate::booking_request::RequestRefusal;

    #[test]
    fn app_client_requests_error_bodies_keep_their_code() {
        let err = ApiCallError::from_body(
            409,
            r#"{"code":"REQUEST_ALREADY_LIVE","message":"You already have an open request for this property."}"#,
        );
        assert_eq!(err.code.as_deref(), Some("REQUEST_ALREADY_LIVE"));
        assert_eq!(
            err.to_string(),
            "You already have an open request for this property."
        );
        let wrapped: anyhow::Error = err.into();
        assert_eq!(
            api_error_code(&wrapped).and_then(RequestRefusal::from_code),
            Some(RequestRefusal::RequestAlreadyLive)
        );
    }

    #[test]
    fn app_client_requests_bodies_without_a_code_keep_the_text() {
        let err = ApiCallError::from_body(500, "upstream exploded");
        assert_eq!(
            (err.code, err.message.as_str()),
            (None, "upstream exploded")
        );
        let other = anyhow::anyhow!("network down");
        assert_eq!(api_error_code(&other), None);
    }
}
