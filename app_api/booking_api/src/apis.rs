use actix_web::middleware::from_fn;
use actix_web::{FromRequest, HttpRequest, HttpResponse, Responder, web};
use api_core::api_common::content_negotiation_middleware;
use api_core::response::{Payload, respond};
use api_core::{
    error::ApiError,
    models::{BookingResponse, BookingsWrapper, map_booking_to_response},
    pagination,
    settings::Settings,
};
use chrono::NaiveDate;
use common::models::NewBookingRequest;
use common::models::{BookingMaterialTerms, is_material_booking_change};
use common::pricing::BookingCalculator;
use db_core::booking as db_booking;
use db_core::booking_message as db_booking_message;
use db_core::listing as db_listing;
use db_core::models::{
    Booking, BookingMetadata, BookingStatus, CancellationPolicy, FeeItem, NewBooking,
    UpdatedBooking,
};
use db_core::payout_ledger as db_payout_ledger;
use rand::RngExt;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;
use validator::Validate;

fn map_pricing_error(err: common::pricing::PricingError) -> ApiError {
    match err {
        common::pricing::PricingError::InvalidDateRange => {
            ApiError::Database(db_core::error::DbError::ValidationError(
                "Check-out date must be after check-in date".to_string(),
            ))
        }
        common::pricing::PricingError::MinNightsNotMet { required, provided } => {
            ApiError::Database(db_core::error::DbError::ValidationError(format!(
                "Minimum night stay requirement not met for seasonal period: required {}, provided {}",
                required, provided
            )))
        }
    }
}

pub fn generate_confirmation_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

    const LENGTH: usize = 8;
    let mut rng = rand::rng();

    (0..LENGTH)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdatedBookingRequest {
    pub status: Option<BookingStatus>,
    pub metadata: Option<BookingMetadata>,
    pub door_access_code: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct AvailabilityParams {
    pub listing_id: Uuid,
    pub date_from: NaiveDate,
    pub date_to: NaiveDate,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AvailabilityResponse {
    pub available: bool,
}

#[tracing::instrument]
#[utoipa::path(
    get,
    path = "/api/v1/bookings/availability",
    tag = "bookings",
    params(AvailabilityParams),
    responses(
        (status = 200, description = "Checked availability", body = AvailabilityResponse),
        (status = 500, description = "Internal server error")
    )
)]
async fn check_availability(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    query: web::Query<AvailabilityParams>,
) -> Result<impl Responder, ApiError> {
    let available = db_booking::check_availability(
        pool.get_ref(),
        query.listing_id,
        query.date_from,
        query.date_to,
    )
    .await
    .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Item(AvailabilityResponse { available }),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument]
