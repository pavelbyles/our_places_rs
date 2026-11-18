//! Schema and constraint tests for the turnover dispatch & staff migration (#66).

use crate::models::UserRole;
use crate::promo_tests::{migrated_db, raw_booking, raw_listing, raw_user};
use uuid::Uuid;

#[tokio::test]
async fn staff_enum_and_turnover_tables_exist() -> Result<(), Box<dyn std::error::Error>> {
    let db = migrated_db();
    let pool = db.get_pool().await;

    let tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.tables
         WHERE table_schema = 'public'
           AND table_name IN (
             'account_action_token',
             'host_staff',
             'listing_staff',
             'turnover',
             'turnover_status_history',
             'turnover_comment'
           )",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(tables, 6);

    let uid = raw_user(&pool).await;
    sqlx::query(r#"UPDATE "user" SET roles = ARRAY['staff'::user_role] WHERE id = $1"#)
        .bind(uid)
        .execute(&pool)
        .await?;

    let roles: Vec<UserRole> = sqlx::query_scalar(r#"SELECT roles FROM "user" WHERE id = $1"#)
        .bind(uid)
        .fetch_one(&pool)
        .await?;
    assert_eq!(roles, vec![UserRole::Staff]);
    Ok(())
}

#[tokio::test]
async fn partial_unique_indexes_prevent_duplicate_stay_turnovers_but_allow_manual()
-> Result<(), Box<dyn std::error::Error>> {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let host = raw_user(&pool).await;
    let guest = raw_user(&pool).await;
    let villa = raw_listing(&pool, host).await;
    let booking = raw_booking(&pool, guest, villa, "confirmed").await;

    sqlx::query(
        "INSERT INTO turnover (listing_id, source_kind, booking_id, checkout_date, status)
         VALUES ($1, 'booking', $2, '2030-01-03', 'needs_cleaning')",
    )
    .bind(villa)
    .bind(booking)
    .execute(&pool)
    .await?;

    let dup_booking = sqlx::query(
        "INSERT INTO turnover (listing_id, source_kind, booking_id, checkout_date, status)
         VALUES ($1, 'booking', $2, '2030-01-03', 'needs_cleaning')",
    )
    .bind(villa)
    .bind(booking)
    .execute(&pool)
    .await;
    assert!(dup_booking.is_err());

    let feed_id: Uuid = sqlx::query_scalar(
        "INSERT INTO listing_calendar_feed (listing_id, name, url)
         VALUES ($1, 'Airbnb', 'https://airbnb.example/feed.ics') RETURNING id",
    )
    .bind(villa)
    .fetch_one(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO turnover (listing_id, source_kind, source_feed_id, source_external_uid, checkout_date, status)
         VALUES ($1, 'ical_feed', $2, 'uid-1', '2030-02-10', 'needs_cleaning')",
    )
    .bind(villa)
    .bind(feed_id)
    .execute(&pool)
    .await?;

    let dup_ical = sqlx::query(
        "INSERT INTO turnover (listing_id, source_kind, source_feed_id, source_external_uid, checkout_date, status)
         VALUES ($1, 'ical_feed', $2, 'uid-1', '2030-02-11', 'needs_cleaning')",
    )
    .bind(villa)
    .bind(feed_id)
    .execute(&pool)
    .await;
    assert!(dup_ical.is_err());

    for idx in 1..=2 {
        sqlx::query(
            "INSERT INTO turnover (listing_id, source_kind, checkout_date, status, title)
             VALUES ($1, 'manual', '2030-03-01', 'maintenance_required', $2)",
        )
        .bind(villa)
        .bind(format!("Manual check {idx}"))
        .execute(&pool)
        .await?;
    }
    Ok(())
}

#[tokio::test]
async fn deleting_block_and_user_preserves_turnover_comments_and_author_name()
-> Result<(), Box<dyn std::error::Error>> {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let host = raw_user(&pool).await;
    let staff = raw_user(&pool).await;
    let villa = raw_listing(&pool, host).await;

    let feed_id: Uuid = sqlx::query_scalar(
        "INSERT INTO listing_calendar_feed (listing_id, name, url)
         VALUES ($1, 'VRBO', 'https://vrbo.example/feed.ics') RETURNING id",
    )
    .bind(villa)
    .fetch_one(&pool)
    .await?;

    let block_id: Uuid = sqlx::query_scalar(
        "INSERT INTO listing_blocked_dates (listing_id, source, feed_id, external_uid, date_from, date_to)
         VALUES ($1, 'ical_feed', $2, 'uid-keep', '2030-04-01', '2030-04-05') RETURNING id",
    )
    .bind(villa)
    .bind(feed_id)
    .fetch_one(&pool)
    .await?;

    let turnover_id: Uuid = sqlx::query_scalar(
        "INSERT INTO turnover (listing_id, source_kind, blocked_date_id, source_feed_id, source_external_uid, checkout_date, status, created_by)
         VALUES ($1, 'ical_feed', $2, $3, 'uid-keep', '2030-04-05', 'needs_cleaning', $4) RETURNING id",
    )
    .bind(villa)
    .bind(block_id)
    .bind(feed_id)
    .bind(staff)
    .fetch_one(&pool)
    .await?;

    let comment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO turnover_comment (turnover_id, author_user_id, author_name, author_role, body)
         VALUES ($1, $2, 'Promo Test', 'staff', 'Replace porch light') RETURNING id",
    )
    .bind(turnover_id)
    .bind(staff)
    .fetch_one(&pool)
    .await?;

    sqlx::query("DELETE FROM listing_blocked_dates WHERE id = $1")
        .bind(block_id)
        .execute(&pool)
        .await?;

    let kept_block: Option<Uuid> =
        sqlx::query_scalar("SELECT blocked_date_id FROM turnover WHERE id = $1")
            .bind(turnover_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(kept_block, None);

    sqlx::query(r#"DELETE FROM "user" WHERE id = $1"#)
        .bind(staff)
        .execute(&pool)
        .await?;

    let (author_user_id, author_name, body): (Option<Uuid>, String, String) = sqlx::query_as(
        "SELECT author_user_id, author_name, body FROM turnover_comment WHERE id = $1",
    )
    .bind(comment_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(author_user_id, None);
    assert_eq!(author_name, "Promo Test");
    assert_eq!(body, "Replace porch light");

    Ok(())
}
