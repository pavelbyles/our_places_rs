//! Foreign exchange rate queries with direct and inverse resolution.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgPool};

use crate::error::{DbError, Result};

/// Retrieves the effective exchange rate from `base` to `target` at a specific timestamp.
///
/// Resolution order:
/// 1. Identity: If `base == target`, returns `1.0`.
/// 2. Direct pair: Looks for `base -> target` with `effective_at <= at`.
/// 3. Inverse pair: If direct is missing, looks for `target -> base` with `effective_at <= at`
///    and inverts: `(1.0 / rate).round_dp(6)`.
/// 4. If neither pair is available, returns `DbError::FxRateUnavailable`.
#[tracing::instrument(skip(conn))]
pub async fn get_rate(
    conn: &mut PgConnection,
    base: &str,
    target: &str,
    at: DateTime<Utc>,
) -> Result<Decimal> {
    if base.eq_ignore_ascii_case(target) {
        return Ok(Decimal::ONE);
    }

    let direct = sqlx::query_scalar!(
        r#"
        SELECT rate FROM currency_exchange_rates
        WHERE base_currency = $1 AND target_currency = $2 AND effective_at <= $3
        ORDER BY effective_at DESC, updated_at DESC
        LIMIT 1
        "#,
        base,
        target,
        at
    )
    .fetch_optional(&mut *conn)
    .await?;

    if let Some(rate) = direct {
        return Ok(rate);
    }

    // Attempt inverse lookup if direct pair was not found
    let inverse = sqlx::query_scalar!(
        r#"
        SELECT rate FROM currency_exchange_rates
        WHERE base_currency = $1 AND target_currency = $2 AND effective_at <= $3
        ORDER BY effective_at DESC, updated_at DESC
        LIMIT 1
        "#,
        target,
        base,
        at
    )
    .fetch_optional(&mut *conn)
    .await?;

    if let Some(inverse_rate) = inverse {
        if inverse_rate.is_zero() {
            return Err(DbError::ValidationError(
                "Inverse exchange rate is zero".to_string(),
            ));
        }
        return Ok((Decimal::ONE / inverse_rate).round_dp(6));
    }

    Err(DbError::FxRateUnavailable {
        base: base.to_string(),
        target: target.to_string(),
    })
}

/// Convenience helper to retrieve exchange rate using a connection pool.
pub async fn get_rate_pool(
    pool: &PgPool,
    base: &str,
    target: &str,
    at: DateTime<Utc>,
) -> Result<Decimal> {
    let mut conn = pool.acquire().await?;
    get_rate(&mut conn, base, target, at).await
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

    #[tokio::test]
    async fn test_exchange_rate_identity() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;
        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        let rate = get_rate(&mut tx, "USD", "USD", Utc::now())
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

        let rate = get_rate(&mut tx, "FOO", "BAR", now)
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
        let rate = get_rate(&mut tx, "FOO", "BAR", now)
            .await
            .expect("Inverse rate lookup failed");
        assert_eq!(rate, dec!(0.500000));
    }

    #[tokio::test]
    async fn test_exchange_rate_unavailable() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;
        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        let result = get_rate(&mut tx, "ZZZ", "YYY", Utc::now()).await;
        match result {
            Err(DbError::FxRateUnavailable { base, target }) => {
                assert_eq!(base, "ZZZ");
                assert_eq!(target, "YYY");
            }
            other => panic!("Expected FxRateUnavailable, got: {:?}", other),
        }
    }
}
