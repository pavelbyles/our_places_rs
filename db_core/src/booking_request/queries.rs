//! Read-only lookups for requests and their quotes.

use crate::error::Result;
use crate::models::{BookingMetadata, DbBookingRequest, DbRequestQuote};
use common::booking_request::{DeclineReason, RequestOrigin, RequestStatus, StayPriceSnapshot};
use sqlx::PgPool;
use sqlx::types::Json;
use uuid::Uuid;

/// A request with the listing's host and its live quote, for access checks
/// and display.
#[derive(Debug, Clone)]
pub struct RequestDetail {
    /// The request.
    pub request: DbBookingRequest,
    /// The listing's host.
    pub host_id: Uuid,
    /// The listing's base currency, for quotes.
    pub base_currency: String,
    /// The current quote, if any.
    pub quote: Option<DbRequestQuote>,
}

/// A request by id, with its host and current quote.
#[tracing::instrument(skip(pool))]
pub async fn get_request(pool: &PgPool, id: Uuid) -> Result<Option<RequestDetail>> {
    let row = sqlx::query!(
        r#"
        SELECT r.id, r.conversation_id, r.listing_id, r.guest_id, r.enquiry_id,
            r.origin AS "origin: RequestOrigin", r.status AS "status: RequestStatus",
            r.date_from, r.date_to, r.number_of_persons, r.currency,
            r.price_snapshot AS "price_snapshot: Json<StayPriceSnapshot>",
            r.metadata AS "metadata: Json<BookingMetadata>",
            r.promo_code_id, r.current_quote_id, r.expires_at, r.first_response_at,
            r.decline_reason AS "decline_reason: DeclineReason", r.decline_note, r.booking_id,
            r.created_at, r.updated_at,
            l.user_id AS host_id, l.base_currency
        FROM booking_request r
        JOIN listing l ON l.id = r.listing_id
        WHERE r.id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let request = DbBookingRequest {
        id: row.id,
        conversation_id: row.conversation_id,
        listing_id: row.listing_id,
        guest_id: row.guest_id,
        enquiry_id: row.enquiry_id,
        origin: row.origin,
        status: row.status,
        date_from: row.date_from,
        date_to: row.date_to,
        number_of_persons: row.number_of_persons,
        currency: row.currency,
        price_snapshot: row.price_snapshot,
        metadata: row.metadata,
        promo_code_id: row.promo_code_id,
        current_quote_id: row.current_quote_id,
        expires_at: row.expires_at,
        first_response_at: row.first_response_at,
        decline_reason: row.decline_reason,
        decline_note: row.decline_note,
        booking_id: row.booking_id,
        created_at: row.created_at,
        updated_at: row.updated_at,
    };
    let quote = match request.current_quote_id {
        Some(quote_id) => quotes_by_id(pool, &[quote_id]).await?.into_iter().next(),
        None => None,
    };
    Ok(Some(RequestDetail {
        request,
        host_id: row.host_id,
        base_currency: row.base_currency,
        quote,
    }))
}

/// Quotes by id, in one query.
pub async fn quotes_by_id(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<DbRequestQuote>> {
    let quotes = sqlx::query_as!(
        DbRequestQuote,
        r#"
        SELECT id, request_id, stay_price_base, terms,
            price_snapshot AS "price_snapshot: Json<StayPriceSnapshot>",
            valid_until, superseded_at, created_by, created_at
        FROM booking_request_quote
        WHERE id = ANY($1)
        "#,
        ids
    )
    .fetch_all(pool)
    .await?;
    Ok(quotes)
}

/// A guest's most recent requests, newest first, with their current quotes.
#[tracing::instrument(skip(pool))]
pub async fn list_for_guest(
    pool: &PgPool,
    guest_id: Uuid,
    limit: i64,
) -> Result<Vec<(DbBookingRequest, Option<DbRequestQuote>)>> {
    let requests = sqlx::query_as!(
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
        FROM booking_request
        WHERE guest_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
        guest_id,
        limit
    )
    .fetch_all(pool)
    .await?;
    let quote_ids: Vec<Uuid> = requests.iter().filter_map(|r| r.current_quote_id).collect();
    let mut quotes = if quote_ids.is_empty() {
        Vec::new()
    } else {
        quotes_by_id(pool, &quote_ids).await?
    };
    Ok(requests
        .into_iter()
        .map(|r| {
            let quote = r
                .current_quote_id
                .and_then(|id| quotes.iter().position(|q| q.id == id))
                .map(|i| quotes.swap_remove(i));
            (r, quote)
        })
        .collect())
}
