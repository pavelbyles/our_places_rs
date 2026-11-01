//! Reading and updating a user, and their listings and bookings.

use super::{UserResponse, field_error, map_user_to_response};
use actix_web::{HttpRequest, Responder, web};
use api_core::error::ApiError;
use api_core::models::{
    BookingResponse, BookingsWrapper, ListingsWrapper, map_booking_to_response,
    map_listing_to_response,
};
use api_core::pagination;
use api_core::response::{Payload, respond};
use common::models::{ListingResponse, UpdateUserRequest};
use db_core::booking as db_booking;
use db_core::listing as db_listing;
use db_core::models::{UpdatedUser, UserRole};
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;
use validator::Validate;

#[tracing::instrument(skip_all)]
#[utoipa::path(
    patch,
    path = "/api/v1/users/user/{id}",
    tag = "users",
    request_body = UpdateUserRequest,
    responses(
        (status = 201, description = "User updated", body = UserResponse),
        (status = 400, description = "Validation error"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn update_user(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    updated_user: web::Json<UpdateUserRequest>,
    path: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    let req_data = updated_user.into_inner();
    req_data.validate()?;
    let slug = req_data
        .slug
        .as_deref()
        .map(common::slug::user_slug)
        .transpose()
        .map_err(|msg| field_error("slug", msg))?;

    let id = path.into_inner();

    let target_user = db_core::user::get_user_by_id(pool.get_ref(), id)
        .await
        .map_err(ApiError::Database)?;
    reject_admin_lockout(&target_user.roles, &req_data)?;

    let password_hash = hash_new_password(req_data.password.as_deref()).await?;

    let updated = UpdatedUser {
        email: req_data.email.clone(),
        password_hash,
        first_name: req_data.first_name.clone(),
        last_name: req_data.last_name.clone(),
        phone_number: req_data.phone_number.clone(),
        is_active: req_data.is_active,
        is_verified: req_data.is_verified,
        verification_code: None,
        verification_code_expires_at: None,
        attributes: req_data.attributes.clone(),
        roles: req_data.roles.as_deref().map(parse_roles),
        default_currency: req_data.default_currency.clone(),
        slug,
    };

    let updated_user = db_core::user::update_user(pool.get_ref(), id, &updated)
        .await
        .map_err(map_update_error)?;

    create_requested_profiles(pool.get_ref(), id, &req_data).await;

    Ok(respond(
        &req,
        Payload::Item(map_user_to_response(updated_user)),
        |_: Vec<UserResponse>| (),
        actix_web::http::StatusCode::OK,
    ))
}

/// Role names that parse; unknown names are dropped.
fn parse_roles(roles: &[String]) -> Vec<UserRole> {
    roles
        .iter()
        .filter_map(|r| UserRole::from_str(r).ok())
        .collect()
}

/// An admin may not be demoted, deactivated or unverified through this
/// endpoint, so the system can never lose its last way in.
fn reject_admin_lockout(
    target_roles: &[UserRole],
    req_data: &UpdateUserRequest,
) -> Result<(), ApiError> {
    if !target_roles.contains(&UserRole::Admin) {
        return Ok(());
    }
    let is_demoting_admin = req_data
        .roles
        .as_deref()
        .is_some_and(|roles| !parse_roles(roles).contains(&UserRole::Admin));
    let is_deactivating = req_data.is_active == Some(false);
    let is_unverifying = req_data.is_verified == Some(false);

    if is_demoting_admin || is_deactivating || is_unverifying {
        return Err(ApiError::Forbidden(
            "Cannot modify roles or active status of the system admin".to_string(),
        ));
    }
    Ok(())
}

/// bcrypt hash of a new password, off the async workers. `None` when no
/// (or an empty) password was sent, leaving the current one in place.
async fn hash_new_password(password: Option<&str>) -> Result<Option<String>, ApiError> {
    let Some(pwd) = password.filter(|p| !p.is_empty()).map(str::to_owned) else {
        return Ok(None);
    };
    tokio::task::spawn_blocking(move || bcrypt::hash(&pwd, bcrypt::DEFAULT_COST))
        .await
        .map_err(|_| ApiError::Internal)?
        .map(Some)
        .map_err(|_| ApiError::Internal)
}

/// Creates the booker and host profiles sent with the update, for the roles
/// the request grants. A failure is logged and does not fail the update.
async fn create_requested_profiles(pool: &PgPool, id: Uuid, req_data: &UpdateUserRequest) {
    let Some(roles) = &req_data.roles else {
        return;
    };
    let has_role = |name: &str| roles.iter().any(|r| r.to_lowercase() == name);

    if has_role("booker")
        && let Some(profile) = &req_data.booker_profile
        && let Err(e) = db_core::user::create_booker_profile(pool, id, profile).await
    {
        tracing::warn!(user_id = %id, error = %e, "booker profile not created");
    }
    if has_role("host")
        && let Some(profile) = &req_data.host_profile
        && let Err(e) = db_core::user::create_host_profile(pool, id, profile).await
    {
        tracing::warn!(user_id = %id, error = %e, "host profile not created");
    }
}

/// A validation failure from the database becomes a 400; anything else is a
/// database error.
fn map_update_error(e: db_core::error::DbError) -> ApiError {
    match e {
        db_core::error::DbError::ValidationError(msg) => {
            let mut map = std::collections::HashMap::new();
            map.insert(
                std::borrow::Cow::from("validation"),
                validator::ValidationErrorsKind::Field(vec![
                    validator::ValidationError::new("custom").with_message(msg.into()),
                ]),
            );
            ApiError::ValidationError(validator::ValidationErrors(map))
        }
        other => ApiError::Database(other),
    }
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v1/users/user/{email}",
    tag = "users",
    params(
        ("email" = String, Path, description = "User email or UUID"),
    ),
    responses(
        (status = 200, description = "User found", body = UserResponse),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn get_user(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    email_or_id: web::Path<String>,
) -> Result<impl Responder, ApiError> {
    let val = email_or_id.into_inner();
    let user = match Uuid::parse_str(&val) {
        Ok(uuid) => db_core::user::get_user_by_id(pool.get_ref(), uuid).await,
        Err(_) => db_core::user::get_user_by_email(pool.get_ref(), &val).await,
    }
    .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Item(map_user_to_response(user)),
        |_: Vec<UserResponse>| (), // No XML wrapper needed for single item
        actix_web::http::StatusCode::OK,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/users/{email}/listings",
    tag = "listings",
    params(
        ("email" = String, Path, description = "User email"),
    ),
    responses(
        (status = 200, description = "User listings", body = [ListingResponse]),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn get_user_listings(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    email: web::Path<String>,
) -> Result<impl Responder, ApiError> {
    // 1. Get user by email to retrieve the ID
    let user = db_core::user::get_user_by_email(pool.get_ref(), &email)
        .await
        .map_err(ApiError::Database)?;

    // 2. Get listings by user ID
    let listings = db_listing::get_listings_by_user_id(pool.get_ref(), user.id)
        .await
        .map_err(ApiError::Database)?;

    let response: Vec<ListingResponse> =
        listings.into_iter().map(map_listing_to_response).collect();

    Ok(respond(
        &req,
        Payload::Collection(response),
        |items| ListingsWrapper { listing: items },
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v1/users/{email}/bookings",
    tag = "bookings",
    params(
        pagination::Pagination
    ),
    responses(
        (status = 200, description = "List of bookings for user", body = [BookingResponse]),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn get_user_bookings(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    query: web::Query<pagination::Pagination>,
) -> Result<impl Responder, ApiError> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(10).min(100);

    let bookings = db_booking::get_bookings_by_user_id(pool.get_ref(), *id, page, per_page)
        .await
        .map_err(ApiError::Database)?;

    let response: Vec<BookingResponse> = bookings
        .into_iter()
        .map(|b| {
            let mut resp = map_booking_to_response(b.booking);
            resp.review_eligibility = b.review_eligibility;
            resp
        })
        .collect();

    Ok(respond(
        &req,
        Payload::Collection(response),
        |items| BookingsWrapper { booking: items },
        actix_web::http::StatusCode::OK,
    ))
}
