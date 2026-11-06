//! Requests to book (#105): submission, reading, and (in later handlers) the
//! host's responses and the guest's checkout.
//!
//! Message text and promo codes never enter a log or span: handlers use
//! `skip_all` and log ids only.

use super::stay_pricing::{
    OnRefusal, PromoRequest, StayRequest, check_guest_count, load_listing, price_stay, stay_today,
};
use super::{caller, is_admin, not_found};
use actix_web::{HttpRequest, Responder, web};
use api_core::auth::Claims;
use api_core::error::ApiError;
use api_core::response::{Payload, respond};
use api_core::settings::Settings;
use chrono::Utc;
use common::booking_request::{
    BookingRequestList, BookingRequestResponse, CreateBookingRequestBody, PriceBreakdown,
    QuoteSummary, RequestRefusal, StayPriceSnapshot, response_window,
};
use db_core::booking_request as db_request;
use db_core::error::DbError;
use db_core::models::{BookingMetadata, CancellationPolicy, DbBookingRequest, DbRequestQuote};
use sqlx::PgPool;
use uuid::Uuid;
use validator::Validate;

/// Most requests a guest's list returns.
const GUEST_LIST_LIMIT: i64 = 50;

/// How the caller relates to a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PartyRole {
    /// The guest who made it.
    Guest,
    /// The listing's host.
    Host,
    /// An admin, acting for the host.
    Admin,
}

fn refused(refusal: RequestRefusal) -> ApiError {
    ApiError::Database(DbError::RequestRefused(refusal))
}

/// Loads a request and checks the caller is its guest, the listing's host or
/// an admin. Anyone else gets 403 `NOT_REQUEST_PARTY`.
pub(super) async fn authorize_request_party(
    pool: &PgPool,
    claims: &Claims,
    request_id: Uuid,
) -> Result<(db_request::RequestDetail, db_core::models::User, PartyRole), ApiError> {
    let detail = db_request::get_request(pool, request_id)
        .await
        .map_err(|e| {
            tracing::error!(%request_id, error = ?e, "failed to load request");
            ApiError::Database(e)
        })?
        .ok_or_else(not_found)?;
    let user = caller(pool, claims).await?;
    let role = if claims.sub == detail.request.guest_id {
        PartyRole::Guest
    } else if claims.sub == detail.host_id {
        PartyRole::Host
    } else if is_admin(&user) {
        PartyRole::Admin
    } else {
        tracing::warn!(%request_id, user_id = %claims.sub, "request access denied: caller is not a party");
        return Err(refused(RequestRefusal::NotParty));
    };
    Ok((detail, user, role))
}

fn quote_summary(quote: &DbRequestQuote) -> QuoteSummary {
    QuoteSummary {
        id: quote.id,
        stay_price_base: quote.stay_price_base,
        terms: quote.terms.clone(),
        price: PriceBreakdown::from(&quote.price_snapshot.0),
        valid_until: quote.valid_until,
        created_at: quote.created_at,
    }
}

/// The API view of a stored request.
pub(super) fn request_response(
    request: &DbBookingRequest,
    quote: Option<&DbRequestQuote>,
) -> BookingRequestResponse {
    let snapshot: &StayPriceSnapshot = &request.price_snapshot.0;
    BookingRequestResponse {
        id: request.id,
        conversation_id: request.conversation_id,
        listing_id: request.listing_id,
        guest_id: request.guest_id,
        enquiry_id: request.enquiry_id,
        origin: request.origin,
        status: request.status,
        check_in: request.date_from,
        check_out: request.date_to,
        guests: request.number_of_persons,
        requested_price: PriceBreakdown::from(snapshot),
        promo_applied: snapshot.promo.is_some(),
        quote: quote.map(quote_summary),
        expires_at: request.expires_at,
        first_response_at: request.first_response_at,
        decline_reason: request.decline_reason,
        decline_note: request.decline_note.clone(),
        booking_id: request.booking_id,
        created_at: request.created_at,
    }
}

/// Rejects control characters other than line breaks and tabs in user text.
pub(super) fn check_user_text(text: &str) -> Result<(), ApiError> {
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
        return Err(ApiError::Database(DbError::ValidationError(
            "Text contains invalid characters".to_string(),
        )));
    }
    Ok(())
}

