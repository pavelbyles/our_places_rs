//! Guest-to-host messaging on a booking.

use super::enqueue_email;
use actix_web::{HttpRequest, Responder, web};
use api_core::error::ApiError;
use api_core::response::{Payload, respond};
use db_core::booking_message as db_booking_message;
use db_core::models::BookingStatus;
use sqlx::PgPool;
use uuid::Uuid;
use validator::Validate;

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v1/bookings/{id}/messages",
    tag = "bookings",
    responses(
        (status = 200, description = "Messages retrieved", body = common::models::BookingMessagesWrapper),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Booking not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn get_booking_messages(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    claims: api_core::auth::Claims,
) -> Result<impl Responder, ApiError> {
    let parties = db_booking_message::get_booking_parties(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::Database(db_core::error::DbError::Sqlx(
            sqlx::Error::RowNotFound,
        )))?;

    let user = db_core::user::get_user_by_id(pool.get_ref(), claims.sub)
        .await
        .map_err(|_| ApiError::Unauthorized("User not found".to_string()))?;

    let is_admin = user.roles.contains(&db_core::models::UserRole::Admin);
    let is_guest = claims.sub == parties.guest_id;
    let is_host = claims.sub == parties.host_id;

    if !is_admin && !is_guest && !is_host {
        return Err(ApiError::Unauthorized(
            "Not authorized to access messages".to_string(),
        ));
    }

    let msgs = db_booking_message::list_booking_messages(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?;

    let unread_count = msgs
        .iter()
        .filter(|m| m.read_at.is_none() && m.sender_id != claims.sub)
        .count() as i64;

    let response = common::models::BookingMessagesWrapper {
        messages: msgs.into_iter().map(Into::into).collect(),
        unread_count,
    };

    Ok(respond(
        &req,
        Payload::Item(response),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    post,
    path = "/api/v1/bookings/{id}/messages",
    tag = "bookings",
    request_body = common::models::CreateBookingMessageRequest,
    responses(
        (status = 201, description = "Message created", body = common::models::BookingMessageResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn send_booking_message(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    claims: api_core::auth::Claims,
    body: web::Json<common::models::CreateBookingMessageRequest>,
) -> Result<impl Responder, ApiError> {
    body.validate().map_err(ApiError::ValidationError)?;
    let text = body.into_inner().message_text;

    // Check for null bytes / control chars
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(
                "Message contains invalid characters".to_string(),
            ),
        ));
    }

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError("Message cannot be empty".to_string()),
        ));
    }

    let parties = db_booking_message::get_booking_parties(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::Database(db_core::error::DbError::Sqlx(
            sqlx::Error::RowNotFound,
        )))?;

    if parties.status == BookingStatus::Cancelled {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(
                "Cannot send message to a cancelled booking".to_string(),
            ),
        ));
    }

    let user = db_core::user::get_user_by_id(pool.get_ref(), claims.sub)
        .await
        .map_err(|_| ApiError::Unauthorized("User not found".to_string()))?;

    let is_admin = user.roles.contains(&db_core::models::UserRole::Admin);
    let is_guest = claims.sub == parties.guest_id;
    let is_host = claims.sub == parties.host_id;

    let role = if is_admin {
        db_core::models::DbMessageSenderRole::Admin
    } else if is_host {
        db_core::models::DbMessageSenderRole::Host
    } else if is_guest {
        db_core::models::DbMessageSenderRole::Guest
    } else {
        return Err(ApiError::Unauthorized(
            "Not authorized to send messages".to_string(),
        ));
    };

    let display_name = if is_admin {
        format!("{} (admin)", user.first_name)
    } else {
        user.first_name.clone()
    };

    let msg = db_booking_message::insert_booking_message(
        pool.get_ref(),
        Uuid::now_v7(),
        *id,
        claims.sub,
        role,
        &display_name,
        trimmed,
    )
    .await
    .map_err(ApiError::Database)?;

    let recipient_user_id = if is_host {
        parties.guest_id
    } else {
        parties.host_id
    };
    let pool_clone = pool.get_ref().clone();
    let display_name_clone = display_name.clone();
    let text_clone = trimmed.to_string();
    let booking_id = *id;

    tokio::spawn(async move {
        tracing::info!(
            "Triggered email notification for message in booking {}",
            booking_id
        );
        let recipient = match db_core::user::get_user_by_id(&pool_clone, recipient_user_id).await {
            Ok(recipient) => recipient,
            Err(e) => {
                tracing::error!(%booking_id, %recipient_user_id, error = ?e, "message notification email not sent: recipient lookup failed");
                return;
            }
        };
        enqueue_email(
            &pool_clone,
            &recipient.email,
            &format!("New message regarding booking {}", booking_id),
            common::email::EmailTemplate::GuestHostMessageNotification.as_str(),
            &serde_json::json!({
                "sender_name": display_name_clone,
                "message_text": text_clone,
                "booking_id": booking_id.to_string(),
            }),
        )
        .await;
    });

    Ok(respond(
        &req,
        Payload::Item::<common::models::BookingMessageResponse>(msg.into()),
        |_| (),
        actix_web::http::StatusCode::CREATED,
    ))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    patch,
    path = "/api/v1/bookings/{id}/messages/read",
    tag = "bookings",
    responses(
        (status = 200, description = "Messages marked as read", body = common::models::MarkMessagesReadResponse),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn mark_booking_messages_read(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    claims: api_core::auth::Claims,
) -> Result<impl Responder, ApiError> {
    let parties = db_booking_message::get_booking_parties(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::Database(db_core::error::DbError::Sqlx(
            sqlx::Error::RowNotFound,
        )))?;

    let user = db_core::user::get_user_by_id(pool.get_ref(), claims.sub)
        .await
        .map_err(|_| ApiError::Unauthorized("User not found".to_string()))?;

    let is_admin = user.roles.contains(&db_core::models::UserRole::Admin);
    let is_guest = claims.sub == parties.guest_id;
    let is_host = claims.sub == parties.host_id;

    if !is_admin && !is_guest && !is_host {
        return Err(ApiError::Unauthorized(
            "Not authorized to access messages".to_string(),
        ));
    }

    let updated =
        db_booking_message::mark_booking_messages_as_read(pool.get_ref(), *id, claims.sub)
            .await
            .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Item(common::models::MarkMessagesReadResponse {
            updated_count: updated,
        }),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}
