use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::http::header::{ACCEPT, CONTENT_TYPE};
use actix_web::middleware::{Next, from_fn};
use actix_web::{Error, HttpRequest, HttpResponse, Responder, web};
use api_core::response::{Payload, respond};
use api_core::{
    error::ApiError,
    models::{BookingResponse, BookingsWrapper, map_booking_to_response},
    pagination,
    settings::Settings,
};
use chrono::NaiveDate;
use db_core::booking as db_booking;
use db_core::models::{BookingStatus, CancellationPolicy, FeeItem, NewBooking, UpdatedBooking};
use rand::Rng;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;
use validator::Validate;

pub fn generate_confirmation_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

    const LENGTH: usize = 8;
    let mut rng = rand::thread_rng();

    (0..LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct NewBookingRequest {
    pub guest_id: Uuid,
    pub listing_id: Uuid,
    pub date_from: NaiveDate,
    pub date_to: NaiveDate,
    pub currency: String,
    pub daily_rate: Decimal,
    pub number_of_persons: i32,
    pub total_days: i32,
    pub sub_total_price: Decimal,
    pub discount_value: Option<Decimal>,
    pub tax_value: Option<Decimal>,
    pub fee_breakdown: Vec<FeeItem>,
    pub total_price: Decimal,
    pub cancellation_policy: CancellationPolicy,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdatedBookingRequest {
    pub status: Option<BookingStatus>,
}

#[tracing::instrument]
#[utoipa::path(
    post,
    path = "/api/v1/booking",
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

    let confirmation_code = generate_confirmation_code();

    let mut attempts = 0;
    let max_attempts = settings.application.max_attempts;

    loop {
        attempts += 1;
        let new_booking = NewBooking {
            confirmation_code: confirmation_code.clone(),
            guest_id: req_data.guest_id,
            listing_id: req_data.listing_id,
            date_from: req_data.date_from,
            date_to: req_data.date_to,
            currency: req_data.currency.clone(),
            daily_rate: req_data.daily_rate,
            number_of_persons: req_data.number_of_persons,
            total_days: req_data.total_days,
            sub_total_price: req_data.sub_total_price,
            discount_value: req_data.discount_value,
            tax_value: req_data.tax_value,
            fee_breakdown: req_data.fee_breakdown.clone(),
            total_price: req_data.total_price,
            cancellation_policy: req_data.cancellation_policy,
        };

        match db_booking::create_booking(pool.get_ref(), &new_booking).await {
            Ok(booking) => {
                return Ok(respond(
                    &req,
                    Payload::Item(map_booking_to_response(booking)),
                    |_| (),
                    actix_web::http::StatusCode::CREATED,
                ));
            }
            Err(e) => {
                let db_core::error::DbError::Sqlx(ref sqlx_error) = e;
                if let Some(db_error) = sqlx_error.as_database_error()
                    && db_error.code().as_deref() == Some("23505")
                    && let Some(constraint) = db_error.constraint()
                    && constraint == "booking_pkey"
                {
                    if attempts >= max_attempts {
                        return Err(ApiError::Internal);
                    }
                    continue;
                }
                return Err(ApiError::Database(e));
            }
        }
    }
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
) -> Result<impl Responder, ApiError> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(10).min(100);

    let bookings = db_booking::get_bookings(pool.get_ref(), page, per_page)
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
) -> Result<impl Responder, ApiError> {
    let booking = db_booking::get_booking_by_id(pool.get_ref(), *id)
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

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    #[derive(OpenApi)]
    #[openapi(
        paths(
            create_booking,
            get_bookings,
            get_booking_by_id,
            update_booking,
            delete_booking,
            api_core::health::health_check,
        ),
        components(
            schemas(NewBookingRequest, UpdatedBookingRequest, BookingResponse, pagination::Pagination, FeeItem, BookingStatus, CancellationPolicy, api_core::health::PingResponse)
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
                "/",
                web::get()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(get_bookings),
            )
            .route(
                "/",
                web::post()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(create_booking),
            )
            .route(
                "/bookings/{id}",
                web::get()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(get_booking_by_id),
            )
            .route(
                "/bookings/{id}",
                web::patch()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(update_booking),
            )
            .route(
                "/bookings/{id}",
                web::delete()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(delete_booking),
            )
            .route(
                "/health_check",
                web::get().to(api_core::health::health_check),
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
