//! Promo code management. Admins manage every code; hosts manage only their own,
//! for their own listings. Code strings never enter a log: only IDs do.

use super::{is_admin, require_staff};
use actix_web::{HttpResponse, web};
use api_core::auth::Claims;
use api_core::error::ApiError;
use common::promo::{
    CreatePromoCodeRequest, PromoCodeResponse, PromoCodesResponse, validate_new_code,
};
use db_core::error::DbError;
use db_core::models::User;
use db_core::promo::{NewPromoCode, PromoCodeWithStats};
use sqlx::PgPool;
use uuid::Uuid;

/// Codes per page when the caller doesn't ask for a size.
const DEFAULT_PAGE_SIZE: u32 = 20;

/// Which page of codes to return.
#[derive(serde::Deserialize, utoipa::IntoParams)]
pub(super) struct PromoCodeListQuery {
    /// Page number, from 1.
    page: Option<u32>,
    /// Codes per page, at most 100.
    per_page: Option<u32>,
}

fn response(stats: PromoCodeWithStats) -> PromoCodeResponse {
    let code = stats.code;
    let used = stats.active_holds + stats.confirmed_uses;
    PromoCodeResponse {
        id: code.id,
        code: code.code,
        discount_type: code.discount_type,
        discount_value: code.discount_value,
        currency: code.currency.map(|c| c.trim().to_string()),
        scope: code.scope,
        valid_from: code.valid_from,
        valid_to: code.valid_to,
        stay_from: code.stay_from,
        stay_to: code.stay_to,
        min_nights: code.min_nights,
        min_booking_value: code.min_booking_value,
        total_usage_limit: code.total_usage_limit,
        per_guest_limit: code.per_guest_limit,
        applies_to_all: code.applies_to_all,
        listing_ids: stats.listing_ids,
        owner_host_id: code.owner_host_id,
        deactivated_at: code.deactivated_at,
        active_holds: stats.active_holds,
        confirmed_uses: stats.confirmed_uses,
        remaining_uses: code
            .total_usage_limit
            .map(|limit| (i64::from(limit) - used).max(0)),
        created_at: code.created_at,
    }
}

/// Loads a code the caller may manage. Admins may manage any; a host only their
/// own, which leaves admin-created codes out of reach.
async fn manageable_code(
    pool: &PgPool,
    user: &User,
    id: Uuid,
    action: &str,
) -> Result<PromoCodeWithStats, ApiError> {
    let stats = db_core::promo::get_code(pool, id).await?;
    if is_admin(user) || stats.code.owner_host_id == Some(user.id) {
        return Ok(stats);
    }
    tracing::warn!(user_id = %user.id, promo_code_id = %id, action, "host attempted to manage a code they don't own");
    Err(ApiError::Forbidden(format!(
        "You can only {action} your own promo codes"
    )))
}

/// Lists promo codes with usage: every code for admins, their own for hosts.
#[utoipa::path(
    get,
    path = "/api/v1/admin/promo-codes",
    tag = "promo-codes",
    params(PromoCodeListQuery),
    responses(
        (status = 200, description = "One page of codes", body = PromoCodesResponse),
        (status = 401, description = "Missing or invalid token"),
        (status = 403, description = "Caller is not an admin or host")
    )
)]
#[tracing::instrument(skip_all, fields(user_id = %claims.sub))]
pub(super) async fn list_promo_codes(
    claims: Claims,
    pool: web::Data<PgPool>,
    query: web::Query<PromoCodeListQuery>,
) -> Result<HttpResponse, ApiError> {
    let user = require_staff(pool.get_ref(), &claims, "manage promo codes").await?;
    let owner = (!is_admin(&user)).then_some(user.id);
    let (codes, total) = db_core::promo::list_codes(
        pool.get_ref(),
        owner,
        query.page.unwrap_or(1),
        query.per_page.unwrap_or(DEFAULT_PAGE_SIZE),
    )
    .await?;
    Ok(HttpResponse::Ok().json(PromoCodesResponse {
        codes: codes.into_iter().map(response).collect(),
        total,
    }))
}

/// One promo code with its usage.
#[utoipa::path(
    get,
    path = "/api/v1/admin/promo-codes/{id}",
    tag = "promo-codes",
    responses(
        (status = 200, description = "The code", body = PromoCodeResponse),
        (status = 401, description = "Missing or invalid token"),
        (status = 403, description = "Caller doesn't manage this code"),
        (status = 404, description = "No such code")
    )
)]
#[tracing::instrument(skip_all, fields(user_id = %claims.sub, promo_code_id = %*id))]
pub(super) async fn get_promo_code(
    claims: Claims,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let user = require_staff(pool.get_ref(), &claims, "manage promo codes").await?;
    let stats = manageable_code(pool.get_ref(), &user, *id, "view").await?;
    Ok(HttpResponse::Ok().json(response(stats)))
}

