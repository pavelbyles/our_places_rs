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

pub(crate) fn migrated_db() -> TestPg {
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

pub(crate) async fn raw_user(pool: &PgPool) -> Uuid {
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

pub(crate) async fn raw_listing(pool: &PgPool, host: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO listing (name, listing_structure_id, country, added_at, user_id, slug)
         VALUES ('Villa ' || gen_random_uuid(), 1, 'Jamaica', NOW(), $1, 'villa-' || replace(gen_random_uuid()::text, '-', ''))
         RETURNING id",
    )
    .bind(host)
    .fetch_one(pool)
    .await
    .expect("insert listing")
}

pub(crate) async fn raw_booking(pool: &PgPool, guest: Uuid, listing: Uuid, status: &str) -> Uuid {
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

pub(crate) async fn insert_code(pool: &PgPool, created_by: Uuid, code: &str) -> Uuid {
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

/// Reads a promo booking through the normal query path and takes it to
/// confirmation, checking the figures each column feeds.
mod booking_columns {
    use super::*;
    use crate::models::{BookingStatus, SupplierInfo, UpdatedBooking};
    use common::promo::{PromoDiscountType, PromoScope, PromoTerms};

    fn terms() -> PromoTerms {
        PromoTerms {
            code: "SUMMER".to_string(),
            discount_type: PromoDiscountType::Percentage,
            discount_value: dec!(10),
            scope: PromoScope::StayAndCleaning,
            currency: None,
            exchange_rate: Decimal::ONE,
            min_nights: None,
            min_booking_value: None,
            stay_from: None,
            stay_to: None,
        }
    }

    #[tokio::test]
    async fn promo_amounts_reach_net_figures_host_gross_and_history() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let guest = raw_user(&pool).await;
        let villa = raw_listing(&pool, host).await;
        sqlx::query("UPDATE listing SET commission_pct = 0.1 WHERE id = $1")
            .bind(villa)
            .execute(&pool)
            .await
            .unwrap();
        let code = insert_code(&pool, host, "SUMMER").await;
        let hold = raw_booking(&pool, guest, villa, "pending").await;
        sqlx::query(
            "UPDATE booking SET expires_at = NOW() + INTERVAL '1 hour', sub_total_price = 1000,
                 discount_value = 100, cleaning_fee = 100, promo_code_id = $2, promo_terms = $3,
                 promo_stay_discount = 90, promo_cleaning_discount = 10 WHERE id = $1",
        )
        .bind(hold)
        .bind(code)
        .bind(sqlx::types::Json(terms()))
        .execute(&pool)
        .await
        .unwrap();

        let read = crate::booking::get_booking_by_id(&pool, hold)
            .await
            .unwrap();
        assert_eq!(read.promo_code_id, Some(code));
        assert_eq!(read.promo_terms.as_ref().map(|t| &t.0), Some(&terms()));
        assert_eq!(read.accommodation_net(), dec!(810));
        assert_eq!(read.cleaning_net(), dec!(90));

        let supplier = SupplierInfo {
            supplier_name: "Our Places Jamaica Limited".to_string(),
            supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
            supplier_trn: "000-111-222".to_string(),
        };
        crate::booking::update_booking(
            &pool,
            hold,
            &UpdatedBooking {
                status: Some(BookingStatus::Confirmed),
                ..Default::default()
            },
            &supplier,
        )
        .await
        .unwrap();

        // (810 + 90) gross, less 10% commission.
        let ledger = sqlx::query(
            "SELECT gross_amount, net_payout_amount FROM host_payout_ledger WHERE booking_id = $1",
        )
        .bind(hold)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(ledger.get::<Decimal, _>("gross_amount"), dec!(900));
        assert_eq!(ledger.get::<Decimal, _>("net_payout_amount"), dec!(810));

        let history = sqlx::query(
            "SELECT promo_code_id, promo_stay_discount, promo_cleaning_discount
             FROM booking_history WHERE booking_id = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(hold)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(history.get::<Option<Uuid>, _>("promo_code_id"), Some(code));
        assert_eq!(
            history.get::<Option<Decimal>, _>("promo_stay_discount"),
            Some(dec!(90))
        );
        assert_eq!(
            history.get::<Option<Decimal>, _>("promo_cleaning_discount"),
            Some(dec!(10))
        );
    }
}

mod usage {
    use super::*;
    use crate::promo::{covers_listing, get_by_code, lock_and_count, usage_for_quote};

    async fn limited_code(pool: &PgPool, created_by: Uuid, code: &str) -> Uuid {
        let id = insert_code(pool, created_by, code).await;
        sqlx::query(
            "UPDATE promo_code SET total_usage_limit = 100, per_guest_limit = 100 WHERE id = $1",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    /// A booking redeeming `code`, with its hold still live.
    async fn redeemed(pool: &PgPool, guest: Uuid, villa: Uuid, code: Uuid, status: &str) -> Uuid {
        let booking = raw_booking(pool, guest, villa, status).await;
        sqlx::query("UPDATE booking SET expires_at = NOW() + INTERVAL '1 hour' WHERE id = $1")
            .bind(booking)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO promo_code_redemption (promo_code_id, booking_id) VALUES ($1, $2)",
        )
        .bind(code)
        .bind(booking)
        .execute(pool)
        .await
        .unwrap();
        booking
    }

    async fn counts(pool: &PgPool, code: Uuid, guest: Uuid) -> (i64, i64) {
        let mut conn = pool.acquire().await.unwrap();
        let (_, usage) = lock_and_count(&mut conn, code, guest).await.unwrap();
        (usage.total_used, usage.guest_used)
    }

    #[tokio::test]
    async fn counts_live_uses_and_leaves_out_the_rest() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let guest = raw_user(&pool).await;
        let other = raw_user(&pool).await;
        let villa = raw_listing(&pool, host).await;
        let code = limited_code(&pool, host, "COUNTED").await;

        redeemed(&pool, guest, villa, code, "pending").await;
        redeemed(&pool, guest, villa, code, "confirmed").await;
        redeemed(&pool, other, villa, code, "completed").await;
        assert_eq!(counts(&pool, code, guest).await, (3, 2));

        // Cancelled: not counted.
        redeemed(&pool, guest, villa, code, "cancelled").await;
        assert_eq!(counts(&pool, code, guest).await, (3, 2));

        // Expired but not yet swept: not counted.
        let expired = redeemed(&pool, guest, villa, code, "pending").await;
        sqlx::query("UPDATE booking SET expires_at = NOW() - INTERVAL '1 minute' WHERE id = $1")
            .bind(expired)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(counts(&pool, code, guest).await, (3, 2));

        // Released when a reservation change lost the discount: not counted.
        let released = redeemed(&pool, guest, villa, code, "confirmed").await;
        sqlx::query("UPDATE promo_code_redemption SET released_at = NOW() WHERE booking_id = $1")
            .bind(released)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(counts(&pool, code, guest).await, (3, 2));
    }

    #[tokio::test]
    async fn the_per_guest_count_follows_the_bookings_guest_after_a_transfer() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let guest = raw_user(&pool).await;
        let other = raw_user(&pool).await;
        let villa = raw_listing(&pool, host).await;
        let code = limited_code(&pool, host, "TRANSFER").await;
        let booking = redeemed(&pool, guest, villa, code, "pending").await;

        crate::booking::transfer_booking_guest(&pool, booking, other)
            .await
            .unwrap();

        assert_eq!(counts(&pool, code, guest).await, (1, 0));
        assert_eq!(counts(&pool, code, other).await, (1, 1));
    }

    #[tokio::test]
    async fn a_second_transaction_waits_on_the_code_lock() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let code = limited_code(&pool, host, "LOCKED").await;

        let mut holder = pool.begin().await.unwrap();
        lock_and_count(&mut holder, code, host).await.unwrap();

        let mut waiter = pool.begin().await.unwrap();
        sqlx::query("SET LOCAL lock_timeout = '200ms'")
            .execute(&mut *waiter)
            .await
            .unwrap();
        let err = sqlx::query("SELECT id FROM promo_code WHERE id = $1 FOR UPDATE")
            .bind(code)
            .execute(&mut *waiter)
            .await
            .unwrap_err();
        assert!(
            matches!(&err, sqlx::Error::Database(e) if e.code().as_deref() == Some("55P03")),
            "expected a lock timeout, got {err:?}"
        );
    }

    #[tokio::test]
    async fn the_lock_timeout_setting_does_not_linger() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let code = limited_code(&pool, host, "LINGER").await;

        let mut tx = pool.begin().await.unwrap();
        lock_and_count(&mut tx, code, host).await.unwrap();
        let timeout: String = sqlx::query_scalar("SHOW lock_timeout")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(timeout, "0", "later statements keep the default timeout");
    }

    #[tokio::test]
    async fn a_quote_leaves_out_the_guests_own_holds_but_a_hold_still_counts_them() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let guest = raw_user(&pool).await;
        let other = raw_user(&pool).await;
        let villa = raw_listing(&pool, host).await;
        let code_id = limited_code(&pool, host, "OWNHOLD").await;
        redeemed(&pool, guest, villa, code_id, "pending").await;
        redeemed(&pool, other, villa, code_id, "pending").await;
        redeemed(&pool, guest, villa, code_id, "confirmed").await;

        let code = get_by_code(&pool, "OWNHOLD").await.unwrap().unwrap();
        let usage = usage_for_quote(&pool, &code, guest).await.unwrap();
        assert_eq!((usage.total_used, usage.guest_used), (2, 1));

        assert_eq!(counts(&pool, code_id, guest).await, (3, 2));
    }

    #[tokio::test]
    async fn a_code_without_limits_is_not_counted() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let guest = raw_user(&pool).await;
        let villa = raw_listing(&pool, host).await;
        let code = insert_code(&pool, host, "NOLIMIT").await;
        redeemed(&pool, guest, villa, code, "confirmed").await;

        assert_eq!(counts(&pool, code, guest).await, (0, 0));
    }

    #[tokio::test]
    async fn coverage_follows_scope_and_ownership() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let other_host = raw_user(&pool).await;
        let villa = raw_listing(&pool, host).await;
        let other_villa = raw_listing(&pool, other_host).await;

        // An admin code on all listings covers everything.
        insert_code(&pool, host, "ADMINALL").await;
        let admin_all = get_by_code(&pool, "ADMINALL").await.unwrap().unwrap();
        assert!(covers_listing(&pool, &admin_all, villa).await.unwrap());
        assert!(
            covers_listing(&pool, &admin_all, other_villa)
                .await
                .unwrap()
        );

        // A selected code covers only its listings.
        let selected_id = insert_code(&pool, host, "SELECTED").await;
        sqlx::query("UPDATE promo_code SET applies_to_all = FALSE WHERE id = $1")
            .bind(selected_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO promo_code_listing (promo_code_id, listing_id) VALUES ($1, $2)")
            .bind(selected_id)
            .bind(villa)
            .execute(&pool)
            .await
            .unwrap();
        let selected = get_by_code(&pool, "SELECTED").await.unwrap().unwrap();
        assert!(covers_listing(&pool, &selected, villa).await.unwrap());
        assert!(!covers_listing(&pool, &selected, other_villa).await.unwrap());

        // A host's "all" covers only that host's listings, including one that's
        // transferred away later, even when it was selected.
        let hosted_id = insert_code(&pool, host, "HOSTALL").await;
        sqlx::query("UPDATE promo_code SET owner_host_id = $2 WHERE id = $1")
            .bind(hosted_id)
            .bind(host)
            .execute(&pool)
            .await
            .unwrap();
        let hosted = get_by_code(&pool, "HOSTALL").await.unwrap().unwrap();
        assert!(covers_listing(&pool, &hosted, villa).await.unwrap());
        assert!(!covers_listing(&pool, &hosted, other_villa).await.unwrap());
        sqlx::query("UPDATE listing SET user_id = $2 WHERE id = $1")
            .bind(villa)
            .bind(other_host)
            .execute(&pool)
            .await
            .unwrap();
        assert!(!covers_listing(&pool, &hosted, villa).await.unwrap());
    }
}

