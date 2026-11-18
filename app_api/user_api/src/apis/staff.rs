//! Host-scoped cleaning/maintenance staff roster and listing assignment endpoints.

use actix_web::http::StatusCode;
use actix_web::{HttpRequest, Responder, web};
use api_core::auth::Claims;
use api_core::error::ApiError;
use api_core::response::{Payload, respond};
use common::turnover::{
    InviteStaffRequest, StaffListQuery, StaffListResponse, StaffMemberSummary,
    UpdateStaffAssignmentsRequest,
};
use db_core::models::{User, UserRole};
use sqlx::PgPool;
use tracing::Instrument;
use uuid::Uuid;
use validator::Validate;

use super::ActionStatusResponse;
use super::account::admin_base_url;

/// Extracts the authenticated active user and verifies they hold `Host` or `Admin`.
async fn require_host_or_admin(pool: &PgPool, claims: &Claims) -> Result<(User, bool), ApiError> {
    let user = db_core::user::get_user_by_id(pool, claims.sub)
        .await
        .map_err(|e| match e {
            db_core::error::DbError::Sqlx(sqlx::Error::RowNotFound) => {
                ApiError::Unauthorized("Unknown user".to_string())
            }
            other => ApiError::Database(other),
        })?;

    if !user.is_active || user.deleted_at.is_some() {
        return Err(ApiError::Unauthorized("Account is not active".to_string()));
    }

    let is_admin = user.roles.contains(&UserRole::Admin);
    let is_host = user.roles.contains(&UserRole::Host);
    if !is_admin && !is_host {
        return Err(ApiError::Forbidden(
            "Only hosts and administrators can manage staff users".to_string(),
        ));
    }

    Ok((user, is_admin))
}

fn spawn_outbox_publish(email_id: Uuid, staff_user_id: Uuid) {
    tokio::spawn(
        async move {
            let publisher = api_core::email_publisher::EmailPublisher::from_env();
            if let Err(e) = publisher.publish_email_event(email_id).await {
                tracing::error!(
                    %email_id,
                    %staff_user_id,
                    error = ?e,
                    "failed to publish staff email event"
                );
            }
        }
        .instrument(tracing::Span::current()),
    );
}

/// Lists cleaning/maintenance staff members within the caller's host scope (or all staff for admin).
#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v1/staff-users",
    tag = "staff",
    params(StaffListQuery),
    responses(
        (status = 200, description = "Host-scoped staff roster", body = StaffListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden")
    )
)]
pub(super) async fn list_staff_users(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    claims: Claims,
    query: web::Query<StaffListQuery>,
) -> Result<impl Responder, ApiError> {
    let (caller, is_admin) = require_host_or_admin(pool.get_ref(), &claims).await?;
    let response = db_core::staff::list_staff_users(pool.get_ref(), caller.id, is_admin, &query)
        .await
        .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Item(response),
        |_: Vec<StaffListResponse>| (),
        StatusCode::OK,
    ))
}

/// Invites a new staff member by email (or links an existing active user to the caller's roster)
/// and assigns them to one or more owned active listings.
#[tracing::instrument(skip_all)]
#[utoipa::path(
    post,
    path = "/api/v1/staff-users",
    tag = "staff",
    request_body = InviteStaffRequest,
    responses(
        (status = 200, description = "Existing staff member updated", body = StaffMemberSummary),
        (status = 201, description = "Staff member invited or attached", body = StaffMemberSummary),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden")
    )
)]
pub(super) async fn invite_staff_user(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    claims: Claims,
    payload: web::Json<InviteStaffRequest>,
) -> Result<impl Responder, ApiError> {
    let (caller, is_admin) = require_host_or_admin(pool.get_ref(), &claims).await?;
    let body = payload.into_inner();
    body.validate()?;

    let outcome = db_core::staff::invite_or_attach_staff(
        pool.get_ref(),
        caller.id,
        is_admin,
        &body,
        &admin_base_url(),
    )
    .await
    .map_err(ApiError::Database)?;

    if let Some(email_id) = outcome.outbox_email_id {
        spawn_outbox_publish(email_id, outcome.staff.user_id);
    }

    let status = if outcome.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };

    Ok(respond(
        &req,
        Payload::Item(outcome.staff),
        |_: Vec<StaffMemberSummary>| (),
        status,
    ))
}

