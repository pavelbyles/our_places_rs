//! Foreign exchange rate resolution: explicit pair first, derived reverse second.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgPool};

use crate::error::{DbError, Result};

/// Why a rate is being resolved, which decides what history may be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateUse {
    /// A new quote: listing search/detail and checkout hold creation. Retired
    /// pairs are ignored and both currencies must be active.
    NewQuote,
    /// Settling or displaying an existing booking: payout at confirmation,
    /// reservation-change payout, tax documents, booking display. Retired pairs
    /// and inactive currencies are still honoured.
    Settlement,
}

impl RateUse {
    fn includes_retired(self) -> bool {
        matches!(self, RateUse::Settlement)
    }
}

/// Fails unless both currencies exist and are active (used for new quotes).
async fn ensure_quotable(conn: &mut PgConnection, base: &str, target: &str) -> Result<()> {
    let rows = sqlx::query!(
        r#"SELECT code, active FROM currencies WHERE code = $1 OR code = $2"#,
        base,
        target
    )
    .fetch_all(&mut *conn)
    .await?;
    [base, target].iter().try_for_each(|code| {
        rows.iter()
            .find(|r| r.code == *code)
            .ok_or_else(|| DbError::CurrencyUnknown((*code).to_string()))
            .and_then(|r| {
                r.active
                    .then_some(())
                    .ok_or_else(|| DbError::CurrencyInactive((*code).to_string()))
            })
    })
}

/// Inverts a stored reverse rate to six decimal places (banker's rounding, as
/// everywhere else in pricing). Done in Rust, never SQL `round()`, which
/// rounds halves away from zero.
fn invert(rate: Decimal) -> Result<Decimal> {
    Decimal::ONE
        .checked_div(rate)
        .map(|r| r.round_dp(6))
        .ok_or_else(|| DbError::ValidationError("Inverse exchange rate is zero".to_string()))
}

/// The rate converting one unit of `base` into `target` effective at `at`.
///
/// Resolution:
/// 1. `base == target` → `1`.
/// 2. The latest explicit `base → target` row with `effective_at <= at`.
/// 3. Otherwise the latest `target → base` row, inverted to 6 dp.
/// 4. Otherwise [`DbError::FxRateUnavailable`]. Never a fallback rate.
///
/// For [`RateUse::NewQuote`] both currencies must be active and retired pairs
/// (direct or as the source of a derived reverse) are ignored.
#[tracing::instrument(skip(conn))]
pub async fn get_rate(
    conn: &mut PgConnection,
    base: &str,
    target: &str,
    at: DateTime<Utc>,
    use_: RateUse,
) -> Result<Decimal> {
    if use_ == RateUse::NewQuote {
        ensure_quotable(conn, base, target).await?;
    }
    if base == target {
        return Ok(Decimal::ONE);
    }

    let found = sqlx::query!(
        r#"
        SELECT r.rate AS "rate!", r.derived AS "derived!"
        FROM (
            SELECT rate, FALSE AS derived, effective_at, updated_at
              FROM currency_exchange_rates
             WHERE base_currency = $1 AND target_currency = $2
               AND effective_at <= $3 AND ($4 OR retired_at IS NULL)
            UNION ALL
            SELECT rate, TRUE AS derived, effective_at, updated_at
              FROM currency_exchange_rates
             WHERE base_currency = $2 AND target_currency = $1
               AND effective_at <= $3 AND ($4 OR retired_at IS NULL)
        ) r
        ORDER BY r.derived, r.effective_at DESC, r.updated_at DESC
        LIMIT 1
        "#,
        base,
        target,
        at,
        use_.includes_retired()
    )
    .fetch_optional(&mut *conn)
    .await?;

    found
        .ok_or_else(|| DbError::FxRateUnavailable {
            base: base.to_string(),
            target: target.to_string(),
        })
        .and_then(|row| {
            if row.derived {
                invert(row.rate)
            } else {
                Ok(row.rate)
            }
        })
}

/// [`get_rate`] on a pooled connection.
pub async fn get_rate_pool(
    pool: &PgPool,
    base: &str,
    target: &str,
    at: DateTime<Utc>,
    use_: RateUse,
) -> Result<Decimal> {
    let mut conn = pool.acquire().await?;
    get_rate(&mut conn, base, target, at, use_).await
}

