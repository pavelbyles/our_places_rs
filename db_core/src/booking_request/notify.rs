//! What request and enquiry emails need, loaded in one query, and the claim
//! that keeps scheduled emails from being sent twice.

use crate::error::Result;
use chrono::{DateTime, NaiveDate, Utc};
use common::booking_request::DeclineReason;
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

/// Who to tell and what about. Contains user text (`decline_note`, `terms`)
/// and email addresses: never log it.
#[derive(Debug, Clone)]
pub struct NotificationFacts {
    /// The request, for request emails.
    pub request_id: Option<Uuid>,
    /// The enquiry, for enquiry emails.
    pub enquiry_id: Option<Uuid>,
    /// The guest.
    pub guest_id: Uuid,
    /// The guest's email.
    pub guest_email: String,
    /// The guest's first name.
    pub guest_name: String,
    /// The listing's host.
    pub host_id: Uuid,
    /// The host's email.
    pub host_email: String,
    /// The host's first name.
    pub host_name: String,
    /// The listing's name.
    pub listing_name: String,
    /// Check-in.
    pub date_from: NaiveDate,
    /// Check-out.
    pub date_to: NaiveDate,
    /// Total guests.
    pub guests: i32,
    /// The total's currency, if priced.
    pub currency: Option<String>,
    /// The current total (the live quote's, otherwise as requested).
    pub total: Option<Decimal>,
    /// When the request or offer stops holding its dates.
    pub expires_at: Option<DateTime<Utc>>,
    /// Why the host declined.
    pub decline_reason: Option<DeclineReason>,
    /// The host's note with a decline.
    pub decline_note: Option<String>,
    /// The live quote's terms.
    pub terms: Option<String>,
    /// Whether a promo code was dropped because the host quoted.
    pub promo_dropped: bool,
}

/// The facts for a request's emails.
pub async fn request_facts(pool: &PgPool, request_id: Uuid) -> Result<NotificationFacts> {
    let r = sqlx::query!(
        r#"
        SELECT r.id, r.guest_id, g.email AS guest_email, g.first_name AS guest_name,
            l.user_id AS host_id, h.email AS host_email, h.first_name AS host_name,
            l.name AS listing_name, r.date_from, r.date_to, r.number_of_persons,
            COALESCE(q.price_snapshot ->> 'currency', r.price_snapshot ->> 'currency') AS currency,
            COALESCE(q.price_snapshot ->> 'total_price', r.price_snapshot ->> 'total_price') AS total,
            r.expires_at, r.decline_reason AS "decline_reason: DeclineReason", r.decline_note,
            q.terms AS "terms?",
            (r.status = 'quoted' AND r.promo_code_id IS NOT NULL) AS "promo_dropped!"
        FROM booking_request r
        JOIN listing l ON l.id = r.listing_id
        JOIN "user" g ON g.id = r.guest_id
        JOIN "user" h ON h.id = l.user_id
        LEFT JOIN booking_request_quote q ON q.id = r.current_quote_id
        WHERE r.id = $1
        "#,
        request_id
    )
    .fetch_one(pool)
    .await?;
    Ok(NotificationFacts {
        request_id: Some(r.id),
        enquiry_id: None,
        guest_id: r.guest_id,
        guest_email: r.guest_email,
        guest_name: r.guest_name,
        host_id: r.host_id,
        host_email: r.host_email,
        host_name: r.host_name,
        listing_name: r.listing_name,
        date_from: r.date_from,
        date_to: r.date_to,
        guests: r.number_of_persons,
        currency: r.currency,
        total: r.total.and_then(|t| t.parse().ok()),
        expires_at: Some(r.expires_at),
        decline_reason: r.decline_reason,
        decline_note: r.decline_note,
        terms: r.terms.filter(|t| !t.is_empty()),
        promo_dropped: r.promo_dropped,
    })
}

/// The facts for an enquiry's emails.
pub async fn enquiry_facts(pool: &PgPool, enquiry_id: Uuid) -> Result<NotificationFacts> {
    let e = sqlx::query!(
        r#"
        SELECT e.id, e.guest_id, g.email AS guest_email, g.first_name AS guest_name,
            l.user_id AS host_id, h.email AS host_email, h.first_name AS host_name,
            l.name AS listing_name, e.date_from, e.date_to, e.number_of_persons
        FROM booking_enquiry e
        JOIN listing l ON l.id = e.listing_id
        JOIN "user" g ON g.id = e.guest_id
        JOIN "user" h ON h.id = l.user_id
        WHERE e.id = $1
        "#,
        enquiry_id
    )
    .fetch_one(pool)
    .await?;
    Ok(NotificationFacts {
        request_id: None,
        enquiry_id: Some(e.id),
        guest_id: e.guest_id,
        guest_email: e.guest_email,
        guest_name: e.guest_name,
        host_id: e.host_id,
        host_email: e.host_email,
        host_name: e.host_name,
        listing_name: e.listing_name,
        date_from: e.date_from,
        date_to: e.date_to,
        guests: e.number_of_persons,
        currency: None,
        total: None,
        expires_at: None,
        decline_reason: None,
        decline_note: None,
        terms: None,
        promo_dropped: false,
    })
}

/// Claims a scheduled request email for one recipient. Returns `false` if it
/// was already claimed, so a re-run sweep never sends it twice.
pub async fn claim_request_notification(
    pool: &PgPool,
    request_id: Uuid,
    notification_type: &str,
    recipient_user_id: Uuid,
) -> Result<bool> {
    let claimed = sqlx::query_scalar!(
        r#"
        INSERT INTO request_notification_log (request_id, notification_type, recipient_user_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (request_id, notification_type, recipient_user_id) DO NOTHING
        RETURNING id
        "#,
        request_id,
        notification_type,
        recipient_user_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(claimed.is_some())
}
