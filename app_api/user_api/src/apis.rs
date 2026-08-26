use actix_web::middleware::from_fn;
use actix_web::{HttpRequest, Responder, web};
use api_core::api_common::content_negotiation_middleware;
use api_core::{
    error::ApiError,
    models::{
        BookingResponse, BookingsWrapper, ListingsWrapper, map_booking_to_response,
        map_listing_to_response,
    },
    pagination,
    response::{Payload, respond},
    settings::Settings,
};
use chrono::{DateTime, Utc};
use common::models::ListingResponse;
use db_core::booking as db_booking;
use db_core::listing as db_listing;
use db_core::models::{NewUser, UpdatedUser, User, UserRole};
use rand::distr::{Alphanumeric, SampleString};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::str::FromStr;
use utoipa::{IntoParams, OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;
use validator::Validate;

pub use common::models::NewUserRequest;
pub use common::models::UpdateUserRequest;

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct VerifyRequest {
    pub email: String,
    pub code: String,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct ResendVerificationRequest {
    #[validate(email)]
    pub email: String,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct UserFilter {
    pub search: Option<String>,
    pub is_deleted: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct PasswordChangeRequest {
    #[validate(email)]
    pub email: String,
    pub current_password: String,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct PasswordChangeConfirm {
    #[validate(email)]
    pub email: String,
    pub code: String,
    pub new_password: String,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct EmailChangeRequest {
    #[validate(email)]
    pub email: String,
    pub current_password: String,
    #[validate(email)]
    pub new_email: String,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct DeactivateRequest {
    #[validate(email)]
    pub email: String,
    pub current_password: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub phone_number: Option<String>,
    pub is_active: bool,
    pub is_verified: bool,
    pub verification_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub attributes: serde_json::Value,
    pub roles: Vec<String>,
    pub default_currency: String,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, ToSchema)]
pub struct UsersWrapper {
    #[schema(xml(name = "user", wrapped))]
    pub user: Vec<UserResponse>,
}

fn map_user_to_response(user: User) -> UserResponse {
    UserResponse {
        id: user.id,
        email: user.email,
        first_name: user.first_name,
        last_name: user.last_name,
        phone_number: user.phone_number,
        is_active: user.is_active,
        is_verified: user.is_verified,
        verification_code: user.verification_code,
        created_at: user.created_at,
        updated_at: user.updated_at,
        attributes: user.attributes,
        roles: user.roles.into_iter().map(|r| r.to_string()).collect(),
        default_currency: user.default_currency,
        deleted_at: user.deleted_at,
    }
}

#[tracing::instrument]
#[utoipa::path(
    post,
    path = "/api/v1/users",
    tag = "users",
    request_body = NewUserRequest,
    responses(
        (status = 201, description = "User Created", body = UserResponse),
        (status = 400, description = "Validation error"),
        (status = 500, description = "Internal server error")
    )
)]
async fn create_user(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    new_user: web::Json<NewUserRequest>,
    settings: web::Data<Settings>,
) -> Result<impl Responder, ApiError> {
    let req_data = new_user.into_inner();
    req_data.validate()?;

    let mut attempts = 0;
    let max_attempts = settings.application.max_attempts;

    loop {
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| ApiError::Database(db_core::error::DbError::Sqlx(e)))?;

        let password = req_data.password.clone();
        let password_hash =
            tokio::task::spawn_blocking(move || bcrypt::hash(&password, bcrypt::DEFAULT_COST))
                .await
                .map_err(|_| ApiError::Internal)?
                .map_err(|_| ApiError::Internal)?;

        let otp: String = Alphanumeric
            .sample_string(&mut rand::rng(), 6)
            .to_uppercase();

        // Let's make it easy to see in logs
        tracing::info!("VERIFICATION CODE FOR {}: {}", req_data.email, otp);

        attempts += 1;
        let user = NewUser {
            id: Uuid::now_v7(),
            email: req_data.email.clone(),
            password_hash,
            first_name: req_data.first_name.clone(),
            last_name: req_data.last_name.clone(),
            phone_number: req_data.phone_number.clone(),
            is_active: req_data.is_active,
            is_verified: false,
            verification_code: Some(otp),
            verification_code_expires_at: Some(Utc::now() + chrono::Duration::minutes(30)),
            attributes: req_data
                .attributes
                .clone()
                .unwrap_or_else(|| serde_json::json!({})),
            roles: req_data.roles.clone().map(|roles| {
                roles
                    .into_iter()
                    .filter_map(|r| UserRole::from_str(&r).ok())
                    .collect()
            }),
            default_currency: req_data
                .default_currency
                .clone()
                .unwrap_or_else(|| "USD".to_string()),
        };

        match db_core::user::create_user(&mut *tx, &user).await {
            Ok(created_user) => {
                let roles_strings = req_data.roles.clone().unwrap_or_default();
                let is_booker = roles_strings.iter().any(|r| r.to_lowercase() == "booker");
                let is_host = roles_strings.iter().any(|r| r.to_lowercase() == "host");

                if is_booker {
                    match &req_data.booker_profile {
                        Some(profile) => {
                            db_core::user::create_booker_profile(
                                &mut *tx,
                                created_user.id,
                                profile,
                            )
                            .await
                            .map_err(ApiError::Database)?;
                        }
                        None => {
                            let mut map = std::collections::HashMap::new();
                            map.insert(
                                std::borrow::Cow::from("booker_profile"),
                                validator::ValidationErrorsKind::Field(vec![
                                    validator::ValidationError::new("required").with_message(
                                        "Booker profile is required for booker role".into(),
                                    ),
                                ]),
                            );
                            return Err(ApiError::ValidationError(validator::ValidationErrors(
                                map,
                            )));
                        }
                    }
                }

                if is_host {
                    match &req_data.host_profile {
                        Some(profile) => {
                            db_core::user::create_host_profile(&mut *tx, created_user.id, profile)
                                .await
                                .map_err(ApiError::Database)?;
                        }
                        None => {
                            let mut map = std::collections::HashMap::new();
                            map.insert(
                                std::borrow::Cow::from("host_profile"),
                                validator::ValidationErrorsKind::Field(vec![
                                    validator::ValidationError::new("required").with_message(
                                        "Host profile is required for host role".into(),
                                    ),
                                ]),
                            );
                            return Err(ApiError::ValidationError(validator::ValidationErrors(
                                map,
                            )));
                        }
                    }
                }

                tx.commit()
                    .await
                    .map_err(|e| ApiError::Database(db_core::error::DbError::Sqlx(e)))?;

                return Ok(respond(
                    &req,
                    Payload::Item(map_user_to_response(created_user)),
                    |_: Vec<UserResponse>| (),
                    actix_web::http::StatusCode::CREATED,
                ));
            }
            Err(e) => {
                match e {
                    db_core::error::DbError::Sqlx(ref sqlx_error) => {
                        if let Some(db_error) = sqlx_error.as_database_error()
                            && db_error.code().as_deref() == Some("23505")
                        {
                            let constraint = db_error.constraint().unwrap_or("");
                            if constraint == "user_pkey" {
                                if attempts >= max_attempts {
                                    return Err(ApiError::Internal);
                                }
                                continue; // Retry
                            } else if constraint == "user_email_key"
                                || constraint == "idx_user_email"
                            {
                                let mut map = std::collections::HashMap::new();
                                map.insert(
                                    std::borrow::Cow::from("email"),
                                    validator::ValidationErrorsKind::Field(vec![
                                        validator::ValidationError::new("unique")
                                            .with_message("Email already taken".into()),
                                    ]),
                                );
                                return Err(ApiError::ValidationError(
                                    validator::ValidationErrors(map),
                                ));
                            }
                        }
                    }
                    db_core::error::DbError::ValidationError(msg) => {
                        let mut map = std::collections::HashMap::new();
                        map.insert(
                            std::borrow::Cow::from("validation"),
                            validator::ValidationErrorsKind::Field(vec![
                                validator::ValidationError::new("custom").with_message(msg.into()),
                            ]),
                        );
                        return Err(ApiError::ValidationError(validator::ValidationErrors(map)));
                    }
                }
                return Err(ApiError::Database(e));
            }
        }
    }
}

#[tracing::instrument]
#[utoipa::path(
    post,
    path = "/api/v1/users/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = UserResponse),
        (status = 401, description = "Invalid credentials"),
        (status = 500, description = "Internal server error")
    )
)]
async fn login(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    login_req: web::Json<LoginRequest>,
) -> Result<impl Responder, ApiError> {
    let credentials = login_req.into_inner();
    credentials.validate()?;

    // Fetch user
    let user = db_core::user::get_user_by_email(pool.get_ref(), &credentials.email)
        .await
        .map_err(|_| ApiError::Unauthorized("Invalid credentials".to_string()))?;

    // Verify password
    let password = credentials.password.clone();
    let hash = user.password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || bcrypt::verify(&password, &hash))
        .await
        .map_err(|_| ApiError::Internal)?
        .map_err(|_| ApiError::Unauthorized("Invalid credentials".to_string()))?;

    if !valid {
        return Err(ApiError::Unauthorized("Invalid credentials".to_string()));
    }

    if !user.is_active {
        return Err(ApiError::Unauthorized(
            "Account has been deactivated".to_string(),
        ));
    }

    if user.deleted_at.is_some() {
        return Err(ApiError::Unauthorized(
            "Account has been deleted".to_string(),
        ));
    }

    if !user.is_verified {
        return Err(ApiError::Unauthorized("Account not verified".to_string()));
    }

    // Return user info
    Ok(respond(
        &req,
        Payload::Item(map_user_to_response(user)),
        |_: Vec<UserResponse>| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument]
#[utoipa::path(
    post,
    path = "/api/v1/users/verify",
    tag = "auth",
    request_body = VerifyRequest,
    responses(
        (status = 200, description = "Verification successful", body = UserResponse),
        (status = 400, description = "Invalid code or expired"),
        (status = 500, description = "Internal server error")
    )
)]
async fn verify_user(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    verify_req: web::Json<VerifyRequest>,
) -> Result<impl Responder, ApiError> {
    let credentials = verify_req.into_inner();

    // Fetch user
    let user = db_core::user::get_user_by_email(pool.get_ref(), &credentials.email)
        .await
        .map_err(|_| ApiError::Unauthorized("User not found".to_string()))?;

    let trimmed_code = credentials.code.trim();

    if let Some(code) = &user.verification_code {
        if !code.trim().eq_ignore_ascii_case(trimmed_code) {
            return Err(ApiError::Unauthorized(
                "Invalid verification code".to_string(),
            ));
        }

        if let Some(expiry) = user.verification_code_expires_at
            && expiry <= Utc::now()
        {
            return Err(ApiError::Unauthorized(
                "Verification code has expired".to_string(),
            ));
        }

        // Success!
        let updated = db_core::user::complete_user_verification(pool.get_ref(), user.id)
            .await
            .map_err(ApiError::Database)?;

        return Ok(respond(
            &req,
            Payload::Item(map_user_to_response(updated)),
            |_: Vec<UserResponse>| (),
            actix_web::http::StatusCode::OK,
        ));
    }

    Err(ApiError::Unauthorized(
        "No verification code found for user".to_string(),
    ))
}

