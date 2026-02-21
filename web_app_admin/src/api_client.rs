use common::http_client::AuthenticatedClient;
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::OnceLock;
use thiserror::Error;

static CLIENT: OnceLock<AuthenticatedClient> = OnceLock::new();

/// Returns a global instance of the AuthenticatedClient
pub fn get_client() -> &'static AuthenticatedClient {
    CLIENT.get_or_init(|| {
        let is_cloud = env::var("EA__DATABASE__CLOUD").is_ok() || env::var("K_SERVICE").is_ok();
        AuthenticatedClient::new(is_cloud)
    })
}

pub fn user_api_url() -> String {
    env::var("USER_API_URL")
        .unwrap_or_else(|_| "http://localhost:8083".to_string())
        .trim_end_matches('/')
        .to_string()
}

pub fn listing_api_url() -> String {
    env::var("LISTING_API_URL")
        .unwrap_or_else(|_| "http://localhost:8082".to_string())
        .trim_end_matches('/')
        .to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Request failed with status: {0}")]
    RequestFailed(reqwest::StatusCode),
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
}

pub async fn login(
    email: &str,
    password: &str,
) -> Result<common::models::UserResponse, ClientError> {
    let url = format!("{}/api/v1/users/login", user_api_url());

    let request = LoginRequest {
        email: email.to_string(),
        password: password.to_string(),
    };

    let client = reqwest::Client::new();
    let response = client.post(&url).json(&request).send().await?;

    if !response.status().is_success() {
        return Err(ClientError::RequestFailed(response.status()));
    }

    let user: common::models::UserResponse = response.json().await?;
    Ok(user)
}
