//! Enquiries (#105): a guest's question with dates and a guest count. They
//! start the conversation and never hold dates. Text is never logged.

use super::booking_requests::{
    PartyRole, QuoteStay, check_user_text, price_quote, request_response,
};
use super::request_notifications::{RequestEvent, Subject, notify};
use super::stay_pricing::{check_guest_count, load_listing, stay_today};
use super::{caller, is_admin, not_found};
use actix_web::{HttpRequest, HttpResponse, web};
use api_core::auth::Claims;
use api_core::error::ApiError;
use api_core::response::{Payload, respond};
use api_core::settings::Settings;
use common::availability::validate_stay_dates;
use common::booking_request::{
    BookingRequestResponse, CreateEnquiryBody, EnquiryResponse, QuoteRequestBody, RequestRefusal,
};
use common::currency::CurrencyCode;
use db_core::booking_request as db_request;
use db_core::error::DbError;
use db_core::models::{CancellationPolicy, DbBookingEnquiry, DbMessageSenderRole};
use sqlx::PgPool;
use uuid::Uuid;
use validator::Validate;

/// The API view of a stored enquiry.
pub(super) fn enquiry_response(e: &DbBookingEnquiry) -> EnquiryResponse {
    EnquiryResponse {
        id: e.id,
        conversation_id: e.conversation_id,
        listing_id: e.listing_id,
        guest_id: e.guest_id,
        check_in: e.date_from,
        check_out: e.date_to,
        guests: e.number_of_persons,
        currency: e.currency.clone(),
        status: e.status,
        closed_reason: e.closed_reason,
        first_response_at: e.first_response_at,
        created_at: e.created_at,
    }
}

fn refused(refusal: RequestRefusal) -> ApiError {
    ApiError::Database(DbError::RequestRefused(refusal))
}

/// Loads an enquiry and the caller's role in it, or refuses with 403.
pub(super) async fn authorize_enquiry_party(
    pool: &PgPool,
    claims: &Claims,
    enquiry_id: Uuid,
) -> Result<(db_request::EnquiryDetail, db_core::models::User, PartyRole), ApiError> {
    let detail = db_request::get_enquiry(pool, enquiry_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or_else(not_found)?;
    let user = caller(pool, claims).await?;
    let role = if claims.sub == detail.enquiry.guest_id {
        PartyRole::Guest
    } else if claims.sub == detail.host_id {
        PartyRole::Host
    } else if is_admin(&user) {
        PartyRole::Admin
    } else {
        tracing::warn!(%enquiry_id, user_id = %claims.sub, "enquiry access denied");
        return Err(refused(RequestRefusal::NotParty));
    };
    Ok((detail, user, role))
}

#[tracing::instrument(skip_all, fields(listing_id))]
#[utoipa::path(
    post,
    path = "/api/v1/enquiries",
    tag = "booking-requests",
    request_body = CreateEnquiryBody,
    responses(
        (status = 201, description = "Enquiry sent; no dates are held", body = EnquiryResponse),
        (status = 400, description = "Invalid stay, guests or text"),
        (status = 403, description = "OWN_LISTING"),
        (status = 409, description = "LISTING_UNAVAILABLE")
    )
)]
/// A signed-in guest asks the host a question about a stay.
pub(super) async fn create_enquiry(
    req: HttpRequest,
    claims: Claims,
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    body: web::Json<CreateEnquiryBody>,
) -> Result<HttpResponse, ApiError> {
    body.validate().map_err(ApiError::ValidationError)?;
    let body = body.into_inner();
    tracing::Span::current().record("listing_id", tracing::field::display(body.listing_id));
    check_user_text(&body.message)?;
    let message = body.message.trim();
    if message.is_empty() {
        return Err(ApiError::Database(DbError::ValidationError(
            "Message cannot be empty".to_string(),
        )));
    }
    let user = caller(pool.get_ref(), &claims).await?;
    let listing = load_listing(pool.get_ref(), body.listing_id).await?;
    if listing.user_id == claims.sub {
        return Err(refused(RequestRefusal::OwnListing));
    }
    validate_stay_dates(
        body.check_in,
        body.check_out,
        stay_today(),
        settings.application.max_stay_nights,
    )
    .map_err(|refused| ApiError::Database(DbError::StayInput(refused)))?;
    let guests = i32::try_from(body.guests)
        .map_err(|_| ApiError::Database(DbError::ValidationError("Too many guests".into())))?;
    check_guest_count(&listing, guests)?;
    let currency = body.currency.trim().to_ascii_uppercase();
    CurrencyCode::try_from(currency.as_str())
        .map_err(|e| ApiError::Database(DbError::ValidationError(e.to_string())))?;

    let created = db_request::create_enquiry(
        pool.get_ref(),
        &db_request::NewEnquiry {
            listing_id: body.listing_id,
            guest_id: claims.sub,
            sender_name: &user.first_name,
            date_from: body.check_in,
            date_to: body.check_out,
            number_of_persons: guests,
            currency: &currency,
            message,
        },
    )
    .await
    .map_err(ApiError::Database)?;
    tracing::info!(enquiry_id = %created.enquiry.id, chars = message.chars().count(), "enquiry sent");
    notify(
        pool.get_ref(),
        RequestEvent::EnquirySent,
        Subject::Enquiry(created.enquiry.id),
        Some(message.to_string()),
    );
    Ok(respond(
        &req,
        Payload::Item(enquiry_response(&created.enquiry)),
        |_| (),
        actix_web::http::StatusCode::CREATED,
    ))
}