mod redemption {
    use super::*;
    use crate::error::DbError;
    use crate::models::{
        Booking, BookingMetadata, BookingStatus, CancellationPolicy, NewBooking, NewBookingPromo,
        SupplierInfo, UpdatedBooking,
    };
    use crate::promo::get_by_code;
    use chrono::NaiveDate;
    use common::promo::PromoRefusal;
    use std::time::Duration;

    const HOLD_MINUTES: i32 = 120;

    fn supplier() -> SupplierInfo {
        SupplierInfo {
            supplier_name: "Our Places Jamaica Limited".to_string(),
            supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
            supplier_trn: "000-111-222".to_string(),
        }
    }

    fn set_status(status: BookingStatus) -> UpdatedBooking {
        UpdatedBooking {
            status: Some(status),
            ..Default::default()
        }
    }

    fn new_booking(guest_id: Uuid, listing_id: Uuid, promo: Option<NewBookingPromo>) -> NewBooking {
        let from = NaiveDate::from_ymd_opt(2030, 3, 1).expect("valid date");
        let to = NaiveDate::from_ymd_opt(2030, 3, 3).expect("valid date");
        NewBooking {
            confirmation_code: Uuid::now_v7().simple().to_string()[22..].to_uppercase(),
            guest_id,
            listing_id,
            date_from: from,
            date_to: to,
            currency: "USD".to_string(),
            daily_rate: dec!(200.00),
            number_of_persons: 1,
            total_days: 2,
            sub_total_price: dec!(400.00),
            discount_value: None,
            tax_value: None,
            fee_breakdown: vec![],
            total_price: dec!(360.00),
            cancellation_policy: CancellationPolicy::Flexible,
            metadata: BookingMetadata {
                num_adults: 1,
                ..Default::default()
            },
            door_access_code: None,
            cleaning_fee: Decimal::ZERO,
            damage_deposit: Decimal::ZERO,
            platform_fee: Decimal::ZERO,
            tax_rate: dec!(0.1500),
            exchange_rate: Decimal::ONE,
            promo,
        }
    }