/// Rates converting each of `bases` into `target` at `at`, in one query.
///
/// Same resolution as [`get_rate`] (explicit first, derived reverse second,
/// `NewQuote` filters). A base equal to `target` maps to `1`. Bases with no
/// resolvable rate are **absent** from the map, so the caller decides how to
/// present them; they are never given a fallback rate. For `NewQuote` the
/// target itself must exist and be active.
#[tracing::instrument(skip(conn))]
pub async fn get_rates_for_bases(
    conn: &mut PgConnection,
    bases: &[String],
    target: &str,
    at: DateTime<Utc>,
    use_: RateUse,
) -> Result<std::collections::HashMap<String, Decimal>> {
    if use_ == RateUse::NewQuote {
        ensure_quotable(conn, target, target).await?;
    }
    let rows = sqlx::query!(
        r#"
        SELECT DISTINCT ON (r.base) r.base AS "base!", r.rate AS "rate!", r.derived AS "derived!"
        FROM (
            SELECT base_currency AS base, rate, FALSE AS derived, effective_at, updated_at
              FROM currency_exchange_rates
             WHERE base_currency = ANY($1) AND target_currency = $2
               AND effective_at <= $3 AND ($4 OR retired_at IS NULL)
            UNION ALL
            SELECT target_currency AS base, rate, TRUE AS derived, effective_at, updated_at
              FROM currency_exchange_rates
             WHERE target_currency = ANY($1) AND base_currency = $2
               AND effective_at <= $3 AND ($4 OR retired_at IS NULL)
        ) r
        ORDER BY r.base, r.derived, r.effective_at DESC, r.updated_at DESC
        "#,
        bases,
        target,
        at,
        use_.includes_retired()
    )
    .fetch_all(&mut *conn)
    .await?;

    let identity = bases
        .iter()
        .filter(|b| b.as_str() == target)
        .map(|b| Ok((b.clone(), Decimal::ONE)));
    rows.into_iter()
        .map(|r| {
            let rate = if r.derived { invert(r.rate)? } else { r.rate };
            Ok((r.base, rate))
        })
        .chain(identity)
        .collect()
}

// ---------------------------------------------------------------------------
// Rate administration (#100)
// ---------------------------------------------------------------------------

/// How a rate transaction locks the base currency row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseLock {
    /// `FOR KEY SHARE` (publish, delete).
    KeyShare,
    /// `FOR UPDATE` (retire: blocks new listings in the base currency).
    Update,
}

/// A locked currency row.
#[derive(Debug, Clone)]
pub(crate) struct LockedCurrency {
    pub code: String,
    pub active: bool,
}

/// Locks both currency rows of a pair **in ascending code order**, before any
/// pair advisory lock, so FK checks on later inserts never wait (no cycles).
/// The base gets `base_lock`; the target always `FOR KEY SHARE`.
pub(crate) async fn lock_pair_currencies(
    conn: &mut PgConnection,
    base: &str,
    target: &str,
    base_lock: BaseLock,
) -> Result<(LockedCurrency, LockedCurrency)> {
    let mut order = [base, target];
    order.sort_unstable();
    let mut locked: Vec<LockedCurrency> = Vec::with_capacity(2);
    for code in order {
        let row = if code == base && base_lock == BaseLock::Update {
            sqlx::query!(
                r#"SELECT code, active FROM currencies WHERE code = $1 FOR UPDATE"#,
                code
            )
            .fetch_optional(&mut *conn)
            .await
            .map(|r| r.map(|r| (r.code, r.active)))
        } else {
            sqlx::query!(
                r#"SELECT code, active FROM currencies WHERE code = $1 FOR KEY SHARE"#,
                code
            )
            .fetch_optional(&mut *conn)
            .await
            .map(|r| r.map(|r| (r.code, r.active)))
        }
        .map_err(|e| DbError::from_sqlx_contended(e, "currency lock"))?
        .ok_or_else(|| DbError::CurrencyUnknown(code.to_string()))?;
        locked.push(LockedCurrency {
            code: row.0,
            active: row.1,
        });
    }
    let find = |code: &str| {
        locked
            .iter()
            .find(|c| c.code == code)
            .cloned()
            .ok_or_else(|| DbError::CurrencyUnknown(code.to_string()))
    };
    Ok((find(base)?, find(target)?))
}

/// Serialises every administrative change to one pair (A→B), transaction-scoped.
pub(crate) async fn lock_pair(conn: &mut PgConnection, base: &str, target: &str) -> Result<()> {
    let key = format!("fx:{base}:{target}");
    sqlx::query_scalar!(
        r#"SELECT pg_advisory_xact_lock(hashtext($1)) IS NULL AS "locked!""#,
        key
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| DbError::from_sqlx_contended(e, "exchange-rate pair lock"))?;
    Ok(())
}

