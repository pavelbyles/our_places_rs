//! Booking status transitions, host payout bookkeeping and deletion.

use super::availability::check_availability_excluding;
use super::history::record_booking_history;
use crate::error::Result;
use crate::models::{
    Booking, BookingStatus, CancellationPolicy, FeeItem, SupplierInfo, UpdatedBooking,
};
use chrono::{NaiveDate, Utc};
use common::payout::{NewPayout, PayoutRecord, PendingPayout};
use rust_decimal::Decimal;
use sqlx::types::Json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Updates a booking's status.
#[tracing::instrument(skip(pool, updated_booking, supplier), fields(booking_id = %id))]
pub async fn update_booking(
    pool: &PgPool,
    id: Uuid,
    updated_booking: &UpdatedBooking,
    supplier: &SupplierInfo,
) -> Result<Booking> {
    let mut tx = pool.begin().await?;

    let current = lock_booking(&mut tx, id).await?;
    let change = BookingChange::new(&current, updated_booking);
    check_change_allowed(&mut tx, id, &current, updated_booking, &change).await?;
    keep_promo_use_on_confirmation(&mut tx, id, &current, updated_booking).await?;

    let booking = write_booking_update(&mut tx, id, updated_booking).await?;

    let history_id = match change.history_reason() {
        Some(reason) => Some(record_booking_history(&mut tx, &booking, reason, None).await?),
        None => None,
    };

    apply_status_side_effects(&mut tx, &booking, &change, supplier, history_id).await?;

    tx.commit().await?;

    Ok(booking)
}

/// The booking as it was before this update, read under its row lock.
struct CurrentBooking {
    listing_id: Uuid,
    status: BookingStatus,
    date_from: NaiveDate,
    date_to: NaiveDate,
    number_of_persons: i32,
    hold_active: bool,
    promo_code_id: Option<Uuid>,
}

/// Locks the booking's listing, then the booking row, and reads the booking.
async fn lock_booking(conn: &mut PgConnection, id: Uuid) -> Result<CurrentBooking> {
    // Standardize global lock acquisition hierarchy (Listing -> Booking) to prevent
    // deadlocks (PostgreSQL 40P01) with concurrent create_booking operations.
    let booking_listing = sqlx::query!("SELECT listing_id FROM booking WHERE id = $1", id)
        .fetch_one(&mut *conn)
        .await?;

    sqlx::query!(
        "SELECT id FROM listing WHERE id = $1 FOR UPDATE",
        booking_listing.listing_id
    )
    .fetch_one(&mut *conn)
    .await?;

    // The clock, not `NOW()`, which is fixed when the transaction begins. This may
    // have waited on the listing lock while the hold ran out and a rival took its dates.
    let row = sqlx::query!(
        r#"SELECT id, listing_id, status as "status: BookingStatus", date_from, date_to, number_of_persons,
            COALESCE(expires_at > clock_timestamp(), FALSE) as "hold_active!", promo_code_id
        FROM booking WHERE id = $1 FOR UPDATE"#,
        id
    )
    .fetch_one(&mut *conn)
    .await?;

    Ok(CurrentBooking {
        listing_id: row.listing_id,
        status: row.status,
        date_from: row.date_from,
        date_to: row.date_to,
        number_of_persons: row.number_of_persons,
        hold_active: row.hold_active,
        promo_code_id: row.promo_code_id,
    })
}

/// What this update changes compared with the stored booking.
struct BookingChange {
    from: BookingStatus,
    to: Option<BookingStatus>,
    status_changed: bool,
    dates_changed: bool,
    guests_changed: bool,
    metadata_changed: bool,
    door_code_changed: bool,
}

impl BookingChange {
    fn new(current: &CurrentBooking, updated: &UpdatedBooking) -> Self {
        let date_from_changed = updated.date_from.is_some_and(|d| d != current.date_from);
        let date_to_changed = updated.date_to.is_some_and(|d| d != current.date_to);
        Self {
            from: current.status,
            to: updated.status,
            status_changed: updated.status.is_some_and(|s| s != current.status),
            dates_changed: date_from_changed || date_to_changed,
            guests_changed: updated
                .number_of_persons
                .is_some_and(|n| n != current.number_of_persons),
            metadata_changed: updated.metadata.is_some(),
            door_code_changed: updated.door_access_code.is_some(),
        }
    }