    async fn promo_of(pool: &PgPool, code: &str) -> NewBookingPromo {
        let found = get_by_code(pool, code)
            .await
            .expect("look up code")
            .expect("code exists");
        NewBookingPromo {
            promo_code_id: found.id,
            terms: found.terms(Decimal::ONE),
            stay_discount: dec!(40.00),
            cleaning_discount: Decimal::ZERO,
        }
    }

    async fn limit(pool: &PgPool, id: Uuid, total: Option<i32>, guest: Option<i32>) {
        sqlx::query(
            "UPDATE promo_code SET total_usage_limit = $2, per_guest_limit = $3 WHERE id = $1",
        )
        .bind(id)
        .bind(total)
        .bind(guest)
        .execute(pool)
        .await
        .expect("set limits");
    }

    async fn hold(
        pool: &PgPool,
        guest: Uuid,
        villa: Uuid,
        code: &str,
    ) -> crate::error::Result<Booking> {
        let promo = promo_of(pool, code).await;
        crate::booking::create_booking(pool, &new_booking(guest, villa, Some(promo)), HOLD_MINUTES)
            .await
    }

    fn refusal(res: crate::error::Result<Booking>) -> PromoRefusal {
        match res {
            Err(DbError::PromoRefused(r)) => r,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    async fn status_of(pool: &PgPool, id: Uuid) -> String {
        sqlx::query_scalar("SELECT status::text FROM booking WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .expect("booking status")
    }

    /// Waits until `n` backends are queued for a lock.
    async fn until_lock_waiters(pool: &PgPool, n: i64) {
        loop {
            let waiting: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM pg_locks WHERE NOT granted")
                    .fetch_one(pool)
                    .await
                    .expect("read pg_locks");
            if waiting >= n {
                return;
            }
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn a_hold_stores_the_snapshot_and_a_redemption() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let guest = raw_user(&pool).await;
        let villa = raw_listing(&pool, host).await;
        let code = insert_code(&pool, host, "SNAPSHOT").await;

        let booking = hold(&pool, guest, villa, "SNAPSHOT").await.unwrap();

        assert_eq!(booking.promo_code_id, Some(code));
        assert_eq!(booking.promo_stay_discount, dec!(40.00));
        assert_eq!(booking.accommodation_net(), dec!(360.00));
        assert_eq!(
            booking.promo_terms.as_ref().map(|t| t.0.code.as_str()),
            Some("SNAPSHOT")
        );
        let redeemed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM promo_code_redemption WHERE booking_id = $1 AND promo_code_id = $2",
        )
        .bind(booking.id)
        .bind(code)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(redeemed, 1);
    }

    /// Distinct guests on distinct listings, so only the promo lock can
    /// serialise them.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_holds_cannot_exceed_the_total_limit() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let code = insert_code(&pool, host, "LASTONE").await;
        limit(&pool, code, Some(1), None).await;

        let mut attempts = Vec::new();
        for _ in 0..8 {
            let guest = raw_user(&pool).await;
            let villa = raw_listing(&pool, host).await;
            let pool = pool.clone();
            attempts.push(tokio::spawn(async move {
                hold(&pool, guest, villa, "LASTONE").await
            }));
        }

        let mut successes = 0;
        let mut refusals = 0;
        for attempt in attempts {
            match attempt.await.expect("task ran") {
                Ok(_) => successes += 1,
                Err(DbError::PromoRefused(PromoRefusal::UsageLimitReached)) => refusals += 1,
                Err(other) => panic!("unexpected error {other:?}"),
            }
        }
        assert_eq!((successes, refusals), (1, 7));
    }

    /// Both holds are released together while a third transaction holds the
    /// code, so this can't pass by the holds simply not overlapping.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_holds_cannot_exceed_the_per_guest_limit() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let guest = raw_user(&pool).await;
        let code = insert_code(&pool, host, "PERGUEST").await;
        limit(&pool, code, None, Some(2)).await;

        let earlier = raw_booking(&pool, guest, raw_listing(&pool, host).await, "confirmed").await;
        sqlx::query(
            "INSERT INTO promo_code_redemption (promo_code_id, booking_id) VALUES ($1, $2)",
        )
        .bind(code)
        .bind(earlier)
        .execute(&pool)
        .await
        .unwrap();

        let mut blocker = pool.begin().await.unwrap();
        crate::promo::lock_code(&mut blocker, code).await.unwrap();

        let mut attempts = Vec::new();
        for _ in 0..2 {
            let villa = raw_listing(&pool, host).await;
            let pool = pool.clone();
            attempts.push(tokio::spawn(async move {
                hold(&pool, guest, villa, "PERGUEST").await
            }));
        }
        until_lock_waiters(&pool, 2).await;
        blocker.commit().await.unwrap();

        for attempt in attempts {
            if let Err(e) = attempt.await.expect("task ran") {
                assert!(
                    matches!(e, DbError::PromoRefused(PromoRefusal::GuestLimitReached)),
                    "unexpected error {e:?}"
                );
            }
        }
        let live: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM promo_code_redemption r JOIN booking b ON b.id = r.booking_id
             WHERE r.promo_code_id = $1
               AND (b.status = 'confirmed' OR (b.status = 'pending' AND b.expires_at > NOW()))",
        )
        .bind(code)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(live, 2, "live uses stay at the limit");
    }