#[tracing::instrument]
#[utoipa::path(
    post,
    path = "/api/v1/users/resend-verification",
    tag = "auth",
    request_body = ResendVerificationRequest,
    responses(
        (status = 200, description = "Resend verification successful", body = UserResponse),
        (status = 401, description = "User not found or already verified"),
        (status = 500, description = "Internal server error")
    )
)]
async fn resend_verification(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    resend_req: web::Json<ResendVerificationRequest>,
) -> Result<impl Responder, ApiError> {
    let payload = resend_req.into_inner();
    payload.validate()?;

    let user = db_core::user::get_user_by_email(pool.get_ref(), &payload.email)
        .await
        .map_err(|_| ApiError::Unauthorized("User not found or already verified".to_string()))?;

    if user.is_verified {
        return Err(ApiError::Unauthorized(
            "User not found or already verified".to_string(),
        ));
    }

    let otp: String = Alphanumeric
        .sample_string(&mut rand::rng(), 6)
        .to_uppercase();

    tracing::info!("RESEND VERIFICATION CODE FOR {}: {}", payload.email, otp);

    let expiry = chrono::Utc::now() + chrono::Duration::minutes(30);

    let updated =
        db_core::user::regenerate_verification_code(pool.get_ref(), &payload.email, &otp, expiry)
            .await
            .map_err(ApiError::Database)?;

    if let Some(user) = updated {
        return Ok(respond(
            &req,
            Payload::Item(map_user_to_response(user)),
            |_: Vec<UserResponse>| (),
            actix_web::http::StatusCode::OK,
        ));
    }

    Err(ApiError::Unauthorized(
        "User not found or already verified".to_string(),
    ))
}