    /// Dates or guest count changed: the stay itself is different.
    fn terms_changed(&self) -> bool {
        self.dates_changed || self.guests_changed
    }

    /// The booking moved into `status` with this update.
    fn became(&self, status: BookingStatus) -> bool {
        self.to == Some(status) && self.status_changed
    }

    /// Why this update belongs in the booking history; `None` when nothing
    /// worth recording changed.
    fn history_reason(&self) -> Option<&'static str> {
        if self.dates_changed {
            return Some("Date change on confirmed reservation");
        }
        if self.guests_changed {
            return Some("Guest count change on confirmed reservation");
        }
        match (
            self.status_changed,
            self.metadata_changed,
            self.door_code_changed,
        ) {
            (false, false, false) => None,
            (true, true, _) => Some("Status and metadata updated"),
            (true, false, false) => Some("Status updated"),
            (false, true, false) => Some("Metadata updated"),
            (false, false, true) => Some("Door access code updated"),
            _ => Some("Booking updated"),
        }
    }
}

/// Rejects updates the booking's current state does not allow. Checked under
/// the row lock so concurrent requests cannot, say, confirm a booking that
/// another request has just cancelled.
async fn check_change_allowed(
    conn: &mut PgConnection,
    id: Uuid,
    current: &CurrentBooking,
    updated: &UpdatedBooking,
    change: &BookingChange,
) -> Result<()> {
    if let Some(to) = updated.status
        && !current.status.can_transition_to(to)
    {
        tracing::warn!(booking_id = %id, from = ?current.status, to = ?to, "rejected invalid booking status change");
        return Err(crate::error::DbError::BookingNotModifiable(format!(
            "A {} booking cannot be changed to {}",
            format!("{:?}", current.status).to_lowercase(),
            format!("{to:?}").to_lowercase()
        )));
    }

    if updated.status == Some(BookingStatus::Confirmed)
        && current.status == BookingStatus::Pending
        && !current.hold_active
    {
        tracing::warn!(booking_id = %id, "rejected confirmation of an expired hold");
        return Err(crate::error::DbError::HoldExpired);
    }

    if change.terms_changed() && current.status != BookingStatus::Confirmed {
        return Err(crate::error::DbError::BookingNotModifiable(
            "Only confirmed bookings can have their dates modified".to_string(),
        ));
    }

    if !change.dates_changed {
        return Ok(());
    }
    let target_from = updated.date_from.unwrap_or(current.date_from);
    let target_to = updated.date_to.unwrap_or(current.date_to);
    let available =
        check_availability_excluding(&mut *conn, current.listing_id, target_from, target_to, id)
            .await?;
    if !available {
        return Err(crate::error::DbError::DateUnavailable);
    }
    Ok(())
}

/// Stops a hold being confirmed after it lost its promo use at expiry. Another
/// hold counts an expired one as free under the code's lock, so confirming must
/// take that lock too and then re-read the time, which may have passed while it
/// waited. Lock order: listing, then booking, then promo code.
async fn keep_promo_use_on_confirmation(
    conn: &mut PgConnection,
    id: Uuid,
    current: &CurrentBooking,
    updated: &UpdatedBooking,
) -> Result<()> {
    let confirming = updated.status == Some(BookingStatus::Confirmed)
        && current.status == BookingStatus::Pending;
    let (true, Some(promo_code_id)) = (confirming, current.promo_code_id) else {
        return Ok(());
    };
    crate::promo::lock_code(&mut *conn, promo_code_id).await?;
    let still_held = sqlx::query_scalar!(
        r#"SELECT COALESCE(expires_at > clock_timestamp(), FALSE) as "held!" FROM booking WHERE id = $1"#,
        id
    )
    .fetch_one(&mut *conn)
    .await?;
    if still_held {
        Ok(())
    } else {
        tracing::warn!(booking_id = %id, "rejected confirmation of a hold that expired while waiting for its promo code");
        Err(crate::error::DbError::HoldExpired)
    }
}

