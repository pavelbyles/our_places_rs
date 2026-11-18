//! Client helpers and URL builders for `/api/v1/turnovers` endpoints on `booking_api`.

use super::booking_requests::ApiCallError;
use super::{booking_api_audience, booking_api_url, get_client};
use crate::turnover::{
    CreateManualTurnoverRequest, CreateTurnoverCommentRequest, TransitionTurnoverRequest,
    TurnoverCommentResponse, TurnoverDetailResponse, TurnoverListQuery, TurnoverListResponse,
    TurnoverResponse,
};
use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;

const NO_BODY: Option<&()> = None;

/// Builds the full query URL for `GET /api/v1/turnovers`.
pub fn turnovers_list_url(api_url: &str, query: &TurnoverListQuery) -> String {
    let mut url = format!("{}/api/v1/turnovers", api_url.trim_end_matches('/'));
    let mut params = Vec::new();
    if let Some(window) = query.window {
        params.push(format!("window={}", window.as_str()));
    }
    if let Some(status) = query.status {
        params.push(format!("status={}", status.as_str()));
    }
    if let Some(listing_id) = query.listing_id {
        params.push(format!("listing_id={listing_id}"));
    }
    if let Some(page) = query.page {
        params.push(format!("page={page}"));
    }
    if let Some(per_page) = query.per_page {
        params.push(format!("per_page={per_page}"));
    }
    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }
    url
}

/// Builds the full URL for `POST /api/v1/turnovers`.
pub fn turnovers_create_url(api_url: &str) -> String {
    format!("{}/api/v1/turnovers", api_url.trim_end_matches('/'))
}

/// Builds the full URL for `GET /api/v1/turnovers/{id}`.
pub fn turnover_detail_url(api_url: &str, turnover_id: Uuid) -> String {
    format!(
        "{}/api/v1/turnovers/{turnover_id}",
        api_url.trim_end_matches('/')
    )
}

/// Builds the full URL for `POST /api/v1/turnovers/{id}/status`.
pub fn turnover_status_url(api_url: &str, turnover_id: Uuid) -> String {
    format!(
        "{}/api/v1/turnovers/{turnover_id}/status",
        api_url.trim_end_matches('/')
    )
}

/// Builds the full URL for `POST /api/v1/turnovers/{id}/comments`.
pub fn turnover_comments_url(api_url: &str, turnover_id: Uuid) -> String {
    format!(
        "{}/api/v1/turnovers/{turnover_id}/comments",
        api_url.trim_end_matches('/')
    )
}

async fn call_url<T: DeserializeOwned>(
    method: reqwest::Method,
    url: &str,
    body: Option<&(impl Serialize + ?Sized)>,
    user_jwt: &str,
) -> Result<T> {
    let mut request = get_client()
        .user_request(method, url, &booking_api_audience(), Some(user_jwt))
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
        .with_context(|| format!("Failed to parse turnover response from {url}"))
}

/// Lists scoped turnover summaries at an explicit `booking_api` base URL.
pub async fn list_turnovers_at(
    api_url: &str,
    query: &TurnoverListQuery,
    user_jwt: &str,
) -> Result<TurnoverListResponse> {
    let url = turnovers_list_url(api_url, query);
    call_url(reqwest::Method::GET, &url, NO_BODY, user_jwt).await
}

/// Lists scoped turnover summaries from the configured `booking_api` base URL.
pub async fn list_turnovers(
    query: &TurnoverListQuery,
    user_jwt: &str,
) -> Result<TurnoverListResponse> {
    list_turnovers_at(&booking_api_url(), query, user_jwt).await
}

/// Fetches a single scoped turnover detail at an explicit `booking_api` base URL.
pub async fn get_turnover_detail_at(
    api_url: &str,
    turnover_id: Uuid,
    user_jwt: &str,
) -> Result<TurnoverDetailResponse> {
    let url = turnover_detail_url(api_url, turnover_id);
    call_url(reqwest::Method::GET, &url, NO_BODY, user_jwt).await
}

/// Fetches a single scoped turnover detail from the configured `booking_api` base URL.
pub async fn get_turnover_detail(
    turnover_id: Uuid,
    user_jwt: &str,
) -> Result<TurnoverDetailResponse> {
    get_turnover_detail_at(&booking_api_url(), turnover_id, user_jwt).await
}

/// Creates a manual turnover at an explicit `booking_api` base URL.
pub async fn create_manual_turnover_at(
    api_url: &str,
    req: &CreateManualTurnoverRequest,
    user_jwt: &str,
) -> Result<TurnoverResponse> {
    let url = turnovers_create_url(api_url);
    call_url(reqwest::Method::POST, &url, Some(req), user_jwt).await
}

/// Creates a manual turnover against the configured `booking_api` base URL.
pub async fn create_manual_turnover(
    req: &CreateManualTurnoverRequest,
    user_jwt: &str,
) -> Result<TurnoverResponse> {
    create_manual_turnover_at(&booking_api_url(), req, user_jwt).await
}

/// Transitions a turnover's status at an explicit `booking_api` base URL.
pub async fn transition_turnover_status_at(
    api_url: &str,
    turnover_id: Uuid,
    req: &TransitionTurnoverRequest,
    user_jwt: &str,
) -> Result<TurnoverResponse> {
    let url = turnover_status_url(api_url, turnover_id);
    call_url(reqwest::Method::POST, &url, Some(req), user_jwt).await
}

/// Transitions a turnover's status against the configured `booking_api` base URL.
pub async fn transition_turnover_status(
    turnover_id: Uuid,
    req: &TransitionTurnoverRequest,
    user_jwt: &str,
) -> Result<TurnoverResponse> {
    transition_turnover_status_at(&booking_api_url(), turnover_id, req, user_jwt).await
}

/// Posts a crew/host comment on a turnover at an explicit `booking_api` base URL.
pub async fn post_turnover_comment_at(
    api_url: &str,
    turnover_id: Uuid,
    req: &CreateTurnoverCommentRequest,
    user_jwt: &str,
) -> Result<TurnoverCommentResponse> {
    let url = turnover_comments_url(api_url, turnover_id);
    call_url(reqwest::Method::POST, &url, Some(req), user_jwt).await
}

/// Posts a crew/host comment on a turnover against the configured `booking_api` base URL.
pub async fn post_turnover_comment(
    turnover_id: Uuid,
    req: &CreateTurnoverCommentRequest,
    user_jwt: &str,
) -> Result<TurnoverCommentResponse> {
    post_turnover_comment_at(&booking_api_url(), turnover_id, req, user_jwt).await
}
