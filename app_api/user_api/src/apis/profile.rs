//! Reading and updating a user, and their listings and bookings.

use super::*;

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

    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 5;

    let id = path.into_inner();

    // --- Admin Guard Middleware Logic ---
    // Fetch the target user to check their roles. If they are an admin, deny the update.
    let target_user = db_core::user::get_user_by_id(pool.get_ref(), id)
        .await
        .map_err(ApiError::Database)?;

    if target_user.roles.contains(&UserRole::Admin) {
        let is_demoting_admin = req_data.roles.as_ref().is_some_and(|roles| {
            let parsed_roles: std::collections::HashSet<UserRole> = roles
                .iter()
                .filter_map(|r| UserRole::from_str(r).ok())
                .collect();
            !parsed_roles.contains(&UserRole::Admin)
        });
        let is_deactivating = req_data.is_active == Some(false);
        let is_unverifying = req_data.is_verified == Some(false);

        if is_demoting_admin || is_deactivating || is_unverifying {
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
            slug: slug.clone(),
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
                    _ => {}
                }
                return Err(ApiError::Database(e));
            }
        }
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
