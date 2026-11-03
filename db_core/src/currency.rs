use crate::error::Result;
use sqlx::PgExecutor;

// ---------------------------------------------------------------------------
// Currency administration (#100)
// ---------------------------------------------------------------------------

use common::currency::{
    CreateCurrencyRequest, Currency, CurrencyCode, ListingRef, UpdateCurrencyRequest,
};
use common::exchange_rate::RateCoverageGap;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::error::{DbError, SQLSTATE_UNIQUE_VIOLATION, sqlstate};

/// Converts a code read from the database (guaranteed by a CHECK) to the
/// domain type, surfacing corruption instead of hiding it.
pub(crate) fn code_from_db(code: String) -> Result<CurrencyCode> {
    CurrencyCode::try_from(code.as_str())
        .map_err(|e| DbError::ValidationError(format!("stored currency code is invalid: {e}")))
}

/// One row for the append-only `currency_audit_log`.
pub(crate) struct AuditEntry<'a> {
    /// Admin performing the action (always the server-loaded user).
    pub actor_id: Uuid,
    /// One of the actions allowed by the table's CHECK.
    pub action: &'a str,
    /// Currency acted on (currency actions).
    pub currency_code: Option<&'a str>,
    /// Pair base (rate actions).
    pub base_currency: Option<&'a str>,
    /// Pair target (rate actions).
    pub target_currency: Option<&'a str>,
    /// Rate row acted on, if any (kept even after deletion).
    pub rate_id: Option<Uuid>,
    /// Action-specific details.
    pub details: serde_json::Value,
}

/// Appends `entry` to the append-only `currency_audit_log`.
pub(crate) async fn write_audit(conn: &mut PgConnection, entry: AuditEntry<'_>) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO currency_audit_log
            (actor_id, action, currency_code, base_currency, target_currency, rate_id, details)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        entry.actor_id,
        entry.action,
        entry.currency_code,
        entry.base_currency,
        entry.target_currency,
        entry.rate_id,
        entry.details
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// All currencies (or only active ones), sorted by code.
#[tracing::instrument(skip(executor))]
pub async fn list_currencies<'e, E>(executor: E, active_only: bool) -> Result<Vec<Currency>>
where
    E: PgExecutor<'e>,
{
    sqlx::query!(
        r#"
        SELECT code, name, active FROM currencies
        WHERE active OR NOT $1
        ORDER BY code
        "#,
        active_only
    )
    .fetch_all(executor)
    .await?
    .into_iter()
    .map(|r| {
        code_from_db(r.code).map(|code| Currency {
            code,
            name: r.name,
            active: r.active,
        })
    })
    .collect()
}

/// Fails unless `code` exists and is active. Takes `FOR KEY SHARE` on the
/// currency row so a concurrent deactivation waits for the caller's
/// transaction (and then sees whatever it wrote).
#[tracing::instrument(skip(conn))]
pub async fn ensure_active(conn: &mut PgConnection, code: &str) -> Result<()> {
    let active = sqlx::query_scalar!(
        r#"SELECT active FROM currencies WHERE code = $1 FOR KEY SHARE"#,
        code
    )
    .fetch_optional(&mut *conn)
    .await?;
    match active {
        None => Err(DbError::CurrencyUnknown(code.to_string())),
        Some(false) => Err(DbError::CurrencyInactive(code.to_string())),
        Some(true) => Ok(()),
    }
}

/// Live (not deleted) listings priced in `code`, oldest first.
pub(crate) async fn live_listings_with_base(
    conn: &mut PgConnection,
    code: &str,
) -> Result<Vec<ListingRef>> {
    Ok(sqlx::query!(
        r#"
        SELECT id, name, slug FROM listing
        WHERE base_currency = $1 AND deleted_at IS NULL
        ORDER BY added_at, id
        "#,
        code
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| ListingRef {
        id: r.id,
        title: r.name,
        slug: r.slug,
    })
    .collect())
}

/// Adds a currency (audited).
#[tracing::instrument(skip(pool, req), fields(code = %req.code))]
pub async fn create_currency(
    pool: &PgPool,
    req: &CreateCurrencyRequest,
    actor_id: Uuid,
) -> Result<Currency> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query!(
        r#"
        INSERT INTO currencies (code, name, created_by) VALUES ($1, $2, $3)
        ON CONFLICT (code) DO NOTHING
        RETURNING code, name, active
        "#,
        req.code.as_str(),
        req.name,
        actor_id
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| match sqlstate(&e).as_deref() {
        Some(SQLSTATE_UNIQUE_VIOLATION) => DbError::CurrencyExists(req.code.to_string()),
        _ => DbError::Sqlx(e),
    })?
    .ok_or_else(|| DbError::CurrencyExists(req.code.to_string()))?;
    write_audit(
        &mut tx,
        AuditEntry {
            actor_id,
            action: "currency_created",
            currency_code: Some(req.code.as_str()),
            base_currency: None,
            target_currency: None,
            rate_id: None,
            details: serde_json::json!({ "name": row.name }),
        },
    )
    .await?;
    tx.commit().await?;
    code_from_db(row.code).map(|code| Currency {
        code,
        name: row.name,
        active: row.active,
    })
}

