//! Availability checks and unavailable date ranges for a listing.

use actix_web::{HttpRequest, HttpResponse, Responder, web};
use api_core::error::ApiError;
use api_core::response::{Payload, respond};
use chrono::NaiveDate;
use db_core::booking as db_booking;
use db_core::listing as db_listing;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// Query parameters of the availability check.
#[derive(Debug, Deserialize, IntoParams)]
pub struct AvailabilityParams {
    /// The listing to check.
    pub listing_id: Uuid,
    /// First night of the stay.
    pub date_from: NaiveDate,
    /// Departure date of the stay.
    pub date_to: NaiveDate,
}

/// Result of the availability check.
#[derive(Debug, Serialize, ToSchema)]
pub struct AvailabilityResponse {
    /// Whether the stay is free.
    pub available: bool,
}

/// Longest window `unavailable-ranges` accepts, in days.
const MAX_UNAVAILABLE_RANGE_DAYS: i64 = 366;

/// Query parameters of the unavailable ranges endpoint.
#[derive(Debug, Deserialize, IntoParams)]
pub struct UnavailableRangesParams {
    /// First day of the window (inclusive).
    pub from: NaiveDate,
    /// Last day of the window (exclusive).
    pub to: NaiveDate,
}

/// Public: returns only merged date ranges (no booking IDs, guest data or
/// statuses), so the guest calendar can grey out booked nights.
#[tracing::instrument]
#[utoipa::path(
    get,
    path = "/api/v1/bookings/listing/{id}/unavailable-ranges",
    tag = "bookings",
    params(
        ("id" = Uuid, Path, description = "Listing UUID"),
        UnavailableRangesParams
    ),
    responses(
        (status = 200, description = "Booked date ranges in the window", body = common::availability::UnavailableRangesResponse),
        (status = 400, description = "Window is empty, reversed, or longer than 366 days"),
        (status = 404, description = "Listing not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn get_unavailable_ranges(
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    query: web::Query<UnavailableRangesParams>,
) -> Result<impl Responder, ApiError> {
    let span_days = (query.to - query.from).num_days();
    if span_days <= 0 || span_days > MAX_UNAVAILABLE_RANGE_DAYS {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(format!(
                "`to` must be after `from` and at most {MAX_UNAVAILABLE_RANGE_DAYS} days later"
            )),
        ));
    }

    db_listing::get_listing_by_id(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?;

    let ranges = db_booking::get_unavailable_ranges(pool.get_ref(), *id, query.from, query.to)
        .await
        .map_err(ApiError::Database)?;

    Ok(
        HttpResponse::Ok().json(common::availability::UnavailableRangesResponse {
            ranges: common::availability::merge_ranges(ranges),
        }),
    )
}

#[tracing::instrument(skip_all)]
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
pub(super) async fn check_availability(
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
