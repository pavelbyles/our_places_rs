//! Users, authentication and portal sessions.

use super::{get_client, user_api_audience, user_api_url};
use crate::http_client::AuthenticatedClient;
use crate::models::{
    CreateSessionRequest, LoginRequest, NewUserRequest, ResendVerificationRequest, SessionResponse,
    UpdateUserRequest, UserResponse, VerifyRequest,
};
use crate::turnover::{PasswordResetRequest, PasswordTokenRedeemRequest};
use anyhow::{Context, Result, bail};
use reqwest::Response;
use uuid::Uuid;

/// Fetches a user's raw profile response.
pub async fn fetch_user_profile(user_id: &str) -> Result<Response> {
    let url = format!("{}/api/v1/users/{}", user_api_url(), user_id);
    let audience = user_api_audience();
    get_client().get(&url, &audience).await
}

/// Retrieves all users with optional pagination, search filtering, and soft-delete filtering.
pub async fn get_all_users_filtered(
    page: Option<u32>,
    per_page: Option<u32>,
    search: Option<String>,
    is_deleted: Option<bool>,
) -> Result<Vec<UserResponse>> {
    let api_url = user_api_url();
    let audience = user_api_audience();
    let page = page.unwrap_or(1);
    let per_page = per_page.unwrap_or(50);
    let mut url = format!(
        "{}/api/v1/users?page={}&per_page={}",
        api_url, page, per_page
    );
    if let Some(s) = search.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&search={}", s));
    }
    if let Some(del) = is_deleted {
        url.push_str(&format!("&is_deleted={}", del));
    } else {
        url.push_str("&all=true");
    }

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to user service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to fetch users ({}): {}", status, err_text);
    }

    res.json::<Vec<UserResponse>>()
        .await
        .context("Failed to parse users response")
}

/// Retrieves active users with pagination and search.
pub async fn get_all_users(
    page: Option<u32>,
    per_page: Option<u32>,
    search: Option<String>,
) -> Result<Vec<UserResponse>> {
    get_all_users_filtered(page, per_page, search, None).await
}

/// Soft deletes a user account (marks as deleted and deactivated).
pub async fn soft_delete_user(id: Uuid) -> Result<UserResponse> {
    let url = format!("{}/api/v1/users/user/{}", user_api_url(), id);
    let audience = user_api_audience();
    let res = get_client()
        .delete(&url, &audience)
        .await
        .context("Failed to connect to user service for soft deletion")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to soft delete user ({}): {}", status, err_text);
    }

    res.json::<UserResponse>()
        .await
        .context("Failed to parse soft deleted user response")
}

/// Permanently purges a user from the database.
pub async fn hard_delete_user(id: Uuid) -> Result<()> {
    let url = format!("{}/api/v1/users/user/{}/hard", user_api_url(), id);
    let audience = user_api_audience();
    let res = get_client()
        .delete(&url, &audience)
        .await
        .context("Failed to connect to user service for hard deletion")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&err_text)
            && let Some(err_msg) = json_val.get("error").and_then(|v| v.as_str())
        {
            bail!("{}", err_msg);
        }
        bail!(
            "Failed to permanently purge user ({}): {}",
            status,
            err_text
        );
    }

    Ok(())
}

/// Restores an archived / soft-deleted user.
pub async fn restore_user(id: Uuid) -> Result<UserResponse> {
    let url = format!("{}/api/v1/users/user/{}/restore", user_api_url(), id);
    let audience = user_api_audience();
    let res = get_client()
        .post(&url, &audience, &serde_json::json!({}))
        .await
        .context("Failed to connect to user service for user restoration")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to restore user ({}): {}", status, err_text);
    }

    res.json::<UserResponse>()
        .await
        .context("Failed to parse restored user response")
}

/// Changes a user.
pub async fn update_user(id: Uuid, req: &UpdateUserRequest) -> Result<UserResponse> {
    let url = format!("{}/api/v1/users/user/{}", user_api_url(), id);
    let audience = user_api_audience();
    let res = get_client()
        .patch(&url, &audience, req)
        .await
        .context("Failed to connect to user service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to update user ({}): {}", status, err_text);
    }

    res.json::<UserResponse>()
        .await
        .context("Failed to parse updated user response")
}

/// Creates a user.
pub async fn create_user(req: &NewUserRequest) -> Result<UserResponse> {
    let url = format!("{}/api/v1/users", user_api_url());
    let audience = user_api_audience();
    let res = get_client()
        .post(&url, &audience, req)
        .await
        .context("Failed to connect to user service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to create user ({}): {}", status, err_text);
    }

    res.json::<UserResponse>()
        .await
        .context("Failed to parse created user response")
}

/// Creates a session against an explicit `user_api` base URL.
pub async fn create_session_at(
    client: &AuthenticatedClient,
    api_url: &str,
    audience: &str,
    req: &CreateSessionRequest,
) -> Result<SessionResponse> {
    let url = format!("{}/api/v1/sessions", api_url.trim_end_matches('/'));
    let res = client
        .post(&url, audience, req)
        .await
        .context("Failed to connect to user service for session creation")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to create session ({}): {}", status, err_text);
    }

    res.json::<SessionResponse>()
        .await
        .context("Failed to parse session creation response")
}

/// Creates a session.
pub async fn create_session(req: &CreateSessionRequest) -> Result<SessionResponse> {
    create_session_at(get_client(), &user_api_url(), &user_api_audience(), req).await
}

