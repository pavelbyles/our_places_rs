//! Promo codes: lookups, eligibility and usage.
//!
//! Usage is derived from the live status of each redeemed booking, so holds that
//! expire or are cancelled free their use without any bookkeeping.
//!
//! Code strings never enter a log or span field, so anything taking one skips it.

use crate::error::{DbError, Result};
use chrono::{DateTime, NaiveDate, Utc};
use common::promo::{PromoDiscountType, PromoScope, PromoStatus, PromoTerms, PromoUsage};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgExecutor};
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
    let row = sqlx::query!(
        r#"
        SELECT COUNT(*) as "total!", COUNT(*) FILTER (WHERE b.guest_id = $2) as "guest!"
        FROM promo_code_redemption r
        JOIN booking b ON b.id = r.booking_id
        WHERE r.promo_code_id = $1
          AND r.released_at IS NULL
          AND (b.status IN ('confirmed', 'completed')
               OR (b.status = 'pending' AND b.expires_at > NOW()))
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

/// The usage a quote should show, without a lock. A guest's own pending holds
/// are left out of both counts, because placing the hold cancels them first, so
/// a quote can't refuse a code that the hold would accept.
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
        SELECT COUNT(*) as "total!", COUNT(*) FILTER (WHERE b.guest_id = $2) as "guest!"
        FROM promo_code_redemption r
        JOIN booking b ON b.id = r.booking_id
        WHERE r.promo_code_id = $1
          AND r.released_at IS NULL
          AND (b.status IN ('confirmed', 'completed')
               OR (b.status = 'pending' AND b.expires_at > NOW() AND b.guest_id <> $2))
        "#,
        code.id,
        guest_id
    )
    .fetch_one(executor)
    .await?;
    Ok(code.usage(row.total, row.guest))
}
