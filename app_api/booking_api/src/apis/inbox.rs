//! The admin and host inbox (#105): open enquiries and live requests with
//! response-time tracking. Hosts see their own listings; admins see all.

use super::caller;
use actix_web::{HttpRequest, HttpResponse, web};
use api_core::auth::Claims;
use api_core::error::ApiError;
use api_core::response::{Payload, respond};
use api_core::settings::Settings;
use chrono::Utc;
use common::booking_request::{
    HostResponseMetrics, InboxItem, InboxQuery, InboxResponse, ThreadSubject, UnreadThread,
    is_near_expiry,
};
use db_core::booking_request as db_request;
use db_core::conversation as db_conversation;
use db_core::error::DbError;
use db_core::models::UserRole;
use sqlx::PgPool;

/// Items per inbox page.
const PER_PAGE: u32 = 50;

/// Most threads with unread guest messages listed at once.
const UNREAD_THREADS: i64 = 50;

fn unread_thread(t: db_conversation::UnreadThread) -> UnreadThread {
    UnreadThread {
        conversation_id: t.conversation_id,
        listing_id: t.listing_id,
        listing_name: t.listing_name,
        guest_name: t.guest_name,
        unread: t.unread,
        latest_at: t.latest_at,
        latest_text: t.latest_text,
        subject: t.subject.map(|s| match s {
            db_conversation::ThreadSubject::Booking {
                id,
                confirmation_code,
            } => ThreadSubject::Booking {
                id,
                confirmation_code,
            },
            db_conversation::ThreadSubject::Request(id) => ThreadSubject::Request { id },
            db_conversation::ThreadSubject::Enquiry(id) => ThreadSubject::Enquiry { id },
        }),
        check_in: t.dates.map(|(from, _)| from),
        check_out: t.dates.map(|(_, to)| to),
    }
}

fn item(row: db_request::InboxRow, now: chrono::DateTime<Utc>, pct: i64) -> InboxItem {
    InboxItem {
        age_secs: (now - row.created_at).num_seconds(),
        first_response_secs: row
            .first_response_at
            .map(|at| (at - row.created_at).num_seconds()),
        time_left_secs: row.expires_at.map(|at| (at - now).num_seconds().max(0)),
        near_expiry: row
            .expires_at
            .is_some_and(|at| is_near_expiry(row.created_at, at, now, pct)),
        kind: row.kind,
        id: row.id,
        conversation_id: row.conversation_id,
        listing_id: row.listing_id,
        listing_name: row.listing_name,
        guest_id: row.guest_id,
        guest_name: row.guest_name,
        check_in: row.date_from,
        check_out: row.date_to,
        guests: row.guests,
        status: row.status,
        created_at: row.created_at,
        expires_at: row.expires_at,
    }
}

fn metrics(m: db_request::HostMetrics) -> HostResponseMetrics {
    let pct = if m.requests > 0 {
        m.expired_unanswered * 100 / m.requests
    } else {
        0
    };
    HostResponseMetrics {
        host_id: m.host_id,
        host_name: m.host_name,
        median_first_response_secs: m.median_first_response_secs,
        received: m.received,
        requests: m.requests,
        expired_unanswered: m.expired_unanswered,
        expired_unanswered_pct: pct,
    }
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v1/inbox",
    tag = "booking-requests",
    params(InboxQuery),
    responses(
        (status = 200, description = "Unread guest threads, open enquiries and live requests", body = InboxResponse),
        (status = 400, description = "Unknown kind"),
        (status = 403, description = "Hosts and admins only")
    )
)]
/// The inbox: a host's own listings, or every listing for an admin.
pub(super) async fn get_inbox(
    req: HttpRequest,
    claims: Claims,
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    query: web::Query<InboxQuery>,
) -> Result<HttpResponse, ApiError> {
    let user = caller(pool.get_ref(), &claims).await?;
    let is_admin = user.roles.contains(&UserRole::Admin);
    if !is_admin && !user.roles.contains(&UserRole::Host) {
        return Err(ApiError::Forbidden(
            "Only hosts and admins have an inbox".to_string(),
        ));
    }
    let query = query.into_inner();
    if let Some(kind) = query.kind.as_deref()
        && kind != "enquiry"
        && kind != "request"
    {
        return Err(ApiError::Database(DbError::ValidationError(
            "kind must be enquiry or request".to_string(),
        )));
    }
    let page = query.page.unwrap_or(1).max(1);
    let host_id = (!is_admin).then_some(claims.sub);
    let filter = db_request::InboxFilter {
        host_id,
        listing_id: query.listing_id,
        kind: query.kind,
    };
    let rows = db_request::inbox_items(
        pool.get_ref(),
        &filter,
        i64::from(PER_PAGE),
        i64::from(page - 1) * i64::from(PER_PAGE),
    )
    .await
    .map_err(ApiError::Database)?;
    let host_metrics = db_request::host_response_metrics(pool.get_ref(), host_id)
        .await
        .map_err(ApiError::Database)?;
    let unread_threads = db_conversation::unread_threads(pool.get_ref(), host_id, UNREAD_THREADS)
        .await
        .map_err(|e| {
            tracing::error!(user_id = %claims.sub, error = ?e, "failed to list unread threads");
            ApiError::Database(e)
        })?;
    let now = Utc::now();
    let pct = settings.application.request_near_expiry_pct;
    Ok(respond(
        &req,
        Payload::Item(InboxResponse {
            unread_threads: unread_threads.into_iter().map(unread_thread).collect(),
            items: rows.into_iter().map(|r| item(r, now, pct)).collect(),
            host_metrics: host_metrics.into_iter().map(metrics).collect(),
            page,
            per_page: PER_PAGE,
        }),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}
