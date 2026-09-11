use crate::error::Result;
use crate::models::{BookingParties, DbBookingMessage, DbMessageSenderRole};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn insert_booking_message(
    pool: &PgPool,
    id: Uuid,
    booking_id: Uuid,
    sender_id: Uuid,
    sender_role: DbMessageSenderRole,
    sender_name: &str,
    message_text: &str,
) -> Result<DbBookingMessage> {
    let msg = sqlx::query_as!(
        DbBookingMessage,
        r#"
        INSERT INTO booking_message (id, booking_id, sender_id, sender_role, sender_name, message_text)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, booking_id, sender_id, sender_role AS "sender_role: DbMessageSenderRole", sender_name, message_text, read_at, created_at
        "#,
        id,
        booking_id,
        sender_id,
        sender_role as DbMessageSenderRole,
        sender_name,
        message_text
    )
    .fetch_one(pool)
    .await?;

    Ok(msg)
}

pub async fn list_booking_messages(
    pool: &PgPool,
    booking_id: Uuid,
) -> Result<Vec<DbBookingMessage>> {
    let msgs = sqlx::query_as!(
        DbBookingMessage,
        r#"
        SELECT id, booking_id, sender_id, sender_role AS "sender_role: DbMessageSenderRole", sender_name, message_text, read_at, created_at
        FROM booking_message
        WHERE booking_id = $1
        ORDER BY created_at ASC
        LIMIT 200
        "#,
        booking_id
    )
    .fetch_all(pool)
    .await?;

    Ok(msgs)
}

pub async fn mark_booking_messages_as_read(
    pool: &PgPool,
    booking_id: Uuid,
    reader_id: Uuid,
) -> Result<u64> {
    let result = sqlx::query!(
        r#"
        UPDATE booking_message
        SET read_at = NOW()
        WHERE booking_id = $1 AND sender_id != $2 AND read_at IS NULL
        "#,
        booking_id,
        reader_id
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

pub async fn get_booking_parties(
    pool: &PgPool,
    booking_id: Uuid,
) -> Result<Option<BookingParties>> {
    let parties = sqlx::query_as!(
        BookingParties,
        r#"
        SELECT 
            b.guest_id,
            l.user_id as host_id,
            b.status AS "status: crate::models::BookingStatus"
        FROM booking b
        JOIN listing l ON b.listing_id = l.id
        WHERE b.id = $1
        "#,
        booking_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(parties)
}
