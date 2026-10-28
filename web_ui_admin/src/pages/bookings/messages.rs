//! Host/admin side of a booking's message thread.

use common::models::{BookingMessageResponse, BookingResponse, MessageSenderRole};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form,
        error::{not_found, see_other},
        page, path_param,
    },
    view::{View, component, view},
};
use web_client::{get_api_client, session::AdminSession};
use web_ui_common::components::chat::{chat_bubble, chat_thread};

use crate::support::{scoped_bookings, scoped_listings, service_token, staff};

path_param!(code);

/// Message form fields.
#[derive(Deserialize)]
pub struct MessageForm {
    /// The message text.
    pub message_text: String,
}

/// Finds the booking by confirmation code (or ID) among the bookings the
/// user may manage; anything else is a 404.
async fn managed_by_code(cx: &Cx, user: &AdminSession, code: &str) -> Result<BookingResponse> {
    let listings = scoped_listings(cx, user).await;
    scoped_bookings(cx, user, &listings)
        .await
        .into_iter()
        .find(|b| b.confirmation_code.eq_ignore_ascii_case(code) || b.id.to_string() == code)
        .ok_or_else(|| not_found().into())
}

#[component]
async fn message_bubble(message: BookingMessageResponse) -> Result<impl View> {
    let mine = message.sender_role != MessageSenderRole::Guest;
    let sent_at = message.created_at.format("%b %d, %H:%M").to_string();
    Ok(view! {
        chat_bubble(sender: message.sender_name.clone(), sent_at: sent_at, text: message.message_text.clone(), mine: mine, attrs: topcoat::view::attributes! { id=(format!("msg-{}", message.id)) })
    })
}

/// The thread for one booking.
#[page("/admin/bookings/{code}/messages")]
pub async fn admin_messages_page(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let booking = managed_by_code(cx, &user, path_param::<Code>(cx)).await?;
    let token = service_token(cx, &user)?;
    let api = get_api_client(cx);
    let thread = api
        .get_booking_messages_as(booking.id, &token)
        .await
        .map_err(web_client::app_error)?;
    if thread.unread_count > 0
        && let Err(e) = api.mark_booking_messages_read_as(booking.id, &token).await
    {
        tracing::warn!("failed to mark messages read: {e}");
    }
    let action = format!("/admin/bookings/{}/messages", booking.confirmation_code);
    Ok(view! {
        <div class="mx-auto max-w-3xl space-y-6">
            <div>
                <a href="/admin/bookings" class="text-sm">"← Bookings"</a>
                <h1 class="text-2xl font-semibold">(format!("Messages · {}", booking.confirmation_code))</h1>
            </div>
            chat_thread(attrs: topcoat::view::attributes! { id="admin-messages-list" class="rounded-xl border border-border p-4" },
                if thread.messages.is_empty() {
                    <p class="text-sm text-muted-foreground">"No messages yet."</p>
                }
                for message in thread.messages {
                    message_bubble(message: message)
                }
            )
            <form method="post" action=(action.clone()) hx-post=(action) hx-target="#admin-messages-list" hx-swap="beforeend" data-reset-on-success="" class="flex gap-2">
                <label for="admin-message-text" class="sr-only">"Message"</label>
                <input id="admin-message-text" name="message_text" required=(true) maxlength="2000" autocomplete="off" placeholder="Reply to the guest…" class="h-10 flex-1 rounded-lg border border-border bg-card px-3 text-sm" />
                <button type="submit" class="h-10 rounded-lg bg-primary px-5 text-sm font-medium text-primary-foreground">"Send"</button>
            </form>
        </div>
    })
}

/// Sends a reply. HTMX gets the new bubble; a plain post returns to the thread.
#[page(POST "/admin/bookings/{code}/messages")]
pub async fn admin_send_message(
    cx: &Cx,
    /// The message form.
    Form(form): Form<MessageForm>,
) -> Result<impl View> {
    let user = staff(cx).await?;
    let booking = managed_by_code(cx, &user, path_param::<Code>(cx)).await?;
    let token = service_token(cx, &user)?;
    let text = form.message_text.trim().to_string();
    let sent = if text.is_empty() {
        Err("Write a message first.".to_string())
    } else {
        get_api_client(cx)
            .send_booking_message_as(booking.id, &text, &token)
            .await
            .map_err(|e| {
                tracing::info!("admin message rejected: {e}");
                "The message could not be sent.".to_string()
            })
    };
    if !topcoat::htmx::hx_request(cx) {
        return Err(see_other(format!(
            "/admin/bookings/{}/messages",
            booking.confirmation_code
        ))
        .into());
    }
    Ok(view! {
        match sent {
            Ok(message) => { message_bubble(message: message) }
            Err(error) => { <p role="alert" class="text-sm text-destructive">(error)</p> }
        }
    })
}
