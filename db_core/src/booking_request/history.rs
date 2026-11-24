//! The append-only `booking_request_history` log.

use crate::error::Result;
use crate::models::DbMessageSenderRole;
use sqlx::PgConnection;
use uuid::Uuid;

/// What a history row is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistorySubject {
    /// An enquiry.
    Enquiry(Uuid),
    /// A request to book.
    Request(Uuid),
}

/// Who made a change. `None` in its place means the system (a sweep, or a
/// supersession caused by another action).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    /// The user.
    pub id: Uuid,
    /// The role they acted in.
    pub role: DbMessageSenderRole,
}

/// Appends one history row. `from` is `None` when the subject is created.
pub(crate) async fn record_history(
    conn: &mut PgConnection,
    subject: HistorySubject,
    from: Option<&str>,
    to: &str,
    quote_id: Option<Uuid>,
    actor: Option<Actor>,
    reason: &str,
) -> Result<()> {
    let (enquiry_id, request_id) = match subject {
        HistorySubject::Enquiry(id) => (Some(id), None),
        HistorySubject::Request(id) => (None, Some(id)),
    };
    sqlx::query!(
        r#"
        INSERT INTO booking_request_history
            (enquiry_id, request_id, from_status, to_status, quote_id, actor_id, actor_role, reason)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
        enquiry_id,
        request_id,
        from,
        to,
        quote_id,
        actor.map(|a| a.id),
        actor.map(|a| a.role) as Option<DbMessageSenderRole>,
        reason
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}
