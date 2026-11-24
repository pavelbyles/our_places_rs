//! Custom quotes: an adjusted stay price plus special terms, on a request or
//! straight on an enquiry. A quote is an offer that holds its dates until it
//! expires; a newer quote replaces the previous one.

use super::enquiry::close_open;
use super::history::{Actor, HistorySubject, record_history};
use super::request::{
    NewRequest, check_dates_free, expire_lapsed_for_guest, insert_row, lock_listing_for_request,
    map_live_violation, refuse_if_live, upsert_conversation,
};
use super::respond::{check_move, lock_request};
use crate::error::{DbError, Result};
use crate::models::{BookingMetadata, DbBookingRequest, DbRequestQuote};
use chrono::{DateTime, Utc};
use common::booking_request::{
    DeclineReason, EnquiryClosedReason, EnquiryStatus, RequestOrigin, RequestRefusal,
    RequestStatus, StayPriceSnapshot, offer_window,
};
use rust_decimal::Decimal;
use sqlx::types::Json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// A priced quote to send.
#[derive(Debug, Clone)]
pub struct NewQuote<'a> {
    /// The stay price the host entered, in the listing's base currency.
    pub stay_price_base: Decimal,
    /// The host's special terms (user text).
    pub terms: &'a str,
    /// The quoted price, promo-free.
    pub snapshot: StayPriceSnapshot,
    /// How long the quote stays open (`quote_valid_hours`), capped at check-in.
    pub valid_hours: i64,
}

/// A sent quote and the request it now makes an offer of.
#[derive(Debug, Clone)]
pub struct SentQuote {
    /// The request, now `quoted`.
    pub request: DbBookingRequest,
    /// The new quote.
    pub quote: DbRequestQuote,
}

/// When a quote is sent and how it is recorded.
struct Sending<'a> {
    /// The request's status before the quote; `None` for one just created.
    from: Option<RequestStatus>,
    /// Who sent it.
    actor: Actor,
    /// When the quote expires.
    valid_until: DateTime<Utc>,
    /// The database clock, read under the locks.
    now: DateTime<Utc>,
    /// The history reason.
    reason: &'a str,
}

/// Inserts a quote, replacing any live one, and marks the request quoted.
async fn insert_quote(
    conn: &mut PgConnection,
    request: &DbBookingRequest,
    quote: &NewQuote<'_>,
    sending: Sending<'_>,
) -> Result<SentQuote> {
    let Sending {
        from,
        actor,
        valid_until,
        now,
        reason,
    } = sending;
    sqlx::query!(
        "UPDATE booking_request_quote SET superseded_at = $2
         WHERE request_id = $1 AND superseded_at IS NULL",
        request.id,
        now
    )
    .execute(&mut *conn)
    .await?;
    let inserted = sqlx::query_as!(
        DbRequestQuote,
        r#"
        INSERT INTO booking_request_quote (request_id, stay_price_base, terms, price_snapshot,
            valid_until, created_by)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, request_id, stay_price_base, terms,
            price_snapshot AS "price_snapshot: Json<StayPriceSnapshot>",
            valid_until, superseded_at, created_by, created_at
        "#,
        request.id,
        quote.stay_price_base,
        quote.terms.trim(),
        Json(&quote.snapshot) as _,
        valid_until,
        actor.id
    )
    .fetch_one(&mut *conn)
    .await?;
    let updated = sqlx::query_as!(
        DbBookingRequest,
        r#"
        UPDATE booking_request
        SET status = 'quoted', current_quote_id = $2, expires_at = $3,
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
        request.id,
        inserted.id,
        valid_until,
        now
    )
    .fetch_one(&mut *conn)
    .await?;
    record_history(
        conn,
        HistorySubject::Request(request.id),
        from.map(RequestStatus::as_str),
        RequestStatus::Quoted.as_str(),
        Some(inserted.id),
        Some(actor),
        reason,
    )
    .await?;
    Ok(SentQuote {
        request: updated,
        quote: inserted,
    })
}