/// Writes the update; fields left `None` keep their stored values.
async fn write_booking_update(
    conn: &mut PgConnection,
    id: Uuid,
    updated_booking: &UpdatedBooking,
) -> Result<Booking> {
    let booking = sqlx::query_as!(
        Booking,
        r#"
        UPDATE booking
        SET status = COALESCE($1, status), 
            metadata = COALESCE($2, metadata),
            door_access_code = COALESCE($3, door_access_code),
            date_from = COALESCE($4, date_from),
            date_to = COALESCE($5, date_to),
            daily_rate = COALESCE($6, daily_rate),
            total_days = COALESCE($7, total_days),
            sub_total_price = COALESCE($8, sub_total_price),
            discount_value = CASE WHEN $21 THEN $9 ELSE COALESCE($9, discount_value) END,
            tax_value = COALESCE($10, tax_value),
            fee_breakdown = COALESCE($11, fee_breakdown),
            total_price = COALESCE($12, total_price),
            cleaning_fee = COALESCE($13, cleaning_fee),
            damage_deposit = COALESCE($14, damage_deposit),
            platform_fee = COALESCE($15, platform_fee),
            tax_rate = COALESCE($16, tax_rate),
            exchange_rate = COALESCE($17, exchange_rate),
            updated_at = $18,
            number_of_persons = COALESCE($20, number_of_persons),
            promo_code_id = CASE WHEN $22 THEN NULL ELSE promo_code_id END,
            promo_terms = CASE WHEN $22 THEN NULL ELSE promo_terms END,
            promo_stay_discount = CASE WHEN $22 THEN 0 ELSE COALESCE($23, promo_stay_discount) END,
            promo_cleaning_discount = CASE WHEN $22 THEN 0 ELSE COALESCE($24, promo_cleaning_discount) END
        WHERE id = $19
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
        updated_booking.status as Option<BookingStatus>,
        updated_booking.metadata.as_ref().map(Json) as _,
        updated_booking.door_access_code.as_deref(),
        updated_booking.date_from,
        updated_booking.date_to,
        updated_booking.daily_rate,
        updated_booking.total_days,
        updated_booking.sub_total_price,
        updated_booking.discount_value,
        updated_booking.tax_value,
        updated_booking.fee_breakdown.as_ref().map(Json) as _,
        updated_booking.total_price,
        updated_booking.cleaning_fee,
        updated_booking.damage_deposit,
        updated_booking.platform_fee,
        updated_booking.tax_rate,
        updated_booking.exchange_rate,
        Utc::now(),
        id,
        updated_booking.number_of_persons,
        updated_booking.replace_discounts,
        updated_booking.clear_promo,
        updated_booking.promo_stay_discount,
        updated_booking.promo_cleaning_discount
    )
    .fetch_one(&mut *conn)
    .await?;
    if updated_booking.clear_promo {
        crate::promo::release_redemption(&mut *conn, id).await?;
    }
    Ok(booking)
}

/// Tax documents, host payouts, review tokens and change requests that follow
/// from the update, in the order they must be written.
async fn apply_status_side_effects(
    conn: &mut PgConnection,
    booking: &Booking,
    change: &BookingChange,
    supplier: &SupplierInfo,
    history_id: Option<Uuid>,
) -> Result<()> {
    if change.became(BookingStatus::Completed) {
        crate::review::create_review_token(&mut *conn, booking.id).await?;
    }
    if change.became(BookingStatus::Confirmed) {
        start_host_payout(&mut *conn, booking, supplier, history_id).await?;
    }
    if change.terms_changed() && change.from == BookingStatus::Confirmed {
        revise_confirmed_terms(&mut *conn, booking, supplier, history_id).await?;
    }
    if change.became(BookingStatus::Cancelled) {
        settle_cancellation(&mut *conn, booking, change.from, supplier, history_id).await?;
    }
    if change.became(BookingStatus::Completed) {
        complete_stay(&mut *conn, booking, supplier, history_id).await?;
    }
    Ok(())
}

