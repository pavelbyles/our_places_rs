//! Promo codes: lookups, eligibility and usage.
//!
//! Usage is derived from the live status of each redeemed booking, so holds that
//! expire or are cancelled free their use without any bookkeeping.
//!
//! Code strings never enter a log or span field, so anything taking one skips it.

use crate::error::{DbError, Result, SQLSTATE_FOREIGN_KEY_VIOLATION, SQLSTATE_UNIQUE_VIOLATION};
use chrono::{DateTime, NaiveDate, Utc};
use common::promo::{PromoDiscountType, PromoScope, PromoStatus, PromoTerms, PromoUsage};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgExecutor, PgPool};
use uuid::Uuid;

/// A promo code as stored. Its terms never change after creation.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PromoCode {
    /// Unique identifier.
    pub id: Uuid,
    /// The normalized code.
    pub code: String,
    /// Whether `discount_value` is a percentage or an amount.
    pub discount_type: PromoDiscountType,
    /// The percentage, or the amount in `currency`.
    pub discount_value: Decimal,
    /// The stated currency of a fixed discount or minimum value.
    pub currency: Option<String>,
    /// What the discount applies to.
    pub scope: PromoScope,
    /// When the code starts being valid.
    pub valid_from: DateTime<Utc>,
    /// When the code stops being valid.
    pub valid_to: DateTime<Utc>,
    /// First eligible night.
    pub stay_from: Option<NaiveDate>,
    /// Last eligible night.
    pub stay_to: Option<NaiveDate>,
    /// Fewest nights the stay must have.
    pub min_nights: Option<i32>,
    /// Least the eligible amount must be, in `currency`.
    pub min_booking_value: Option<Decimal>,
    /// Most uses allowed across all guests.
    pub total_usage_limit: Option<i32>,
    /// Most uses allowed per guest.
    pub per_guest_limit: Option<i32>,
    /// Whether the code covers every listing in its owner's reach.
    pub applies_to_all: bool,
    /// The host who funds and manages the code; `None` for an admin-created code.
    pub owner_host_id: Option<Uuid>,
    /// Who created the code.
    pub created_by: Uuid,
    /// When the code was deactivated.
    pub deactivated_at: Option<DateTime<Utc>>,
    /// Who deactivated the code.
    pub deactivated_by: Option<Uuid>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl PromoCode {
    /// The state a redemption's status check needs.
    pub fn status(&self) -> PromoStatus {
        PromoStatus {
            deactivated: self.deactivated_at.is_some(),
            valid_from: self.valid_from,
            valid_to: self.valid_to,
        }
    }

    /// The terms to snapshot on a booking, with the rate that converts the
    /// code's currency to the payment currency.
    pub fn terms(&self, exchange_rate: Decimal) -> PromoTerms {
        PromoTerms {
            code: self.code.clone(),
            discount_type: self.discount_type,
            discount_value: self.discount_value,
            scope: self.scope,
            currency: self.currency.as_ref().map(|c| c.trim().to_string()),
            exchange_rate,
            min_nights: self.min_nights,
            min_booking_value: self.min_booking_value,
            stay_from: self.stay_from,
            stay_to: self.stay_to,
        }
    }

    /// Whether either usage limit is set, so usage needs counting at all.
    pub fn has_usage_limits(&self) -> bool {
        self.total_usage_limit.is_some() || self.per_guest_limit.is_some()
    }

    fn usage(&self, total_used: i64, guest_used: i64) -> PromoUsage {
        PromoUsage {
            total_limit: self.total_usage_limit,
            guest_limit: self.per_guest_limit,
            total_used,
            guest_used,
        }
    }
}

/// Finds a code by its normalized string, without taking a lock.
pub async fn get_by_code<'e, E>(executor: E, code: &str) -> Result<Option<PromoCode>>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as!(
        PromoCode,
        r#"
        SELECT id, code, discount_type as "discount_type: PromoDiscountType", discount_value,
            currency, scope as "scope: PromoScope", valid_from, valid_to, stay_from, stay_to,
            min_nights, min_booking_value, total_usage_limit, per_guest_limit, applies_to_all,
            owner_host_id, created_by, deactivated_at, deactivated_by, created_at, updated_at
        FROM promo_code WHERE code = $1
        "#,
        code
    )
    .fetch_optional(executor)
    .await?)
}

