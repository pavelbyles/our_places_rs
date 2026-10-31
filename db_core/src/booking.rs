use crate::error::Result;
#[allow(unused_imports)]
use crate::models::{
    Booking, BookingHistory, BookingStatus, CancellationPolicy, FeeItem, NewBooking, SupplierInfo,
    UpdatedBooking,
};
#[allow(unused_imports)]
use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
#[allow(unused_imports)]
use sqlx::types::Json;

use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

/// Creates a new booking in the database.
#[tracing::instrument(skip(pool))]
pub async fn create_booking(pool: &PgPool, new_booking: &NewBooking) -> Result<Booking> {
    let mut tx = pool.begin().await?;

    let _listing = sqlx::query!(
        "SELECT id FROM listing WHERE id = $1 FOR UPDATE",
        new_booking.listing_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(crate::error::DbError::Sqlx(sqlx::Error::RowNotFound))?;

    let overlapping = sqlx::query!(
        r#"
        SELECT id, guest_id, status as "status: BookingStatus" FROM booking 
        WHERE listing_id = $1 
          AND status IN ('pending', 'confirmed') 
          AND date_from < $3 
          AND date_to > $2
        "#,
        new_booking.listing_id,
        new_booking.date_from,
        new_booking.date_to
    )
    .fetch_all(&mut *tx)
    .await?;

    let has_conflict = overlapping.iter().any(|b| {
        b.status == BookingStatus::Confirmed
            || (b.status == BookingStatus::Pending && b.guest_id != new_booking.guest_id)
    });

    if has_conflict {
        return Err(crate::error::DbError::ValidationError(
            "Listing is not available for the selected dates".to_string(),
        ));
    }

    // Cancel any existing pending holds for this guest across all listings
    let existing_guest_holds = sqlx::query_as!(
        Booking,
        r#"
        UPDATE booking
        SET status = 'cancelled', updated_at = $2
        WHERE guest_id = $1 AND status = 'pending'
        RETURNING id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus", 
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy", 
            metadata as "metadata: Json<crate::models::BookingMetadata>",
            door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            created_at, updated_at
        "#,
        new_booking.guest_id,
        Utc::now()
    )
    .fetch_all(&mut *tx)
    .await?;

    for cancelled in existing_guest_holds {
        record_booking_history(
            &mut tx,
            &cancelled,
            "Cancelled previous pending hold due to new booking placement",
            Some(new_booking.guest_id),
        )
        .await?;
    }

    let booking = sqlx::query_as!(
        Booking,
        r#"
        INSERT INTO booking (
            id, confirmation_code, guest_id, listing_id, status,
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown,
            total_price, cancellation_policy, metadata, door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26)
        RETURNING id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus", 
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy", 
            metadata as "metadata: Json<crate::models::BookingMetadata>",
            door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            created_at, updated_at
        "#,
        Uuid::now_v7(),
        new_booking.confirmation_code,
        new_booking.guest_id,
        new_booking.listing_id,
        BookingStatus::Pending as BookingStatus,
        new_booking.date_from,
        new_booking.date_to,
        new_booking.currency,
        new_booking.daily_rate,
        new_booking.number_of_persons,
        new_booking.total_days,
        new_booking.sub_total_price,
        new_booking.discount_value,
        new_booking.tax_value,
        Json(&new_booking.fee_breakdown) as _,
        new_booking.total_price,
        new_booking.cancellation_policy as CancellationPolicy,
        Json(&new_booking.metadata) as _,
        new_booking.door_access_code.as_deref(),
        new_booking.cleaning_fee,
        new_booking.damage_deposit,
        new_booking.platform_fee,
        new_booking.tax_rate,
        new_booking.exchange_rate,
        Utc::now(),
        Utc::now()
    )
    .fetch_one(&mut *tx)
    .await?;

    // Record initial history
    record_booking_history(&mut tx, &booking, "Booking created", None).await?;

    tx.commit().await?;

    Ok(booking)
}

/// Retrieves all bookings with pagination.
#[tracing::instrument(skip(executor))]
pub async fn get_bookings<'e, E>(executor: E, page: u32, per_page: u32) -> Result<Vec<Booking>>
where
    E: PgExecutor<'e>,
{
    let limit = per_page as i64;
    let offset = ((page.max(1) - 1) * per_page) as i64;

    let bookings = sqlx::query_as!(
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
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2
        "#,
        limit,
        offset
    )
    .fetch_all(executor)
    .await?;

    Ok(bookings)
}

/// Retrieves a single booking by ID.
#[tracing::instrument(skip(executor))]
pub async fn get_booking_by_id<'e, E>(executor: E, id: Uuid) -> Result<Booking>
where
    E: PgExecutor<'e>,
{
    let booking = sqlx::query_as!(
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
        "#,
        id
    )
    .fetch_one(executor)
    .await?;

    Ok(booking)
}

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
        r#"SELECT id, listing_id, status as "status: BookingStatus", date_from, date_to, number_of_persons FROM booking WHERE id = $1 FOR UPDATE"#,
        id
    )
    .fetch_one(&mut *tx)
    .await?;

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

        let listing_info = sqlx::query!(
            r#"SELECT user_id, commission_pct, base_currency FROM listing WHERE id = $1"#,
            booking.listing_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let host_currency = sqlx::query_scalar!(
            r#"SELECT default_currency FROM "user" WHERE id = $1"#,
            listing_info.user_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let exchange_rate = if listing_info.base_currency == host_currency {
            rust_decimal::Decimal::ONE
        } else {
            sqlx::query_scalar!(
                r#"SELECT rate FROM currency_exchange_rates 
                   WHERE base_currency = $1 AND target_currency = $2 
                   ORDER BY effective_at DESC, updated_at DESC LIMIT 1"#,
                listing_info.base_currency,
                host_currency
            )
            .fetch_optional(&mut *tx)
            .await?
            .unwrap_or(rust_decimal::Decimal::ONE)
        };

        let discounted_subtotal = booking.accommodation_net();
        let gross_amount = (discounted_subtotal * exchange_rate).round_dp(2);
        let platform_fee_pct = listing_info.commission_pct;
        let (platform_fee_amount, net_payout_amount) = common::pricing::calculate_host_payout(
            gross_amount,
            platform_fee_pct,
            rust_decimal::Decimal::ZERO,
        );

        crate::payout_ledger::create_payout_ledger_entry(
            &mut tx,
            uuid::Uuid::now_v7(),
            booking.id,
            booking.listing_id,
            listing_info.user_id,
            &host_currency,
            gross_amount,
            platform_fee_pct,
            platform_fee_amount,
            rust_decimal::Decimal::ZERO,
            exchange_rate,
            net_payout_amount,
        )
        .await?;
    }

    // Trigger revised receipt issuance and update pending host payout ledger if confirmed booking terms changed
    if terms_changed && current.status == BookingStatus::Confirmed {
        crate::tax_document::issue_payment_receipt(&mut tx, &booking, supplier, history_id).await?;

        let listing_info = sqlx::query!(
            r#"SELECT user_id, commission_pct, base_currency FROM listing WHERE id = $1"#,
            booking.listing_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let host_currency = sqlx::query_scalar!(
            r#"SELECT default_currency FROM "user" WHERE id = $1"#,
            listing_info.user_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let exchange_rate = if listing_info.base_currency == host_currency {
            rust_decimal::Decimal::ONE
        } else {
            sqlx::query_scalar!(
                r#"SELECT rate FROM currency_exchange_rates 
                   WHERE base_currency = $1 AND target_currency = $2 
                   ORDER BY effective_at DESC, updated_at DESC LIMIT 1"#,
                listing_info.base_currency,
                host_currency
            )
            .fetch_optional(&mut *tx)
            .await?
            .unwrap_or(rust_decimal::Decimal::ONE)
        };

        let discounted_subtotal = booking.accommodation_net();
        let gross_amount = (discounted_subtotal * exchange_rate).round_dp(2);
        let platform_fee_pct = listing_info.commission_pct;
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
            exchange_rate,
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
                let listing_info = sqlx::query!(
                    r#"SELECT user_id, commission_pct, base_currency FROM listing WHERE id = $1"#,
                    booking.listing_id
                )
                .fetch_one(&mut *tx)
                .await?;

                let host_currency = sqlx::query_scalar!(
                    r#"SELECT default_currency FROM "user" WHERE id = $1"#,
                    listing_info.user_id
                )
                .fetch_one(&mut *tx)
                .await?;

                let exchange_rate = if listing_info.base_currency == host_currency {
                    Decimal::ONE
                } else {
                    sqlx::query_scalar!(
                        r#"SELECT rate FROM currency_exchange_rates 
                           WHERE base_currency = $1 AND target_currency = $2 
                           ORDER BY effective_at DESC, updated_at DESC LIMIT 1"#,
                        listing_info.base_currency,
                        host_currency
                    )
                    .fetch_optional(&mut *tx)
                    .await?
                    .unwrap_or(Decimal::ONE)
                };

                let retained_gross = (host_retained * exchange_rate).round_dp(2);
                let platform_fee_pct = listing_info.commission_pct;
                let (platform_fee_amount, net_payout_amount) =
                    common::pricing::calculate_host_payout(
                        retained_gross,
                        platform_fee_pct,
                        Decimal::ZERO,
                    );

                crate::payout_ledger::create_payout_ledger_entry(
                    &mut tx,
                    uuid::Uuid::now_v7(),
                    booking.id,
                    booking.listing_id,
                    listing_info.user_id,
                    &host_currency,
                    retained_gross,
                    platform_fee_pct,
                    platform_fee_amount,
                    Decimal::ZERO,
                    exchange_rate,
                    net_payout_amount,
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

/// Deletes a booking (Hard Delete).
#[tracing::instrument(skip(pool))]
pub async fn delete_booking(pool: &PgPool, id: Uuid) -> Result<()> {
    let result: sqlx::postgres::PgQueryResult =
        sqlx::query!("DELETE FROM booking WHERE id = $1", id)
            .execute(pool)
            .await?;

    if result.rows_affected() == 0 {
        return Err(crate::error::DbError::Sqlx(sqlx::Error::RowNotFound));
    }

    Ok(())
}

/// Transfers a pending booking placeholder to a newly authenticated user.
#[tracing::instrument(skip(pool))]
pub async fn transfer_booking_guest(
    pool: &PgPool,
    booking_id: Uuid,
    new_guest_id: Uuid,
) -> Result<Booking> {
    let mut tx = pool.begin().await?;

    let booking = sqlx::query_as!(
        Booking,
        r#"
        UPDATE booking
        SET guest_id = $1,
            updated_at = $2
        WHERE id = $3 AND status = 'pending'
        RETURNING id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus", 
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy", 
            metadata as "metadata: Json<crate::models::BookingMetadata>",
            door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            created_at, updated_at
        "#,
        new_guest_id,
        Utc::now(),
        booking_id
    )
    .fetch_one(&mut *tx)
    .await?;

    record_booking_history(
        &mut tx,
        &booking,
        "Transferred booking from guest placeholder to authenticated user",
        Some(new_guest_id),
    )
    .await?;

    tx.commit().await?;

    Ok(booking)
}

/// Retrieves all confirmed bookings whose checkout date has passed (`date_to <= cutoff_date`).
#[tracing::instrument(skip(executor))]
pub async fn get_confirmed_bookings_past_checkout<'e, E>(
    executor: E,
    cutoff_date: NaiveDate,
) -> Result<Vec<Booking>>
where
    E: PgExecutor<'e>,
{
    let records = sqlx::query_as!(
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
        WHERE status = 'confirmed'::booking_status
          AND date_to <= $1
        ORDER BY date_to ASC, id ASC
        "#,
        cutoff_date
    )
    .fetch_all(executor)
    .await?;

    Ok(records)
}

/// Retrieves bookings for a specific user, sorted by date_from ASC.
#[tracing::instrument(skip(executor))]
pub async fn get_bookings_by_user_id<'e, E>(
    executor: E,
    guest_id: Uuid,
    page: u32,
    per_page: u32,
) -> Result<Vec<crate::models::BookingWithEligibility>>
where
    E: PgExecutor<'e>,
{
    let limit = per_page as i64;
    let offset = ((page.max(1) - 1) * per_page) as i64;

    let records = sqlx::query!(
        r#"
        SELECT b.id, b.confirmation_code, b.guest_id, b.listing_id, b.status as "status: BookingStatus", 
            b.date_from, b.date_to, b.currency, b.daily_rate, b.number_of_persons, b.total_days,
            b.sub_total_price, b.discount_value, b.tax_value, b.fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>",
            b.total_price, b.cancellation_policy as "cancellation_policy: CancellationPolicy", 
            b.metadata as "metadata: Json<crate::models::BookingMetadata>",
            json_build_object(
                'booking_id', b.id,
                'is_eligible', 
                    CASE 
                        WHEN b.status = 'cancelled' THEN false
                        WHEN r.id IS NOT NULL THEN false
                        WHEN CURRENT_DATE < b.date_to THEN false
                        WHEN CURRENT_DATE > (b.date_to + 15) THEN false
                        ELSE true
                    END,
                'token', rt.token,
                'has_reviewed', CASE WHEN r.id IS NOT NULL THEN true ELSE false END,
                'days_remaining', 
                    CASE 
                        WHEN CURRENT_DATE > (b.date_to + 15) THEN 0
                        ELSE (b.date_to + 15) - CURRENT_DATE
                    END,
                'status_message', 
                    CASE 
                        WHEN b.status = 'cancelled' THEN 'Cancelled bookings are not eligible for review'
                        WHEN r.id IS NOT NULL THEN 'You have already submitted a review for this stay'
                        WHEN CURRENT_DATE < b.date_to THEN 'Reviews can only be submitted after your stay has ended'
                        WHEN CURRENT_DATE > (b.date_to + 15) THEN 'The 15-day review period for this stay has expired'
                        ELSE 'Eligible for review'
                    END
            ) as "review_eligibility!",
            b.created_at, b.updated_at,
            b.door_access_code,
            b.cleaning_fee, b.damage_deposit, b.platform_fee, b.tax_rate, b.exchange_rate
        FROM booking b
        LEFT JOIN review r ON r.booking_id = b.id
        LEFT JOIN LATERAL (
            SELECT token FROM review_token 
            WHERE booking_id = b.id AND used_at IS NULL AND expires_at > NOW() 
            ORDER BY created_at DESC LIMIT 1
        ) rt ON true
        WHERE b.guest_id = $1
        ORDER BY b.date_from ASC
        LIMIT $2 OFFSET $3
        "#,
        guest_id,
        limit,
        offset
    )
    .fetch_all(executor)
    .await?;

    let mut bookings = Vec::with_capacity(records.len());
    for r in records {
        let booking = Booking {
            id: r.id,
            confirmation_code: r.confirmation_code,
            guest_id: r.guest_id,
            listing_id: r.listing_id,
            status: r.status,
            date_from: r.date_from,
            date_to: r.date_to,
            currency: r.currency,
            daily_rate: r.daily_rate,
            number_of_persons: r.number_of_persons,
            total_days: r.total_days,
            sub_total_price: r.sub_total_price,
            discount_value: r.discount_value,
            tax_value: r.tax_value,
            fee_breakdown: r.fee_breakdown,
            total_price: r.total_price,
            cancellation_policy: r.cancellation_policy,
            metadata: r.metadata,
            door_access_code: r.door_access_code,
            cleaning_fee: r.cleaning_fee,
            damage_deposit: r.damage_deposit,
            platform_fee: r.platform_fee,
            tax_rate: r.tax_rate,
            exchange_rate: r.exchange_rate,
            created_at: r.created_at,
            updated_at: r.updated_at,
        };

        let eligibility = serde_json::from_value::<common::models::BookingReviewEligibility>(
            r.review_eligibility,
        )
        .ok();

        bookings.push(crate::models::BookingWithEligibility {
            booking,
            review_eligibility: eligibility,
        });
    }

    Ok(bookings)
}

/// Retrieves bookings for a specific listing, sorted by date_from DESC, created_at DESC.
#[tracing::instrument(skip(executor))]
pub async fn get_bookings_by_listing_id<'e, E>(
    executor: E,
    listing_id: Uuid,
    page: u32,
    per_page: u32,
) -> Result<Vec<Booking>>
where
    E: PgExecutor<'e>,
{
    let limit = per_page as i64;
    let offset = ((page.max(1) - 1) * per_page) as i64;

    let bookings = sqlx::query_as!(
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
        WHERE listing_id = $1
        ORDER BY date_from DESC, created_at DESC
        LIMIT $2 OFFSET $3
        "#,
        listing_id,
        limit,
        offset
    )
    .fetch_all(executor)
    .await?;

    Ok(bookings)
}

/// Checks if a listing is available for a specified date range.
#[tracing::instrument(skip(executor))]
pub async fn check_availability<'e, E>(
    executor: E,
    listing_id: Uuid,
    date_from: chrono::NaiveDate,
    date_to: chrono::NaiveDate,
) -> Result<bool>
where
    E: PgExecutor<'e>,
{
    if date_from >= date_to {
        return Ok(false);
    }

    let overlapping = sqlx::query!(
        r#"
        SELECT count(*) as count FROM booking 
        WHERE listing_id = $1 
          AND status IN ('pending', 'confirmed') 
          AND date_from < $3 
          AND date_to > $2
        "#,
        listing_id,
        date_from,
        date_to
    )
    .fetch_one(executor)
    .await?;

    Ok(overlapping.count == Some(0))
}

/// Checks if a listing is available for a specified date range, excluding a specific booking.
#[tracing::instrument(skip(executor))]
pub async fn check_availability_excluding<'e, E>(
    executor: E,
    listing_id: Uuid,
    date_from: chrono::NaiveDate,
    date_to: chrono::NaiveDate,
    exclude_booking_id: Uuid,
) -> Result<bool>
where
    E: PgExecutor<'e>,
{
    if date_from >= date_to {
        return Ok(false);
    }

    let overlapping = sqlx::query!(
        r#"
        SELECT count(*) as count FROM booking 
        WHERE listing_id = $1 
          AND status IN ('pending', 'confirmed') 
          AND date_from < $3 
          AND date_to > $2
          AND id != $4
        "#,
        listing_id,
        date_from,
        date_to,
        exclude_booking_id
    )
    .fetch_one(executor)
    .await?;

    Ok(overlapping.count == Some(0))
}

