//! Turning an offer (an accepted request or a quote) into a `pending` hold.

use super::history::{Actor, HistorySubject, record_history};
use super::request::check_dates_free;
use super::respond::{check_move, lock_request};
use crate::booking::{cancel_guest_holds, insert_hold};
use crate::error::{DbError, Result};
use crate::models::{Booking, BookingMetadata, DbBookingRequest, DbMessageSenderRole};
use common::booking_request::{
    DeclineReason, RequestOrigin, RequestRefusal, RequestStatus, StayPriceSnapshot,
};
use sqlx::PgPool;
use sqlx::types::Json;
use uuid::Uuid;

/// The guest's checkout of an offer.
#[derive(Debug, Clone)]
pub struct OfferCheckout {
    /// The request.
    pub request_id: Uuid,
    /// The signed-in guest.
    pub guest_id: Uuid,
    /// The quote the guest saw, for a quoted request; `None` for an accepted one.
    pub quote_id: Option<Uuid>,
    /// The new booking's confirmation code.
    pub confirmation_code: String,
    /// How long the new hold lasts (`pending_hold_minutes`).
    pub hold_minutes: i32,
}

/// The hold an offer became, and the request now marked `converted`.
#[derive(Debug, Clone)]
pub struct Converted {
    /// The new `pending` booking.
    pub booking: Booking,
    /// The request, now `converted`.
    pub request: DbBookingRequest,
}

/// Converts an offer into a `pending` hold, in one transaction, at exactly
/// the price the guest accepted: the quote's snapshot for a quoted request,
/// otherwise the request's own.
///
/// Locks: listing → request → currency → the guest's pending bookings →
/// promo code. The request stops holding the dates and the booking starts
/// holding them in the same commit, so there is never a moment when both or
/// neither hold them. A promo use was reserved when the request was
/// submitted, so it is redeemed here without re-checking the code's status
/// or limits.
#[tracing::instrument(skip(pool, checkout), fields(request_id = %checkout.request_id))]
pub async fn convert_offer_to_hold(pool: &PgPool, checkout: &OfferCheckout) -> Result<Converted> {
    let mut tx = pool.begin().await?;
    let locked = lock_request(&mut tx, checkout.request_id).await?;
    let request = &locked.request;
    if request.guest_id != checkout.guest_id {
        return Err(DbError::RequestRefused(RequestRefusal::NotParty));
    }
    check_move(&locked, RequestStatus::Converted)?;

    let quote_snapshot = match (request.status, request.current_quote_id, checkout.quote_id) {
        (RequestStatus::Quoted, Some(current), Some(seen)) if current == seen => {
            let snapshot = sqlx::query_scalar!(
                r#"SELECT price_snapshot AS "price_snapshot: Json<StayPriceSnapshot>"
                FROM booking_request_quote WHERE id = $1"#,
                current
            )
            .fetch_one(&mut *tx)
            .await?;
            Some(snapshot.0)
        }
        (RequestStatus::Accepted, _, None) => None,
        _ => return Err(DbError::RequestRefused(RequestRefusal::QuoteSuperseded)),
    };
    if !locked.listing_active {
        return Err(DbError::ListingUnavailable);
    }
    let snapshot = quote_snapshot.as_ref().unwrap_or(&request.price_snapshot.0);
    crate::currency::ensure_active(&mut tx, &snapshot.currency).await?;
    // Defence in depth: the offer has held these dates all along.
    check_dates_free(
        &mut tx,
        request.listing_id,
        request.guest_id,
        request.date_from,
        request.date_to,
        locked.gap,
        Some(request.id),
    )
    .await?;

    cancel_guest_holds(&mut tx, request.guest_id).await?;
    if let Some(promo) = &snapshot.promo {
        crate::promo::lock_code(&mut tx, promo.promo_code_id).await?;
    }
    let new_booking = request.to_new_booking(snapshot, checkout.confirmation_code.clone());
    let booking = insert_hold(
        &mut tx,
        &new_booking,
        checkout.hold_minutes,
        &format!("Booking created from request {}", request.id),
    )
    .await?;

    let converted = sqlx::query_as!(
        DbBookingRequest,
        r#"
        UPDATE booking_request
        SET status = 'converted', booking_id = $2, updated_at = NOW()
        WHERE id = $1
        RETURNING id, conversation_id, listing_id, guest_id, enquiry_id,
            origin AS "origin: RequestOrigin", status AS "status: RequestStatus",
            date_from, date_to, number_of_persons, currency,
            price_snapshot AS "price_snapshot: Json<StayPriceSnapshot>",
            metadata AS "metadata: Json<BookingMetadata>",
            promo_code_id, current_quote_id, expires_at, first_response_at,
            decline_reason AS "decline_reason: DeclineReason", decline_note, booking_id,
            created_at, updated_at
        "#,
        request.id,
        booking.id
    )
    .fetch_one(&mut *tx)
    .await?;
    record_history(
        &mut tx,
        HistorySubject::Request(request.id),
        Some(request.status.as_str()),
        RequestStatus::Converted.as_str(),
        request
            .current_quote_id
            .filter(|_| quote_snapshot.is_some()),
        Some(Actor {
            id: request.guest_id,
            role: DbMessageSenderRole::Guest,
        }),
        "Proceeded to checkout",
    )
    .await?;
    tx.commit().await?;
    Ok(Converted {
        booking,
        request: converted,
    })
}