/// Whether a code covers a listing. A host-owned code only ever covers that
/// host's current listings, so a later ownership change can't make a different
/// host fund it.
#[tracing::instrument(skip(executor, code), fields(promo_code_id = %code.id))]
pub async fn covers_listing<'e, E>(executor: E, code: &PromoCode, listing_id: Uuid) -> Result<bool>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM listing l
            WHERE l.id = $1
              AND ($2::uuid IS NULL OR l.user_id = $2)
              AND ($3 OR EXISTS (
                  SELECT 1 FROM promo_code_listing pl
                  WHERE pl.promo_code_id = $4 AND pl.listing_id = l.id
              ))
        ) as "covered!"
        "#,
        listing_id,
        code.owner_host_id,
        code.applies_to_all,
        code.id
    )
    .fetch_one(executor)
    .await?)
}

/// Locks a code and returns it with the usage a hold must respect. The count is
/// a separate statement after the lock: under READ COMMITTED, that gives it a
/// fresh snapshot, where a single statement would reuse the one from before the
/// lock wait and miss uses committed during it.
///
/// A short lock timeout bounds how long a popular code can pin a connection.
pub async fn lock_and_count(
    conn: &mut PgConnection,
    code_id: Uuid,
    guest_id: Uuid,
) -> Result<(PromoCode, PromoUsage)> {
    let code = lock_code(conn, code_id).await?;
    if !code.has_usage_limits() {
        let usage = code.usage(0, 0);
        return Ok((code, usage));
    }
    // A live open or accepted request with the code reserves a use (#105),
    // so the offer can always convert at the price the guest accepted.
    let row = sqlx::query!(
        r#"
        WITH bookings AS (
            SELECT COUNT(*) AS total, COUNT(*) FILTER (WHERE b.guest_id = $2) AS guest
            FROM promo_code_redemption r
            JOIN booking b ON b.id = r.booking_id
            WHERE r.promo_code_id = $1
              AND r.released_at IS NULL
              AND (b.status IN ('confirmed', 'completed')
                   OR (b.status = 'pending' AND b.expires_at > NOW()))
        ), requests AS (
            SELECT COUNT(*) AS total, COUNT(*) FILTER (WHERE q.guest_id = $2) AS guest
            FROM booking_request q
            WHERE q.promo_code_id = $1
              AND q.status IN ('open', 'accepted') AND q.expires_at > NOW()
        )
        SELECT bookings.total + requests.total AS "total!",
            bookings.guest + requests.guest AS "guest!"
        FROM bookings, requests
        "#,
        code_id,
        guest_id
    )
    .fetch_one(&mut *conn)
    .await?;
    let usage = code.usage(row.total, row.guest);
    Ok((code, usage))
}