#[utoipa::path(
    post,
    path = "/api/v1/bookings",
    tag = "bookings",
    request_body = NewBookingRequest,
    responses(
        (status = 201, description = "Booking created", body = BookingResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    )
)]
async fn create_booking(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    body: web::Json<NewBookingRequest>,
    settings: web::Data<Settings>,
) -> Result<impl Responder, ApiError> {
    body.validate().map_err(ApiError::ValidationError)?;
    let req_data = body.into_inner();

    let listing_details = db_listing::get_listing_by_id(pool.get_ref(), req_data.listing_id)
        .await
        .map_err(|e| {
            if let db_core::error::DbError::Sqlx(sqlx::Error::RowNotFound) = e {
                ApiError::Database(db_core::error::DbError::ValidationError(
                    "Listing not found".to_string(),
                ))
            } else {
                ApiError::Database(e)
            }
        })?;

    let total_days = (req_data.check_out - req_data.check_in)
        .num_days()
        .try_into()
        .ok()
        .filter(|&days| days > 0)
        .ok_or_else(|| {
            ApiError::Database(db_core::error::DbError::ValidationError(
                "Check-out date must be after check-in date".to_string(),
            ))
        })?;

    let mut base_nightly_rate = listing_details
        .listing
        .price_per_night
        .unwrap_or(Decimal::ZERO);
    let mut booking_currency = req_data.currency.clone();
    let mut conversion_rate = Decimal::ONE;

    #[allow(clippy::collapsible_if)]
    if req_data.currency != listing_details.listing.base_currency {
        if let Ok((rate, final_curr)) = db_core::currency::get_exchange_rate_and_currency(
            pool.get_ref(),
            &listing_details.listing.base_currency,
            &req_data.currency,
        )
        .await
        {
            conversion_rate = rate;
            base_nightly_rate = (base_nightly_rate * rate).round_dp(2);
            booking_currency = final_curr;
        }
    }

    let raw_overrides = db_listing::get_active_overrides_for_dates(
        pool.get_ref(),
        req_data.listing_id,
        req_data.check_in,
        req_data.check_out,
    )
    .await
    .unwrap_or_default();

    let converted_overrides: Vec<common::models::PriceOverride> = raw_overrides
        .into_iter()
        .map(|mut ovr| {
            if conversion_rate != Decimal::ONE {
                ovr.nightly_rate = (ovr.nightly_rate * conversion_rate).round_dp(2);
            }
            ovr
        })
        .collect();

    let dynamic_quote = common::pricing::calculate_dynamic_quote(
        base_nightly_rate,
        listing_details.listing.minimum_stay,
        &converted_overrides,
        req_data.check_in,
        req_data.check_out,
    )
    .map_err(map_pricing_error)?;

    let calculator = BookingCalculator::with_subtotal(
        dynamic_quote.effective_daily_rate,
        total_days,
        dynamic_quote.subtotal,
    )
    .apply_discounts(
        listing_details.listing.monthly_discount_percentage,
        listing_details.listing.weekly_discount_percentage,
    )
    .apply_taxes()
    .finalize();

    let confirmation_code = generate_confirmation_code();
    let policy = req_data
        .agreed_cancellation_policy
        .parse::<CancellationPolicy>()
        .unwrap_or(CancellationPolicy::Flexible);

    let mut attempts = 0;
    let max_attempts = settings.application.max_attempts;

    loop {
        attempts += 1;
        let new_booking = NewBooking {
            confirmation_code: confirmation_code.clone(),
            guest_id: req_data.guest_id,
            listing_id: req_data.listing_id,
            date_from: req_data.check_in,
            date_to: req_data.check_out,
            currency: booking_currency.clone(),
            daily_rate: calculator.actual_daily_rate,
            number_of_persons: (req_data.num_adults + req_data.num_children + req_data.num_infants)
                as i32,
            total_days: calculator.total_days,
            sub_total_price: calculator.sub_total_price,
            discount_value: calculator.discount_value,
            tax_value: calculator.tax_value,
            fee_breakdown: calculator.fee_breakdown.clone(),
            total_price: calculator.total_price,
            cancellation_policy: policy,
            metadata: BookingMetadata {
                num_adults: req_data.num_adults,
                num_children: req_data.num_children,
                num_infants: req_data.num_infants,
                num_pets: req_data.num_pets,
                message_to_host: req_data.message_to_host.clone(),
                estimated_arrival_time: req_data.estimated_arrival_time.clone(),
                is_business_trip: req_data.is_business_trip,
            },
            door_access_code: None,
        };

        match db_booking::create_booking(pool.get_ref(), &new_booking).await {
            Ok(booking) => {
                tracing::info!(booking_id = %booking.id, "Successfully created booking");

                let pool_clone = pool.get_ref().clone();
                let guest_id = booking.guest_id;
                let conf_code = booking.confirmation_code.clone();
                let total_price = format!("{} {}", booking.total_price, booking.currency);
                let dates = format!("{} to {}", booking.date_from, booking.date_to);

                tokio::spawn(async move {
                    if let Ok(guest) = db_core::user::get_user_by_id(&pool_clone, guest_id).await
                        && let Ok(outbox) = db_core::email_outbox::insert_email_outbox(
                            &pool_clone,
                            &guest.email,
                            "Your Booking Confirmation - Our Places",
                            common::email::EmailTemplate::BookingConfirmation.as_str(),
                            &serde_json::json!({
                                "confirmation_code": conf_code,
                                "dates": dates,
                                "total_price": total_price,
                            }),
                            3,
                        )
                        .await
                    {
                        let publisher = api_core::email_publisher::EmailPublisher::from_env();
                        let _ = publisher.publish_email_event(outbox.id).await;
                    }
                });

                return Ok(respond(
                    &req,
                    Payload::Item(map_booking_to_response(booking)),
                    |_| (),
                    actix_web::http::StatusCode::CREATED,
                ));
            }
            Err(e) => {
                if let db_core::error::DbError::Sqlx(sqlx_error) = &e
                    && let Some(db_error) = sqlx_error.as_database_error()
                    && db_error.code().as_deref() == Some("23505")
                    && let Some(constraint) = db_error.constraint()
                    && constraint == "booking_pkey"
                {
                    if attempts >= max_attempts {
                        tracing::error!(
                            "Failed to generate unique confirmation code after {} attempts",
                            max_attempts
                        );
                        return Err(ApiError::Internal);
                    }
                    tracing::warn!(
                        "Confirmation code collision, retrying (attempt {})",
                        attempts
                    );
                    continue;
                }
                tracing::error!(error = %e, "Failed to create booking in database");
                return Err(ApiError::Database(e));
            }
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct CurrencyQuery {
    pub currency: Option<String>,
}

#[tracing::instrument]
#[utoipa::path(
    get,
    path = "/api/v1/bookings",
    tag = "bookings",
    params(
        pagination::Pagination
    ),
    responses(
        (status = 200, description = "List of bookings", body = [BookingResponse]),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_bookings(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    query: web::Query<pagination::Pagination>,
    currency_query: web::Query<CurrencyQuery>,
) -> Result<impl Responder, ApiError> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(10).min(100);

    let bookings = db_booking::get_bookings(pool.get_ref(), page, per_page)
        .await
        .map_err(ApiError::Database)?;

    let mut response: Vec<BookingResponse> =
        bookings.into_iter().map(map_booking_to_response).collect();

    if let Some(target_currency) = &currency_query.currency {
        let rates = db_core::currency::get_exchange_rates_cache(
            pool.get_ref(),
            response.iter().map(|b| &b.currency),
            target_currency,
        )
        .await;

        for booking in &mut response {
            if let Some((rate, final_curr)) = rates.get(&booking.currency) {
                booking.daily_rate = (booking.daily_rate * rate).round_dp(2);
                booking.sub_total_price = (booking.sub_total_price * rate).round_dp(2);
                booking.total_price = (booking.total_price * rate).round_dp(2);
                booking.discount_value = booking.discount_value.map(|d| (d * rate).round_dp(2));
                booking.tax_value = booking.tax_value.map(|t| (t * rate).round_dp(2));
                booking.currency = final_curr.clone();
            }
        }
    }

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
    path = "/api/v1/bookings/booking/{id}",
    tag = "bookings",
    responses(
        (status = 200, description = "Booking found", body = BookingResponse),
        (status = 404, description = "Booking not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_booking_by_id(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    currency_query: web::Query<CurrencyQuery>,
) -> Result<impl Responder, ApiError> {
    let booking = db_booking::get_booking_by_id(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?;

    let mut response = map_booking_to_response(booking);

    #[allow(clippy::collapsible_if)]
    if let Some(target_currency) = &currency_query.currency {
        if let Ok((rate, final_curr)) = db_core::currency::get_exchange_rate_and_currency(
            pool.get_ref(),
            &response.currency,
            target_currency,
        )
        .await
        {
            response.daily_rate = (response.daily_rate * rate).round_dp(2);
            response.sub_total_price = (response.sub_total_price * rate).round_dp(2);
            response.total_price = (response.total_price * rate).round_dp(2);
            response.discount_value = response.discount_value.map(|d| (d * rate).round_dp(2));
            response.tax_value = response.tax_value.map(|t| (t * rate).round_dp(2));
            response.currency = final_curr;
        }
    }

    Ok(respond(
        &req,
        Payload::Item(response),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument]
#[utoipa::path(
    patch,
    path = "/api/v1/bookings/booking/{id}",
    tag = "bookings",
    request_body = UpdatedBookingRequest,
    responses(
        (status = 200, description = "Booking updated successfully", body = BookingResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - only hosts or admins may set door access code"),
        (status = 404, description = "Booking not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn update_booking(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    body: web::Json<UpdatedBookingRequest>,
) -> Result<impl Responder, ApiError> {
    body.validate().map_err(ApiError::ValidationError)?;

    // If door_access_code is being modified, verify that caller is the host of the listing or an admin
    if body.door_access_code.is_some() {
        let parties = db_booking_message::get_booking_parties(pool.get_ref(), *id)
            .await
            .map_err(ApiError::Database)?
            .ok_or(ApiError::Database(db_core::error::DbError::Sqlx(
                sqlx::Error::RowNotFound,
            )))?;

        let mut dev_payload = actix_web::dev::Payload::None;
        let claims = match api_core::auth::Claims::from_request(&req, &mut dev_payload).into_inner()
        {
            Ok(c) => c,
            Err(_) => {
                return Err(ApiError::Unauthorized(
                    "Authorization required to configure door access codes".into(),
                ));
            }
        };

        let user = db_core::user::get_user_by_id(pool.get_ref(), claims.sub)
            .await
            .map_err(|_| ApiError::Unauthorized("User not found".to_string()))?;

        let is_admin = user.roles.contains(&db_core::models::UserRole::Admin);
        let is_host = claims.sub == parties.host_id;

        if !is_admin && !is_host {
            return Err(ApiError::Forbidden(
                "Only hosts and administrators can configure property door access codes"
                    .to_string(),
            ));
        }
    }

    let old_booking = db_booking::get_booking_by_id(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?;

    let updated_data = UpdatedBooking {
        status: body.status,
        metadata: body.metadata.clone(),
        door_access_code: body.door_access_code.clone(),
    };

    let booking = db_booking::update_booking(pool.get_ref(), *id, &updated_data)
        .await
        .map_err(ApiError::Database)?;

    dispatch_booking_status_notifications(pool.get_ref(), old_booking, booking.clone()).await;

    Ok(respond(
        &req,
        Payload::Item(map_booking_to_response(booking)),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument]
#[utoipa::path(
    delete,
    path = "/api/v1/bookings/booking/{id}",
    tag = "bookings",
    responses(
        (status = 204, description = "Booking deleted"),
        (status = 404, description = "Booking not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn delete_booking(
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    db_booking::delete_booking(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?;
    Ok(HttpResponse::NoContent().finish())
}

#[tracing::instrument]
#[utoipa::path(
    post,
    path = "/api/v1/bookings/bookings/{id}/transfer",
    tag = "bookings",
    request_body = common::models::TransferBookingRequest,
    responses(
        (status = 200, description = "Booking guest transferred", body = BookingResponse),
        (status = 404, description = "Booking not found or not pending"),
        (status = 500, description = "Internal server error")
    )
)]
async fn transfer_booking(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    body: web::Json<common::models::TransferBookingRequest>,
) -> Result<impl Responder, ApiError> {
    body.validate().map_err(ApiError::ValidationError)?;

    let booking = db_booking::transfer_booking_guest(pool.get_ref(), *id, body.guest_id)
        .await
        .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Item(map_booking_to_response(booking)),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument]
#[utoipa::path(
    get,
    path = "/api/v1/bookings/user/{id}",
    tag = "bookings",
    params(
        ("id" = Uuid, Path, description = "Guest user UUID"),
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
    let per_page = query.per_page.unwrap_or(20).min(100);

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
    path = "/api/v1/bookings/listing/{id}",
    tag = "bookings",
    params(
        ("id" = Uuid, Path, description = "Listing UUID"),
        pagination::Pagination
    ),
    responses(
        (status = 200, description = "List of bookings for listing", body = [BookingResponse]),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_listing_bookings(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    query: web::Query<pagination::Pagination>,
) -> Result<impl Responder, ApiError> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(50).min(100);

    let bookings = db_booking::get_bookings_by_listing_id(pool.get_ref(), *id, page, per_page)
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
    path = "/api/v1/bookings/{id}/messages",
    tag = "bookings",
    responses(
        (status = 200, description = "Messages retrieved", body = common::models::BookingMessagesWrapper),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Booking not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_booking_messages(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    claims: api_core::auth::Claims,
) -> Result<impl Responder, ApiError> {
    let parties = db_booking_message::get_booking_parties(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::Database(db_core::error::DbError::Sqlx(
            sqlx::Error::RowNotFound,
        )))?;

    let user = db_core::user::get_user_by_id(pool.get_ref(), claims.sub)
        .await
        .map_err(|_| ApiError::Unauthorized("User not found".to_string()))?;

    let is_admin = user.roles.contains(&db_core::models::UserRole::Admin);
    let is_guest = claims.sub == parties.guest_id;
    let is_host = claims.sub == parties.host_id;

    if !is_admin && !is_guest && !is_host {
        return Err(ApiError::Unauthorized(
            "Not authorized to access messages".to_string(),
        ));
    }

    let msgs = db_booking_message::list_booking_messages(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?;

    let unread_count = msgs
        .iter()
        .filter(|m| m.read_at.is_none() && m.sender_id != claims.sub)
        .count() as i64;

    let response = common::models::BookingMessagesWrapper {
        messages: msgs.into_iter().map(Into::into).collect(),
        unread_count,
    };

    Ok(respond(
        &req,
        Payload::Item(response),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument]
#[utoipa::path(
    post,
    path = "/api/v1/bookings/{id}/messages",
    tag = "bookings",
    request_body = common::models::CreateBookingMessageRequest,
    responses(
        (status = 201, description = "Message created", body = common::models::BookingMessageResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error")
    )
)]
async fn send_booking_message(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    claims: api_core::auth::Claims,
    body: web::Json<common::models::CreateBookingMessageRequest>,
) -> Result<impl Responder, ApiError> {
    body.validate().map_err(ApiError::ValidationError)?;
    let text = body.into_inner().message_text;

    // Check for null bytes / control chars
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(
                "Message contains invalid characters".to_string(),
            ),
        ));
    }

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError("Message cannot be empty".to_string()),
        ));
    }

    let parties = db_booking_message::get_booking_parties(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::Database(db_core::error::DbError::Sqlx(
            sqlx::Error::RowNotFound,
        )))?;

    if parties.status == BookingStatus::Cancelled {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(
                "Cannot send message to a cancelled booking".to_string(),
            ),
        ));
    }

    let user = db_core::user::get_user_by_id(pool.get_ref(), claims.sub)
        .await
        .map_err(|_| ApiError::Unauthorized("User not found".to_string()))?;

    let is_admin = user.roles.contains(&db_core::models::UserRole::Admin);
    let is_guest = claims.sub == parties.guest_id;
    let is_host = claims.sub == parties.host_id;

    let role = if is_admin {
        db_core::models::DbMessageSenderRole::Admin
    } else if is_host {
        db_core::models::DbMessageSenderRole::Host
    } else if is_guest {
        db_core::models::DbMessageSenderRole::Guest
    } else {
        return Err(ApiError::Unauthorized(
            "Not authorized to send messages".to_string(),
        ));
    };

    let display_name = if is_admin {
        format!("{} (admin)", user.first_name)
    } else {
        user.first_name.clone()
    };

    let msg = db_booking_message::insert_booking_message(
        pool.get_ref(),
        Uuid::now_v7(),
        *id,
        claims.sub,
        role,
        &display_name,
        trimmed,
    )
    .await
    .map_err(ApiError::Database)?;

    let recipient_user_id = if is_host {
        parties.guest_id
    } else {
        parties.host_id
    };
    let pool_clone = pool.get_ref().clone();
    let display_name_clone = display_name.clone();
    let text_clone = trimmed.to_string();
    let booking_id = *id;

    tokio::spawn(async move {
        tracing::info!(
            "Triggered email notification for message in booking {}",
            booking_id
        );
        if let Ok(recipient) = db_core::user::get_user_by_id(&pool_clone, recipient_user_id).await
            && let Ok(outbox) = db_core::email_outbox::insert_email_outbox(
                &pool_clone,
                &recipient.email,
                &format!("New message regarding booking {}", booking_id),
                common::email::EmailTemplate::GuestHostMessageNotification.as_str(),
                &serde_json::json!({
                    "sender_name": display_name_clone,
                    "message_text": text_clone,
                    "booking_id": booking_id.to_string(),
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
        Payload::Item::<common::models::BookingMessageResponse>(msg.into()),
        |_| (),
        actix_web::http::StatusCode::CREATED,
    ))
}

#[tracing::instrument]
#[utoipa::path(
    patch,
    path = "/api/v1/bookings/{id}/messages/read",
    tag = "bookings",
    responses(
        (status = 200, description = "Messages marked as read", body = common::models::MarkMessagesReadResponse),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error")
    )
)]
async fn mark_booking_messages_read(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    claims: api_core::auth::Claims,
) -> Result<impl Responder, ApiError> {
    let parties = db_booking_message::get_booking_parties(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::Database(db_core::error::DbError::Sqlx(
            sqlx::Error::RowNotFound,
        )))?;

    let user = db_core::user::get_user_by_id(pool.get_ref(), claims.sub)
        .await
        .map_err(|_| ApiError::Unauthorized("User not found".to_string()))?;

    let is_admin = user.roles.contains(&db_core::models::UserRole::Admin);
    let is_guest = claims.sub == parties.guest_id;
    let is_host = claims.sub == parties.host_id;

    if !is_admin && !is_guest && !is_host {
        return Err(ApiError::Unauthorized(
            "Not authorized to access messages".to_string(),
        ));
    }

    let updated =
        db_booking_message::mark_booking_messages_as_read(pool.get_ref(), *id, claims.sub)
            .await
            .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Item(common::models::MarkMessagesReadResponse {
            updated_count: updated,
        }),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip(pool))]
async fn get_host_ledger_entries(
    req: HttpRequest,
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
    filter: web::Query<common::payout::PayoutFilter>,
) -> Result<impl Responder, ApiError> {
    let user = db_core::user::get_user_by_id(pool.get_ref(), claims.sub)
        .await
        .map_err(ApiError::Database)?;
    let is_admin = user.roles.contains(&db_core::models::UserRole::Admin);
    let target_host_id = if is_admin {
        filter.host_id
    } else {
        Some(claims.sub)
    };

    let page = filter.page.unwrap_or(1);
    let per_page = filter.per_page.unwrap_or(20);

    let (entries, total_count) = db_payout_ledger::get_payout_ledger_entries(
        pool.get_ref(),
        target_host_id,
        filter.listing_id,
        filter.status.map(Into::into),
        filter.date_from,
        filter.date_to,
        page,
        per_page,
    )
    .await
    .map_err(ApiError::Database)?;

    let response = common::payout::PayoutLedgerResponse {
        entries: entries.into_iter().map(Into::into).collect(),
        total_count,
        page,
        per_page,
    };

    Ok(respond(
        &req,
        Payload::Item(response),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip(pool))]
async fn get_host_ledger_summary(
    req: HttpRequest,
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
    filter: web::Query<common::payout::PayoutFilter>,
) -> Result<impl Responder, ApiError> {
    let user = db_core::user::get_user_by_id(pool.get_ref(), claims.sub)
        .await
        .map_err(ApiError::Database)?;
    let is_admin = user.roles.contains(&db_core::models::UserRole::Admin);
    let target_host_id = if is_admin {
        filter.host_id
    } else {
        Some(claims.sub)
    };

    let summary =
        db_payout_ledger::get_payout_summary(pool.get_ref(), target_host_id, filter.listing_id)
            .await
            .map_err(ApiError::Database)?;

    let common_summary: common::payout::PayoutSummary = summary.into();

    Ok(respond(
        &req,
        Payload::Item(common_summary),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip(pool))]
async fn export_host_ledger_csv(
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
    filter: web::Query<common::payout::PayoutFilter>,
) -> Result<HttpResponse, ApiError> {
    let user = db_core::user::get_user_by_id(pool.get_ref(), claims.sub)
        .await
        .map_err(ApiError::Database)?;
    let is_admin = user.roles.contains(&db_core::models::UserRole::Admin);
    let target_host_id = if is_admin {
        filter.host_id
    } else {
        Some(claims.sub)
    };

    let (entries, _) = db_payout_ledger::get_payout_ledger_entries(
        pool.get_ref(),
        target_host_id,
        filter.listing_id,
        filter.status.map(Into::into),
        filter.date_from,
        filter.date_to,
        1,
        1000,
    )
    .await
    .map_err(ApiError::Database)?;

    let common_entries: Vec<common::payout::PayoutLedgerEntry> =
        entries.into_iter().map(Into::into).collect();
    let csv_content = common::csv::format_payout_ledger_csv(&common_entries);

    Ok(HttpResponse::Ok()
        .content_type("text/csv")
        .insert_header((
            "Content-Disposition",
            "attachment; filename=\"host_payout_ledger.csv\"",
        ))
        .body(csv_content))
}

#[tracing::instrument(skip(pool))]
async fn update_admin_payout_status(
    req: HttpRequest,
    claims: api_core::auth::Claims,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    body: web::Json<common::payout::UpdatePayoutStatusRequest>,
) -> Result<impl Responder, ApiError> {
    let user = db_core::user::get_user_by_id(pool.get_ref(), claims.sub)
        .await
        .map_err(ApiError::Database)?;
    if !user.roles.contains(&db_core::models::UserRole::Admin) {
        return Err(ApiError::Unauthorized("Admin role required".to_string()));
    }

    let ledger_id = path.into_inner();

    let updated = db_payout_ledger::update_payout_status(
        pool.get_ref(),
        ledger_id,
        body.status.into(),
        body.gateway_reference.clone(),
        body.failure_reason.clone(),
    )
    .await
    .map_err(ApiError::Database)?;

    let common_entry: common::payout::PayoutLedgerEntry = updated.into();

    Ok(respond(
        &req,
        Payload::Item(common_entry),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

async fn dispatch_booking_status_notifications(pool: &PgPool, old_b: Booking, new_b: Booking) {
    let pool_clone = pool.clone();

    tokio::spawn(async move {
        let publisher = api_core::email_publisher::EmailPublisher::from_env();

        let listing = match db_listing::get_listing_by_id(&pool_clone, new_b.listing_id).await {
            Ok(l) => l,
            Err(_) => return,
        };
        let guest = match db_core::user::get_user_by_id(&pool_clone, new_b.guest_id).await {
            Ok(u) => u,
            Err(_) => return,
        };
        let host = match db_core::user::get_user_by_id(&pool_clone, listing.listing.user_id).await {
            Ok(u) => u,
            Err(_) => return,
        };

        // 1. Confirmation: status changed to Confirmed
        if old_b.status != BookingStatus::Confirmed && new_b.status == BookingStatus::Confirmed {
            // Guest confirmation
            let guest_payload = common::email::BookingConfirmationGuestPayload {
                booking_id: new_b.id,
                confirmation_code: new_b.confirmation_code.clone(),
                listing_name: listing.listing.name.clone(),
                date_from: new_b.date_from.to_string(),
                date_to: new_b.date_to.to_string(),
                total_price: new_b.total_price,
                currency: new_b.currency.clone(),
                guest_name: format!("{} {}", guest.first_name, guest.last_name),
            };
            if let Ok(outbox) = db_core::email_outbox::insert_email_outbox(
                &pool_clone,
                &guest.email,
                &format!(
                    "Booking Confirmed - {} (Code: {})",
                    listing.listing.name, new_b.confirmation_code
                ),
                common::email::EmailTemplate::BookingConfirmationGuest.as_str(),
                &serde_json::to_value(&guest_payload).unwrap_or_default(),
                3,
            )
            .await
            {
                let _ = publisher.publish_email_event(outbox.id).await;
            }

            // Host confirmation
            let host_payload = common::email::BookingConfirmationHostPayload {
                booking_id: new_b.id,
                confirmation_code: new_b.confirmation_code.clone(),
                listing_name: listing.listing.name.clone(),
                date_from: new_b.date_from.to_string(),
                date_to: new_b.date_to.to_string(),
                total_payout: new_b.sub_total_price,
                currency: new_b.currency.clone(),
                guest_name: format!("{} {}", guest.first_name, guest.last_name),
                host_name: format!("{} {}", host.first_name, host.last_name),
            };
            if let Ok(outbox) = db_core::email_outbox::insert_email_outbox(
                &pool_clone,
                &host.email,
                &format!(
                    "New Booking Confirmed - {} (Code: {})",
                    listing.listing.name, new_b.confirmation_code
                ),
                common::email::EmailTemplate::BookingConfirmationHost.as_str(),
                &serde_json::to_value(&host_payload).unwrap_or_default(),
                3,
            )
            .await
            {
                let _ = publisher.publish_email_event(outbox.id).await;
            }
        }
        // 2. Cancellation: status changed to Cancelled
        else if old_b.status != BookingStatus::Cancelled
            && new_b.status == BookingStatus::Cancelled
        {
            let refund_amount = new_b.total_price;
            let guest_payload = common::email::BookingCancelledGuestPayload {
                booking_id: new_b.id,
                confirmation_code: new_b.confirmation_code.clone(),
                listing_name: listing.listing.name.clone(),
                date_from: new_b.date_from.to_string(),
                date_to: new_b.date_to.to_string(),
                refund_amount,
                currency: new_b.currency.clone(),
                cancellation_policy: format!("{:?}", new_b.cancellation_policy),
            };
            if let Ok(outbox) = db_core::email_outbox::insert_email_outbox(
                &pool_clone,
                &guest.email,
                &format!(
                    "Booking Cancelled - {} (Code: {})",
                    listing.listing.name, new_b.confirmation_code
                ),
                common::email::EmailTemplate::BookingCancelledGuest.as_str(),
                &serde_json::to_value(&guest_payload).unwrap_or_default(),
                3,
            )
            .await
            {
                let _ = publisher.publish_email_event(outbox.id).await;
            }

            let host_payload = common::email::BookingCancelledHostPayload {
                booking_id: new_b.id,
                confirmation_code: new_b.confirmation_code.clone(),
                listing_name: listing.listing.name.clone(),
                date_from: new_b.date_from.to_string(),
                date_to: new_b.date_to.to_string(),
                guest_name: format!("{} {}", guest.first_name, guest.last_name),
                payout_impact: new_b.sub_total_price,
                currency: new_b.currency.clone(),
            };
            if let Ok(outbox) = db_core::email_outbox::insert_email_outbox(
                &pool_clone,
                &host.email,
                &format!(
                    "Booking Cancelled - {} (Code: {})",
                    listing.listing.name, new_b.confirmation_code
                ),
                common::email::EmailTemplate::BookingCancelledHost.as_str(),
                &serde_json::to_value(&host_payload).unwrap_or_default(),
                3,
            )
            .await
            {
                let _ = publisher.publish_email_event(outbox.id).await;
            }
        }
        // 3. Material Update on Confirmed booking
        else if new_b.status == BookingStatus::Confirmed {
            let old_persons = (old_b.metadata.num_adults
                + old_b.metadata.num_children
                + old_b.metadata.num_infants) as i32;
            let new_persons = (new_b.metadata.num_adults
                + new_b.metadata.num_children
                + new_b.metadata.num_infants) as i32;

            let old_terms = BookingMaterialTerms {
                date_from: old_b.date_from,
                date_to: old_b.date_to,
                number_of_persons: old_persons,
                total_price: old_b.total_price,
            };
            let new_terms = BookingMaterialTerms {
                date_from: new_b.date_from,
                date_to: new_b.date_to,
                number_of_persons: new_persons,
                total_price: new_b.total_price,
            };

            if is_material_booking_change(&old_terms, &new_terms) {
                let mut changes = Vec::new();
                if old_b.date_from != new_b.date_from || old_b.date_to != new_b.date_to {
                    changes.push(format!(
                        "Dates changed from {} - {} to {} - {}",
                        old_b.date_from, old_b.date_to, new_b.date_from, new_b.date_to
                    ));
                }
                if old_persons != new_persons {
                    changes.push(format!(
                        "Guest count changed from {} to {}",
                        old_persons, new_persons
                    ));
                }
                if old_b.total_price != new_b.total_price {
                    changes.push(format!(
                        "Total price changed from {} to {}",
                        old_b.total_price, new_b.total_price
                    ));
                }
                let changes_summary = changes.join(", ");

                let guest_payload = common::email::BookingUpdatedGuestPayload {
                    booking_id: new_b.id,
                    confirmation_code: new_b.confirmation_code.clone(),
                    listing_name: listing.listing.name.clone(),
                    date_from: new_b.date_from.to_string(),
                    date_to: new_b.date_to.to_string(),
                    total_price: new_b.total_price,
                    currency: new_b.currency.clone(),
                    changes_summary: changes_summary.clone(),
                };
                if let Ok(outbox) = db_core::email_outbox::insert_email_outbox(
                    &pool_clone,
                    &guest.email,
                    &format!(
                        "Booking Updated - {} (Code: {})",
                        listing.listing.name, new_b.confirmation_code
                    ),
                    common::email::EmailTemplate::BookingUpdatedGuest.as_str(),
                    &serde_json::to_value(&guest_payload).unwrap_or_default(),
                    3,
                )
                .await
                {
                    let _ = publisher.publish_email_event(outbox.id).await;
                }

                let host_payload = common::email::BookingUpdatedHostPayload {
                    booking_id: new_b.id,
                    confirmation_code: new_b.confirmation_code.clone(),
                    listing_name: listing.listing.name.clone(),
                    date_from: new_b.date_from.to_string(),
                    date_to: new_b.date_to.to_string(),
                    total_payout: new_b.sub_total_price,
                    currency: new_b.currency.clone(),
                    guest_name: format!("{} {}", guest.first_name, guest.last_name),
                    changes_summary,
                };
                if let Ok(outbox) = db_core::email_outbox::insert_email_outbox(
                    &pool_clone,
                    &host.email,
                    &format!(
                        "Booking Updated - {} (Code: {})",
                        listing.listing.name, new_b.confirmation_code
                    ),
                    common::email::EmailTemplate::BookingUpdatedHost.as_str(),
                    &serde_json::to_value(&host_payload).unwrap_or_default(),
                    3,
                )
                .await
                {
                    let _ = publisher.publish_email_event(outbox.id).await;
                }
            }
        }
    });
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CronSweepResponse {
    pub status: String,
    pub pre_arrival_processed: usize,
    pub hold_reminders_processed: usize,
}

#[tracing::instrument(skip(req, pool))]
#[utoipa::path(
    post,
    path = "/api/v1/internal/cron/process-scheduled-notifications",
    tag = "bookings",
    responses(
        (status = 200, description = "Scheduled notification sweep completed", body = CronSweepResponse),
        (status = 401, description = "Unauthorized - invalid or missing cron secret"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn process_scheduled_notifications(
    req: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ApiError> {
    static CRON_SECRET_ENV: &str = "CRON_SECRET";
    let configured_secret =
        std::env::var(CRON_SECRET_ENV).unwrap_or_else(|_| "dev-cron-secret".to_string());
    let provided_secret = req
        .headers()
        .get("x-cron-secret")
        .and_then(|v| v.to_str().ok());

    if provided_secret != Some(&configured_secret) {
        tracing::warn!("Unauthorized attempt to trigger scheduled notifications cron");
        return Err(ApiError::Unauthorized(
            "Invalid or missing cron secret".into(),
        ));
    }

    let publisher = api_core::email_publisher::EmailPublisher::from_env();
    let mut pre_arrival_count = 0;
    let mut hold_reminders_count = 0;

    // Sweep 48h Pre-Arrival Candidates
    let pre_arrival_candidates =
        db_core::notification_log::get_48h_pre_arrival_candidates(pool.get_ref())
            .await
            .map_err(ApiError::Database)?;

    for candidate in pre_arrival_candidates {
        let claimed_guest = db_core::notification_log::claim_and_log_notification(
            pool.get_ref(),
            candidate.booking_id,
            "pre_arrival_guide_guest",
            candidate.guest_id,
        )
        .await
        .map_err(ApiError::Database)?;

        if claimed_guest {
            let wifi_ssid = candidate
                .listing_details
                .0
                .get("wifi_ssid")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            let wifi_password = candidate
                .listing_details
                .0
                .get("wifi_password")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            let check_in_instructions = candidate
                .listing_details
                .0
                .get("check_in_instructions")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            let address = candidate
                .listing_details
                .0
                .get("address")
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
                .unwrap_or_else(|| {
                    candidate
                        .listing_city
                        .clone()
                        .unwrap_or_else(|| "Jamaica".to_string())
                });
            let door_code = candidate
                .door_access_code
                .clone()
                .unwrap_or_else(|| "Pending Host Setup".to_string());

            let guest_payload = common::email::PreArrivalGuideGuestPayload {
                booking_id: candidate.booking_id,
                confirmation_code: candidate.confirmation_code.clone(),
                listing_name: candidate.listing_name.clone(),
                address,
                date_from: candidate.date_from.to_string(),
                date_to: candidate.date_to.to_string(),
                door_access_code: door_code,
                wifi_ssid,
                wifi_password,
                check_in_instructions,
            };

            if let Ok(outbox) = db_core::email_outbox::insert_email_outbox(
                pool.get_ref(),
                &candidate.guest_email,
                &format!(
                    "Pre-Arrival Guide for Your Stay at {}",
                    candidate.listing_name
                ),
                common::email::EmailTemplate::PreArrivalGuideGuest.as_str(),
                &serde_json::to_value(&guest_payload).unwrap_or_default(),
                3,
            )
            .await
            {
                let _ = publisher.publish_email_event(outbox.id).await;
                pre_arrival_count += 1;
            }
        }

        let claimed_host = db_core::notification_log::claim_and_log_notification(
            pool.get_ref(),
            candidate.booking_id,
            "host_upcoming_arrival",
            candidate.host_id,
        )
        .await
        .map_err(ApiError::Database)?;

        if claimed_host {
            let host_payload = common::email::HostUpcomingArrivalPayload {
                booking_id: candidate.booking_id,
                confirmation_code: candidate.confirmation_code.clone(),
                listing_name: candidate.listing_name.clone(),
                guest_name: format!(
                    "{} {}",
                    candidate.guest_first_name, candidate.guest_last_name
                ),
                number_of_persons: candidate.number_of_persons,
                date_from: candidate.date_from.to_string(),
                date_to: candidate.date_to.to_string(),
                door_access_code: candidate.door_access_code,
            };

            if let Ok(outbox) = db_core::email_outbox::insert_email_outbox(
                pool.get_ref(),
                &candidate.host_email,
                &format!("Guest Arrival in 48 Hours: {}", candidate.listing_name),
                common::email::EmailTemplate::HostUpcomingArrival.as_str(),
                &serde_json::to_value(&host_payload).unwrap_or_default(),
                3,
            )
            .await
            {
                let _ = publisher.publish_email_event(outbox.id).await;
                pre_arrival_count += 1;
            }
        }
    }

    // Sweep Expiring 2-Hour Holds
    let hold_candidates = db_core::notification_log::get_expiring_hold_candidates(pool.get_ref())
        .await
        .map_err(ApiError::Database)?;

    for candidate in hold_candidates {
        let claimed_hold = db_core::notification_log::claim_and_log_notification(
            pool.get_ref(),
            candidate.booking_id,
            "payment_hold_expiry_reminder",
            candidate.guest_id,
        )
        .await
        .map_err(ApiError::Database)?;

        if claimed_hold {
            let expires_at = candidate.created_at + chrono::Duration::hours(2);
            let hold_payload = common::email::PaymentHoldExpiryReminderPayload {
                booking_id: candidate.booking_id,
                confirmation_code: candidate.confirmation_code.clone(),
                listing_name: candidate.listing_name.clone(),
                expires_at: expires_at.format("%Y-%m-%d %H:%M UTC").to_string(),
                total_price: candidate.total_price,
                currency: candidate.currency.clone(),
                checkout_url: format!(
                    "https://ourplaces.co/checkout/{}",
                    candidate.confirmation_code
                ),
            };

            if let Ok(outbox) = db_core::email_outbox::insert_email_outbox(
                pool.get_ref(),
                &candidate.guest_email,
                &format!(
                    "Reminder: Your Reservation Hold for {} is Expiring Soon",
                    candidate.listing_name
                ),
                common::email::EmailTemplate::PaymentHoldExpiryReminder.as_str(),
                &serde_json::to_value(&hold_payload).unwrap_or_default(),
                3,
            )
            .await
            {
                let _ = publisher.publish_email_event(outbox.id).await;
                hold_reminders_count += 1;
            }
        }
    }

    Ok(HttpResponse::Ok().json(CronSweepResponse {
        status: "success".to_string(),
        pre_arrival_processed: pre_arrival_count,
        hold_reminders_processed: hold_reminders_count,
    }))
}

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    #[derive(OpenApi)]
    #[openapi(
        paths(
            check_availability,
            create_booking,
            get_bookings,
            get_booking_by_id,
            get_user_bookings,
            get_listing_bookings,
            update_booking,
            delete_booking,
            transfer_booking,
            get_booking_messages,
            send_booking_message,
            mark_booking_messages_read,
            process_scheduled_notifications,
        ),
        components(
            schemas(NewBookingRequest, UpdatedBookingRequest, common::models::TransferBookingRequest, AvailabilityResponse, BookingResponse, pagination::Pagination, FeeItem, BookingStatus, CancellationPolicy, common::models::BookingMessageResponse, common::models::BookingMessagesWrapper, common::models::CreateBookingMessageRequest, common::models::MarkMessagesReadResponse, CronSweepResponse)
        ),
        tags(
            (name = "bookings", description = "Booking management endpoints")
        ),
    )]
    struct ApiDoc;

    // Register Swagger UI services at the ROOT scope so paths match
    cfg.service(
        SwaggerUi::new("/api/docs/swagger-ui/{_:.*}")
            .url("/api-docs/openapi.json", ApiDoc::openapi()),
    );

    cfg.service(
        web::scope("/api/v1/bookings")
            .route(
                "/availability",
                web::get()
                    .to(check_availability)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "",
                web::get()
                    .to(get_bookings)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "",
                web::post()
                    .to(create_booking)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{id}",
                web::get()
                    .to(get_user_bookings)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/listing/{id}",
                web::get()
                    .to(get_listing_bookings)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}",
                web::get()
                    .to(get_booking_by_id)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}",
                web::patch()
                    .to(update_booking)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}",
                web::delete()
                    .to(delete_booking)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/transfer",
                web::post()
                    .to(transfer_booking)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/messages",
                web::get()
                    .to(get_booking_messages)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/messages",
                web::post()
                    .to(send_booking_message)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/messages/read",
                web::patch()
                    .to(mark_booking_messages_read)
                    .wrap(from_fn(content_negotiation_middleware)),
            ),
    );

    cfg.service(
        web::scope("/api/v1/hosts/ledger")
            .route(
                "",
                web::get()
                    .to(get_host_ledger_entries)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/summary",
                web::get()
                    .to(get_host_ledger_summary)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route("/export", web::get().to(export_host_ledger_csv)),
    );

    cfg.service(
        web::scope("/api/v1/admin/ledger").route(
            "/{id}/status",
            web::patch()
                .to(update_admin_payout_status)
                .wrap(from_fn(content_negotiation_middleware)),
        ),
    );

    cfg.service(web::scope("/api/v1/internal/cron").route(
        "/process-scheduled-notifications",
        web::post().to(process_scheduled_notifications),
    ));
}

#[cfg(test)]
#[path = "apis_test.rs"]
mod tests;