/// The database clock right now (not the transaction start time).
pub(crate) async fn db_now(conn: &mut PgConnection) -> Result<DateTime<Utc>> {
    Ok(sqlx::query_scalar!(r#"SELECT clock_timestamp() AS "now!""#)
        .fetch_one(&mut *conn)
        .await?)
}

/// A request to publish one rate version, already shape-validated.
#[derive(Debug, Clone)]
pub struct PublishRate {
    /// Currency converted from.
    pub base: common::currency::CurrencyCode,
    /// Currency converted to.
    pub target: common::currency::CurrencyCode,
    /// Validated rate (see `common::exchange_rate::parse_rate`).
    pub rate: Decimal,
    /// Requested effective time; `None` means now.
    pub effective_at: Option<DateTime<Utc>>,
    /// Confirms a change beyond the typo guard.
    pub confirm_large_change: bool,
    /// Admin publishing it (server-loaded, never from the request body).
    pub actor_id: uuid::Uuid,
    /// Grace window for "effective now" submissions.
    pub grace: chrono::Duration,
}

/// Publishes a rate version inside `tx` (see [`publish_rate`]).
pub async fn publish_rate_tx(
    tx: &mut PgConnection,
    req: &PublishRate,
) -> Result<common::exchange_rate::RateVersion> {
    use common::exchange_rate as rules;
    let (base, target) = (req.base.as_str(), req.target.as_str());
    if base == target {
        return Err(DbError::ValidationError(
            "base and target currencies must differ".to_string(),
        ));
    }
    let rate =
        rules::validate_rate(req.rate).map_err(|e| DbError::ValidationError(e.to_string()))?;

    // 1. Currency rows (ascending), 2. pair lock, 3. clock read after the lock.
    let (b, t) = lock_pair_currencies(tx, base, target, BaseLock::KeyShare).await?;
    [&b, &t].iter().try_for_each(|c| {
        c.active
            .then_some(())
            .ok_or_else(|| DbError::CurrencyInactive(c.code.clone()))
    })?;
    lock_pair(tx, base, target).await?;
    let now = db_now(tx).await?;

    let effective_at = rules::normalize_effective_at(
        req.effective_at,
        now,
        req.grace,
        chrono::Duration::days(rules::EFFECTIVE_AT_HORIZON_DAYS),
    )
    .map_err(|e| match e {
        rules::EffectiveAtError::InPast => DbError::EffectiveAtInPast,
        rules::EffectiveAtError::TooFarAhead => DbError::EffectiveAtTooFar,
    })?;

    // The explicit rate this one follows (retired history included).
    let baseline = sqlx::query_scalar!(
        r#"
        SELECT rate FROM currency_exchange_rates
        WHERE base_currency = $1 AND target_currency = $2 AND effective_at < $3
        ORDER BY effective_at DESC, updated_at DESC
        LIMIT 1
        "#,
        base,
        target,
        effective_at
    )
    .fetch_optional(&mut *tx)
    .await?;
    let guard_tripped = baseline.is_some_and(|b| rules::requires_confirmation(b, rate));
    if let Some(b) = baseline.filter(|_| guard_tripped && !req.confirm_large_change) {
        return Err(DbError::RateChangeConfirmationRequired {
            baseline: b,
            proposed: rate,
            deviation_pct: rules::deviation_pct(b, rate),
        });
    }

    let reinstated = sqlx::query!(
        r#"
        UPDATE currency_exchange_rates SET retired_at = NULL, retired_by = NULL
        WHERE base_currency = $1 AND target_currency = $2 AND retired_at IS NOT NULL
        "#,
        base,
        target
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if reinstated > 0 {
        crate::currency::write_audit(
            tx,
            crate::currency::AuditEntry {
                actor_id: req.actor_id,
                action: "pair_reinstated",
                currency_code: None,
                base_currency: Some(base),
                target_currency: Some(target),
                rate_id: None,
                details: serde_json::json!({ "rows": reinstated }),
            },
        )
        .await?;
    }

    let row = sqlx::query!(
        r#"
        INSERT INTO currency_exchange_rates
            (base_currency, target_currency, rate, effective_at, created_by)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, rate, effective_at, created_by
        "#,
        base,
        target,
        rate,
        effective_at,
        req.actor_id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match crate::error::sqlstate(&e).as_deref() {
        Some(crate::error::SQLSTATE_UNIQUE_VIOLATION) => DbError::RateVersionExists,
        _ => DbError::Sqlx(e),
    })?;

    crate::currency::write_audit(
        tx,
        crate::currency::AuditEntry {
            actor_id: req.actor_id,
            action: "rate_published",
            currency_code: None,
            base_currency: Some(base),
            target_currency: Some(target),
            rate_id: Some(row.id),
            details: serde_json::json!({
                "rate": rate.to_string(),
                "effective_at": effective_at,
                "baseline": baseline.map(|b| b.to_string()),
                "deviation_pct": baseline
                    .and_then(|b| rules::deviation_pct(b, rate))
                    .map(|d| d.to_string()),
                "guard_overridden": guard_tripped && req.confirm_large_change,
            }),
        },
    )
    .await?;

    let state = if row.effective_at > now {
        rules::RateState::Scheduled
    } else {
        rules::RateState::Current
    };
    Ok(rules::RateVersion {
        id: row.id,
        rate: row.rate,
        effective_at: row.effective_at,
        created_by: row.created_by,
        state,
    })
}

/// Publishes a rate version for a pair (creating or reinstating the pair).
///
/// One transaction: lock both currency rows (ascending), take the pair lock,
/// read the clock **after** the lock, normalise the effective time (grace,
/// no backdating, 366-day horizon), apply the 5% guard against the explicit
/// rate it follows, reinstate a retired pair, insert, audit.
#[tracing::instrument(skip(pool, req), fields(base = %req.base, target = %req.target))]
pub async fn publish_rate(
    pool: &PgPool,
    req: &PublishRate,
) -> Result<common::exchange_rate::RateVersion> {
    let mut tx = pool.begin().await?;
    let version = publish_rate_tx(&mut tx, req).await?;
    tx.commit()
        .await
        .map_err(|e| DbError::from_sqlx_contended(e, "rate publish"))?;
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;
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

    /// Test-only currencies, so fixture rates satisfy the `currencies` FK.
    async fn insert_test_currencies(conn: &mut PgConnection, codes: &[&str]) {
        for code in codes {
            sqlx::query(
                "INSERT INTO currencies (code, name) VALUES ($1, $1) ON CONFLICT DO NOTHING",
            )
            .bind(code)
            .execute(&mut *conn)
            .await
            .expect("insert test currency");
        }
    }

    #[tokio::test]
    async fn test_exchange_rate_identity() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;
        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        let rate = get_rate(&mut tx, "USD", "USD", Utc::now(), RateUse::Settlement)
            .await
            .expect("Identity rate lookup failed");
        assert_eq!(rate, Decimal::ONE);
    }

    #[tokio::test]
    async fn test_exchange_rate_direct() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;
        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        let now = Utc::now();
        insert_test_currencies(&mut tx, &["FOO", "BAR"]).await;
        // Insert a unique known pair
        sqlx::query!(
            r#"
            INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
            VALUES ('FOO', 'BAR', 150.500000, $1)
            "#,
            now
        )
        .execute(&mut *tx)
        .await
        .expect("Failed to insert test rate");

        let rate = get_rate(&mut tx, "FOO", "BAR", now, RateUse::Settlement)
            .await
            .expect("Direct rate lookup failed");
        assert_eq!(rate, dec!(150.500000));
    }

    #[tokio::test]
    async fn test_exchange_rate_inverse() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;
        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        let now = Utc::now();
        insert_test_currencies(&mut tx, &["FOO", "BAR"]).await;
        // Insert only BAR -> FOO with rate 2.0
        sqlx::query!(
            r#"
            INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
            VALUES ('BAR', 'FOO', 2.000000, $1)
            "#,
            now
        )
        .execute(&mut *tx)
        .await
        .expect("Failed to insert test rate");

        // Query FOO -> BAR which requires inverse
        let rate = get_rate(&mut tx, "FOO", "BAR", now, RateUse::Settlement)
            .await
            .expect("Inverse rate lookup failed");
        assert_eq!(rate, dec!(0.500000));
    }

    #[tokio::test]
    async fn test_exchange_rate_unavailable() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;
        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        let result = get_rate(&mut tx, "ZZZ", "YYY", Utc::now(), RateUse::Settlement).await;
        match result {
            Err(DbError::FxRateUnavailable { base, target }) => {
                assert_eq!(base, "ZZZ");
                assert_eq!(target, "YYY");
            }
            other => panic!("Expected FxRateUnavailable, got: {:?}", other),
        }
    }
}

