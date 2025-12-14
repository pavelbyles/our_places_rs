use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::http::header::{ACCEPT, CONTENT_TYPE};
use actix_web::middleware::{Next, from_fn};
use actix_web::{Error, HttpRequest, Responder, web};
use api_core::{
    error::ApiError,
    models::{
        BookingResponse, BookingsWrapper, ListingResponse, ListingsWrapper,
        map_booking_to_response, map_listing_to_response,
    },
    pagination,
    response::{Payload, respond},
    settings::Settings,
};
use chrono::{DateTime, Utc};
use db_core::booking as db_booking;
use db_core::listing as db_listing;
use db_core::models::{NewUser, UpdatedUser, User};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct NewUserRequest {
    pub email: String,
    pub password_hash: String,
    pub first_name: String,
    pub last_name: String,
    pub phone_number: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct UpdatedUserRequest {
    pub email: Option<String>,
    pub password_hash: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub phone_number: Option<String>,
    pub is_active: Option<bool>,
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
    }
}

#[tracing::instrument]
#[utoipa::path(
    post,
    path = "/api/v1/users/",
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
        attempts += 1;
        let user = NewUser {
            id: Uuid::new_v4(),
            email: req_data.email.clone(),
            password_hash: req_data.password_hash.clone(),
            first_name: req_data.first_name.clone(),
            last_name: req_data.last_name.clone(),
            phone_number: req_data.phone_number.clone(),
            is_active: req_data.is_active,
        };

        match db_core::user::create_user(pool.get_ref(), &user).await {
            Ok(created_user) => {
                return Ok(respond(
                    &req,
                    Payload::Item(map_user_to_response(created_user)),
                    |_: Vec<UserResponse>| (),
                    actix_web::http::StatusCode::CREATED,
                ));
            }
            Err(e) => {
                let db_core::error::DbError::Sqlx(ref sqlx_error) = e;
                if let Some(db_error) = sqlx_error.as_database_error()
                    && db_error.code().as_deref() == Some("23505")
                {
                    // 23505 is unique_violation
                    let constraint = db_error.constraint().unwrap_or("");
                    if constraint == "user_pkey" {
                        if attempts >= max_attempts {
                            return Err(ApiError::Internal);
                        }
                        continue; // Retry
                    } else if constraint == "user_email_key" || constraint == "idx_user_email" {
                        let mut map = std::collections::HashMap::new();
                        map.insert(
                            std::borrow::Cow::from("email"),
                            validator::ValidationErrorsKind::Field(vec![
                                validator::ValidationError::new("unique")
                                    .with_message("Email already taken".into()),
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
    patch,
    path = "/api/v1/users/user/{id}",
    tag = "users",
    request_body = UpdatedUserRequest,
    responses(
        (status = 201, description = "User updated", body = UserResponse),
        (status = 400, description = "Validation error"),
        (status = 500, description = "Internal server error")
    )
)]
async fn update_user(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    updated_user: web::Json<UpdatedUserRequest>,
    path: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    let req_data = updated_user.into_inner();
    req_data.validate()?;

    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 5;

    let id = path.into_inner();

    loop {
        attempts += 1;
        let updated = UpdatedUser {
            email: req_data.email.clone(),
            password_hash: req_data.password_hash.clone(),
            first_name: req_data.first_name.clone(),
            last_name: req_data.last_name.clone(),
            phone_number: req_data.phone_number.clone(),
            is_active: req_data.is_active,
        };

        match db_core::user::update_user(pool.get_ref(), id, &updated).await {
            Ok(updated_user) => {
                return Ok(respond(
                    &req,
                    Payload::Item(map_user_to_response(updated_user)),
                    |_: Vec<UserResponse>| (),
                    actix_web::http::StatusCode::OK,
                ));
            }
            Err(e) => {
                {
                    let db_core::error::DbError::Sqlx(ref sqlx_error) = e;
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

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    #[derive(OpenApi)]
    #[openapi(
        paths(
            create_user,
            update_user,
            get_user,
            get_user_bookings,
            get_user_listings,
        ),
        components(
            schemas(NewUserRequest, UpdatedUserRequest, UserResponse, ListingResponse, BookingResponse, pagination::Pagination)
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
            ),
    );
}

/// Content-Type - Requests
/// Accept - Responses
/// Middleware to check Content-Type and Accept headers
/// Returns 415 Unsupported Media Type or 406 Not Acceptable if invalid
async fn content_negotiation_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let headers = req.headers();

    // Check Content-Type (if present) -> 415 Unsupported Media Type
    if let Some(ct_str) = headers.get(CONTENT_TYPE).and_then(|ct| ct.to_str().ok()) {
        let mime = ct_str.split(';').next().unwrap_or("").trim().to_lowercase();
        let supported_formats = [
            "application/json",
            "application/xml",
            "application/x-www-form-urlencoded",
        ];

        if !supported_formats.contains(&mime.as_str()) {
            return Err(actix_web::error::ErrorUnsupportedMediaType(
                "Unsupported Content-Type",
            ));
        }
    }

    // Check Accept header (if present) -> 406 Not Acceptable
    if let Some(accept_str) = headers.get(ACCEPT).and_then(|a| a.to_str().ok()) {
        let supported_responses = ["application/json", "application/xml"];

        let accepts_supported = accept_str.split(',').any(|s| {
            let mime = s.split(';').next().unwrap_or("").trim().to_lowercase();
            mime == "*/*" || supported_responses.contains(&mime.as_str())
        });

        if !accepts_supported {
            return Err(actix_web::error::ErrorNotAcceptable(
                "The requested response format is not supported",
            ));
        }
    }

    // If checks pass, call the next service in the chain
    next.call(req).await
}
