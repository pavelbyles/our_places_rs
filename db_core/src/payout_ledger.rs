use crate::error::Result;
use crate::models::{DbPayoutLedgerEntry, DbPayoutStatus, DbPayoutSummary};
use chrono::{DateTime, Utc};
use common::payout::{PayoutFilter, PayoutRecord, PendingPayout};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Ledger page size when the caller doesn't ask for one.
pub const DEFAULT_LEDGER_PAGE_SIZE: u32 = 20;

/// Inserts a pending payout and returns the number of rows written. A booking
/// has at most one ledger entry; a second insert for the same booking is
/// ignored and returns 0.
pub async fn create_payout_ledger_entry(
    conn: &mut PgConnection,
    payout: &PayoutRecord<PendingPayout>,
) -> Result<u64> {
    let inserted = sqlx::query!(
        r#"
        INSERT INTO host_payout_ledger (
            id, booking_id, listing_id, host_id, currency, gross_amount,
            platform_fee_pct, platform_fee_amount, tax_withheld_amount,
            exchange_rate, net_payout_amount, cleaning_fee_amount, promo_discount_amount, status
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 'pending'::payout_status)
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
        payout.cleaning_fee_amount,
        payout.promo_discount_amount,
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();

    Ok(inserted)
}

/// Recalculates a booking's pending payout for new terms, using the row's own
/// rate and commission, so its amounts always match its currency even if the
/// host has since changed theirs. A booking with no pending row is left alone.
pub async fn reprice_pending_payout(
    conn: &mut PgConnection,
    booking_id: Uuid,
    accommodation_net: Decimal,
    cleaning_net: Decimal,
    promo_discount: Decimal,
) -> Result<()> {
    let Some(row) = sqlx::query!(
        r#"
        SELECT id, exchange_rate, platform_fee_pct FROM host_payout_ledger
        WHERE booking_id = $1 AND status = 'pending'::payout_status
        FOR UPDATE
        "#,
        booking_id
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(());
    };

    let gross_amount =
        common::pricing::host_gross(accommodation_net, cleaning_net, row.exchange_rate);
    let (platform_fee_amount, net_payout_amount) =
        common::pricing::calculate_host_payout(gross_amount, row.platform_fee_pct, Decimal::ZERO);

    sqlx::query!(
        r#"
        UPDATE host_payout_ledger
        SET gross_amount = $1,
            platform_fee_amount = $2,
            net_payout_amount = $3,
            cleaning_fee_amount = $4,
            promo_discount_amount = $5,
            updated_at = NOW()
        WHERE id = $6
        "#,
        gross_amount,
        platform_fee_amount,
        net_payout_amount,
        (cleaning_net * row.exchange_rate).round_dp(2),
        (promo_discount * row.exchange_rate).round_dp(2),
        row.id
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}

/// Returns a just-cancelled payout to `pending` for the amount the host keeps
/// under the cancellation policy. `retained` is in the booking currency, and
/// the row's own rate and commission are used, so the amounts always match the
/// row's currency even if the host has since changed theirs.
pub async fn reopen_payout_as_retained(
    conn: &mut PgConnection,
    ledger_id: Uuid,
    retained: Decimal,
) -> Result<()> {
    let row = sqlx::query!(
        "SELECT exchange_rate, platform_fee_pct FROM host_payout_ledger WHERE id = $1 FOR UPDATE",
        ledger_id
    )
    .fetch_one(&mut *conn)
    .await?;

    let gross_amount = (retained * row.exchange_rate).round_dp(2);
    let (platform_fee_amount, net_payout_amount) =
        common::pricing::calculate_host_payout(gross_amount, row.platform_fee_pct, Decimal::ZERO);

    sqlx::query!(
        r#"
        UPDATE host_payout_ledger
        SET gross_amount = $1,
            platform_fee_amount = $2,
            net_payout_amount = $3,
            cleaning_fee_amount = 0,
            promo_discount_amount = 0,
            status = 'pending'::payout_status,
            updated_at = NOW()
        WHERE id = $4 AND status = 'cancelled'::payout_status
        "#,
        gross_amount,
        platform_fee_amount,
        net_payout_amount,
        ledger_id
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}

/// A booking's ledger row, for stating a payout in an email.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookingPayout {
    /// The currency the host is paid in.
    pub currency: String,
    /// What the host receives after commission.
    pub net_payout_amount: Decimal,
    /// The promo discount the host funded.
    pub promo_discount_amount: Decimal,
    /// Whether the payout is still due.
    pub pending: bool,
}

/// A booking's ledger row, if it has one.
pub async fn get_booking_payout(pool: &PgPool, booking_id: Uuid) -> Result<Option<BookingPayout>> {
    Ok(sqlx::query!(
        r#"
        SELECT currency, net_payout_amount, promo_discount_amount,
            status = 'pending'::payout_status as "pending!"
        FROM host_payout_ledger WHERE booking_id = $1
        "#,
        booking_id
    )
    .fetch_optional(pool)
    .await?
    .map(|row| BookingPayout {
        currency: row.currency.trim().to_string(),
        net_payout_amount: row.net_payout_amount,
        promo_discount_amount: row.promo_discount_amount,
        pending: row.pending,
    }))
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
            l.cleaning_fee_amount,
            l.promo_discount_amount,
            b.promo_terms->>'code' AS "promo_code?",
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
            l.cleaning_fee_amount,
            l.promo_discount_amount,
            b.promo_terms->>'code' AS "promo_code?",
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

#[cfg(test)]
mod promo_tests {
    //! The cleaning fee and promo discount on ledger rows.

    use crate::models::{BookingStatus, SupplierInfo, UpdatedBooking};
    use crate::promo_tests::{insert_code, migrated_db, raw_booking, raw_listing, raw_user};
    use common::payout::PayoutFilter;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use sqlx::{PgPool, Row};
    use uuid::Uuid;

    fn supplier() -> SupplierInfo {
        SupplierInfo {
            supplier_name: "Our Places Jamaica Limited".to_string(),
            supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
            supplier_trn: "000-111-222".to_string(),
        }
    }

    /// The worked example held with 10% off the stay and cleaning fee, for a
    /// host paid in `host_currency`.
    async fn held(pool: &PgPool, host_currency: &str) -> (Uuid, Uuid) {
        let host = raw_user(pool).await;
        sqlx::query(r#"UPDATE "user" SET default_currency = $2 WHERE id = $1"#)
            .bind(host)
            .bind(host_currency)
            .execute(pool)
            .await
            .unwrap();
        let guest = raw_user(pool).await;
        let villa = raw_listing(pool, host).await;
        sqlx::query("UPDATE listing SET commission_pct = 0.1 WHERE id = $1")
            .bind(villa)
            .execute(pool)
            .await
            .unwrap();
        let code = insert_code(pool, host, "LEDGERED").await;
        let booking = raw_booking(pool, guest, villa, "pending").await;
        sqlx::query(
            r#"UPDATE booking SET expires_at = NOW() + INTERVAL '1 hour', sub_total_price = 1000,
                   discount_value = 100, cleaning_fee = 100, platform_fee = 40.50, tax_rate = 0.15,
                   tax_value = 135, total_price = 1075.50, date_from = '2030-06-01', date_to = '2030-06-08',
                   total_days = 7, promo_code_id = $2, promo_stay_discount = 90, promo_cleaning_discount = 10,
                   promo_terms = '{"code":"LEDGERED","discount_type":"percentage","discount_value":"10","scope":"stay_and_cleaning","currency":null,"exchange_rate":"1","min_nights":null,"min_booking_value":null,"stay_from":null,"stay_to":null}'
               WHERE id = $1"#,
        )
        .bind(booking)
        .bind(code)
        .execute(pool)
        .await
        .unwrap();
        (booking, host)
    }

    async fn update(pool: &PgPool, id: Uuid, updated: UpdatedBooking) {
        crate::booking::update_booking(pool, id, &updated, &supplier())
            .await
            .unwrap();
    }

    fn set_status(status: BookingStatus) -> UpdatedBooking {
        UpdatedBooking {
            status: Some(status),
            ..Default::default()
        }
    }

    async fn amounts(pool: &PgPool, id: Uuid) -> (Decimal, Decimal, Decimal) {
        let row = sqlx::query(
            "SELECT gross_amount, cleaning_fee_amount, promo_discount_amount
             FROM host_payout_ledger WHERE booking_id = $1",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
        (
            row.get("gross_amount"),
            row.get("cleaning_fee_amount"),
            row.get("promo_discount_amount"),
        )
    }

    #[tokio::test]
    async fn confirmation_writes_both_amounts_in_the_host_currency() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let (usd, _) = held(&pool, "USD").await;
        update(&pool, usd, set_status(BookingStatus::Confirmed)).await;
        assert_eq!(
            amounts(&pool, usd).await,
            (dec!(900.00), dec!(90.00), dec!(100.00))
        );
    }

    #[tokio::test]
    async fn confirmation_converts_both_amounts_for_a_host_paid_in_another_currency() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let (jmd, _) = held(&pool, "JMD").await;
        update(&pool, jmd, set_status(BookingStatus::Confirmed)).await;
        // The seeded USD -> JMD rate is 160.
        assert_eq!(
            amounts(&pool, jmd).await,
            (dec!(144000.00), dec!(14400.00), dec!(16000.00))
        );
    }

    #[tokio::test]
    async fn a_revision_rewrites_both_amounts() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let (id, _) = held(&pool, "USD").await;
        update(&pool, id, set_status(BookingStatus::Confirmed)).await;
        update(
            &pool,
            id,
            UpdatedBooking {
                date_to: Some(chrono::NaiveDate::from_ymd_opt(2030, 6, 9).expect("valid date")),
                total_days: Some(8),
                sub_total_price: Some(dec!(1200)),
                promo_stay_discount: Some(dec!(110)),
                promo_cleaning_discount: Some(dec!(10)),
                ..Default::default()
            },
        )
        .await;
        // 1200 - 100 weekly - 110 promo, plus 90 cleaning.
        assert_eq!(
            amounts(&pool, id).await,
            (dec!(1080.00), dec!(90.00), dec!(120.00))
        );
    }

    #[tokio::test]
    async fn a_retained_row_holds_zero_for_both() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let (id, _) = held(&pool, "USD").await;
        update(&pool, id, set_status(BookingStatus::Confirmed)).await;
        sqlx::query(
            "UPDATE booking SET cancellation_policy = 'moderate', date_from = CURRENT_DATE + 3,
                 date_to = CURRENT_DATE + 5 WHERE id = $1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        update(&pool, id, set_status(BookingStatus::Cancelled)).await;
        // Half of the 810.00 net stay.
        assert_eq!(
            amounts(&pool, id).await,
            (dec!(405.00), Decimal::ZERO, Decimal::ZERO)
        );
    }

    #[tokio::test]
    async fn entries_carry_the_code_and_an_older_row_reads_as_zero() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let (id, host) = held(&pool, "USD").await;
        update(&pool, id, set_status(BookingStatus::Confirmed)).await;
        let filter = PayoutFilter {
            host_id: Some(host),
            ..Default::default()
        };
        let (entries, _) = crate::payout_ledger::get_payout_ledger_entries(&pool, &filter)
            .await
            .unwrap();
        let entry = entries.iter().find(|e| e.booking_id == id).expect("entry");
        assert_eq!(entry.promo_code.as_deref(), Some("LEDGERED"));
        assert_eq!(entry.cleaning_fee_amount, dec!(90.00));
        assert_eq!(entry.promo_discount_amount, dec!(100.00));

        let other_guest = raw_user(&pool).await;
        let other_villa = raw_listing(&pool, host).await;
        let plain = raw_booking(&pool, other_guest, other_villa, "confirmed").await;
        sqlx::query(
            "INSERT INTO host_payout_ledger (booking_id, listing_id, host_id, gross_amount, net_payout_amount)
             VALUES ($1, $2, $3, 400, 360)",
        )
        .bind(plain)
        .bind(other_villa)
        .bind(host)
        .execute(&pool)
        .await
        .unwrap();
        let (entries, _) = crate::payout_ledger::get_payout_ledger_entries(&pool, &filter)
            .await
            .unwrap();
        let older = entries
            .iter()
            .find(|e| e.booking_id == plain)
            .expect("entry");
        assert_eq!(older.promo_code, None);
        assert_eq!(older.cleaning_fee_amount, Decimal::ZERO);
        assert_eq!(older.promo_discount_amount, Decimal::ZERO);
    }
}
