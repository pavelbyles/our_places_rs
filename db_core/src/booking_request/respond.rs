//! A host's accept or decline, and a guest's withdrawal.
//!
//! Each takes the listing lock, then the request's row lock, so it serialises
//! with holds, conversions, quotes and other responses. Expiry is checked with
//! `clock_timestamp()`, because `NOW()` is fixed when the transaction began
//! and a response that waited on a lock must not succeed after its window.

use super::history::{Actor, HistorySubject, record_history};
use crate::error::{DbError, Result};
use crate::models::{BookingMetadata, DbBookingRequest};
use chrono::{DateTime, Utc};
use common::booking_request::{
    DeclineReason, RequestOrigin, RequestRefusal, RequestStatus, StayPriceSnapshot, check_decline,
    offer_window,
};
use sqlx::PgConnection;
use sqlx::types::Json;
use uuid::Uuid;

/// A request row locked for change, with the database clock and the
/// listing's state read under the same locks.
pub(super) struct Locked {
    pub(super) request: DbBookingRequest,
    pub(super) listing_active: bool,
    pub(super) gap: i32,
    pub(super) now: DateTime<Utc>,
}

/// Locks the request's listing, then the request. Not found is `RowNotFound`.
pub(super) async fn lock_request(conn: &mut PgConnection, request_id: Uuid) -> Result<Locked> {
    let listing_id = sqlx::query_scalar!(
        "SELECT listing_id FROM booking_request WHERE id = $1",
        request_id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;
    let listing = sqlx::query!(
        r#"SELECT is_active, LEAST(GREATEST(days_between_bookings, 0), 365) AS "gap!"
        FROM listing WHERE id = $1 FOR UPDATE"#,
        listing_id
    )
    .fetch_one(&mut *conn)
    .await?;
    let request = sqlx::query_as!(
        DbBookingRequest,
        r#"
        SELECT id, conversation_id, listing_id, guest_id, enquiry_id,
            origin AS "origin: RequestOrigin", status AS "status: RequestStatus",
            date_from, date_to, number_of_persons, currency,
            price_snapshot AS "price_snapshot: Json<StayPriceSnapshot>",
            metadata AS "metadata: Json<BookingMetadata>",
            promo_code_id, current_quote_id, expires_at, first_response_at,
            decline_reason AS "decline_reason: DeclineReason", decline_note, booking_id,
            created_at, updated_at
        FROM booking_request WHERE id = $1 FOR UPDATE
        "#,
        request_id
    )
    .fetch_one(&mut *conn)
    .await?;
    let now = sqlx::query_scalar!(r#"SELECT clock_timestamp() AS "now!""#)
        .fetch_one(&mut *conn)
        .await?;
    Ok(Locked {
        request,
        listing_active: listing.is_active,
        gap: listing.gap,
        now,
    })
}

/// Refuses a move the lifecycle doesn't allow, or one on a request whose
/// window has already ended.
pub(super) fn check_move(locked: &Locked, to: RequestStatus) -> Result<()> {
    let from = locked.request.status;
    if !from.can_transition_to(to) {
        return Err(DbError::RequestRefused(RequestRefusal::RequestNotOpen));
    }
    if locked.request.expires_at <= locked.now {
        let refusal = if from == RequestStatus::Quoted {
            RequestRefusal::QuoteExpired
        } else {
            RequestRefusal::RequestExpired
        };
        return Err(DbError::RequestRefused(refusal));
    }
    Ok(())
}

/// The host or an admin accepts an open request at its requested price. The
/// dates stay held as an offer for `offer_hours` (capped at check-in); no
/// booking exists until the guest proceeds to checkout.
#[tracing::instrument(skip(pool, actor), fields(actor_id = %actor.id))]
pub async fn accept_request(
    pool: &sqlx::PgPool,
    request_id: Uuid,
    actor: Actor,
    offer_hours: i64,
) -> Result<DbBookingRequest> {
    let mut tx = pool.begin().await?;
    let locked = lock_request(&mut tx, request_id).await?;
    check_move(&locked, RequestStatus::Accepted)?;
    if !locked.listing_active {
        return Err(DbError::ListingUnavailable);
    }
    let expires_at = offer_window(locked.now, offer_hours, locked.request.date_from)
        .map_err(DbError::RequestRefused)?;
    let request = sqlx::query_as!(
        DbBookingRequest,
        r#"
        UPDATE booking_request
        SET status = 'accepted', expires_at = $2,
            first_response_at = COALESCE(first_response_at, $3), updated_at = $3
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
        request_id,
        expires_at,
        locked.now
    )
    .fetch_one(&mut *tx)
    .await?;
    record_history(
        &mut tx,
        HistorySubject::Request(request_id),
        Some(locked.request.status.as_str()),
        RequestStatus::Accepted.as_str(),
        None,
        Some(actor),
        "Accepted",
    )
    .await?;
    tx.commit().await?;
    Ok(request)
}

/// The host or an admin declines a request, or withdraws an offer already
/// made, releasing its dates. `Other` needs a note; both are shown to the
/// guest.
#[tracing::instrument(skip(pool, actor, note), fields(actor_id = %actor.id))]
pub async fn decline_request(
    pool: &sqlx::PgPool,
    request_id: Uuid,
    actor: Actor,
    reason: DeclineReason,
    note: Option<&str>,
) -> Result<DbBookingRequest> {
    check_decline(reason, note).map_err(DbError::RequestRefused)?;
    let note = note.map(str::trim).filter(|n| !n.is_empty());
    let mut tx = pool.begin().await?;
    let locked = lock_request(&mut tx, request_id).await?;
    check_move(&locked, RequestStatus::Declined)?;
    let request = sqlx::query_as!(
        DbBookingRequest,
        r#"
        UPDATE booking_request
        SET status = 'declined', decline_reason = $2, decline_note = $3,
            first_response_at = COALESCE(first_response_at, $4), updated_at = $4
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
        request_id,
        reason as DeclineReason,
        note,
        locked.now
    )
    .fetch_one(&mut *tx)
    .await?;
    record_history(
        &mut tx,
        HistorySubject::Request(request_id),
        Some(locked.request.status.as_str()),
        RequestStatus::Declined.as_str(),
        None,
        Some(actor),
        &format!("Declined: {}", reason.as_str()),
    )
    .await?;
    tx.commit().await?;
    Ok(request)
}

/// The guest withdraws their own request or offer, releasing its dates.
#[tracing::instrument(skip(pool))]
pub async fn withdraw_request(
    pool: &sqlx::PgPool,
    request_id: Uuid,
    guest_id: Uuid,
) -> Result<DbBookingRequest> {
    let mut tx = pool.begin().await?;
    let locked = lock_request(&mut tx, request_id).await?;
    if locked.request.guest_id != guest_id {
        return Err(DbError::RequestRefused(RequestRefusal::NotParty));
    }
    check_move(&locked, RequestStatus::Withdrawn)?;
    let request = sqlx::query_as!(
        DbBookingRequest,
        r#"
        UPDATE booking_request
        SET status = 'withdrawn', updated_at = $2
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
        request_id,
        locked.now
    )
    .fetch_one(&mut *tx)
    .await?;
    record_history(
        &mut tx,
        HistorySubject::Request(request_id),
        Some(locked.request.status.as_str()),
        RequestStatus::Withdrawn.as_str(),
        None,
        Some(Actor {
            id: guest_id,
            role: crate::models::DbMessageSenderRole::Guest,
        }),
        "Withdrawn by the guest",
    )
    .await?;
    tx.commit().await?;
    Ok(request)
}
