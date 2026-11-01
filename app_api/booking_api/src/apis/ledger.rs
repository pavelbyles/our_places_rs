//! Host payout ledger, admin payout status and admin exchange rates.

use actix_web::{HttpRequest, HttpResponse, Responder, web};
use api_core::error::ApiError;
use api_core::response::{Payload, respond};
use common::payout::PayoutFilter;
use db_core::payout_ledger as db_payout_ledger;
use sqlx::PgPool;
use uuid::Uuid;

#[tracing::instrument(skip(pool))]
pub(super) async fn get_host_ledger_entries(
    req: HttpRequest,
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
    filter: web::Query<PayoutFilter>,
) -> Result<impl Responder, ApiError> {
    let user = db_core::user::get_user_by_id(pool.get_ref(), claims.sub)
        .await
        .map_err(ApiError::Database)?;
    let is_admin = user.roles.contains(&db_core::models::UserRole::Admin);
    let target_host_id = if is_admin {
        filter.host_id
    } else {
        Some(claims.sub)
    };

    let page = filter.page.unwrap_or(1);
    let per_page = filter
        .per_page
        .unwrap_or(db_payout_ledger::DEFAULT_LEDGER_PAGE_SIZE);
    let filter = PayoutFilter {
        host_id: target_host_id,
        page: Some(page),
        per_page: Some(per_page),
        ..filter.into_inner()
    };

    let (entries, total_count) =
        db_payout_ledger::get_payout_ledger_entries(pool.get_ref(), &filter)
            .await
            .map_err(ApiError::Database)?;

    let response = common::payout::PayoutLedgerResponse {
        entries: entries.into_iter().map(Into::into).collect(),
        total_count,
        page,
        per_page,
    };

    Ok(respond(
        &req,
        Payload::Item(response),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip(pool))]
pub(super) async fn get_host_ledger_summary(
    req: HttpRequest,
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
    filter: web::Query<PayoutFilter>,
) -> Result<impl Responder, ApiError> {
    let user = db_core::user::get_user_by_id(pool.get_ref(), claims.sub)
        .await
        .map_err(ApiError::Database)?;
    let is_admin = user.roles.contains(&db_core::models::UserRole::Admin);
    let target_host_id = if is_admin {
        filter.host_id
    } else {
        Some(claims.sub)
    };

    let summary =
        db_payout_ledger::get_payout_summary(pool.get_ref(), target_host_id, filter.listing_id)
            .await
            .map_err(ApiError::Database)?;

    let common_summary: common::payout::PayoutSummary = summary.into();

    Ok(respond(
        &req,
        Payload::Item(common_summary),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip(pool))]
pub(super) async fn export_host_ledger_csv(
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
    filter: web::Query<PayoutFilter>,
) -> Result<HttpResponse, ApiError> {
    let user = db_core::user::get_user_by_id(pool.get_ref(), claims.sub)
        .await
        .map_err(ApiError::Database)?;
    let is_admin = user.roles.contains(&db_core::models::UserRole::Admin);
    let target_host_id = if is_admin {
        filter.host_id
    } else {
        Some(claims.sub)
    };

    let filter = PayoutFilter {
        host_id: target_host_id,
        page: Some(1),
        per_page: Some(1000),
        ..filter.into_inner()
    };
    let (entries, _) = db_payout_ledger::get_payout_ledger_entries(pool.get_ref(), &filter)
        .await
        .map_err(ApiError::Database)?;

    let common_entries: Vec<common::payout::PayoutLedgerEntry> =
        entries.into_iter().map(Into::into).collect();
    let csv_content = common::csv::format_payout_ledger_csv(&common_entries);

    Ok(HttpResponse::Ok()
        .content_type("text/csv")
        .insert_header((
            "Content-Disposition",
            "attachment; filename=\"host_payout_ledger.csv\"",
        ))
        .body(csv_content))
}

#[tracing::instrument(skip(pool))]
pub(super) async fn update_admin_payout_status(
    req: HttpRequest,
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    body: web::Json<common::payout::UpdatePayoutStatusRequest>,
) -> Result<impl Responder, ApiError> {
    let user = db_core::user::get_user_by_id(pool.get_ref(), claims.sub)
        .await
        .map_err(ApiError::Database)?;
    if !user.roles.contains(&db_core::models::UserRole::Admin) {
        return Err(ApiError::Unauthorized("Admin role required".to_string()));
    }

    let ledger_id = path.into_inner();

    let updated = db_payout_ledger::update_payout_status(
        pool.get_ref(),
        ledger_id,
        body.status,
        body.gateway_reference.clone(),
        body.failure_reason.clone(),
    )
    .await
    .map_err(ApiError::Database)?;

    let common_entry: common::payout::PayoutLedgerEntry = updated.into();

    Ok(respond(
        &req,
        Payload::Item(common_entry),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

/// Admins and hosts (read-only): the newest rate for every stored currency pair.
#[tracing::instrument(skip(pool))]
#[utoipa::path(
    get,
    path = "/api/v1/admin/exchange-rates",
    tag = "admin",
    responses(
        (status = 200, description = "Newest rate per currency pair", body = common::exchange_rate::ExchangeRatesResponse),
        (status = 401, description = "Missing token or not an admin or host"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn get_admin_exchange_rates(
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
) -> Result<impl Responder, ApiError> {
    let user = db_core::user::get_user_by_id(pool.get_ref(), claims.sub)
        .await
        .map_err(ApiError::Database)?;
    let is_staff = user.roles.iter().any(|r| {
        matches!(
            r,
            db_core::models::UserRole::Admin | db_core::models::UserRole::Host
        )
    });
    if !is_staff {
        return Err(ApiError::Unauthorized(
            "Admin or host role required".to_string(),
        ));
    }

    let rates = db_core::currency::list_latest_exchange_rates(pool.get_ref())
        .await
        .map_err(ApiError::Database)?;

    Ok(HttpResponse::Ok().json(common::exchange_rate::ExchangeRatesResponse { rates }))
}
