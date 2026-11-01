use crate::error::Result;
use crate::models::{DbPayoutLedgerEntry, DbPayoutStatus, DbPayoutSummary};
use chrono::{DateTime, Utc};
use common::payout::{PayoutFilter, PayoutRecord, PendingPayout};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Ledger page size when the caller doesn't ask for one.
pub const DEFAULT_LEDGER_PAGE_SIZE: u32 = 20;

/// Inserts a pending payout. A booking has at most one ledger entry; a
/// second insert for the same booking is ignored.
pub async fn create_payout_ledger_entry(
    conn: &mut PgConnection,
    payout: &PayoutRecord<PendingPayout>,
) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO host_payout_ledger (
            id, booking_id, listing_id, host_id, currency, gross_amount,
            platform_fee_pct, platform_fee_amount, tax_withheld_amount,
            exchange_rate, net_payout_amount, status
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'pending'::payout_status)
        ON CONFLICT (booking_id) DO NOTHING
        "#,
        payout.id,
        payout.booking_id,
        payout.listing_id,
        payout.host_id,
        payout.currency,
        payout.gross_amount,
        payout.platform_fee_pct,
        payout.platform_fee_amount,
        payout.tax_withheld_amount,
        payout.exchange_rate,
        payout.net_payout_amount,
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}

/// One page of ledger entries matching `filter`, newest first, plus the
/// total number of matches. `page` defaults to 1 and `per_page` to
/// [`DEFAULT_LEDGER_PAGE_SIZE`] (capped at 100).
pub async fn get_payout_ledger_entries(
    pool: &PgPool,
    filter: &PayoutFilter,
) -> Result<(Vec<DbPayoutLedgerEntry>, i64)> {
    let page = filter.page.unwrap_or(1).max(1);
    let limit = filter
        .per_page
        .unwrap_or(DEFAULT_LEDGER_PAGE_SIZE)
        .clamp(1, 100);
    let offset = i64::from(page - 1) * i64::from(limit);
    let limit = i64::from(limit);

    let entries = sqlx::query_as!(
        DbPayoutLedgerEntry,
        r#"
        SELECT 
            l.id,
            l.booking_id,
            b.confirmation_code AS booking_confirmation_code,
            l.listing_id,
            list.name AS listing_name,
            l.host_id,
            CONCAT(u.first_name, ' ', u.last_name) AS host_name,
            b.date_from AS "check_in_date?",
            b.date_to AS "check_out_date?",
            l.currency,
            l.gross_amount,
            l.platform_fee_pct,
            l.platform_fee_amount,
            l.tax_withheld_amount,
            l.exchange_rate,
            l.net_payout_amount,
            l.status AS "status: DbPayoutStatus",
            l.gateway_reference,
            l.failure_reason,
            l.payout_date,
            l.created_at,
            l.updated_at
        FROM host_payout_ledger l
        LEFT JOIN booking b ON l.booking_id = b.id
        LEFT JOIN listing list ON l.listing_id = list.id
        LEFT JOIN "user" u ON l.host_id = u.id
        WHERE ($1::uuid IS NULL OR l.host_id = $1)
          AND ($2::uuid IS NULL OR l.listing_id = $2)
          AND ($3::payout_status IS NULL OR l.status = $3)
          AND ($4::date IS NULL OR b.date_from >= $4)
          AND ($5::date IS NULL OR b.date_to <= $5)
        ORDER BY l.created_at DESC
        LIMIT $6 OFFSET $7
        "#,
        filter.host_id,
        filter.listing_id,
        filter.status as Option<DbPayoutStatus>,
        filter.date_from,
        filter.date_to,
        limit,
        offset
    )
    .fetch_all(pool)
    .await?;

    let count = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)
        FROM host_payout_ledger l
        LEFT JOIN booking b ON l.booking_id = b.id
        WHERE ($1::uuid IS NULL OR l.host_id = $1)
          AND ($2::uuid IS NULL OR l.listing_id = $2)
          AND ($3::payout_status IS NULL OR l.status = $3)
          AND ($4::date IS NULL OR b.date_from >= $4)
          AND ($5::date IS NULL OR b.date_to <= $5)
        "#,
        filter.host_id,
        filter.listing_id,
        filter.status as Option<DbPayoutStatus>,
        filter.date_from,
        filter.date_to
    )
    .fetch_one(pool)
    .await?
    .unwrap_or(0);

    Ok((entries, count))
}

