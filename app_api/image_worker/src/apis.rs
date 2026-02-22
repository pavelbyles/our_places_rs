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
    pub attributes: PubSubAttributes,
    pub data: String,
    pub message_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PubSubAttributes {
    pub bucket_id: String,
    pub event_time: String,
    pub event_type: String,
    pub notification_config: String,
    pub object_generation: String,
    pub object_id: String,
    pub payload_format: String,
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
    tracing::info!(
        "Received PubSub notification for object: {}",
        payload_data.message.attributes.object_id
    );

    // TODO: Actually process the image by downloading it from GCS, resizing, and uploading to public bucket

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
