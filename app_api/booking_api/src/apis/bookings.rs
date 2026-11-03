//! Creating bookings (pending holds) and reading them back.

use super::stay_pricing::{
    OnRefusal, PromoRequest, StayRequest, check_guest_count, log_promo_refusal, price_stay,
};
use super::{
    CurrencyQuery, authorize_booking_party, caller, convert_booking_amounts, display_rate,
    enqueue_email, generate_confirmation_code, is_admin, is_confirmation_code_collision, not_found,
};
use actix_web::{HttpRequest, Responder, web};
use api_core::auth::Claims;
use api_core::response::{Payload, respond};
use api_core::{
    error::ApiError,
    models::{BookingResponse, BookingsWrapper, map_booking_to_response},
    pagination,
    settings::Settings,
};
use common::models::NewBookingRequest;
use db_core::booking as db_booking;
use db_core::listing as db_listing;
use db_core::models::{BookingMetadata, CancellationPolicy, NewBooking};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;
use validator::Validate;

#[tracing::instrument(skip_all)]
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
pub(super) async fn create_booking(
    req: HttpRequest,
    claims: Claims,
    pool: web::Data<PgPool>,
    body: web::Json<NewBookingRequest>,
    settings: web::Data<Settings>,
) -> Result<impl Responder, ApiError> {
    body.validate().map_err(ApiError::ValidationError)?;
    let req_data = body.into_inner();

    // The guest is whoever is signed in. Creating a hold also cancels the
    // guest's other holds, so booking "as" someone else must be impossible.
    if req_data.guest_id != claims.sub {
        tracing::warn!(
            caller = %claims.sub,
            requested_guest = %req_data.guest_id,
            "rejected booking created for a different user"
        );
        return Err(ApiError::Forbidden(
            "You can only create bookings for yourself".to_string(),
        ));
    }

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

    if req_data.num_adults < 1 {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(
                "Booking must include at least 1 adult".to_string(),
            ),
        ));
    }
    let number_of_persons =
        (req_data.num_adults + req_data.num_children + req_data.num_infants) as i32;
    check_guest_count(&listing_details.listing, number_of_persons)?;

    let priced = price_stay(
        pool.get_ref(),
        StayRequest {
            listing: &listing_details.listing,
            check_in: req_data.check_in,
            check_out: req_data.check_out,
            number_of_persons,
            currency: &req_data.currency,
            promo: req_data.promo_code.as_deref().map(|raw_code| PromoRequest {
                raw_code,
                guest_id: req_data.guest_id,
                on_refusal: OnRefusal::Fail,
            }),
            max_stay_nights: settings.application.max_stay_nights,
        },
    )
    .await?;
    let calculator = &priced.calculator;

    let policy = req_data
        .agreed_cancellation_policy
        .parse::<CancellationPolicy>()
        .map_err(|_| {
            ApiError::Database(db_core::error::DbError::ValidationError(format!(
                "Unknown cancellation policy '{}'",
                req_data.agreed_cancellation_policy
            )))
        })?;

    let mut attempts = 0;
    let max_attempts = settings.application.max_attempts.max(1);

    loop {
        attempts += 1;
        // Fresh code per attempt: retrying with the same code would collide again.
        let confirmation_code = generate_confirmation_code();
        let new_booking = NewBooking {
            confirmation_code: confirmation_code.clone(),
            guest_id: req_data.guest_id,
            listing_id: req_data.listing_id,
            date_from: req_data.check_in,
            date_to: req_data.check_out,
            currency: priced.currency.clone(),
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
                business_company_name: req_data.business_company_name.clone(),
                business_tax_id: req_data.business_tax_id.clone(),
            },
            door_access_code: None,
            cleaning_fee: calculator.cleaning_fee,
            damage_deposit: calculator.damage_deposit,
            platform_fee: calculator.platform_fee,
            tax_rate: calculator.tax_rate,
            exchange_rate: priced.conversion_rate,
            promo: priced.promo.clone(),
        };

        match db_booking::create_booking(
            pool.get_ref(),
            &new_booking,
            settings.application.pending_hold_minutes,
        )
        .await
        {
            Ok(booking) => {
                tracing::info!(booking_id = %booking.id, "Successfully created booking");

                let pool_clone = pool.get_ref().clone();
                let guest_id = booking.guest_id;
                let conf_code = booking.confirmation_code.clone();
                let total_price = format!("{} {}", booking.total_price, booking.currency);
                let dates = format!("{} to {}", booking.date_from, booking.date_to);
                let currency = booking.currency.clone();
                let (promo_code, promo_discount) = super::notifications::promo_summary(&booking);

                tokio::spawn(async move {
                    let guest = match db_core::user::get_user_by_id(&pool_clone, guest_id).await {
                        Ok(guest) => guest,
                        Err(e) => {
                            tracing::error!(%guest_id, confirmation_code = %conf_code, error = ?e, "booking confirmation email not sent: guest lookup failed");
                            return;
                        }
                    };
                    enqueue_email(
                        &pool_clone,
                        &guest.email,
                        "Your Booking Confirmation - Our Places",
                        common::email::EmailTemplate::BookingConfirmation.as_str(),
                        &serde_json::json!({
                            "confirmation_code": conf_code,
                            "dates": dates,
                            "total_price": total_price,
                            "currency": currency,
                            "promo_code": promo_code,
                            "promo_discount": promo_discount,
                        }),
                    )
                    .await;
                });

                return Ok(respond(
                    &req,
                    Payload::Item(map_booking_to_response(booking)),
                    |_| (),
                    actix_web::http::StatusCode::CREATED,
                ));
            }
            Err(e) if is_confirmation_code_collision(&e) => {
                if attempts >= max_attempts {
                    tracing::error!(
                        listing_id = %req_data.listing_id,
                        guest_id = %req_data.guest_id,
                        attempts,
                        last_code = %confirmation_code,
                        "gave up generating a unique confirmation code; every attempt collided"
                    );
                    return Err(ApiError::ServiceUnavailable(
                        "We couldn't reserve a confirmation code. Please try again.".to_string(),
                    ));
                }
                tracing::warn!(
                    listing_id = %req_data.listing_id,
                    attempt = attempts,
                    max_attempts,
                    code = %confirmation_code,
                    "confirmation code collision, retrying with a new code"
                );
            }
            Err(db_core::error::DbError::PromoRefused(refusal)) => {
                log_promo_refusal(
                    req_data.guest_id,
                    refusal,
                    req_data.promo_code.as_deref().unwrap_or_default(),
                );
                return Err(ApiError::Database(db_core::error::DbError::PromoRefused(
                    refusal,
                )));
            }
            Err(e) => {
                tracing::error!(
                    listing_id = %req_data.listing_id,
                    guest_id = %req_data.guest_id,
                    check_in = %req_data.check_in,
                    check_out = %req_data.check_out,
                    currency = %priced.currency,
                    error = ?e,
                    "failed to create booking"
                );
                return Err(ApiError::Database(e));
            }
        }
    }
}

