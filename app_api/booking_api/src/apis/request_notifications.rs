//! Emails for enquiries and requests to book (#105): one guest email and one
//! host email per event, through the outbox. Admin actions email the same
//! two parties. Scheduled emails (expiry) are claimed first, so a re-run
//! sweep never sends twice.

use super::{admin_base_url, enqueue_email, guest_base_url};
use common::email::EmailTemplate;
use db_core::booking_request::{self as db_request, NotificationFacts};
use sqlx::PgPool;
use uuid::Uuid;

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RequestEvent {
    /// A guest sent an enquiry.
    EnquirySent,
    /// A guest submitted a request.
    Submitted,
    /// The host accepted.
    Accepted,
    /// The host declined or withdrew the offer.
    Declined,
    /// The host sent a custom quote.
    Quoted,
    /// The guest took up the offer.
    Converted,
    /// The guest withdrew, or an instant booking replaced, the request.
    Withdrawn,
    /// The window passed.
    Expired,
    /// An unanswered request is about to expire (host only).
    Reminder,
}

impl RequestEvent {
    fn templates(self) -> (EmailTemplate, EmailTemplate) {
        use EmailTemplate as T;
        match self {
            Self::EnquirySent => (T::EnquirySentGuest, T::EnquiryReceivedHost),
            Self::Submitted => (T::RequestSubmittedGuest, T::RequestReceivedHost),
            Self::Accepted => (T::RequestAcceptedGuest, T::RequestAcceptedHost),
            Self::Declined => (T::RequestDeclinedGuest, T::RequestDeclinedHost),
            Self::Quoted => (T::QuoteSentGuest, T::QuoteSentHost),
            Self::Converted => (T::RequestConvertedGuest, T::RequestConvertedHost),
            Self::Withdrawn => (T::RequestWithdrawnGuest, T::RequestWithdrawnHost),
            Self::Expired => (T::RequestExpiredGuest, T::RequestExpiredHost),
            Self::Reminder => (T::RequestHostReminder, T::RequestHostReminder),
        }
    }

    fn subjects(self) -> (&'static str, &'static str) {
        match self {
            Self::EnquirySent => ("Your question was sent", "New enquiry"),
            Self::Submitted => ("Your request to book was sent", "New request to book"),
            Self::Accepted => ("Your request was accepted", "You accepted a request"),
            Self::Declined => ("Your request was declined", "Request declined"),
            Self::Quoted => ("You have a custom quote", "Quote sent"),
            Self::Converted => ("Complete your booking", "Your guest is checking out"),
            Self::Withdrawn => ("Request withdrawn", "Request withdrawn"),
            Self::Expired => ("Your request expired", "Request expired"),
            Self::Reminder => ("", "A request is about to expire"),
        }
    }
}

/// Which side an email is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    /// The guest.
    Guest,
    /// The listing's host.
    Host,
}

/// The payload for one side of an event. `message` is the guest's text for
/// enquiries and submissions.
fn payload(
    event: RequestEvent,
    side: Side,
    facts: &NotificationFacts,
    message: Option<&str>,
) -> serde_json::Value {
    let action_url = match (side, event, facts.request_id, facts.enquiry_id) {
        (Side::Guest, RequestEvent::Converted, _, _) => {
            format!("{}/bookings?new_booking=true", guest_base_url())
        }
        (Side::Guest, RequestEvent::Declined | RequestEvent::Expired, _, _) => {
            format!("{}/listings", guest_base_url())
        }
        (Side::Guest, _, Some(id), _) => format!("{}/requests/{id}", guest_base_url()),
        (Side::Guest, _, None, Some(id)) => {
            format!("{}/requests/enquiries/{id}", guest_base_url())
        }
        (Side::Host, RequestEvent::Expired | RequestEvent::Withdrawn, _, _) => {
            format!("{}/admin/inbox", admin_base_url())
        }
        (Side::Host, _, Some(id), _) => format!("{}/admin/requests/{id}", admin_base_url()),
        (Side::Host, _, None, Some(id)) => format!("{}/admin/enquiries/{id}", admin_base_url()),
        _ => String::default(),
    };
    let total = match (&facts.currency, facts.total) {
        (Some(currency), Some(total)) => common::money::total_amount(currency, total),
        _ => String::default(),
    };
    serde_json::json!({
        "recipient_name": match side { Side::Guest => &facts.guest_name, Side::Host => &facts.host_name },
        "listing_name": facts.listing_name,
        "guest_name": facts.guest_name,
        "date_from": facts.date_from.to_string(),
        "date_to": facts.date_to.to_string(),
        "guests": facts.guests.to_string(),
        "total": total,
        "expires_at": facts.expires_at.map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string()),
        "message_text": message,
        "terms": facts.terms.as_deref().filter(|_| event == RequestEvent::Quoted),
        "decline_reason": facts.decline_reason.filter(|_| event == RequestEvent::Declined).map(|r| r.label()),
        "decline_note": facts.decline_note.as_deref().filter(|_| event == RequestEvent::Declined),
        "promo_dropped": (event == RequestEvent::Quoted && facts.promo_dropped).to_string(),
        "action_url": action_url,
    })
}

