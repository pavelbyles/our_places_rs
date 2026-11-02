//! Bilateral reservation change requests and lifecycle operations.

use chrono::{NaiveDate, Utc};
use sqlx::{PgExecutor, PgPool, types::Json};
use uuid::Uuid;

use crate::error::{DbError, Result};
use crate::models::{
    Booking, BookingStatus, CancellationPolicy, ChangeInitiatorRole, ChangeSettlementStatus,
    DbPayoutStatus, FeeItem, NewReservationChangeRequest, ReservationChangePricingSnapshot,
    ReservationChangeRequestRow, ReservationChangeStatus, SupplierInfo,
};

/// Refuses a change quoted as keeping a promo when the booking no longer has it,
/// as an admin's direct edit can remove it between the quote and the approval.
pub fn check_promo_still_attached(
    snapshot: &ReservationChangePricingSnapshot,
    promo_code_id: Option<Uuid>,
) -> Result<()> {
    if snapshot.promo_retained && promo_code_id.is_none() {
        return Err(DbError::BookingNotModifiable(
            "This booking's promo code was removed after the change was quoted. Please request the change again."
                .to_string(),
        ));
    }
    Ok(())
}

/// Host payout context for a booking used when computing a reservation change quote.
#[derive(Debug, Clone)]
pub struct BookingPayoutContext {
    /// Whether the booking's host payout has already been processed or paid.
    pub is_payout_processed: bool,
    /// Exchange rate from the booking currency to the host's default currency.
    pub host_exchange_rate: rust_decimal::Decimal,
    /// Existing pending net payout amount if a `host_payout_ledger` row exists.
    pub existing_net_payout: Option<rust_decimal::Decimal>,
    /// Commission recorded on the `host_payout_ledger` row if one exists. Approval
    /// uses this percentage, so the quote must too.
    pub existing_commission_pct: Option<rust_decimal::Decimal>,
}

/// Retrieves the host payout context (status, exchange rate, and current net payout) for a booking.
#[tracing::instrument(skip(pool))]
pub async fn get_booking_payout_context(
    pool: &PgPool,
    booking_id: Uuid,
    host_id: Uuid,
    booking_currency: &str,
) -> Result<BookingPayoutContext> {
    let existing = sqlx::query!(
        r#"
        SELECT status as "status: DbPayoutStatus", exchange_rate, net_payout_amount, platform_fee_pct
        FROM host_payout_ledger
        WHERE booking_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        booking_id
    )
    .fetch_optional(pool)
    .await?;

    if let Some(row) = existing {
        return Ok(BookingPayoutContext {
            is_payout_processed: matches!(
                row.status,
                DbPayoutStatus::Processing | DbPayoutStatus::Paid
            ),
            host_exchange_rate: row.exchange_rate,
            existing_net_payout: Some(row.net_payout_amount),
            existing_commission_pct: Some(row.platform_fee_pct),
        });
    }

    let host_currency = sqlx::query_scalar!(
        r#"SELECT default_currency FROM "user" WHERE id = $1"#,
        host_id
    )
    .fetch_one(pool)
    .await?;

    let host_exchange_rate = crate::exchange_rate::get_rate_pool(
        pool,
        booking_currency,
        &host_currency,
        Utc::now(),
        crate::exchange_rate::RateUse::Settlement,
    )
    .await
    .inspect_err(|e| {
        tracing::error!(
            %booking_id,
            from = booking_currency,
            to = %host_currency,
            error = %e,
            "host payout currency conversion failed"
        )
    })?;

    Ok(BookingPayoutContext {
        is_payout_processed: false,
        host_exchange_rate,
        existing_net_payout: None,
        existing_commission_pct: None,
    })
}

/// Transitions any `pending` reservation change requests whose `expires_at_date <= today_jm`
/// to `expired`. Optionally scopes the sweep to a single `booking_id`.
#[tracing::instrument(skip(executor))]
pub async fn expire_past_due_change_requests<'e, E>(
    executor: E,
    booking_id: Option<Uuid>,
    today_jm: NaiveDate,
) -> Result<u64>
where
    E: PgExecutor<'e>,
{
    let res = sqlx::query!(
        r#"
        UPDATE reservation_change_request
        SET status = 'expired'::reservation_change_status,
            updated_at = NOW()
        WHERE status = 'pending'::reservation_change_status
          AND expires_at_date <= $1
          AND ($2::uuid IS NULL OR booking_id = $2)
        "#,
        today_jm,
        booking_id
    )
    .execute(executor)
    .await?;

    Ok(res.rows_affected())
}

/// Retrieves a single `reservation_change_request` row by its primary key.
#[tracing::instrument(skip(executor))]
pub async fn get_change_request_by_id<'e, E>(
    executor: E,
    id: Uuid,
) -> Result<Option<ReservationChangeRequestRow>>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query_as!(
        ReservationChangeRequestRow,
        r#"
        SELECT
            id,
            booking_id,
            listing_id,
            initiated_by_id,
            initiated_by_role as "initiated_by_role: ChangeInitiatorRole",
            status as "status: ReservationChangeStatus",
            previous_date_from,
            previous_date_to,
            proposed_date_from,
            proposed_date_to,
            previous_number_of_persons,
            proposed_number_of_persons,
            proposed_num_adults,
            proposed_num_children,
            proposed_num_infants,
            proposed_num_pets,
            currency,
            previous_total_price,
            proposed_total_price,
            balance_due,
            previous_net_payout,
            proposed_net_payout,
            host_payout_delta,
            pricing_snapshot as "pricing_snapshot: Json<ReservationChangePricingSnapshot>",
            settlement_status as "settlement_status: ChangeSettlementStatus",
            gateway_reference,
            reason,
            responder_id,
            response_note,
            responded_at,
            expires_at_date,
            created_at,
            updated_at
        FROM reservation_change_request
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(executor)
    .await?;

    Ok(row)
}

