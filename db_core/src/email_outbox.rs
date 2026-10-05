use crate::error::Result;
use crate::models::{DbEmailStatus, EmailOutbox};
use sqlx::{PgConnection, PgPool, Transaction};
use uuid::Uuid;

pub async fn insert_email_outbox(
    pool: &PgPool,
    recipient_email: &str,
    subject: &str,
    template_id: &str,
    payload: &serde_json::Value,
    max_retries: i32,
) -> Result<EmailOutbox> {
    let record = sqlx::query_as!(
        EmailOutbox,
        r#"
        INSERT INTO email_outbox (recipient_email, subject, template_id, payload, max_retries)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING 
            id, recipient_email, subject, template_id, payload,
            status AS "status: DbEmailStatus",
            attempts, max_retries, last_error, sent_at, created_at, updated_at
        "#,
        recipient_email,
        subject,
        template_id,
        payload,
        max_retries
    )
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub async fn insert_email_outbox_tx(
    tx: &mut Transaction<'_, sqlx::Postgres>,
    recipient_email: &str,
    subject: &str,
    template_id: &str,
    payload: &serde_json::Value,
    max_retries: i32,
) -> Result<EmailOutbox> {
    let record = sqlx::query_as!(
        EmailOutbox,
        r#"
        INSERT INTO email_outbox (recipient_email, subject, template_id, payload, max_retries)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING 
            id, recipient_email, subject, template_id, payload,
            status AS "status: DbEmailStatus",
            attempts, max_retries, last_error, sent_at, created_at, updated_at
        "#,
        recipient_email,
        subject,
        template_id,
        payload,
        max_retries
    )
    .fetch_one(&mut **tx)
    .await?;

    Ok(record)
}

pub async fn insert_email_outbox_conn(
    conn: &mut PgConnection,
    recipient_email: &str,
    subject: &str,
    template_id: &str,
    payload: &serde_json::Value,
    max_retries: i32,
) -> Result<EmailOutbox> {
    let record = sqlx::query_as!(
        EmailOutbox,
        r#"
        INSERT INTO email_outbox (recipient_email, subject, template_id, payload, max_retries)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING 
            id, recipient_email, subject, template_id, payload,
            status AS "status: DbEmailStatus",
            attempts, max_retries, last_error, sent_at, created_at, updated_at
        "#,
        recipient_email,
        subject,
        template_id,
        payload,
        max_retries
    )
    .fetch_one(&mut *conn)
    .await?;

    Ok(record)
}

pub async fn get_email_outbox_by_id(pool: &PgPool, id: Uuid) -> Result<Option<EmailOutbox>> {
    let record = sqlx::query_as!(
        EmailOutbox,
        r#"
        SELECT 
            id, recipient_email, subject, template_id, payload,
            status AS "status: DbEmailStatus",
            attempts, max_retries, last_error, sent_at, created_at, updated_at
        FROM email_outbox
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub async fn get_email_outbox_for_update(
    tx: &mut Transaction<'_, sqlx::Postgres>,
    id: Uuid,
) -> Result<Option<EmailOutbox>> {
    let record = sqlx::query_as!(
        EmailOutbox,
        r#"
        SELECT 
            id, recipient_email, subject, template_id, payload,
            status AS "status: DbEmailStatus",
            attempts, max_retries, last_error, sent_at, created_at, updated_at
        FROM email_outbox
        WHERE id = $1
        FOR UPDATE
        "#,
        id
    )
    .fetch_optional(&mut **tx)
    .await?;

    Ok(record)
}

pub async fn mark_email_processing(
    tx: &mut Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    attempts: i32,
) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE email_outbox
        SET status = 'processing'::email_status,
            attempts = $2,
            updated_at = NOW()
        WHERE id = $1
        "#,
        id,
        attempts
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub async fn mark_email_sent(pool: &PgPool, id: Uuid, attempts: i32) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE email_outbox
        SET status = 'sent'::email_status,
            attempts = $2,
            sent_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
        "#,
        id,
        attempts
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn mark_email_failed(
    pool: &PgPool,
    id: Uuid,
    attempts: i32,
    last_error: &str,
) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE email_outbox
        SET status = 'failed'::email_status,
            attempts = $2,
            last_error = $3,
            updated_at = NOW()
        WHERE id = $1
        "#,
        id,
        attempts,
        last_error
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn delete_sent_emails_older_than(pool: &PgPool, days: i32) -> Result<u64> {
    let result = sqlx::query!(
        r#"
        DELETE FROM email_outbox
        WHERE status = 'sent'::email_status
          AND created_at < NOW() - make_interval(days => $1)
        "#,
        days
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
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
    async fn test_email_outbox_lifecycle() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;

        // 1. Insert pending email
        let payload = serde_json::json!({ "code": "654321" });
        let email = insert_email_outbox(
            &pool,
            "test_user@example.com",
            "Verify your email",
            "user_verification_otp",
            &payload,
            3,
        )
        .await
        .expect("Failed to insert email outbox");

        assert_eq!(email.recipient_email, "test_user@example.com");
        assert_eq!(email.status, DbEmailStatus::Pending);
        assert_eq!(email.attempts, 0);
        assert_eq!(email.max_retries, 3);
        assert_eq!(email.last_error, None);
        assert_eq!(email.sent_at, None);

        // 2. Fetch for update in transaction & transition to processing
        let mut tx = pool.begin().await.expect("Failed to begin tx");
        let locked = get_email_outbox_for_update(&mut tx, email.id)
            .await
            .expect("Failed to lock email")
            .expect("Email record not found");
        assert_eq!(locked.id, email.id);

        mark_email_processing(&mut tx, email.id, 1)
            .await
            .expect("Failed to mark processing");
        tx.commit().await.expect("Failed to commit tx");

        let updated = get_email_outbox_by_id(&pool, email.id)
            .await
            .expect("Query failed")
            .expect("Record not found");
        assert_eq!(updated.status, DbEmailStatus::Processing);
        assert_eq!(updated.attempts, 1);

        // 3. Mark sent
        mark_email_sent(&pool, email.id, 1)
            .await
            .expect("Failed to mark sent");

        let sent = get_email_outbox_by_id(&pool, email.id)
            .await
            .expect("Query failed")
            .expect("Record not found");
        assert_eq!(sent.status, DbEmailStatus::Sent);
        assert_eq!(sent.attempts, 1);
        assert!(sent.sent_at.is_some());

        // 4. Test failure flow with a second record
        let email2 = insert_email_outbox(
            &pool,
            "fail_user@example.com",
            "Password Reset",
            "password_reset_otp",
            &serde_json::json!({ "token": "abc" }),
            3,
        )
        .await
        .expect("Failed to insert second email");

        mark_email_failed(&pool, email2.id, 1, "SMTP connection timeout")
            .await
            .expect("Failed to mark failed");

        let failed = get_email_outbox_by_id(&pool, email2.id)
            .await
            .expect("Query failed")
            .expect("Record not found");
        assert_eq!(failed.status, DbEmailStatus::Failed);
        assert_eq!(
            failed.last_error.as_deref(),
            Some("SMTP connection timeout")
        );
    }
}
