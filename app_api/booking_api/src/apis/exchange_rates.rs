//! Exchange-rate administration (#100). Staff (admins and hosts) read the
//! current rates; only admins see history and coverage, publish rates, delete
//! scheduled rates and retire pairs.

use actix_web::{HttpResponse, web};
use api_core::error::ApiError;
use api_core::settings::Settings;
use chrono::Utc;
use common::currency::CurrencyCode;
use common::exchange_rate::{
    ExchangeRatesResponse, PairHistoryResponse, PublishRateRequest, RateCoverageGap, RateVersion,
};
use db_core::error::DbError;
use sqlx::PgPool;
use uuid::Uuid;

use super::{require_admin, require_staff};

fn bad_request(e: impl std::fmt::Display) -> ApiError {
    ApiError::Database(DbError::ValidationError(e.to_string()))
}

/// Parses a currency code from the URL path (400 when malformed).
fn path_code(raw: &str) -> Result<CurrencyCode, ApiError> {
    CurrencyCode::try_from(raw).map_err(|e| {
        tracing::warn!(code = raw, "rejected malformed currency code in path");
        bad_request(e)
    })
}

fn log_db(action: &'static str) -> impl Fn(DbError) -> ApiError {
    move |e| {
        if matches!(e, DbError::Sqlx(_)) {
            tracing::error!(action, error = ?e, "exchange-rate database operation failed");
        }
        ApiError::Database(e)
    }
}

/// The current rate for every explicit, non-retired pair. Admins and hosts.
#[utoipa::path(
    get,
    path = "/api/v1/admin/exchange-rates",
    tag = "exchange-rates",
    responses(
        (status = 200, description = "Current rate per pair", body = ExchangeRatesResponse),
        (status = 401, description = "Missing or invalid token"),
        (status = 403, description = "Caller is neither an admin nor a host")
    )
)]
#[tracing::instrument(skip(pool))]
pub(super) async fn current_rates(
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ApiError> {
    require_staff(pool.get_ref(), &claims, "view exchange rates").await?;
    db_core::exchange_rate::list_current_rates(pool.get_ref(), Utc::now())
        .await
        .map(|rates| HttpResponse::Ok().json(ExchangeRatesResponse { rates }))
        .map_err(log_db("list current rates"))
}

/// Live listing base currencies that cannot be quoted in an active currency.
#[utoipa::path(
    get,
    path = "/api/v1/admin/exchange-rates/coverage",
    tag = "exchange-rates",
    responses(
        (status = 200, description = "Unquotable base/target combinations", body = [RateCoverageGap]),
        (status = 403, description = "Caller is not an admin")
    )
)]
#[tracing::instrument(skip(pool))]
pub(super) async fn coverage(
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ApiError> {
    require_admin(pool.get_ref(), &claims, "view rate coverage").await?;
    db_core::currency::rate_coverage_gaps(pool.get_ref(), Utc::now())
        .await
        .map(|gaps| HttpResponse::Ok().json(gaps))
        .map_err(log_db("rate coverage"))
}

/// Every version of one pair with its state. Admin only.
#[utoipa::path(
    get,
    path = "/api/v1/admin/exchange-rates/{base}/{target}/history",
    tag = "exchange-rates",
    params(
        ("base" = String, Path, description = "Base currency code"),
        ("target" = String, Path, description = "Target currency code")
    ),
    responses(
        (status = 200, description = "Pair history", body = PairHistoryResponse),
        (status = 400, description = "Malformed currency code"),
        (status = 403, description = "Caller is not an admin"),
        (status = 404, description = "No such pair")
    )
)]
#[tracing::instrument(skip(pool))]
pub(super) async fn history(
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    require_admin(pool.get_ref(), &claims, "view rate history").await?;
    let (base, target) = path.into_inner();
    let (base, target) = (path_code(&base)?, path_code(&target)?);
    let mut conn = pool.acquire().await.map_err(|e| {
        tracing::error!(error = %e, "could not acquire a connection for rate history");
        ApiError::Database(e.into())
    })?;
    db_core::exchange_rate::pair_history(&mut conn, &base, &target, Utc::now())
        .await
        .map(|h| HttpResponse::Ok().json(h))
        .map_err(log_db("pair history"))
}

