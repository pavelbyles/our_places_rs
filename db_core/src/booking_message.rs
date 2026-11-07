use crate::error::Result;
use crate::models::{BookingParties, DbBookingMessage, DbMessageSenderRole};
use sqlx::PgPool;
use uuid::Uuid;

/// Posts a message about a booking into the guest × listing conversation of
/// that booking, creating the conversation on first use.
pub async fn insert_booking_message(
    pool: &PgPool,
    booking_id: Uuid,
    sender_id: Uuid,
    sender_role: DbMessageSenderRole,
    sender_name: &str,
    message_text: &str,
) -> Result<DbBookingMessage> {
    let mut tx = pool.begin().await?;
    let pair = sqlx::query!(
        "SELECT listing_id, guest_id FROM booking WHERE id = $1",
        booking_id
    )
    .fetch_one(&mut *tx)
    .await?;
    let conversation_id =
        crate::booking_request::upsert_conversation(&mut tx, pair.listing_id, pair.guest_id)
            .await?;
    let msg = crate::conversation::insert_message_tx(
        &mut tx,
        &crate::conversation::NewMessage {
            conversation_id,
            sender_id,
            sender_role,
            sender_name,
            text: message_text,
            booking_id: Some(booking_id),
            enquiry_id: None,
            request_id: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(msg)
}

/// The conversation a booking belongs to, if it has one yet.
async fn booking_conversation(pool: &PgPool, booking_id: Uuid) -> Result<Option<Uuid>> {
    let id = sqlx::query_scalar!(
        r#"
        SELECT c.id FROM conversation c
        JOIN booking b ON b.listing_id = c.listing_id AND b.guest_id = c.guest_id
        WHERE b.id = $1
        "#,
        booking_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(id)
}

/// The whole guest × listing conversation of a booking, oldest first, so a
/// thread started on an enquiry shows on the booking it led to.
pub async fn list_booking_messages(
    pool: &PgPool,
    booking_id: Uuid,
) -> Result<Vec<DbBookingMessage>> {
    match booking_conversation(pool, booking_id).await? {
        Some(conversation_id) => crate::conversation::list_messages(pool, conversation_id).await,
        None => Ok(Vec::new()),
    }
}

/// Marks the other party's messages in the booking's conversation as read,
/// returning how many changed.
pub async fn mark_booking_messages_as_read(
    pool: &PgPool,
    booking_id: Uuid,
    reader_id: Uuid,
) -> Result<u64> {
    match booking_conversation(pool, booking_id).await? {
        Some(conversation_id) => {
            crate::conversation::mark_read(pool, conversation_id, reader_id).await
        }
        None => Ok(0),
    }
}

/// Who is on a booking, for checking access.
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