#[tracing::instrument]
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
async fn update_user(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    updated_user: web::Json<UpdateUserRequest>,
    path: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    let req_data = updated_user.into_inner();
    req_data.validate()?;

    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 5;

    let id = path.into_inner();

    // --- Admin Guard Middleware Logic ---
    // Fetch the target user to check their roles. If they are an admin, deny the update.
    let target_user = db_core::user::get_user_by_id(pool.get_ref(), id)
        .await
        .map_err(ApiError::Database)?;

    if target_user.roles.contains(&UserRole::Admin) {
        let is_modifying_roles = req_data.roles.is_some();
        let is_modifying_active = req_data.is_active.is_some();
        let is_modifying_verified = req_data.is_verified.is_some();

        if is_modifying_roles || is_modifying_active || is_modifying_verified {
            return Err(ApiError::Forbidden(
                "Cannot modify roles or active status of the system admin".to_string(),
            ));
        }
    }
    // ------------------------------------

    let password_hash = if let Some(ref password) = req_data.password {
        if !password.is_empty() {
            let pwd = password.clone();
            Some(
                tokio::task::spawn_blocking(move || bcrypt::hash(&pwd, bcrypt::DEFAULT_COST))
                    .await
                    .map_err(|_| ApiError::Internal)?
                    .map_err(|_| ApiError::Internal)?,
            )
        } else {
            None
        }
    } else {
        None
    };

    loop {
        attempts += 1;
        let updated = UpdatedUser {
            email: req_data.email.clone(),
            password_hash: password_hash.clone(),
            first_name: req_data.first_name.clone(),
            last_name: req_data.last_name.clone(),
            phone_number: req_data.phone_number.clone(),
            is_active: req_data.is_active,
            is_verified: req_data.is_verified,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: req_data.attributes.clone(),
            roles: req_data.roles.clone().map(|roles| {
                roles
                    .into_iter()
                    .filter_map(|r| UserRole::from_str(&r).ok())
                    .collect()
            }),
            default_currency: req_data.default_currency.clone(),
        };

        match db_core::user::update_user(pool.get_ref(), id, &updated).await {
            Ok(updated_user) => {
                // Check for profile creation if roles/profiles are provided
                if let Some(roles_vec) = &req_data.roles {
                    let is_booker = roles_vec.iter().any(|r| r.to_lowercase() == "booker");
                    let is_host = roles_vec.iter().any(|r| r.to_lowercase() == "host");

                    if is_booker && let Some(profile) = &req_data.booker_profile {
                        let _ =
                            db_core::user::create_booker_profile(pool.get_ref(), id, profile).await;
                    }

                    if is_host && let Some(profile) = &req_data.host_profile {
                        let _ =
                            db_core::user::create_host_profile(pool.get_ref(), id, profile).await;
                    }
                }

                return Ok(respond(
                    &req,
                    Payload::Item(map_user_to_response(updated_user)),
                    |_: Vec<UserResponse>| (),
                    actix_web::http::StatusCode::OK,
                ));
            }
            Err(e) => {
                match e {
                    db_core::error::DbError::Sqlx(ref sqlx_error) => {
                        if let Some(db_error) = sqlx_error.as_database_error()
                            && db_error.code().as_deref() == Some("23505")
                            && let Some(constraint) = db_error.constraint()
                            && constraint == "user_pkey"
                        {
                            if attempts >= MAX_ATTEMPTS {
                                return Err(ApiError::Internal);
                            }
                            continue;
                        }
                    }
                    db_core::error::DbError::ValidationError(msg) => {
                        let mut map = std::collections::HashMap::new();
                        map.insert(
                            std::borrow::Cow::from("validation"),
                            validator::ValidationErrorsKind::Field(vec![
                                validator::ValidationError::new("custom").with_message(msg.into()),
                            ]),
                        );
                        return Err(ApiError::ValidationError(validator::ValidationErrors(map)));
                    }
                }
                return Err(ApiError::Database(e));
            }
        }
    }
}

