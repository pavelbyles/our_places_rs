//! The guest × listing conversation: one thread for every enquiry, request,
//! quote and booking between a guest and a listing's host.

use crate::error::Result;
use crate::models::{DbBookingMessage, DbConversation, DbMessageSenderRole};
use sqlx::PgPool;
use uuid::Uuid;

/// A conversation and the listing's host, for access checks.
#[derive(Debug, Clone)]
pub struct ConversationParties {
    /// The conversation.
    pub conversation: DbConversation,
    /// The listing's host.
    pub host_id: Uuid,
}

/// A message to post.
#[derive(Debug, Clone)]
pub struct NewMessage<'a> {
    /// The conversation.
    pub conversation_id: Uuid,
    /// The sender.
    pub sender_id: Uuid,
    /// The role they write as.
    pub sender_role: DbMessageSenderRole,
    /// Their display name.
    pub sender_name: &'a str,
    /// The text (user content; never logged).
    pub text: &'a str,
    /// The booking it is about, if any.
    pub booking_id: Option<Uuid>,
    /// The enquiry it is about, if any.
    pub enquiry_id: Option<Uuid>,
    /// The request it is about, if any.
    pub request_id: Option<Uuid>,
}

/// The guest × listing conversation, created on first use.
pub async fn upsert(pool: &PgPool, listing_id: Uuid, guest_id: Uuid) -> Result<Uuid> {
    let mut conn = pool.acquire().await?;
    crate::booking_request::upsert_conversation(&mut conn, listing_id, guest_id).await
}

/// A conversation with the listing's host.
#[tracing::instrument(skip(pool))]
pub async fn get_with_parties(
    pool: &PgPool,
    conversation_id: Uuid,
) -> Result<Option<ConversationParties>> {
    let row = sqlx::query!(
        r#"
        SELECT c.id, c.listing_id, c.guest_id, c.created_at, c.last_message_at,
            l.user_id AS host_id
        FROM conversation c
        JOIN listing l ON l.id = c.listing_id
        WHERE c.id = $1
        "#,
        conversation_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| ConversationParties {
        conversation: DbConversation {
            id: r.id,
            listing_id: r.listing_id,
            guest_id: r.guest_id,
            created_at: r.created_at,
            last_message_at: r.last_message_at,
        },
        host_id: r.host_id,
    }))
}

/// The conversation's messages, oldest first (at most 500).
#[tracing::instrument(skip(pool))]
pub async fn list_messages(pool: &PgPool, conversation_id: Uuid) -> Result<Vec<DbBookingMessage>> {
    let messages = sqlx::query_as!(
        DbBookingMessage,
        r#"
        SELECT id, conversation_id, booking_id, enquiry_id, request_id, sender_id,
            sender_role AS "sender_role: DbMessageSenderRole", sender_name, message_text,
            read_at, created_at
        FROM (
            SELECT * FROM booking_message
            WHERE conversation_id = $1
            ORDER BY created_at DESC
            LIMIT 500
        ) recent
        ORDER BY created_at ASC
        "#,
        conversation_id
    )
    .fetch_all(pool)
    .await?;
    Ok(messages)
}

/// Posts a message. A host's or admin's first message also records the first
/// response time on the pair's open enquiry and live request.
#[tracing::instrument(skip(pool, message), fields(conversation_id = %message.conversation_id))]
pub async fn insert_message(pool: &PgPool, message: &NewMessage<'_>) -> Result<DbBookingMessage> {
    let mut tx = pool.begin().await?;
    let inserted = insert_message_tx(&mut tx, message).await?;
    tx.commit().await?;
    Ok(inserted)
}

/// [`insert_message`] inside the caller's transaction.
pub(crate) async fn insert_message_tx(
    conn: &mut sqlx::PgConnection,
    message: &NewMessage<'_>,
) -> Result<DbBookingMessage> {
    let inserted = sqlx::query_as!(
        DbBookingMessage,
        r#"
        INSERT INTO booking_message (id, conversation_id, booking_id, enquiry_id, request_id,
            sender_id, sender_role, sender_name, message_text)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id, conversation_id, booking_id, enquiry_id, request_id, sender_id,
            sender_role AS "sender_role: DbMessageSenderRole", sender_name, message_text,
            read_at, created_at
        "#,
        Uuid::now_v7(),
        message.conversation_id,
        message.booking_id,
        message.enquiry_id,
        message.request_id,
        message.sender_id,
        message.sender_role as DbMessageSenderRole,
        message.sender_name,
        message.text
    )
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query!(
        "UPDATE conversation SET last_message_at = $2 WHERE id = $1",
        message.conversation_id,
        inserted.created_at
    )
    .execute(&mut *conn)
    .await?;
    if message.sender_role != DbMessageSenderRole::Guest {
        record_first_response(conn, message.conversation_id, inserted.created_at).await?;
    }
    Ok(inserted)
}

/// Sets `first_response_at` on the pair's open enquiry and live request where
/// it isn't set yet.
pub(crate) async fn record_first_response(
    conn: &mut sqlx::PgConnection,
    conversation_id: Uuid,
    at: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    sqlx::query!(
        r#"UPDATE booking_enquiry SET first_response_at = $2
        WHERE conversation_id = $1 AND status = 'open' AND first_response_at IS NULL"#,
        conversation_id,
        at
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        r#"UPDATE booking_request SET first_response_at = $2
        WHERE conversation_id = $1 AND status IN ('open', 'accepted', 'quoted')
          AND first_response_at IS NULL"#,
        conversation_id,
        at
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Marks the other party's messages read, returning how many changed.
pub async fn mark_read(pool: &PgPool, conversation_id: Uuid, reader_id: Uuid) -> Result<u64> {
    let result = sqlx::query!(
        r#"
        UPDATE booking_message SET read_at = NOW()
        WHERE conversation_id = $1 AND sender_id <> $2 AND read_at IS NULL
        "#,
        conversation_id,
        reader_id
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