/// Locks a code's row, giving up after a few seconds with a retryable error.
pub async fn lock_code(conn: &mut PgConnection, code_id: Uuid) -> Result<PromoCode> {
    sqlx::query("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *conn)
        .await?;
    let code = sqlx::query_as!(
        PromoCode,
        r#"
        SELECT id, code, discount_type as "discount_type: PromoDiscountType", discount_value,
            currency, scope as "scope: PromoScope", valid_from, valid_to, stay_from, stay_to,
            min_nights, min_booking_value, total_usage_limit, per_guest_limit, applies_to_all,
            owner_host_id, created_by, deactivated_at, deactivated_by, created_at, updated_at
        FROM promo_code WHERE id = $1 FOR UPDATE
        "#,
        code_id
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| DbError::from_sqlx_contended(e, "promo code"))?;
    sqlx::query("SET LOCAL lock_timeout = DEFAULT")
        .execute(&mut *conn)
        .await?;
    Ok(code)
}

/// Frees the use a booking holds, because a change means it no longer qualifies.
pub async fn release_redemption(conn: &mut PgConnection, booking_id: Uuid) -> Result<()> {
    sqlx::query!(
        "UPDATE promo_code_redemption SET released_at = NOW() WHERE booking_id = $1 AND released_at IS NULL",
        booking_id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The usage a quote should show, without a lock. A guest's own pending holds
/// and live requests are left out of both counts, because placing a hold or
/// request on the listing cancels or replaces them first, so a quote can't
/// refuse a code that the hold would accept.
pub async fn usage_for_quote<'e, E>(
    executor: E,
    code: &PromoCode,
    guest_id: Uuid,
) -> Result<PromoUsage>
where
    E: PgExecutor<'e>,
{
    if !code.has_usage_limits() {
        return Ok(code.usage(0, 0));
    }
    let row = sqlx::query!(
        r#"
        WITH bookings AS (
            SELECT COUNT(*) AS total, COUNT(*) FILTER (WHERE b.guest_id = $2) AS guest
            FROM promo_code_redemption r
            JOIN booking b ON b.id = r.booking_id
            WHERE r.promo_code_id = $1
              AND r.released_at IS NULL
              AND (b.status IN ('confirmed', 'completed')
                   OR (b.status = 'pending' AND b.expires_at > NOW() AND b.guest_id <> $2))
        ), requests AS (
            SELECT COUNT(*) AS total
            FROM booking_request q
            WHERE q.promo_code_id = $1 AND q.guest_id <> $2
              AND q.status IN ('open', 'accepted') AND q.expires_at > NOW()
        )
        SELECT bookings.total + requests.total AS "total!", bookings.guest AS "guest!"
        FROM bookings, requests
        "#,
        code.id,
        guest_id
    )
    .fetch_one(executor)
    .await?;
    Ok(code.usage(row.total, row.guest))
}

/// A code to create. Validated by the caller with
/// [`common::promo::validate_new_code`].
#[derive(Debug, Clone)]
pub struct NewPromoCode {
    /// The normalized code.
    pub code: String,
    /// Whether `discount_value` is a percentage or an amount.
    pub discount_type: PromoDiscountType,
    /// The percentage, or the amount in `currency`.
    pub discount_value: Decimal,
    /// The stated currency of a fixed discount or minimum value.
    pub currency: Option<String>,
    /// What the discount applies to.
    pub scope: PromoScope,
    /// When the code starts being valid.
    pub valid_from: DateTime<Utc>,
    /// When the code stops being valid.
    pub valid_to: DateTime<Utc>,
    /// First eligible night.
    pub stay_from: Option<NaiveDate>,
    /// Last eligible night.
    pub stay_to: Option<NaiveDate>,
    /// Fewest nights the stay must have.
    pub min_nights: Option<i32>,
    /// Least the eligible amount must be, in `currency`.
    pub min_booking_value: Option<Decimal>,
    /// Most uses allowed across all guests.
    pub total_usage_limit: Option<i32>,
    /// Most uses allowed per guest.
    pub per_guest_limit: Option<i32>,
    /// Whether the code covers every listing in its owner's reach.
    pub applies_to_all: bool,
    /// The selected listings, when it doesn't.
    pub listing_ids: Vec<Uuid>,
    /// The host who funds and manages the code; `None` for an admin-created code.
    pub owner_host_id: Option<Uuid>,
    /// Who is creating the code.
    pub created_by: Uuid,
}

/// A code with its selected listings and how it's being used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromoCodeWithStats {
    /// The code.
    pub code: PromoCode,
    /// The selected listings, empty when it covers all.
    pub listing_ids: Vec<Uuid>,
    /// Live holds using the code.
    pub active_holds: i64,
    /// Confirmed or completed bookings using the code.
    pub confirmed_uses: i64,
}

/// Creates a code and its selected listings.
pub async fn create_code(pool: &PgPool, new: &NewPromoCode) -> Result<Uuid> {
    let mut tx = pool.begin().await?;
    if let Some(currency) = &new.currency {
        crate::currency::ensure_active(&mut tx, currency).await?;
    }
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO promo_code (
            code, discount_type, discount_value, currency, scope, valid_from, valid_to,
            stay_from, stay_to, min_nights, min_booking_value, total_usage_limit,
            per_guest_limit, applies_to_all, owner_host_id, created_by
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
        RETURNING id
        "#,
        new.code,
        new.discount_type as PromoDiscountType,
        new.discount_value,
        new.currency,
        new.scope as PromoScope,
        new.valid_from,
        new.valid_to,
        new.stay_from,
        new.stay_to,
        new.min_nights,
        new.min_booking_value,
        new.total_usage_limit,
        new.per_guest_limit,
        new.applies_to_all,
        new.owner_host_id,
        new.created_by
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match e.as_database_error().and_then(|d| d.code()) {
        Some(code) if code == SQLSTATE_UNIQUE_VIOLATION => DbError::PromoCodeExists,
        _ => DbError::Sqlx(e),
    })?;
    for listing_id in &new.listing_ids {
        sqlx::query!(
            "INSERT INTO promo_code_listing (promo_code_id, listing_id) VALUES ($1, $2)",
            id,
            listing_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| match e.as_database_error().and_then(|d| d.code()) {
            Some(code) if code == SQLSTATE_FOREIGN_KEY_VIOLATION => {
                DbError::ValidationError("A selected listing doesn't exist".to_string())
            }
            _ => DbError::Sqlx(e),
        })?;
    }
    tx.commit().await?;
    Ok(id)
}

/// Adds selected listings and usage to a page of codes, in one query each.
async fn with_stats(pool: &PgPool, codes: Vec<PromoCode>) -> Result<Vec<PromoCodeWithStats>> {
    let ids: Vec<Uuid> = codes.iter().map(|c| c.id).collect();
    let stats = sqlx::query!(
        r#"
        SELECT r.promo_code_id,
            COUNT(*) FILTER (WHERE b.status = 'pending' AND b.expires_at > NOW()) as "active_holds!",
            COUNT(*) FILTER (WHERE b.status IN ('confirmed', 'completed')) as "confirmed_uses!"
        FROM promo_code_redemption r
        JOIN booking b ON b.id = r.booking_id
        WHERE r.promo_code_id = ANY($1) AND r.released_at IS NULL
        GROUP BY r.promo_code_id
        "#,
        &ids
    )
    .fetch_all(pool)
    .await?;
    let listings = sqlx::query!(
        "SELECT promo_code_id, listing_id FROM promo_code_listing WHERE promo_code_id = ANY($1)",
        &ids
    )
    .fetch_all(pool)
    .await?;
    Ok(codes
        .into_iter()
        .map(|code| {
            let usage = stats.iter().find(|s| s.promo_code_id == code.id);
            PromoCodeWithStats {
                listing_ids: listings
                    .iter()
                    .filter(|l| l.promo_code_id == code.id)
                    .map(|l| l.listing_id)
                    .collect(),
                active_holds: usage.map_or(0, |s| s.active_holds),
                confirmed_uses: usage.map_or(0, |s| s.confirmed_uses),
                code,
            }
        })
        .collect())
}

/// One page of codes, newest first, with the total. `owner` limits it to one
/// host's codes; `None` is every code.
pub async fn list_codes(
    pool: &PgPool,
    owner: Option<Uuid>,
    page: u32,
    per_page: u32,
) -> Result<(Vec<PromoCodeWithStats>, i64)> {
    let per_page = per_page.clamp(1, 100);
    let offset = i64::from(page.max(1) - 1) * i64::from(per_page);
    let total = sqlx::query_scalar!(
        r#"SELECT COUNT(*) as "total!" FROM promo_code WHERE ($1::uuid IS NULL OR owner_host_id = $1)"#,
        owner
    )
    .fetch_one(pool)
    .await?;
    let codes = sqlx::query_as!(
        PromoCode,
        r#"
        SELECT id, code, discount_type as "discount_type: PromoDiscountType", discount_value,
            currency, scope as "scope: PromoScope", valid_from, valid_to, stay_from, stay_to,
            min_nights, min_booking_value, total_usage_limit, per_guest_limit, applies_to_all,
            owner_host_id, created_by, deactivated_at, deactivated_by, created_at, updated_at
        FROM promo_code
        WHERE ($1::uuid IS NULL OR owner_host_id = $1)
        ORDER BY created_at DESC, id DESC
        LIMIT $2 OFFSET $3
        "#,
        owner,
        i64::from(per_page),
        offset
    )
    .fetch_all(pool)
    .await?;
    Ok((with_stats(pool, codes).await?, total))
}

/// One code with its stats. A missing code is `RowNotFound`.
pub async fn get_code(pool: &PgPool, id: Uuid) -> Result<PromoCodeWithStats> {
    let code = sqlx::query_as!(
        PromoCode,
        r#"
        SELECT id, code, discount_type as "discount_type: PromoDiscountType", discount_value,
            currency, scope as "scope: PromoScope", valid_from, valid_to, stay_from, stay_to,
            min_nights, min_booking_value, total_usage_limit, per_guest_limit, applies_to_all,
            owner_host_id, created_by, deactivated_at, deactivated_by, created_at, updated_at
        FROM promo_code WHERE id = $1
        "#,
        id
    )
    .fetch_one(pool)
    .await?;
    with_stats(pool, vec![code])
        .await?
        .into_iter()
        .next()
        .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))
}

/// Deactivates a code, which is one-way. The row lock this takes makes it wait
/// for any hold using the code, and that hold's result stands.
pub async fn deactivate_code(pool: &PgPool, id: Uuid, actor_id: Uuid) -> Result<()> {
    let updated = sqlx::query!(
        "UPDATE promo_code SET deactivated_at = NOW(), deactivated_by = $2
         WHERE id = $1 AND deactivated_at IS NULL",
        id,
        actor_id
    )
    .execute(pool)
    .await?
    .rows_affected();
    if updated == 0 {
        // Either it's already deactivated, or it doesn't exist (`RowNotFound`).
        get_code(pool, id).await?;
        return Err(DbError::PromoAlreadyDeactivated);
    }
    Ok(())
}

/// The owner of each listing that exists, for checking who may select them.
pub async fn listing_owners(pool: &PgPool, listing_ids: &[Uuid]) -> Result<Vec<(Uuid, Uuid)>> {
    Ok(sqlx::query!(
        "SELECT id, user_id FROM listing WHERE id = ANY($1) AND deleted_at IS NULL",
        listing_ids
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.id, row.user_id))
    .collect())
}