#[tracing::instrument]
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
async fn get_user(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    email_or_id: web::Path<String>,
) -> Result<impl Responder, ApiError> {
    let val = email_or_id.into_inner();
    let user = if let Ok(uuid) = Uuid::parse_str(&val) {
        db_core::user::get_user_by_id(pool.get_ref(), uuid)
            .await
            .map_err(ApiError::Database)?
    } else {
        db_core::user::get_user_by_email(pool.get_ref(), &val)
            .await
            .map_err(ApiError::Database)?
    };

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
async fn get_user_listings(
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

#[tracing::instrument]
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
async fn get_user_bookings(
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

#[tracing::instrument]
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
async fn get_all_users(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    query: web::Query<pagination::Pagination>,
    filter: web::Query<UserFilter>,
) -> Result<impl Responder, ApiError> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(10).min(100);

    let users = db_core::user::get_all_users(
        pool.get_ref(),
        page,
        per_page,
        filter.search.clone(),
        filter.is_deleted,
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

#[tracing::instrument]
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
async fn soft_delete_user(
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

#[tracing::instrument]
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
async fn restore_user(
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

#[tracing::instrument]
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
async fn hard_delete_user(
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
            "Cannot hard delete admin users".to_string(),
        ));
    }

    db_core::user::hard_delete_user(pool.get_ref(), id)
        .await
        .map_err(ApiError::Database)?;

    Ok(actix_web::HttpResponse::NoContent().finish())
}

// --- Profile Endpoints ---

#[tracing::instrument]
#[utoipa::path(
    post,
    path = "/api/v1/users/profile/password/request",
    tag = "users",
    request_body = PasswordChangeRequest,
    responses(
        (status = 200, description = "Password change requested", body = UserResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Invalid credentials"),
        (status = 500, description = "Internal server error")
    )
)]
async fn request_password_change(
    req: actix_web::HttpRequest,
    pool: web::Data<PgPool>,
    req_data: web::Json<PasswordChangeRequest>,
) -> Result<impl Responder, ApiError> {
    let payload = req_data.into_inner();
    payload.validate()?;

    let user = db_core::user::get_user_by_email(pool.get_ref(), &payload.email)
        .await
        .map_err(|_| ApiError::Unauthorized("Invalid credentials".to_string()))?;

    let current_password = payload.current_password.clone();
    let hash = user.password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || bcrypt::verify(&current_password, &hash))
        .await
        .map_err(|_| ApiError::Internal)?
        .map_err(|_| ApiError::Unauthorized("Invalid credentials".to_string()))?;

    if !valid {
        return Err(ApiError::Unauthorized("Invalid credentials".to_string()));
    }

    let otp: String = Alphanumeric
        .sample_string(&mut rand::rng(), 6)
        .to_uppercase();

    tracing::info!("PASSWORD CHANGE CODE FOR {}: {}", payload.email, otp);

    let expiry = chrono::Utc::now() + chrono::Duration::minutes(30);

    // Reuse regenerate_verification_code, it just sets the code and expiry
    let updated =
        db_core::user::regenerate_verification_code(pool.get_ref(), &payload.email, &otp, expiry)
            .await
            .map_err(ApiError::Database)?;

    if let Some(user) = updated {
        return Ok(respond(
            &req,
            Payload::Item(map_user_to_response(user)),
            |_: Vec<UserResponse>| (),
            actix_web::http::StatusCode::OK,
        ));
    }

    Err(ApiError::Internal)
}

#[tracing::instrument]
#[utoipa::path(
    post,
    path = "/api/v1/users/profile/password/confirm",
    tag = "users",
    request_body = PasswordChangeConfirm,
    responses(
        (status = 200, description = "Password updated", body = UserResponse),
        (status = 400, description = "Validation error / Invalid code"),
        (status = 401, description = "Invalid credentials / Expired code"),
        (status = 500, description = "Internal server error")
    )
)]
async fn confirm_password_change(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    req_data: web::Json<PasswordChangeConfirm>,
) -> Result<impl Responder, ApiError> {
    let payload = req_data.into_inner();
    payload.validate()?;

    let user = db_core::user::get_user_by_email(pool.get_ref(), &payload.email)
        .await
        .map_err(|_| ApiError::Unauthorized("Invalid user".to_string()))?;

    if let Some(code) = &user.verification_code
        && code == &payload.code
        && let Some(expiry) = user.verification_code_expires_at
        && expiry > Utc::now()
    {
        let new_password = payload.new_password.clone();
        let new_hash =
            tokio::task::spawn_blocking(move || bcrypt::hash(&new_password, bcrypt::DEFAULT_COST))
                .await
                .map_err(|_| ApiError::Internal)?
                .map_err(|_| ApiError::Internal)?;

        let updated = db_core::user::update_user_password(pool.get_ref(), user.id, new_hash)
            .await
            .map_err(ApiError::Database)?;

        return Ok(respond(
            &req,
            Payload::Item(map_user_to_response(updated)),
            |_: Vec<UserResponse>| (),
            actix_web::http::StatusCode::OK,
        ));
    }

    Err(ApiError::Unauthorized(
        "Invalid or expired verification code".to_string(),
    ))
}

