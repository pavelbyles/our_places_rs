//! Submitting a request to book.

use super::history::{Actor, HistorySubject, record_history};
use crate::booking::record_booking_history;
use crate::error::{DbError, Result, SQLSTATE_UNIQUE_VIOLATION, sqlstate};
use crate::models::{
    Booking, BookingMetadata, BookingStatus, CancellationPolicy, DbBookingRequest,
    DbMessageSenderRole, FeeItem,
};
use chrono::{DateTime, NaiveDate, Utc};
use common::booking_request::{
    BookingMode, DeclineReason, RequestOrigin, RequestRefusal, RequestStatus, StayPriceSnapshot,
};
use sqlx::types::Json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// A request to book, priced and ready to store.
#[derive(Debug, Clone)]
pub struct NewRequest {
    /// The listing requested.
    pub listing_id: Uuid,
    /// The signed-in guest.
    pub guest_id: Uuid,
    /// The guest's enquiry it was prefilled from, if any.
    pub enquiry_id: Option<Uuid>,
    /// Check-in date.
    pub date_from: NaiveDate,
    /// Check-out date.
    pub date_to: NaiveDate,
    /// Total guests.
    pub number_of_persons: i32,
    /// The price as requested.
    pub snapshot: StayPriceSnapshot,
    /// Guest breakdown and arrival details.
    pub metadata: BookingMetadata,
    /// End of the host's response window (already capped at check-in).
    pub expires_at: DateTime<Utc>,
}

/// A stored request and what its creation changed.
#[derive(Debug, Clone)]
pub struct CreatedRequest {
    /// The new request.
    pub request: DbBookingRequest,
    /// The guest's own `pending` holds on the listing that it replaced.
    pub superseded_holds: Vec<Uuid>,
    /// The enquiry it closed, if it came from one.
    pub closed_enquiry: Option<Uuid>,
}

/// Stores a request to book, blocking its dates until `expires_at`.
///
/// One transaction, under the listing lock: checks the listing is active and
/// takes requests; expires this guest's lapsed requests on the listing (so the
/// one-live-request index can't trip on one no sweep has reached); refuses a
/// second live request; checks the dates against confirmed stays, other
/// guests' live holds and live requests, widened by the listing's gap; then
/// cancels the guest's own pending holds on the listing, checks the promo
/// code's usage, and inserts the request with its history.
#[tracing::instrument(
    skip(pool, new),
    fields(listing_id = %new.listing_id, guest_id = %new.guest_id)
)]
pub async fn create_request(pool: &PgPool, new: &NewRequest) -> Result<CreatedRequest> {
    let mut tx = pool.begin().await?;
    let created = insert_request(&mut tx, new, RequestOrigin::GuestRequest).await?;
    tx.commit().await.map_err(map_live_violation)?;
    Ok(created)
}

/// The listing facts a request needs, read under its row lock.
pub(super) struct LockedListing {
    pub(super) gap: i32,
}

/// Locks the listing row and checks it is active, takes requests, and isn't
/// the guest's own.
pub(super) async fn lock_listing_for_request(
    conn: &mut PgConnection,
    listing_id: Uuid,
    guest_id: Uuid,
    check_mode: bool,
) -> Result<LockedListing> {
    let listing = sqlx::query!(
        r#"SELECT is_active, user_id, booking_mode AS "booking_mode: BookingMode",
            LEAST(GREATEST(days_between_bookings, 0), 365) AS "gap!"
        FROM listing WHERE id = $1 FOR UPDATE"#,
        listing_id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;
    if listing.user_id == guest_id {
        return Err(DbError::RequestRefused(RequestRefusal::OwnListing));
    }
    if !listing.is_active {
        return Err(DbError::ListingUnavailable);
    }
    if check_mode && !listing.booking_mode.allows_request() {
        return Err(DbError::RequestRefused(
            RequestRefusal::BookingModeNotAllowed,
        ));
    }
    Ok(LockedListing { gap: listing.gap })
}

/// Expires this guest's live requests on the listing whose window has
/// passed, with history, so a new one can take their place.
pub(super) async fn expire_lapsed_for_guest(
    conn: &mut PgConnection,
    listing_id: Uuid,
    guest_id: Uuid,
) -> Result<()> {
    let lapsed = sqlx::query!(
        r#"
        WITH due AS (
            SELECT id, status::text AS from_status FROM booking_request
            WHERE listing_id = $1 AND guest_id = $2
              AND status IN ('open', 'accepted', 'quoted')
              AND expires_at <= clock_timestamp()
            FOR UPDATE
        )
        UPDATE booking_request r
        SET status = 'expired', updated_at = NOW()
        FROM due
        WHERE r.id = due.id
        RETURNING r.id, due.from_status AS "from_status!"
        "#,
        listing_id,
        guest_id
    )
    .fetch_all(&mut *conn)
    .await?;
    for row in lapsed {
        record_history(
            conn,
            HistorySubject::Request(row.id),
            Some(&row.from_status),
            RequestStatus::Expired.as_str(),
            None,
            None,
            "Expired before a new request",
        )
        .await?;
    }
    Ok(())
}

/// Refuses a second live request from the guest on the listing.
pub(super) async fn refuse_if_live(
    conn: &mut PgConnection,
    listing_id: Uuid,
    guest_id: Uuid,
) -> Result<()> {
    let live = sqlx::query_scalar!(
        r#"SELECT EXISTS (
            SELECT 1 FROM booking_request
            WHERE listing_id = $1 AND guest_id = $2
              AND status IN ('open', 'accepted', 'quoted')
        ) AS "live!""#,
        listing_id,
        guest_id
    )
    .fetch_one(&mut *conn)
    .await?;
    if live {
        Err(DbError::RequestRefused(RequestRefusal::RequestAlreadyLive))
    } else {
        Ok(())
    }
}

