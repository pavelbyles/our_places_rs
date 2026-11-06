//! The admin and host inbox: open enquiries and live requests, with
//! response-time tracking. One query per list; no per-row lookups.

use crate::error::Result;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Which items to show.
#[derive(Debug, Clone, Default)]
pub struct InboxFilter {
    /// Only this host's listings; `None` shows every listing (admins).
    pub host_id: Option<Uuid>,
    /// Only this listing.
    pub listing_id: Option<Uuid>,
    /// Only `"enquiry"` or `"request"` items.
    pub kind: Option<String>,
}

/// One open enquiry or live request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxRow {
    /// `"enquiry"` or `"request"`.
    pub kind: String,
    /// The enquiry or request id.
    pub id: Uuid,
    /// The conversation.
    pub conversation_id: Uuid,
    /// The listing.
    pub listing_id: Uuid,
    /// The listing's name.
    pub listing_name: String,
    /// The host.
    pub host_id: Uuid,
    /// The guest.
    pub guest_id: Uuid,
    /// The guest's first name.
    pub guest_name: String,
    /// Check-in.
    pub date_from: NaiveDate,
    /// Check-out.
    pub date_to: NaiveDate,
    /// Guests.
    pub guests: i32,
    /// The enquiry's or request's status.
    pub status: String,
    /// When it arrived.
    pub created_at: DateTime<Utc>,
    /// When the host first responded.
    pub first_response_at: Option<DateTime<Utc>>,
    /// When a request's window ends; `None` for enquiries.
    pub expires_at: Option<DateTime<Utc>>,
    /// Latest message in the thread.
    pub last_message_at: DateTime<Utc>,
}

/// Open enquiries and live requests, soonest-expiring requests first, then
/// the most recently active threads. Paged by `limit`/`offset`.
#[tracing::instrument(skip(pool))]
pub async fn inbox_items(
    pool: &PgPool,
    filter: &InboxFilter,
    limit: i64,
    offset: i64,
) -> Result<Vec<InboxRow>> {
    let rows = sqlx::query!(
        r#"
        SELECT kind AS "kind!", id AS "id!", conversation_id AS "conversation_id!",
            listing_id AS "listing_id!", listing_name AS "listing_name!", host_id AS "host_id!",
            guest_id AS "guest_id!", guest_name AS "guest_name!", date_from AS "date_from!",
            date_to AS "date_to!", guests AS "guests!", status AS "status!",
            created_at AS "created_at!", first_response_at, expires_at,
            last_message_at AS "last_message_at!"
        FROM (
            SELECT 'enquiry'::text AS kind, e.id, e.conversation_id, e.listing_id,
                l.name AS listing_name, l.user_id AS host_id, e.guest_id,
                g.first_name AS guest_name, e.date_from, e.date_to,
                e.number_of_persons AS guests, e.status::text AS status, e.created_at,
                e.first_response_at, NULL::timestamptz AS expires_at, c.last_message_at
            FROM booking_enquiry e
            JOIN listing l ON l.id = e.listing_id
            JOIN "user" g ON g.id = e.guest_id
            JOIN conversation c ON c.id = e.conversation_id
            WHERE e.status = 'open'
            UNION ALL
            SELECT 'request'::text, r.id, r.conversation_id, r.listing_id, l.name, l.user_id,
                r.guest_id, g.first_name, r.date_from, r.date_to, r.number_of_persons,
                r.status::text, r.created_at, r.first_response_at, r.expires_at,
                c.last_message_at
            FROM booking_request r
            JOIN listing l ON l.id = r.listing_id
            JOIN "user" g ON g.id = r.guest_id
            JOIN conversation c ON c.id = r.conversation_id
            WHERE r.status IN ('open', 'accepted', 'quoted') AND r.expires_at > NOW()
        ) items
        WHERE ($1::uuid IS NULL OR host_id = $1)
          AND ($2::uuid IS NULL OR listing_id = $2)
          AND ($3::text IS NULL OR kind = $3)
        ORDER BY expires_at ASC NULLS LAST, last_message_at DESC
        LIMIT $4 OFFSET $5
        "#,
        filter.host_id,
        filter.listing_id,
        filter.kind,
        limit,
        offset
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| InboxRow {
            kind: r.kind,
            id: r.id,
            conversation_id: r.conversation_id,
            listing_id: r.listing_id,
            listing_name: r.listing_name,
            host_id: r.host_id,
            guest_id: r.guest_id,
            guest_name: r.guest_name,
            date_from: r.date_from,
            date_to: r.date_to,
            guests: r.guests,
            status: r.status,
            created_at: r.created_at,
            first_response_at: r.first_response_at,
            expires_at: r.expires_at,
            last_message_at: r.last_message_at,
        })
        .collect())
}

/// A host's response times over the last 90 days.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostMetrics {
    /// The host.
    pub host_id: Uuid,
    /// The host's first name.
    pub host_name: String,
    /// Median time to first response, in whole seconds, over items that got
    /// one; `None` if none did.
    pub median_first_response_secs: Option<i64>,
    /// Enquiries and requests received.
    pub received: i64,
    /// Requests received.
    pub requests: i64,
    /// Requests that expired with no response.
    pub expired_unanswered: i64,
}

/// Response-time metrics per host over the last 90 days (one host, or all).
#[tracing::instrument(skip(pool))]
pub async fn host_response_metrics(
    pool: &PgPool,
    host_id: Option<Uuid>,
) -> Result<Vec<HostMetrics>> {
    let rows = sqlx::query!(
        r#"
        WITH items AS (
            SELECT l.user_id AS host_id, e.created_at, e.first_response_at,
                FALSE AS is_request, FALSE AS expired_unanswered
            FROM booking_enquiry e JOIN listing l ON l.id = e.listing_id
            WHERE e.created_at > NOW() - INTERVAL '90 days'
            UNION ALL
            SELECT l.user_id, r.created_at, r.first_response_at, TRUE,
                (r.status = 'expired' AND r.first_response_at IS NULL)
            FROM booking_request r JOIN listing l ON l.id = r.listing_id
            WHERE r.created_at > NOW() - INTERVAL '90 days' AND r.origin = 'guest_request'
        )
        SELECT i.host_id AS "host_id!", h.first_name AS "host_name!",
            (percentile_cont(0.5) WITHIN GROUP (
                ORDER BY EXTRACT(EPOCH FROM (i.first_response_at - i.created_at))
            ) FILTER (WHERE i.first_response_at IS NOT NULL))::int8 AS median_secs,
            COUNT(*) AS "received!",
            COUNT(*) FILTER (WHERE i.is_request) AS "requests!",
            COUNT(*) FILTER (WHERE i.expired_unanswered) AS "expired_unanswered!"
        FROM items i JOIN "user" h ON h.id = i.host_id
        WHERE ($1::uuid IS NULL OR i.host_id = $1)
        GROUP BY i.host_id, h.first_name
        ORDER BY h.first_name
        "#,
        host_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| HostMetrics {
            host_id: r.host_id,
            host_name: r.host_name,
            median_first_response_secs: r.median_secs,
            received: r.received,
            requests: r.requests,
            expired_unanswered: r.expired_unanswered,
        })
        .collect())
}
