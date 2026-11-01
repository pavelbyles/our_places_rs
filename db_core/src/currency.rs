use crate::error::Result;
use rust_decimal::Decimal;
use sqlx::PgExecutor;
use std::collections::HashMap;

/// Gets the exchange rate from base_currency to target_currency.
/// If the pair does not exist, it falls back to USD as requested.
pub async fn get_exchange_rate_and_currency<'e, E>(
    executor: E,
    base_currency: &str,
    target_currency: &str,
) -> Result<(Decimal, String)>
where
    E: PgExecutor<'e> + Copy,
{
    if base_currency == target_currency {
        return Ok((Decimal::new(1, 0), target_currency.to_string()));
    }

    let rate = sqlx::query_scalar!(
        r#"
        SELECT rate FROM currency_exchange_rates
        WHERE base_currency = $1 AND target_currency = $2
        ORDER BY effective_at DESC
        LIMIT 1
        "#,
        base_currency,
        target_currency
    )
    .fetch_optional(executor)
    .await?;

    if let Some(rate) = rate {
        return Ok((rate, target_currency.to_string()));
    }

    // Fall back to USD if the requested pair is missing
    let fallback_currency = "USD";
    if base_currency == fallback_currency {
        return Ok((Decimal::new(1, 0), fallback_currency.to_string()));
    }

    // Try converting base to USD
    let rate_to_usd = sqlx::query_scalar!(
        r#"
        SELECT rate FROM currency_exchange_rates
        WHERE base_currency = $1 AND target_currency = $2
        ORDER BY effective_at DESC
        LIMIT 1
        "#,
        base_currency,
        fallback_currency
    )
    .fetch_optional(executor)
    .await?;

    Ok(rate_to_usd
        .map(|rate| (rate, fallback_currency.to_string()))
        .unwrap_or_else(|| (Decimal::new(1, 0), base_currency.to_string())))
}

/// Fetches exchange rates for multiple base currencies to a target currency, caching distinct rates in a map.
pub async fn get_exchange_rates_cache<'e, E, I, S>(
    executor: E,
    base_currencies: I,
    target_currency: &str,
) -> Result<HashMap<String, (Decimal, String)>>
where
    E: PgExecutor<'e> + Copy,
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut rates = HashMap::new();
    for base in base_currencies {
        let base_ref = base.as_ref();
        if !rates.contains_key(base_ref) {
            let rate_info =
                get_exchange_rate_and_currency(executor, base_ref, target_currency).await?;
            rates.insert(base_ref.to_string(), rate_info);
        }
    }
    Ok(rates)
}

/// Lists the newest rate for every stored currency pair (the one
/// [`get_exchange_rate_and_currency`] uses), sorted by base then target
/// currency. Read-only; one row per pair.
#[tracing::instrument(skip(executor))]
pub async fn list_latest_exchange_rates<'e, E>(
    executor: E,
) -> Result<Vec<common::exchange_rate::ExchangeRate>>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query!(
        r#"
        SELECT DISTINCT ON (base_currency, target_currency)
               base_currency, target_currency, rate, effective_at
        FROM currency_exchange_rates
        ORDER BY base_currency, target_currency, effective_at DESC
        "#
    )
    .fetch_all(executor)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| common::exchange_rate::ExchangeRate {
            base_currency: r.base_currency,
            target_currency: r.target_currency,
            rate: r.rate,
            effective_at: r.effective_at,
        })
        .collect())
}

#[cfg(test)]
mod migration_tests {
    //! Guarantees of the `currency_reference_and_rate_management` migration.

    use sqlx::PgPool;
    use sqlx::migrate::Migrator;
    use sqlx_db_tester::TestPg;
    use std::env;
    use std::path::{Path, PathBuf};

    /// File stem of the migration under test.
    const CURRENCY_MIGRATION: &str = "currency_reference_and_rate_management";

    /// Every currency-code column in the schema: (table, column).
    const CURRENCY_COLUMNS: [(&str, &str); 10] = [
        ("currency_exchange_rates", "base_currency"),
        ("currency_exchange_rates", "target_currency"),
        ("listing", "base_currency"),
        ("user", "default_currency"),
        ("user_history", "default_currency"),
        ("booking", "currency"),
        ("booking_history", "currency"),
        ("host_payout_ledger", "currency"),
        ("tax_document", "currency"),
        ("reservation_change_request", "currency"),
    ];

