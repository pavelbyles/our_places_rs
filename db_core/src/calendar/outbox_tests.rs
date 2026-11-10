//! Stuck calendar email selection.

use super::outbox::stuck_calendar_emails;
use crate::promo_tests::migrated_db;
use chrono::Duration;
use sqlx::PgPool;
use uuid::Uuid;

async fn outbox(
    pool: &PgPool,
    template: &str,
    status: &str,
    age_minutes: i32,
    attempts: i32,
) -> Uuid {
    sqlx::query_scalar(&format!(
        "INSERT INTO email_outbox (recipient_email, subject, template_id, payload, max_retries, status,
             attempts, created_at)
         VALUES ('h@example.com', 's', $1, '{{}}'::jsonb, 3, '{status}'::email_status, $2,
             NOW() - ($3 * INTERVAL '1 minute'))
         RETURNING id"
    ))
    .bind(template)
    .bind(attempts)
    .bind(age_minutes)
    .fetch_one(pool)
    .await
    .expect("outbox row")
}

#[tokio::test]
async fn only_old_pending_calendar_emails_with_attempts_left_are_stuck() {
    let db = migrated_db();
    let pool = db.get_pool().await;
    let stuck = outbox(&pool, "calendar_conflict_host", "pending", 10, 0).await;
    let stuck_failing = outbox(&pool, "calendar_feed_failing_host", "pending", 30, 2).await;
    outbox(&pool, "booking_confirmation_guest", "pending", 10, 0).await;
    outbox(&pool, "calendar_conflict_host", "pending", 1, 0).await;
    outbox(&pool, "calendar_conflict_host", "sent", 10, 1).await;
    outbox(&pool, "calendar_conflict_host", "pending", 10, 3).await;
    outbox(&pool, "calendarXconflict", "pending", 10, 0).await;
    let found = stuck_calendar_emails(&pool, Duration::minutes(5), 100)
        .await
        .expect("query");
    assert_eq!(found, vec![stuck_failing, stuck]);
    assert_eq!(
        stuck_calendar_emails(&pool, Duration::minutes(5), 1)
            .await
            .expect("query")
            .len(),
        1
    );
}
