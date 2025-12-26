use actix_web::{HttpRequest, HttpResponse, web};
use serde::{Deserialize, Serialize};
use std::str;
use utoipa::ToSchema;

#[derive(Deserialize)]
pub struct HealthCheckFormData {
    _name: String,
    _email: String,
}

#[derive(Deserialize, Serialize, Debug, ToSchema)]
pub struct PingResponse {
    pub status: String,
}

#[allow(dead_code)]
#[utoipa::path(
    get,
    path = "/health_check",
    responses(
        (status = 200, description = "Service is alive", body = PingResponse),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn health_check(_req: HttpRequest) -> HttpResponse {
    let resp = PingResponse {
        status: "alive".to_string(),
    };

    tracing::info!(target: "root", "Received request for: health_check");

    HttpResponse::Ok().json(resp)
}

#[allow(dead_code)]
pub async fn health_post_check(_form: web::Form<HealthCheckFormData>) -> HttpResponse {
    HttpResponse::Ok().finish()
}
