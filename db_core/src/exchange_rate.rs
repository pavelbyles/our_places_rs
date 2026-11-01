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
