use crate::config::WorkerConfig;
use crate::processor::process_email_event;
use crate::provider::EmailProvider;
use actix_web::{HttpRequest, HttpResponse, Responder, web};
use base64::Engine;
use common::email::EmailNotificationEvent;
use db_core::PgPool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PubSubPushEnvelope {
    pub message: PubSubMessage,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PubSubMessage {
    pub data: String,
    #[serde(default)]
    pub _message_id: Option<String>,
    #[serde(default)]
    pub _publish_time: Option<String>,
}

#[utoipa::path(
    post,
    path = "/pubsub/email-events",
    request_body = PubSubPushEnvelope,
    responses(
        (status = 200, description = "Email event processed successfully"),
        (status = 401, description = "Unauthorized Pub/Sub token")
    ),
    tag = "email"
)]
pub async fn handle_pubsub_email_event(
    req: HttpRequest,
    body: web::Json<PubSubPushEnvelope>,
    pool: web::Data<PgPool>,
    provider: web::Data<Arc<dyn EmailProvider>>,
    config: web::Data<WorkerConfig>,
) -> impl Responder {
    // 1. Verify Secret Token if configured
    if let Some(expected_token) = &config.pubsub_secret_token {
        let auth_header = req
            .headers()
            .get("X-PubSub-Secret-Token")
            .and_then(|v| v.to_str().ok())
            .or_else(|| {
                req.headers()
                    .get("Authorization")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.strip_prefix("Bearer "))
            });

        match auth_header {
            Some(token) if token == expected_token => {}
            _ => {
                warn!("Unauthorized Pub/Sub push request received");
                return HttpResponse::Unauthorized()
                    .json(serde_json::json!({ "error": "Unauthorized" }));
            }
        }
    }

    // 2. Decode Base64 Pub/Sub Message Data
    let decoded_bytes = match base64::engine::general_purpose::STANDARD.decode(&body.message.data) {
        Ok(bytes) => bytes,
        Err(e) => {
            error!("Failed to decode base64 Pub/Sub payload: {}", e);
            // Return 200 OK to consume malformed payload and prevent infinite Pub/Sub redelivery
            return HttpResponse::Ok()
                .json(serde_json::json!({ "error": "Malformed base64 payload" }));
        }
    };

    // 3. Deserialize JSON EmailNotificationEvent
    let event: EmailNotificationEvent = match serde_json::from_slice(&decoded_bytes) {
        Ok(evt) => evt,
        Err(e) => {
            error!("Failed to deserialize EmailNotificationEvent: {}", e);
            // Return 200 OK to consume malformed JSON and prevent infinite Pub/Sub redelivery
            return HttpResponse::Ok().json(serde_json::json!({ "error": "Invalid event JSON" }));
        }
    };

    info!(
        "Received email notification event for email_id: {}",
        event.email_id
    );

    // 4. Process Email via State Machine & Decoupled I/O
    if let Err(e) = process_email_event(
        pool.get_ref(),
        provider.as_ref().as_ref(),
        event.email_id,
        config.max_retries,
    )
    .await
    {
        error!(
            "Unexpected error processing email_id {}: {}",
            event.email_id, e
        );
    }

    HttpResponse::Ok().json(serde_json::json!({
        "status": "processed",
        "email_id": event.email_id
    }))
}

#[utoipa::path(
    get,
    path = "/healthz",
    responses(
        (status = 200, description = "Service is healthy")
    ),
    tag = "health"
)]
pub async fn healthz() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}

pub fn configure_routes_with_provider(
    cfg: &mut web::ServiceConfig,
    provider: Arc<dyn EmailProvider>,
    config: WorkerConfig,
) {
    #[derive(OpenApi)]
    #[openapi(
        paths(
            handle_pubsub_email_event,
            healthz
        ),
        components(
            schemas(PubSubPushEnvelope, PubSubMessage)
        ),
        tags(
            (name = "email", description = "Email worker endpoints"),
            (name = "health", description = "Health endpoints")
        )
    )]
    struct ApiDoc;

    cfg.app_data(web::Data::new(provider));
    cfg.app_data(web::Data::new(config));

    cfg.service(
        SwaggerUi::new("/api/docs/swagger-ui/{_:.*}")
            .url("/api-docs/openapi.json", ApiDoc::openapi()),
    );

    cfg.service(
        web::resource("/pubsub/email-events").route(web::post().to(handle_pubsub_email_event)),
    );
    cfg.service(
        web::resource("/api/v1/internal/email/process_email")
            .route(web::post().to(handle_pubsub_email_event)),
    );
    cfg.service(web::resource("/healthz").route(web::get().to(healthz)));
    cfg.service(web::resource("/health").route(web::get().to(healthz)));
}

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    let worker_config = WorkerConfig::from_env();
    let provider: Arc<dyn EmailProvider> = if worker_config.use_mock_provider {
        Arc::new(crate::provider::MockEmailProvider::new())
    } else {
        Arc::new(crate::provider::LettreSmtpEmailProvider::from_env())
    };

    configure_routes_with_provider(cfg, provider, worker_config);
}