#[tracing::instrument(skip_all, fields(enquiry_id = %id))]
#[utoipa::path(
    get,
    path = "/api/v1/enquiries/{id}",
    tag = "booking-requests",
    responses(
        (status = 200, description = "The enquiry", body = EnquiryResponse),
        (status = 403, description = "NOT_REQUEST_PARTY"),
        (status = 404, description = "No such enquiry")
    )
)]
/// An enquiry, for its guest, the listing's host or an admin.
pub(super) async fn get_enquiry(
    req: HttpRequest,
    claims: Claims,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let (detail, _, _) = authorize_enquiry_party(pool.get_ref(), &claims, *id).await?;
    Ok(respond(
        &req,
        Payload::Item(enquiry_response(&detail.enquiry)),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip_all, fields(enquiry_id = %id))]
#[utoipa::path(
    post,
    path = "/api/v1/enquiries/{id}/close",
    tag = "booking-requests",
    responses(
        (status = 200, description = "Closed", body = EnquiryResponse),
        (status = 403, description = "NOT_REQUEST_PARTY"),
        (status = 409, description = "REQUEST_NOT_OPEN")
    )
)]
/// The listing's host or an admin closes an enquiry.
pub(super) async fn close_enquiry(
    req: HttpRequest,
    claims: Claims,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let (_, _, role) = authorize_enquiry_party(pool.get_ref(), &claims, *id).await?;
    let role = match role {
        PartyRole::Host => DbMessageSenderRole::Host,
        PartyRole::Admin => DbMessageSenderRole::Admin,
        PartyRole::Guest => return Err(refused(RequestRefusal::NotParty)),
    };
    db_request::close_enquiry(
        pool.get_ref(),
        *id,
        db_request::Actor {
            id: claims.sub,
            role,
        },
    )
    .await
    .map_err(ApiError::Database)?;
    tracing::info!(actor_id = %claims.sub, enquiry_id = %id, action = "close", "enquiry closed");
    let detail = db_request::get_enquiry(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?
        .ok_or_else(not_found)?;
    Ok(respond(
        &req,
        Payload::Item(enquiry_response(&detail.enquiry)),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip_all, fields(enquiry_id = %id))]
#[utoipa::path(
    post,
    path = "/api/v1/enquiries/{id}/quote",
    tag = "booking-requests",
    request_body = QuoteRequestBody,
    responses(
        (status = 201, description = "Quote sent; the enquiry is now a quoted request holding its dates", body = BookingRequestResponse),
        (status = 400, description = "INVALID_QUOTE_PRICE or invalid terms"),
        (status = 403, description = "NOT_REQUEST_PARTY"),
        (status = 409, description = "DATE_UNAVAILABLE, REQUEST_ALREADY_LIVE, REQUEST_NOT_OPEN or LISTING_UNAVAILABLE"),
        (status = 422, description = "FX_RATE_UNAVAILABLE")
    )
)]
/// The host or an admin quotes straight on an enquiry, at today's rate.
pub(super) async fn quote_enquiry(
    req: HttpRequest,
    claims: Claims,
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    id: web::Path<Uuid>,
    body: web::Json<QuoteRequestBody>,
) -> Result<HttpResponse, ApiError> {
    body.validate().map_err(ApiError::ValidationError)?;
    let (detail, _, role) = authorize_enquiry_party(pool.get_ref(), &claims, *id).await?;
    let actor = role.host_side_actor(claims.sub)?;
    let policy = match body.cancellation_policy.as_deref() {
        None | Some("") => CancellationPolicy::Flexible,
        Some(name) => name.parse::<CancellationPolicy>().map_err(|_| {
            ApiError::Database(DbError::ValidationError(format!(
                "Unknown cancellation policy '{name}'"
            )))
        })?,
    };
    let enquiry = &detail.enquiry;
    let snapshot = price_quote(
        pool.get_ref(),
        &settings,
        QuoteStay {
            listing_id: enquiry.listing_id,
            check_in: enquiry.date_from,
            check_out: enquiry.date_to,
            guests: enquiry.number_of_persons,
            currency: &enquiry.currency,
            locked_rate: None,
            policy,
        },
        &body,
    )
    .await?;
    let sent = db_request::quote_enquiry(
        pool.get_ref(),
        *id,
        actor,
        &db_request::NewQuote {
            stay_price_base: body.stay_price,
            terms: &body.terms,
            snapshot,
            valid_hours: settings.application.quote_valid_hours,
        },
    )
    .await
    .map_err(ApiError::Database)?;
    tracing::info!(actor_id = %claims.sub, enquiry_id = %id, request_id = %sent.request.id, action = "quote", "custom quote sent on an enquiry");
    notify(
        pool.get_ref(),
        RequestEvent::Quoted,
        Subject::Request(sent.request.id),
        None,
    );
    Ok(respond(
        &req,
        Payload::Item(request_response(&sent.request, Some(&sent.quote))),
        |_| (),
        actix_web::http::StatusCode::CREATED,
    ))
}
