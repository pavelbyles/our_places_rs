//! An instant hold by a guest replaces their own live request on the listing.

use super::history::{Actor, HistorySubject, record_history};
use super::request::expire_lapsed_for_guest;
use crate::error::Result;
use crate::models::DbMessageSenderRole;
use common::booking_request::{EnquiryClosedReason, EnquiryStatus, RequestStatus};
use sqlx::PgConnection;
use uuid::Uuid;

/// What a guest's instant hold replaced on the listing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Superseded {
    /// Their live request or offer, now `cancelled`.
    pub request: Option<Uuid>,
    /// Their open enquiry, now closed as `booked`.
    pub enquiry: Option<Uuid>,
}

/// Cancels the guest's live request on the listing and closes their open
/// enquiry there, with history, inside the hold's transaction. The caller
/// holds the listing lock, so nothing can change them in between.
pub(crate) async fn supersede_for_instant_hold(
    conn: &mut PgConnection,
    listing_id: Uuid,
    guest_id: Uuid,
) -> Result<Superseded> {
    expire_lapsed_for_guest(conn, listing_id, guest_id).await?;
    let actor = Some(Actor {
        id: guest_id,
        role: DbMessageSenderRole::Guest,
    });

    let request = sqlx::query!(
        r#"
        WITH live AS (
            SELECT id, status::text AS from_status FROM booking_request
            WHERE listing_id = $1 AND guest_id = $2
              AND status IN ('open', 'accepted', 'quoted')
            FOR UPDATE
        )
        UPDATE booking_request r
        SET status = 'cancelled', updated_at = NOW()
        FROM live
        WHERE r.id = live.id
        RETURNING r.id, live.from_status AS "from_status!"
        "#,
        listing_id,
        guest_id
    )
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(row) = &request {
        record_history(
            conn,
            HistorySubject::Request(row.id),
            Some(&row.from_status),
            RequestStatus::Cancelled.as_str(),
            None,
            actor,
            "Superseded by an instant booking",
        )
        .await?;
    }

    let enquiry = sqlx::query_scalar!(
        r#"
        UPDATE booking_enquiry
        SET status = 'closed', closed_reason = 'booked', updated_at = NOW()
        WHERE listing_id = $1 AND guest_id = $2 AND status = 'open'
        RETURNING id
        "#,
        listing_id,
        guest_id
    )
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(id) = enquiry {
        record_history(
            conn,
            HistorySubject::Enquiry(id),
            Some(EnquiryStatus::Open.as_str()),
            EnquiryStatus::Closed.as_str(),
            None,
            actor,
            EnquiryClosedReason::Booked.as_str(),
        )
        .await?;
    }

    Ok(Superseded {
        request: request.map(|r| r.id),
        enquiry,
    })
}
