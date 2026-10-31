//! Self-service account changes: password, email, deactivation.

use super::*;

#[tracing::instrument(skip_all)]
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
pub(super) async fn request_password_change(
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

    let user = updated.ok_or(ApiError::Internal)?;

    let pool_clone = pool.get_ref().clone();
    let email_clone = payload.email.clone();
    let first_name = user.first_name.clone();
    let otp_clone = otp.clone();

    tokio::spawn(async move {
        if let Ok(outbox) = db_core::email_outbox::insert_email_outbox(
            &pool_clone,
            &email_clone,
            "Password change request",
            common::email::EmailTemplate::PasswordResetOtp.as_str(),
            &serde_json::json!({
                "token": otp_clone,
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

#[tracing::instrument(skip_all)]
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
pub(super) async fn confirm_password_change(
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

#[tracing::instrument(skip_all)]
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
pub(super) async fn change_email(
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

#[tracing::instrument(skip_all)]
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
pub(super) async fn deactivate_account(
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
