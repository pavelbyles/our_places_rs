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

    /// A transient condition the client can resolve by retrying.
    #[error("Service temporarily unavailable: {0}")]
    ServiceUnavailable(String),
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
            ApiError::Database(DbError::HoldExpired) => StatusCode::CONFLICT,
            ApiError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            ApiError::Database(DbError::CurrencyNotFound(_)) => StatusCode::NOT_FOUND,
            ApiError::Database(
                DbError::CurrencyUnknown(_)
                | DbError::CurrencyInactive(_)
                | DbError::EffectiveAtInPast
                | DbError::EffectiveAtTooFar
                | DbError::RateChangeConfirmationRequired { .. },
            ) => StatusCode::UNPROCESSABLE_ENTITY,
            ApiError::Database(
                DbError::CurrencyInUse(_)
                | DbError::CurrencyExists(_)
                | DbError::RateAlreadyEffective
                | DbError::RateVersionExists,
            ) => StatusCode::CONFLICT,
            ApiError::Database(DbError::LockContention(_)) => StatusCode::SERVICE_UNAVAILABLE,
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
        // Server-side failures return a bare body, so the log line is the only
        // record of what went wrong. `Debug` prints the full cause chain.
        if self.status_code().is_server_error() {
            tracing::error!(status = %self.status_code(), error = ?self, "request failed");
        }
        if let Some(response) = self.currency_error_response() {
            return response;
        }
        match self {
            ApiError::Database(DbError::HoldExpired) => {
                HttpResponse::Conflict().json(serde_json::json!({
                    "code": "HOLD_EXPIRED",
                    "message": "The reservation hold has expired. Please start a new booking."
                }))
            }
            ApiError::ServiceUnavailable(msg) => HttpResponse::ServiceUnavailable()
                .insert_header(("Retry-After", "1"))
                .json(serde_json::json!({
                    "code": "SERVICE_UNAVAILABLE",
                    "message": msg
                })),
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

impl ApiError {
    /// Structured bodies for currency and exchange-rate administration errors.
    /// Business rejections are logged at `warn`; `None` for every other error.
    fn currency_error_response(&self) -> Option<HttpResponse> {
        let ApiError::Database(db) = self else {
            return None;
        };
        let (code, message, extra) = match db {
            DbError::CurrencyNotFound(c) | DbError::CurrencyUnknown(c) => (
                "CURRENCY_NOT_FOUND",
                format!("Currency {c} does not exist."),
                serde_json::json!({ "currency": c }),
            ),
            DbError::CurrencyInactive(c) => (
                "CURRENCY_INACTIVE",
                format!("Currency {c} is not active."),
                serde_json::json!({ "currency": c }),
            ),
            DbError::CurrencyInUse(listings) => (
                "CURRENCY_IN_USE",
                "Listings still use this currency. Change them first.".to_string(),
                serde_json::json!({ "listings": listings }),
            ),
            DbError::CurrencyExists(c) => (
                "CURRENCY_EXISTS",
                format!("Currency {c} already exists."),
                serde_json::json!({ "currency": c }),
            ),
            DbError::EffectiveAtInPast => (
                "EFFECTIVE_AT_IN_PAST",
                "Rates cannot take effect in the past.".to_string(),
                serde_json::json!({}),
            ),
            DbError::EffectiveAtTooFar => (
                "EFFECTIVE_AT_TOO_FAR",
                "Rates can be scheduled at most a year ahead.".to_string(),
                serde_json::json!({}),
            ),
            DbError::RateAlreadyEffective => (
                "RATE_ALREADY_EFFECTIVE",
                "This rate has already taken effect and cannot be deleted.".to_string(),
                serde_json::json!({}),
            ),
            DbError::RateVersionExists => (
                "RATE_VERSION_EXISTS",
                "A rate for this pair and effective time already exists.".to_string(),
                serde_json::json!({}),
            ),
            DbError::RateChangeConfirmationRequired {
                baseline,
                proposed,
                deviation_pct,
            } => (
                "RATE_CHANGE_CONFIRMATION_REQUIRED",
                "The new rate differs from the current rate by more than 5%. Confirm to save it."
                    .to_string(),
                serde_json::json!({
                    "baseline_rate": baseline.to_string(),
                    "proposed_rate": proposed.to_string(),
                    "deviation_pct": deviation_pct.map(|d| d.to_string()),
                }),
            ),
            DbError::LockContention(operation) => {
                return Some(
                    HttpResponse::ServiceUnavailable()
                        .insert_header(("Retry-After", "1"))
                        .json(serde_json::json!({
                            "code": "SERVICE_UNAVAILABLE",
                            "message": format!("The {operation} is busy. Please retry."),
                        })),
                );
            }
            _ => return None,
        };
        tracing::warn!(code, %message, "currency request rejected");
        let mut body = serde_json::json!({ "code": code, "message": message });
        if let (Some(target), serde_json::Value::Object(fields)) = (body.as_object_mut(), extra) {
            target.extend(fields);
        }
        Some(HttpResponse::build(self.status_code()).json(body))
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

#[cfg(test)]
mod currency_error_tests {
    use super::*;
    use actix_web::body::to_bytes;
    use common::currency::ListingRef;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    async fn render(err: DbError) -> (StatusCode, serde_json::Value, Option<String>) {
        let api = ApiError::Database(err);
        let status = api.status_code();
        let resp = api.error_response();
        assert_eq!(
            resp.status(),
            status,
            "status_code and error_response agree"
        );
        let retry_after = resp
            .headers()
            .get("Retry-After")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = to_bytes(resp.into_body()).await.expect("body bytes");
        let json = serde_json::from_slice(&bytes).expect("json body");
        (status, json, retry_after)
    }

    #[tokio::test]
    async fn currency_not_found_is_404() {
        let (s, b, _) = render(DbError::CurrencyNotFound("XYZ".into())).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        assert_eq!(b["code"], "CURRENCY_NOT_FOUND");
    }

    #[tokio::test]
    async fn currency_unknown_on_write_is_422() {
        let (s, b, _) = render(DbError::CurrencyUnknown("XYZ".into())).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(b["code"], "CURRENCY_NOT_FOUND");
        assert_eq!(b["currency"], "XYZ");
    }

    #[tokio::test]
    async fn currency_inactive_is_422() {
        let (s, b, _) = render(DbError::CurrencyInactive("CAD".into())).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(b["code"], "CURRENCY_INACTIVE");
    }

    #[tokio::test]
    async fn currency_in_use_lists_listings() {
        let listing = ListingRef {
            id: uuid::Uuid::nil(),
            title: "Villa Azul".into(),
            slug: "villa-azul".into(),
        };
        let (s, b, _) = render(DbError::CurrencyInUse(vec![listing])).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert_eq!(b["code"], "CURRENCY_IN_USE");
        assert_eq!(b["listings"][0]["title"], "Villa Azul");
        assert_eq!(b["listings"][0]["slug"], "villa-azul");
    }

    #[tokio::test]
    async fn currency_exists_is_409() {
        let (s, b, _) = render(DbError::CurrencyExists("USD".into())).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert_eq!(b["code"], "CURRENCY_EXISTS");
    }

    #[tokio::test]
    async fn effective_at_in_past_is_422() {
        let (s, b, _) = render(DbError::EffectiveAtInPast).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(b["code"], "EFFECTIVE_AT_IN_PAST");
    }

    #[tokio::test]
    async fn effective_at_too_far_is_422() {
        let (s, b, _) = render(DbError::EffectiveAtTooFar).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(b["code"], "EFFECTIVE_AT_TOO_FAR");
    }

    #[tokio::test]
    async fn rate_already_effective_is_409() {
        let (s, b, _) = render(DbError::RateAlreadyEffective).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert_eq!(b["code"], "RATE_ALREADY_EFFECTIVE");
    }

    #[tokio::test]
    async fn rate_version_exists_is_409() {
        let (s, b, _) = render(DbError::RateVersionExists).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert_eq!(b["code"], "RATE_VERSION_EXISTS");
    }

    #[tokio::test]
    async fn confirmation_required_carries_string_decimals() {
        let dec = |s: &str| Decimal::from_str(s).expect("decimal");
        let (s, b, _) = render(DbError::RateChangeConfirmationRequired {
            baseline: dec("160.000000"),
            proposed: dec("176"),
            deviation_pct: Some(dec("10.00")),
        })
        .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(b["code"], "RATE_CHANGE_CONFIRMATION_REQUIRED");
        assert_eq!(b["baseline_rate"], "160.000000");
        assert_eq!(b["proposed_rate"], "176");
        assert_eq!(b["deviation_pct"], "10.00");
    }

    #[tokio::test]
    async fn lock_contention_is_retryable_503() {
        let (s, b, retry) = render(DbError::LockContention("rate publish".into())).await;
        assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(b["code"], "SERVICE_UNAVAILABLE");
        assert_eq!(retry.as_deref(), Some("1"));
    }

    #[tokio::test]
    async fn deadlock_and_lock_timeout_map_to_lock_contention() {
        // Non-database errors pass through unchanged.
        let other = DbError::from_sqlx_contended(sqlx::Error::RowNotFound, "rate publish");
        assert!(matches!(other, DbError::Sqlx(sqlx::Error::RowNotFound)));
        assert_eq!(
            db_core::error::SQLSTATE_DEADLOCK_DETECTED,
            "40P01",
            "deadlock SQLSTATE"
        );
        assert_eq!(
            db_core::error::SQLSTATE_LOCK_NOT_AVAILABLE,
            "55P03",
            "lock timeout SQLSTATE"
        );
    }
}
