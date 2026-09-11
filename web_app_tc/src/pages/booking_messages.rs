use common::models::{BookingMessageResponse, CreateBookingMessageRequest, MessageSenderRole};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use topcoat::router::content::Form;
use topcoat::{
    Result,
    context::Cx,
    router::{error::RouterErrorExt, page, path_param},
    view::{View, view},
};
use uuid::Uuid;
use web_app_common_tc::get_api_client;

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    exp: usize,
}

fn generate_jwt_for_user(user_id: Uuid) -> String {
    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());
    let claims = Claims {
        sub: user_id,
        exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}

path_param!(code);

#[derive(serde::Deserialize)]
pub struct SendMessageForm {
    pub message_text: String,
}

#[page(POST "/bookings/{code}/messages")]
pub async fn send_booking_message_htmx(
    cx: &Cx,
    Form(payload): Form<SendMessageForm>,
) -> Result<impl View> {
    let user = web_app_common_tc::auth::get_guest_session(cx)
        .await
        .ok_or_else(|| anyhow::anyhow!("Unauthorized"))?;
    let code: &str = path_param::<Code>(cx);
    let api = get_api_client(cx);
    let bookings = api
        .get_all_bookings(Some(1), Some(100))
        .await
        .unwrap_or_default();
    let booking = bookings
        .into_iter()
        .find(|b| b.confirmation_code.eq_ignore_ascii_case(code) || b.id.to_string() == code)
        .ok_or_else(|| anyhow::anyhow!("Booking not found"))?;

    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let url = format!(
        "{}/api/v1/bookings/{}/messages",
        common::app_client::booking_api_url(),
        booking.id
    );
    let client = reqwest::Client::new();
    let req_payload = CreateBookingMessageRequest {
        message_text: payload.message_text,
    };

    let res = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&req_payload)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Server error: {}", e))?;

    let (err_text, msg) = if !res.status().is_success() {
        let err = if res.status() == reqwest::StatusCode::BAD_REQUEST {
            "Cannot send message: this booking has been cancelled or has an invalid status."
        } else {
            "Failed to send message. Please try again later."
        };
        (Some(err), None)
    } else {
        let msg = res
            .json::<BookingMessageResponse>()
            .await
            .map_err(|e| anyhow::anyhow!("Parse error: {}", e))?;
        (None, Some(msg))
    };

    Ok(view! {
        if let Some(err) = err_text {
            <div class="alert alert-error text-xs shadow-sm my-2 flex items-center justify-between">
                <span>(err)</span>
            </div>
        } else if let Some(msg) = msg {
            let time_str = msg.created_at.format("%b %d, %H:%M").to_string();
            <div class="chat chat-end mb-4">
                <div class="chat-header text-xs opacity-70 mb-1">
                    (msg.sender_name) " · " (time_str)
                </div>
                <div class="chat-bubble chat-bubble-primary text-sm shadow-sm whitespace-pre-wrap">
                    (msg.message_text)
                </div>
            </div>
        }
    })
}