/// A newly confirmed booking: issue the payment receipt and queue the host payout.
async fn start_host_payout(
    conn: &mut PgConnection,
    booking: &Booking,
    supplier: &SupplierInfo,
    history_id: Option<Uuid>,
) -> Result<()> {
    crate::tax_document::issue_payment_receipt(&mut *conn, booking, supplier, history_id).await?;

    let terms = host_payout_terms(&mut *conn, booking).await?;
    let gross_amount = common::pricing::host_gross(
        booking.accommodation_net(),
        booking.cleaning_net(),
        terms.exchange_rate,
    );
    let promo_discount = booking.promo_stay_discount + booking.promo_cleaning_discount;
    crate::payout_ledger::create_payout_ledger_entry(
        &mut *conn,
        &terms.pending_payout(
            booking,
            gross_amount,
            (booking.cleaning_net() * terms.exchange_rate).round_dp(2),
            (promo_discount * terms.exchange_rate).round_dp(2),
        ),
    )
    .await?;
    Ok(())
}

/// A confirmed booking's dates or guest count changed: issue a revised receipt
/// and reprice the pending host payout.
async fn revise_confirmed_terms(
    conn: &mut PgConnection,
    booking: &Booking,
    supplier: &SupplierInfo,
    history_id: Option<Uuid>,
) -> Result<()> {
    crate::tax_document::issue_payment_receipt(&mut *conn, booking, supplier, history_id).await?;

    crate::payout_ledger::reprice_pending_payout(
        &mut *conn,
        booking.id,
        booking.accommodation_net(),
        booking.cleaning_net(),
        booking.promo_stay_discount + booking.promo_cleaning_discount,
    )
    .await
}

/// A cancelled booking: close its pending change requests and, if it had been
/// confirmed, refund the guest and pay the host whatever the policy retains.
async fn settle_cancellation(
    conn: &mut PgConnection,
    booking: &Booking,
    from: BookingStatus,
    supplier: &SupplierInfo,
    history_id: Option<Uuid>,
) -> Result<()> {
    sqlx::query!(
        r#"
            UPDATE reservation_change_request
            SET status = 'cancelled'::reservation_change_status,
                updated_at = NOW()
            WHERE booking_id = $1 AND status = 'pending'::reservation_change_status
            "#,
        booking.id
    )
    .execute(&mut *conn)
    .await?;

    if from != BookingStatus::Confirmed {
        crate::payout_ledger::cancel_pending_payout_for_booking(&mut *conn, booking.id).await?;
        return Ok(());
    }

    let ref_doc =
        crate::tax_document::issue_refund_receipt(&mut *conn, booking, supplier, history_id)
            .await?;

    let cancelled_payout =
        crate::payout_ledger::cancel_pending_payout_for_booking(&mut *conn, booking.id).await?;

    // If accommodation was partially retained by host (policy_pct < 1.00), issue adjusted payout
    let host_retained = (booking.accommodation_net() - ref_doc.accommodation_net).round_dp(2);
    if host_retained <= Decimal::ZERO {
        return Ok(());
    }
    if let Some(ledger_id) = cancelled_payout {
        return crate::payout_ledger::reopen_payout_as_retained(
            &mut *conn,
            ledger_id,
            host_retained,
        )
        .await;
    }

    // No pending row: either the booking predates the ledger, or its payout is
    // already processing, paid or refunded, which only an operator can reconcile.
    let terms = host_payout_terms(&mut *conn, booking).await?;
    let retained_gross = (host_retained * terms.exchange_rate).round_dp(2);
    let inserted = crate::payout_ledger::create_payout_ledger_entry(
        &mut *conn,
        &terms.pending_payout(booking, retained_gross, Decimal::ZERO, Decimal::ZERO),
    )
    .await?;
    if inserted == 0 {
        tracing::warn!(
            booking_id = %booking.id,
            %host_retained,
            "retained payout not recorded: the payout already left the pending state"
        );
    }
    Ok(())
}

/// A completed stay: expire its pending change requests and issue the
/// statutory tax invoice (INV-YYYY-{code}).
async fn complete_stay(
    conn: &mut PgConnection,
    booking: &Booking,
    supplier: &SupplierInfo,
    history_id: Option<Uuid>,
) -> Result<()> {
    sqlx::query!(
        r#"
            UPDATE reservation_change_request
            SET status = 'expired'::reservation_change_status,
                updated_at = NOW()
            WHERE booking_id = $1 AND status = 'pending'::reservation_change_status
            "#,
        booking.id
    )
    .execute(&mut *conn)
    .await?;

    crate::tax_document::issue_tax_invoice(&mut *conn, booking, supplier, history_id).await?;
    Ok(())
}

