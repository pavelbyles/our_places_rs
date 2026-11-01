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
use web_client::{get_api_client, session::GuestSession};
use web_ui_common::components::chat::{chat_bubble, chat_thread};

use crate::support::{guest, service_token};

path_param!(code);

/// Message form fields.
#[derive(Deserialize)]
pub struct MessageForm {
    /// The message text.
    pub message_text: String,
}

/// Finds the booking by confirmation code (or ID) among the guest's own
/// bookings only; anything else is a 404.
async fn own_booking_by_code(cx: &Cx, guest: &GuestSession, code: &str) -> Result<BookingResponse> {
    let token = service_token(cx, guest)?;
    get_api_client(cx)
        .get_user_bookings_as(guest.user_id, &token)
        .await
        .map_err(web_client::app_error)?
        .into_iter()
        .find(|b| b.confirmation_code.eq_ignore_ascii_case(code) || b.id.to_string() == code)
        .ok_or_else(|| not_found().into())
}

fn bubble_parts(m: &BookingMessageResponse) -> (String, String, String, bool) {
    (
        m.sender_name.clone(),
        m.created_at.format("%b %d, %H:%M").to_string(),
        m.message_text.clone(),
        m.sender_role == MessageSenderRole::Guest,
    )
}

#[component]
async fn message_bubble(message: BookingMessageResponse) -> Result<impl View> {
    let (sender, sent_at, text, mine) = bubble_parts(&message);
    Ok(
        view! { chat_bubble(sender: sender, sent_at: sent_at, text: text, mine: mine, attrs: topcoat::view::attributes! { id=(format!("msg-{}", message.id)) }) },
    )
}

/// The guest–host thread for one of the guest's bookings.
#[page("/bookings/{code}/messages")]
pub async fn messages_page(cx: &Cx) -> Result<impl View> {
    let guest = guest(cx).await?;
    let booking = own_booking_by_code(cx, &guest, path_param::<Code>(cx)).await?;
    let token = service_token(cx, &guest)?;
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
    let status = booking.status.to_lowercase();
    let closed = status == "cancelled" || status == "refunded";
    let action = format!("/bookings/{}/messages", booking.confirmation_code);
    Ok(view! {
        <div class="mx-auto max-w-3xl space-y-6">
            <div class="space-y-1">
                <a href="/bookings" class="text-sm">"← My bookings"</a>
                <h1 class="font-serif text-[clamp(28px,4vw,40px)]">"Messages with your host"</h1>
                <p class="text-sm text-muted-foreground">"Reservation " <span class="font-mono">(booking.confirmation_code.clone())</span></p>
            </div>
            chat_thread(attrs: topcoat::view::attributes! { id="messages-list" class="rounded-[var(--radius-surface,16px)] border border-border bg-card p-4" },
                if thread.messages.is_empty() {
                    <p class="text-sm text-muted-foreground">"No messages yet."</p>
                }
                for message in thread.messages {
                    message_bubble(message: message)
                }
            )
            if closed {
                <p role="note" class="text-sm text-muted-foreground">"This booking is cancelled, so messaging is closed."</p>
            } else {
                <form method="post" action=(action.clone()) hx-post=(action) hx-target="#messages-list" hx-swap="beforeend" data-reset-on-success="" class="flex gap-2">
                    <label for="message-text" class="sr-only">"Message"</label>
                    <input id="message-text" name="message_text" required=(true) maxlength="2000" autocomplete="off" placeholder="Type your message…" class="h-11 flex-1 rounded-full border border-border bg-card px-4 text-sm" />
                    <button type="submit" class="h-11 rounded-full bg-primary px-6 text-sm font-medium text-primary-foreground">"Send"</button>
                </form>
            }
        </div>
    })
}

/// Sends a message. HTMX gets the new bubble to append; a plain form post
/// returns to the thread.
#[page(POST "/bookings/{code}/messages")]
pub async fn send_message(
    cx: &Cx,
    /// The message form.
    Form(form): Form<MessageForm>,
) -> Result<impl View> {
    let guest = guest(cx).await?;
    let booking = own_booking_by_code(cx, &guest, path_param::<Code>(cx)).await?;
    let token = service_token(cx, &guest)?;
    let text = form.message_text.trim().to_string();
    let sent = if text.is_empty() {
        Err("Write a message first.".to_string())
    } else {
        get_api_client(cx)
            .send_booking_message_as(booking.id, &text, &token)
            .await
            .map_err(|e| {
                tracing::info!("message rejected: {e}");
                "Your message could not be sent. Messaging closes when a booking is cancelled."
                    .to_string()
            })
    };
    if !topcoat::htmx::hx_request(cx) {
        return Err(see_other(format!("/bookings/{}/messages", booking.confirmation_code)).into());
    }
    Ok(view! {
        match sent {
            Ok(message) => { message_bubble(message: message) }
            Err(error) => { <p role="alert" class="text-sm text-destructive">(error)</p> }
        }
    })
}