/// Renames and/or (de)activates a currency inside `tx` (audited).
///
/// Deactivation locks the currency row `FOR UPDATE` and is refused with
/// [`DbError::CurrencyInUse`] while live listings are priced in it. A request
/// that changes nothing is a no-op and writes no audit row.
pub async fn update_currency_tx(
    tx: &mut PgConnection,
    code: &CurrencyCode,
    req: &UpdateCurrencyRequest,
    actor_id: Uuid,
) -> Result<Currency> {
    let current = sqlx::query!(
        r#"SELECT name, active FROM currencies WHERE code = $1 FOR UPDATE"#,
        code.as_str()
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| DbError::from_sqlx_contended(e, "currency update"))?
    .ok_or_else(|| DbError::CurrencyNotFound(code.to_string()))?;

    let new_name = req
        .name
        .clone()
        .filter(|n| *n != current.name)
        .unwrap_or_else(|| current.name.clone());
    let new_active = req.active.unwrap_or(current.active);
    let renamed = new_name != current.name;
    let toggled = new_active != current.active;

    if toggled && !new_active {
        let blocking = live_listings_with_base(tx, code.as_str()).await?;
        if !blocking.is_empty() {
            return Err(DbError::CurrencyInUse(blocking));
        }
    }
    if renamed || toggled {
        sqlx::query!(
            r#"UPDATE currencies SET name = $2, active = $3, updated_at = now() WHERE code = $1"#,
            code.as_str(),
            new_name,
            new_active
        )
        .execute(&mut *tx)
        .await?;
    }
    if renamed {
        write_audit(
            tx,
            AuditEntry {
                actor_id,
                action: "currency_updated",
                currency_code: Some(code.as_str()),
                base_currency: None,
                target_currency: None,
                rate_id: None,
                details: serde_json::json!({ "from": current.name, "to": new_name }),
            },
        )
        .await?;
    }
    if toggled {
        let action = if new_active {
            "currency_activated"
        } else {
            "currency_deactivated"
        };
        write_audit(
            tx,
            AuditEntry {
                actor_id,
                action,
                currency_code: Some(code.as_str()),
                base_currency: None,
                target_currency: None,
                rate_id: None,
                details: serde_json::json!({}),
            },
        )
        .await?;
    }
    Ok(Currency {
        code: code.clone(),
        name: new_name,
        active: new_active,
    })
}

/// [`update_currency_tx`] in its own transaction.
#[tracing::instrument(skip(pool, req), fields(code = %code))]
pub async fn update_currency(
    pool: &PgPool,
    code: &CurrencyCode,
    req: &UpdateCurrencyRequest,
    actor_id: Uuid,
) -> Result<Currency> {
    let mut tx = pool.begin().await?;
    let currency = update_currency_tx(&mut tx, code, req, actor_id).await?;
    tx.commit()
        .await
        .map_err(|e| DbError::from_sqlx_contended(e, "currency update"))?;
    Ok(currency)
}

/// Every (live listing base currency, active currency) combination that cannot
/// be quoted at `now`: no non-retired rate in either direction is effective.
#[tracing::instrument(skip(executor))]
pub async fn rate_coverage_gaps<'e, E>(
    executor: E,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<RateCoverageGap>>