#[cfg(test)]
mod get_rate_tests {
    //! Resolution rules for [`get_rate`], all at a fixed instant.

    use super::*;
    use chrono::{Duration, TimeZone};
    use rust_decimal_macros::dec;
    use sqlx_db_tester::TestPg;
    use std::env;
    use std::path::Path;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0)
            .single()
            .expect("fixed instant")
    }

    async fn db() -> TestPg {
        dotenvy::dotenv().ok();
        let url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/our_places".to_string()
        });
        TestPg::new(url, Path::new("migrations"))
    }

    async fn currency(conn: &mut PgConnection, code: &str, active: bool) {
        sqlx::query(
            "INSERT INTO currencies (code, name, active) VALUES ($1, $1, $2)
             ON CONFLICT (code) DO UPDATE SET active = EXCLUDED.active",
        )
        .bind(code)
        .bind(active)
        .execute(&mut *conn)
        .await
        .expect("insert currency");
    }

    async fn rate(
        conn: &mut PgConnection,
        base: &str,
        target: &str,
        rate: Decimal,
        at: DateTime<Utc>,
    ) {
        sqlx::query(
            "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(base)
        .bind(target)
        .bind(rate)
        .bind(at)
        .execute(&mut *conn)
        .await
        .expect("insert rate");
    }

    async fn retire(conn: &mut PgConnection, base: &str, target: &str) {
        sqlx::query(
            "UPDATE currency_exchange_rates SET retired_at = now()
             WHERE base_currency = $1 AND target_currency = $2",
        )
        .bind(base)
        .bind(target)
        .execute(&mut *conn)
        .await
        .expect("retire pair");
    }

    #[tokio::test]
    async fn explicit_pair_beats_derived_reverse() {
        let t = db().await;
        let pool = t.get_pool().await;
        let mut c = pool.acquire().await.expect("conn");
        currency(&mut c, "AAA", true).await;
        currency(&mut c, "BBB", true).await;
        rate(&mut c, "AAA", "BBB", dec!(3), at() - Duration::days(1)).await;
        rate(&mut c, "BBB", "AAA", dec!(0.5), at() - Duration::hours(1)).await;
        let r = get_rate(&mut c, "AAA", "BBB", at(), RateUse::NewQuote).await;
        assert_eq!(
            r.expect("rate"),
            dec!(3),
            "explicit wins even if reverse is newer"
        );
    }

    #[tokio::test]
    async fn derived_when_only_reverse_exists() {
        let t = db().await;
        let pool = t.get_pool().await;
        let mut c = pool.acquire().await.expect("conn");
        currency(&mut c, "AAA", true).await;
        currency(&mut c, "BBB", true).await;
        rate(&mut c, "BBB", "AAA", dec!(4), at() - Duration::days(1)).await;
        let r = get_rate(&mut c, "AAA", "BBB", at(), RateUse::NewQuote).await;
        assert_eq!(r.expect("rate"), dec!(0.25));
    }

    #[tokio::test]
    async fn scheduled_only_explicit_pair_falls_through_to_derived() {
        let t = db().await;
        let pool = t.get_pool().await;
        let mut c = pool.acquire().await.expect("conn");
        currency(&mut c, "AAA", true).await;
        currency(&mut c, "BBB", true).await;
        rate(&mut c, "AAA", "BBB", dec!(3), at() + Duration::days(1)).await;
        rate(&mut c, "BBB", "AAA", dec!(2), at() - Duration::days(1)).await;
        let r = get_rate(&mut c, "AAA", "BBB", at(), RateUse::NewQuote).await;
        assert_eq!(r.expect("rate"), dec!(0.5));
    }

    #[tokio::test]
    async fn future_row_is_ignored_until_effective() {
        let t = db().await;
        let pool = t.get_pool().await;
        let mut c = pool.acquire().await.expect("conn");
        currency(&mut c, "AAA", true).await;
        currency(&mut c, "BBB", true).await;
        rate(&mut c, "AAA", "BBB", dec!(10), at() - Duration::days(1)).await;
        rate(&mut c, "AAA", "BBB", dec!(11), at() + Duration::hours(1)).await;
        let now = get_rate(&mut c, "AAA", "BBB", at(), RateUse::NewQuote).await;
        assert_eq!(now.expect("rate"), dec!(10));
        let later = get_rate(
            &mut c,
            "AAA",
            "BBB",
            at() + Duration::hours(2),
            RateUse::NewQuote,
        )
        .await;
        assert_eq!(later.expect("rate"), dec!(11));
    }

    #[tokio::test]
    async fn single_row_is_effective_indefinitely() {
        let t = db().await;
        let pool = t.get_pool().await;
        let mut c = pool.acquire().await.expect("conn");
        currency(&mut c, "AAA", true).await;
        currency(&mut c, "BBB", true).await;
        rate(&mut c, "AAA", "BBB", dec!(7), at() - Duration::days(400)).await;
        let far = at() + Duration::days(3650);
        let r = get_rate(&mut c, "AAA", "BBB", far, RateUse::NewQuote).await;
        assert_eq!(r.expect("rate"), dec!(7));
    }

    #[tokio::test]
    async fn missing_pair_is_unavailable_not_a_fallback() {
        let t = db().await;
        let pool = t.get_pool().await;
        let mut c = pool.acquire().await.expect("conn");
        currency(&mut c, "AAA", true).await;
        currency(&mut c, "BBB", true).await;
        let r = get_rate(&mut c, "AAA", "BBB", at(), RateUse::NewQuote).await;
        assert!(
            matches!(r, Err(DbError::FxRateUnavailable { .. })),
            "got {r:?}"
        );
    }

    #[tokio::test]
    async fn retired_pair_excluded_for_new_quotes_but_used_for_settlement() {
        let t = db().await;
        let pool = t.get_pool().await;
        let mut c = pool.acquire().await.expect("conn");
        currency(&mut c, "AAA", true).await;
        currency(&mut c, "BBB", true).await;
        rate(&mut c, "AAA", "BBB", dec!(5), at() - Duration::days(1)).await;
        retire(&mut c, "AAA", "BBB").await;

        let direct_quote = get_rate(&mut c, "AAA", "BBB", at(), RateUse::NewQuote).await;
        assert!(matches!(
            direct_quote,
            Err(DbError::FxRateUnavailable { .. })
        ));
        let derived_quote = get_rate(&mut c, "BBB", "AAA", at(), RateUse::NewQuote).await;
        assert!(
            matches!(derived_quote, Err(DbError::FxRateUnavailable { .. })),
            "a retired pair must not feed a derived reverse"
        );

        let settle = get_rate(&mut c, "AAA", "BBB", at(), RateUse::Settlement).await;
        assert_eq!(settle.expect("settlement rate"), dec!(5));
        let settle_rev = get_rate(&mut c, "BBB", "AAA", at(), RateUse::Settlement).await;
        assert_eq!(settle_rev.expect("settlement reverse"), dec!(0.2));
    }

    #[tokio::test]
    async fn inactive_or_unknown_currency_rejected_for_new_quotes_only() {
        let t = db().await;
        let pool = t.get_pool().await;
        let mut c = pool.acquire().await.expect("conn");
        currency(&mut c, "AAA", true).await;
        currency(&mut c, "BBB", false).await;
        rate(&mut c, "AAA", "BBB", dec!(5), at() - Duration::days(1)).await;

        let quote = get_rate(&mut c, "AAA", "BBB", at(), RateUse::NewQuote).await;
        assert!(
            matches!(&quote, Err(DbError::CurrencyInactive(code)) if code == "BBB"),
            "got {quote:?}"
        );
        let unknown = get_rate(&mut c, "AAA", "QQQ", at(), RateUse::NewQuote).await;
        assert!(
            matches!(&unknown, Err(DbError::CurrencyUnknown(code)) if code == "QQQ"),
            "got {unknown:?}"
        );

        let settle = get_rate(&mut c, "AAA", "BBB", at(), RateUse::Settlement).await;
        assert_eq!(settle.expect("settlement ignores active flag"), dec!(5));
    }

    #[tokio::test]
    async fn derived_rate_uses_bankers_rounding_at_midpoint() {
        let t = db().await;
        let pool = t.get_pool().await;
        let mut c = pool.acquire().await.expect("conn");
        currency(&mut c, "AAA", true).await;
        currency(&mut c, "BBB", true).await;
        // 1 / 80000 = 0.0000125: a true 7th-place midpoint. Banker's rounding
        // (rust_decimal) gives 0.000012; SQL round() would give 0.000013.
        rate(&mut c, "BBB", "AAA", dec!(80000), at() - Duration::days(1)).await;
        let r = get_rate(&mut c, "AAA", "BBB", at(), RateUse::NewQuote)
            .await
            .expect("rate");
        assert_eq!(r, dec!(0.000012));
    }
}

