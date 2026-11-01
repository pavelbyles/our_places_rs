use actix_web::{FromRequest, HttpRequest, HttpResponse, ResponseError, web};
use db_core::models::UserRole;
use sqlx::PgPool;
use std::collections::HashMap;
use std::env;

use crate::actuator::models::{
    LoggerLevelInfo, LoggersResponse, UpdateLoggerRequest, UpdateLoggerResponse,
};

const VALID_LEVELS: &[&str] = &["OFF", "ERROR", "WARN", "INFO", "DEBUG", "TRACE"];

/// Whether `presented` matches the configured actuator secret. With no secret
/// (unset or blank) token access is off: there is no well-known default.
/// Compared in constant time so the secret cannot be guessed byte by byte.
fn actuator_token_matches(presented: &str, expected: Option<&str>) -> bool {
    expected
        .filter(|s| !s.trim().is_empty())
        .is_some_and(|expected| {
            presented.len() == expected.len()
                && presented
                    .bytes()
                    .zip(expected.bytes())
                    .fold(0u8, |diff, (a, b)| diff | (a ^ b))
                    == 0
        })
}

fn actuator_denied(
    status: actix_web::http::StatusCode,
    code: &str,
    message: &str,
) -> Box<HttpResponse> {
    Box::new(
        HttpResponse::build(status).json(serde_json::json!({ "code": code, "message": message })),
    )
}

/// Verifies whether the request contains valid actuator credentials:
/// either an `X-Actuator-Token` header matching `ACTUATOR_SECRET`, or a user
/// JWT (`X-Forwarded-Authorization` behind Cloud Run, else `Authorization`)
/// for a user whose stored roles include admin. Token claims alone never make
/// someone an admin: the roles are read from the database.
pub async fn verify_actuator_auth(req: &HttpRequest) -> Result<(), Box<HttpResponse>> {
    use actix_web::http::StatusCode;

    // 1. Check X-Actuator-Token header
    if let Some(token_val) = req
        .headers()
        .get("x-actuator-token")
        .and_then(|h| h.to_str().ok())
    {
        let expected = env::var("ACTUATOR_SECRET")
            .or_else(|_| env::var("ADMIN_SECRET"))
            .ok();
        if actuator_token_matches(token_val, expected.as_deref()) {
            return Ok(());
        }
        tracing::warn!(
            secret_configured = expected.as_deref().is_some_and(|s| !s.trim().is_empty()),
            "rejected actuator token"
        );
    }

    // 2. A signed-in admin.
    let headers = req.headers();
    if !headers.contains_key(common::http_client::USER_AUTHORIZATION_HEADER)
        && !headers.contains_key("authorization")
    {
        return Err(actuator_denied(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "Missing or invalid actuator credentials",
        ));
    }
    let claims = crate::auth::Claims::from_request(req, &mut actix_web::dev::Payload::None)
        .into_inner()
        .map_err(|e| {
            tracing::warn!(error = %e, "rejected actuator JWT");
            Box::new(e.error_response())
        })?;
    let pool = req.app_data::<web::Data<PgPool>>().ok_or_else(|| {
        tracing::error!("actuator: no database pool to check the caller's admin role");
        actuator_denied(
            StatusCode::SERVICE_UNAVAILABLE,
            "SERVICE_UNAVAILABLE",
            "Cannot verify administrator access right now",
        )
    })?;
    match db_core::user::get_user_by_id(pool.get_ref(), claims.sub).await {
        Ok(user) if user.roles.contains(&UserRole::Admin) => Ok(()),
        Ok(_) => {
            tracing::warn!(user_id = %claims.sub, "non-admin attempted to use the actuator");
            Err(actuator_denied(
                StatusCode::FORBIDDEN,
                "FORBIDDEN",
                "Only administrators can change log levels",
            ))
        }
        Err(db_core::error::DbError::Sqlx(sqlx::Error::RowNotFound)) => {
            tracing::warn!(user_id = %claims.sub, "actuator token is for a user that does not exist");
            Err(actuator_denied(
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "Missing or invalid actuator credentials",
            ))
        }
        Err(e) => {
            tracing::error!(user_id = %claims.sub, error = ?e, "actuator: failed to load the caller's roles");
            Err(actuator_denied(
                StatusCode::SERVICE_UNAVAILABLE,
                "SERVICE_UNAVAILABLE",
                "Cannot verify administrator access right now",
            ))
        }
    }
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
    if let Err(resp) = verify_actuator_auth(&req).await {
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

#[cfg(test)]
mod tests {
    use super::actuator_token_matches;

    #[test]
    fn no_configured_secret_means_no_token_access() {
        assert!(!actuator_token_matches("secret", None));
        assert!(!actuator_token_matches("", Some("")));
        assert!(!actuator_token_matches("  ", Some("  ")));
    }

    #[test]
    fn only_the_exact_secret_matches() {
        let secret = Some("super-secret-token");
        assert!(actuator_token_matches("super-secret-token", secret));
        assert!(!actuator_token_matches("super-secret-tokeN", secret));
        assert!(!actuator_token_matches("super-secret", secret));
        assert!(!actuator_token_matches("secret", secret));
    }
}