#[page("/bookings/{code}/messages")]
pub async fn booking_messages_page(cx: &Cx) -> Result<impl View> {
    let user = web_app_common_tc::auth::get_guest_session(cx)
        .await
        .ok_or_redirect("/login?redirect=%2Fbookings")?;
    let code: &str = path_param::<Code>(cx);

    let api = get_api_client(cx);

    // Fetch the booking directly
    let bookings = api
        .get_all_bookings(Some(1), Some(100))
        .await
        .unwrap_or_default();
    let booking = bookings
        .into_iter()
        .find(|b| b.confirmation_code.eq_ignore_ascii_case(code) || b.id.to_string() == code)
        .ok_or_redirect("/bookings")?;

    let listing_details = api
        .get_listing_by_id(&booking.listing_id.to_string(), None)
        .await
        .ok();
    let host_name = listing_details
        .as_ref()
        .and_then(|l| l.host_name.clone())
        .unwrap_or_else(|| "Host".to_string());
    let listing_name = listing_details
        .as_ref()
        .map(|l| l.listing.name.clone())
        .unwrap_or_else(|| "Luxury Jamaican Villa".to_string());
    let listing_slug = listing_details
        .as_ref()
        .map(|l| l.listing.slug.clone())
        .unwrap_or_else(|| booking.listing_id.to_string());
    let listing_url = format!("/listings/{}", listing_slug);

    // Fetch messages
    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let url = format!(
        "{}/api/v1/bookings/{}/messages",
        common::app_client::booking_api_url(),
        booking.id
    );
    let client = reqwest::Client::new();
    let msgs_wrapper = if let Ok(res) = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
    {
        if res.status().is_success() {
            res.json::<common::models::BookingMessagesWrapper>()
                .await
                .ok()
        } else {
            None
        }
    } else {
        None
    };
    let messages = msgs_wrapper
        .as_ref()
        .map(|w| w.messages.clone())
        .unwrap_or_default();

    // Mark as read
    let read_url = format!(
        "{}/api/v1/bookings/{}/messages/read",
        common::app_client::booking_api_url(),
        booking.id
    );
    let _ = client
        .patch(&read_url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await;

    let start_str = booking.date_from.format("%b %d, %Y").to_string();
    let end_str = booking.date_to.format("%b %d, %Y").to_string();
    let total_str = format!("{} {}", booking.currency, booking.total_price);
    let is_cancelled = booking.status.eq_ignore_ascii_case("cancelled")
        || booking.status.eq_ignore_ascii_case("refunded");
    let conf_code = booking.confirmation_code.clone();

    Ok(view! {
        <div class="max-w-4xl mx-auto px-4 py-8">
            <div class="mb-6 flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                <div>
                    <a href="/bookings" class="btn btn-ghost btn-sm pl-0">"← Back to Bookings"</a>
                    <h1 class="text-3xl font-extrabold tracking-tight mt-2">"Message " (host_name.clone())</h1>
                    <div class="text-sm mt-1 flex flex-wrap items-center gap-2">
                        <span class="font-medium text-base-content/70">"Property:"</span>
                        <a href=(listing_url.clone()) class="link link-primary font-semibold hover:underline inline-flex items-center gap-1">
                            (listing_name.clone())
                            <span class="text-xs">"↗"</span>
                        </a>
                        <span class="text-base-content/40">"•"</span>
                        <span class="text-base-content/70">"Stay from " (start_str) " to " (end_str)</span>
                        <span class="text-base-content/40">"•"</span>
                        <span class="text-base-content/70">"Total: " (total_str)</span>
                        <span class="text-base-content/40">"•"</span>
                        <span class="text-base-content/70">"Status: "</span>
                        <span class="font-semibold capitalize">(booking.status)</span>
                    </div>
                </div>
                <div class="flex items-center gap-2">
                    <a href=(listing_url) class="btn btn-outline btn-sm rounded-xl font-semibold gap-1">
                        "View Listing"
                        <span class="text-xs">"↗"</span>
                    </a>
                </div>
            </div>

            <div class="card bg-base-100 shadow-xl border border-base-200 flex flex-col h-[600px] overflow-hidden">
                <div class="p-4 border-b border-base-200 bg-base-200/50 flex justify-between items-center">
                    <div class="font-bold">"Conversation Details"</div>
                    <div class="text-xs text-base-content/60">"Booking ID: " (conf_code.clone())</div>
                </div>

                <div class="flex-1 overflow-y-auto p-6 space-y-2 bg-base-100/50" id="messages-list">
                    if messages.is_empty() {
                        <div class="text-center text-sm text-base-content/50 my-10 italic">"No messages yet. Say hello to " (host_name) "!"</div>
                    } else {
                        for m in messages {
                            let is_me = m.sender_role == MessageSenderRole::Guest;
                            let align_class = if is_me { "chat chat-end mb-4" } else { "chat chat-start mb-4" };
                            let bubble_class = if is_me { "chat-bubble chat-bubble-primary text-sm shadow-sm whitespace-pre-wrap" } else { "chat-bubble chat-bubble-secondary text-sm shadow-sm whitespace-pre-wrap" };
                            let time_str = m.created_at.format("%b %d, %H:%M").to_string();
                            <div class=(align_class)>
                                <div class="chat-header text-xs opacity-70 mb-1">
                                    (m.sender_name) " · " (time_str)
                                </div>
                                <div class=(bubble_class)>
                                    (m.message_text)
                                </div>
                            </div>
                        }
                    }
                </div>

                if is_cancelled {
                    <div class="p-4 border-t border-base-200 bg-base-200/50 text-center">
                        <div class="alert alert-warning text-xs inline-flex w-auto py-2 px-4 shadow-sm">
                            <span>"⚠️ This booking is cancelled. Messaging is no longer available."</span>
                        </div>
                    </div>
                } else {
                    <div class="p-4 border-t border-base-200 bg-base-100">
                        <form
                            hx-post=(format!("/bookings/{}/messages", conf_code))
                            hx-target="#messages-list"
                            hx-swap="beforeend"
                            onsubmit="setTimeout(() => { this.reset(); let list = document.getElementById('messages-list'); list.scrollTop = list.scrollHeight; }, 10);"
                            class="flex gap-3"
                        >
                            <input
                                type="text"
                                name="message_text"
                                class="input input-bordered flex-1 rounded-2xl"
                                placeholder="Type your message here..."
                                required="true"
                                autocomplete="off"
                            />
                            <button type="submit" class="btn btn-primary rounded-2xl px-8 font-bold">
                                "Send"
                            </button>
                        </form>
                    </div>
                }
            </div>
        </div>
    })
}
