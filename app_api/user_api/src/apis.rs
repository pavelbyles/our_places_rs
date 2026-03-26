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

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct UserFilter {
    pub search: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub phone_number: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub attributes: serde_json::Value,
    pub roles: Vec<UserRole>,
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
        created_at: user.created_at,
        updated_at: user.updated_at,
        attributes: user.attributes,
        roles: user.roles,
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

        let password_hash = bcrypt::hash(&req_data.password, bcrypt::DEFAULT_COST)
            .map_err(|_| ApiError::Internal)?;

        attempts += 1;
        let user = NewUser {
            id: Uuid::now_v7(),
            email: req_data.email.clone(),
            password_hash,
            first_name: req_data.first_name.clone(),
            last_name: req_data.last_name.clone(),
            phone_number: req_data.phone_number.clone(),
            is_active: req_data.is_active,
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
    let valid = bcrypt::verify(&credentials.password, &user.password_hash)
        .map_err(|_| ApiError::Unauthorized("Invalid credentials".to_string()))?;

    if !valid {
        return Err(ApiError::Unauthorized("Invalid credentials".to_string()));
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

    let password_hash = if let Some(ref password) = req_data.password {
        if !password.is_empty() {
            Some(bcrypt::hash(password, bcrypt::DEFAULT_COST).map_err(|_| ApiError::Internal)?)
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
            attributes: req_data.attributes.clone(),
            roles: req_data.roles.clone().map(|roles| {
                roles
                    .into_iter()
                    .filter_map(|r| UserRole::from_str(&r).ok())
                    .collect()
            }),
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
        ("email" = String, Path, description = "User email"),
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
    email: web::Path<String>,
) -> Result<impl Responder, ApiError> {
    let user = db_core::user::get_user_by_email(pool.get_ref(), &email)
        .await
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

    let response: Vec<BookingResponse> =
        bookings.into_iter().map(map_booking_to_response).collect();

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

    let users = db_core::user::get_all_users(pool.get_ref(), page, per_page, filter.search.clone())
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

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    #[derive(OpenApi)]
    #[openapi(
        paths(
            get_all_users,
            create_user,
            update_user,
            get_user,
            get_user_bookings,
            get_user_listings,
            api_core::health::health_check,
        ),
        components(
            schemas(NewUserRequest, UpdateUserRequest, UserResponse, ListingResponse, BookingResponse, pagination::Pagination, api_core::health::PingResponse, UserFilter, UsersWrapper)
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
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(get_all_users),
            )
            .route(
                "/",
                web::post()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(create_user),
            )
            .route(
                "/user/{email}",
                web::get()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(get_user),
            )
            .route(
                "/user/{id}",
                web::patch()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(update_user),
            )
            .route(
                "/user/{email}/bookings",
                web::get()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(get_user_bookings),
            )
            .route(
                "/user/{email}/listings",
                web::get()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(get_user_listings), // TODO: implement
            )
            .route(
                "/health_check",
                web::get().to(api_core::health::health_check),
            )
            .route(
                "/login",
                web::post()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(login),
            ),
    );
}

#[cfg(test)]
#[path = "apis_test.rs"]
mod tests;
