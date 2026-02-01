use chrono::{DateTime, Utc};
use common::http_client::AuthenticatedClient;
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::OnceLock;
use uuid::Uuid;

static CLIENT: OnceLock<AuthenticatedClient> = OnceLock::new();

/// Returns a global instance of the AuthenticatedClient
pub fn get_client() -> &'static AuthenticatedClient {
    CLIENT.get_or_init(|| {
        let is_cloud = env::var("EA__DATABASE__CLOUD").is_ok() || env::var("K_SERVICE").is_ok();
        AuthenticatedClient::new(is_cloud)
    })
}

fn user_api_url() -> String {
    env::var("USER_API_URL").unwrap_or_else(|_| "http://localhost:8083".to_string())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub phone_number: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub attributes: serde_json::Value,
}

pub async fn login(email: &str, password: &str) -> anyhow::Result<UserResponse> {
    let url = format!("{}/api/v1/users/login", user_api_url());

    let request = LoginRequest {
        email: email.to_string(),
        password: password.to_string(),
    };

    let client = reqwest::Client::new();
    let response = client.post(&url).json(&request).send().await?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Login failed: {}", response.status()));
    }

    let user: UserResponse = response.json().await?;
    Ok(user)
}