    fn db_url() -> String {
        dotenvy::dotenv().ok();
        env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/our_places".to_string()
        })
    }

    async fn migrated_db() -> TestPg {
        TestPg::new(db_url(), Path::new("migrations"))
    }

    /// A temp copy of every migration that precedes the one under test.
    fn previous_migrations_dir() -> PathBuf {
        let dir = env::temp_dir().join(format!("op_prev_migrations_{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).expect("create temp migrations dir");
        let mut names: Vec<_> = std::fs::read_dir("migrations")
            .expect("read migrations dir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".sql"))
            .collect();
        names.sort();
        let cutoff = names
            .iter()
            .position(|n| n.contains(CURRENCY_MIGRATION))
            .expect("currency migration exists");
        for name in &names[..cutoff] {
            std::fs::copy(Path::new("migrations").join(name), dir.join(name))
                .expect("copy migration");
        }
        dir
    }

    async fn apply_remaining_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
        Migrator::new(Path::new("migrations"))
            .await
            .expect("load migrations")
            .run(pool)
            .await
    }

    #[tokio::test]
    async fn every_currency_column_references_currencies() {
        let test_db = migrated_db().await;
        let pool = test_db.get_pool().await;
        for (table, column) in CURRENCY_COLUMNS {
            let fk: Option<(String, String)> = sqlx::query_as(
                r#"
                SELECT c.confupdtype::text, c.confdeltype::text
                FROM pg_constraint c
                JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = ANY (c.conkey)
                WHERE c.contype = 'f'
                  AND c.conrelid = format('%I', $1::text)::regclass
                  AND c.confrelid = 'currencies'::regclass
                  AND a.attname = $2
                "#,
            )
            .bind(table)
            .bind(column)
            .fetch_optional(&pool)
            .await
            .expect("query constraints");
            // 'r' = RESTRICT for both ON UPDATE and ON DELETE.
            assert_eq!(
                fk,
                Some(("r".to_string(), "r".to_string())),
                "{table}.{column} must reference currencies(code) ON UPDATE/DELETE RESTRICT"
            );
        }
    }

    #[tokio::test]
    async fn unknown_currency_is_rejected_and_currencies_cannot_be_deleted() {
        let test_db = migrated_db().await;
        let pool = test_db.get_pool().await;

        let unknown = sqlx::query(
            "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate) VALUES ('XXX', 'USD', 1)",
        )
        .execute(&pool)
        .await;
        assert!(
            matches!(&unknown, Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("23503")),
            "unknown currency must violate the FK, got {unknown:?}"
        );

        let delete = sqlx::query("DELETE FROM currencies WHERE code = 'USD'")
            .execute(&pool)
            .await;
        assert!(delete.is_err(), "deleting a referenced currency must fail");

        let seeded: Vec<String> =
            sqlx::query_scalar("SELECT code::text FROM currencies ORDER BY code")
                .fetch_all(&pool)
                .await
                .expect("list currencies");
        for code in ["CAD", "EUR", "GBP", "JMD", "USD"] {
            assert!(seeded.iter().any(|c| c == code), "{code} must be seeded");
        }
    }

    #[tokio::test]
    async fn rate_checks_reject_non_positive_and_self_pairs() {
        let test_db = migrated_db().await;
        let pool = test_db.get_pool().await;
        for sql in [
            "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate) VALUES ('USD', 'EUR', 0)",
            "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate) VALUES ('USD', 'EUR', -1)",
            "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate) VALUES ('USD', 'USD', 1)",
        ] {
            let res = sqlx::query(sql).execute(&pool).await;
            assert!(
                matches!(&res, Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("23514")),
                "{sql} must violate a CHECK, got {res:?}"
            );
        }
    }

    #[tokio::test]
    async fn audit_log_is_append_only() {
        let test_db = migrated_db().await;
        let pool = test_db.get_pool().await;
        let actor: uuid::Uuid = sqlx::query_scalar(
            r#"INSERT INTO "user" (email, password_hash, first_name, last_name, slug)
               VALUES ('audit-' || gen_random_uuid() || '@example.com', 'x', 'Audit', 'Actor',
                       'audit-' || replace(gen_random_uuid()::text, '-', ''))
               RETURNING id"#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert actor");
        sqlx::query(
            "INSERT INTO currency_audit_log (actor_id, action, currency_code, details) VALUES ($1, 'currency_created', 'USD', '{}')",
        )
        .bind(actor)
        .execute(&pool)
        .await
        .expect("insert audit row");

        for sql in [
            "UPDATE currency_audit_log SET action = 'currency_updated'",
            "DELETE FROM currency_audit_log",
            "TRUNCATE currency_audit_log",
        ] {
            let res = sqlx::query(sql).execute(&pool).await;
            assert!(res.is_err(), "{sql} must be rejected on the audit log");
        }
    }

    #[tokio::test]
    async fn legacy_codes_are_normalised_and_clean_rows_untouched() {
        let prev = previous_migrations_dir();
        let test_db = TestPg::new(db_url(), prev.clone());
        let pool = test_db.get_pool().await;

        sqlx::query(
            "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate) VALUES ('usd', 'Cad', 1.36)",
        )
        .execute(&pool)
        .await
        .expect("insert legacy lowercase rate");
        let clean_xmin_before: String = sqlx::query_scalar(
            "SELECT xmin::text FROM currency_exchange_rates WHERE base_currency = 'USD' AND target_currency = 'JMD'",
        )
        .fetch_one(&pool)
        .await
        .expect("seeded USD/JMD row");

        apply_remaining_migrations(&pool)
            .await
            .expect("currency migration applies to legacy data");

        let normalised: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM currency_exchange_rates WHERE base_currency = 'USD' AND target_currency = 'CAD'",
        )
        .fetch_one(&pool)
        .await
        .expect("count normalised");
        assert_eq!(normalised, 1, "lowercase codes are upper-cased");

        let clean_xmin_after: String = sqlx::query_scalar(
            "SELECT xmin::text FROM currency_exchange_rates WHERE base_currency = 'USD' AND target_currency = 'JMD'",
        )
        .fetch_one(&pool)
        .await
        .expect("seeded USD/JMD row");
        assert_eq!(
            clean_xmin_before, clean_xmin_after,
            "already-normalised rows must not be rewritten (would fire updated_at triggers)"
        );

        let _ = std::fs::remove_dir_all(prev);
    }

    #[tokio::test]
    async fn invalid_legacy_code_aborts_with_table_and_column() {
        let prev = previous_migrations_dir();
        let test_db = TestPg::new(db_url(), prev.clone());
        let pool = test_db.get_pool().await;

        sqlx::query(
            "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate) VALUES ('U1D', 'JMD', 1)",
        )
        .execute(&pool)
        .await
        .expect("insert invalid legacy code");

        let err = apply_remaining_migrations(&pool)
            .await
            .expect_err("invalid code must abort the migration");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("currency_exchange_rates")
                && msg.contains("base_currency")
                && msg.contains("U1D"),
            "error must name table, column and value: {msg}"
        );

        let _ = std::fs::remove_dir_all(prev);
    }
}