/// Who a booking's host payout goes to, at what commission, and the rate that
/// converts the booking's amounts (stored in the booking currency) into the
/// host's payout currency. Fails rather than guessing when no rate exists.
struct HostPayoutTerms {
    host_id: Uuid,
    commission_pct: Decimal,
    host_currency: String,
    exchange_rate: Decimal,
}

impl HostPayoutTerms {
    /// A pending payout to this host of `gross_amount`, already converted
    /// into the host's currency, less the platform commission.
    fn pending_payout(
        &self,
        booking: &Booking,
        gross_amount: Decimal,
        cleaning_fee_amount: Decimal,
        promo_discount_amount: Decimal,
    ) -> PayoutRecord<PendingPayout> {
        let (platform_fee_amount, net_payout_amount) = common::pricing::calculate_host_payout(
            gross_amount,
            self.commission_pct,
            Decimal::ZERO,
        );
        PayoutRecord::new(NewPayout {
            id: Uuid::now_v7(),
            booking_id: booking.id,
            listing_id: booking.listing_id,
            host_id: self.host_id,
            currency: self.host_currency.clone(),
            gross_amount,
            platform_fee_pct: self.commission_pct,
            platform_fee_amount,
            tax_withheld_amount: Decimal::ZERO,
            exchange_rate: self.exchange_rate,
            net_payout_amount,
            cleaning_fee_amount,
            promo_discount_amount,
        })
    }
}

async fn host_payout_terms(
    conn: &mut sqlx::PgConnection,
    booking: &Booking,
) -> Result<HostPayoutTerms> {
    let listing = sqlx::query!(
        r#"SELECT user_id, commission_pct FROM listing WHERE id = $1"#,
        booking.listing_id
    )
    .fetch_one(&mut *conn)
    .await?;

    let host_currency = sqlx::query_scalar!(
        r#"SELECT default_currency FROM "user" WHERE id = $1"#,
        listing.user_id
    )
    .fetch_one(&mut *conn)
    .await?;

    let exchange_rate = crate::exchange_rate::get_rate(
        conn,
        &booking.currency,
        &host_currency,
        Utc::now(),
        crate::exchange_rate::RateUse::Settlement,
    )
    .await
    .inspect_err(|e| {
        tracing::error!(
            booking_id = %booking.id,
            from = %booking.currency,
            to = %host_currency,
            error = %e,
            "host payout currency conversion failed"
        )
    })?;

    Ok(HostPayoutTerms {
        host_id: listing.user_id,
        commission_pct: listing.commission_pct,
        host_currency,
        exchange_rate,
    })
}

/// Deletes a booking (Hard Delete).
#[tracing::instrument(skip(pool))]
pub async fn delete_booking(pool: &PgPool, id: Uuid) -> Result<()> {
    // Tax documents and payout ledger rows reference the booking with
    // ON DELETE RESTRICT: statutory records must outlive it. Refuse clearly.
    let result: sqlx::postgres::PgQueryResult =
        sqlx::query!("DELETE FROM booking WHERE id = $1", id)
            .execute(pool)
            .await
            .map_err(|e| match e.as_database_error() {
                // 23001 restrict_violation (ON DELETE RESTRICT), 23503 foreign_key_violation.
                Some(db) if matches!(db.code().as_deref(), Some("23001" | "23503")) => {
                    tracing::warn!(booking_id = %id, constraint = ?db.constraint(), "hard delete refused: booking has retained records");
                    crate::error::DbError::BookingNotModifiable(
                        "This booking has tax documents, payout records or a promo redemption and cannot be deleted; cancel it instead".to_string(),
                    )
                }
                _ => e.into(),
            })?;

    if result.rows_affected() == 0 {
        return Err(crate::error::DbError::Sqlx(sqlx::Error::RowNotFound));
    }

    Ok(())
}

#[cfg(test)]
#[path = "confirm_clock_tests.rs"]
mod confirm_clock_tests;
