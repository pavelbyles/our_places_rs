//! Enquiries: a guest's question with dates and a guest count. They start
//! the guest × listing conversation and never hold dates.

use super::history::{Actor, HistorySubject, record_history};
use super::request::{lock_listing_for_request, upsert_conversation};
use crate::error::{DbError, Result};
use crate::models::{DbBookingEnquiry, DbMessageSenderRole};
use chrono::NaiveDate;
use common::booking_request::{EnquiryClosedReason, EnquiryStatus, RequestRefusal};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// An enquiry to store.
#[derive(Debug, Clone)]
pub struct NewEnquiry<'a> {
    /// The listing asked about.
    pub listing_id: Uuid,
    /// The signed-in guest.
    pub guest_id: Uuid,
    /// The guest's display name, for the message.
    pub sender_name: &'a str,
    /// Check-in date asked about.
    pub date_from: NaiveDate,
    /// Check-out date asked about.
    pub date_to: NaiveDate,
    /// Guests asked about.
    pub number_of_persons: i32,
    /// The guest's display currency.
    pub currency: &'a str,
    /// The question (user text).
    pub message: &'a str,
}

/// A stored enquiry and the open one it replaced, if any.
#[derive(Debug, Clone)]
pub struct CreatedEnquiry {
    /// The new enquiry.
    pub enquiry: DbBookingEnquiry,
    /// The guest's previous open enquiry on the listing, now closed.
    pub superseded: Option<Uuid>,
}

/// An enquiry with the listing's host, for access checks.
#[derive(Debug, Clone)]
pub struct EnquiryDetail {
    /// The enquiry.
    pub enquiry: DbBookingEnquiry,
    /// The listing's host.
    pub host_id: Uuid,
}

/// Closes an open enquiry with `reason`, with history. Returns whether it was
/// open.
pub(super) async fn close_open(
    conn: &mut PgConnection,
    enquiry_id: Uuid,
    reason: EnquiryClosedReason,
    actor: Option<Actor>,
) -> Result<bool> {
    let closed = sqlx::query_scalar!(
        r#"UPDATE booking_enquiry
        SET status = 'closed', closed_reason = $2, updated_at = NOW()
        WHERE id = $1 AND status = 'open'
        RETURNING id"#,
        enquiry_id,
        reason as EnquiryClosedReason
    )
    .fetch_optional(&mut *conn)
    .await?;
    if closed.is_some() {
        record_history(
            conn,
            HistorySubject::Enquiry(enquiry_id),
            Some(EnquiryStatus::Open.as_str()),
            EnquiryStatus::Closed.as_str(),
            None,
            actor,
            reason.as_str(),
        )
        .await?;
    }
    Ok(closed.is_some())
}

