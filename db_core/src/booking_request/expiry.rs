//! Housekeeping sweeps for requests and enquiries. Availability already
//! ignores lapsed requests, so these only tidy statuses and drive the emails.

use super::history::{HistorySubject, record_history};
use crate::error::Result;
use chrono::NaiveDate;
use common::booking_request::{EnquiryClosedReason, EnquiryStatus, RequestStatus};
use sqlx::PgPool;
use uuid::Uuid;

/// A request or offer the sweep expired, with who to tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpiredRequest {
    /// The request.
    pub id: Uuid,
    /// The listing.
    pub listing_id: Uuid,
    /// The guest.
    pub guest_id: Uuid,
    /// The listing's host.
    pub host_id: Uuid,
    /// Its status before it expired.
    pub from_status: String,
}

/// Expires every live request whose window has passed, with history. Rows
/// locked by an in-flight action are skipped and picked up next time.
#[tracing::instrument(skip(pool))]
pub async fn expire_stale_requests(pool: &PgPool) -> Result<Vec<ExpiredRequest>> {
    let mut tx = pool.begin().await?;
    let rows = sqlx::query!(
        r#"
        WITH due AS (
            SELECT id, status::text AS from_status FROM booking_request
            WHERE status IN ('open', 'accepted', 'quoted') AND expires_at <= NOW()
            FOR UPDATE SKIP LOCKED
        )
        UPDATE booking_request r
        SET status = 'expired', updated_at = NOW()
        FROM due, listing l
        WHERE r.id = due.id AND l.id = r.listing_id
        RETURNING r.id, r.listing_id, r.guest_id, l.user_id AS host_id,
            due.from_status AS "from_status!"
        "#
    )
    .fetch_all(&mut *tx)
    .await?;
    let mut expired = Vec::with_capacity(rows.len());
    for row in rows {
        record_history(
            &mut tx,
            HistorySubject::Request(row.id),
            Some(&row.from_status),
            RequestStatus::Expired.as_str(),
            None,
            None,
            "Expired with no response",
        )
        .await?;
        expired.push(ExpiredRequest {
            id: row.id,
            listing_id: row.listing_id,
            guest_id: row.guest_id,
            host_id: row.host_id,
            from_status: row.from_status,
        });
    }
    tx.commit().await?;
    Ok(expired)
}

/// Closes open enquiries whose check-in day (in Jamaica) has arrived.
#[tracing::instrument(skip(pool))]
pub async fn close_past_enquiries(pool: &PgPool, today: NaiveDate) -> Result<u64> {
    let mut tx = pool.begin().await?;
    let closed = sqlx::query_scalar!(
        r#"
        WITH due AS (
            SELECT id FROM booking_enquiry
            WHERE status = 'open' AND date_from <= $1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE booking_enquiry e
        SET status = 'closed', closed_reason = 'expired', updated_at = NOW()
        FROM due WHERE e.id = due.id
        RETURNING e.id
        "#,
        today
    )
    .fetch_all(&mut *tx)
    .await?;
    for id in &closed {
        record_history(
            &mut tx,
            HistorySubject::Enquiry(*id),
            Some(EnquiryStatus::Open.as_str()),
            EnquiryStatus::Closed.as_str(),
            None,
            None,
            EnquiryClosedReason::Expired.as_str(),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(closed.len() as u64)
}
