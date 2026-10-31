//! Booking status transitions, host payout bookkeeping and deletion.

use super::availability::check_availability_excluding;
use super::history::record_booking_history;
use crate::error::Result;
use crate::models::{
    Booking, BookingStatus, CancellationPolicy, FeeItem, SupplierInfo, UpdatedBooking,
};
use chrono::Utc;
use common::payout::{NewPayout, PayoutRecord, PendingPayout};
use rust_decimal::Decimal;
use sqlx::PgPool;
use sqlx::types::Json;
use uuid::Uuid;

/// Updates a booking's status.
#[tracing::instrument(skip(pool))]
// skipcq: RS-R1000
pub async fn update_booking(
    pool: &PgPool,
    id: Uuid,
    updated_booking: &UpdatedBooking,
    supplier: &SupplierInfo,
) -> Result<Booking> {
    // Check existence first or let standard update fail if not found?
    // Listing API uses transaction and SELECT FOR UPDATE, then COALESCE.
    // I'll follow that pattern.

    let mut tx = pool.begin().await?;

    // Standardize global lock acquisition hierarchy (Listing -> Booking) to prevent
    // deadlocks (PostgreSQL 40P01) with concurrent create_booking operations.
    let booking_listing = sqlx::query!("SELECT listing_id FROM booking WHERE id = $1", id)
        .fetch_one(&mut *tx)
        .await?;

    sqlx::query!(
        "SELECT id FROM listing WHERE id = $1 FOR UPDATE",
        booking_listing.listing_id
    )
    .fetch_one(&mut *tx)
    .await?;

    let current = sqlx::query!(
        r#"SELECT id, listing_id, status as "status: BookingStatus", date_from, date_to, number_of_persons,
            COALESCE(expires_at > NOW(), FALSE) as "hold_active!"
        FROM booking WHERE id = $1 FOR UPDATE"#,
        id
    )
    .fetch_one(&mut *tx)
    .await?;

    // Checked under the row lock so concurrent requests cannot, say, confirm
    // a booking that another request has just cancelled.
    if let Some(to) = updated_booking.status
        && !current.status.can_transition_to(to)
    {
        tracing::warn!(booking_id = %id, from = ?current.status, to = ?to, "rejected invalid booking status change");
        return Err(crate::error::DbError::BookingNotModifiable(format!(
            "A {} booking cannot be changed to {}",
            format!("{:?}", current.status).to_lowercase(),
            format!("{to:?}").to_lowercase()
        )));
    }

    if updated_booking.status == Some(BookingStatus::Confirmed)
        && current.status == BookingStatus::Pending
        && !current.hold_active
    {
        tracing::warn!(booking_id = %id, "rejected confirmation of an expired hold");
        return Err(crate::error::DbError::HoldExpired);
    }

    let dates_changed = (updated_booking.date_from.is_some()
        && updated_booking.date_from != Some(current.date_from))
        || (updated_booking.date_to.is_some() && updated_booking.date_to != Some(current.date_to));
    let guests_changed = updated_booking.number_of_persons.is_some()
        && updated_booking.number_of_persons != Some(current.number_of_persons);
    let terms_changed = dates_changed || guests_changed;

    if terms_changed && current.status != BookingStatus::Confirmed {
        return Err(crate::error::DbError::BookingNotModifiable(
            "Only confirmed bookings can have their dates modified".to_string(),
        ));
    }

    if dates_changed {
        let target_from = updated_booking.date_from.unwrap_or(current.date_from);
        let target_to = updated_booking.date_to.unwrap_or(current.date_to);

        let available =
            check_availability_excluding(&mut *tx, current.listing_id, target_from, target_to, id)
                .await?;

        if !available {
            return Err(crate::error::DbError::DateUnavailable);
        }
    }

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
            discount_value = COALESCE($9, discount_value),
            tax_value = COALESCE($10, tax_value),
            fee_breakdown = COALESCE($11, fee_breakdown),
            total_price = COALESCE($12, total_price),
            cleaning_fee = COALESCE($13, cleaning_fee),
            damage_deposit = COALESCE($14, damage_deposit),
            platform_fee = COALESCE($15, platform_fee),
            tax_rate = COALESCE($16, tax_rate),
            exchange_rate = COALESCE($17, exchange_rate),
            updated_at = $18,
            number_of_persons = COALESCE($20, number_of_persons)
        WHERE id = $19
        RETURNING id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus", 
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy", 
            metadata as "metadata: Json<crate::models::BookingMetadata>",
            door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
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
        updated_booking.number_of_persons
    )
    .fetch_one(&mut *tx)
    .await?;

    // Record history if anything changed
    let status_changed = updated_booking
        .status
        .map(|s| s != current.status)
        .unwrap_or(false);
    let metadata_changed = updated_booking.metadata.is_some();
    let door_code_changed = updated_booking.door_access_code.is_some();

    let history_id = if status_changed || metadata_changed || door_code_changed || terms_changed {
        let reason = if dates_changed {
            "Date change on confirmed reservation"
        } else if guests_changed {
            "Guest count change on confirmed reservation"
        } else {
            match (status_changed, metadata_changed, door_code_changed) {
                (true, true, _) => "Status and metadata updated",
                (true, false, false) => "Status updated",
                (false, true, false) => "Metadata updated",
                (false, false, true) => "Door access code updated",
                _ => "Booking updated",
            }
        };
        Some(record_booking_history(&mut tx, &booking, reason, None).await?)
    } else {
        None
    };

    // Trigger review token generation if booking transitions to Completed
    if let Some(BookingStatus::Completed) = updated_booking.status
        && status_changed
    {
        crate::review::create_review_token(&mut tx, booking.id).await?;
    }

    // Trigger payment receipt issuance and host payout ledger generation if booking transitions to Confirmed
    if let Some(BookingStatus::Confirmed) = updated_booking.status
        && status_changed
    {
        crate::tax_document::issue_payment_receipt(&mut tx, &booking, supplier, history_id).await?;

        let terms = host_payout_terms(&mut tx, &booking).await?;
        let gross_amount = (booking.accommodation_net() * terms.exchange_rate).round_dp(2);
        crate::payout_ledger::create_payout_ledger_entry(
            &mut tx,
            &terms.pending_payout(&booking, gross_amount),
        )
        .await?;
    }

    // Trigger revised receipt issuance and update pending host payout ledger if confirmed booking terms changed
    if terms_changed && current.status == BookingStatus::Confirmed {
        crate::tax_document::issue_payment_receipt(&mut tx, &booking, supplier, history_id).await?;

        let terms = host_payout_terms(&mut tx, &booking).await?;
        let gross_amount = (booking.accommodation_net() * terms.exchange_rate).round_dp(2);
        let platform_fee_pct = terms.commission_pct;
        let (platform_fee_amount, net_payout_amount) = common::pricing::calculate_host_payout(
            gross_amount,
            platform_fee_pct,
            rust_decimal::Decimal::ZERO,
        );

        sqlx::query!(
            r#"
            UPDATE host_payout_ledger
            SET gross_amount = $1,
                platform_fee_amount = $2,
                net_payout_amount = $3,
                exchange_rate = $4,
                updated_at = NOW()
            WHERE booking_id = $5 AND status = 'pending'::payout_status
            "#,
            gross_amount,
            platform_fee_amount,
            net_payout_amount,
            terms.exchange_rate,
            booking.id
        )
        .execute(&mut *tx)
        .await?;
    }

    // Handle cancellation: issue refund receipt and adjust host payout ledger if transitioning from Confirmed
    if let Some(BookingStatus::Cancelled) = updated_booking.status
        && status_changed
    {
        sqlx::query!(
            r#"
            UPDATE reservation_change_request
            SET status = 'cancelled'::reservation_change_status,
                updated_at = NOW()
            WHERE booking_id = $1 AND status = 'pending'::reservation_change_status
            "#,
            booking.id
        )
        .execute(&mut *tx)
        .await?;

        if current.status == BookingStatus::Confirmed {
            let ref_doc =
                crate::tax_document::issue_refund_receipt(&mut tx, &booking, supplier, history_id)
                    .await?;

            crate::payout_ledger::cancel_pending_payout_for_booking(&mut tx, booking.id).await?;

            // If accommodation was partially retained by host (policy_pct < 1.00), issue adjusted payout
            let accommodation_net = booking.accommodation_net();
            let host_retained = (accommodation_net - ref_doc.accommodation_net).round_dp(2);

            if host_retained > Decimal::ZERO {
                let terms = host_payout_terms(&mut tx, &booking).await?;
                let retained_gross = (host_retained * terms.exchange_rate).round_dp(2);
                crate::payout_ledger::create_payout_ledger_entry(
                    &mut tx,
                    &terms.pending_payout(&booking, retained_gross),
                )
                .await?;
            }
        } else {
            crate::payout_ledger::cancel_pending_payout_for_booking(&mut tx, booking.id).await?;
        }
    }

    // Handle completion: issue statutory tax invoice (INV-YYYY-{code}) if transitioning to Completed
    if let Some(BookingStatus::Completed) = updated_booking.status
        && status_changed
    {
        sqlx::query!(
            r#"
            UPDATE reservation_change_request
            SET status = 'expired'::reservation_change_status,
                updated_at = NOW()
            WHERE booking_id = $1 AND status = 'pending'::reservation_change_status
            "#,
            booking.id
        )
        .execute(&mut *tx)
        .await?;

        crate::tax_document::issue_tax_invoice(&mut tx, &booking, supplier, history_id).await?;
    }

    tx.commit().await?;

    Ok(booking)
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

    let exchange_rate =
        crate::exchange_rate::get_rate(conn, &booking.currency, &host_currency, Utc::now())
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
                        "This booking has tax documents or payout records and cannot be deleted; cancel it instead".to_string(),
                    )
                }
                _ => e.into(),
            })?;

    if result.rows_affected() == 0 {
        return Err(crate::error::DbError::Sqlx(sqlx::Error::RowNotFound));
    }

    Ok(())
}
