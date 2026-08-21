use actix_web::middleware::from_fn;
use actix_web::{HttpRequest, HttpResponse, Responder, web};
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
use common::pricing::BookingCalculator;
use db_core::booking as db_booking;
use db_core::listing as db_listing;
use db_core::models::{
    BookingMetadata, BookingStatus, CancellationPolicy, FeeItem, NewBooking, UpdatedBooking,
};
use rand::RngExt;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;
use validator::Validate;

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

    let total_days = (req_data.check_out - req_data.check_in).num_days() as i32;
    if total_days <= 0 {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(
                "Check-out date must be after check-in date".to_string(),
            ),
        ));
    }

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
    .map_err(|err| match err {
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
    })?;

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

    let policy = match req_data.agreed_cancellation_policy.to_lowercase().as_str() {
        "strict" => CancellationPolicy::Strict,
        "moderate" => CancellationPolicy::Moderate,
        _ => CancellationPolicy::Flexible, // Default fallback
    };

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
        };

        match db_booking::create_booking(pool.get_ref(), &new_booking).await {
            Ok(booking) => {
                tracing::info!(booking_id = %booking.id, "Successfully created booking");
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
        let mut rates = std::collections::HashMap::new();
        for booking in &response {
            let base = &booking.currency;
            #[allow(clippy::collapsible_if)]
            if !rates.contains_key(base) {
                if let Ok((rate, final_curr)) = db_core::currency::get_exchange_rate_and_currency(
                    pool.get_ref(),
                    base,
                    target_currency,
                )
                .await
                {
                    rates.insert(base.clone(), (rate, final_curr));
                }
            }
        }
        for booking in &mut response {
            if let Some((rate, final_curr)) = rates.get(&booking.currency) {
                booking.daily_rate = (booking.daily_rate * rate).round_dp(2);
                booking.sub_total_price = (booking.sub_total_price * rate).round_dp(2);
                booking.total_price = (booking.total_price * rate).round_dp(2);
                if let Some(d) = booking.discount_value {
                    booking.discount_value = Some((d * rate).round_dp(2));
                }
                if let Some(t) = booking.tax_value {
                    booking.tax_value = Some((t * rate).round_dp(2));
                }
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

    if let Some(target_currency) = &currency_query.currency {
        #[allow(clippy::collapsible_if)]
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
            if let Some(d) = response.discount_value {
                response.discount_value = Some((d * rate).round_dp(2));
            }
            if let Some(t) = response.tax_value {
                response.tax_value = Some((t * rate).round_dp(2));
            }
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
        (status = 200, description = "Booking updated", body = BookingResponse),
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

    let updated_data = UpdatedBooking {
        status: body.status,
        metadata: body.metadata.clone(),
    };

    let booking = db_booking::update_booking(pool.get_ref(), *id, &updated_data)
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
            check_availability,
            create_booking,
            get_bookings,
            get_booking_by_id,
            get_user_bookings,
            update_booking,
            delete_booking,
            transfer_booking,
            api_core::health::health_check,
        ),
        components(
            schemas(NewBookingRequest, UpdatedBookingRequest, common::models::TransferBookingRequest, AvailabilityResponse, BookingResponse, pagination::Pagination, FeeItem, BookingStatus, CancellationPolicy, api_core::health::PingResponse)
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
                "/health_check",
                web::get().to(api_core::health::health_check),
            )
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
            ),
    );
}

#[cfg(test)]
#[path = "apis_test.rs"]
mod tests;