#[tracing::instrument(skip_all)]
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
pub(super) async fn get_bookings(
    req: HttpRequest,
    claims: Claims,
    pool: web::Data<PgPool>,
    query: web::Query<pagination::Pagination>,
    currency_query: web::Query<CurrencyQuery>,
) -> Result<impl Responder, ApiError> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(10).min(100);

    // Admins see every booking; anyone else only bookings on their own listings.
    let user = caller(pool.get_ref(), &claims).await?;
    let bookings = if is_admin(&user) {
        db_booking::get_bookings(pool.get_ref(), page, per_page).await
    } else {
        db_booking::get_bookings_by_host_id(pool.get_ref(), claims.sub, page, per_page).await
    }
    .map_err(|e| {
        tracing::error!(user_id = %claims.sub, error = ?e, "failed to list bookings");
        ApiError::Database(e)
    })?;

    let mut response: Vec<BookingResponse> =
        bookings.into_iter().map(map_booking_to_response).collect();

    if let Some(target_currency) = &currency_query.currency {
        let mut rates: std::collections::HashMap<String, Decimal> =
            std::collections::HashMap::new();
        for booking in &mut response {
            let rate = match rates.get(&booking.currency) {
                Some(rate) => *rate,
                None => {
                    let rate =
                        display_rate(pool.get_ref(), &booking.currency, target_currency).await?;
                    rates.insert(booking.currency.clone(), rate);
                    rate
                }
            };
            convert_booking_amounts(booking, rate, target_currency);
        }
    }

    Ok(respond(
        &req,
        Payload::Collection(response),
        |items| BookingsWrapper { booking: items },
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip_all)]
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
pub(super) async fn get_booking_by_id(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    currency_query: web::Query<CurrencyQuery>,
) -> Result<impl Responder, ApiError> {
    authorize_booking_party(&req, pool.get_ref(), *id).await?;

    let booking = db_booking::get_booking_by_id(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?;

    let mut response = map_booking_to_response(booking);

    if let Some(target_currency) = &currency_query.currency {
        let rate = display_rate(pool.get_ref(), &response.currency, target_currency).await?;
        convert_booking_amounts(&mut response, rate, target_currency);
    }

    Ok(respond(
        &req,
        Payload::Item(response),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip_all)]
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
pub(super) async fn get_user_bookings(
    req: HttpRequest,
    claims: Claims,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    query: web::Query<pagination::Pagination>,
) -> Result<impl Responder, ApiError> {
    if *id != claims.sub && !is_admin(&caller(pool.get_ref(), &claims).await?) {
        tracing::warn!(caller = %claims.sub, requested_user = %id, "user bookings access denied");
        return Err(not_found());
    }

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

#[tracing::instrument(skip_all)]
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
pub(super) async fn get_listing_bookings(
    req: HttpRequest,
    claims: Claims,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    query: web::Query<pagination::Pagination>,
) -> Result<impl Responder, ApiError> {
    let host_id = db_listing::get_listing_by_id(pool.get_ref(), *id)
        .await
        .map(|details| details.listing.user_id)
        .map_err(|e| match e {
            db_core::error::DbError::Sqlx(sqlx::Error::RowNotFound) => not_found(),
            e => {
                tracing::error!(listing_id = %id, error = ?e, "failed to load listing for its bookings");
                ApiError::Database(e)
            }
        })?;
    if host_id != claims.sub && !is_admin(&caller(pool.get_ref(), &claims).await?) {
        tracing::warn!(caller = %claims.sub, listing_id = %id, "listing bookings access denied");
        return Err(not_found());
    }

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