/// Lists all reservation change requests for a booking (most recent first),
/// lazily expiring any past-due `pending` requests first.
#[tracing::instrument(skip(pool))]
pub async fn list_change_requests_for_booking(
    pool: &PgPool,
    booking_id: Uuid,
    today_jm: NaiveDate,
) -> Result<Vec<ReservationChangeRequestRow>> {
    expire_past_due_change_requests(pool, Some(booking_id), today_jm).await?;

    let rows = sqlx::query_as!(
        ReservationChangeRequestRow,
        r#"
        SELECT
            id,
            booking_id,
            listing_id,
            initiated_by_id,
            initiated_by_role as "initiated_by_role: ChangeInitiatorRole",
            status as "status: ReservationChangeStatus",
            previous_date_from,
            previous_date_to,
            proposed_date_from,
            proposed_date_to,
            previous_number_of_persons,
            proposed_number_of_persons,
            proposed_num_adults,
            proposed_num_children,
            proposed_num_infants,
            proposed_num_pets,
            currency,
            previous_total_price,
            proposed_total_price,
            balance_due,
            previous_net_payout,
            proposed_net_payout,
            host_payout_delta,
            pricing_snapshot as "pricing_snapshot: Json<ReservationChangePricingSnapshot>",
            settlement_status as "settlement_status: ChangeSettlementStatus",
            gateway_reference,
            reason,
            responder_id,
            response_note,
            responded_at,
            expires_at_date,
            created_at,
            updated_at
        FROM reservation_change_request
        WHERE booking_id = $1
        ORDER BY created_at DESC
        "#,
        booking_id
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Creates a new `pending` reservation change request after acquiring row locks
/// (`listing` -> `booking`), lazily expiring past-due pending requests, verifying
/// that the booking is `Confirmed` and its host payout is not yet processed, and
/// checking date availability.
#[tracing::instrument(skip(pool, req))]
pub async fn create_change_request(
    pool: &PgPool,
    req: &NewReservationChangeRequest,
    today_jm: NaiveDate,
) -> Result<ReservationChangeRequestRow> {
    let mut tx = pool.begin().await?;

    // Global lock ordering: 1. listing -> 2. booking
    sqlx::query!(
        "SELECT id FROM listing WHERE id = $1 FOR UPDATE",
        req.listing_id
    )
    .fetch_one(&mut *tx)
    .await?;

    let booking_row = sqlx::query!(
        r#"SELECT id, status as "status: BookingStatus", date_from, date_to FROM booking WHERE id = $1 FOR UPDATE"#,
        req.booking_id
    )
    .fetch_one(&mut *tx)
    .await?;

    if booking_row.status != BookingStatus::Confirmed {
        return Err(DbError::BookingNotModifiable(
            "Only confirmed bookings can have their reservation terms modified".to_string(),
        ));
    }

    // Lazily expire any past-due pending requests on this booking before checking uniqueness
    expire_past_due_change_requests(&mut *tx, Some(req.booking_id), today_jm).await?;

    // Reject if host payout has already been processed or paid
    let processed_payout = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)
        FROM host_payout_ledger
        WHERE booking_id = $1
          AND status IN ('processing'::payout_status, 'paid'::payout_status)
        "#,
        req.booking_id
    )
    .fetch_one(&mut *tx)
    .await?
    .unwrap_or(0);

    if processed_payout > 0 {
        return Err(DbError::BookingNotModifiable(
            "Reservation cannot be modified after host payout has been processed".to_string(),
        ));
    }

    let dates_changed = req.proposed_date_from != booking_row.date_from
        || req.proposed_date_to != booking_row.date_to;
    if dates_changed {
        let available = crate::booking::check_availability_excluding(
            &mut *tx,
            req.listing_id,
            req.proposed_date_from,
            req.proposed_date_to,
            req.booking_id,
        )
        .await?;

        if !available {
            return Err(DbError::DateUnavailable);
        }
    }

    let id = Uuid::now_v7();
    let insert_res = sqlx::query_as!(
        ReservationChangeRequestRow,
        r#"
        INSERT INTO reservation_change_request (
            id,
            booking_id,
            listing_id,
            initiated_by_id,
            initiated_by_role,
            status,
            previous_date_from,
            previous_date_to,
            proposed_date_from,
            proposed_date_to,
            previous_number_of_persons,
            proposed_number_of_persons,
            proposed_num_adults,
            proposed_num_children,
            proposed_num_infants,
            proposed_num_pets,
            currency,
            previous_total_price,
            proposed_total_price,
            balance_due,
            previous_net_payout,
            proposed_net_payout,
            host_payout_delta,
            pricing_snapshot,
            settlement_status,
            reason,
            expires_at_date
        )
        VALUES (
            $1, $2, $3, $4, $5, 'pending'::reservation_change_status,
            $6, $7, $8, $9,
            $10, $11, $12, $13, $14, $15,
            $16, $17, $18, $19,
            $20, $21, $22,
            $23, $24, $25, $26
        )
        RETURNING
            id,
            booking_id,
            listing_id,
            initiated_by_id,
            initiated_by_role as "initiated_by_role: ChangeInitiatorRole",
            status as "status: ReservationChangeStatus",
            previous_date_from,
            previous_date_to,
            proposed_date_from,
            proposed_date_to,
            previous_number_of_persons,
            proposed_number_of_persons,
            proposed_num_adults,
            proposed_num_children,
            proposed_num_infants,
            proposed_num_pets,
            currency,
            previous_total_price,
            proposed_total_price,
            balance_due,
            previous_net_payout,
            proposed_net_payout,
            host_payout_delta,
            pricing_snapshot as "pricing_snapshot: Json<ReservationChangePricingSnapshot>",
            settlement_status as "settlement_status: ChangeSettlementStatus",
            gateway_reference,
            reason,
            responder_id,
            response_note,
            responded_at,
            expires_at_date,
            created_at,
            updated_at
        "#,
        id,
        req.booking_id,
        req.listing_id,
        req.initiated_by_id,
        req.initiated_by_role as ChangeInitiatorRole,
        req.previous_date_from,
        req.previous_date_to,
        req.proposed_date_from,
        req.proposed_date_to,
        req.previous_number_of_persons,
        req.proposed_number_of_persons,
        req.proposed_num_adults,
        req.proposed_num_children,
        req.proposed_num_infants,
        req.proposed_num_pets,
        req.currency,
        req.previous_total_price,
        req.proposed_total_price,
        req.balance_due,
        req.previous_net_payout,
        req.proposed_net_payout,
        req.host_payout_delta,
        Json(&req.pricing_snapshot) as _,
        req.settlement_status as ChangeSettlementStatus,
        req.reason.as_deref(),
        req.expires_at_date
    )
    .fetch_one(&mut *tx)
    .await;

    let row = match insert_res {
        Ok(r) => r,
        Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("23505") => {
            return Err(DbError::ChangeRequestAlreadyPending);
        }
        Err(e) => return Err(DbError::Sqlx(e)),
    };

    tx.commit().await?;
    Ok(row)
}