pub async fn get_payout_summary(
    pool: &PgPool,
    host_id: Option<Uuid>,
    listing_id: Option<Uuid>,
) -> Result<DbPayoutSummary> {
    let summary = sqlx::query_as!(
        DbPayoutSummary,
        r#"
        SELECT 
            SUM(gross_amount) AS total_gross,
            SUM(platform_fee_amount) AS total_platform_fee,
            SUM(tax_withheld_amount) AS total_tax_withheld,
            SUM(net_payout_amount) AS total_net,
            SUM(CASE WHEN status = 'paid' THEN net_payout_amount ELSE 0 END) AS total_paid,
            SUM(CASE WHEN status = 'pending' THEN net_payout_amount ELSE 0 END) AS total_pending,
            SUM(CASE WHEN status = 'processing' THEN net_payout_amount ELSE 0 END) AS total_processing,
            COUNT(*) AS count_entries
        FROM host_payout_ledger
        WHERE ($1::uuid IS NULL OR host_id = $1)
          AND ($2::uuid IS NULL OR listing_id = $2)
        "#,
        host_id,
        listing_id
    )
    .fetch_one(pool)
    .await?;

    Ok(summary)
}

pub async fn update_payout_status(
    pool: &PgPool,
    id: Uuid,
    new_status: DbPayoutStatus,
    gateway_reference: Option<String>,
    failure_reason: Option<String>,
) -> Result<DbPayoutLedgerEntry> {
    let now = Utc::now();
    let payout_date: Option<DateTime<Utc>> = if new_status == DbPayoutStatus::Paid {
        Some(now)
    } else {
        None
    };

    let updated = sqlx::query_as!(
        DbPayoutLedgerEntry,
        r#"
        UPDATE host_payout_ledger l
        SET status = $1,
            gateway_reference = COALESCE($2, l.gateway_reference),
            failure_reason = $3,
            payout_date = COALESCE($4, l.payout_date),
            updated_at = $5
        FROM booking b, listing list, "user" u
        WHERE l.id = $6
          AND l.booking_id = b.id
          AND l.listing_id = list.id
          AND l.host_id = u.id
        RETURNING 
            l.id,
            l.booking_id,
            b.confirmation_code AS booking_confirmation_code,
            l.listing_id,
            list.name AS listing_name,
            l.host_id,
            CONCAT(u.first_name, ' ', u.last_name) AS host_name,
            b.date_from AS "check_in_date?",
            b.date_to AS "check_out_date?",
            l.currency,
            l.gross_amount,
            l.platform_fee_pct,
            l.platform_fee_amount,
            l.tax_withheld_amount,
            l.exchange_rate,
            l.net_payout_amount,
            l.status AS "status: DbPayoutStatus",
            l.gateway_reference,
            l.failure_reason,
            l.payout_date,
            l.created_at,
            l.updated_at
        "#,
        new_status as DbPayoutStatus,
        gateway_reference,
        failure_reason,
        payout_date,
        now,
        id
    )
    .fetch_one(pool)
    .await?;

    Ok(updated)
}

pub async fn cancel_pending_payout_for_booking(
    conn: &mut PgConnection,
    booking_id: Uuid,
) -> Result<Option<Uuid>> {
    let result = sqlx::query_scalar!(
        r#"
        UPDATE host_payout_ledger
        SET status = 'cancelled'::payout_status,
            updated_at = NOW()
        WHERE booking_id = $1 AND status = 'pending'::payout_status
        RETURNING id
        "#,
        booking_id
    )
    .fetch_optional(&mut *conn)
    .await?;

    Ok(result)
}
