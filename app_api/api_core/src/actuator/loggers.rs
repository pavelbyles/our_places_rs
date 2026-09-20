use actix_web::{HttpRequest, HttpResponse, web};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use uuid::Uuid;

use crate::actuator::models::{
    LoggerLevelInfo, LoggersResponse, UpdateLoggerRequest, UpdateLoggerResponse,
};

const VALID_LEVELS: &[&str] = &["OFF", "ERROR", "WARN", "INFO", "DEBUG", "TRACE"];

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ActuatorJwtClaims {
    pub sub: Uuid,
    pub exp: usize,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub is_admin: Option<bool>,
}

/// Verifies whether the request contains valid actuator credentials:
/// Either an `X-Actuator-Token` header matching `ACTUATOR_SECRET`,
/// or an `Authorization: Bearer <jwt>` token with an admin role or valid claims.
pub fn verify_actuator_auth(req: &HttpRequest) -> Result<(), Box<HttpResponse>> {
    // 1. Check X-Actuator-Token header
    if let Some(token_val) = req
        .headers()
        .get("x-actuator-token")
        .and_then(|h| h.to_str().ok())
    {
        let expected_secret = env::var("ACTUATOR_SECRET")
            .or_else(|_| env::var("ADMIN_SECRET"))
            .unwrap_or_else(|_| "secret".to_string());

        if token_val == expected_secret {
            return Ok(());
        }
    }

    // 2. Check Authorization Bearer JWT
    if let Some(auth_header) = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        && let Some(jwt_str) = auth_header.strip_prefix("Bearer ")
    {
        let secret = env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;

        if let Ok(token_data) = decode::<ActuatorJwtClaims>(
            jwt_str,
            &DecodingKey::from_secret(secret.as_bytes()),
            &validation,
        ) {
            let is_admin = token_data.claims.role.as_deref() == Some("admin")
                || token_data.claims.is_admin == Some(true)
                || !token_data.claims.sub.is_nil();

            if is_admin {
                return Ok(());
            }
        }
    }

    Err(Box::new(HttpResponse::Unauthorized().json(
        serde_json::json!({
            "code": "UNAUTHORIZED",
            "message": "Missing or invalid actuator credentials"
        }),
    )))
}

/// Lists all supported log levels and current active root level.
#[utoipa::path(
    get,
    path = "/loggers",
    tag = "actuator",
    responses(
        (status = 200, description = "Current logger configurations", body = LoggersResponse)
    )
)]
pub async fn get_loggers_handler() -> HttpResponse {
    let current_filter = crate::tracing_utils::get_current_filter();

    let mut loggers = HashMap::new();
    loggers.insert(
        "ROOT".to_string(),
        LoggerLevelInfo {
            configured_level: current_filter.to_uppercase(),
            effective_level: current_filter.to_uppercase(),
        },
    );

    HttpResponse::Ok().json(LoggersResponse {
        levels: VALID_LEVELS.iter().map(|s| s.to_string()).collect(),
        loggers,
    })
}

/// Returns log level for a specific target logger.
#[utoipa::path(
    get,
    path = "/loggers/{name}",
    tag = "actuator",
    params(
        ("name" = String, Path, description = "Logger or crate target name")
    ),
    responses(
        (status = 200, description = "Target logger level", body = LoggerLevelInfo)
    )
)]
pub async fn get_logger_by_name_handler(path: web::Path<String>) -> HttpResponse {
    let _name = path.into_inner();
    let current_filter = crate::tracing_utils::get_current_filter();

    HttpResponse::Ok().json(LoggerLevelInfo {
        configured_level: current_filter.to_uppercase(),
        effective_level: current_filter.to_uppercase(),
    })
}

/// Dynamically reloads the logging filter at runtime.
/// Requires admin authorization via X-Actuator-Token or Admin JWT.
#[utoipa::path(
    post,
    path = "/loggers/{name}",
    tag = "actuator",
    params(
        ("name" = String, Path, description = "Logger or crate target name")
    ),
    request_body = UpdateLoggerRequest,
    responses(
        (status = 200, description = "Log level updated successfully", body = UpdateLoggerResponse),
        (status = 400, description = "Invalid log level"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn update_logger_handler(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<UpdateLoggerRequest>,
) -> HttpResponse {
    if let Err(resp) = verify_actuator_auth(&req) {
        return *resp;
    }

    let logger_name = path.into_inner();
    let requested_level = body.configured_level.trim().to_uppercase();

    if !VALID_LEVELS.contains(&requested_level.as_str()) {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "code": "INVALID_LOG_LEVEL",
            "message": format!("Supported levels are: {}", VALID_LEVELS.join(", "))
        }));
    }

    let filter_directive = if logger_name == "ROOT" || logger_name.is_empty() {
        requested_level.to_lowercase()
    } else {
        format!("{}={}", logger_name, requested_level.to_lowercase())
    };

    match crate::tracing_utils::reload_filter(&filter_directive) {
        Ok(()) => {
            tracing::warn!(
                "Actuator: dynamic log level change for '{}' to '{}'",
                logger_name,
                requested_level
            );
            HttpResponse::Ok().json(UpdateLoggerResponse {
                name: logger_name,
                configured_level: requested_level,
            })
        }
        Err(e) => {
            tracing::error!("Actuator: failed to reload filter directive: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "code": "FILTER_RELOAD_ERROR",
                "message": e
            }))
        }
    }
}
