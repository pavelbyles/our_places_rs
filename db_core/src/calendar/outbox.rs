//! Calendar emails stuck in the outbox. Publishing happens after commit, so
//! a failed publish leaves a `pending` row; the sync job republishes them.

use crate::error::Result;
use chrono::Duration;
use sqlx::PgPool;
use uuid::Uuid;

/// `pending` `calendar_*` outbox rows older than `older_than` that still have
/// attempts left, oldest first, at most `limit`.
#[tracing::instrument(skip(pool))]
pub async fn stuck_calendar_emails(
    pool: &PgPool,
    older_than: Duration,
    limit: i64,
) -> Result<Vec<Uuid>> {
    Ok(sqlx::query_scalar!(
        r#"SELECT id FROM email_outbox
        WHERE status = 'pending' AND template_id LIKE 'calendar\_%'
          AND created_at < now() - ($1::int8 * INTERVAL '1 second')
          AND attempts < max_retries
        ORDER BY created_at
        LIMIT $2"#,
        older_than.num_seconds(),
        limit
    )
    .fetch_all(pool)
    .await?)
}