/// Declines a `pending` reservation change request on behalf of the counterparty.
#[tracing::instrument(skip(pool))]
pub async fn decline_change_request(
    pool: &PgPool,
    id: Uuid,
    responder_id: Uuid,
    response_note: Option<&str>,
    today_jm: NaiveDate,
) -> Result<ReservationChangeRequestRow> {
    let mut tx = pool.begin().await?;

    let existing = sqlx::query!(
        r#"
        SELECT id, booking_id, status as "status: ReservationChangeStatus", expires_at_date
        FROM reservation_change_request
        WHERE id = $1
        FOR UPDATE
        "#,
        id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;

    if existing.status == ReservationChangeStatus::Pending && existing.expires_at_date <= today_jm {
        expire_past_due_change_requests(&mut *tx, Some(existing.booking_id), today_jm).await?;
        tx.commit().await?;
        return Err(DbError::BookingNotModifiable(
            "Reservation change request has expired".to_string(),
        ));
    }

    if existing.status != ReservationChangeStatus::Pending {
        return Err(DbError::BookingNotModifiable(
            "Only pending reservation change requests can be declined".to_string(),
        ));
    }

    let row = sqlx::query_as!(
        ReservationChangeRequestRow,
        r#"
        UPDATE reservation_change_request
        SET status = 'declined'::reservation_change_status,
            responder_id = $2,
            response_note = $3,
            responded_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id,
            booking_id,
            listing_id,
            initiated_by_id,
            initiated_by_role as "initiated_by_role: ChangeInitiatorRole",
            status as "status: ReservationChangeStatus",
            previous_date_from,
            previous_date_to,
            proposed_date_from,
            proposed_date_to,
            previous_number_of_persons,
            proposed_number_of_persons,
            proposed_num_adults,
            proposed_num_children,
            proposed_num_infants,
            proposed_num_pets,
            currency,
            previous_total_price,
            proposed_total_price,
            balance_due,
            previous_net_payout,
            proposed_net_payout,
            host_payout_delta,
            pricing_snapshot as "pricing_snapshot: Json<ReservationChangePricingSnapshot>",
            settlement_status as "settlement_status: ChangeSettlementStatus",
            gateway_reference,
            reason,
            responder_id,
            response_note,
            responded_at,
            expires_at_date,
            created_at,
            updated_at
        "#,
        id,
        responder_id,
        response_note
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(row)
}

/// Cancels (withdraws) a `pending` reservation change request on behalf of the initiator or an admin.
#[tracing::instrument(skip(pool))]
pub async fn cancel_change_request(
    pool: &PgPool,
    id: Uuid,
    actor_id: Uuid,
    is_admin: bool,
    today_jm: NaiveDate,
) -> Result<ReservationChangeRequestRow> {
    let mut tx = pool.begin().await?;

    let existing = sqlx::query!(
        r#"
        SELECT id, booking_id, initiated_by_id, status as "status: ReservationChangeStatus", expires_at_date
        FROM reservation_change_request
        WHERE id = $1
        FOR UPDATE
        "#,
        id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;

    if existing.status == ReservationChangeStatus::Pending && existing.expires_at_date <= today_jm {
        expire_past_due_change_requests(&mut *tx, Some(existing.booking_id), today_jm).await?;
        tx.commit().await?;
        return Err(DbError::BookingNotModifiable(
            "Reservation change request has expired".to_string(),
        ));
    }

    if existing.status != ReservationChangeStatus::Pending {
        return Err(DbError::BookingNotModifiable(
            "Only pending reservation change requests can be cancelled".to_string(),
        ));
    }

    if !is_admin && existing.initiated_by_id != actor_id {
        return Err(DbError::BookingNotModifiable(
            "Only the initiator or an administrator can cancel a pending change request"
                .to_string(),
        ));
    }

    let row = sqlx::query_as!(
        ReservationChangeRequestRow,
        r#"
        UPDATE reservation_change_request
        SET status = 'cancelled'::reservation_change_status,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id,
            booking_id,
            listing_id,
            initiated_by_id,
            initiated_by_role as "initiated_by_role: ChangeInitiatorRole",
            status as "status: ReservationChangeStatus",
            previous_date_from,
            previous_date_to,
            proposed_date_from,
            proposed_date_to,
            previous_number_of_persons,
            proposed_number_of_persons,
            proposed_num_adults,
            proposed_num_children,
            proposed_num_infants,
            proposed_num_pets,
            currency,
            previous_total_price,
            proposed_total_price,
            balance_due,
            previous_net_payout,
            proposed_net_payout,
            host_payout_delta,
            pricing_snapshot as "pricing_snapshot: Json<ReservationChangePricingSnapshot>",
            settlement_status as "settlement_status: ChangeSettlementStatus",
            gateway_reference,
            reason,
            responder_id,
            response_note,
            responded_at,
            expires_at_date,
            created_at,
            updated_at
        "#,
        id
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(row)
}

/// Records that payment settlement failed on a `pending` change request without altering its `pending` status.
#[tracing::instrument(skip(pool))]
pub async fn record_failed_settlement(pool: &PgPool, id: Uuid) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE reservation_change_request
        SET settlement_status = 'failed'::change_settlement_status,
            updated_at = NOW()
        WHERE id = $1 AND status = 'pending'::reservation_change_status
        "#,
        id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Parameters for atomically approving a `pending` reservation change request.
#[derive(Debug, Clone)]
pub struct ApproveChangeRequestParams<'a> {
    /// Target reservation change request UUID.
    pub id: Uuid,
    /// Authenticated counterparty or Admin user UUID approving the request.
    pub responder_id: Uuid,
    /// Optional response note from the responder.
    pub response_note: Option<&'a str>,
    /// Final settlement status (`charged`, `refunded`, or `not_required`).
    pub settlement_status: ChangeSettlementStatus,
    /// Optional payment gateway reference string.
    pub gateway_reference: Option<&'a str>,
    /// Statutory supplier metadata for issuing the revised payment receipt.
    pub supplier: &'a SupplierInfo,
    /// Current calendar date in Jamaica (`UTC-5`).
    pub today_jm: NaiveDate,
}

/// Atomically approves a `pending` reservation change request under the global lock hierarchy
/// (`listing` -> `booking` -> `reservation_change_request`), updating `booking`, `booking_history`,
/// superseding and issuing a revised `tax_document` (`REC-YYYY-{code}-R{n}`), updating the
/// `pending` `host_payout_ledger` entry, and marking the change request `approved`.
#[tracing::instrument(skip(pool, params))]
pub async fn approve_change_request(
    pool: &PgPool,
    params: &ApproveChangeRequestParams<'_>,
) -> Result<(ReservationChangeRequestRow, Booking)> {
    let id = params.id;
    let responder_id = params.responder_id;
    let response_note = params.response_note;
    let settlement_status = params.settlement_status;
    let gateway_reference = params.gateway_reference;
    let supplier = params.supplier;
    let today_jm = params.today_jm;
    let mut tx = pool.begin().await?;

    // Lookup IDs first so we can acquire locks in strict global order:
    // 1. listing -> 2. booking -> 3. reservation_change_request
    let ids = sqlx::query!(
        "SELECT booking_id, listing_id FROM reservation_change_request WHERE id = $1",
        id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;

    // 1. Lock listing
    sqlx::query!(
        "SELECT id FROM listing WHERE id = $1 FOR UPDATE",
        ids.listing_id
    )
    .fetch_one(&mut *tx)
    .await?;

    // 2. Lock booking
    let current_booking = sqlx::query_as!(
        Booking,
        r#"
        SELECT id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus",
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy",
            metadata as "metadata: Json<crate::models::BookingMetadata>",
            door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            promo_code_id, promo_terms as "promo_terms: Json<common::promo::PromoTerms>",
            promo_stay_discount, promo_cleaning_discount,
            created_at, updated_at
        FROM booking
        WHERE id = $1
        FOR UPDATE
        "#,
        ids.booking_id
    )
    .fetch_one(&mut *tx)
    .await?;

    // 3. Lock reservation_change_request
    let req_row = sqlx::query_as!(
        ReservationChangeRequestRow,
        r#"
        SELECT
            id,
            booking_id,
            listing_id,
            initiated_by_id,
            initiated_by_role as "initiated_by_role: ChangeInitiatorRole",
            status as "status: ReservationChangeStatus",
            previous_date_from,
            previous_date_to,
            proposed_date_from,
            proposed_date_to,
            previous_number_of_persons,
            proposed_number_of_persons,
            proposed_num_adults,
            proposed_num_children,
            proposed_num_infants,
            proposed_num_pets,
            currency,
            previous_total_price,
            proposed_total_price,
            balance_due,
            previous_net_payout,
            proposed_net_payout,
            host_payout_delta,
            pricing_snapshot as "pricing_snapshot: Json<ReservationChangePricingSnapshot>",
            settlement_status as "settlement_status: ChangeSettlementStatus",
            gateway_reference,
            reason,
            responder_id,
            response_note,
            responded_at,
            expires_at_date,
            created_at,
            updated_at
        FROM reservation_change_request
        WHERE id = $1
        FOR UPDATE
        "#,
        id
    )
    .fetch_one(&mut *tx)
    .await?;

    if req_row.status == ReservationChangeStatus::Pending && req_row.expires_at_date <= today_jm {
        expire_past_due_change_requests(&mut *tx, Some(req_row.booking_id), today_jm).await?;
        tx.commit().await?;
        return Err(DbError::BookingNotModifiable(
            "Reservation change request has expired".to_string(),
        ));
    }

    if req_row.status != ReservationChangeStatus::Pending {
        return Err(DbError::BookingNotModifiable(
            "Only pending reservation change requests can be approved".to_string(),
        ));
    }

    if current_booking.status != BookingStatus::Confirmed {
        return Err(DbError::BookingNotModifiable(
            "Only confirmed bookings can be modified".to_string(),
        ));
    }

    common::pricing::validate_reservation_change_dates(
        current_booking.date_from,
        current_booking.date_to,
        req_row.proposed_date_from,
        req_row.proposed_date_to,
        today_jm,
    )
    .map_err(|e| DbError::BookingNotModifiable(e.to_string()))?;

    // Verify host payout has not been processed/paid
    let processed_payout = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)
        FROM host_payout_ledger
        WHERE booking_id = $1
          AND status IN ('processing'::payout_status, 'paid'::payout_status)
        "#,
        current_booking.id
    )
    .fetch_one(&mut *tx)
    .await?
    .unwrap_or(0);

    if processed_payout > 0 {
        return Err(DbError::BookingNotModifiable(
            "Reservation cannot be modified after host payout has been processed".to_string(),
        ));
    }

    // Re-verify date availability under the listing lock
    let dates_changed = req_row.proposed_date_from != current_booking.date_from
        || req_row.proposed_date_to != current_booking.date_to;
    if dates_changed {
        let available = crate::booking::check_availability_excluding(
            &mut *tx,
            current_booking.listing_id,
            req_row.proposed_date_from,
            req_row.proposed_date_to,
            current_booking.id,
        )
        .await?;

        if !available {
            return Err(DbError::DateUnavailable);
        }
    }

    let mut updated_metadata = current_booking.metadata.0.clone();
    updated_metadata.num_adults = req_row.proposed_num_adults.max(1) as u32;
    updated_metadata.num_children = req_row.proposed_num_children.max(0) as u32;
    updated_metadata.num_infants = req_row.proposed_num_infants.max(0) as u32;
    updated_metadata.num_pets = req_row.proposed_num_pets.max(0) as u32;

    let snap = &req_row.pricing_snapshot.0;
    check_promo_still_attached(snap, current_booking.promo_code_id)?;

    let updated_booking = sqlx::query_as!(
        Booking,
        r#"
        UPDATE booking
        SET date_from = $1,
            date_to = $2,
            number_of_persons = $3,
            metadata = $4,
            daily_rate = $5,
            total_days = $6,
            sub_total_price = $7,
            discount_value = $8,
            tax_value = $9,
            fee_breakdown = $10,
            total_price = $11,
            cleaning_fee = $12,
            damage_deposit = $13,
            platform_fee = $14,
            tax_rate = $15,
            updated_at = $16,
            promo_code_id = CASE WHEN $18 THEN promo_code_id ELSE NULL END,
            promo_terms = CASE WHEN $18 THEN promo_terms ELSE NULL END,
            promo_stay_discount = $19,
            promo_cleaning_discount = $20
        WHERE id = $17
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
        req_row.proposed_date_from,
        req_row.proposed_date_to,
        req_row.proposed_number_of_persons,
        Json(&updated_metadata) as _,
        snap.daily_rate,
        snap.total_days,
        snap.sub_total_price,
        snap.discount_value,
        snap.tax_value,
        Json(&snap.fee_breakdown) as _,
        snap.total_price,
        snap.cleaning_fee,
        snap.damage_deposit,
        snap.platform_fee,
        snap.tax_rate,
        Utc::now(),
        current_booking.id,
        snap.promo_retained,
        snap.promo_stay_discount,
        snap.promo_cleaning_discount
    )
    .fetch_one(&mut *tx)
    .await?;

    if !snap.promo_retained && current_booking.promo_code_id.is_some() {
        crate::promo::release_redemption(&mut tx, current_booking.id).await?;
    }

    let history_id = crate::booking::record_booking_history(
        &mut tx,
        &updated_booking,
        "Approved reservation change request",
        Some(responder_id),
    )
    .await?;

    crate::tax_document::issue_payment_receipt(
        &mut tx,
        &updated_booking,
        supplier,
        Some(history_id),
    )
    .await?;

    // Recalculated rather than copied from the snapshot, so a request quoted
    // under an earlier formula can't write that formula back to the ledger.
    crate::payout_ledger::reprice_pending_payout(
        &mut tx,
        updated_booking.id,
        updated_booking.accommodation_net(),
        updated_booking.cleaning_net(),
    )
    .await?;

    let updated_req = sqlx::query_as!(
        ReservationChangeRequestRow,
        r#"
        UPDATE reservation_change_request
        SET status = 'approved'::reservation_change_status,
            settlement_status = $2,
            gateway_reference = $3,
            responder_id = $4,
            response_note = $5,
            responded_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id,
            booking_id,
            listing_id,
            initiated_by_id,
            initiated_by_role as "initiated_by_role: ChangeInitiatorRole",
            status as "status: ReservationChangeStatus",
            previous_date_from,
            previous_date_to,
            proposed_date_from,
            proposed_date_to,
            previous_number_of_persons,
            proposed_number_of_persons,
            proposed_num_adults,
            proposed_num_children,
            proposed_num_infants,
            proposed_num_pets,
            currency,
            previous_total_price,
            proposed_total_price,
            balance_due,
            previous_net_payout,
            proposed_net_payout,
            host_payout_delta,
            pricing_snapshot as "pricing_snapshot: Json<ReservationChangePricingSnapshot>",
            settlement_status as "settlement_status: ChangeSettlementStatus",
            gateway_reference,
            reason,
            responder_id,
            response_note,
            responded_at,
            expires_at_date,
            created_at,
            updated_at
        "#,
        id,
        settlement_status as ChangeSettlementStatus,
        gateway_reference,
        responder_id,
        response_note
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok((updated_req, updated_booking))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        BookingMetadata, NewBooking, NewHostProfile, NewListing, NewUser, UpdatedBooking, UserRole,
    };
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use sqlx_db_tester::TestPg;
    use std::env;
    use std::path::Path;

    static DATABASE_URL_ENV: &str = "DATABASE_URL";

    async fn setup_test_db() -> TestPg {
        dotenvy::dotenv().ok();
        let db_url = env::var(DATABASE_URL_ENV).unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/our_places".to_string()
        });
        TestPg::new(db_url, Path::new("migrations"))
    }

    #[tokio::test]
    async fn test_reservation_change_lifecycle_and_duplicate_pending_guard() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;
        let mut tx = pool.begin().await.unwrap();

        let host_id = Uuid::now_v7();
        let guest_id = Uuid::now_v7();

        crate::user::create_user(
            &mut *tx,
            &NewUser {
                id: host_id,
                email: format!("host_{}@example.com", host_id),
                password_hash: "hash".to_string(),
                first_name: "Host".to_string(),
                last_name: "Owner".to_string(),
                phone_number: None,
                is_active: true,
                is_verified: true,
                verification_code: None,
                verification_code_expires_at: None,
                attributes: serde_json::json!({}),
                roles: Some(vec![UserRole::Host]),
                default_currency: "USD".to_string(),
            },
        )
        .await
        .unwrap();

        crate::user::create_host_profile(
            &mut *tx,
            host_id,
            &NewHostProfile {
                description: Some("Host".to_string()),
                verified_status: Some("verified".to_string()),
                payout_details: None,
            },
        )
        .await
        .unwrap();

        crate::user::create_user(
            &mut *tx,
            &NewUser {
                id: guest_id,
                email: format!("guest_{}@example.com", guest_id),
                password_hash: "hash".to_string(),
                first_name: "Guest".to_string(),
                last_name: "Booker".to_string(),
                phone_number: None,
                is_active: true,
                is_verified: true,
                verification_code: None,
                verification_code_expires_at: None,
                attributes: serde_json::json!({}),
                roles: Some(vec![UserRole::Booker]),
                default_currency: "USD".to_string(),
            },
        )
        .await
        .unwrap();

        let listing = crate::listing::create_listing(
            &mut *tx,
            &NewListing {
                name: format!("Change Lifecycle Villa {}", Uuid::now_v7()),
                user_id: host_id,
                description: None,
                listing_structure_id: 1,
                country: "JM".to_string(),
                price_per_night: Some(dec!(200.00)),
                weekly_discount_percentage: None,
                monthly_discount_percentage: None,
                max_guests: 6,
                bedrooms: 3,
                beds: 3,
                full_bathrooms: 2,
                half_bathrooms: 0,
                square_meters: None,
                latitude: None,
                longitude: None,
                listing_details: None,
                city: None,
                base_currency: "USD".to_string(),
                minimum_stay: 1,
                days_between_bookings: 0,
                commission_pct: Some(dec!(0.1000)),
                cleaning_fee: Some(dec!(50.00)),
                damage_deposit: Some(dec!(100.00)),
                guests_included: Some(2),
                extra_guest_fee: Some(dec!(25.00)),
            },
        )
        .await
        .unwrap();

        sqlx::query!(
            r#"
            INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
            VALUES ('USD', 'JMD', 160.000000, NOW())
            ON CONFLICT (base_currency, target_currency, effective_at) DO NOTHING
            "#
        )
        .execute(&mut *tx)
        .await
        .unwrap();

        tx.commit().await.unwrap();

        let code = format!("RC{}", Uuid::now_v7().simple())[..8].to_uppercase();
        let booking = crate::booking::create_booking(
            &pool,
            &NewBooking {
                confirmation_code: code.clone(),
                guest_id,
                listing_id: listing.id,
                date_from: NaiveDate::from_ymd_opt(2026, 12, 10).unwrap(),
                date_to: NaiveDate::from_ymd_opt(2026, 12, 14).unwrap(),
                currency: "USD".to_string(),
                daily_rate: dec!(200.00),
                number_of_persons: 2,
                total_days: 4,
                sub_total_price: dec!(800.00),
                discount_value: None,
                tax_value: Some(dec!(127.50)),
                fee_breakdown: vec![],
                total_price: dec!(1077.50),
                cancellation_policy: CancellationPolicy::Flexible,
                metadata: BookingMetadata {
                    num_adults: 2,
                    ..Default::default()
                },
                door_access_code: None,
                cleaning_fee: dec!(50.00),
                damage_deposit: dec!(100.00),
                platform_fee: dec!(40.00),
                tax_rate: dec!(0.1500),
                exchange_rate: Decimal::ONE,
                promo: None,
            },
            120,
        )
        .await
        .unwrap();

        let supplier = SupplierInfo {
            supplier_name: "Our Places Jamaica Limited".to_string(),
            supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
            supplier_trn: "000-111-222".to_string(),
        };

        let confirmed = crate::booking::update_booking(
            &pool,
            booking.id,
            &UpdatedBooking {
                status: Some(BookingStatus::Confirmed),
                ..Default::default()
            },
            &supplier,
        )
        .await
        .unwrap();
        assert_eq!(confirmed.status, BookingStatus::Confirmed);

        let today_jm = NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();
        let snapshot = ReservationChangePricingSnapshot {
            daily_rate: dec!(200.00),
            total_days: 5,
            extra_guests: 2,
            extra_guest_fee_per_night: dec!(25.00),
            extra_guest_fee_total: dec!(250.00),
            sub_total_price: dec!(1250.00),
            discount_value: None,
            discounted_subtotal: dec!(1250.00),
            cleaning_fee: dec!(50.00),
            promo_stay_discount: Decimal::ZERO,
            promo_cleaning_discount: Decimal::ZERO,
            cleaning_net: dec!(50.00),
            promo_retained: false,
            promo_lost_reason: None,
            damage_deposit: dec!(100.00),
            platform_fee: dec!(62.50),
            tax_rate: dec!(0.1500),
            tax_value: Some(dec!(195.00)),
            fee_breakdown: vec![],
            total_price: dec!(1657.50),
            host_gross_amount: dec!(1250.00),
            host_platform_fee_amount: dec!(125.00),
            host_net_payout_amount: dec!(1125.00),
        };

        let new_req = NewReservationChangeRequest {
            booking_id: booking.id,
            listing_id: listing.id,
            initiated_by_id: guest_id,
            initiated_by_role: ChangeInitiatorRole::Guest,
            previous_date_from: booking.date_from,
            previous_date_to: booking.date_to,
            proposed_date_from: NaiveDate::from_ymd_opt(2026, 12, 10).unwrap(),
            proposed_date_to: NaiveDate::from_ymd_opt(2026, 12, 15).unwrap(),
            previous_number_of_persons: 2,
            proposed_number_of_persons: 4,
            proposed_num_adults: 3,
            proposed_num_children: 1,
            proposed_num_infants: 0,
            proposed_num_pets: 0,
            currency: "USD".to_string(),
            previous_total_price: dec!(1077.50),
            proposed_total_price: dec!(1657.50),
            balance_due: dec!(580.00),
            previous_net_payout: dec!(720.00),
            proposed_net_payout: dec!(1125.00),
            host_payout_delta: dec!(405.00),
            pricing_snapshot: snapshot,
            settlement_status: ChangeSettlementStatus::PendingCharge,
            reason: Some("Adding family members and 1 extra night".to_string()),
            expires_at_date: NaiveDate::from_ymd_opt(2026, 12, 10).unwrap(),
        };

        // 1. Create pending change request
        let created = create_change_request(&pool, &new_req, today_jm)
            .await
            .expect("Failed to create change request");
        assert_eq!(created.status, ReservationChangeStatus::Pending);

        // 2. Duplicate pending request on same booking is blocked by partial unique index
        let dup_err = create_change_request(&pool, &new_req, today_jm)
            .await
            .unwrap_err();
        assert!(matches!(dup_err, DbError::ChangeRequestAlreadyPending));

        // 3. Approve change request
        let (approved_req, updated_booking) = approve_change_request(
            &pool,
            &ApproveChangeRequestParams {
                id: created.id,
                responder_id: host_id,
                response_note: Some("Welcome!"),
                settlement_status: ChangeSettlementStatus::Charged,
                gateway_reference: Some("SIM-CHG-001"),
                supplier: &supplier,
                today_jm,
            },
        )
        .await
        .expect("Failed to approve change request");

        assert_eq!(approved_req.status, ReservationChangeStatus::Approved);
        assert_eq!(
            approved_req.settlement_status,
            ChangeSettlementStatus::Charged
        );
        assert_eq!(updated_booking.total_days, 5);
        assert_eq!(updated_booking.number_of_persons, 4);
        assert_eq!(updated_booking.metadata.0.num_adults, 3);
        assert_eq!(updated_booking.metadata.0.num_children, 1);
        assert_eq!(updated_booking.total_price, dec!(1657.50));

        // Verify REC-R2 receipt created and R1 superseded
        let docs = crate::tax_document::list_for_booking(&pool, booking.id)
            .await
            .unwrap();
        assert_eq!(docs.len(), 2);
        let r2 = docs.iter().find(|d| d.revision == 2).unwrap();
        assert_eq!(r2.amount_previously_paid, Some(dec!(1077.50)));
        assert_eq!(r2.balance_due, Some(dec!(580.00)));

        // Verify pending payout ledger updated
        let payout_net = sqlx::query_scalar!(
            "SELECT net_payout_amount FROM host_payout_ledger WHERE booking_id = $1",
            booking.id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        // (1250.00 stay + 50.00 cleaning) less 10% commission, not the snapshot's
        // 1125.00, which was quoted without the cleaning fee.
        assert_eq!(payout_net, dec!(1170.00));

        // 4. Create a second change request and test auto-expiry when today_jm >= expires_at_date
        let second_req = NewReservationChangeRequest {
            previous_date_from: updated_booking.date_from,
            previous_date_to: updated_booking.date_to,
            proposed_date_from: NaiveDate::from_ymd_opt(2026, 12, 11).unwrap(),
            proposed_date_to: NaiveDate::from_ymd_opt(2026, 12, 15).unwrap(),
            expires_at_date: NaiveDate::from_ymd_opt(2026, 12, 10).unwrap(),
            ..new_req
        };
        let created2 = create_change_request(&pool, &second_req, today_jm)
            .await
            .unwrap();
        assert_eq!(created2.status, ReservationChangeStatus::Pending);

        let expired_count = expire_past_due_change_requests(
            &pool,
            Some(booking.id),
            NaiveDate::from_ymd_opt(2026, 12, 10).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(expired_count, 1);

        let fetched2 = get_change_request_by_id(&pool, created2.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched2.status, ReservationChangeStatus::Expired);
    }
}

#[cfg(test)]
mod promo_tests {
    //! How approving a change treats the booking's promo.

    use super::*;
    use crate::promo_tests::{insert_code, migrated_db, raw_booking, raw_listing, raw_user};
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use sqlx::PgPool;

    fn snapshot(retained: bool) -> ReservationChangePricingSnapshot {
        ReservationChangePricingSnapshot {
            daily_rate: dec!(200.00),
            total_days: 3,
            extra_guests: 0,
            extra_guest_fee_per_night: Decimal::ZERO,
            extra_guest_fee_total: Decimal::ZERO,
            sub_total_price: dec!(600.00),
            discount_value: None,
            discounted_subtotal: if retained { dec!(540.00) } else { dec!(600.00) },
            cleaning_fee: Decimal::ZERO,
            promo_stay_discount: if retained { dec!(60.00) } else { Decimal::ZERO },
            promo_cleaning_discount: Decimal::ZERO,
            cleaning_net: Decimal::ZERO,
            promo_retained: retained,
            promo_lost_reason: (!retained).then(|| "Too short".to_string()),
            damage_deposit: Decimal::ZERO,
            platform_fee: dec!(27.00),
            tax_rate: dec!(0.1500),
            tax_value: Some(dec!(81.00)),
            fee_breakdown: vec![],
            total_price: dec!(648.00),
            host_gross_amount: dec!(540.00),
            host_platform_fee_amount: Decimal::ZERO,
            host_net_payout_amount: dec!(540.00),
        }
    }

    /// A confirmed promo booking with a pending change to one more night.
    async fn pending_change(pool: &PgPool, retained: bool) -> (Uuid, Uuid, Uuid) {
        let host = raw_user(pool).await;
        let guest = raw_user(pool).await;
        let villa = raw_listing(pool, host).await;
        let code = insert_code(pool, host, "APPROVED").await;
        let booking = raw_booking(pool, guest, villa, "confirmed").await;
        sqlx::query(
            r#"UPDATE booking SET promo_code_id = $2, promo_stay_discount = 40,
                   promo_terms = '{"code":"APPROVED","discount_type":"percentage","discount_value":"10","scope":"stay","currency":null,"exchange_rate":"1","min_nights":null,"min_booking_value":null,"stay_from":null,"stay_to":null}'
               WHERE id = $1"#,
        )
        .bind(booking)
        .bind(code)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO promo_code_redemption (promo_code_id, booking_id) VALUES ($1, $2)",
        )
        .bind(code)
        .bind(booking)
        .execute(pool)
        .await
        .unwrap();
        let from = NaiveDate::from_ymd_opt(2030, 1, 1).expect("valid date");
        let request = NewReservationChangeRequest {
            booking_id: booking,
            listing_id: villa,
            initiated_by_id: guest,
            initiated_by_role: ChangeInitiatorRole::Guest,
            previous_date_from: from,
            previous_date_to: NaiveDate::from_ymd_opt(2030, 1, 3).expect("valid date"),
            proposed_date_from: from,
            proposed_date_to: NaiveDate::from_ymd_opt(2030, 1, 4).expect("valid date"),
            previous_number_of_persons: 1,
            proposed_number_of_persons: 1,
            proposed_num_adults: 1,
            proposed_num_children: 0,
            proposed_num_infants: 0,
            proposed_num_pets: 0,
            currency: "USD".to_string(),
            previous_total_price: dec!(200.00),
            proposed_total_price: dec!(648.00),
            balance_due: dec!(448.00),
            previous_net_payout: Decimal::ZERO,
            proposed_net_payout: dec!(540.00),
            host_payout_delta: dec!(540.00),
            pricing_snapshot: snapshot(retained),
            settlement_status: ChangeSettlementStatus::PendingCharge,
            reason: None,
            expires_at_date: from,
        };
        let created = create_change_request(pool, &request, today())
            .await
            .expect("create change request");
        (created.id, booking, host)
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2029, 12, 1).expect("valid date")
    }

    async fn approve(pool: &PgPool, id: Uuid, responder_id: Uuid) -> Result<Booking> {
        let supplier = SupplierInfo {
            supplier_name: "Our Places Jamaica Limited".to_string(),
            supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
            supplier_trn: "000-111-222".to_string(),
        };
        approve_change_request(
            pool,
            &ApproveChangeRequestParams {
                id,
                responder_id,
                response_note: None,
                settlement_status: ChangeSettlementStatus::Charged,
                gateway_reference: None,
                supplier: &supplier,
                today_jm: today(),
            },
        )
        .await
        .map(|(_, booking)| booking)
    }

    async fn released(pool: &PgPool, booking: Uuid) -> bool {
        sqlx::query_scalar(
            "SELECT released_at IS NOT NULL FROM promo_code_redemption WHERE booking_id = $1",
        )
        .bind(booking)
        .fetch_one(pool)
        .await
        .expect("redemption")
    }

    #[tokio::test]
    async fn approving_a_kept_promo_writes_the_new_amounts() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let (id, booking, host) = pending_change(&pool, true).await;

        let updated = approve(&pool, id, host).await.unwrap();

        assert!(updated.promo_code_id.is_some());
        assert!(updated.promo_terms.is_some());
        assert_eq!(updated.promo_stay_discount, dec!(60.00));
        assert_eq!(updated.accommodation_net(), dec!(540.00));
        assert!(!released(&pool, booking).await);
    }

    #[tokio::test]
    async fn approving_a_lost_promo_clears_it_and_frees_the_use() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let (id, booking, host) = pending_change(&pool, false).await;

        let updated = approve(&pool, id, host).await.unwrap();

        assert_eq!(updated.promo_code_id, None);
        assert!(updated.promo_terms.is_none());
        assert_eq!(updated.promo_stay_discount, Decimal::ZERO);
        assert!(released(&pool, booking).await);
    }

    #[tokio::test]
    async fn approval_is_refused_when_a_kept_promo_was_removed_after_the_quote() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let (id, booking, host) = pending_change(&pool, true).await;
        sqlx::query(
            "UPDATE booking SET promo_code_id = NULL, promo_terms = NULL, promo_stay_discount = 0 WHERE id = $1",
        )
        .bind(booking)
        .execute(&pool)
        .await
        .unwrap();

        let res = approve(&pool, id, host).await;

        assert!(
            matches!(res, Err(DbError::BookingNotModifiable(_))),
            "got {res:?}"
        );
        let current = crate::booking::get_booking_by_id(&pool, booking)
            .await
            .unwrap();
        assert_eq!(current.promo_stay_discount, Decimal::ZERO);
    }

    #[test]
    fn a_snapshot_stored_before_this_release_still_deserializes() {
        let mut stored = serde_json::to_value(snapshot(false)).expect("serialize");
        let fields = stored.as_object_mut().expect("an object");
        for field in [
            "promo_stay_discount",
            "promo_cleaning_discount",
            "cleaning_net",
            "promo_retained",
            "promo_lost_reason",
        ] {
            fields.remove(field);
        }
        let parsed: ReservationChangePricingSnapshot =
            serde_json::from_value(stored).expect("old snapshots still parse");
        assert!(!parsed.promo_retained);
        assert_eq!(parsed.promo_stay_discount, Decimal::ZERO);
        assert_eq!(parsed.promo_lost_reason, None);
    }
}