/// Queues one side's email for an event.
pub(super) async fn send_side(
    pool: &PgPool,
    event: RequestEvent,
    side: Side,
    facts: &NotificationFacts,
    message: Option<&str>,
) -> bool {
    let (guest_template, host_template) = event.templates();
    let (guest_subject, host_subject) = event.subjects();
    let (recipient, template, subject) = match side {
        Side::Guest => (&facts.guest_email, guest_template, guest_subject),
        Side::Host => (&facts.host_email, host_template, host_subject),
    };
    enqueue_email(
        pool,
        recipient,
        subject,
        template.as_str(),
        &payload(event, side, facts, message),
    )
    .await
}

/// What an event is about.
#[derive(Debug, Clone, Copy)]
pub(super) enum Subject {
    /// A request to book.
    Request(Uuid),
    /// An enquiry.
    Enquiry(Uuid),
}

/// Emails both parties about an event, from a detached task so a slow outbox
/// never slows the response. Failures are logged with ids only.
pub(super) fn notify(
    pool: &PgPool,
    event: RequestEvent,
    subject: Subject,
    message: Option<String>,
) {
    let pool = pool.clone();
    tokio::spawn(async move {
        let facts = match subject {
            Subject::Request(id) => db_request::request_facts(&pool, id).await,
            Subject::Enquiry(id) => db_request::enquiry_facts(&pool, id).await,
        };
        let facts = match facts {
            Ok(facts) => facts,
            Err(e) => {
                tracing::error!(?subject, ?event, error = ?e, "request emails not sent: facts lookup failed");
                return;
            }
        };
        for side in [Side::Guest, Side::Host] {
            send_side(&pool, event, side, &facts, message.as_deref()).await;
        }
    });
}

/// Emails both parties that a request expired, once each: the sweep claims
/// each email before queueing it.
pub(super) async fn notify_expired(pool: &PgPool, request_id: Uuid) -> usize {
    let facts = match db_request::request_facts(pool, request_id).await {
        Ok(facts) => facts,
        Err(e) => {
            tracing::error!(%request_id, error = ?e, "expiry emails not sent: facts lookup failed");
            return 0;
        }
    };
    let mut sent = 0;
    for (side, recipient) in [(Side::Guest, facts.guest_id), (Side::Host, facts.host_id)] {
        match db_request::claim_request_notification(pool, request_id, "request_expired", recipient)
            .await
        {
            Ok(true) => {
                if send_side(pool, RequestEvent::Expired, side, &facts, None).await {
                    sent += 1;
                }
            }
            Ok(false) => {}
            Err(e) => {
                tracing::error!(%request_id, error = ?e, "expiry email not claimed");
            }
        }
    }
    sent
}

/// Reminds each host once about open requests in the near-expiry window.
pub(super) async fn send_host_reminders(pool: &PgPool, pct: i64) -> usize {
    let due = match db_request::requests_due_for_reminder(pool, pct).await {
        Ok(due) => due,
        Err(e) => {
            tracing::error!(error = ?e, "host reminders not sent: candidate lookup failed");
            return 0;
        }
    };
    let mut sent = 0;
    for (request_id, host_id) in due {
        match db_request::claim_request_notification(
            pool,
            request_id,
            "request_host_reminder",
            host_id,
        )
        .await
        {
            Ok(true) => match db_request::request_facts(pool, request_id).await {
                Ok(facts) => {
                    if send_side(pool, RequestEvent::Reminder, Side::Host, &facts, None).await {
                        sent += 1;
                    }
                }
                Err(e) => {
                    tracing::error!(%request_id, error = ?e, "host reminder not sent: facts lookup failed")
                }
            },
            Ok(false) => {}
            Err(e) => tracing::error!(%request_id, error = ?e, "host reminder not claimed"),
        }
    }
    sent
}
