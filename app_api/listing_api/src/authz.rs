//! Who may change a listing (spec §1.8).
//!
//! Every write takes [`Claims`] first, so a missing or invalid JWT is a 401
//! before any lookup. Then the caller must be an active user (401), the
//! listing must exist and not be deleted (404), and the caller must own it
//! or be an admin (403, logged at `warn`).

use api_core::auth::Claims;
use api_core::error::ApiError;
use db_core::error::DbError;
use db_core::models::{User, UserRole};
use sqlx::PgPool;
use uuid::Uuid;

/// The listing a manager may change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListingOwner {
    /// The listing.
    pub listing_id: Uuid,
    /// Its owner.
    pub owner_id: Uuid,
}

/// Whether `user` is an admin.
pub fn is_admin(user: &User) -> bool {
    user.roles.contains(&UserRole::Admin)
}

/// The active user behind `claims`; unknown, deactivated or deleted is 401.
pub async fn caller(pool: &PgPool, claims: &Claims) -> Result<User, ApiError> {
    let user = db_core::user::get_user_by_id(pool, claims.sub)
        .await
        .map_err(|e| match e {
            DbError::Sqlx(sqlx::Error::RowNotFound) => {
                tracing::warn!(user_id = %claims.sub, "token is for a user that does not exist");
                ApiError::Unauthorized("Unknown user".to_string())
            }
            e => {
                tracing::error!(user_id = %claims.sub, error = ?e, "failed to load the calling user");
                ApiError::Database(e)
            }
        })?;
    match user.is_active && user.deleted_at.is_none() {
        true => Ok(user),
        false => {
            tracing::warn!(user_id = %claims.sub, "token is for a deactivated or deleted account");
            Err(ApiError::Unauthorized("Account is not active".to_string()))
        }
    }
}

/// The caller, who must be allowed to manage `listing_id`: its owner or an
/// admin. `action` names the attempt in the 403 log line.
pub async fn require_listing_manager(
    pool: &PgPool,
    claims: &Claims,
    listing_id: Uuid,
    action: &str,
) -> Result<(User, ListingOwner), ApiError> {
    let user = caller(pool, claims).await?;
    let owner_id = sqlx::query_scalar!(
        "SELECT user_id FROM listing WHERE id = $1 AND deleted_at IS NULL",
        listing_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| ApiError::Database(e.into()))?
    .ok_or(ApiError::Database(DbError::Sqlx(sqlx::Error::RowNotFound)))?;
    if is_admin(&user) || owner_id == user.id {
        return Ok((
            user,
            ListingOwner {
                listing_id,
                owner_id,
            },
        ));
    }
    tracing::warn!(user_id = %user.id, %listing_id, action, "listing change refused: not the owner");
    Err(ApiError::Forbidden("FORBIDDEN".to_string()))
}

/// The caller, who must be a host or an admin to create listings.
pub async fn require_host_or_admin(
    pool: &PgPool,
    claims: &Claims,
    action: &str,
) -> Result<User, ApiError> {
    let user = caller(pool, claims).await?;
    match is_admin(&user) || user.roles.contains(&UserRole::Host) {
        true => Ok(user),
        false => {
            tracing::warn!(user_id = %user.id, action, "refused: not a host or admin");
            Err(ApiError::Forbidden("FORBIDDEN".to_string()))
        }
    }
}

/// The owner a new listing gets. A host always owns what they create; an
/// admin may name any active host or admin, otherwise 422.
pub async fn listing_owner_for(
    pool: &PgPool,
    creator: &User,
    requested: Uuid,
) -> Result<Uuid, ApiError> {
    if !is_admin(creator) {
        return Ok(creator.id);
    }
    let owner = db_core::user::get_user_by_id(pool, requested)
        .await
        .map_err(|e| match e {
            DbError::Sqlx(sqlx::Error::RowNotFound) => ApiError::Unprocessable {
                code: "VALIDATION_ERROR",
                message: "The listing owner does not exist.".to_string(),
            },
            e => ApiError::Database(e),
        })?;
    let eligible = owner.is_active
        && owner.deleted_at.is_none()
        && (owner.roles.contains(&UserRole::Host) || owner.roles.contains(&UserRole::Admin));
    match eligible {
        true => Ok(owner.id),
        false => Err(ApiError::Unprocessable {
            code: "VALIDATION_ERROR",
            message: "The listing owner must be an active host or admin.".to_string(),
        }),
    }
}