/// Retrieves the history of a specific booking.
#[tracing::instrument(skip(executor))]
pub async fn get_booking_history<'e, E>(
    executor: E,
    booking_id: Uuid,
) -> Result<Vec<BookingHistory>>
where
    E: PgExecutor<'e>,
{
    let history = sqlx::query_as!(
        BookingHistory,
        r#"
        SELECT id, booking_id, confirmation_code, guest_id, listing_id, 
            status as "status: BookingStatus", date_from, date_to, currency, 
            daily_rate, number_of_persons, total_days, sub_total_price, 
            discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<FeeItem>>", 
            total_price, cancellation_policy as "cancellation_policy: CancellationPolicy", 
            metadata as "metadata: Json<crate::models::BookingMetadata>", 
            door_access_code,
            changed_by_id, change_reason, created_at
        FROM booking_history
        WHERE booking_id = $1
        ORDER BY created_at ASC
        "#,
        booking_id
    )
    .fetch_all(executor)
    .await?;

    Ok(history)
}

pub(crate) async fn record_booking_history(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    booking: &Booking,
    change_reason: &str,
    changed_by_id: Option<Uuid>,
) -> Result<Uuid> {
    let history_id = sqlx::query_scalar!(
        r#"
        INSERT INTO booking_history (
            booking_id, confirmation_code, guest_id, listing_id, status,
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown,
            total_price, cancellation_policy, metadata, door_access_code,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            change_reason, changed_by_id
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26)
        RETURNING id
        "#,
        booking.id,
        booking.confirmation_code,
        booking.guest_id,
        booking.listing_id,
        booking.status as BookingStatus,
        booking.date_from,
        booking.date_to,
        booking.currency,
        booking.daily_rate,
        booking.number_of_persons,
        booking.total_days,
        booking.sub_total_price,
        booking.discount_value,
        booking.tax_value,
        Json(&booking.fee_breakdown.0) as _,
        booking.total_price,
        booking.cancellation_policy as CancellationPolicy,
        Json(&booking.metadata.0) as _,
        booking.door_access_code.as_deref(),
        booking.cleaning_fee,
        booking.damage_deposit,
        booking.platform_fee,
        booking.tax_rate,
        booking.exchange_rate,
        change_reason,
        changed_by_id
    )
    .fetch_one(&mut **tx)
    .await?;

    Ok(history_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use sqlx_db_tester::TestPg;
    use std::env;
    use std::path::Path;

    async fn setup_test_db() -> TestPg {
        dotenvy::dotenv().ok();
        let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/our_places".to_string()
        });
        TestPg::new(db_url, Path::new("migrations"))
    }

    #[tokio::test]
    async fn test_guest_pending_hold_cancellation_on_new_booking() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;
        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        // Create host user with host profile
        let host_id = Uuid::now_v7();
        let host_user = crate::models::NewUser {
            id: host_id,
            email: format!("host_{}@example.com", host_id),
            password_hash: "hash".to_string(),
            first_name: "Host".to_string(),
            last_name: "User".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![crate::models::UserRole::Host]),
            default_currency: "USD".to_string(),
        };
        crate::user::create_user(&mut *tx, &host_user)
            .await
            .unwrap();

        let profile = crate::models::NewHostProfile {
            description: Some("Host".to_string()),
            verified_status: Some("pending".to_string()),
            payout_details: None,
        };
        crate::user::create_host_profile(&mut *tx, host_id, &profile)
            .await
            .unwrap();

        let nl1 = crate::models::NewListing {
            name: format!("Villa 1 {}", Uuid::now_v7()),
            user_id: host_id,
            description: None,
            listing_structure_id: 1,
            country: "Jamaica".to_string(),
            price_per_night: Some(dec!(200.00)),
            weekly_discount_percentage: None,
            monthly_discount_percentage: None,
            max_guests: 4,
            bedrooms: 2,
            beds: 2,
            full_bathrooms: 1,
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
            cleaning_fee: None,
            damage_deposit: None,
            guests_included: None,
            extra_guest_fee: None,
        };
        let listing1 = crate::listing::create_listing(&mut *tx, &nl1)
            .await
            .unwrap();

        let nl2 = crate::models::NewListing {
            name: format!("Villa 2 {}", Uuid::now_v7()),
            user_id: host_id,
            description: None,
            listing_structure_id: 1,
            country: "Jamaica".to_string(),
            price_per_night: Some(dec!(300.00)),
            weekly_discount_percentage: None,
            monthly_discount_percentage: None,
            max_guests: 4,
            bedrooms: 2,
            beds: 2,
            full_bathrooms: 1,
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
            cleaning_fee: None,
            damage_deposit: None,
            guests_included: None,
            extra_guest_fee: None,
        };
        let listing2 = crate::listing::create_listing(&mut *tx, &nl2)
            .await
            .unwrap();

        let guest_id = Uuid::now_v7();
        let guest_user = crate::models::NewUser {
            id: guest_id,
            email: format!("guest_{}@example.com", guest_id),
            password_hash: "hash".to_string(),
            first_name: "Guest".to_string(),
            last_name: "User".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![crate::models::UserRole::Booker]),
            default_currency: "USD".to_string(),
        };
        crate::user::create_user(&mut *tx, &guest_user)
            .await
            .unwrap();

        tx.commit().await.unwrap();

        // Guest places Booking 1 on Villa 1
        let code1 = format!("C{}", Uuid::now_v7().simple())[..10].to_uppercase();
        let new_booking1 = NewBooking {
            confirmation_code: code1,
            guest_id,
            listing_id: listing1.id,
            date_from: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
            date_to: NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
            currency: "USD".to_string(),
            daily_rate: dec!(200.00),
            number_of_persons: 2,
            total_days: 4,
            sub_total_price: dec!(800.00),
            discount_value: None,
            tax_value: None,
            fee_breakdown: vec![],
            total_price: dec!(800.00),
            cancellation_policy: CancellationPolicy::Flexible,
            metadata: crate::models::BookingMetadata {
                num_adults: 2,
                ..Default::default()
            },
            door_access_code: None,
            cleaning_fee: Decimal::ZERO,
            damage_deposit: Decimal::ZERO,
            platform_fee: Decimal::ZERO,
            tax_rate: dec!(0.1000),
            exchange_rate: Decimal::ONE,
        };

        let b1 = create_booking(&pool, &new_booking1)
            .await
            .expect("Failed to create booking 1");
        assert_eq!(b1.status, BookingStatus::Pending);

        // Guest places Booking 2 on Villa 2
        let code2 = format!("D{}", Uuid::now_v7().simple())[..10].to_uppercase();
        let new_booking2 = NewBooking {
            confirmation_code: code2,
            guest_id,
            listing_id: listing2.id,
            date_from: NaiveDate::from_ymd_opt(2026, 11, 10).unwrap(),
            date_to: NaiveDate::from_ymd_opt(2026, 11, 15).unwrap(),
            currency: "USD".to_string(),
            daily_rate: dec!(300.00),
            number_of_persons: 2,
            total_days: 5,
            sub_total_price: dec!(1500.00),
            discount_value: None,
            tax_value: None,
            fee_breakdown: vec![],
            total_price: dec!(1500.00),
            cancellation_policy: CancellationPolicy::Flexible,
            metadata: crate::models::BookingMetadata {
                num_adults: 2,
                ..Default::default()
            },
            door_access_code: None,
            cleaning_fee: Decimal::ZERO,
            damage_deposit: Decimal::ZERO,
            platform_fee: Decimal::ZERO,
            tax_rate: dec!(0.1000),
            exchange_rate: Decimal::ONE,
        };

        let b2 = create_booking(&pool, &new_booking2)
            .await
            .expect("Failed to create booking 2");
        assert_eq!(b2.status, BookingStatus::Pending);

        // Verify Booking 1 on Villa 1 was automatically CANCELLED
        let b1_updated = get_booking_by_id(&pool, b1.id).await.unwrap();
        assert_eq!(b1_updated.status, BookingStatus::Cancelled);

        // Test get_bookings_by_listing_id for listing1
        let listing1_bookings = get_bookings_by_listing_id(&pool, listing1.id, 1, 10)
            .await
            .expect("Failed to fetch bookings for listing1");
        assert_eq!(listing1_bookings.len(), 1);
        assert_eq!(listing1_bookings[0].id, b1.id);

        // Test get_bookings_by_listing_id for listing2
        let listing2_bookings = get_bookings_by_listing_id(&pool, listing2.id, 1, 10)
            .await
            .expect("Failed to fetch bookings for listing2");
        assert_eq!(listing2_bookings.len(), 1);
        assert_eq!(listing2_bookings[0].id, b2.id);
    }

    #[tokio::test]
    async fn test_update_booking_confirmed_issues_payment_receipt() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;

        let host_id = Uuid::now_v7();
        let guest_id = Uuid::now_v7();

        let mut tx = pool.begin().await.unwrap();

        let host_user = crate::models::NewUser {
            id: host_id,
            email: format!("host_{}@example.com", host_id),
            password_hash: "hash".to_string(),
            first_name: "TaxHost".to_string(),
            last_name: "Owner".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![crate::models::UserRole::Host]),
            default_currency: "USD".to_string(),
        };
        crate::user::create_user(&mut *tx, &host_user)
            .await
            .unwrap();

        let profile = crate::models::NewHostProfile {
            description: Some("Host".to_string()),
            verified_status: Some("pending".to_string()),
            payout_details: None,
        };
        crate::user::create_host_profile(&mut *tx, host_id, &profile)
            .await
            .unwrap();

        let guest_user = crate::models::NewUser {
            id: guest_id,
            email: format!("guest_{}@example.com", guest_id),
            password_hash: "hash".to_string(),
            first_name: "TaxGuest".to_string(),
            last_name: "Booker".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![crate::models::UserRole::Booker]),
            default_currency: "USD".to_string(),
        };
        crate::user::create_user(&mut *tx, &guest_user)
            .await
            .unwrap();

        let listing = crate::listing::create_listing(
            &mut *tx,
            &crate::models::NewListing {
                name: format!("Receipt Test Villa {}", Uuid::now_v7()),
                user_id: host_id,
                description: None,
                listing_structure_id: 1,
                country: "JM".to_string(),
                price_per_night: Some(dec!(200.00)),
                weekly_discount_percentage: None,
                monthly_discount_percentage: None,
                max_guests: 4,
                bedrooms: 2,
                beds: 2,
                full_bathrooms: 1,
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
                guests_included: None,
                extra_guest_fee: None,
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

        let code = format!("R{}", Uuid::now_v7().simple())[..10].to_uppercase();
        let new_booking = NewBooking {
            confirmation_code: code.clone(),
            guest_id,
            listing_id: listing.id,
            date_from: NaiveDate::from_ymd_opt(2026, 12, 1).unwrap(),
            date_to: NaiveDate::from_ymd_opt(2026, 12, 5).unwrap(),
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
            metadata: crate::models::BookingMetadata {
                num_adults: 2,
                business_company_name: Some("Global Corp".to_string()),
                business_tax_id: Some("999-888-777".to_string()),
                ..Default::default()
            },
            door_access_code: None,
            cleaning_fee: dec!(50.00),
            damage_deposit: dec!(100.00),
            platform_fee: dec!(40.00),
            tax_rate: dec!(0.1500),
            exchange_rate: Decimal::ONE,
        };

        let booking = create_booking(&pool, &new_booking).await.unwrap();
        assert_eq!(booking.status, BookingStatus::Pending);

        let supplier = SupplierInfo {
            supplier_name: "Our Places Jamaica Limited".to_string(),
            supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
            supplier_trn: "000-111-222".to_string(),
        };

        // Transition from Pending to Confirmed
        let updated = UpdatedBooking {
            status: Some(BookingStatus::Confirmed),
            ..Default::default()
        };

        let confirmed_booking = update_booking(&pool, booking.id, &updated, &supplier)
            .await
            .expect("Failed to confirm booking");
        assert_eq!(confirmed_booking.status, BookingStatus::Confirmed);

        // Verify that exactly one payment receipt document was created
        let docs = crate::tax_document::list_for_booking(&pool, booking.id)
            .await
            .expect("Failed to list tax documents");
        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert_eq!(
            doc.document_type,
            crate::models::TaxDocumentType::PaymentReceipt
        );
        assert_eq!(doc.total_amount, dec!(1077.50));
        assert_eq!(doc.jmd_total_amount, dec!(172400.00)); // 1077.50 * 160
        assert_eq!(doc.recipient_company_name.as_deref(), Some("Global Corp"));
        assert_eq!(doc.recipient_trn.as_deref(), Some("999-888-777"));
        assert_eq!(doc.document_number, format!("REC-2026-{}", code));

        // Transition from Confirmed to Cancelled
        let cancel_update = UpdatedBooking {
            status: Some(BookingStatus::Cancelled),
            ..Default::default()
        };
        let cancelled_booking = update_booking(&pool, booking.id, &cancel_update, &supplier)
            .await
            .expect("Failed to cancel confirmed booking");
        assert_eq!(cancelled_booking.status, BookingStatus::Cancelled);

        // Verify that a refund receipt document was created
        let docs_after_cancel = crate::tax_document::list_for_booking(&pool, booking.id)
            .await
            .expect("Failed to list tax documents after cancel");
        assert_eq!(docs_after_cancel.len(), 2);
        let ref_doc = docs_after_cancel
            .iter()
            .find(|d| d.document_type == crate::models::TaxDocumentType::RefundReceipt)
            .expect("Refund receipt document not found");
        assert_eq!(ref_doc.document_number, format!("REF-2026-{}", code));
        assert!(ref_doc.refund_pct.is_some());
        assert!(ref_doc.retained_amount.is_some());
        assert_eq!(
            ref_doc.total_amount + ref_doc.retained_amount.unwrap(),
            booking.total_price
        );

        // Cleanup created data to preserve pagination baseline for other tests
        let _ = sqlx::query!("DELETE FROM tax_document WHERE booking_id = $1", booking.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query!(
            "DELETE FROM host_payout_ledger WHERE booking_id = $1",
            booking.id
        )
        .execute(&pool)
        .await;
        let _ = sqlx::query!(
            "DELETE FROM booking_history WHERE booking_id = $1",
            booking.id
        )
        .execute(&pool)
        .await;
        let _ = sqlx::query!("DELETE FROM booking WHERE id = $1", booking.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query!("DELETE FROM listing WHERE id = $1", listing.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query!("DELETE FROM host_profiles WHERE user_id = $1", host_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query!(
            "DELETE FROM \"user\" WHERE id = $1 OR id = $2",
            host_id,
            guest_id
        )
        .execute(&pool)
        .await;
    }

    #[tokio::test]
    async fn test_update_booking_date_change_and_revised_receipt() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;
        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        let host_id = Uuid::now_v7();
        let host_user = crate::models::NewUser {
            id: host_id,
            email: format!("host_{}@example.com", host_id),
            password_hash: "hash".to_string(),
            first_name: "Host".to_string(),
            last_name: "User".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![crate::models::UserRole::Host]),
            default_currency: "USD".to_string(),
        };
        crate::user::create_user(&mut *tx, &host_user)
            .await
            .unwrap();

        let profile = crate::models::NewHostProfile {
            description: Some("Host".to_string()),
            verified_status: Some("pending".to_string()),
            payout_details: None,
        };
        crate::user::create_host_profile(&mut *tx, host_id, &profile)
            .await
            .unwrap();

        let nl = crate::models::NewListing {
            name: format!("Date Change Villa {}", Uuid::now_v7()),
            user_id: host_id,
            description: None,
            listing_structure_id: 1,
            country: "Jamaica".to_string(),
            price_per_night: Some(dec!(200.00)),
            weekly_discount_percentage: None,
            monthly_discount_percentage: None,
            max_guests: 4,
            bedrooms: 2,
            beds: 2,
            full_bathrooms: 1,
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
            guests_included: None,
            extra_guest_fee: None,
        };
        let listing = crate::listing::create_listing(&mut *tx, &nl).await.unwrap();

        let guest_id = Uuid::now_v7();
        let guest_user = crate::models::NewUser {
            id: guest_id,
            email: format!("guest_{}@example.com", guest_id),
            password_hash: "hash".to_string(),
            first_name: "Guest".to_string(),
            last_name: "User".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![crate::models::UserRole::Booker]),
            default_currency: "USD".to_string(),
        };
        crate::user::create_user(&mut *tx, &guest_user)
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

        let code = format!("R{}", Uuid::new_v4().simple())[..10].to_uppercase();
        let new_booking = NewBooking {
            confirmation_code: code.clone(),
            guest_id,
            listing_id: listing.id,
            date_from: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
            date_to: NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
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
            metadata: crate::models::BookingMetadata {
                num_adults: 2,
                ..Default::default()
            },
            door_access_code: None,
            cleaning_fee: dec!(50.00),
            damage_deposit: dec!(100.00),
            platform_fee: dec!(40.00),
            tax_rate: dec!(0.1500),
            exchange_rate: Decimal::ONE,
        };

        let booking = create_booking(&pool, &new_booking).await.unwrap();

        let supplier = SupplierInfo {
            supplier_name: "Our Places Jamaica Limited".to_string(),
            supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
            supplier_trn: "000-111-222".to_string(),
        };

        // Confirm booking -> Issues R1 (PaymentReceipt)
        let confirm_update = UpdatedBooking {
            status: Some(BookingStatus::Confirmed),
            ..Default::default()
        };
        let confirmed = update_booking(&pool, booking.id, &confirm_update, &supplier)
            .await
            .expect("Failed to confirm booking");
        assert_eq!(confirmed.status, BookingStatus::Confirmed);

        let initial_docs = crate::tax_document::list_for_booking(&pool, booking.id)
            .await
            .unwrap();
        assert_eq!(initial_docs.len(), 1);
        assert_eq!(initial_docs[0].revision, 1);
        assert_eq!(
            initial_docs[0].document_number,
            format!("REC-2026-{}", code)
        );
        assert_eq!(
            initial_docs[0].status,
            crate::models::TaxDocumentStatus::Issued
        );

        // Date Change: extend stay by 2 days (Nov 1 - Nov 7, 6 nights)
        let date_change = UpdatedBooking {
            date_from: Some(NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()),
            date_to: Some(NaiveDate::from_ymd_opt(2026, 11, 7).unwrap()),
            daily_rate: Some(dec!(200.00)),
            total_days: Some(6),
            sub_total_price: Some(dec!(1200.00)),
            discount_value: None,
            tax_value: Some(dec!(187.50)),
            fee_breakdown: Some(vec![]),
            total_price: Some(dec!(1537.50)),
            cleaning_fee: Some(dec!(50.00)),
            damage_deposit: Some(dec!(100.00)),
            platform_fee: Some(dec!(60.00)),
            tax_rate: Some(dec!(0.1500)),
            exchange_rate: Some(Decimal::ONE),
            ..Default::default()
        };

        let updated_booking = update_booking(&pool, booking.id, &date_change, &supplier)
            .await
            .expect("Failed to update booking dates");
        assert_eq!(updated_booking.total_days, 6);
        assert_eq!(updated_booking.total_price, dec!(1537.50));
        assert_eq!(
            updated_booking.date_to,
            NaiveDate::from_ymd_opt(2026, 11, 7).unwrap()
        );

        // Verify revised receipt: R1 is superseded, R2 is active with amount_previously_paid and balance_due
        let docs = crate::tax_document::list_for_booking(&pool, booking.id)
            .await
            .unwrap();
        assert_eq!(docs.len(), 2);

        let r1 = docs.iter().find(|d| d.revision == 1).unwrap();
        assert_eq!(r1.status, crate::models::TaxDocumentStatus::Superseded);

        let r2 = docs.iter().find(|d| d.revision == 2).unwrap();
        assert_eq!(r2.status, crate::models::TaxDocumentStatus::Issued);
        assert_eq!(r2.document_number, format!("REC-2026-{}-R2", code));
        assert_eq!(r2.amount_previously_paid, Some(dec!(1077.50)));
        assert_eq!(r2.balance_due, Some(dec!(460.00))); // 1537.50 - 1077.50
        assert_eq!(r2.supersedes_id, Some(r1.id));

        // Create a 2nd booking: Nov 10 - Nov 15
        let code2 = format!("S{}", Uuid::new_v4().simple())[..10].to_uppercase();
        let second_booking = NewBooking {
            confirmation_code: code2,
            guest_id,
            listing_id: listing.id,
            date_from: NaiveDate::from_ymd_opt(2026, 11, 10).unwrap(),
            date_to: NaiveDate::from_ymd_opt(2026, 11, 15).unwrap(),
            currency: "USD".to_string(),
            daily_rate: dec!(200.00),
            number_of_persons: 2,
            total_days: 5,
            sub_total_price: dec!(1000.00),
            discount_value: None,
            tax_value: Some(dec!(150.00)),
            fee_breakdown: vec![],
            total_price: dec!(1300.00),
            cancellation_policy: CancellationPolicy::Flexible,
            metadata: crate::models::BookingMetadata {
                num_adults: 2,
                ..Default::default()
            },
            door_access_code: None,
            cleaning_fee: dec!(50.00),
            damage_deposit: dec!(100.00),
            platform_fee: dec!(50.00),
            tax_rate: dec!(0.1500),
            exchange_rate: Decimal::ONE,
        };
        let b2 = create_booking(&pool, &second_booking).await.unwrap();
        let _ = update_booking(&pool, b2.id, &confirm_update, &supplier)
            .await
            .unwrap();

        // Attempt to change booking 1 dates to overlap with booking 2 (Nov 9 - Nov 12)
        let overlapping_change = UpdatedBooking {
            date_from: Some(NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()),
            date_to: Some(NaiveDate::from_ymd_opt(2026, 11, 12).unwrap()),
            total_days: Some(3),
            total_price: Some(dec!(900.00)),
            ..Default::default()
        };
        let err = update_booking(&pool, booking.id, &overlapping_change, &supplier)
            .await
            .unwrap_err();
        match err {
            crate::error::DbError::DateUnavailable => {}
            other => panic!("Expected DateUnavailable, got: {:?}", other),
        }

        // Attempt date change on a Pending booking -> BookingNotModifiable
        let pending_req = NewBooking {
            confirmation_code: format!("P{}", Uuid::new_v4().simple())[..10].to_uppercase(),
            guest_id,
            listing_id: listing.id,
            date_from: NaiveDate::from_ymd_opt(2026, 12, 1).unwrap(),
            date_to: NaiveDate::from_ymd_opt(2026, 12, 5).unwrap(),
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
            metadata: crate::models::BookingMetadata {
                num_adults: 2,
                ..Default::default()
            },
            door_access_code: None,
            cleaning_fee: dec!(50.00),
            damage_deposit: dec!(100.00),
            platform_fee: dec!(40.00),
            tax_rate: dec!(0.1500),
            exchange_rate: Decimal::ONE,
        };
        let pending_b = create_booking(&pool, &pending_req).await.unwrap();
        let pending_change = UpdatedBooking {
            date_from: Some(NaiveDate::from_ymd_opt(2026, 12, 2).unwrap()),
            date_to: Some(NaiveDate::from_ymd_opt(2026, 12, 6).unwrap()),
            ..Default::default()
        };
        let pending_err = update_booking(&pool, pending_b.id, &pending_change, &supplier)
            .await
            .unwrap_err();
        match pending_err {
            crate::error::DbError::BookingNotModifiable(_) => {}
            other => panic!("Expected BookingNotModifiable, got: {:?}", other),
        }

        // Clean up
        let _ = sqlx::query!(
            "DELETE FROM tax_document WHERE booking_id = $1 OR booking_id = $2 OR booking_id = $3",
            booking.id,
            b2.id,
            pending_b.id
        )
        .execute(&pool)
        .await;
        let _ = sqlx::query!("DELETE FROM host_payout_ledger WHERE booking_id = $1 OR booking_id = $2 OR booking_id = $3", booking.id, b2.id, pending_b.id).execute(&pool).await;
        let _ = sqlx::query!("DELETE FROM booking_history WHERE booking_id = $1 OR booking_id = $2 OR booking_id = $3", booking.id, b2.id, pending_b.id).execute(&pool).await;
        let _ = sqlx::query!(
            "DELETE FROM booking WHERE id = $1 OR id = $2 OR id = $3",
            booking.id,
            b2.id,
            pending_b.id
        )
        .execute(&pool)
        .await;
        let _ = sqlx::query!("DELETE FROM listing WHERE id = $1", listing.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query!("DELETE FROM host_profiles WHERE user_id = $1", host_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query!(
            "DELETE FROM \"user\" WHERE id = $1 OR id = $2",
            host_id,
            guest_id
        )
        .execute(&pool)
        .await;
    }
}

/// Returns the booked date ranges (pending holds and confirmed stays) that
/// overlap `[date_from, date_to)` for a listing, ordered by start date.
///
/// Uses the same status set and overlap rule as [`check_availability`], so
/// the calendar and the hold logic agree. Read-only and lock-free: it is for
/// display; placing a hold still takes the row lock.
#[tracing::instrument(skip(executor))]
pub async fn get_unavailable_ranges<'e, E>(
    executor: E,
    listing_id: Uuid,
    date_from: chrono::NaiveDate,
    date_to: chrono::NaiveDate,
) -> Result<Vec<common::availability::UnavailableRange>>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query!(
        r#"
        SELECT date_from, date_to FROM booking
        WHERE listing_id = $1
          AND status IN ('pending', 'confirmed')
          AND date_from < $3
          AND date_to > $2
        ORDER BY date_from
        "#,
        listing_id,
        date_from,
        date_to
    )
    .fetch_all(executor)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| common::availability::UnavailableRange {
            date_from: r.date_from,
            date_to: r.date_to,
        })
        .collect())
}

