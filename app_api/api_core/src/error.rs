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

    /// The stay can't be priced as asked, such as below its minimum stay.
    #[error("Stay refused: {0}")]
    StayPricing(common::pricing::PricingError),

    /// Reservation is in a terminal status or dates cannot be modified.
    #[error("Booking not modifiable: {0}")]
    BookingNotModifiable(String),

    /// A reservation change request is already pending for this booking.
    #[error("A reservation change request is already pending for this booking")]
    ChangeRequestAlreadyPending,

    /// Payment or refund settlement with payment gateway failed.
    #[error("Payment settlement failed: {0}")]
    PaymentSettlementFailed(String),

    /// The request is well-formed but refused, with a stable code (422).
    #[error("Unprocessable: {code}: {message}")]
    Unprocessable {
        /// Stable error code, such as `VALIDATION_ERROR`.
        code: &'static str,
        /// Text for the caller.
        message: String,
    },

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
            ApiError::Database(DbError::ValidationError(_) | DbError::StayInput(_)) => {
                StatusCode::BAD_REQUEST
            }
            ApiError::Database(DbError::FxRateUnavailable { .. }) => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            // Validation errors are always 400 Bad Request
            ApiError::ValidationError(_) | ApiError::StayPricing(_) => StatusCode::BAD_REQUEST,
            ApiError::Database(DbError::DateUnavailable) | ApiError::DateUnavailable => {
                StatusCode::CONFLICT
            }
            ApiError::Database(DbError::ListingUnavailable) => StatusCode::CONFLICT,
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
            ApiError::Database(DbError::PromoRefused(_)) => StatusCode::UNPROCESSABLE_ENTITY,
            ApiError::Database(DbError::RequestRefused(refusal)) => {
                StatusCode::from_u16(refusal.http_status()).unwrap_or(StatusCode::CONFLICT)
            }
            ApiError::Database(DbError::PromoCodeExists | DbError::PromoAlreadyDeactivated) => {
                StatusCode::CONFLICT
            }
            ApiError::Database(
                DbError::CalendarFeedLimit
                | DbError::CalendarFeedDuplicate
                | DbError::CalendarBlockManagedByFeed
                | DbError::CalendarBlockDuplicate,
            ) => StatusCode::CONFLICT,
            ApiError::Database(DbError::CalendarSyncRateLimited { .. }) => {
                StatusCode::TOO_MANY_REQUESTS
            }
            ApiError::Database(DbError::Forbidden(_))
            | ApiError::Database(DbError::TurnoverTransitionForbidden(_)) => StatusCode::FORBIDDEN,
            ApiError::Database(DbError::TurnoverStatusConflict { .. }) => StatusCode::CONFLICT,
            // All other database errors are likely internal server errors.
            ApiError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            // A generic internal error.
            ApiError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            // Feature disabled error is 403 Forbidden
            ApiError::FeatureDisabled(_) => StatusCode::FORBIDDEN,
            ApiError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden(_) => StatusCode::FORBIDDEN,
            ApiError::Unprocessable { .. } => StatusCode::UNPROCESSABLE_ENTITY,
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
        if let Some(response) = self.calendar_error_response() {
            return response;
        }
        match self {
            ApiError::Database(DbError::RequestRefused(refusal)) => {
                tracing::warn!(code = refusal.code(), "request action refused");
                HttpResponse::build(self.status_code()).json(serde_json::json!({
                    "code": refusal.code(),
                    "message": refusal.message(),
                }))
            }
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
            ApiError::Unprocessable { code, message } => {
                tracing::warn!(code, %message, "request refused");
                HttpResponse::UnprocessableEntity()
                    .json(serde_json::json!({ "code": code, "message": message }))
            }
            ApiError::StayPricing(refused) => {
                tracing::warn!(code = refused.code(), %refused, "stay refused");
                let required = match refused {
                    common::pricing::PricingError::MinNightsNotMet { required, .. } => {
                        Some(*required)
                    }
                    common::pricing::PricingError::InvalidDateRange => None,
                };
                HttpResponse::BadRequest().json(serde_json::json!({
                    "code": refused.code(),
                    "message": refused.message(),
                    "required_nights": required,
                }))
            }
            ApiError::Database(DbError::ValidationError(msg)) => {
                HttpResponse::BadRequest().json(serde_json::json!({ "error": msg }))
            }
            ApiError::Database(DbError::StayInput(refused)) => HttpResponse::BadRequest().json(
                serde_json::json!({
                    "code": refused.code(),
                    "message": refused.message()
                }),
            ),
            ApiError::Database(DbError::PromoRefused(refusal)) => HttpResponse::UnprocessableEntity()
                .json(serde_json::json!({
                    "code": refusal.code(),
                    "message": refusal.message()
                })),
            ApiError::Database(DbError::PromoCodeExists) => {
                HttpResponse::Conflict().json(serde_json::json!({
                    "code": "PROMO_CODE_EXISTS",
                    "message": "A promo code with this value already exists."
                }))
            }
            ApiError::Database(DbError::PromoAlreadyDeactivated) => {
                HttpResponse::Conflict().json(serde_json::json!({
                    "code": "PROMO_ALREADY_DEACTIVATED",
                    "message": "This promo code has already been deactivated."
                }))
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
            ApiError::Database(DbError::ListingUnavailable) => {
                HttpResponse::Conflict().json(serde_json::json!({
                    "code": "LISTING_UNAVAILABLE",
                    "message": "This listing is not available for booking."
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
            ApiError::Forbidden(msg) | ApiError::Database(DbError::Forbidden(msg)) => {
                HttpResponse::Forbidden().json(serde_json::json!({ "error": msg }))
            }
            ApiError::Database(DbError::TurnoverTransitionForbidden(err)) => {
                let msg = err.to_string();
                HttpResponse::Forbidden().json(serde_json::json!({
                    "code": "TURNOVER_TRANSITION_FORBIDDEN",
                    "error": msg,
                    "message": msg,
                }))
            }
            ApiError::Database(DbError::TurnoverStatusConflict { expected, actual }) => {
                let msg = format!(
                    "Turnover status changed concurrently (expected {}, found {}). Refresh and try again.",
                    expected.label(),
                    actual.label(),
                );
                HttpResponse::Conflict().json(serde_json::json!({
                    "code": "TURNOVER_STATUS_CONFLICT",
                    "error": msg,
                    "message": msg,
                    "expected_status": expected,
                    "actual_status": actual,
                }))
            }
            _ => HttpResponse::build(self.status_code()).finish(),
        }
    }
}

impl ApiError {
    /// Structured bodies for calendar sync refusals, logged at `warn`; `None`
    /// for every other error.
    fn calendar_error_response(&self) -> Option<HttpResponse> {
        let ApiError::Database(db) = self else {
            return None;
        };
        let (code, message) = match db {
            DbError::CalendarFeedLimit => (
                "CALENDAR_FEED_LIMIT",
                "A listing can import at most 5 calendars.".to_string(),
            ),
            DbError::CalendarFeedDuplicate => (
                "CALENDAR_FEED_DUPLICATE",
                "This calendar is already imported for the listing.".to_string(),
            ),
            DbError::CalendarBlockManagedByFeed => (
                "CALENDAR_BLOCK_MANAGED_BY_FEED",
                "These dates come from an imported calendar. Change them in that calendar."
                    .to_string(),
            ),
            DbError::CalendarBlockDuplicate => (
                "CALENDAR_BLOCK_DUPLICATE",
                "These dates are already blocked.".to_string(),
            ),
            DbError::CalendarSyncRateLimited { retry_after_secs } => {
                tracing::warn!(retry_after_secs, "calendar sync rate limited");
                return Some(
                    HttpResponse::TooManyRequests()
                        .insert_header(("Retry-After", retry_after_secs.max(&1).to_string()))
                        .json(serde_json::json!({
                            "code": "CALENDAR_SYNC_RATE_LIMITED",
                            "message": "Calendars were synced less than a minute ago.",
                            "retry_after_secs": retry_after_secs,
                        })),
                );
            }
            _ => return None,
        };
        tracing::warn!(code, "calendar request refused");
        Some(
            HttpResponse::build(self.status_code())
                .json(serde_json::json!({ "code": code, "message": message })),
        )
    }

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
/// Writes an error and each of its causes.
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
/// A result with an [`ApiError`].
pub type Result<T> = std::result::Result<T, ApiError>;

#[cfg(test)]
mod error_mapping_tests {
    use super::*;
    use actix_web::body::to_bytes;
    use common::availability::StayInputError;
    use common::booking_request::RequestRefusal;
    use common::currency::ListingRef;
    use common::pricing::PricingError;
    use common::promo::PromoRefusal;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    async fn render_api(api: ApiError) -> (StatusCode, serde_json::Value, Option<String>) {
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
    async fn error_status_code_and_payload_table() {
        for refused in [
            StayInputError::DateMalformed,
            StayInputError::DateMissing,
            StayInputError::CheckOutNotAfterCheckIn,
            StayInputError::InPast,
            StayInputError::TooLong { max_nights: 30 },
            StayInputError::InvalidGuests,
            StayInputError::InvalidSearchText,
        ] {
            let (status, body, _) =
                render_api(ApiError::Database(DbError::StayInput(refused))).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{refused:?}");
            assert_eq!(body["code"], refused.code(), "{refused:?}");
            assert_eq!(body["message"], refused.message(), "{refused:?}");
        }

        for refusal in RequestRefusal::ALL {
            let (status, body, _) =
                render_api(ApiError::Database(DbError::RequestRefused(refusal))).await;
            assert_eq!(status.as_u16(), refusal.http_status(), "{refusal:?}");
            assert!(!status.is_server_error(), "{refusal:?}");
            assert_eq!(body["code"], refusal.code(), "{refusal:?}");
            assert_eq!(body["message"], refusal.message(), "{refusal:?}");
        }

        for refusal in [
            PromoRefusal::NotFound,
            PromoRefusal::Deactivated,
            PromoRefusal::NotYetValid,
            PromoRefusal::Expired,
            PromoRefusal::ListingNotEligible,
            PromoRefusal::FxRateUnavailable,
            PromoRefusal::MinNightsNotMet,
            PromoRefusal::StayDatesNotEligible,
            PromoRefusal::MinValueNotMet,
            PromoRefusal::UsageLimitReached,
            PromoRefusal::GuestLimitReached,
        ] {
            let (status, body, _) =
                render_api(ApiError::Database(DbError::PromoRefused(refusal))).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refusal:?}");
            assert_eq!(body["code"], refusal.code(), "{refusal:?}");
            assert_eq!(body["message"], refusal.message(), "{refusal:?}");
        }

        let cases: Vec<(ApiError, StatusCode, &str)> = vec![
            (
                ApiError::Database(DbError::PromoCodeExists),
                StatusCode::CONFLICT,
                "PROMO_CODE_EXISTS",
            ),
            (
                ApiError::Database(DbError::PromoAlreadyDeactivated),
                StatusCode::CONFLICT,
                "PROMO_ALREADY_DEACTIVATED",
            ),
            (
                ApiError::Database(DbError::CalendarFeedLimit),
                StatusCode::CONFLICT,
                "CALENDAR_FEED_LIMIT",
            ),
            (
                ApiError::Database(DbError::CalendarFeedDuplicate),
                StatusCode::CONFLICT,
                "CALENDAR_FEED_DUPLICATE",
            ),
            (
                ApiError::Database(DbError::CalendarBlockManagedByFeed),
                StatusCode::CONFLICT,
                "CALENDAR_BLOCK_MANAGED_BY_FEED",
            ),
            (
                ApiError::Database(DbError::CalendarBlockDuplicate),
                StatusCode::CONFLICT,
                "CALENDAR_BLOCK_DUPLICATE",
            ),
            (
                ApiError::Unprocessable {
                    code: "CALENDAR_FEED_SELF",
                    message: "no".into(),
                },
                StatusCode::UNPROCESSABLE_ENTITY,
                "CALENDAR_FEED_SELF",
            ),
            (
                ApiError::Database(DbError::CurrencyNotFound("XYZ".into())),
                StatusCode::NOT_FOUND,
                "CURRENCY_NOT_FOUND",
            ),
            (
                ApiError::Database(DbError::CurrencyInactive("CAD".into())),
                StatusCode::UNPROCESSABLE_ENTITY,
                "CURRENCY_INACTIVE",
            ),
            (
                ApiError::Database(DbError::CurrencyExists("USD".into())),
                StatusCode::CONFLICT,
                "CURRENCY_EXISTS",
            ),
            (
                ApiError::Database(DbError::EffectiveAtInPast),
                StatusCode::UNPROCESSABLE_ENTITY,
                "EFFECTIVE_AT_IN_PAST",
            ),
            (
                ApiError::Database(DbError::EffectiveAtTooFar),
                StatusCode::UNPROCESSABLE_ENTITY,
                "EFFECTIVE_AT_TOO_FAR",
            ),
            (
                ApiError::Database(DbError::RateAlreadyEffective),
                StatusCode::CONFLICT,
                "RATE_ALREADY_EFFECTIVE",
            ),
            (
                ApiError::Database(DbError::RateVersionExists),
                StatusCode::CONFLICT,
                "RATE_VERSION_EXISTS",
            ),
        ];

        for (api, expected_status, expected_code) in cases {
            let (status, body, retry) = render_api(api).await;
            assert_eq!(status, expected_status, "{expected_code}");
            assert_eq!(body["code"], expected_code);
            assert!(body["message"].as_str().is_some_and(|m| !m.is_empty()));
            assert_eq!(retry, None);
        }

        // Structured payload variants
        let (s, b, _) = render_api(ApiError::StayPricing(PricingError::MinNightsNotMet {
            required: 5,
            provided: 4,
        }))
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(b["code"], "MINIMUM_STAY_NOT_MET");
        assert_eq!(b["required_nights"], 5);

        let (s, b, _) =
            render_api(ApiError::Database(DbError::CurrencyUnknown("XYZ".into()))).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(b["code"], "CURRENCY_NOT_FOUND");
        assert_eq!(b["currency"], "XYZ");

        let listing = ListingRef {
            id: uuid::Uuid::nil(),
            title: "Villa Azul".into(),
            slug: "villa-azul".into(),
        };
        let (s, b, _) = render_api(ApiError::Database(DbError::CurrencyInUse(vec![listing]))).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert_eq!(b["code"], "CURRENCY_IN_USE");
        assert_eq!(b["listings"][0]["title"], "Villa Azul");
        assert_eq!(b["listings"][0]["slug"], "villa-azul");

        let dec = |s: &str| Decimal::from_str(s).expect("decimal");
        let (s, b, _) = render_api(ApiError::Database(
            DbError::RateChangeConfirmationRequired {
                baseline: dec("160.000000"),
                proposed: dec("176"),
                deviation_pct: Some(dec("10.00")),
            },
        ))
        .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(b["code"], "RATE_CHANGE_CONFIRMATION_REQUIRED");
        assert_eq!(b["baseline_rate"], "160.000000");
        assert_eq!(b["proposed_rate"], "176");
        assert_eq!(b["deviation_pct"], "10.00");
    }

    #[tokio::test]
    async fn rate_limit_sets_retry_after_header_with_one_second_floor() {
        let (status, body, retry) =
            render_api(ApiError::Database(DbError::CalendarSyncRateLimited {
                retry_after_secs: 42,
            }))
            .await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(body["code"], "CALENDAR_SYNC_RATE_LIMITED");
        assert_eq!(body["retry_after_secs"], 42);
        assert_eq!(retry.as_deref(), Some("42"));

        let (_, _, retry_floor) =
            render_api(ApiError::Database(DbError::CalendarSyncRateLimited {
                retry_after_secs: 0,
            }))
            .await;
        assert_eq!(retry_floor.as_deref(), Some("1"));
    }

    #[tokio::test]
    async fn lock_contention_is_retryable_503_and_sqlstates_match() {
        let (s, b, retry) = render_api(ApiError::Database(DbError::LockContention(
            "rate publish".into(),
        )))
        .await;
        assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(b["code"], "SERVICE_UNAVAILABLE");
        assert_eq!(retry.as_deref(), Some("1"));

        let other = DbError::from_sqlx_contended(sqlx::Error::RowNotFound, "rate publish");
        assert!(matches!(other, DbError::Sqlx(sqlx::Error::RowNotFound)));
        assert_eq!(db_core::error::SQLSTATE_DEADLOCK_DETECTED, "40P01");
        assert_eq!(db_core::error::SQLSTATE_LOCK_NOT_AVAILABLE, "55P03");
    }
}
