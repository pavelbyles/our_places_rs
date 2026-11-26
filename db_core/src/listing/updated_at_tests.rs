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

// ---- T7: last_modified_at on the read path ----

fn at(year: i32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, 1, 1, 0, 0, 0).unwrap()
}

/// A photo row at `status`, last changed at `changed`. `parent` makes it a
/// resized variant of another photo.
async fn photo(
    pool: &PgPool,
    villa: Uuid,
    status: &str,
    changed: DateTime<Utc>,
    parent: Option<Uuid>,
    primary: bool,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO listing_image
           (listing_id, client_file_id, status, upload_url, parent_id, is_primary, updated_at,
            resolution)
         VALUES ($1, $2, $3::image_status, 'https://example.com/p.webp', $4, $5, $6,
                 CASE WHEN $4::uuid IS NULL THEN 'Raw' ELSE 'Mobile720w' END::image_resolution)
         RETURNING id",
    )
    .bind(villa)
    .bind(Uuid::now_v7().to_string())
    .bind(status)
    .bind(parent)
    .bind(primary)
    .bind(changed)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn last_modified(pool: &PgPool, villa: Uuid) -> DateTime<Utc> {
    super::get_listing_by_id(pool, villa)
        .await
        .unwrap()
        .listing
        .last_modified_at
}

#[tokio::test]
async fn processed_original_photos_count_towards_last_modified() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let villa = listing(&pool, host(&pool).await).await;
    backdate(&pool, villa).await;
    assert_eq!(last_modified(&pool, villa).await, baseline());

    photo(&pool, villa, "Processed", at(2010), None, true).await;
    assert_eq!(last_modified(&pool, villa).await, at(2010));
}

#[tokio::test]
async fn unfinished_uploads_and_variants_do_not_count() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let villa = listing(&pool, host(&pool).await).await;
    backdate(&pool, villa).await;
    let original = photo(&pool, villa, "Processed", at(2010), None, true).await;
    for status in ["PendingUpload", "Uploaded", "Processing", "Failed"] {
        photo(&pool, villa, status, at(2020), None, false).await;
    }
    photo(&pool, villa, "Processed", at(2020), Some(original), false).await;
    assert_eq!(last_modified(&pool, villa).await, at(2010));
}

#[tokio::test]
async fn listing_search_returns_last_modified_too() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let villa = listing(&pool, host(&pool).await).await;
    backdate(&pool, villa).await;
    photo(&pool, villa, "Processed", at(2010), None, true).await;
    let rows = super::get_listings(&pool, 1, 100, None).await.unwrap();
    let row = rows.iter().find(|l| l.id == villa).unwrap();
    assert_eq!(row.updated_at, baseline());
    assert_eq!(row.last_modified_at, at(2010));
}

#[tokio::test]
async fn renaming_a_photo_moves_last_modified() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let villa = listing(&pool, host(&pool).await).await;
    backdate(&pool, villa).await;
    let p = photo(&pool, villa, "Processed", at(2010), None, true).await;
    super::rename_listing_image(&pool, villa, p, Some("Pool deck"))
        .await
        .unwrap();
    assert!(last_modified(&pool, villa).await > at(2010));
}

#[tokio::test]
async fn deleting_a_photo_moves_updated_at() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let villa = listing(&pool, host(&pool).await).await;
    let p = photo(&pool, villa, "Processed", at(2010), None, true).await;
    backdate(&pool, villa).await;
    let mut conn = pool.acquire().await.unwrap();
    super::delete_listing_image(&mut conn, villa, p)
        .await
        .unwrap();
    assert!(updated_at(&pool, villa).await > baseline());
}

#[tokio::test]
async fn hard_deleting_a_listing_with_photos_succeeds() {
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let villa = listing(&pool, host(&pool).await).await;
    photo(&pool, villa, "Processed", at(2010), None, true).await;
    super::delete_listing(&pool, villa, true).await.unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM listing WHERE id = $1")
        .bind(villa)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

// ---- I8: photo delete vs concurrent photo writers, no deadlock ----

/// Waits until some other backend in this database is blocked on a lock.
async fn wait_for_lock_waiter(pool: &PgPool) {
    for _ in 0..500 {
        let waiting: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity
              WHERE datname = current_database() AND wait_event_type = 'Lock'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        if waiting > 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("the concurrent photo write never waited on a lock");
}

/// Photo A (primary) is deleted in an open transaction, which bumps the
/// listing and promotes photo B; `write` then touches B from another
/// connection and must simply wait, not deadlock.
async fn delete_races_with<F, Fut>(write: F)
where
    F: FnOnce(PgPool, Uuid, Uuid) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = crate::error::Result<()>> + Send,
{
    let tdb = test_db();
    let pool = tdb.get_pool().await;
    let villa = listing(&pool, host(&pool).await).await;
    let a = photo(&pool, villa, "Processed", at(2010), None, true).await;
    let b = photo(&pool, villa, "Processed", at(2010), None, false).await;

    let mut tx = pool.begin().await.unwrap();
    super::delete_listing_image(&mut tx, villa, a)
        .await
        .unwrap();

    let writer_pool = pool.clone();
    let writer = tokio::spawn(async move { write(writer_pool, villa, b).await });
    wait_for_lock_waiter(&pool).await;
    tx.commit().await.unwrap();

    let result = writer.await.unwrap();
    assert!(result.is_ok(), "concurrent photo write failed: {result:?}");
}

#[tokio::test]
async fn photo_delete_and_concurrent_rename_do_not_deadlock() {
    delete_races_with(|pool, villa, b| async move {
        let mut conn = pool.acquire().await?;
        sqlx::query("SET lock_timeout = '10s'")
            .execute(&mut *conn)
            .await?;
        super::rename_listing_image(&mut *conn, villa, b, Some("Terrace")).await
    })
    .await;
}

#[tokio::test]
async fn photo_delete_and_concurrent_processing_do_not_deadlock() {
    delete_races_with(|pool, _villa, b| async move {
        let mut conn = pool.acquire().await?;
        sqlx::query("SET lock_timeout = '10s'")
            .execute(&mut *conn)
            .await?;
        super::mark_listing_image_processed(&mut *conn, b, "https://example.com/b.webp".into())
            .await
    })
    .await;
}