    #[tokio::test]
    async fn a_guest_replacing_their_own_hold_can_reuse_a_one_use_code() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let guest = raw_user(&pool).await;
        let code = insert_code(&pool, host, "REPLACE").await;
        limit(&pool, code, Some(1), Some(1)).await;

        let first = hold(&pool, guest, raw_listing(&pool, host).await, "REPLACE")
            .await
            .unwrap();
        let second = hold(&pool, guest, raw_listing(&pool, host).await, "REPLACE")
            .await
            .unwrap();

        assert_eq!(status_of(&pool, first.id).await, "cancelled");
        assert_eq!(status_of(&pool, second.id).await, "pending");
    }

    #[tokio::test]
    async fn a_refused_hold_rolls_back_and_keeps_the_guests_earlier_hold() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let guest = raw_user(&pool).await;
        let code = insert_code(&pool, host, "GONE").await;
        let earlier = crate::booking::create_booking(
            &pool,
            &new_booking(guest, raw_listing(&pool, host).await, None),
            HOLD_MINUTES,
        )
        .await
        .unwrap();
        let promo = promo_of(&pool, "GONE").await;
        sqlx::query(
            "UPDATE promo_code SET deactivated_at = NOW(), deactivated_by = $2 WHERE id = $1",
        )
        .bind(code)
        .bind(host)
        .execute(&pool)
        .await
        .unwrap();

        let villa = raw_listing(&pool, host).await;
        let res = crate::booking::create_booking(
            &pool,
            &new_booking(guest, villa, Some(promo)),
            HOLD_MINUTES,
        )
        .await;

        assert_eq!(refusal(res), PromoRefusal::Deactivated);
        assert_eq!(status_of(&pool, earlier.id).await, "pending");
        let created: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM booking WHERE listing_id = $1")
            .bind(villa)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(created, 0);
    }

    #[tokio::test]
    async fn an_expired_hold_frees_its_use_for_another_guest() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let code = insert_code(&pool, host, "EXPIRY").await;
        limit(&pool, code, Some(1), None).await;
        let first = hold(
            &pool,
            raw_user(&pool).await,
            raw_listing(&pool, host).await,
            "EXPIRY",
        )
        .await
        .unwrap();

        let refused = hold(
            &pool,
            raw_user(&pool).await,
            raw_listing(&pool, host).await,
            "EXPIRY",
        )
        .await;
        assert_eq!(refusal(refused), PromoRefusal::UsageLimitReached);

        sqlx::query("UPDATE booking SET expires_at = NOW() - INTERVAL '1 minute' WHERE id = $1")
            .bind(first.id)
            .execute(&pool)
            .await
            .unwrap();
        // Freed before any sweep runs.
        let second = hold(
            &pool,
            raw_user(&pool).await,
            raw_listing(&pool, host).await,
            "EXPIRY",
        )
        .await
        .unwrap();

        let swept = crate::booking::expire_stale_holds(&pool).await.unwrap();
        assert_eq!(
            swept.iter().map(|b| b.id).collect::<Vec<_>>(),
            vec![first.id]
        );
        assert_eq!(status_of(&pool, second.id).await, "pending");
    }

    #[tokio::test]
    async fn cancelling_a_confirmed_booking_frees_its_use() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let code = insert_code(&pool, host, "CANCELLED").await;
        limit(&pool, code, Some(1), None).await;
        let booking = hold(
            &pool,
            raw_user(&pool).await,
            raw_listing(&pool, host).await,
            "CANCELLED",
        )
        .await
        .unwrap();
        crate::booking::update_booking(
            &pool,
            booking.id,
            &set_status(BookingStatus::Confirmed),
            &supplier(),
        )
        .await
        .unwrap();

        let refused = hold(
            &pool,
            raw_user(&pool).await,
            raw_listing(&pool, host).await,
            "CANCELLED",
        )
        .await;
        assert_eq!(refusal(refused), PromoRefusal::UsageLimitReached);

        crate::booking::update_booking(
            &pool,
            booking.id,
            &set_status(BookingStatus::Cancelled),
            &supplier(),
        )
        .await
        .unwrap();
        hold(
            &pool,
            raw_user(&pool).await,
            raw_listing(&pool, host).await,
            "CANCELLED",
        )
        .await
        .expect("the use is free again");
    }

    #[tokio::test]
    async fn a_live_hold_can_be_confirmed_after_its_code_is_deactivated_or_expires() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        for (code, change) in [
            (
                "DEACTIVATE",
                "deactivated_at = NOW(), deactivated_by = created_by",
            ),
            (
                "LAPSES",
                "valid_from = NOW() - INTERVAL '2 days', valid_to = NOW() - INTERVAL '1 day'",
            ),
        ] {
            let id = insert_code(&pool, host, code).await;
            let booking = hold(
                &pool,
                raw_user(&pool).await,
                raw_listing(&pool, host).await,
                code,
            )
            .await
            .unwrap();
            sqlx::query(&format!("UPDATE promo_code SET {change} WHERE id = $1"))
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();

            let confirmed = crate::booking::update_booking(
                &pool,
                booking.id,
                &set_status(BookingStatus::Confirmed),
                &supplier(),
            )
            .await
            .unwrap();
            assert_eq!(confirmed.status, BookingStatus::Confirmed, "{code}");
            assert_eq!(confirmed.promo_stay_discount, dec!(40.00), "{code}");
        }
    }

    #[tokio::test]
    async fn a_hold_refuses_an_expired_code() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let id = insert_code(&pool, host, "TOOLATE").await;
        sqlx::query("UPDATE promo_code SET valid_from = NOW() - INTERVAL '2 days', valid_to = NOW() - INTERVAL '1 day' WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        let res = hold(
            &pool,
            raw_user(&pool).await,
            raw_listing(&pool, host).await,
            "TOOLATE",
        )
        .await;
        assert_eq!(refusal(res), PromoRefusal::Expired);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_hold_with_no_code_takes_no_promo_lock() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let code = insert_code(&pool, host, "HELD").await;
        let mut blocker = pool.begin().await.unwrap();
        crate::promo::lock_code(&mut blocker, code).await.unwrap();

        let plain = new_booking(raw_user(&pool).await, raw_listing(&pool, host).await, None);
        let done = tokio::time::timeout(
            Duration::from_secs(10),
            crate::booking::create_booking(&pool, &plain, HOLD_MINUTES),
        )
        .await
        .expect("a no-code hold doesn't wait on the promo row");
        assert!(done.is_ok());
    }

    /// A confirmation waits on the code while its hold expires, as happens when
    /// another guest's hold has just counted the use as free. It must refuse,
    /// or the code would end up with one more use than its limit.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_hold_that_lost_its_use_at_expiry_cannot_be_confirmed() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let host = raw_user(&pool).await;
        let code = insert_code(&pool, host, "RACE").await;
        let booking = hold(
            &pool,
            raw_user(&pool).await,
            raw_listing(&pool, host).await,
            "RACE",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE booking SET expires_at = clock_timestamp() + INTERVAL '2 seconds' WHERE id = $1")
            .bind(booking.id)
            .execute(&pool)
            .await
            .unwrap();

        let mut blocker = pool.begin().await.unwrap();
        crate::promo::lock_code(&mut blocker, code).await.unwrap();

        let confirm = {
            let pool = pool.clone();
            tokio::spawn(async move {
                crate::booking::update_booking(
                    &pool,
                    booking.id,
                    &set_status(BookingStatus::Confirmed),
                    &supplier(),
                )
                .await
            })
        };
        until_lock_waiters(&pool, 1).await;
        loop {
            let expired: bool = sqlx::query_scalar(
                "SELECT expires_at <= clock_timestamp() FROM booking WHERE id = $1",
            )
            .bind(booking.id)
            .fetch_one(&pool)
            .await
            .unwrap();
            if expired {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        blocker.commit().await.unwrap();

        let res = confirm.await.expect("task ran");
        assert!(matches!(res, Err(DbError::HoldExpired)), "got {res:?}");
        assert_eq!(status_of(&pool, booking.id).await, "pending");
    }
}

mod update {
    use super::*;
    use crate::models::{SupplierInfo, UpdatedBooking};

    fn supplier() -> SupplierInfo {
        SupplierInfo {
            supplier_name: "Our Places Jamaica Limited".to_string(),
            supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
            supplier_trn: "000-111-222".to_string(),
        }
    }

    /// A live hold redeeming a one-use code, with 40.00 off the stay.
    async fn redeemed_hold(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
        let host = raw_user(pool).await;
        let guest = raw_user(pool).await;
        let villa = raw_listing(pool, host).await;
        let code = insert_code(pool, host, "CHANGED").await;
        sqlx::query("UPDATE promo_code SET total_usage_limit = 1 WHERE id = $1")
            .bind(code)
            .execute(pool)
            .await
            .unwrap();
        let booking = raw_booking(pool, guest, villa, "pending").await;
        sqlx::query(
            r#"UPDATE booking SET expires_at = NOW() + INTERVAL '1 hour', promo_code_id = $2,
                   promo_terms = '{"code":"CHANGED","discount_type":"percentage","discount_value":"10","scope":"stay","currency":null,"exchange_rate":"1","min_nights":null,"min_booking_value":null,"stay_from":null,"stay_to":null}',
                   promo_stay_discount = 40
               WHERE id = $1"#,
        )
        .bind(booking)
        .bind(code)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO promo_code_redemption (promo_code_id, booking_id) VALUES ($1, $2)",
        )
        .bind(code)
        .bind(booking)
        .execute(pool)
        .await
        .unwrap();
        (booking, code, guest)
    }

    async fn total_used(pool: &PgPool, code: Uuid, guest: Uuid) -> i64 {
        let mut conn = pool.acquire().await.unwrap();
        let (_, usage) = crate::promo::lock_and_count(&mut conn, code, guest)
            .await
            .unwrap();
        usage.total_used
    }

    #[tokio::test]
    async fn new_amounts_replace_the_discount_and_keep_the_code() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let (booking, code, guest) = redeemed_hold(&pool).await;

        let updated = crate::booking::update_booking(
            &pool,
            booking,
            &UpdatedBooking {
                promo_stay_discount: Some(dec!(80)),
                promo_cleaning_discount: Some(dec!(5)),
                ..Default::default()
            },
            &supplier(),
        )
        .await
        .unwrap();

        assert_eq!(updated.promo_code_id, Some(code));
        assert!(updated.promo_terms.is_some());
        assert_eq!(updated.promo_stay_discount, dec!(80));
        assert_eq!(updated.promo_cleaning_discount, dec!(5));
        assert_eq!(total_used(&pool, code, guest).await, 1);
    }

    #[tokio::test]
    async fn an_unrelated_update_leaves_the_promo_alone() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let (booking, code, guest) = redeemed_hold(&pool).await;

        let updated = crate::booking::update_booking(
            &pool,
            booking,
            &UpdatedBooking {
                door_access_code: Some("1234".to_string()),
                ..Default::default()
            },
            &supplier(),
        )
        .await
        .unwrap();

        assert_eq!(updated.promo_code_id, Some(code));
        assert_eq!(updated.promo_stay_discount, dec!(40));
        assert_eq!(total_used(&pool, code, guest).await, 1);
    }

    #[tokio::test]
    async fn losing_the_discount_clears_it_frees_the_use_and_keeps_the_old_snapshot() {
        let test_db = migrated_db();
        let pool = test_db.get_pool().await;
        let (booking, code, guest) = redeemed_hold(&pool).await;

        // A status snapshot taken before the change, which must survive it.
        sqlx::query(
            "INSERT INTO booking_history (booking_id, confirmation_code, guest_id, listing_id, status,
                 date_from, date_to, currency, daily_rate, number_of_persons, total_days, sub_total_price,
                 fee_breakdown, total_price, cancellation_policy, metadata, promo_code_id, promo_terms,
                 promo_stay_discount, change_reason)
             SELECT id, confirmation_code, guest_id, listing_id, status, date_from, date_to, currency,
                 daily_rate, number_of_persons, total_days, sub_total_price, fee_breakdown, total_price,
                 cancellation_policy, metadata, promo_code_id, promo_terms, promo_stay_discount,
                 'Booking created'
             FROM booking WHERE id = $1",
        )
        .bind(booking)
        .execute(&pool)
        .await
        .unwrap();

        let updated = crate::booking::update_booking(
            &pool,
            booking,
            &UpdatedBooking {
                clear_promo: true,
                promo_stay_discount: Some(Decimal::ZERO),
                promo_cleaning_discount: Some(Decimal::ZERO),
                ..Default::default()
            },
            &supplier(),
        )
        .await
        .unwrap();

        assert_eq!(updated.promo_code_id, None);
        assert!(updated.promo_terms.is_none());
        assert_eq!(updated.promo_stay_discount, Decimal::ZERO);
        assert_eq!(total_used(&pool, code, guest).await, 0);

        let released: bool = sqlx::query_scalar(
            "SELECT released_at IS NOT NULL FROM promo_code_redemption WHERE booking_id = $1",
        )
        .bind(booking)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(released, "the redemption is kept, marked released");

        let old: Option<String> = sqlx::query_scalar(
            "SELECT promo_terms->>'code' FROM booking_history
             WHERE booking_id = $1 AND change_reason = 'Booking created'",
        )
        .bind(booking)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(old.as_deref(), Some("CHANGED"));
    }
}
