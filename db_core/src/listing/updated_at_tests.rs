//! Spec 102 §1.7: `listing.updated_at` and the public `last_modified_at`.

use crate::booking::unavailable_ranges_tests::{host, listing};
use chrono::{DateTime, TimeZone, Utc};
use sqlx::PgPool;
use sqlx_db_tester::TestPg;
use std::path::Path;
use uuid::Uuid;

fn test_db() -> TestPg {
    dotenvy::dotenv().ok();
    let db_url = std::env::var(crate::DATABASE_URL_ENV)
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".into());
    TestPg::new(db_url, Path::new("migrations"))
}

fn baseline() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap()
}

/// Sets `updated_at` to 2000-01-01 without firing the trigger, so a later
/// change is visible (the trigger would overwrite any value with now()).
async fn backdate(pool: &PgPool, id: Uuid) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL session_replication_role = replica")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE listing SET updated_at = $2 WHERE id = $1")
        .bind(id)
        .bind(baseline())
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn updated_at(pool: &PgPool, id: Uuid) -> DateTime<Utc> {
    sqlx::query_scalar("SELECT updated_at FROM listing WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ---- I1: column shape, trigger and function exist ----

#[tokio::test]
async fn updated_at_is_a_required_timestamptz_defaulting_to_now() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let (data_type, nullable, default): (String, String, Option<String>) = sqlx::query_as(
        "SELECT data_type, is_nullable, column_default FROM information_schema.columns
          WHERE table_name = 'listing' AND column_name = 'updated_at'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(data_type, "timestamp with time zone");
    assert_eq!(nullable, "NO");
    assert_eq!(default.as_deref(), Some("now()"));
}

#[tokio::test]
async fn listing_update_trigger_and_last_modified_function_exist() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let triggers: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.triggers
          WHERE event_object_table = 'listing' AND trigger_name = 'update_listing_updated_at'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(triggers, 1);
    let functions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pg_proc WHERE proname = 'listing_last_modified'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(functions, 1);
    // No listing_image triggers: they deadlock with photo delete (spec §1.7.1).
    let image_triggers: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.triggers WHERE event_object_table = 'listing_image'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(image_triggers, 0);
}

#[tokio::test]
async fn new_listings_start_with_a_current_updated_at() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let villa = listing(&pool, host(&pool).await).await;
    assert!(updated_at(&pool, villa).await > baseline());
}

// ---- I2: any listing update moves it ----

#[tokio::test]
async fn updating_a_listing_moves_updated_at() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let villa = listing(&pool, host(&pool).await).await;
    backdate(&pool, villa).await;
    assert_eq!(updated_at(&pool, villa).await, baseline());

    sqlx::query("UPDATE listing SET description = 'Sea views' WHERE id = $1")
        .bind(villa)
        .execute(&pool)
        .await
        .unwrap();
    assert!(updated_at(&pool, villa).await > baseline());
}

#[tokio::test]
async fn updating_one_listing_leaves_others_alone() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let owner = host(&pool).await;
    let (a, b) = (listing(&pool, owner).await, listing(&pool, owner).await);
    backdate(&pool, b).await;
    sqlx::query("UPDATE listing SET description = 'x' WHERE id = $1")
        .bind(a)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(updated_at(&pool, b).await, baseline());
}

// ---- I5: price overrides are rates, not content ----

#[tokio::test]
async fn price_overrides_do_not_move_updated_at() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let villa = listing(&pool, host(&pool).await).await;
    backdate(&pool, villa).await;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO listing_price_overrides (listing_id, start_date, end_date, nightly_rate)
         VALUES ($1, '2030-01-01', '2030-01-10', 300) RETURNING id",
    )
    .bind(villa)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE listing_price_overrides SET nightly_rate = 350 WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM listing_price_overrides WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(updated_at(&pool, villa).await, baseline());
}