/// Replaces the target staff member's listing assignments within the caller's ownership scope.
#[tracing::instrument(skip_all)]
#[utoipa::path(
    put,
    path = "/api/v1/staff-users/{id}/listings",
    tag = "staff",
    params(("id" = Uuid, Path, description = "Staff user ID")),
    request_body = UpdateStaffAssignmentsRequest,
    responses(
        (status = 200, description = "Staff listing assignments updated", body = StaffMemberSummary),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Staff member not in caller roster")
    )
)]
pub(super) async fn update_staff_listings(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    claims: Claims,
    path: web::Path<Uuid>,
    payload: web::Json<UpdateStaffAssignmentsRequest>,
) -> Result<impl Responder, ApiError> {
    let (caller, is_admin) = require_host_or_admin(pool.get_ref(), &claims).await?;
    let staff_user_id = path.into_inner();
    let body = payload.into_inner();

    let (summary, outbox_email_id) = db_core::staff::update_staff_listings(
        pool.get_ref(),
        caller.id,
        is_admin,
        staff_user_id,
        &body.listing_ids,
        &admin_base_url(),
    )
    .await
    .map_err(ApiError::Database)?;

    if let Some(email_id) = outbox_email_id {
        spawn_outbox_publish(email_id, staff_user_id);
    }

    Ok(respond(
        &req,
        Payload::Item(summary),
        |_: Vec<StaffMemberSummary>| (),
        StatusCode::OK,
    ))
}

/// Invalidates prior pending invite links and emails a fresh onboarding link to the staff member.
#[tracing::instrument(skip_all)]
#[utoipa::path(
    post,
    path = "/api/v1/staff-users/{id}/resend-invite",
    tag = "staff",
    params(("id" = Uuid, Path, description = "Staff user ID")),
    responses(
        (status = 200, description = "Invite link resent", body = ActionStatusResponse),
        (status = 400, description = "Password already set or account inactive"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Staff member not in caller roster")
    )
)]
pub(super) async fn resend_staff_invite(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    claims: Claims,
    path: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    let (caller, is_admin) = require_host_or_admin(pool.get_ref(), &claims).await?;
    let staff_user_id = path.into_inner();

    let email_id = db_core::staff::resend_staff_invite(
        pool.get_ref(),
        caller.id,
        is_admin,
        staff_user_id,
        &admin_base_url(),
    )
    .await
    .map_err(ApiError::Database)?;

    spawn_outbox_publish(email_id, staff_user_id);

    Ok(respond(
        &req,
        Payload::Item(ActionStatusResponse {
            status: "invite_resent".to_string(),
        }),
        |_: Vec<ActionStatusResponse>| (),
        StatusCode::OK,
    ))
}

/// Unlinks the staff member from the caller's roster and owned listings; removes `'staff'` role and
/// revokes admin portal sessions if no links remain across any host.
#[tracing::instrument(skip_all)]
#[utoipa::path(
    delete,
    path = "/api/v1/staff-users/{id}",
    tag = "staff",
    params(("id" = Uuid, Path, description = "Staff user ID")),
    responses(
        (status = 200, description = "Staff member unlinked", body = ActionStatusResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Staff member not in caller roster")
    )
)]
pub(super) async fn remove_staff_user(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    claims: Claims,
    path: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    let (caller, is_admin) = require_host_or_admin(pool.get_ref(), &claims).await?;
    let staff_user_id = path.into_inner();

    db_core::staff::remove_staff_member(pool.get_ref(), caller.id, is_admin, staff_user_id)
        .await
        .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Item(ActionStatusResponse {
            status: "removed".to_string(),
        }),
        |_: Vec<ActionStatusResponse>| (),
        StatusCode::OK,
    ))
}