/// Checks the stay is free for `guest_id`: no confirmed stay, no other
/// guest's live hold and no other live request overlaps it, all widened by
/// the gap. The guest's own live holds don't count; callers cancel them.
pub(super) async fn check_dates_free(
    conn: &mut PgConnection,
    listing_id: Uuid,
    guest_id: Uuid,
    date_from: NaiveDate,
    date_to: NaiveDate,
    gap: i32,
    exclude_request: Option<Uuid>,
) -> Result<()> {
    let blocked = sqlx::query_scalar!(
        r#"SELECT (
            EXISTS (
                SELECT 1 FROM booking b
                WHERE b.listing_id = $1
                  AND (b.status = 'confirmed'
                       OR (b.status = 'pending' AND b.expires_at > NOW() AND b.guest_id <> $2))
                  AND b.date_from - $5::int4 < $4
                  AND b.date_to + $5::int4 > $3
            )
            OR EXISTS (
                SELECT 1 FROM booking_request r
                WHERE r.listing_id = $1
                  AND r.status IN ('open', 'accepted', 'quoted') AND r.expires_at > NOW()
                  AND r.id IS DISTINCT FROM $6
                  AND r.date_from - $5::int4 < $4
                  AND r.date_to + $5::int4 > $3
            )
        ) AS "blocked!""#,
        listing_id,
        guest_id,
        date_from,
        date_to,
        gap,
        exclude_request
    )
    .fetch_one(&mut *conn)
    .await?;
    if blocked {
        Err(DbError::DateUnavailable)
    } else {
        Ok(())
    }
}

/// Cancels the guest's own `pending` holds on the listing, with history.
pub(super) async fn cancel_own_holds_on_listing(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    listing_id: Uuid,
    guest_id: Uuid,
    reason: &str,
) -> Result<Vec<Uuid>> {
    let cancelled = sqlx::query_as!(
        Booking,
        r#"
        UPDATE booking
        SET status = 'cancelled', updated_at = NOW()
        WHERE guest_id = $1 AND listing_id = $2 AND status = 'pending'
        RETURNING id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus",
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy",
            metadata as "metadata: Json<crate::models::BookingMetadata>",
            door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            promo_code_id, promo_terms as "promo_terms: Json<common::promo::PromoTerms>",
            promo_stay_discount, promo_cleaning_discount,
            created_at, updated_at
        "#,
        guest_id,
        listing_id
    )
    .fetch_all(&mut **tx)
    .await?;
    for booking in &cancelled {
        record_booking_history(tx, booking, reason, Some(guest_id)).await?;
    }
    Ok(cancelled.into_iter().map(|b| b.id).collect())
}

/// The guest × listing conversation, created on first use.
pub(crate) async fn upsert_conversation(
    conn: &mut PgConnection,
    listing_id: Uuid,
    guest_id: Uuid,
) -> Result<Uuid> {
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO conversation (listing_id, guest_id) VALUES ($1, $2)
        ON CONFLICT (listing_id, guest_id) DO UPDATE SET listing_id = EXCLUDED.listing_id
        RETURNING id
        "#,
        listing_id,
        guest_id
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(id)
}