/// Creates a promo code. A host's code is theirs and can only select their own
/// listings. An admin's code has no owner.
#[utoipa::path(
    post,
    path = "/api/v1/admin/promo-codes",
    tag = "promo-codes",
    request_body = CreatePromoCodeRequest,
    responses(
        (status = 201, description = "The new code", body = PromoCodeResponse),
        (status = 400, description = "Invalid terms"),
        (status = 401, description = "Missing or invalid token"),
        (status = 403, description = "Caller is not an admin or host, or selected a listing they don't own"),
        (status = 409, description = "PROMO_CODE_EXISTS"),
        (status = 422, description = "CURRENCY_INACTIVE or CURRENCY_NOT_FOUND")
    )
)]
#[tracing::instrument(skip_all, fields(user_id = %claims.sub))]
pub(super) async fn create_promo_code(
    claims: Claims,
    pool: web::Data<PgPool>,
    body: web::Json<CreatePromoCodeRequest>,
) -> Result<HttpResponse, ApiError> {
    let user = require_staff(pool.get_ref(), &claims, "manage promo codes").await?;
    let body = body.into_inner();
    let code = validate_new_code(&body).map_err(|reason| {
        tracing::warn!(user_id = %user.id, %reason, "rejected promo code terms");
        ApiError::Database(DbError::ValidationError(reason))
    })?;

    let admin = is_admin(&user);
    let owners = db_core::promo::listing_owners(pool.get_ref(), &body.listing_ids).await?;
    if owners.len() != body.listing_ids.len() {
        return Err(ApiError::Database(DbError::ValidationError(
            "A selected listing doesn't exist".to_string(),
        )));
    }
    if !admin && owners.iter().any(|(_, owner)| *owner != user.id) {
        tracing::warn!(user_id = %user.id, "host attempted to create a code for a listing they don't own");
        return Err(ApiError::Forbidden(
            "You can only create promo codes for your own listings".to_string(),
        ));
    }

    let id = db_core::promo::create_code(
        pool.get_ref(),
        &NewPromoCode {
            code,
            discount_type: body.discount_type,
            discount_value: body.discount_value,
            currency: body.currency,
            scope: body.scope,
            valid_from: body.valid_from,
            valid_to: body.valid_to,
            stay_from: body.stay_from,
            stay_to: body.stay_to,
            min_nights: body.min_nights,
            min_booking_value: body.min_booking_value,
            total_usage_limit: body.total_usage_limit,
            per_guest_limit: body.per_guest_limit,
            applies_to_all: body.applies_to_all,
            listing_ids: body.listing_ids,
            owner_host_id: (!admin).then_some(user.id),
            created_by: user.id,
        },
    )
    .await?;
    tracing::info!(promo_code_id = %id, actor_id = %user.id, "promo code created");

    let stats = db_core::promo::get_code(pool.get_ref(), id).await?;
    Ok(HttpResponse::Created().json(response(stats)))
}

/// Deactivates a promo code, which can't be undone. Live holds and existing
/// bookings keep their discount.
#[utoipa::path(
    post,
    path = "/api/v1/admin/promo-codes/{id}/deactivate",
    tag = "promo-codes",
    responses(
        (status = 200, description = "The deactivated code", body = PromoCodeResponse),
        (status = 401, description = "Missing or invalid token"),
        (status = 403, description = "Caller doesn't manage this code"),
        (status = 404, description = "No such code"),
        (status = 409, description = "PROMO_ALREADY_DEACTIVATED")
    )
)]
#[tracing::instrument(skip_all, fields(user_id = %claims.sub, promo_code_id = %*id))]
pub(super) async fn deactivate_promo_code(
    claims: Claims,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let user = require_staff(pool.get_ref(), &claims, "manage promo codes").await?;
    manageable_code(pool.get_ref(), &user, *id, "deactivate").await?;
    db_core::promo::deactivate_code(pool.get_ref(), *id, user.id).await?;
    tracing::info!(promo_code_id = %*id, actor_id = %user.id, "promo code deactivated");
    let stats = db_core::promo::get_code(pool.get_ref(), *id).await?;
    Ok(HttpResponse::Ok().json(response(stats)))
}
