//! Users, authentication and portal sessions.

use super::*;

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

pub async fn create_session(req: &CreateSessionRequest) -> Result<SessionResponse> {
    let url = format!("{}/api/v1/sessions", user_api_url());
    let audience = user_api_audience();

    let res = get_client()
        .post(&url, &audience, req)
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

pub async fn get_session(
    token_hash: &str,
    namespace: Option<&str>,
) -> Result<Option<SessionResponse>> {
    let api_url = user_api_url();
    let audience = user_api_audience();
    let mut url = format!("{}/api/v1/sessions/{}", api_url, token_hash);
    if let Some(ns) = namespace {
        url.push_str(&format!("?namespace={}", ns));
    }

    let res = get_client()
        .get(&url, &audience)
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

pub async fn login_user(req: &LoginRequest) -> Result<UserResponse> {
    let api_url = user_api_url();
    let audience = user_api_audience();
    let url = format!("{}/api/v1/users/login", api_url);

    let res = get_client()
        .post(&url, &audience, req)
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

/// Verifies a user's email address with their 6-character OTP code.
pub async fn verify_user(req: &VerifyRequest) -> Result<UserResponse> {
    let api_url = user_api_url();
    let audience = user_api_audience();
    let url = format!("{}/api/v1/users/verify", api_url);

    let res = get_client()
        .post(&url, &audience, req)
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