#[tracing::instrument]
#[utoipa::path(
    post,
    path = "/api/v1/users/profile/email",
    tag = "users",
    request_body = EmailChangeRequest,
    responses(
        (status = 200, description = "Email updated", body = UserResponse),
        (status = 400, description = "Validation error / Email taken"),
        (status = 401, description = "Invalid credentials"),
        (status = 500, description = "Internal server error")
    )
)]
async fn change_email(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    req_data: web::Json<EmailChangeRequest>,
) -> Result<impl Responder, ApiError> {
    let payload = req_data.into_inner();
    payload.validate()?;

    let user = db_core::user::get_user_by_email(pool.get_ref(), &payload.email)
        .await
        .map_err(|_| ApiError::Unauthorized("Invalid credentials".to_string()))?;

    let current_password = payload.current_password.clone();
    let hash = user.password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || bcrypt::verify(&current_password, &hash))
        .await
        .map_err(|_| ApiError::Internal)?
        .map_err(|_| ApiError::Unauthorized("Invalid credentials".to_string()))?;

    if !valid {
        return Err(ApiError::Unauthorized("Invalid credentials".to_string()));
    }

    let otp: String = Alphanumeric
        .sample_string(&mut rand::rng(), 6)
        .to_uppercase();

    tracing::info!(
        "VERIFICATION CODE FOR NEW EMAIL {}: {}",
        payload.new_email,
        otp
    );

    let expiry = chrono::Utc::now() + chrono::Duration::minutes(30);

    match db_core::user::update_user_email(pool.get_ref(), user.id, payload.new_email, otp, expiry)
        .await
    {
        Ok(updated) => Ok(respond(
            &req,
            Payload::Item(map_user_to_response(updated)),
            |_: Vec<UserResponse>| (),
            actix_web::http::StatusCode::OK,
        )),
        Err(e) => {
            if let db_core::error::DbError::Sqlx(ref sqlx_error) = e
                && let Some(db_error) = sqlx_error.as_database_error()
                && db_error.code().as_deref() == Some("23505")
            {
                let mut map = std::collections::HashMap::new();
                map.insert(
                    std::borrow::Cow::from("new_email"),
                    validator::ValidationErrorsKind::Field(vec![
                        validator::ValidationError::new("unique")
                            .with_message("Email already taken".into()),
                    ]),
                );
                return Err(ApiError::ValidationError(validator::ValidationErrors(map)));
            }
            Err(ApiError::Database(e))
        }
    }
}

