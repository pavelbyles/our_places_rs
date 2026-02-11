use anyhow::{Context, Result};
use reqwest::header::AUTHORIZATION;
use reqwest::Client;

use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// A trait for fetching OIDC Identity Tokens.
#[async_trait::async_trait]
pub trait TokenProvider: Send + Sync {
    async fn get_token(&self, audience: &str) -> Result<String>;
}

struct CachedToken {
    token: String,
    expires_at: Instant,
}

/// Fetches tokens from the Google Cloud Metadata Server (for Cloud Run).
pub struct GoogleMetadataTokenProvider {
    cache: Mutex<Option<CachedToken>>,
}

impl GoogleMetadataTokenProvider {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(None),
        }
    }
}

impl Default for GoogleMetadataTokenProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl TokenProvider for GoogleMetadataTokenProvider {
    async fn get_token(&self, audience: &str) -> Result<String> {
        let mut cache = self.cache.lock().await;

        if let Some(cached) = &*cache {
            if cached.expires_at > Instant::now() {
                return Ok(cached.token.clone());
            }
        }

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
            .context("Failed to read token from Metadata Server")?
            .trim()
            .to_string();

        // Cache for 50 minutes (tokens usually last 1 hour)
        *cache = Some(CachedToken {
            token: token.clone(),
            expires_at: Instant::now() + Duration::from_secs(50 * 60),
        });

        Ok(token)
    }
}

/// Fetches tokens using the `gcloud` CLI (for local development).
pub struct LocalGcloudTokenProvider {
    cache: Mutex<Option<CachedToken>>,
}

impl LocalGcloudTokenProvider {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(None),
        }
    }
}

impl Default for LocalGcloudTokenProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl TokenProvider for LocalGcloudTokenProvider {
    async fn get_token(&self, audience: &str) -> Result<String> {
        let mut cache = self.cache.lock().await;

        if let Some(cached) = &*cache {
            if cached.expires_at > Instant::now() {
                return Ok(cached.token.clone());
            }
        }

        let audience = audience.to_string();
        let output = tokio::task::spawn_blocking(move || {
            // Runs: gcloud auth print-identity-token --audiences=...
            let mut output = Command::new("gcloud")
                .arg("auth")
                .arg("print-identity-token")
                .arg(format!("--audiences={}", audience))
                .output()
                .context("Failed to execute gcloud command")?;

            // If that failed (likely due to being a User account not supporting --audiences), try without audience
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("Invalid account Type")
                    || stderr.contains("Requires valid service account")
                {
                    output = Command::new("gcloud")
                        .arg("auth")
                        .arg("print-identity-token")
                        .output()
                        .context("Failed to execute gcloud command (fallback)")?;
                }
            }
            Ok::<_, anyhow::Error>(output)
        })
        .await??;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("gcloud command failed: {}", stderr);
        }

        let token = String::from_utf8(output.stdout)
            .context("Invalid UTF-8 in gcloud output")?
            .trim()
            .to_string();

        // Cache for 50 minutes
        *cache = Some(CachedToken {
            token: token.clone(),
            expires_at: Instant::now() + Duration::from_secs(50 * 60),
        });

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
            Arc::new(GoogleMetadataTokenProvider::new())
        } else {
            Arc::new(LocalGcloudTokenProvider::new())
        };

        Self {
            client: Client::new(),
            token_provider,
        }
    }

    /// Creates a GET request builder with OIDC Authorization.
    pub async fn get_request(&self, url: &str, audience: &str) -> Result<reqwest::RequestBuilder> {
        let token = self.token_provider.get_token(audience).await?;
        Ok(self
            .client
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
        Ok(self
            .client
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

    /// Creates a PATCH request builder with OIDC Authorization.
    pub async fn patch_request<T: serde::Serialize + ?Sized>(
        &self,
        url: &str,
        audience: &str,
        json: &T,
    ) -> Result<reqwest::RequestBuilder> {
        let token = self.token_provider.get_token(audience).await?;
        Ok(self
            .client
            .patch(url)
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .json(json))
    }

    /// Sends a PATCH request with OIDC Authorization.
    pub async fn patch<T: serde::Serialize + ?Sized>(
        &self,
        url: &str,
        audience: &str,
        json: &T,
    ) -> Result<reqwest::Response> {
        self.patch_request(url, audience, json)
            .await?
            .send()
            .await
            .context("Failed to send PATCH request")
    }
}
