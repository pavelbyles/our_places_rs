use crate::error::Result;
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PreArrivalCandidate {
    pub booking_id: Uuid,
    pub confirmation_code: String,
    pub date_from: NaiveDate,
    pub date_to: NaiveDate,
    pub door_access_code: Option<String>,
    pub guest_id: Uuid,
    pub guest_email: String,
    pub guest_first_name: String,
    pub guest_last_name: String,
    pub guest_phone: Option<String>,
    pub host_id: Uuid,
    pub host_email: String,
    pub host_first_name: String,
    pub host_last_name: String,
    pub host_phone: Option<String>,
    pub listing_id: Uuid,
    pub listing_name: String,
    pub listing_city: Option<String>,
    pub listing_details: sqlx::types::Json<serde_json::Value>,
    pub number_of_persons: i32,
    pub estimated_arrival_time: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExpiringHoldCandidate {
    pub booking_id: Uuid,
    pub confirmation_code: String,
    pub guest_id: Uuid,
    pub guest_email: String,
    pub guest_first_name: String,
    pub listing_id: Uuid,
    pub listing_name: String,
    pub currency: String,
    pub total_price: Decimal,
    pub created_at: DateTime<Utc>,
}

/// Atomically claims and logs a notification for a booking and recipient.
/// Returns `Ok(true)` if claimed (first time), or `Ok(false)` if already claimed.
pub async fn claim_and_log_notification(
    pool: &PgPool,
    booking_id: Uuid,
    notification_type: &str,
    recipient_user_id: Uuid,
) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        INSERT INTO booking_notification_log (booking_id, notification_type, recipient_user_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (booking_id, notification_type, recipient_user_id) DO NOTHING
        RETURNING id
        "#,
        booking_id,
        notification_type,
        recipient_user_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(result.is_some())
}

/// Transactional variant of `claim_and_log_notification`.
pub async fn claim_and_log_notification_tx(
    tx: &mut PgConnection,
    booking_id: Uuid,
    notification_type: &str,
    recipient_user_id: Uuid,
) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        INSERT INTO booking_notification_log (booking_id, notification_type, recipient_user_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (booking_id, notification_type, recipient_user_id) DO NOTHING
        RETURNING id
        "#,
        booking_id,
        notification_type,
        recipient_user_id
    )
    .fetch_optional(tx)
    .await?;

    Ok(result.is_some())
}

/// Retrieves all confirmed bookings due for 48-hour pre-arrival notifications
/// where a pre-arrival guide has not yet been logged.
pub async fn get_48h_pre_arrival_candidates(pool: &PgPool) -> Result<Vec<PreArrivalCandidate>> {
    let candidates = sqlx::query_as!(
        PreArrivalCandidate,
        r#"
        SELECT 
            b.id AS booking_id,
            b.confirmation_code,
            b.date_from,
            b.date_to,
            b.door_access_code,
            g.id AS guest_id,
            g.email AS guest_email,
            g.first_name AS guest_first_name,
            g.last_name AS guest_last_name,
            g.phone_number AS guest_phone,
            h.id AS host_id,
            h.email AS host_email,
            h.first_name AS host_first_name,
            h.last_name AS host_last_name,
            h.phone_number AS host_phone,
            l.id AS listing_id,
            l.name AS listing_name,
            l.city AS listing_city,
            l.listing_details AS "listing_details: sqlx::types::Json<serde_json::Value>",
            b.number_of_persons,
            b.metadata->>'estimated_arrival_time' AS estimated_arrival_time
        FROM booking b
        JOIN "user" g ON b.guest_id = g.id
        JOIN listing l ON b.listing_id = l.id
        JOIN "user" h ON l.user_id = h.id
        WHERE b.status = 'confirmed'
          AND b.date_from = CURRENT_DATE + INTERVAL '2 days'
          AND NOT EXISTS (
              SELECT 1 FROM booking_notification_log 
              WHERE booking_id = b.id AND notification_type = 'pre_arrival_guide_guest'
          )
        ORDER BY b.created_at ASC
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(candidates)
}

/// Retrieves all active pending holds approaching expiration (between 75 and 105 minutes old)
/// that have not yet received a payment hold reminder.
pub async fn get_expiring_hold_candidates(pool: &PgPool) -> Result<Vec<ExpiringHoldCandidate>> {
    let candidates = sqlx::query_as!(
        ExpiringHoldCandidate,
        r#"
        SELECT 
            b.id AS booking_id,
            b.confirmation_code,
            g.id AS guest_id,
            g.email AS guest_email,
            g.first_name AS guest_first_name,
            l.id AS listing_id,
            l.name AS listing_name,
            b.currency,
            b.total_price,
            b.created_at
        FROM booking b
        JOIN "user" g ON b.guest_id = g.id
        JOIN listing l ON b.listing_id = l.id
        WHERE b.status = 'pending'
          AND b.created_at >= NOW() - INTERVAL '105 minutes'
          AND b.created_at <= NOW() - INTERVAL '75 minutes'
          AND NOT EXISTS (
              SELECT 1 FROM booking_notification_log 
              WHERE booking_id = b.id AND notification_type = 'payment_hold_reminder'
          )
        ORDER BY b.created_at ASC
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(candidates)
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
    async fn test_notification_log_idempotency() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;

        let host_id = Uuid::new_v4();
        let guest_id = Uuid::new_v4();

        sqlx::query!(
            r#"INSERT INTO "user" (id, email, password_hash, first_name, last_name, default_currency)
               VALUES ($1, 'host_notify@test.com', 'hash', 'Host', 'User', 'USD'),
                      ($2, 'guest_notify@test.com', 'hash', 'Guest', 'User', 'USD')"#,
            host_id, guest_id
        )
        .execute(&pool)
        .await
        .unwrap();

        let listing_id = Uuid::new_v4();
        sqlx::query!(
            r#"INSERT INTO listing (id, user_id, name, country, listing_structure_id, added_at, slug)
               VALUES ($1, $2, 'Villa Notify Test', 'Jamaica', 1, NOW(), 'villa-notify-test')"#,
            listing_id, host_id
        )
        .execute(&pool)
        .await
        .unwrap();

        let booking_id = Uuid::new_v4();
        sqlx::query!(
            r#"INSERT INTO booking (id, confirmation_code, guest_id, listing_id, status,
                                   date_from, date_to, currency, daily_rate, number_of_persons,
                                   total_days, sub_total_price, total_price, cancellation_policy)
               VALUES ($1, 'NOTIFY01', $2, $3, 'confirmed', CURRENT_DATE + INTERVAL '2 days',
                       CURRENT_DATE + INTERVAL '5 days', 'USD', 200.00, 2, 3, 600.00, 600.00, 'flexible')"#,
            booking_id, guest_id, listing_id
        )
        .execute(&pool)
        .await
        .unwrap();

        // 1. First claim succeeds
        let claimed_first =
            claim_and_log_notification(&pool, booking_id, "pre_arrival_guide_guest", guest_id)
                .await
                .unwrap();
        assert!(claimed_first);

        // 2. Duplicate claim returns false (idempotent)
        let claimed_second =
            claim_and_log_notification(&pool, booking_id, "pre_arrival_guide_guest", guest_id)
                .await
                .unwrap();
        assert!(!claimed_second);

        // 3. Different notification type for same booking succeeds
        let claimed_host_alert =
            claim_and_log_notification(&pool, booking_id, "host_upcoming_arrival", host_id)
                .await
                .unwrap();
        assert!(claimed_host_alert);
    }
}
