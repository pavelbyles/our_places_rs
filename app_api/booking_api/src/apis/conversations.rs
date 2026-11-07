//! The guest × listing conversation (#105): read, post and mark read.
//!
//! Only the guest, the listing's host or an admin may use a conversation.
//! Message text is user content: it is never logged, only its length.

use super::booking_requests::check_user_text;
use super::{caller, enqueue_email, is_admin, not_found};
use actix_web::{HttpRequest, HttpResponse, web};
use api_core::auth::Claims;
use api_core::error::ApiError;
use api_core::response::{Payload, respond};
use common::booking_request::RequestRefusal;
use common::models::{
    BookingMessageResponse, BookingMessagesWrapper, CreateBookingMessageRequest,
    MarkMessagesReadResponse,
};
use db_core::conversation as db_conversation;
use db_core::error::DbError;
use db_core::models::DbMessageSenderRole;
use sqlx::PgPool;
use uuid::Uuid;
use validator::Validate;

/// Loads a conversation and the caller's role in it, or refuses with 403.
async fn authorize(
    pool: &PgPool,
    claims: &Claims,
    conversation_id: Uuid,
) -> Result<
    (
        db_conversation::ConversationParties,
        db_core::models::User,
        DbMessageSenderRole,
    ),
    ApiError,
> {
    let parties = db_conversation::get_with_parties(pool, conversation_id)
        .await
        .map_err(|e| {
            tracing::error!(%conversation_id, error = ?e, "failed to load conversation");
            ApiError::Database(e)
        })?
        .ok_or_else(not_found)?;
    let user = caller(pool, claims).await?;
    let role = if claims.sub == parties.conversation.guest_id {
        DbMessageSenderRole::Guest
    } else if claims.sub == parties.host_id {
        DbMessageSenderRole::Host
    } else if is_admin(&user) {
        DbMessageSenderRole::Admin
    } else {
        tracing::warn!(%conversation_id, user_id = %claims.sub, "conversation access denied");
        return Err(ApiError::Database(DbError::RequestRefused(
            RequestRefusal::NotParty,
        )));
    };
    Ok((parties, user, role))
}

#[tracing::instrument(skip_all, fields(conversation_id = %id))]
#[utoipa::path(
    get,
    path = "/api/v1/conversations/{id}/messages",
    tag = "booking-requests",
    responses(
        (status = 200, description = "The thread, oldest first", body = BookingMessagesWrapper),
        (status = 403, description = "NOT_REQUEST_PARTY"),
        (status = 404, description = "No such conversation")
    )
)]
/// The conversation's messages, for its guest, the host or an admin.
pub(super) async fn get_conversation_messages(
    req: HttpRequest,
    claims: Claims,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    authorize(pool.get_ref(), &claims, *id).await?;
    let messages = db_conversation::list_messages(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?;
    let unread_count = messages
        .iter()
        .filter(|m| m.read_at.is_none() && m.sender_id != claims.sub)
        .count() as i64;
    Ok(respond(
        &req,
        Payload::Item(BookingMessagesWrapper {
            messages: messages.into_iter().map(Into::into).collect(),
            unread_count,
        }),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip_all, fields(conversation_id = %id))]
#[utoipa::path(
    post,
    path = "/api/v1/conversations/{id}/messages",
    tag = "booking-requests",
    request_body = CreateBookingMessageRequest,
    responses(
        (status = 201, description = "Posted", body = BookingMessageResponse),
        (status = 400, description = "Empty or invalid text"),
        (status = 403, description = "NOT_REQUEST_PARTY"),
        (status = 404, description = "No such conversation")
    )
)]
/// Posts a message, and emails the other party.
pub(super) async fn post_conversation_message(
    req: HttpRequest,
    claims: Claims,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    body: web::Json<CreateBookingMessageRequest>,
) -> Result<HttpResponse, ApiError> {
    body.validate().map_err(ApiError::ValidationError)?;
    let text = body.into_inner().message_text;
    check_user_text(&text)?;
    let text = text.trim();
    if text.is_empty() {
        return Err(ApiError::Database(DbError::ValidationError(
            "Message cannot be empty".to_string(),
        )));
    }
    let (parties, user, role) = authorize(pool.get_ref(), &claims, *id).await?;
    let sender_name = if role == DbMessageSenderRole::Admin {
        format!("{} (admin)", user.first_name)
    } else {
        user.first_name.clone()
    };
    let message = db_conversation::insert_message(
        pool.get_ref(),
        &db_conversation::NewMessage {
            conversation_id: *id,
            sender_id: claims.sub,
            sender_role: role,
            sender_name: &sender_name,
            text,
            booking_id: None,
            enquiry_id: None,
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::Database)?;
    tracing::info!(conversation_id = %id, chars = text.chars().count(), "conversation message posted");

    let recipient = if role == DbMessageSenderRole::Guest {
        parties.host_id
    } else {
        parties.conversation.guest_id
    };
    let pool_clone = pool.get_ref().clone();
    let text = text.to_string();
    let conversation_id = *id;
    tokio::spawn(async move {
        let recipient = match db_core::user::get_user_by_id(&pool_clone, recipient).await {
            Ok(user) => user,
            Err(e) => {
                tracing::error!(%conversation_id, %recipient, error = ?e, "message email not sent: recipient lookup failed");
                return;
            }
        };
        enqueue_email(
            &pool_clone,
            &recipient.email,
            "New message on Our Places",
            common::email::EmailTemplate::GuestHostMessageNotification.as_str(),
            &serde_json::json!({
                "sender_name": sender_name,
                "message_text": text,
                "conversation_id": conversation_id.to_string(),
            }),
        )
        .await;
    });

    Ok(respond(
        &req,
        Payload::Item::<BookingMessageResponse>(message.into()),
        |_| (),
        actix_web::http::StatusCode::CREATED,
    ))
}

#[tracing::instrument(skip_all, fields(conversation_id = %id))]
#[utoipa::path(
    patch,
    path = "/api/v1/conversations/{id}/messages/read",
    tag = "booking-requests",
    responses(
        (status = 200, description = "Marked read", body = MarkMessagesReadResponse),
        (status = 403, description = "NOT_REQUEST_PARTY")
    )
)]
/// Marks the other party's messages as read.
pub(super) async fn mark_conversation_read(
    req: HttpRequest,
    claims: Claims,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    authorize(pool.get_ref(), &claims, *id).await?;
    let updated = db_conversation::mark_read(pool.get_ref(), *id, claims.sub)
        .await
        .map_err(ApiError::Database)?;
    Ok(respond(
        &req,
        Payload::Item(MarkMessagesReadResponse {
            updated_count: updated,
        }),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}