where
    E: PgExecutor<'e>,
{
    sqlx::query!(
        r#"
        SELECT b.base AS "base!", c.code AS "target!"
        FROM (SELECT DISTINCT base_currency AS base FROM listing WHERE deleted_at IS NULL) b
        JOIN currencies c ON c.active AND c.code <> b.base
        WHERE NOT EXISTS (
            SELECT 1 FROM currency_exchange_rates r
            WHERE r.retired_at IS NULL AND r.effective_at <= $1
              AND ((r.base_currency = b.base AND r.target_currency = c.code)
                OR (r.base_currency = c.code AND r.target_currency = b.base))
        )
        ORDER BY 1, 2
        "#,
        now
    )
    .fetch_all(executor)
    .await?
    .into_iter()
    .map(|r| {
        Ok(RateCoverageGap {
            listing_base_currency: code_from_db(r.base)?,
            target_currency: code_from_db(r.target)?,
        })
    })
    .collect()
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
        env::var(crate::DATABASE_URL_ENV).unwrap_or_else(|_| {
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

#[cfg(test)]
mod currency_admin_tests {
    //! Currency administration: create, rename, (de)activate, in-use guard,
    //! audit rows, coverage gaps and lock ordering against listing writes.

    use super::*;
    use crate::models::{NewHostProfile, NewListing, NewUser, UserRole};
    use chrono::{Duration, TimeZone, Utc};
    use rust_decimal_macros::dec;
    use sqlx_db_tester::TestPg;
    use std::env;
    use std::path::Path;

    async fn db() -> TestPg {
        dotenvy::dotenv().ok();
        let url = env::var(crate::DATABASE_URL_ENV).unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/our_places".to_string()
        });
        TestPg::new(url, Path::new("migrations"))
    }

    fn code(c: &str) -> CurrencyCode {
        CurrencyCode::try_from(c).expect("valid code")
    }

    async fn user(pool: &PgPool, role: UserRole) -> Uuid {
        let id = Uuid::now_v7();
        crate::user::create_user(
            pool,
            &NewUser {
                id,
                email: format!("u_{id}@example.com"),
                password_hash: "hash".into(),
                first_name: "Test".into(),
                last_name: "User".into(),
                phone_number: None,
                is_active: true,
                is_verified: true,
                verification_code: None,
                verification_code_expires_at: None,
                attributes: serde_json::json!({}),
                roles: Some(vec![role]),
                default_currency: "USD".into(),
            },
        )
        .await
        .expect("create user");
        id
    }

    async fn host(pool: &PgPool) -> Uuid {
        let id = user(pool, UserRole::Host).await;
        crate::user::create_host_profile(
            pool,
            id,
            &NewHostProfile {
                description: None,
                verified_status: Some("pending".into()),
                payout_details: None,
            },
        )
        .await
        .expect("host profile");
        id
    }

    fn listing(host_id: Uuid, base: &str) -> NewListing {
        NewListing {
            name: format!("Villa {}", Uuid::now_v7()),
            user_id: host_id,
            description: None,
            listing_structure_id: 1,
            country: "Jamaica".into(),
            price_per_night: Some(dec!(100)),
            weekly_discount_percentage: None,
            monthly_discount_percentage: None,
            max_guests: 2,
            bedrooms: 1,
            beds: 1,
            full_bathrooms: 1,
            half_bathrooms: 0,
            square_meters: None,
            latitude: None,
            longitude: None,
            listing_details: None,
            city: None,
            base_currency: base.into(),
            minimum_stay: 1,
            days_between_bookings: 0,
            commission_pct: None,
            cleaning_fee: None,
            damage_deposit: None,
            guests_included: None,
            extra_guest_fee: None,
            is_active: true,
        }
    }

    async fn audit_count(pool: &PgPool, code: &str) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM currency_audit_log WHERE currency_code = $1")
            .bind(code)
            .fetch_one(pool)
            .await
            .expect("count audit")
    }

    fn deactivate() -> UpdateCurrencyRequest {
        UpdateCurrencyRequest {
            name: None,
            active: Some(false),
        }
    }

    #[tokio::test]
    async fn create_lists_and_audits() {
        let t = db().await;
        let pool = t.get_pool().await;
        let admin = user(&pool, UserRole::Admin).await;
        let req = CreateCurrencyRequest {
            code: code("TTD"),
            name: "Trinidad Dollar".into(),
        };
        let created = create_currency(&pool, &req, admin).await.expect("create");
        assert!(created.active);
        let all = list_currencies(&pool, true).await.expect("list");
        assert!(all.iter().any(|c| c.code.as_str() == "TTD"));
        assert_eq!(audit_count(&pool, "TTD").await, 1);
    }

    #[tokio::test]
    async fn duplicate_create_is_currency_exists() {
        let t = db().await;
        let pool = t.get_pool().await;
        let admin = user(&pool, UserRole::Admin).await;
        let req = CreateCurrencyRequest {
            code: code("USD"),
            name: "Dup".into(),
        };
        let r = create_currency(&pool, &req, admin).await;
        assert!(
            matches!(&r, Err(DbError::CurrencyExists(c)) if c == "USD"),
            "got {r:?}"
        );
    }

    #[tokio::test]
    async fn deactivate_blocked_by_live_listing() {
        let t = db().await;
        let pool = t.get_pool().await;
        let admin = user(&pool, UserRole::Admin).await;
        let h = host(&pool).await;
        let l = crate::listing::create_listing(&pool, &listing(h, "CAD"))
            .await
            .expect("listing");
        let r = update_currency(&pool, &code("CAD"), &deactivate(), admin).await;
        match r {
            Err(DbError::CurrencyInUse(listings)) => {
                assert_eq!(listings.len(), 1);
                assert_eq!(listings[0].id, l.id);
            }
            other => panic!("expected CurrencyInUse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn soft_deleted_listing_does_not_block() {
        let t = db().await;
        let pool = t.get_pool().await;
        let admin = user(&pool, UserRole::Admin).await;
        let h = host(&pool).await;
        let l = crate::listing::create_listing(&pool, &listing(h, "CAD"))
            .await
            .expect("listing");
        sqlx::query("UPDATE listing SET deleted_at = now() WHERE id = $1")
            .bind(l.id)
            .execute(&pool)
            .await
            .expect("soft delete");
        let r = update_currency(&pool, &code("CAD"), &deactivate(), admin).await;
        assert!(matches!(r, Ok(Currency { active: false, .. })), "got {r:?}");
    }

    #[tokio::test]
    async fn reactivation_always_works() {
        let t = db().await;
        let pool = t.get_pool().await;
        let admin = user(&pool, UserRole::Admin).await;
        update_currency(&pool, &code("GBP"), &deactivate(), admin)
            .await
            .expect("deactivate");
        let back = update_currency(
            &pool,
            &code("GBP"),
            &UpdateCurrencyRequest {
                name: None,
                active: Some(true),
            },
            admin,
        )
        .await
        .expect("reactivate");
        assert!(back.active);
        assert_eq!(audit_count(&pool, "GBP").await, 2);
    }

    #[tokio::test]
    async fn no_op_update_writes_no_audit_row() {
        let t = db().await;
        let pool = t.get_pool().await;
        let admin = user(&pool, UserRole::Admin).await;
        let same = UpdateCurrencyRequest {
            name: Some("Euro".into()),
            active: Some(true),
        };
        update_currency(&pool, &code("EUR"), &same, admin)
            .await
            .expect("no-op update");
        assert_eq!(audit_count(&pool, "EUR").await, 0);
    }

    #[tokio::test]
    async fn rename_is_audited() {
        let t = db().await;
        let pool = t.get_pool().await;
        let admin = user(&pool, UserRole::Admin).await;
        let renamed = update_currency(
            &pool,
            &code("EUR"),
            &UpdateCurrencyRequest {
                name: Some("Euro (EU)".into()),
                active: None,
            },
            admin,
        )
        .await
        .expect("rename");
        assert_eq!(renamed.name, "Euro (EU)");
        assert_eq!(audit_count(&pool, "EUR").await, 1);
    }

    #[tokio::test]
    async fn update_of_unknown_code_is_not_found() {
        let t = db().await;
        let pool = t.get_pool().await;
        let admin = user(&pool, UserRole::Admin).await;
        let r = update_currency(&pool, &code("QQQ"), &deactivate(), admin).await;
        assert!(matches!(r, Err(DbError::CurrencyNotFound(_))), "got {r:?}");
    }

    #[tokio::test]
    async fn ensure_active_distinguishes_unknown_and_inactive() {
        let t = db().await;
        let pool = t.get_pool().await;
        let mut c = pool.acquire().await.expect("conn");
        assert!(ensure_active(&mut c, "USD").await.is_ok());
        let unknown = ensure_active(&mut c, "QQQ").await;
        assert!(
            matches!(unknown, Err(DbError::CurrencyUnknown(_))),
            "got {unknown:?}"
        );
        sqlx::query("UPDATE currencies SET active = false WHERE code = 'CAD'")
            .execute(&mut *c)
            .await
            .expect("deactivate");
        let inactive = ensure_active(&mut c, "CAD").await;
        assert!(
            matches!(inactive, Err(DbError::CurrencyInactive(_))),
            "got {inactive:?}"
        );
    }

    #[tokio::test]
    async fn coverage_gaps_list_unquotable_combinations() {
        let t = db().await;
        let pool = t.get_pool().await;
        let h = host(&pool).await;
        crate::listing::create_listing(&pool, &listing(h, "USD"))
            .await
            .expect("listing");
        let now = Utc
            .with_ymd_and_hms(2030, 1, 1, 0, 0, 0)
            .single()
            .expect("fixed now");
        // Seeds cover USD↔JMD and USD↔GBP; EUR and CAD have no USD pair.
        let gaps = rate_coverage_gaps(&pool, now).await.expect("gaps");
        let targets: Vec<&str> = gaps
            .iter()
            .filter(|g| g.listing_base_currency.as_str() == "USD")
            .map(|g| g.target_currency.as_str())
            .collect();
        assert_eq!(targets, vec!["CAD", "EUR"]);

        // A scheduled (future) rate does not close the gap yet.
        sqlx::query(
            "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
             VALUES ('EUR', 'USD', 1.08, $1)",
        )
        .bind(now + Duration::days(1))
        .execute(&pool)
        .await
        .expect("scheduled EUR rate");
        let still = rate_coverage_gaps(&pool, now).await.expect("gaps");
        assert!(still.iter().any(|g| g.target_currency.as_str() == "EUR"));
        let later = rate_coverage_gaps(&pool, now + Duration::days(2))
            .await
            .expect("gaps");
        assert!(!later.iter().any(|g| g.target_currency.as_str() == "EUR"));
    }

    #[tokio::test]
    async fn listing_write_holding_key_share_blocks_deactivation_until_commit() {
        let t = db().await;
        let pool = t.get_pool().await;
        let admin = user(&pool, UserRole::Admin).await;
        let h = host(&pool).await;

        // tx A: the listing write path takes FOR KEY SHARE on CAD and inserts.
        let mut a = pool.begin().await.expect("tx A");
        ensure_active(&mut a, "CAD").await.expect("CAD active");
        crate::listing::create_listing(&mut *a, &listing(h, "CAD"))
            .await
            .expect("listing in A");

        // tx B: deactivation must wait on A's lock → lock_timeout → retryable.
        let mut b = pool.begin().await.expect("tx B");
        sqlx::query("SET LOCAL lock_timeout = '200ms'")
            .execute(&mut *b)
            .await
            .expect("lock_timeout");
        let blocked = update_currency_tx(&mut b, &code("CAD"), &deactivate(), admin).await;
        assert!(
            matches!(blocked, Err(DbError::LockContention(_))),
            "got {blocked:?}"
        );
        b.rollback().await.expect("rollback B");

        a.commit().await.expect("commit A");

        // Retried after A commits: the new listing now blocks deactivation.
        let retried = update_currency(&pool, &code("CAD"), &deactivate(), admin).await;
        assert!(
            matches!(retried, Err(DbError::CurrencyInUse(ref l)) if l.len() == 1),
            "got {retried:?}"
        );
    }

    #[tokio::test]
    async fn deactivation_first_then_listing_write_sees_inactive() {
        let t = db().await;
        let pool = t.get_pool().await;
        let admin = user(&pool, UserRole::Admin).await;
        update_currency(&pool, &code("CAD"), &deactivate(), admin)
            .await
            .expect("deactivate CAD");
        let mut c = pool.acquire().await.expect("conn");
        let r = ensure_active(&mut c, "CAD").await;
        assert!(matches!(r, Err(DbError::CurrencyInactive(_))), "got {r:?}");
    }
}