#[cfg(test)]
mod unavailable_ranges_tests {
    use super::*;
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use sqlx_db_tester::TestPg;
    use std::path::Path;

    fn day(m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2030, m, d).unwrap()
    }

    async fn user(pool: &PgPool, role: crate::models::UserRole) -> Uuid {
        let id = Uuid::now_v7();
        let new_user = crate::models::NewUser {
            id,
            email: format!("u_{id}@example.com"),
            password_hash: "hash".to_string(),
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![role]),
            default_currency: "USD".to_string(),
        };
        crate::user::create_user(pool, &new_user).await.unwrap();
        id
    }

    async fn host(pool: &PgPool) -> Uuid {
        let host_id = user(pool, crate::models::UserRole::Host).await;
        let profile = crate::models::NewHostProfile {
            description: None,
            verified_status: Some("pending".to_string()),
            payout_details: None,
        };
        crate::user::create_host_profile(pool, host_id, &profile)
            .await
            .unwrap();
        host_id
    }

    async fn listing(pool: &PgPool, host_id: Uuid) -> Uuid {
        let new_listing = crate::models::NewListing {
            name: format!("Villa {}", Uuid::now_v7()),
            user_id: host_id,
            description: None,
            listing_structure_id: 1,
            country: "Jamaica".to_string(),
            price_per_night: Some(dec!(200.00)),
            weekly_discount_percentage: None,
            monthly_discount_percentage: None,
            max_guests: 4,
            bedrooms: 2,
            beds: 2,
            full_bathrooms: 1,
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
            cleaning_fee: None,
            damage_deposit: None,
            guests_included: None,
            extra_guest_fee: None,
        };
        crate::listing::create_listing(pool, &new_listing)
            .await
            .unwrap()
            .id
    }

    /// Each booking gets its own guest: a new hold cancels the guest's
    /// other pending holds.
    async fn book(pool: &PgPool, listing_id: Uuid, from: NaiveDate, to: NaiveDate) -> Booking {
        let guest_id = user(pool, crate::models::UserRole::Booker).await;
        let new_booking = NewBooking {
            // UUIDv7 starts with a timestamp; take the random tail.
            confirmation_code: Uuid::now_v7().simple().to_string()[22..].to_uppercase(),
            guest_id,
            listing_id,
            date_from: from,
            date_to: to,
            currency: "USD".to_string(),
            daily_rate: dec!(200.00),
            number_of_persons: 2,
            total_days: (to - from).num_days() as i32,
            sub_total_price: dec!(400.00),
            discount_value: None,
            tax_value: None,
            fee_breakdown: vec![],
            total_price: dec!(400.00),
            cancellation_policy: CancellationPolicy::Flexible,
            metadata: crate::models::BookingMetadata {
                num_adults: 2,
                ..Default::default()
            },
            door_access_code: None,
            cleaning_fee: Decimal::ZERO,
            damage_deposit: Decimal::ZERO,
            platform_fee: Decimal::ZERO,
            tax_rate: dec!(0.1000),
            exchange_rate: Decimal::ONE,
        };
        create_booking(pool, &new_booking).await.unwrap()
    }

    #[tokio::test]
    async fn returns_only_active_overlapping_ranges_in_order() {
        dotenvy::dotenv().ok();
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".into());
        let test_db = TestPg::new(db_url, Path::new("migrations"));
        let pool = test_db.get_pool().await;

        let host = host(&pool).await;
        let villa = listing(&pool, host).await;
        let other_villa = listing(&pool, host).await;

        let late = book(&pool, villa, day(3, 20), day(3, 25)).await;
        let early = book(&pool, villa, day(3, 1), day(3, 4)).await;
        let cancelled = book(&pool, villa, day(3, 10), day(3, 12)).await;
        sqlx::query!(
            "UPDATE booking SET status = 'cancelled' WHERE id = $1",
            cancelled.id
        )
        .execute(&pool)
        .await
        .unwrap();
        // Outside the window, and on another listing: both excluded.
        book(&pool, villa, day(6, 1), day(6, 3)).await;
        book(&pool, other_villa, day(3, 5), day(3, 8)).await;

        let ranges = get_unavailable_ranges(&pool, villa, day(3, 1), day(4, 1))
            .await
            .unwrap();
        let got: Vec<_> = ranges.iter().map(|r| (r.date_from, r.date_to)).collect();
        assert_eq!(
            got,
            vec![
                (early.date_from, early.date_to),
                (late.date_from, late.date_to)
            ]
        );
    }
}