/// Stores an enquiry and posts its question into the conversation. A newer
/// enquiry from the same guest on the listing replaces their open one. Dates
/// aren't checked or held.
#[tracing::instrument(
    skip(pool, new),
    fields(listing_id = %new.listing_id, guest_id = %new.guest_id)
)]
pub async fn create_enquiry(pool: &PgPool, new: &NewEnquiry<'_>) -> Result<CreatedEnquiry> {
    let mut tx = pool.begin().await?;
    lock_listing_for_request(&mut tx, new.listing_id, new.guest_id, false).await?;
    let guest = Some(Actor {
        id: new.guest_id,
        role: DbMessageSenderRole::Guest,
    });
    let previous = sqlx::query_scalar!(
        "SELECT id FROM booking_enquiry WHERE listing_id = $1 AND guest_id = $2 AND status = 'open'",
        new.listing_id,
        new.guest_id
    )
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(previous) = previous {
        close_open(&mut tx, previous, EnquiryClosedReason::Superseded, guest).await?;
    }

    let conversation_id = upsert_conversation(&mut tx, new.listing_id, new.guest_id).await?;
    let enquiry = sqlx::query_as!(
        DbBookingEnquiry,
        r#"
        INSERT INTO booking_enquiry (conversation_id, listing_id, guest_id, date_from, date_to,
            number_of_persons, currency)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, conversation_id, listing_id, guest_id, date_from, date_to,
            number_of_persons, currency, status AS "status: EnquiryStatus",
            closed_reason AS "closed_reason: EnquiryClosedReason", first_response_at,
            created_at, updated_at
        "#,
        conversation_id,
        new.listing_id,
        new.guest_id,
        new.date_from,
        new.date_to,
        new.number_of_persons,
        new.currency
    )
    .fetch_one(&mut *tx)
    .await?;
    record_history(
        &mut tx,
        HistorySubject::Enquiry(enquiry.id),
        None,
        EnquiryStatus::Open.as_str(),
        None,
        guest,
        "Enquiry sent",
    )
    .await?;
    crate::conversation::insert_message_tx(
        &mut tx,
        &crate::conversation::NewMessage {
            conversation_id,
            sender_id: new.guest_id,
            sender_role: DbMessageSenderRole::Guest,
            sender_name: new.sender_name,
            text: new.message,
            booking_id: None,
            enquiry_id: Some(enquiry.id),
            request_id: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(CreatedEnquiry {
        enquiry,
        superseded: previous,
    })
}

/// The host or an admin closes an open enquiry.
#[tracing::instrument(skip(pool, actor), fields(actor_id = %actor.id))]
pub async fn close_enquiry(pool: &PgPool, enquiry_id: Uuid, actor: Actor) -> Result<()> {
    let mut tx = pool.begin().await?;
    // Closing is a response: record it while the enquiry is still open.
    let conversation_id = sqlx::query_scalar!(
        "SELECT conversation_id FROM booking_enquiry WHERE id = $1 FOR UPDATE",
        enquiry_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;
    crate::conversation::record_first_response(&mut tx, conversation_id, chrono::Utc::now())
        .await?;
    if !close_open(
        &mut tx,
        enquiry_id,
        EnquiryClosedReason::ClosedByHost,
        Some(actor),
    )
    .await?
    {
        return Err(DbError::RequestRefused(RequestRefusal::RequestNotOpen));
    }
    tx.commit().await?;
    Ok(())
}

/// An enquiry by id, with the listing's host.
pub async fn get_enquiry(pool: &PgPool, enquiry_id: Uuid) -> Result<Option<EnquiryDetail>> {
    let row = sqlx::query!(
        r#"
        SELECT e.id, e.conversation_id, e.listing_id, e.guest_id, e.date_from, e.date_to,
            e.number_of_persons, e.currency, e.status AS "status: EnquiryStatus",
            e.closed_reason AS "closed_reason: EnquiryClosedReason", e.first_response_at,
            e.created_at, e.updated_at, l.user_id AS host_id
        FROM booking_enquiry e JOIN listing l ON l.id = e.listing_id
        WHERE e.id = $1
        "#,
        enquiry_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| EnquiryDetail {
        enquiry: DbBookingEnquiry {
            id: r.id,
            conversation_id: r.conversation_id,
            listing_id: r.listing_id,
            guest_id: r.guest_id,
            date_from: r.date_from,
            date_to: r.date_to,
            number_of_persons: r.number_of_persons,
            currency: r.currency,
            status: r.status,
            closed_reason: r.closed_reason,
            first_response_at: r.first_response_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
        },
        host_id: r.host_id,
    }))
}

/// A guest's most recent enquiries, newest first.
pub async fn list_enquiries_for_guest(
    pool: &PgPool,
    guest_id: Uuid,
    limit: i64,
) -> Result<Vec<DbBookingEnquiry>> {
    let rows = sqlx::query_as!(
        DbBookingEnquiry,
        r#"
        SELECT id, conversation_id, listing_id, guest_id, date_from, date_to,
            number_of_persons, currency, status AS "status: EnquiryStatus",
            closed_reason AS "closed_reason: EnquiryClosedReason", first_response_at,
            created_at, updated_at
        FROM booking_enquiry WHERE guest_id = $1
        ORDER BY created_at DESC LIMIT $2
        "#,
        guest_id,
        limit
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
