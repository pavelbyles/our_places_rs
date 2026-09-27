use base64::Engine;
use common::email::EmailNotificationEvent;
use reqwest::Client;
use tracing::{error, info, instrument};
use uuid::Uuid;

/// Asynchronous GCP Cloud Pub/Sub publisher for dispatching email notification events.
///
/// Implements the publisher side of the **Transactional Outbox Pattern**:
/// Upstream API services (`user_api`, `booking_api`) first persist an email record
/// to PostgreSQL in `db_core::email_outbox`, then asynchronously emit an [`EmailNotificationEvent`]
/// to a Google Cloud Pub/Sub topic containing the outbox record's `Uuid`.
///
/// ### Environment Variables
/// - `EMAIL_PUBSUB_TOPIC_ID` / `PUBSUB_TOPIC_ID`: Pub/Sub topic name (defaults to `"email-notifications-topic"`).
/// - `GCP_PROJECT_ID`: Target GCP Project ID (defaults to `"our-places-dev"`).
/// - `PUBSUB_EMULATOR_HOST`: When present, targets the local Pub/Sub emulator instead of Google Cloud.
/// - `DISABLE_PUBSUB_PUBLISH`: If set to `"true"` or `"1"`, publishing becomes a no-op (useful for local dev and unit tests).
/// - `PUBSUB_AUTH_TOKEN`: Optional Bearer authentication token for authenticated endpoints.
#[derive(Clone, Debug)]
pub struct EmailPublisher {
    http_client: Client,
    topic_url: String,
    auth_token: Option<String>,
    disabled: bool,
}

impl EmailPublisher {
    /// Constructs a new [`EmailPublisher`] targeting the specified Pub/Sub `topic_id`.
    ///
    /// Automatically detects whether a local emulator is active via `PUBSUB_EMULATOR_HOST`,
    /// or constructs the standard Google Cloud Pub/Sub v1 REST endpoint:
    /// `https://pubsub.googleapis.com/v1/projects/{project_id}/topics/{topic_id}:publish`.
    pub fn new(topic_id: &str) -> Self {
        let gcp_project =
            std::env::var("GCP_PROJECT_ID").unwrap_or_else(|_| "our-places-dev".to_string());
        let topic_url = if let Ok(emulator_host) = std::env::var("PUBSUB_EMULATOR_HOST") {
            format!(
                "http://{}/v1/projects/{}/topics/{}:publish",
                emulator_host, gcp_project, topic_id
            )
        } else {
            format!(
                "https://pubsub.googleapis.com/v1/projects/{}/topics/{}:publish",
                gcp_project, topic_id
            )
        };
        let disabled = std::env::var("DISABLE_PUBSUB_PUBLISH")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        Self {
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| Client::new()),
            topic_url,
            auth_token: std::env::var("PUBSUB_AUTH_TOKEN").ok(),
            disabled,
        }
    }

    /// Initializes an [`EmailPublisher`] with topic configuration resolved from environment variables.
    ///
    /// Checks `EMAIL_PUBSUB_TOPIC_ID` first, falling back to `PUBSUB_TOPIC_ID`,
    /// and defaults to `"email-notifications-topic"` if neither is specified.
    pub fn from_env() -> Self {
        let topic_id = std::env::var("EMAIL_PUBSUB_TOPIC_ID")
            .or_else(|_| std::env::var("PUBSUB_TOPIC_ID"))
            .unwrap_or_else(|_| "email-notifications-topic".to_string());
        Self::new(&topic_id)
    }

    /// Creates a disabled [`EmailPublisher`] instance where all publish calls are no-ops.
    ///
    /// Ideal for unit tests, offline development, or integration test suites where external Pub/Sub
    /// messaging should not take place.
    pub fn disabled() -> Self {
        Self {
            http_client: Client::new(),
            topic_url: String::new(),
            auth_token: None,
            disabled: true,
        }
    }

    /// Publishes an [`EmailNotificationEvent`] for the specified `email_id` to the Pub/Sub topic.
    ///
    /// Serializes the event into JSON, base64-encodes the payload according to GCP Pub/Sub REST API
    /// specifications, and sends a POST request with a 5-second HTTP timeout.
    ///
    /// Returns `Ok(())` if published successfully (or if disabled), or returns [`ApiError::Internal`](crate::error::ApiError::Internal)
    /// on serialization failure, network errors, or non-2xx HTTP responses.

    #[instrument(skip(self))]
    pub async fn publish_email_event(&self, email_id: Uuid) -> Result<(), crate::error::ApiError> {
        if self.disabled {
            info!(
                "Pub/Sub email publish is disabled; skipping publish for email_id {}",
                email_id
            );
            return Ok(());
        }

        let event = EmailNotificationEvent { email_id };
        let payload_bytes = serde_json::to_vec(&event).map_err(|e| {
            error!("Failed to serialize EmailNotificationEvent: {}", e);
            crate::error::ApiError::Internal
        })?;

        let base64_data = base64::engine::general_purpose::STANDARD.encode(payload_bytes);

        let pubsub_body = serde_json::json!({
            "messages": [{
                "data": base64_data
            }]
        });

        let mut req = self.http_client.post(&self.topic_url).json(&pubsub_body);
        if let Some(token) = &self.auth_token {
            req = req.bearer_auth(token);
        }

        let res = req.send().await.map_err(|e| {
            error!("Failed to publish email event to Pub/Sub: {}", e);
            crate::error::ApiError::Internal
        })?;

        if res.status().is_success() {
            info!(
                "Successfully published email event for email_id {}",
                email_id
            );
            Ok(())
        } else {
            let status = res.status();
            let err_text = res.text().await.unwrap_or_default();
            error!(
                "Pub/Sub publish returned error status {}: {}",
                status, err_text
            );
            Err(crate::error::ApiError::Internal)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_disabled_publisher_succeeds() {
        let publisher = EmailPublisher::disabled();
        let res = publisher.publish_email_event(Uuid::new_v4()).await;
        assert!(res.is_ok());
    }
}
