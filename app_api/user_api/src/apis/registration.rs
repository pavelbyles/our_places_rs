//! Sign-up, login, email verification and resending codes.

use super::{UserResponse, map_user_to_response};
use actix_web::{HttpRequest, Responder, web};
use api_core::error::ApiError;
use api_core::response::{Payload, respond};
use api_core::settings::Settings;
use chrono::Utc;
use common::models::{LoginRequest, NewUserRequest, ResendVerificationRequest, VerifyRequest};
use db_core::models::{NewUser, UserRole};
use rand::distr::{Alphanumeric, SampleString};
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;
use validator::Validate;

#[tracing::instrument(skip_all)]
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
pub(super) async fn create_user(
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
            verification_code: Some(otp.clone()),
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

        db_core::currency::ensure_active(&mut tx, &user.default_currency)
            .await
            .map_err(|e| {
                tracing::warn!(currency = %user.default_currency, error = %e, "registration currency rejected");
                ApiError::Database(e)
            })?;

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

                let outbox_res = db_core::email_outbox::insert_email_outbox_conn(
                    &mut tx,
                    &created_user.email,
                    "Verify your Our Places account",
                    common::email::EmailTemplate::UserVerificationOtp.as_str(),
                    &serde_json::json!({
                        "code": otp,
                        "first_name": created_user.first_name,
                    }),
                    3,
                )
                .await;

                tx.commit()
                    .await
                    .map_err(|e| ApiError::Database(db_core::error::DbError::Sqlx(e)))?;

                if let Ok(outbox) = outbox_res {
                    let publisher = api_core::email_publisher::EmailPublisher::from_env();
                    tokio::spawn(async move {
                        let _ = publisher.publish_email_event(outbox.id).await;
                    });
                }

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
                    _ => {}
                }
                return Err(ApiError::Database(e));
            }
        }
    }
}

#[tracing::instrument(skip_all)]
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
pub(super) async fn login(
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

#[tracing::instrument(skip_all)]
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
pub(super) async fn verify_user(
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

#[tracing::instrument(skip_all)]
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
pub(super) async fn resend_verification(
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

    let user = updated
        .ok_or_else(|| ApiError::Unauthorized("User not found or already verified".to_string()))?;

    let pool_clone = pool.get_ref().clone();
    let email_clone = payload.email.clone();
    let first_name = user.first_name.clone();
    let otp_clone = otp.clone();

    tokio::spawn(async move {
        if let Ok(outbox) = db_core::email_outbox::insert_email_outbox(
            &pool_clone,
            &email_clone,
            "Your verification code",
            common::email::EmailTemplate::UserVerificationOtp.as_str(),
            &serde_json::json!({
                "code": otp_clone,
                "first_name": first_name,
            }),
            3,
        )
        .await
        {
            let publisher = api_core::email_publisher::EmailPublisher::from_env();
            let _ = publisher.publish_email_event(outbox.id).await;
        }
    });

    Ok(respond(
        &req,
        Payload::Item(map_user_to_response(user)),
        |_: Vec<UserResponse>| (),
        actix_web::http::StatusCode::OK,
    ))
}
