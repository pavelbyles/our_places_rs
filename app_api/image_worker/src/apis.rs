use actix_web::middleware::from_fn;
use actix_web::{Error, HttpRequest, HttpResponse, Responder, web};
use api_core::api_common::content_negotiation_middleware;
use serde::Deserialize;
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

#[derive(Debug, Deserialize, ToSchema)]
pub struct PubSubPayload {
    pub message: PubSubMessage,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PubSubMessage {
    pub _attributes: Option<PubSubAttributes>,
    pub data: String,
    pub _message_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PubSubAttributes {
    pub _bucket_id: Option<String>,
    pub _event_time: Option<String>,
    pub _event_type: Option<String>,
    pub _notification_config: Option<String>,
    pub _object_generation: Option<String>,
    pub _object_id: Option<String>,
    pub _payload_format: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GcsObjectMetadata {
    pub _kind: Option<String>,
    pub _id: Option<String>,
    pub _self_link: Option<String>,
    pub name: String,
    pub bucket: String,
    pub _generation: Option<String>,
    pub _metageneration: Option<String>,
    pub content_type: Option<String>,
    pub _time_created: Option<String>,
    pub _updated: Option<String>,
    pub _storage_class: Option<String>,
    pub _time_storage_class_updated: Option<String>,
    pub _size: Option<String>,
    pub _md5_hash: Option<String>,
    pub _media_link: Option<String>,
    pub _crc32c: Option<String>,
    pub _etag: Option<String>,
}

use base64::Engine;

impl PubSubMessage {
    pub fn decode_data(&self) -> Result<GcsObjectMetadata, Box<dyn std::error::Error>> {
        let decoded = base64::engine::general_purpose::STANDARD.decode(&self.data)?;
        let metadata: GcsObjectMetadata = serde_json::from_slice(&decoded)?;
        Ok(metadata)
    }
}

#[tracing::instrument]
#[utoipa::path(
    post,
    path = "/api/v1/internal/image/process_image",
    tag = "images",
    request_body = PubSubPayload,
    responses(
        (status = 200, description = "Image processed payload received"),
        (status = 400, description = "Bad Request"),
        (status = 500, description = "Internal Server Error")
    )
)]
pub async fn process_image(
    req: HttpRequest,
    payload: web::Json<PubSubPayload>,
) -> Result<impl Responder, Error> {
    let payload_data = payload.into_inner();

    let object_metadata = match payload_data.message.decode_data() {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("Failed to decode payload data: {}", e);
            return Err(actix_web::error::ErrorBadRequest("Invalid payload data"));
        }
    };

    tracing::info!(
        "Received PubSub notification for object: {} (bucket: {}, {:?})",
        object_metadata.name,
        object_metadata.bucket,
        object_metadata.content_type
    );

    // TODO: Actually process the image by downloading it from GCS, resizing, convert to webp and uploading to public bucket / CDN

    Ok(HttpResponse::Ok().finish())
}

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    #[derive(OpenApi)]
    #[openapi(
        paths(
            process_image,
            api_core::health::health_check,
        ),
        components(
            schemas(PubSubPayload, PubSubMessage, PubSubAttributes, api_core::health::PingResponse)
        ),
        tags(
            (name = "images", description = "Image processing endpoints")
        ),
    )]
    struct ApiDoc;

    // Register Swagger UI services at the ROOT scope so paths match
    cfg.service(
        SwaggerUi::new("/api/docs/swagger-ui/{_:.*}")
            .url("/api-docs/openapi.json", ApiDoc::openapi()),
    );

    cfg.service(
        web::scope("/api/v1/internal/image")
            .route(
                "/process_image",
                web::post()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(process_image),
            )
            .route(
                "/health_check",
                web::get().to(api_core::health::health_check),
            ),
    );
}
