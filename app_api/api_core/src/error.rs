use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use db_core::error::DbError;
use std::fmt::{Debug, Formatter};
use thiserror::Error;
use validator::ValidationErrors;

#[derive(Error)]
pub enum ApiError {
    #[error("Database error")]
    Database(#[from] DbError),

    #[error("Internal server error")]
    Internal,

    #[error("Validation error: {0}")]
    ValidationError(#[from] ValidationErrors),

    #[error("Feature '{0}' is currently disabled")]
    FeatureDisabled(String),
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
            // Validation errors are always 400 Bad Request
            ApiError::ValidationError(_) => StatusCode::BAD_REQUEST,
            // All other database errors are likely internal server errors.
            ApiError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            // A generic internal error.
            ApiError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            // Feature disabled error is 403 Forbidden
            ApiError::FeatureDisabled(_) => StatusCode::FORBIDDEN,
        }
    }

    fn error_response(&self) -> HttpResponse {
        match self {
            // For validation errors, return the actual validation details JSON
            ApiError::ValidationError(e) => HttpResponse::BadRequest().json(e),
            // For all other errors, use the default behavior (status code + canonical reason)
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