#[tracing::instrument]
#[utoipa::path(
    post,
    path = "/api/v1/users/profile/deactivate",
    tag = "users",
    request_body = DeactivateRequest,
    responses(
        (status = 200, description = "Account deactivated", body = UserResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Invalid credentials"),
        (status = 500, description = "Internal server error")
    )
)]
async fn deactivate_account(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    req_data: web::Json<DeactivateRequest>,
) -> Result<impl Responder, ApiError> {
    let payload = req_data.into_inner();
    payload.validate()?;

    let user = db_core::user::get_user_by_email(pool.get_ref(), &payload.email)
        .await
        .map_err(|_| ApiError::Unauthorized("Invalid credentials".to_string()))?;

    let current_password = payload.current_password.clone();
    let hash = user.password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || bcrypt::verify(&current_password, &hash))
        .await
        .map_err(|_| ApiError::Internal)?
        .map_err(|_| ApiError::Unauthorized("Invalid credentials".to_string()))?;

    if !valid {
        return Err(ApiError::Unauthorized("Invalid credentials".to_string()));
    }

    if user.roles.contains(&UserRole::Admin) {
        return Err(ApiError::Forbidden(
            "Cannot deactivate system admin".to_string(),
        ));
    }

    let updated = db_core::user::deactivate_user(pool.get_ref(), user.id)
        .await
        .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Item(map_user_to_response(updated)),
        |_: Vec<UserResponse>| (),
        actix_web::http::StatusCode::OK,
    ))
}

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    #[derive(OpenApi)]
    #[openapi(
        paths(
            get_all_users,
            create_user,
            update_user,
            resend_verification,
            verify_user,
            get_user,
            get_user_bookings,
            get_user_listings,
            request_password_change,
            confirm_password_change,
            change_email,
            deactivate_account,
            api_core::health::health_check,
        ),
        components(
            schemas(NewUserRequest, UpdateUserRequest, VerifyRequest, ResendVerificationRequest, UserResponse, ListingResponse, BookingResponse, pagination::Pagination, api_core::health::PingResponse, UserFilter, UsersWrapper, PasswordChangeRequest, PasswordChangeConfirm, EmailChangeRequest, DeactivateRequest)
        ),
        tags(
            (name = "users", description = "User management endpoints")
        ),
    )]
    struct ApiDoc;

    // Register Swagger UI services at the ROOT scope so paths match
    cfg.service(
        SwaggerUi::new("/api/docs/swagger-ui/{_:.*}")
            .url("/api-docs/openapi.json", ApiDoc::openapi()),
    );

    cfg.service(
        web::scope("/api/v1/users")
            .route(
                "/",
                web::get()
                    .to(get_all_users)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/",
                web::post()
                    .to(create_user)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{email}",
                web::get()
                    .to(get_user)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{id}",
                web::patch()
                    .to(update_user)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{id}",
                web::delete()
                    .to(soft_delete_user)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{id}/restore",
                web::post()
                    .to(restore_user)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{id}/hard",
                web::delete()
                    .to(hard_delete_user)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{email}/bookings",
                web::get()
                    .to(get_user_bookings)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{email}/listings",
                web::get()
                    .to(get_user_listings) // TODO: implement
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/health_check",
                web::get().to(api_core::health::health_check),
            )
            .route(
                "/login",
                web::post()
                    .to(login)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/verify",
                web::post()
                    .to(verify_user)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/resend-verification",
                web::post()
                    .to(resend_verification)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/profile/password/request",
                web::post()
                    .to(request_password_change)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/profile/password/confirm",
                web::post()
                    .to(confirm_password_change)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/profile/email",
                web::post()
                    .to(change_email)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/profile/deactivate",
                web::post()
                    .to(deactivate_account)
                    .wrap(from_fn(content_negotiation_middleware)),
            ),
    );
}

#[cfg(test)]
#[path = "apis_test.rs"]
mod tests;
