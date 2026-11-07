//! Enquiries (#105): a guest asks the host a question with dates and a guest
//! count, and the reply continues in one thread with the host.

use chrono::NaiveDate;
use common::app_client::booking_requests::{
    api_error_code, create_enquiry, get_conversation_messages, get_enquiry, mark_conversation_read,
    post_conversation_message,
};
use common::booking_request::{CreateEnquiryBody, EnquiryStatus, RequestRefusal};
use common::models::{BookingMessageResponse, ListingDetails, MessageSenderRole};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form,
        error::{not_found, see_other},
        page, parse_query_params, path_param,
    },
    view::{View, component, view},
};
use uuid::Uuid;
use web_client::{get_api_client, session::GuestSession};
use web_ui_common::components::chat::{chat_bubble, chat_thread};

use crate::support::{display_currency, guest, guests_or_default, jamaica_today, service_token};

path_param!(slug);
path_param!(id);

/// What to tell the guest when an enquiry or request call is refused.
pub fn friendly_error(err: &anyhow::Error) -> String {
    let code = api_error_code(err);
    if let Some(refusal) = code.and_then(RequestRefusal::from_code) {
        return refusal.message();
    }
    match code {
        Some("LISTING_UNAVAILABLE") => "This property is no longer taking bookings.".to_string(),
        Some("DATE_UNAVAILABLE") => {
            "Those dates are no longer available. Please choose different dates.".to_string()
        }
        Some(code) if code.starts_with("INVALID_") => {
            common::availability::stay_refusal_message_in_error(&format!(r#"{{"code":"{code}"}}"#))
                .unwrap_or_else(|| "Please check the dates and guests.".to_string())
        }
        _ => "Something went wrong. Please try again.".to_string(),
    }
}

/// The guest's view of one message.
#[component]
pub async fn thread_message(
    /// The message.
    message: BookingMessageResponse,
) -> Result<impl View> {
    let mine = message.sender_role == MessageSenderRole::Guest;
    let sent_at = message.created_at.format("%b %d, %H:%M").to_string();
    Ok(view! {
        chat_bubble(sender: message.sender_name.clone(), sent_at: sent_at, text: message.message_text.clone(), mine: mine, attrs: topcoat::view::attributes! { id=(format!("msg-{}", message.id)) })
    })
}

/// A conversation with its reply form. `action` is where replies post.
#[component]
pub async fn conversation_panel(
    /// The thread, oldest first.
    messages: Vec<BookingMessageResponse>,
    /// Where replies post.
    action: String,
) -> Result<impl View> {
    Ok(view! {
        <section aria-label="Messages with your host" class="space-y-3">
            <h2 class="font-serif text-2xl">"Messages"</h2>
            chat_thread(attrs: topcoat::view::attributes! { id="messages-list" class="rounded-[var(--radius-surface,16px)] border border-border bg-card p-4" },
                if messages.is_empty() {
                    <p class="text-sm text-muted-foreground">"No messages yet."</p>
                }
                for message in messages {
                    thread_message(message: message)
                }
            )
            <form method="post" action=(action.clone()) hx-post=(action) hx-target="#messages-list" hx-swap="beforeend" data-reset-on-success="" class="flex gap-2">
                <label for="message-text" class="sr-only">"Message"</label>
                <input id="message-text" name="message_text" required=(true) maxlength="2000" autocomplete="off" placeholder="Write to your host…" class="h-11 flex-1 rounded-full border border-border bg-card px-4 text-sm" />
                <button type="submit" class="h-11 rounded-full bg-primary px-6 text-sm font-medium text-primary-foreground">"Send"</button>
            </form>
        </section>
    })
}

/// Loads a conversation's messages as the guest, marking the host's read.
pub async fn load_thread(conversation_id: Uuid, token: &str) -> Vec<BookingMessageResponse> {
    match get_conversation_messages(conversation_id, token).await {
        Ok(thread) => {
            if thread.unread_count > 0
                && let Err(e) = mark_conversation_read(conversation_id, token).await
            {
                tracing::warn!(%conversation_id, "failed to mark messages read: {e}");
            }
            thread.messages
        }
        Err(e) => {
            tracing::warn!(%conversation_id, "thread unavailable: {e}");
            Vec::new()
        }
    }
}

/// Reply form fields.
#[derive(Deserialize)]
pub struct ReplyForm {
    /// The message text.
    pub message_text: String,
}

/// Posts a reply. A plain (non-HTMX) post is answered with a redirect to
/// `back`; HTMX gets the outcome to render with [`reply_result`].
pub async fn post_reply(
    cx: &Cx,
    conversation_id: Uuid,
    token: &str,
    text: &str,
    back: String,
) -> Result<std::result::Result<BookingMessageResponse, String>> {
    let text = text.trim().to_string();
    let sent = if text.is_empty() {
        Err("Write a message first.".to_string())
    } else {
        post_conversation_message(conversation_id, &text, token)
            .await
            .map_err(|e| friendly_error(&e))
    };
    if !topcoat::htmx::hx_request(cx) {
        return Err(see_other(back).into());
    }
    Ok(sent)
}

/// The new bubble to append, or why the message wasn't sent.
#[component]
pub async fn reply_result(
    /// The posted message, or why it wasn't sent.
    sent: std::result::Result<BookingMessageResponse, String>,
) -> Result<impl View> {
    Ok(view! {
        match sent {
            Ok(message) => { thread_message(message: message) }
            Err(error) => { <p role="alert" class="text-sm text-destructive">(error)</p> }
        }
    })
}

#[derive(Deserialize, Default)]
struct EnquireQuery {
    check_in: Option<NaiveDate>,
    check_out: Option<NaiveDate>,
    guests: Option<u32>,
}

/// Enquiry form fields.
#[derive(Deserialize)]
pub struct EnquiryForm {
    /// Check-in date asked about.
    pub check_in: NaiveDate,
    /// Check-out date asked about.
    pub check_out: NaiveDate,
    /// Guests asked about.
    pub guests: u32,
    /// The question.
    pub message: String,
}

async fn active_listing(cx: &Cx, slug: &str) -> Result<ListingDetails> {
    get_api_client(cx)
        .get_listing_by_id(slug, None)
        .await
        .ok()
        .filter(|d| d.listing.is_active)
        .ok_or_else(|| not_found().into())
}

#[component]
async fn enquiry_form(
    details: ListingDetails,
    check_in: String,
    check_out: String,
    guests: u32,
    message: String,
    error: Option<String>,
) -> Result<impl View> {
    let label = "block text-xs font-semibold tracking-wide text-muted-foreground uppercase";
    let input = "mt-1 block h-11 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground normal-case tracking-normal font-normal";
    let action = format!("/listings/{}/enquire", details.listing.slug);
    let max_guests = details.listing.max_guests.max(1);
    let today = jamaica_today().to_string();
    Ok(view! {
        <div class="mx-auto max-w-2xl space-y-6">
            <div class="space-y-1">
                <a href=(format!("/listings/{}", details.listing.slug)) class="text-sm">"← Back to the listing"</a>
                <h1 class="font-serif text-[clamp(28px,4vw,40px)]">"Ask the host a question"</h1>
                <p class="text-sm text-muted-foreground">(details.listing.name.clone()) ". Asking doesn't hold any dates or charge you."</p>
            </div>
            if let Some(error) = error {
                <div id="enquiry-error" role="alert" class="rounded-lg border border-destructive p-4 text-sm text-destructive">(error)</div>
            }
            <form id="enquiry-form" method="post" action=(action) class="space-y-4 rounded-[var(--radius-surface,16px)] border border-border bg-card p-6">
                <div class="grid gap-3 sm:grid-cols-3">
                    <label class=(label)>"Check in"<input id="enquiry-check-in" type="date" name="check_in" min=(today.clone()) required=(true) value=(check_in) class=(input) /></label>
                    <label class=(label)>"Check out"<input id="enquiry-check-out" type="date" name="check_out" min=(today) required=(true) value=(check_out) class=(input) /></label>
                    <label class=(label)>"Guests"<input id="enquiry-guests" type="number" name="guests" min="1" max=(max_guests.to_string()) required=(true) value=(guests.to_string()) class=(input) /></label>
                </div>
                <label class=(label) for="enquiry-message">"Your question"
                    <textarea id="enquiry-message" name="message" rows="5" required=(true) maxlength="2000" class=(format!("{input} h-auto py-2"))>(message)</textarea>
                </label>
                <button id="enquiry-submit" type="submit" class="h-11 w-full rounded-[var(--radius-control,10px)] bg-primary text-[15px] font-medium text-primary-foreground">"Send question"</button>
            </form>
        </div>
    })
}

/// The enquiry form, prefilled with the stay chosen on the listing page.
#[page("/listings/{slug}/enquire")]
pub async fn enquire_page(cx: &Cx) -> Result<impl View> {
    guest(cx).await?;
    let details = active_listing(cx, path_param::<Slug>(cx)).await?;
    let q = parse_query_params::<EnquireQuery>(cx).unwrap_or_default();
    Ok(view! {
        enquiry_form(
            details: details,
            check_in: q.check_in.map(|d| d.to_string()).unwrap_or_default(),
            check_out: q.check_out.map(|d| d.to_string()).unwrap_or_default(),
            guests: guests_or_default(q.guests),
            message: String::new(),
            error: None,
        )
    })
}

/// Sends the enquiry, then shows its thread.
#[page(POST "/listings/{slug}/enquire")]
pub async fn enquire_submit(
    cx: &Cx,
    /// The enquiry form.
    Form(form): Form<EnquiryForm>,
) -> Result<impl View> {
    let guest = guest(cx).await?;
    let details = active_listing(cx, path_param::<Slug>(cx)).await?;
    let token = service_token(cx, &guest)?;
    let currency = display_currency(cx)
        .await
        .unwrap_or_else(|| details.listing.base_currency.clone());
    let body = CreateEnquiryBody {
        listing_id: details.listing.id,
        check_in: form.check_in,
        check_out: form.check_out,
        guests: form.guests,
        currency,
        message: form.message.trim().to_string(),
    };
    match create_enquiry(&body, &token).await {
        Ok(enquiry) => Err(see_other(format!("/requests/enquiries/{}", enquiry.id)).into()),
        Err(e) => {
            tracing::info!(
                code = api_error_code(&e).unwrap_or("none"),
                "enquiry refused"
            );
            Ok(view! {
                enquiry_form(
                    details: details,
                    check_in: form.check_in.to_string(),
                    check_out: form.check_out.to_string(),
                    guests: form.guests,
                    message: form.message.clone(),
                    error: Some(friendly_error(&e)),
                )
            })
        }
    }
}

/// The checkout link that turns an enquiry into a request to book.
pub fn request_from_enquiry_href(
    slug: &str,
    e: &common::booking_request::EnquiryResponse,
) -> String {
    format!(
        "/checkout/{slug}?check_in={}&check_out={}&guests={}&enquiry_id={}",
        e.check_in, e.check_out, e.guests, e.id
    )
}

async fn own_enquiry(
    cx: &Cx,
    guest: &GuestSession,
) -> Result<(common::booking_request::EnquiryResponse, String)> {
    let id: Uuid = path_param::<Id>(cx).parse().map_err(|_| not_found())?;
    let token = service_token(cx, guest)?;
    let enquiry = get_enquiry(id, &token).await.map_err(|_| not_found())?;
    if enquiry.guest_id != guest.user_id {
        return Err(not_found().into());
    }
    Ok((enquiry, token))
}

/// An enquiry and its thread, with the way on to a request to book.
#[page("/requests/enquiries/{id}")]
pub async fn enquiry_page(cx: &Cx) -> Result<impl View> {
    let guest = guest(cx).await?;
    let (enquiry, token) = own_enquiry(cx, &guest).await?;
    let details = get_api_client(cx)
        .get_listing_by_id(&enquiry.listing_id.to_string(), None)
        .await
        .map_err(|_| not_found())?;
    let messages = load_thread(enquiry.conversation_id, &token).await;
    let open = enquiry.status == EnquiryStatus::Open;
    let request_href = request_from_enquiry_href(&details.listing.slug, &enquiry);
    let action = format!("/requests/enquiries/{}/messages", enquiry.id);
    Ok(view! {
        <div class="mx-auto max-w-3xl space-y-6">
            <div class="space-y-1">
                <a href="/requests" class="text-sm">"← My requests"</a>
                <h1 class="font-serif text-[clamp(28px,4vw,40px)]">"Your question about " (details.listing.name.clone())</h1>
                <p class="text-sm text-muted-foreground">(format!("{} → {} · {} guest(s)", enquiry.check_in.format("%a %-d %b %Y"), enquiry.check_out.format("%a %-d %b %Y"), enquiry.guests))</p>
                <span id="enquiry-status" class="inline-flex rounded-full border border-border px-3 py-1 text-xs font-medium">(if open { "Open" } else { "Closed" })</span>
            </div>
            if open && details.listing.booking_mode.allows_request() {
                <a id="enquiry-request-cta" href=(request_href) class="inline-flex h-11 items-center rounded-[var(--radius-control,10px)] bg-primary px-6 text-[15px] font-medium text-primary-foreground no-underline">"Submit a request to book"</a>
            }
            conversation_panel(messages: messages, action: action)
        </div>
    })
}

/// Posts a reply to an enquiry's thread.
#[page(POST "/requests/enquiries/{id}/messages")]
pub async fn enquiry_reply(
    cx: &Cx,
    /// The reply form.
    Form(form): Form<ReplyForm>,
) -> Result<impl View> {
    let guest = guest(cx).await?;
    let (enquiry, token) = own_enquiry(cx, &guest).await?;
    let back = format!("/requests/enquiries/{}", enquiry.id);
    let sent = post_reply(
        cx,
        enquiry.conversation_id,
        &token,
        &form.message_text,
        back,
    )
    .await?;
    Ok(view! { reply_result(sent: sent) })
}

#[cfg(test)]
mod enquiry_pages {
    use super::*;
    use common::app_client::booking_requests::ApiCallError;

    fn refused(code: &str) -> anyhow::Error {
        ApiCallError::from_body(409, &format!(r#"{{"code":"{code}","message":"raw"}}"#)).into()
    }

    #[test]
    fn enquiry_pages_refusals_read_as_plain_english() {
        assert_eq!(
            friendly_error(&refused("OWN_LISTING")),
            RequestRefusal::OwnListing.message()
        );
        assert!(friendly_error(&refused("LISTING_UNAVAILABLE")).contains("no longer taking"));
        assert!(friendly_error(&anyhow::anyhow!("boom")).contains("try again"));
        assert!(!friendly_error(&refused("INVALID_STAY_DATES")).is_empty());
    }

    #[test]
    fn enquiry_pages_a_request_from_an_enquiry_keeps_its_stay() {
        let enquiry = common::booking_request::EnquiryResponse {
            id: Uuid::nil(),
            conversation_id: Uuid::nil(),
            listing_id: Uuid::nil(),
            guest_id: Uuid::nil(),
            check_in: NaiveDate::from_ymd_opt(2030, 8, 1).unwrap_or_default(),
            check_out: NaiveDate::from_ymd_opt(2030, 8, 4).unwrap_or_default(),
            guests: 6,
            currency: "USD".into(),
            status: EnquiryStatus::Open,
            closed_reason: None,
            first_response_at: None,
            created_at: chrono::Utc::now(),
            host_estimate: None,
        };
        assert_eq!(
            request_from_enquiry_href("sea-grape", &enquiry),
            format!(
                "/checkout/sea-grape?check_in=2030-08-01&check_out=2030-08-04&guests=6&enquiry_id={}",
                Uuid::nil()
            )
        );
    }
}