#[tracing::instrument(skip_all, fields(listing_id, guest_id))]
#[utoipa::path(
    post,
    path = "/api/v1/booking-requests",
    tag = "booking-requests",
    request_body = CreateBookingRequestBody,
    responses(
        (status = 201, description = "Request submitted; its dates are held", body = BookingRequestResponse),
        (status = 400, description = "Invalid stay, guests or text"),
        (status = 403, description = "OWN_LISTING"),
        (status = 409, description = "BOOKING_MODE_NOT_ALLOWED, DATE_UNAVAILABLE, REQUEST_ALREADY_LIVE or LISTING_UNAVAILABLE"),
        (status = 422, description = "PROMO_*, FX_RATE_UNAVAILABLE or REQUEST_WINDOW_TOO_SHORT"),
        (status = 503, description = "Lock contention; retry")
    )
)]
/// Submits a request to book. The stay is priced now and the dates are held
/// until the host responds or the window ends.
pub(super) async fn create_booking_request(
    req: HttpRequest,
    claims: Claims,
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    body: web::Json<CreateBookingRequestBody>,
) -> Result<impl Responder, ApiError> {
    body.validate().map_err(ApiError::ValidationError)?;
    let body = body.into_inner();
    tracing::Span::current().record("listing_id", tracing::field::display(body.listing_id));
    tracing::Span::current().record("guest_id", tracing::field::display(claims.sub));
    if let Some(message) = &body.message {
        check_user_text(message)?;
    }
    caller(pool.get_ref(), &claims).await?;

    let listing = load_listing(pool.get_ref(), body.listing_id).await?;
    if listing.user_id == claims.sub {
        return Err(refused(RequestRefusal::OwnListing));
    }
    if body.num_adults < 1 {
        return Err(ApiError::Database(DbError::ValidationError(
            "A request must include at least 1 adult".to_string(),
        )));
    }
    let guests = body.num_adults + body.num_children + body.num_infants;
    let number_of_persons = i32::try_from(guests)
        .map_err(|_| ApiError::Database(DbError::ValidationError("Too many guests".to_string())))?;
    check_guest_count(&listing, number_of_persons)?;
    let policy = body
        .agreed_cancellation_policy
        .parse::<CancellationPolicy>()
        .map_err(|_| {
            ApiError::Database(DbError::ValidationError(format!(
                "Unknown cancellation policy '{}'",
                body.agreed_cancellation_policy
            )))
        })?;

    let priced = price_stay(
        pool.get_ref(),
        StayRequest {
            listing: &listing,
            check_in: body.check_in,
            check_out: body.check_out,
            number_of_persons,
            currency: &body.currency,
            promo: body.promo_code.as_deref().map(|raw_code| PromoRequest {
                raw_code,
                guest_id: claims.sub,
                on_refusal: OnRefusal::Fail,
            }),
            max_stay_nights: settings.application.max_stay_nights,
            today: stay_today(),
            locked_rate: None,
            stay_override: None,
        },
    )
    .await?;
    let snapshot = StayPriceSnapshot::from_calculator(
        &priced.calculator,
        priced.currency.clone(),
        priced.conversion_rate,
        policy,
        priced.promo.map(|p| (p.promo_code_id, p.terms)),
    );
    let expires_at = response_window(
        Utc::now(),
        settings.application.request_response_hours,
        body.check_in,
    )
    .map_err(|refusal| {
        tracing::warn!(code = refusal.code(), "request window refused");
        refused(refusal)
    })?;

    let message = body
        .message
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(str::to_string);
    let created = db_request::create_request(
        pool.get_ref(),
        &db_request::NewRequest {
            listing_id: body.listing_id,
            guest_id: claims.sub,
            enquiry_id: body.enquiry_id,
            date_from: body.check_in,
            date_to: body.check_out,
            number_of_persons,
            snapshot,
            metadata: BookingMetadata {
                num_adults: body.num_adults,
                num_children: body.num_children,
                num_infants: body.num_infants,
                num_pets: body.num_pets,
                message_to_host: message,
                estimated_arrival_time: body.estimated_arrival_time,
                is_business_trip: body.is_business_trip,
                business_company_name: body.business_company_name,
                business_tax_id: body.business_tax_id,
            },
            expires_at,
        },
    )
    .await
    .map_err(|e| {
        if let DbError::PromoRefused(refusal) = &e {
            tracing::warn!(guest_id = %claims.sub, reason = refusal.code(), "promo code refused at request");
        }
        ApiError::Database(e)
    })?;
    tracing::info!(
        request_id = %created.request.id,
        superseded_holds = created.superseded_holds.len(),
        "request to book submitted"
    );

    Ok(respond(
        &req,
        Payload::Item(request_response(&created.request, None)),
        |_| (),
        actix_web::http::StatusCode::CREATED,
    ))
}

#[tracing::instrument(skip_all, fields(request_id = %id))]
#[utoipa::path(
    get,
    path = "/api/v1/booking-requests/{id}",
    tag = "booking-requests",
    responses(
        (status = 200, description = "The request", body = BookingRequestResponse),
        (status = 403, description = "NOT_REQUEST_PARTY"),
        (status = 404, description = "No such request")
    )
)]
/// A request, for its guest, the listing's host or an admin.
pub(super) async fn get_booking_request(
    req: HttpRequest,
    claims: Claims,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    let (detail, _, _) = authorize_request_party(pool.get_ref(), &claims, *id).await?;
    Ok(respond(
        &req,
        Payload::Item(request_response(&detail.request, detail.quote.as_ref())),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v1/booking-requests/mine",
    tag = "booking-requests",
    responses(
        (status = 200, description = "The caller's requests, newest first", body = BookingRequestList)
    )
)]
/// The signed-in guest's own requests, newest first.
pub(super) async fn my_booking_requests(
    req: HttpRequest,
    claims: Claims,
    pool: web::Data<PgPool>,
) -> Result<impl Responder, ApiError> {
    caller(pool.get_ref(), &claims).await?;
    let rows = db_request::list_for_guest(pool.get_ref(), claims.sub, GUEST_LIST_LIMIT)
        .await
        .map_err(ApiError::Database)?;
    let requests = rows
        .iter()
        .map(|(request, quote)| request_response(request, quote.as_ref()))
        .collect();
    Ok(respond(
        &req,
        Payload::Item(BookingRequestList { requests }),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}
