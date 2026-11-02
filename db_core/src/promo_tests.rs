//! Promo code schema, usage and redemption tests.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sqlx::migrate::Migrator;
use sqlx::{PgPool, Row};
use sqlx_db_tester::TestPg;
use std::env;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// File stem of the migration under test.
const PROMO_MIGRATION: &str = "promo_codes";

fn db_url() -> String {
    dotenvy::dotenv().ok();
    env::var(crate::DATABASE_URL_ENV)
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".to_string())
}

fn migrated_db() -> TestPg {
    TestPg::new(db_url(), Path::new("migrations"))
}

fn migration_names() -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir("migrations")
        .expect("read migrations dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".sql"))
        .collect();
    names.sort();
    names
}

/// A temp copy of every migration that precedes the promo migration.
fn previous_migrations_dir() -> PathBuf {
    let dir = env::temp_dir().join(format!("op_prev_promo_migrations_{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).expect("create temp migrations dir");
    let names = migration_names();
    let cutoff = names
        .iter()
        .position(|n| n.contains(PROMO_MIGRATION))
        .expect("promo migration exists");
    for name in &names[..cutoff] {
        std::fs::copy(Path::new("migrations").join(name), dir.join(name)).expect("copy migration");
    }
    dir
}

/// The SQL the migration schedules for `cleanup_stale_pending_holds`. Test
/// databases skip the block that schedules it, and `cron.schedule` doesn't
/// validate its text, so this is the only check that the job body runs.
fn cron_job_sql() -> String {
    let name = migration_names()
        .into_iter()
        .find(|n| n.contains(PROMO_MIGRATION))
        .expect("promo migration exists");
    let text = std::fs::read_to_string(Path::new("migrations").join(name)).expect("read migration");
    let start = text
        .find("'WITH due AS (")
        .expect("job body starts with the expiry CTE")
        + 1;
    let end = text[start..]
        .find("FROM expired_bookings;'")
        .expect("job body ends after the history insert")
        + start
        + "FROM expired_bookings;".len();
    text[start..end].replace("''", "'")
}

async fn raw_user(pool: &PgPool) -> Uuid {
    sqlx::query_scalar(
        r#"INSERT INTO "user" (email, password_hash, first_name, last_name, slug)
           VALUES ('promo-' || gen_random_uuid() || '@example.com', 'x', 'Promo', 'Test',
                   'promo-' || replace(gen_random_uuid()::text, '-', ''))
           RETURNING id"#,
    )
    .fetch_one(pool)
    .await
    .expect("insert user")
}

async fn raw_listing(pool: &PgPool, host: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO listing (name, listing_structure_id, country, added_at, user_id, slug)
         VALUES ('Villa', 1, 'Jamaica', NOW(), $1, 'villa-' || replace(gen_random_uuid()::text, '-', ''))
         RETURNING id",
    )
    .bind(host)
    .fetch_one(pool)
    .await
    .expect("insert listing")
}

async fn raw_booking(pool: &PgPool, guest: Uuid, listing: Uuid, status: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO booking (confirmation_code, guest_id, listing_id, date_from, date_to, daily_rate,
             number_of_persons, total_days, sub_total_price, total_price, cancellation_policy, status)
         VALUES (upper(substr(md5(random()::text), 1, 10)), $1, $2, '2030-01-01', '2030-01-03', 100,
             1, 2, 200, 200, 'flexible', $3::booking_status)
         RETURNING id",
    )
    .bind(guest)
    .bind(listing)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("insert booking")
}

async fn insert_code(pool: &PgPool, created_by: Uuid, code: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO promo_code (code, discount_type, discount_value, scope, valid_from, valid_to,
             applies_to_all, created_by)
         VALUES ($1, 'percentage', 10, 'stay', NOW() - INTERVAL '1 day', NOW() + INTERVAL '1 day', TRUE, $2)
         RETURNING id",
    )
    .bind(code)
    .bind(created_by)
    .fetch_one(pool)
    .await
    .expect("insert promo code")
}

