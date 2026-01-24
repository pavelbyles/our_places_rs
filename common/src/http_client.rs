use anyhow::{Context, Result};
use reqwest::header::AUTHORIZATION;
use reqwest::Client;

use std::process::Command;
use std::sync::Arc;

/// A trait for fetching OIDC Identity Tokens.
#[async_trait::async_trait]
pub trait TokenProvider: Send + Sync {
    async fn get_token(&self, audience: &str) -> Result<String>;
}

/// Fetches tokens from the Google Cloud Metadata Server (for Cloud Run).
pub struct GoogleMetadataTokenProvider;

#[async_trait::async_trait]
impl TokenProvider for GoogleMetadataTokenProvider {
    async fn get_token(&self, audience: &str) -> Result<String> {
        let client = Client::new();
        // Google Metadata Server URL for ID tokens
        let url = format!(
            "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/identity?audience={}",
            audience
        );

        let response = client
            .get(&url)
            .header("Metadata-Flavor", "Google")
            .send()
            .await
            .context("Failed to connect to Metadata Server")?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Metadata Server returned error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            );
        }

        let token = response
            .text()
            .await
            .context("Failed to read token from Metadata Server")?;
        Ok(token)
    }
}

/// Fetches tokens using the `gcloud` CLI (for local development).
pub struct LocalGcloudTokenProvider;

#[async_trait::async_trait]
impl TokenProvider for LocalGcloudTokenProvider {
    async fn get_token(&self, audience: &str) -> Result<String> {
        // Runs: gcloud auth print-identity-token --audiences=...
        let output = Command::new("gcloud")
            .arg("auth")
            .arg("print-identity-token")
            .arg(format!("--audiences={}", audience))
            .output()
            .context("Failed to execute gcloud command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("gcloud command failed: {}", stderr);
        }

        let token = String::from_utf8(output.stdout)
            .context("Invalid UTF-8 in gcloud output")?
            .trim()
            .to_string();
        Ok(token)
    }
}

/// An HTTP client that automatically attaches Authorization headers.
#[derive(Clone)]
pub struct AuthenticatedClient {
    client: Client,
    token_provider: Arc<dyn TokenProvider>,
}

impl AuthenticatedClient {
    /// Creates a new authenticated client.
    /// Detects environment: if `EA__DATABASE__CLOUD` (or generic `RUN_ENV=Production`) is set or metadata server is reachable, uses GoogleMetadataTokenProvider.
    /// Otherwise defaults to LocalGcloudTokenProvider.
    pub fn new(is_cloud: bool) -> Self {
        let token_provider: Arc<dyn TokenProvider> = if is_cloud {
            Arc::new(GoogleMetadataTokenProvider)
        } else {
            Arc::new(LocalGcloudTokenProvider)
        };

        Self {
            client: Client::new(),
            token_provider,
        }
    }

    /// Creates a GET request builder with OIDC Authorization.
    pub async fn get_request(&self, url: &str, audience: &str) -> Result<reqwest::RequestBuilder> {
        let token = self.token_provider.get_token(audience).await?;
        Ok(self.client
            .get(url)
            .header(AUTHORIZATION, format!("Bearer {}", token)))
    }

    /// Sends a GET request with OIDC Authorization.
    pub async fn get(&self, url: &str, audience: &str) -> Result<reqwest::Response> {
        self.get_request(url, audience)
            .await?
            .send()
            .await
            .context("Failed to send GET request")
    }

    /// Creates a POST request builder with OIDC Authorization.
    pub async fn post_request<T: serde::Serialize + ?Sized>(
        &self,
        url: &str,
        audience: &str,
        json: &T,
    ) -> Result<reqwest::RequestBuilder> {
        let token = self.token_provider.get_token(audience).await?;
        Ok(self.client
            .post(url)
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .json(json))
    }

    /// Sends a POST request with OIDC Authorization.
    pub async fn post<T: serde::Serialize + ?Sized>(
        &self,
        url: &str,
        audience: &str,
        json: &T,
    ) -> Result<reqwest::Response> {
        self.post_request(url, audience, json)
            .await?
            .send()
            .await
            .context("Failed to send POST request")
    }
}
