//! user_api HTTP handlers, request/response types and route wiring.

use actix_web::middleware::from_fn;
use actix_web::web;
use api_core::api_common::content_negotiation_middleware;
use api_core::{error::ApiError, models::BookingResponse, pagination};
use chrono::{DateTime, Utc};
use common::models::ListingResponse;
use db_core::models::User;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;
use validator::Validate;

pub use common::models::{
    CreateSessionRequest, NewUserRequest, ResendVerificationRequest, SessionResponse,
    UpdateUserRequest, VerifyRequest,
};

mod account;
mod admin;
mod profile;
mod registration;
mod sessions;

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct SessionNamespaceQuery {
    pub namespace: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct UserFilter {
    pub search: Option<String>,
    pub is_deleted: Option<bool>,
    pub all: Option<bool>,
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
    /// Unique public handle, e.g. `jane-brown`.
    pub slug: String,
}

#[derive(Serialize, ToSchema)]
pub struct UsersWrapper {
    #[schema(xml(name = "user", wrapped))]
    pub user: Vec<UserResponse>,
}

/// A 400 with one validation message for `field`.
fn field_error(field: &'static str, message: String) -> ApiError {
    let mut errors = validator::ValidationErrors::new();
    errors.add(
        field,
        validator::ValidationError::new("invalid").with_message(message.into()),
    );
    ApiError::ValidationError(errors)
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
        slug: user.slug,
    }
}

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    #[derive(OpenApi)]
    #[openapi(
        paths(
            admin::get_all_users,
            registration::create_user,
            profile::update_user,
            registration::resend_verification,
            registration::verify_user,
            profile::get_user,
            profile::get_user_bookings,
            profile::get_user_listings,
            account::request_password_change,
            account::confirm_password_change,
            account::change_email,
            account::deactivate_account,
            sessions::create_session_handler,
            sessions::get_session_handler,
            sessions::delete_session_handler,
            sessions::revoke_user_sessions_handler,
        ),
        components(
            schemas(
                NewUserRequest, UpdateUserRequest, VerifyRequest, ResendVerificationRequest,
                UserResponse, ListingResponse, BookingResponse, pagination::Pagination,
                UserFilter, UsersWrapper, PasswordChangeRequest,
                PasswordChangeConfirm, EmailChangeRequest, DeactivateRequest,
                CreateSessionRequest, SessionResponse, SessionNamespaceQuery
            )
        ),
        tags(
            (name = "users", description = "User management endpoints"),
            (name = "sessions", description = "Session authentication & state endpoints")
        ),
    )]
    struct ApiDoc;

    // Register Swagger UI services at the ROOT scope so paths match
    cfg.service(
        SwaggerUi::new("/api/docs/swagger-ui/{_:.*}")
            .url("/api-docs/openapi.json", ApiDoc::openapi()),
    );

    cfg.service(
        web::scope("/api/v1/sessions")
            .route(
                "",
                web::post()
                    .to(sessions::create_session_handler)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{token_hash}",
                web::get()
                    .to(sessions::get_session_handler)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{token_hash}",
                web::delete()
                    .to(sessions::delete_session_handler)
                    .wrap(from_fn(content_negotiation_middleware)),
            ),
    );

    cfg.service(
        web::scope("/api/v1/users")
            .route(
                "",
                web::get()
                    .to(admin::get_all_users)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/",
                web::get()
                    .to(admin::get_all_users)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "",
                web::post()
                    .to(registration::create_user)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/",
                web::post()
                    .to(registration::create_user)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{email}",
                web::get()
                    .to(profile::get_user)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{id}",
                web::patch()
                    .to(profile::update_user)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{id}",
                web::delete()
                    .to(admin::soft_delete_user)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{id}/sessions",
                web::delete()
                    .to(sessions::revoke_user_sessions_handler)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/sessions",
                web::delete()
                    .to(sessions::revoke_user_sessions_handler)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{id}/restore",
                web::post()
                    .to(admin::restore_user)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{id}/hard",
                web::delete()
                    .to(admin::hard_delete_user)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{email}/bookings",
                web::get()
                    .to(profile::get_user_bookings)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{email}/listings",
                web::get()
                    .to(profile::get_user_listings) // TODO: implement
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/login",
                web::post()
                    .to(registration::login)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/verify",
                web::post()
                    .to(registration::verify_user)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/resend-verification",
                web::post()
                    .to(registration::resend_verification)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/profile/password/request",
                web::post()
                    .to(account::request_password_change)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/profile/password/confirm",
                web::post()
                    .to(account::confirm_password_change)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/profile/email",
                web::post()
                    .to(account::change_email)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/profile/deactivate",
                web::post()
                    .to(account::deactivate_account)
                    .wrap(from_fn(content_negotiation_middleware)),
            ),
    );
}

#[cfg(test)]
mod tests;