#[tokio::test]
async fn promo_schema_rows_are_unchanged_by_the_migration() {
    let prev = previous_migrations_dir();
    let test_db = TestPg::new(db_url(), prev.clone());
    let pool = test_db.get_pool().await;

    let host = raw_user(&pool).await;
    let guest = raw_user(&pool).await;
    let villa = raw_listing(&pool, host).await;
    let booking = raw_booking(&pool, guest, villa, "confirmed").await;
    sqlx::query(
        "INSERT INTO host_payout_ledger (booking_id, listing_id, host_id, gross_amount, net_payout_amount)
         VALUES ($1, $2, $3, 400, 360)",
    )
    .bind(booking)
    .bind(villa)
    .bind(host)
    .execute(&pool)
    .await
    .expect("insert ledger row");
    let snapshot = "SELECT b.xmin::text AS booking_xmin, l.xmin::text AS ledger_xmin, l.updated_at::text AS ledger_updated
                    FROM booking b JOIN host_payout_ledger l ON l.booking_id = b.id WHERE b.id = $1";
    let read = |row: sqlx::postgres::PgRow| -> (String, String, String) {
        (
            row.get("booking_xmin"),
            row.get("ledger_xmin"),
            row.get("ledger_updated"),
        )
    };
    let before = read(
        sqlx::query(snapshot)
            .bind(booking)
            .fetch_one(&pool)
            .await
            .unwrap(),
    );

    Migrator::new(Path::new("migrations"))
        .await
        .expect("load migrations")
        .run(&pool)
        .await
        .expect("promo migration applies to existing data");

    let after = read(
        sqlx::query(snapshot)
            .bind(booking)
            .fetch_one(&pool)
            .await
            .unwrap(),
    );
    assert_eq!(before, after, "existing rows must not be rewritten");

    let ledger = sqlx::query(
        "SELECT gross_amount, net_payout_amount, cleaning_fee_amount, promo_discount_amount
         FROM host_payout_ledger WHERE booking_id = $1",
    )
    .bind(booking)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ledger.get::<Decimal, _>("gross_amount"), dec!(400));
    assert_eq!(ledger.get::<Decimal, _>("net_payout_amount"), dec!(360));
    assert_eq!(
        ledger.get::<Decimal, _>("cleaning_fee_amount"),
        Decimal::ZERO
    );
    assert_eq!(
        ledger.get::<Decimal, _>("promo_discount_amount"),
        Decimal::ZERO
    );

    let row = sqlx::query(
        "SELECT promo_code_id, promo_stay_discount, promo_cleaning_discount FROM booking WHERE id = $1",
    )
    .bind(booking)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.get::<Option<Uuid>, _>("promo_code_id"), None);
    assert_eq!(row.get::<Decimal, _>("promo_stay_discount"), Decimal::ZERO);
    assert_eq!(
        row.get::<Decimal, _>("promo_cleaning_discount"),
        Decimal::ZERO
    );

    let _ = std::fs::remove_dir_all(prev);
}

#[tokio::test]
async fn promo_schema_cron_job_snapshots_promo_columns_of_an_expired_hold() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let host = raw_user(&pool).await;
    let guest = raw_user(&pool).await;
    let villa = raw_listing(&pool, host).await;
    let code = insert_code(&pool, host, "EXPIRED10").await;
    let hold = raw_booking(&pool, guest, villa, "pending").await;
    sqlx::query(
        r#"UPDATE booking SET expires_at = NOW() - INTERVAL '1 minute', promo_code_id = $2,
               promo_terms = '{"code":"EXPIRED10"}', promo_stay_discount = 20, promo_cleaning_discount = 5
           WHERE id = $1"#,
    )
    .bind(hold)
    .bind(code)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::raw_sql(&cron_job_sql())
        .execute(&pool)
        .await
        .expect("the scheduled job body runs");

    let status: String = sqlx::query_scalar("SELECT status::text FROM booking WHERE id = $1")
        .bind(hold)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "cancelled");
    let history = sqlx::query(
        "SELECT promo_code_id, promo_terms->>'code' AS code, promo_stay_discount, promo_cleaning_discount
         FROM booking_history WHERE booking_id = $1 AND change_reason = 'Hold expired'",
    )
    .bind(hold)
    .fetch_one(&pool)
    .await
    .expect("history row");
    assert_eq!(history.get::<Option<Uuid>, _>("promo_code_id"), Some(code));
    assert_eq!(
        history.get::<Option<String>, _>("code").as_deref(),
        Some("EXPIRED10")
    );
    assert_eq!(
        history.get::<Option<Decimal>, _>("promo_stay_discount"),
        Some(dec!(20))
    );
    assert_eq!(
        history.get::<Option<Decimal>, _>("promo_cleaning_discount"),
        Some(dec!(5))
    );
}

#[tokio::test]
async fn promo_schema_checks_reject_invalid_codes() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let user = raw_user(&pool).await;
    let insert = |code: &'static str,
                  kind: &'static str,
                  value: i32,
                  currency: Option<&'static str>| {
        sqlx::query(
            "INSERT INTO promo_code (code, discount_type, discount_value, currency, scope, valid_from,
                 valid_to, applies_to_all, created_by)
             VALUES ($1, $2::promo_discount_type, $3, $4, 'stay', NOW(), NOW() + INTERVAL '1 day', TRUE, $5)",
        )
        .bind(code)
        .bind(kind)
        .bind(value)
        .bind(currency)
        .bind(user)
    };

    for (code, kind, value, currency, sqlstate) in [
        ("TOOMUCH", "percentage", 101, None, "23514"),
        ("NOCURRENCY", "fixed", 10, None, "23514"),
        ("lowercase", "percentage", 10, None, "23514"),
        ("-LEADING", "percentage", 10, None, "23514"),
        ("ABC", "percentage", 10, None, "23514"),
        ("BADCURRENCY", "fixed", 10, Some("ZZZ"), "23503"),
    ] {
        let res = insert(code, kind, value, currency).execute(&pool).await;
        assert!(
            matches!(&res, Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some(sqlstate)),
            "{code} must fail with {sqlstate}, got {res:?}"
        );
    }

    insert("SUMMER-10", "percentage", 100, None)
        .execute(&pool)
        .await
        .expect("a 100% percentage code is valid");
    let duplicate = insert("SUMMER-10", "percentage", 10, None)
        .execute(&pool)
        .await;
    assert!(
        matches!(&duplicate, Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("23505")),
        "codes are unique, got {duplicate:?}"
    );
}
