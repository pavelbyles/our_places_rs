//! Currency reference data (#100): the public list of active currencies and
//! admin-only management. Currencies are never deleted, only deactivated.

use actix_web::{HttpResponse, web};
use api_core::error::ApiError;
use common::currency::{
    CreateCurrencyRequest, CurrenciesResponse, Currency, CurrencyCode, CurrencyRequestError,
    UpdateCurrencyRequest,
};
use db_core::error::DbError;
use sqlx::PgPool;

use super::require_admin;

/// A request-shape problem, as a 400 with a readable message.
fn bad_request(e: impl std::fmt::Display) -> ApiError {
    ApiError::Database(DbError::ValidationError(e.to_string()))
}

fn invalid_request(e: CurrencyRequestError) -> ApiError {
    tracing::warn!(error = %e, "rejected currency request");
    bad_request(e)
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
        tracing::error!(action, error = ?e, "currency database operation failed");
        ApiError::Database(e)
    }
}

/// Active currencies, for currency selectors. Public.
#[utoipa::path(
    get,
    path = "/api/v1/currencies",
    tag = "currencies",
    responses(
        (status = 200, description = "Active currencies, sorted by code", body = CurrenciesResponse),
        (status = 500, description = "Internal server error")
    )
)]
#[tracing::instrument(skip(pool))]
pub(super) async fn list_active_currencies(
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ApiError> {
    db_core::currency::list_currencies(pool.get_ref(), true)
        .await
        .map(|currencies| HttpResponse::Ok().json(CurrenciesResponse { currencies }))
        .map_err(log_db("list active currencies"))
}

/// Every currency, including inactive ones. Admin only.
#[utoipa::path(
    get,
    path = "/api/v1/admin/currencies",
    tag = "currencies",
    responses(
        (status = 200, description = "All currencies, sorted by code", body = CurrenciesResponse),
        (status = 401, description = "Missing or invalid token"),
        (status = 403, description = "Caller is not an admin")
    )
)]
#[tracing::instrument(skip(pool))]
pub(super) async fn admin_list_currencies(
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ApiError> {
    require_admin(pool.get_ref(), &claims, "list all currencies").await?;
    db_core::currency::list_currencies(pool.get_ref(), false)
        .await
        .map(|currencies| HttpResponse::Ok().json(CurrenciesResponse { currencies }))
        .map_err(log_db("list all currencies"))
}

/// Adds a currency. Admin only.
#[utoipa::path(
    post,
    path = "/api/v1/admin/currencies",
    tag = "currencies",
    request_body = CreateCurrencyRequest,
    responses(
        (status = 201, description = "Currency created", body = Currency),
        (status = 400, description = "Malformed code or name"),
        (status = 403, description = "Caller is not an admin"),
        (status = 409, description = "CURRENCY_EXISTS")
    )
)]
#[tracing::instrument(skip(pool, body))]
pub(super) async fn create_currency(
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
    body: web::Json<CreateCurrencyRequest>,
) -> Result<HttpResponse, ApiError> {
    let admin = require_admin(pool.get_ref(), &claims, "add currencies").await?;
    let req = body.into_inner().validated().map_err(invalid_request)?;
    db_core::currency::create_currency(pool.get_ref(), &req, admin.id)
        .await
        .map(|currency| HttpResponse::Created().json(currency))
        .map_err(ApiError::Database)
}

/// Renames and/or (de)activates a currency. Admin only.
#[utoipa::path(
    patch,
    path = "/api/v1/admin/currencies/{code}",
    tag = "currencies",
    params(("code" = String, Path, description = "Currency code, e.g. JMD")),
    request_body = UpdateCurrencyRequest,
    responses(
        (status = 200, description = "Currency updated", body = Currency),
        (status = 400, description = "Malformed code, name or empty update"),
        (status = 403, description = "Caller is not an admin"),
        (status = 404, description = "CURRENCY_NOT_FOUND"),
        (status = 409, description = "CURRENCY_IN_USE: live listings are priced in it")
    )
)]
#[tracing::instrument(skip(pool, body))]
pub(super) async fn update_currency(
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
    path: web::Path<String>,
    body: web::Json<UpdateCurrencyRequest>,
) -> Result<HttpResponse, ApiError> {
    let admin = require_admin(pool.get_ref(), &claims, "change currencies").await?;
    let code = path_code(&path.into_inner())?;
    let req = body.into_inner().validated().map_err(invalid_request)?;
    db_core::currency::update_currency(pool.get_ref(), &code, &req, admin.id)
        .await
        .map(|currency| HttpResponse::Ok().json(currency))
        .map_err(ApiError::Database)
}