/// Fetches a session against an explicit `user_api` base URL.
pub async fn get_session_at(
    client: &AuthenticatedClient,
    api_url: &str,
    audience: &str,
    token_hash: &str,
    namespace: Option<&str>,
) -> Result<Option<SessionResponse>> {
    let mut url = format!(
        "{}/api/v1/sessions/{}",
        api_url.trim_end_matches('/'),
        token_hash
    );
    if let Some(ns) = namespace {
        url.push_str(&format!("?namespace={}", ns));
    }

    let res = client
        .get(&url, audience)
        .await
        .context("Failed to connect to user service for session lookup")?;

    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to fetch session ({}): {}", status, err_text);
    }

    let session = res
        .json::<SessionResponse>()
        .await
        .context("Failed to parse session response")?;

    Ok(Some(session))
}

/// Fetches a session if it exists, optionally only from one portal.
pub async fn get_session(
    token_hash: &str,
    namespace: Option<&str>,
) -> Result<Option<SessionResponse>> {
    get_session_at(
        get_client(),
        &user_api_url(),
        &user_api_audience(),
        token_hash,
        namespace,
    )
    .await
}

/// Deletes a session.
pub async fn delete_session(token_hash: &str) -> Result<()> {
    let url = format!("{}/api/v1/sessions/{}", user_api_url(), token_hash);
    let audience = user_api_audience();

    let res = get_client()
        .delete(&url, &audience)
        .await
        .context("Failed to connect to user service for session deletion")?;

    if !res.status().is_success() && res.status() != reqwest::StatusCode::NOT_FOUND {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to delete session ({}): {}", status, err_text);
    }

    Ok(())
}

/// Ends a user's sessions, optionally only in one portal.
pub async fn revoke_user_sessions(user_id: Uuid, namespace: Option<&str>) -> Result<()> {
    let api_url = user_api_url();
    let audience = user_api_audience();
    let mut url = format!("{}/api/v1/users/{}/sessions", api_url, user_id);
    if let Some(ns) = namespace {
        url.push_str(&format!("?namespace={}", ns));
    }

    let res = get_client()
        .delete(&url, &audience)
        .await
        .context("Failed to connect to user service for user session revocation")?;

    if !res.status().is_success() && res.status() != reqwest::StatusCode::NOT_FOUND {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to revoke user sessions ({}): {}", status, err_text);
    }

    Ok(())
}

/// Checks a user's email and password against an explicit `user_api` base URL.
pub async fn login_user_at(
    client: &AuthenticatedClient,
    api_url: &str,
    audience: &str,
    req: &LoginRequest,
) -> Result<UserResponse> {
    let url = format!("{}/api/v1/users/login", api_url.trim_end_matches('/'));
    let res = client
        .post(&url, audience, req)
        .await
        .context("Failed to connect to user service for login")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Login failed ({}): {}", status, err_text);
    }

    let user: UserResponse = res
        .json()
        .await
        .context("Failed to parse user response from login")?;

    Ok(user)
}

/// Checks a user's email and password.
pub async fn login_user(req: &LoginRequest) -> Result<UserResponse> {
    login_user_at(get_client(), &user_api_url(), &user_api_audience(), req).await
}

/// Verifies a user's email address with their OTP code against an explicit `user_api` base URL.
pub async fn verify_user_at(
    client: &AuthenticatedClient,
    api_url: &str,
    audience: &str,
    req: &VerifyRequest,
) -> Result<UserResponse> {
    let url = format!("{}/api/v1/users/verify", api_url.trim_end_matches('/'));
    let res = client
        .post(&url, audience, req)
        .await
        .context("Failed to connect to user service for verification")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Verification failed ({}): {}", status, err_text);
    }

    let user: UserResponse = res
        .json()
        .await
        .context("Failed to parse user response from verification")?;

    Ok(user)
}

/// Verifies a user's email address with their 6-character OTP code.
pub async fn verify_user(req: &VerifyRequest) -> Result<UserResponse> {
    verify_user_at(get_client(), &user_api_url(), &user_api_audience(), req).await
}

/// Resends the 6-character OTP verification code to a user's email address.
pub async fn resend_verification(req: &ResendVerificationRequest) -> Result<UserResponse> {
    let api_url = user_api_url();
    let audience = user_api_audience();
    let url = format!("{}/api/v1/users/resend-verification", api_url);

    let res = get_client()
        .post(&url, &audience, req)
        .await
        .context("Failed to connect to user service for resending verification")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Resend verification failed ({}): {}", status, err_text);
    }

    let user: UserResponse = res
        .json()
        .await
        .context("Failed to parse user response from resend verification")?;

    Ok(user)
}

/// Initiates a single-use password-reset email link for an active account (`202 Accepted`).
pub async fn request_password_reset(req: &PasswordResetRequest) -> Result<()> {
    let url = format!("{}/api/v1/users/password-reset/request", user_api_url());
    let audience = user_api_audience();
    let res = get_client()
        .post(&url, &audience, req)
        .await
        .context("Failed to connect to user service for password reset request")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Password reset request failed ({}): {}", status, err_text);
    }

    Ok(())
}

/// Redeems a single-use invite or password-reset token to set a new password.
pub async fn confirm_password_reset(req: &PasswordTokenRedeemRequest) -> Result<()> {
    let url = format!("{}/api/v1/users/password-reset/confirm", user_api_url());
    let audience = user_api_audience();
    let res = get_client()
        .post(&url, &audience, req)
        .await
        .context("Failed to connect to user service for password reset confirmation")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!(
            "Password reset confirmation failed ({}): {}",
            status,
            err_text
        );
    }

    Ok(())
}