#[cfg(test)]
mod publish_rate_tests {
    //! Publishing rate versions: grace, horizon, 5% guard, reinstatement,
    //! duplicates and serialisation of concurrent publishes.

    use super::*;
    use chrono::Duration;
    use common::currency::CurrencyCode;
    use rust_decimal_macros::dec;
    use sqlx_db_tester::TestPg;
    use std::env;
    use std::path::Path;
    use uuid::Uuid;

    async fn db() -> TestPg {
        dotenvy::dotenv().ok();
        let url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/our_places".to_string()
        });
        TestPg::new(url, Path::new("migrations"))
    }

    async fn admin(pool: &PgPool) -> Uuid {
        sqlx::query_scalar(
            r#"INSERT INTO "user" (email, password_hash, first_name, last_name, slug)
               VALUES ('admin-' || gen_random_uuid() || '@example.com', 'x', 'Rate', 'Admin',
                       'rate-admin-' || replace(gen_random_uuid()::text, '-', ''))
               RETURNING id"#,
        )
        .fetch_one(pool)
        .await
        .expect("admin user")
    }

    fn req(actor: Uuid, base: &str, target: &str, rate: Decimal) -> PublishRate {
        PublishRate {
            base: CurrencyCode::try_from(base).expect("base"),
            target: CurrencyCode::try_from(target).expect("target"),
            rate,
            effective_at: None,
            confirm_large_change: false,
            actor_id: actor,
            grace: Duration::seconds(60),
        }
    }

    async fn audit_actions(pool: &PgPool) -> Vec<(String, serde_json::Value)> {
        sqlx::query_as("SELECT action::text, details FROM currency_audit_log ORDER BY created_at")
            .fetch_all(pool)
            .await
            .expect("audit rows")
    }

    #[tokio::test]
    async fn backdating_beyond_grace_is_rejected() {
        let t = db().await;
        let pool = t.get_pool().await;
        let a = admin(&pool).await;
        let r = publish_rate(
            &pool,
            &PublishRate {
                effective_at: Some(Utc::now() - Duration::minutes(5)),
                ..req(a, "USD", "JMD", dec!(161))
            },
        )
        .await;
        assert!(matches!(r, Err(DbError::EffectiveAtInPast)), "got {r:?}");
    }

    #[tokio::test]
    async fn within_grace_is_stored_as_now_and_current() {
        let t = db().await;
        let pool = t.get_pool().await;
        let a = admin(&pool).await;
        let v = publish_rate(
            &pool,
            &PublishRate {
                effective_at: Some(Utc::now() - Duration::seconds(5)),
                ..req(a, "USD", "JMD", dec!(161))
            },
        )
        .await
        .expect("publish");
        assert_eq!(v.state, common::exchange_rate::RateState::Current);
        let mut c = pool.acquire().await.expect("conn");
        let now = get_rate(&mut c, "USD", "JMD", Utc::now(), RateUse::NewQuote)
            .await
            .expect("rate");
        assert_eq!(now, dec!(161));
    }

    #[tokio::test]
    async fn beyond_horizon_is_rejected() {
        let t = db().await;
        let pool = t.get_pool().await;
        let a = admin(&pool).await;
        let r = publish_rate(
            &pool,
            &PublishRate {
                effective_at: Some(Utc::now() + Duration::days(400)),
                ..req(a, "USD", "JMD", dec!(161))
            },
        )
        .await;
        assert!(matches!(r, Err(DbError::EffectiveAtTooFar)), "got {r:?}");
    }

    #[tokio::test]
    async fn large_change_needs_confirmation() {
        let t = db().await;
        let pool = t.get_pool().await;
        let a = admin(&pool).await;
        // Seeded USD→JMD is 160; 170 is +6.25%.
        let r = publish_rate(&pool, &req(a, "USD", "JMD", dec!(170))).await;
        match r {
            Err(DbError::RateChangeConfirmationRequired {
                baseline,
                proposed,
                deviation_pct,
            }) => {
                assert_eq!(baseline, dec!(160));
                assert_eq!(proposed, dec!(170));
                assert_eq!(deviation_pct, Some(dec!(6.25)));
            }
            other => panic!("expected confirmation, got {other:?}"),
        }
        assert!(audit_actions(&pool).await.is_empty(), "nothing written");
    }

    #[tokio::test]
    async fn confirmed_large_change_is_saved_and_audited() {
        let t = db().await;
        let pool = t.get_pool().await;
        let a = admin(&pool).await;
        publish_rate(
            &pool,
            &PublishRate {
                confirm_large_change: true,
                ..req(a, "USD", "JMD", dec!(170))
            },
        )
        .await
        .expect("confirmed publish");
        let audit = audit_actions(&pool).await;
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].0, "rate_published");
        assert_eq!(audit[0].1["guard_overridden"], true);
        assert_eq!(audit[0].1["baseline"], "160.000000");
    }

    #[tokio::test]
    async fn new_pair_has_no_guard() {
        let t = db().await;
        let pool = t.get_pool().await;
        let a = admin(&pool).await;
        let v = publish_rate(&pool, &req(a, "EUR", "JMD", dec!(175.5)))
            .await
            .expect("new pair");
        assert_eq!(v.rate, dec!(175.5));
    }

    #[tokio::test]
    async fn publishing_to_a_retired_pair_reinstates_it() {
        let t = db().await;
        let pool = t.get_pool().await;
        let a = admin(&pool).await;
        sqlx::query(
            "UPDATE currency_exchange_rates SET retired_at = now()
             WHERE base_currency = 'USD' AND target_currency = 'JMD'",
        )
        .execute(&pool)
        .await
        .expect("retire");
        publish_rate(&pool, &req(a, "USD", "JMD", dec!(161)))
            .await
            .expect("publish");
        let retired: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM currency_exchange_rates
             WHERE base_currency = 'USD' AND target_currency = 'JMD' AND retired_at IS NOT NULL",
        )
        .fetch_one(&pool)
        .await
        .expect("count");
        assert_eq!(retired, 0);
        let actions: Vec<String> = audit_actions(&pool)
            .await
            .into_iter()
            .map(|a| a.0)
            .collect();
        assert_eq!(actions, vec!["pair_reinstated", "rate_published"]);
    }

    #[tokio::test]
    async fn duplicate_effective_time_is_rate_version_exists() {
        let t = db().await;
        let pool = t.get_pool().await;
        let a = admin(&pool).await;
        let at = Utc::now() + Duration::hours(3);
        let scheduled = PublishRate {
            effective_at: Some(at),
            ..req(a, "USD", "JMD", dec!(161))
        };
        let v = publish_rate(&pool, &scheduled).await.expect("first");
        assert_eq!(v.state, common::exchange_rate::RateState::Scheduled);
        let again = publish_rate(&pool, &scheduled).await;
        assert!(
            matches!(again, Err(DbError::RateVersionExists)),
            "got {again:?}"
        );
    }

    #[tokio::test]
    async fn inactive_currency_cannot_get_a_rate() {
        let t = db().await;
        let pool = t.get_pool().await;
        let a = admin(&pool).await;
        sqlx::query("UPDATE currencies SET active = false WHERE code = 'CAD'")
            .execute(&pool)
            .await
            .expect("deactivate");
        let r = publish_rate(&pool, &req(a, "USD", "CAD", dec!(1.36))).await;
        assert!(
            matches!(r, Err(DbError::CurrencyInactive(ref c)) if c == "CAD"),
            "got {r:?}"
        );
    }

    #[tokio::test]
    async fn concurrent_publishes_serialise_and_second_compares_with_first() {
        let t = db().await;
        let pool = t.get_pool().await;
        let a = admin(&pool).await;

        // A holds the pair lock with a confirmed +6.25% rate.
        let mut tx_a = pool.begin().await.expect("tx A");
        let first = publish_rate_tx(
            &mut tx_a,
            &PublishRate {
                confirm_large_change: true,
                ..req(a, "USD", "JMD", dec!(170))
            },
        )
        .await
        .expect("A publishes");

        // B cannot get past the pair lock while A is open.
        let mut tx_b = pool.begin().await.expect("tx B");
        sqlx::query("SET LOCAL lock_timeout = '200ms'")
            .execute(&mut *tx_b)
            .await
            .expect("lock_timeout");
        let blocked = publish_rate_tx(&mut tx_b, &req(a, "USD", "JMD", dec!(171))).await;
        assert!(
            matches!(blocked, Err(DbError::LockContention(_))),
            "got {blocked:?}"
        );
        tx_b.rollback().await.expect("rollback B");
        tx_a.commit().await.expect("commit A");

        // Retried: 171 is within 5% of A's 170 (but >5% from the seeded 160),
        // so it only passes if A's row is the baseline; and it is later.
        let second = publish_rate(&pool, &req(a, "USD", "JMD", dec!(171)))
            .await
            .expect("B after A");
        assert!(second.effective_at > first.effective_at);
    }
}
