use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use db_core::error::DbError;
use std::fmt::{Debug, Formatter};
use thiserror::Error;
use validator::ValidationErrors;

/// Unified HTTP API error enum returned by Actix route handlers.
#[derive(Error)]
pub enum ApiError {
    /// Database error mapped from db_core.
    #[error("Database error")]
    Database(#[from] DbError),

    /// Internal unexpected server error.
    #[error("Internal server error")]
    Internal,

    /// Request payload validation failure.
    #[error("Validation error: {0}")]
    ValidationError(#[from] ValidationErrors),

    /// Requested feature is currently toggled off.
    #[error("Feature '{0}' is currently disabled")]
    FeatureDisabled(String),

    /// Authentication failed or caller lacks required credentials.
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Caller is authenticated but lacks required permission.
    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// Requested booking dates overlap with an existing reservation.
    #[error("The requested dates are no longer available")]
    DateUnavailable,

    /// Reservation is in a terminal status or dates cannot be modified.
    #[error("Booking not modifiable: {0}")]
    BookingNotModifiable(String),

    /// A reservation change request is already pending for this booking.
    #[error("A reservation change request is already pending for this booking")]
    ChangeRequestAlreadyPending,

    /// Payment or refund settlement with payment gateway failed.
    #[error("Payment settlement failed: {0}")]
    PaymentSettlementFailed(String),
}

// Implement Debug manually to avoid printing the source error in production.
impl Debug for ApiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

// This is the key implementation that allows Actix Web to convert our errors.
impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::Database(DbError::Sqlx(sqlx::Error::RowNotFound)) => StatusCode::NOT_FOUND,
            ApiError::Database(DbError::ValidationError(_)) => StatusCode::BAD_REQUEST,
            ApiError::Database(DbError::FxRateUnavailable { .. }) => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            // Validation errors are always 400 Bad Request
            ApiError::ValidationError(_) => StatusCode::BAD_REQUEST,
            ApiError::Database(DbError::DateUnavailable) | ApiError::DateUnavailable => {
                StatusCode::CONFLICT
            }
            ApiError::Database(DbError::BookingNotModifiable(_))
            | ApiError::BookingNotModifiable(_) => StatusCode::CONFLICT,
            ApiError::Database(DbError::ChangeRequestAlreadyPending)
            | ApiError::ChangeRequestAlreadyPending => StatusCode::CONFLICT,
            ApiError::PaymentSettlementFailed(_) => StatusCode::UNPROCESSABLE_ENTITY,
            // All other database errors are likely internal server errors.
            ApiError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            // A generic internal error.
            ApiError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            // Feature disabled error is 403 Forbidden
            ApiError::FeatureDisabled(_) => StatusCode::FORBIDDEN,
            ApiError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden(_) => StatusCode::FORBIDDEN,
        }
    }

    fn error_response(&self) -> HttpResponse {
        match self {
            ApiError::ValidationError(e) => HttpResponse::BadRequest().json(e),
            ApiError::Database(DbError::ValidationError(msg)) => {
                HttpResponse::BadRequest().json(serde_json::json!({ "error": msg }))
            }
            ApiError::Database(DbError::FxRateUnavailable { base, target }) => {
                HttpResponse::UnprocessableEntity().json(serde_json::json!({
                    "code": "FX_RATE_UNAVAILABLE",
                    "message": format!("Foreign exchange rate unavailable for currency pair: {} -> {}", base, target)
                }))
            }
            ApiError::DateUnavailable | ApiError::Database(DbError::DateUnavailable) => {
                HttpResponse::Conflict().json(serde_json::json!({
                    "code": "DATE_UNAVAILABLE",
                    "message": "The requested dates are no longer available."
                }))
            }
            ApiError::BookingNotModifiable(msg) => {
                HttpResponse::Conflict().json(serde_json::json!({
                    "code": "BOOKING_NOT_MODIFIABLE",
                    "message": msg
                }))
            }
            ApiError::Database(DbError::BookingNotModifiable(msg)) => {
                HttpResponse::Conflict().json(serde_json::json!({
                    "code": "BOOKING_NOT_MODIFIABLE",
                    "message": msg
                }))
            }
            ApiError::ChangeRequestAlreadyPending
            | ApiError::Database(DbError::ChangeRequestAlreadyPending) => {
                HttpResponse::Conflict().json(serde_json::json!({
                    "code": "CHANGE_REQUEST_ALREADY_PENDING",
                    "message": "A reservation change request is already pending for this booking."
                }))
            }
            ApiError::PaymentSettlementFailed(msg) => {
                HttpResponse::UnprocessableEntity().json(serde_json::json!({
                    "code": "PAYMENT_SETTLEMENT_FAILED",
                    "message": msg
                }))
            }
            ApiError::Unauthorized(msg) => {
                HttpResponse::Unauthorized().json(serde_json::json!({ "error": msg }))
            }
            ApiError::FeatureDisabled(msg) => {
                HttpResponse::Forbidden().json(serde_json::json!({ "error": msg }))
            }
            ApiError::Forbidden(msg) => {
                HttpResponse::Forbidden().json(serde_json::json!({ "error": msg }))
            }
            _ => HttpResponse::build(self.status_code()).finish(),
        }
    }
}

// A helper function to format the error chain for logging.
pub fn error_chain_fmt(e: &impl std::error::Error, f: &mut Formatter<'_>) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        current = cause.source();
    }
    Ok(())
}

// A custom `Result` type for our API.
pub type Result<T> = std::result::Result<T, ApiError>;
