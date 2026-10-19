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

/// Host payout context for a booking used when computing a reservation change quote.
#[derive(Debug, Clone)]
pub struct BookingPayoutContext {
    /// Whether the booking's host payout has already been processed or paid.
    pub is_payout_processed: bool,
    /// Exchange rate from the listing's base currency to the host's default currency.
    pub host_exchange_rate: rust_decimal::Decimal,
    /// Existing pending net payout amount if a `host_payout_ledger` row exists.
    pub existing_net_payout: Option<rust_decimal::Decimal>,
}

/// Retrieves the host payout context (status, exchange rate, and current net payout) for a booking.
#[tracing::instrument(skip(pool))]
pub async fn get_booking_payout_context(
    pool: &PgPool,
    booking_id: Uuid,
    host_id: Uuid,
    listing_base_currency: &str,
) -> Result<BookingPayoutContext> {
    let existing = sqlx::query!(
        r#"
        SELECT status as "status: DbPayoutStatus", exchange_rate, net_payout_amount
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
        });
    }

    let host_currency = sqlx::query_scalar!(
        r#"SELECT default_currency FROM "user" WHERE id = $1"#,
        host_id
    )
    .fetch_optional(pool)
    .await?
    .unwrap_or_else(|| listing_base_currency.to_string());

    let host_exchange_rate = if listing_base_currency == host_currency {
        rust_decimal::Decimal::ONE
    } else {
        sqlx::query_scalar!(
            r#"SELECT rate FROM currency_exchange_rates 
               WHERE base_currency = $1 AND target_currency = $2 
               ORDER BY effective_at DESC, updated_at DESC LIMIT 1"#,
            listing_base_currency,
            host_currency
        )
        .fetch_optional(pool)
        .await?
        .unwrap_or(rust_decimal::Decimal::ONE)
    };

    Ok(BookingPayoutContext {
        is_payout_processed: false,
        host_exchange_rate,
        existing_net_payout: None,
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
            updated_at = $16
        WHERE id = $17
        RETURNING id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus",
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy",
            metadata as "metadata: Json<crate::models::BookingMetadata>",
            door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
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
        current_booking.id
    )
    .fetch_one(&mut *tx)
    .await?;

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

    sqlx::query!(
        r#"
        UPDATE host_payout_ledger
        SET gross_amount = $1,
            platform_fee_amount = $2,
            net_payout_amount = $3,
            updated_at = NOW()
        WHERE booking_id = $4 AND status = 'pending'::payout_status
        "#,
        snap.host_gross_amount,
        snap.host_platform_fee_amount,
        snap.host_net_payout_amount,
        updated_booking.id
    )
    .execute(&mut *tx)
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
            },
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
        assert_eq!(payout_net, dec!(1125.00));

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