/// Publishes a rate version (creating or reinstating the pair). Admin only.
#[utoipa::path(
    post,
    path = "/api/v1/admin/exchange-rates",
    tag = "exchange-rates",
    request_body = PublishRateRequest,
    responses(
        (status = 201, description = "Rate version stored", body = RateVersion),
        (status = 400, description = "Malformed codes or rate"),
        (status = 403, description = "Caller is not an admin"),
        (status = 409, description = "RATE_VERSION_EXISTS"),
        (status = 422, description = "EFFECTIVE_AT_IN_PAST, EFFECTIVE_AT_TOO_FAR, CURRENCY_INACTIVE, CURRENCY_NOT_FOUND or RATE_CHANGE_CONFIRMATION_REQUIRED"),
        (status = 503, description = "Busy (lock contention); retry")
    )
)]
#[tracing::instrument(skip(pool, settings, body))]
pub(super) async fn publish(
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    body: web::Json<PublishRateRequest>,
) -> Result<HttpResponse, ApiError> {
    let admin = require_admin(pool.get_ref(), &claims, "publish exchange rates").await?;
    let body = body.into_inner();
    let rate = common::exchange_rate::parse_rate(&body.rate).map_err(|e| {
        tracing::warn!(rate = %body.rate, error = %e, "rejected exchange rate");
        bad_request(e)
    })?;
    let grace = settings.application.rate_effective_grace().map_err(|e| {
        tracing::error!(error = %e, "invalid rate grace setting");
        ApiError::Internal
    })?;
    let req = db_core::exchange_rate::PublishRate {
        base: body.base_currency,
        target: body.target_currency,
        rate,
        effective_at: body.effective_at,
        confirm_large_change: body.confirm_large_change,
        actor_id: admin.id,
        grace,
    };
    db_core::exchange_rate::publish_rate(pool.get_ref(), &req)
        .await
        .map(|version| {
            tracing::info!(base = %req.base, target = %req.target, rate = %version.rate, admin_id = %admin.id, "exchange rate published");
            HttpResponse::Created().json(version)
        })
        .map_err(log_db("publish rate"))
}

/// Deletes a rate version that has not taken effect yet. Admin only.
#[utoipa::path(
    delete,
    path = "/api/v1/admin/exchange-rates/{id}",
    tag = "exchange-rates",
    params(("id" = Uuid, Path, description = "Rate version id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 403, description = "Caller is not an admin"),
        (status = 404, description = "No such rate version"),
        (status = 409, description = "RATE_ALREADY_EFFECTIVE")
    )
)]
#[tracing::instrument(skip(pool))]
pub(super) async fn delete_scheduled(
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let admin = require_admin(pool.get_ref(), &claims, "delete exchange rates").await?;
    let id = path.into_inner();
    db_core::exchange_rate::delete_scheduled_rate(pool.get_ref(), id, admin.id)
        .await
        .map(|()| {
            tracing::info!(rate_id = %id, admin_id = %admin.id, "scheduled exchange rate deleted");
            HttpResponse::NoContent().finish()
        })
        .map_err(log_db("delete scheduled rate"))
}

/// Retires a pair (history kept). Admin only.
#[utoipa::path(
    post,
    path = "/api/v1/admin/exchange-rates/{base}/{target}/retire",
    tag = "exchange-rates",
    params(
        ("base" = String, Path, description = "Base currency code"),
        ("target" = String, Path, description = "Target currency code")
    ),
    responses(
        (status = 200, description = "Retired; pair history", body = PairHistoryResponse),
        (status = 400, description = "Malformed currency code"),
        (status = 403, description = "Caller is not an admin"),
        (status = 404, description = "No such pair"),
        (status = 409, description = "CURRENCY_IN_USE: listings depend on the pair")
    )
)]
#[tracing::instrument(skip(pool))]
pub(super) async fn retire(
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    let admin = require_admin(pool.get_ref(), &claims, "retire currency pairs").await?;
    let (base, target) = path.into_inner();
    let (base, target) = (path_code(&base)?, path_code(&target)?);
    db_core::exchange_rate::retire_pair(pool.get_ref(), &base, &target, admin.id)
        .await
        .map(|h| {
            tracing::info!(base = %base, target = %target, admin_id = %admin.id, "currency pair retired");
            HttpResponse::Ok().json(h)
        })
        .map_err(log_db("retire pair"))
}
