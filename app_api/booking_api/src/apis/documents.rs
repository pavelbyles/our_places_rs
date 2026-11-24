//! Cancellation quotes and statutory tax documents for a booking.

use super::check_booking_authorization;
use actix_web::{HttpRequest, HttpResponse, Responder, web};
use api_core::error::ApiError;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v1/bookings/{id}/cancellation-quote",
    tag = "bookings",
    responses(
        (status = 200, description = "Cancellation refund quote calculated according to policy schedule", body = common::cancellation::RefundQuote),
        (status = 404, description = "Booking not found or caller unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn get_cancellation_quote(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    let booking_id = *id;
    check_booking_authorization(&req, pool.get_ref(), booking_id).await?;

    let booking = db_core::booking::get_booking_by_id(pool.get_ref(), booking_id)
        .await
        .map_err(ApiError::Database)?;

    let now = Utc::now();
    let accommodation_net = booking.accommodation_net();
    let amounts = common::cancellation::RefundableAmounts::new(
        accommodation_net,
        booking.cleaning_net(),
        booking.damage_deposit,
        booking.platform_fee,
        booking.tax_rate,
        booking.total_price,
    );

    let quote = common::cancellation::quote_refund(
        booking.cancellation_policy,
        amounts,
        booking.created_at,
        booking.date_from,
        now,
    )
    .map_err(|e| ApiError::Database(db_core::error::DbError::ValidationError(e.to_string())))?;

    Ok(HttpResponse::Ok().json(quote))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v1/bookings/{id}/documents",
    tag = "bookings",
    responses(
        (status = 200, description = "List of statutory tax documents for booking", body = Vec<common::tax_document::TaxDocumentResponse>),
        (status = 404, description = "Booking not found or caller unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn get_booking_documents(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    check_booking_authorization(&req, pool.get_ref(), *id).await?;

    let docs = db_core::tax_document::list_for_booking(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?;

    let response: Vec<common::tax_document::TaxDocumentResponse> =
        docs.into_iter().map(|d| d.into()).collect();

    Ok(HttpResponse::Ok().json(response))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v1/bookings/{id}/documents/{number}",
    tag = "bookings",
    responses(
        (status = 200, description = "Statutory tax document found", body = common::tax_document::TaxDocumentResponse),
        (status = 404, description = "Document not found or caller unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn get_booking_document_by_number(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, String)>,
) -> Result<impl Responder, ApiError> {
    let (booking_id, document_number) = path.into_inner();
    check_booking_authorization(&req, pool.get_ref(), booking_id).await?;

    let doc = db_core::tax_document::get_by_number(pool.get_ref(), booking_id, &document_number)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::Database(db_core::error::DbError::Sqlx(
            sqlx::Error::RowNotFound,
        )))?;

    let response: common::tax_document::TaxDocumentResponse = doc.into();
    Ok(HttpResponse::Ok().json(response))
}
