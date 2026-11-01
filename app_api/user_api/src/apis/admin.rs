//! Admin user management: listing, soft/hard delete and restore.

use super::{UserFilter, UserResponse, UsersWrapper, map_user_to_response};
use actix_web::{HttpRequest, Responder, web};
use api_core::error::ApiError;
use api_core::pagination;
use api_core::response::{Payload, respond};
use db_core::models::UserRole;
use sqlx::PgPool;
use uuid::Uuid;

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v1/users",
    tag = "users",
    params(
        pagination::Pagination,
        UserFilter
    ),
    responses(
        (status = 200, description = "List of users", body = [UserResponse]),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn get_all_users(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    query: web::Query<pagination::Pagination>,
    filter: web::Query<UserFilter>,
) -> Result<impl Responder, ApiError> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(10).min(100);

    let is_deleted_filter = if filter.all.unwrap_or(false) {
        None
    } else {
        Some(filter.is_deleted.unwrap_or(false))
    };

    let users = db_core::user::get_all_users(
        pool.get_ref(),
        page,
        per_page,
        filter.search.clone(),
        is_deleted_filter,
    )
    .await
    .map_err(ApiError::Database)?;

    let response: Vec<UserResponse> = users.into_iter().map(map_user_to_response).collect();

    Ok(respond(
        &req,
        Payload::Collection(response),
        |items| UsersWrapper { user: items },
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    delete,
    path = "/api/v1/users/user/{id}",
    tag = "users",
    params(
        ("id" = Uuid, Path, description = "User ID"),
    ),
    responses(
        (status = 200, description = "User soft deleted", body = UserResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn soft_delete_user(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    let id = path.into_inner();
    let target_user = db_core::user::get_user_by_id(pool.get_ref(), id)
        .await
        .map_err(ApiError::Database)?;

    if target_user.roles.contains(&UserRole::Admin) {
        return Err(ApiError::Unauthorized(
            "Cannot delete admin users".to_string(),
        ));
    }

    // Revoke all active sessions across all namespaces immediately
    let session_store = db_core::sessions::SessionsDb::new((**pool).clone());
    if let Err(e) = session_store.delete_user_sessions(id, None).await {
        tracing::warn!(
            "Failed to revoke sessions for soft-deleted user {}: {:?}",
            id,
            e
        );
    }

    let deleted = db_core::user::soft_delete_user(pool.get_ref(), id)
        .await
        .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Item(map_user_to_response(deleted)),
        |_: Vec<UserResponse>| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    post,
    path = "/api/v1/users/user/{id}/restore",
    tag = "users",
    params(
        ("id" = Uuid, Path, description = "User ID"),
    ),
    responses(
        (status = 200, description = "User restored", body = UserResponse),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn restore_user(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    let id = path.into_inner();
    let restored = db_core::user::restore_user(pool.get_ref(), id)
        .await
        .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Item(map_user_to_response(restored)),
        |_: Vec<UserResponse>| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    delete,
    path = "/api/v1/users/user/{id}/hard",
    tag = "users",
    params(
        ("id" = Uuid, Path, description = "User ID"),
    ),
    responses(
        (status = 204, description = "User hard deleted"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn hard_delete_user(
    _req: HttpRequest,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    let id = path.into_inner();
    let target_user = db_core::user::get_user_by_id(pool.get_ref(), id)
        .await
        .map_err(ApiError::Database)?;

    if target_user.roles.contains(&UserRole::Admin) {
        return Err(ApiError::Unauthorized(
            "Cannot hard delete admin users".to_string(),
        ));
    }

    // Revoke all active sessions across all namespaces
    let session_store = db_core::sessions::SessionsDb::new((**pool).clone());
    if let Err(e) = session_store.delete_user_sessions(id, None).await {
        tracing::warn!(
            "Failed to revoke sessions for hard-deleted user {}: {:?}",
            id,
            e
        );
    }

    db_core::user::hard_delete_user(pool.get_ref(), id)
        .await
        .map_err(ApiError::Database)?;

    Ok(actix_web::HttpResponse::NoContent().finish())
}

// --- Profile Endpoints ---
