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

/// Unread messages sent by guests on `host_id`'s listings, or on every
/// listing when `host_id` is `None` (admins). Host and admin messages never
/// count. Uses the partial index over unread messages.
pub async fn unread_guest_messages(pool: &PgPool, host_id: Option<Uuid>) -> Result<i64> {
    let count = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) AS "count!"
        FROM booking_message m
        JOIN conversation c ON c.id = m.conversation_id
        JOIN listing l ON l.id = c.listing_id
        WHERE m.read_at IS NULL
          AND m.sender_id = c.guest_id
          AND ($1::uuid IS NULL OR l.user_id = $1)
        "#,
        host_id
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
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

/// What a thread with unread guest messages is about, so the inbox can link
/// to the right page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadSubject {
    /// A booking, by id and confirmation code.
    Booking {
        /// The booking.
        id: Uuid,
        /// Its confirmation code.
        confirmation_code: String,
    },
    /// A request to book.
    Request(Uuid),
    /// An enquiry.
    Enquiry(Uuid),
}

/// A conversation with unread guest messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadThread {
    /// The conversation.
    pub conversation_id: Uuid,
    /// The listing.
    pub listing_id: Uuid,
    /// The listing's name.
    pub listing_name: String,
    /// The guest's first name.
    pub guest_name: String,
    /// Unread guest messages in the thread.
    pub unread: i64,
    /// When the newest unread message arrived.
    pub latest_at: chrono::DateTime<chrono::Utc>,
    /// The newest unread message (user content; never logged).
    pub latest_text: String,
    /// What the newest unread message is about, else the thread's latest
    /// booking, request or enquiry; `None` if it has none.
    pub subject: Option<ThreadSubject>,
    /// The subject's stay dates.
    pub dates: Option<(chrono::NaiveDate, chrono::NaiveDate)>,
}

/// Threads with unread guest messages on `host_id`'s listings (every listing
/// when `None`), newest first, at most `limit`. Counts the same messages as
/// [`unread_guest_messages`], so the list matches the nav badge.
#[tracing::instrument(skip(pool))]
pub async fn unread_threads(
    pool: &PgPool,
    host_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<UnreadThread>> {
    let rows = sqlx::query!(
        r#"
        WITH unread AS (
            SELECT m.conversation_id, COUNT(*) AS unread, MAX(m.created_at) AS latest_at
            FROM booking_message m
            JOIN conversation c ON c.id = m.conversation_id
            JOIN listing l ON l.id = c.listing_id
            WHERE m.read_at IS NULL
              AND m.sender_id = c.guest_id
              AND ($1::uuid IS NULL OR l.user_id = $1)
            GROUP BY m.conversation_id
            ORDER BY latest_at DESC
            LIMIT $2
        )
        SELECT u.conversation_id AS "conversation_id!", u.unread AS "unread!",
            u.latest_at AS "latest_at!", c.listing_id, l.name AS listing_name,
            g.first_name AS guest_name, lm.message_text AS "latest_text!",
            s.kind AS "subject_kind?", s.id AS "subject_id?", s.code AS "subject_code?",
            s.date_from AS "date_from?", s.date_to AS "date_to?"
        FROM unread u
        JOIN conversation c ON c.id = u.conversation_id
        JOIN listing l ON l.id = c.listing_id
        JOIN "user" g ON g.id = c.guest_id
        JOIN LATERAL (
            SELECT m.message_text, m.booking_id, m.request_id, m.enquiry_id
            FROM booking_message m
            WHERE m.conversation_id = c.id AND m.read_at IS NULL AND m.sender_id = c.guest_id
            ORDER BY m.created_at DESC
            LIMIT 1
        ) lm ON TRUE
        LEFT JOIN LATERAL (
            SELECT kind, id, code, date_from, date_to FROM (
                SELECT 1 AS prio, 'booking'::text AS kind, b.id, b.confirmation_code::text AS code,
                    b.date_from, b.date_to, b.created_at
                FROM booking b WHERE b.id = lm.booking_id
                UNION ALL
                SELECT 2, 'request', r.id, NULL, r.date_from, r.date_to, r.created_at
                FROM booking_request r WHERE r.id = lm.request_id
                UNION ALL
                SELECT 3, 'enquiry', e.id, NULL, e.date_from, e.date_to, e.created_at
                FROM booking_enquiry e WHERE e.id = lm.enquiry_id
                UNION ALL
                SELECT 4, 'booking', b.id, b.confirmation_code::text, b.date_from, b.date_to,
                    b.created_at
                FROM booking b WHERE b.listing_id = c.listing_id AND b.guest_id = c.guest_id
                UNION ALL
                SELECT 5, 'request', r.id, NULL, r.date_from, r.date_to, r.created_at
                FROM booking_request r WHERE r.conversation_id = c.id
                UNION ALL
                SELECT 6, 'enquiry', e.id, NULL, e.date_from, e.date_to, e.created_at
                FROM booking_enquiry e WHERE e.conversation_id = c.id
            ) candidates
            ORDER BY prio, created_at DESC
            LIMIT 1
        ) s ON TRUE
        ORDER BY u.latest_at DESC
        "#,
        host_id,
        limit
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let subject = match (r.subject_kind.as_deref(), r.subject_id) {
                (Some("booking"), Some(id)) => {
                    r.subject_code
                        .map(|confirmation_code| ThreadSubject::Booking {
                            id,
                            confirmation_code,
                        })
                }
                (Some("request"), Some(id)) => Some(ThreadSubject::Request(id)),
                (Some("enquiry"), Some(id)) => Some(ThreadSubject::Enquiry(id)),
                _ => None,
            };
            UnreadThread {
                conversation_id: r.conversation_id,
                listing_id: r.listing_id,
                listing_name: r.listing_name,
                guest_name: r.guest_name,
                unread: r.unread,
                latest_at: r.latest_at,
                latest_text: r.latest_text,
                dates: r.date_from.zip(r.date_to),
                subject,
            }
        })
        .collect())
}