/// Inserts a request row (status `open`, or `quoted` for a quote on an
/// enquiry, set by the caller afterwards) and returns it.
async fn insert_row(
    conn: &mut PgConnection,
    new: &NewRequest,
    conversation_id: Uuid,
    origin: RequestOrigin,
) -> Result<DbBookingRequest> {
    let promo_code_id = new.snapshot.promo.as_ref().map(|p| p.promo_code_id);
    let request = sqlx::query_as!(
        DbBookingRequest,
        r#"
        INSERT INTO booking_request (
            conversation_id, listing_id, guest_id, enquiry_id, origin, status,
            date_from, date_to, number_of_persons, currency, price_snapshot, metadata,
            promo_code_id, expires_at
        )
        VALUES ($1, $2, $3, $4, $5, 'open', $6, $7, $8, $9, $10, $11, $12, $13)
        RETURNING id, conversation_id, listing_id, guest_id, enquiry_id,
            origin AS "origin: RequestOrigin", status AS "status: RequestStatus",
            date_from, date_to, number_of_persons, currency,
            price_snapshot AS "price_snapshot: Json<StayPriceSnapshot>",
            metadata AS "metadata: Json<BookingMetadata>",
            promo_code_id, current_quote_id, expires_at, first_response_at,
            decline_reason AS "decline_reason: DeclineReason", decline_note, booking_id,
            created_at, updated_at
        "#,
        conversation_id,
        new.listing_id,
        new.guest_id,
        new.enquiry_id,
        origin as RequestOrigin,
        new.date_from,
        new.date_to,
        new.number_of_persons,
        new.snapshot.currency,
        Json(&new.snapshot) as _,
        Json(&new.metadata) as _,
        promo_code_id,
        new.expires_at
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(request)
}

/// Checks `enquiry_id` is the guest's own enquiry on the listing, and closes it
/// with `reason` if it is still open. Returns its id when it was closed.
pub(super) async fn close_source_enquiry(
    conn: &mut PgConnection,
    enquiry_id: Uuid,
    listing_id: Uuid,
    guest_id: Uuid,
    reason: common::booking_request::EnquiryClosedReason,
    actor: Option<Actor>,
) -> Result<Option<Uuid>> {
    let owned = sqlx::query_scalar!(
        r#"SELECT status AS "status: common::booking_request::EnquiryStatus"
        FROM booking_enquiry WHERE id = $1 AND listing_id = $2 AND guest_id = $3 FOR UPDATE"#,
        enquiry_id,
        listing_id,
        guest_id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(DbError::RequestRefused(RequestRefusal::NotParty))?;
    if owned != common::booking_request::EnquiryStatus::Open {
        return Ok(None);
    }
    sqlx::query!(
        r#"UPDATE booking_enquiry
        SET status = 'closed', closed_reason = $2, updated_at = NOW()
        WHERE id = $1"#,
        enquiry_id,
        reason as common::booking_request::EnquiryClosedReason
    )
    .execute(&mut *conn)
    .await?;
    record_history(
        conn,
        HistorySubject::Enquiry(enquiry_id),
        Some(common::booking_request::EnquiryStatus::Open.as_str()),
        common::booking_request::EnquiryStatus::Closed.as_str(),
        None,
        actor,
        reason.as_str(),
    )
    .await?;
    Ok(Some(enquiry_id))
}

/// Maps a commit-time or insert-time hit on the one-live-request index to its
/// typed refusal.
pub(super) fn map_live_violation(e: sqlx::Error) -> DbError {
    let live_pair = e
        .as_database_error()
        .and_then(|db| db.constraint())
        .is_some_and(|c| c == "uq_request_live_pair");
    if live_pair && sqlstate(&e).as_deref() == Some(SQLSTATE_UNIQUE_VIOLATION) {
        DbError::RequestRefused(RequestRefusal::RequestAlreadyLive)
    } else {
        DbError::Sqlx(e)
    }
}

/// The body of [`create_request`], inside the caller's transaction.
async fn insert_request(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    new: &NewRequest,
    origin: RequestOrigin,
) -> Result<CreatedRequest> {
    let listing = lock_listing_for_request(tx, new.listing_id, new.guest_id, true).await?;
    expire_lapsed_for_guest(tx, new.listing_id, new.guest_id).await?;
    refuse_if_live(tx, new.listing_id, new.guest_id).await?;
    crate::currency::ensure_active(tx, &new.snapshot.currency).await?;
    check_dates_free(
        tx,
        new.listing_id,
        new.guest_id,
        new.date_from,
        new.date_to,
        listing.gap,
        None,
    )
    .await?;
    let superseded_holds = cancel_own_holds_on_listing(
        tx,
        new.listing_id,
        new.guest_id,
        "Cancelled pending hold: superseded by a request to book",
    )
    .await?;

    let conversation_id = upsert_conversation(tx, new.listing_id, new.guest_id).await?;
    let request = insert_row(tx, new, conversation_id, origin)
        .await
        .map_err(|e| match e {
            DbError::Sqlx(e) => map_live_violation(e),
            other => other,
        })?;
    let guest = Some(Actor {
        id: new.guest_id,
        role: DbMessageSenderRole::Guest,
    });
    record_history(
        tx,
        HistorySubject::Request(request.id),
        None,
        RequestStatus::Open.as_str(),
        None,
        guest,
        "Request submitted",
    )
    .await?;

    let closed_enquiry = match new.enquiry_id {
        Some(enquiry_id) => {
            close_source_enquiry(
                tx,
                enquiry_id,
                new.listing_id,
                new.guest_id,
                common::booking_request::EnquiryClosedReason::RequestSubmitted,
                guest,
            )
            .await?
        }
        None => None,
    };

    Ok(CreatedRequest {
        request,
        superseded_holds,
        closed_enquiry,
    })
}
