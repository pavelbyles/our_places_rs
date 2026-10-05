use crate::error::Result;
use crate::models::{DbPayoutLedgerEntry, DbPayoutStatus, DbPayoutSummary};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[allow(clippy::too_many_arguments)]
pub async fn create_payout_ledger_entry(
    conn: &mut PgConnection,
    id: Uuid,
    booking_id: Uuid,
    listing_id: Uuid,
    host_id: Uuid,
    currency: &str,
    gross_amount: Decimal,
    platform_fee_pct: Decimal,
    platform_fee_amount: Decimal,
    tax_withheld_amount: Decimal,
    exchange_rate: Decimal,
    net_payout_amount: Decimal,
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
        id,
        booking_id,
        listing_id,
        host_id,
        currency,
        gross_amount,
        platform_fee_pct,
        platform_fee_amount,
        tax_withheld_amount,
        exchange_rate,
        net_payout_amount,
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn get_payout_ledger_entries(
    pool: &PgPool,
    host_id: Option<Uuid>,
    listing_id: Option<Uuid>,
    status: Option<DbPayoutStatus>,
    date_from: Option<NaiveDate>,
    date_to: Option<NaiveDate>,
    page: u32,
    per_page: u32,
) -> Result<(Vec<DbPayoutLedgerEntry>, i64)> {
    let offset = ((page.max(1) - 1) * per_page) as i64;
    let limit = per_page.clamp(1, 100) as i64;

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
        host_id,
        listing_id,
        status as Option<DbPayoutStatus>,
        date_from,
        date_to,
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
        host_id,
        listing_id,
        status as Option<DbPayoutStatus>,
        date_from,
        date_to
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