/// The host or an admin quotes on an open or already quoted request. The
/// quote drops any promo code, so the request no longer reserves a use.
#[tracing::instrument(skip(pool, actor, quote), fields(actor_id = %actor.id))]
pub async fn send_quote(
    pool: &PgPool,
    request_id: Uuid,
    actor: Actor,
    quote: &NewQuote<'_>,
) -> Result<SentQuote> {
    let mut tx = pool.begin().await?;
    let locked = lock_request(&mut tx, request_id).await?;
    check_move(&locked, RequestStatus::Quoted)?;
    if !locked.listing_active {
        return Err(DbError::ListingUnavailable);
    }
    let valid_until = offer_window(locked.now, quote.valid_hours, locked.request.date_from)
        .map_err(DbError::RequestRefused)?;
    let sent = insert_quote(
        &mut tx,
        &locked.request,
        quote,
        Sending {
            from: Some(locked.request.status),
            actor,
            valid_until,
            now: locked.now,
            reason: "Custom quote sent",
        },
    )
    .await?;
    tx.commit().await?;
    Ok(sent)
}

/// The host or an admin quotes straight on an open enquiry. The enquiry's
/// dates start being held here, so they are checked under the listing lock;
/// the enquiry becomes a quoted request and closes.
#[tracing::instrument(skip(pool, actor, quote), fields(actor_id = %actor.id))]
pub async fn quote_enquiry(
    pool: &PgPool,
    enquiry_id: Uuid,
    actor: Actor,
    quote: &NewQuote<'_>,
) -> Result<SentQuote> {
    let mut tx = pool.begin().await?;
    let pair = sqlx::query!(
        "SELECT listing_id, guest_id FROM booking_enquiry WHERE id = $1",
        enquiry_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;
    let listing = lock_listing_for_request(&mut tx, pair.listing_id, pair.guest_id, false).await?;
    let enquiry = sqlx::query!(
        r#"SELECT status AS "status: EnquiryStatus", date_from, date_to, number_of_persons
        FROM booking_enquiry WHERE id = $1 FOR UPDATE"#,
        enquiry_id
    )
    .fetch_one(&mut *tx)
    .await?;
    if enquiry.status != EnquiryStatus::Open {
        return Err(DbError::RequestRefused(RequestRefusal::RequestNotOpen));
    }
    expire_lapsed_for_guest(&mut tx, pair.listing_id, pair.guest_id).await?;
    refuse_if_live(&mut tx, pair.listing_id, pair.guest_id).await?;
    crate::currency::ensure_active(&mut tx, &quote.snapshot.currency).await?;
    check_dates_free(
        &mut tx,
        pair.listing_id,
        pair.guest_id,
        enquiry.date_from,
        enquiry.date_to,
        listing.gap,
        None,
    )
    .await?;
    let now = sqlx::query_scalar!(r#"SELECT clock_timestamp() AS "now!""#)
        .fetch_one(&mut *tx)
        .await?;
    let valid_until =
        offer_window(now, quote.valid_hours, enquiry.date_from).map_err(DbError::RequestRefused)?;

    let conversation_id = upsert_conversation(&mut tx, pair.listing_id, pair.guest_id).await?;
    let request = insert_row(
        &mut tx,
        &NewRequest {
            listing_id: pair.listing_id,
            guest_id: pair.guest_id,
            enquiry_id: Some(enquiry_id),
            date_from: enquiry.date_from,
            date_to: enquiry.date_to,
            number_of_persons: enquiry.number_of_persons,
            snapshot: quote.snapshot.clone(),
            metadata: BookingMetadata {
                num_adults: u32::try_from(enquiry.number_of_persons).unwrap_or(1),
                ..BookingMetadata::default()
            },
            expires_at: valid_until,
        },
        conversation_id,
        RequestOrigin::EnquiryQuote,
    )
    .await
    .map_err(|e| match e {
        DbError::Sqlx(e) => map_live_violation(e),
        other => other,
    })?;
    let sent = insert_quote(
        &mut tx,
        &request,
        quote,
        Sending {
            from: None,
            actor,
            valid_until,
            now,
            reason: "Custom quote on an enquiry",
        },
    )
    .await?;
    close_open(
        &mut tx,
        enquiry_id,
        EnquiryClosedReason::Quoted,
        Some(actor),
    )
    .await?;
    tx.commit().await?;
    Ok(sent)
}
