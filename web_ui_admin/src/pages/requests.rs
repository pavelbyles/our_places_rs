//! The host and admin side of enquiries and requests to book (#105): the
//! inbox with response times, a request's detail with accept, decline and
//! custom quotes, and an enquiry's detail.

use common::app_client::booking_requests::{
    accept_booking_request, api_error_code, close_enquiry, decline_booking_request,
    get_booking_request, get_conversation_messages, get_enquiry, get_inbox, mark_conversation_read,
    post_conversation_message, preview_quote, quote_enquiry, send_quote,
};
use common::booking_request::{
    BookingRequestResponse, DeclineReason, DeclineRequestBody, EnquiryStatus, HostResponseMetrics,
    InboxItem, InboxQuery, QuoteRequestBody, RequestRefusal, RequestStatus,
};
use common::models::{BookingMessageResponse, MessageSenderRole};
use rust_decimal::Decimal;
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
use web_client::{get_api_client, session::AdminSession};
use web_ui_common::components::chat::{chat_bubble, chat_thread};
use web_ui_common::components::domain::price_breakdown::{PriceBreakdown, price_breakdown};

use crate::support::{service_token, staff};

path_param!(id);

/// A whole number of seconds as "2d 3h", "4h 05m" or "12m".
pub fn format_duration(secs: i64) -> String {
    let minutes = secs.max(0) / 60;
    match minutes {
        m if m < 60 => format!("{m}m"),
        m if m < 24 * 60 => format!("{}h {:02}m", m / 60, m % 60),
        m => format!("{}d {}h", m / (24 * 60), (m % (24 * 60)) / 60),
    }
}

/// What the host or admin may do with a request in `status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostAction {
    /// Accept at the requested price.
    Accept,
    /// Send (or replace) a custom quote.
    Quote,
    /// Decline, or withdraw an offer already made.
    Decline,
}

/// The actions for a request in `status`, main action first.
pub fn host_actions(status: RequestStatus) -> Vec<HostAction> {
    use HostAction::*;
    match status {
        RequestStatus::Open => vec![Accept, Quote, Decline],
        RequestStatus::Quoted => vec![Quote, Decline],
        RequestStatus::Accepted => vec![Decline],
        _ => Vec::new(),
    }
}

/// Reads a stay price as typed: digits with at most 2 decimal places and no
/// thousands separators. The API checks it again.
pub fn parse_stay_price(text: &str) -> std::result::Result<Decimal, String> {
    let text = text.trim();
    let well_formed = !text.is_empty()
        && text.chars().all(|c| c.is_ascii_digit() || c == '.')
        && text.matches('.').count() <= 1
        && text.split('.').nth(1).is_none_or(|dp| dp.len() <= 2);
    well_formed
        .then(|| text.parse::<Decimal>().ok())
        .flatten()
        .filter(|price| *price > Decimal::ZERO)
        .ok_or_else(|| {
            "Enter the stay price as a number above zero, like 1500 or 1500.50.".to_string()
        })
}

/// The message for a `?notice=CODE` left by an action.
pub fn notice_message(code: &str) -> String {
    match code {
        "accepted" => "Accepted. The guest has been told and the dates are held for them.".into(),
        "declined" => "Declined. The guest has been told and the dates are free again.".into(),
        "quoted" => "Quote sent. The guest has been told.".into(),
        "closed" => "Enquiry closed.".into(),
        "LISTING_UNAVAILABLE" => {
            "This listing is inactive, so the request can't be accepted.".into()
        }
        "DATE_UNAVAILABLE" => "Those dates have been taken in the meantime.".into(),
        "FX_RATE_UNAVAILABLE" => {
            "There's no exchange rate for the guest's currency right now.".into()
        }
        code => RequestRefusal::from_code(code)
            .map(RequestRefusal::message)
            .unwrap_or_else(|| "Something went wrong. Please try again.".into()),
    }
}

fn notice_code(code: Option<&str>) -> String {
    code.unwrap_or("error").to_string()
}

#[derive(Deserialize, Default)]
struct NoticeQuery {
    notice: Option<String>,
}

fn notice(cx: &Cx) -> Option<String> {
    parse_query_params::<NoticeQuery>(cx)
        .ok()
        .and_then(|q| q.notice)
        .map(|code| notice_message(&code))
}

// ---------------------------------------------------------------- inbox

#[component]
async fn metrics_card(metrics: Vec<HostResponseMetrics>) -> Result<impl View> {
    Ok(view! {
        <section id="inbox-metrics" aria-label="Response times" class="grid gap-3 sm:grid-cols-3">
            for m in metrics {
                <div class="rounded-xl border border-border p-4 text-sm">
                    <p class="font-semibold">(m.host_name.clone())</p>
                    <p>"Median first response: " (m.median_first_response_secs.map(format_duration).unwrap_or_else(|| "–".to_string()))</p>
                    <p class="text-muted-foreground">(format!("{} received · {} of {} requests ({}%) expired unanswered", m.received, m.expired_unanswered, m.requests, m.expired_unanswered_pct))</p>
                </div>
            }
        </section>
    })
}

#[component]
async fn inbox_row(item: InboxItem) -> Result<impl View> {
    let href = if item.kind == "request" {
        format!("/admin/requests/{}", item.id)
    } else {
        format!("/admin/enquiries/{}", item.id)
    };
    Ok(view! {
        <tr id=(format!("inbox-{}", item.id)) class="border-t border-border">
            <td class="px-3 py-2"><a href=(href)>(if item.kind == "request" { "Request" } else { "Enquiry" })</a></td>
            <td class="px-3 py-2">(item.listing_name.clone())</td>
            <td class="px-3 py-2">(item.guest_name.clone())</td>
            <td class="px-3 py-2">(format!("{} → {}", item.check_in.format("%-d %b"), item.check_out.format("%-d %b %Y")))</td>
            <td class="px-3 py-2">(item.guests.to_string())</td>
            <td class="px-3 py-2">(item.status.clone())</td>
            <td class="px-3 py-2">(format_duration(item.age_secs))</td>
            <td class="px-3 py-2">(item.first_response_secs.map(format_duration).unwrap_or_else(|| "awaiting".to_string()))</td>
            <td class="px-3 py-2">
                (item.time_left_secs.map(format_duration).unwrap_or_else(|| "–".to_string()))
                if item.near_expiry {
                    " " <span data-near-expiry="" class="badge badge-warning">"Expiring soon"</span>
                }
            </td>
        </tr>
    })
}

/// Open enquiries and live requests on the user's listings (all, for admins).
#[page("/admin/inbox")]
pub async fn inbox_page(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let token = service_token(cx, &user)?;
    let mut query = parse_query_params::<InboxQuery>(cx).unwrap_or_default();
    query.kind = query.kind.filter(|k| k == "enquiry" || k == "request");
    let page = query.page.unwrap_or(1).max(1);
    let inbox = get_inbox(&query, &token)
        .await
        .map_err(web_client::app_error)?;
    let kind = query.kind.clone().unwrap_or_default();
    let more = inbox.items.len() as u32 >= inbox.per_page;
    Ok(view! {
        <div class="space-y-6">
            <h1 class="text-2xl font-semibold">"Inbox"</h1>
            metrics_card(metrics: inbox.host_metrics.clone())
            <form method="get" action="/admin/inbox" class="flex flex-wrap items-end gap-3">
                <label class="text-sm">"Show"
                    <select name="kind" class="mt-1 block h-10 rounded-lg border border-border bg-card px-3 text-sm">
                        <option value="" selected=(kind.is_empty())>"Everything"</option>
                        <option value="request" selected=(kind == "request")>"Requests"</option>
                        <option value="enquiry" selected=(kind == "enquiry")>"Enquiries"</option>
                    </select>
                </label>
                <button type="submit" class="h-10 rounded-lg border border-border px-4 text-sm">"Filter"</button>
            </form>
            if inbox.items.is_empty() {
                <p id="inbox-empty" class="text-sm text-muted-foreground">"Nothing waiting for a response."</p>
            } else {
                <div class="overflow-x-auto rounded-xl border border-border">
                    <table id="inbox-table" class="w-full text-left text-sm">
                        <thead class="bg-muted/40 text-xs uppercase">
                            <tr>
                                <th class="px-3 py-2">"Type"</th><th class="px-3 py-2">"Listing"</th><th class="px-3 py-2">"Guest"</th>
                                <th class="px-3 py-2">"Dates"</th><th class="px-3 py-2">"Guests"</th><th class="px-3 py-2">"Status"</th>
                                <th class="px-3 py-2">"Age"</th><th class="px-3 py-2">"First response"</th><th class="px-3 py-2">"Time left"</th>
                            </tr>
                        </thead>
                        <tbody>
                            for item in inbox.items {
                                inbox_row(item: item)
                            }
                        </tbody>
                    </table>
                </div>
            }
            <nav aria-label="Pages" class="flex gap-3 text-sm">
                if page > 1 {
                    <a href=(format!("/admin/inbox?kind={kind}&page={}", page - 1))>"← Newer"</a>
                }
                if more {
                    <a href=(format!("/admin/inbox?kind={kind}&page={}", page + 1))>"Older →"</a>
                }
            </nav>
        </div>
    })
}

// --------------------------------------------------------------- thread

#[component]
async fn host_bubble(message: BookingMessageResponse) -> Result<impl View> {
    let mine = message.sender_role != MessageSenderRole::Guest;
    let sent_at = message.created_at.format("%b %d, %H:%M").to_string();
    Ok(view! {
        chat_bubble(sender: message.sender_name.clone(), sent_at: sent_at, text: message.message_text.clone(), mine: mine, attrs: topcoat::view::attributes! { id=(format!("msg-{}", message.id)) })
    })
}

#[component]
async fn host_thread(messages: Vec<BookingMessageResponse>, action: String) -> Result<impl View> {
    Ok(view! {
        <section aria-label="Messages" class="space-y-3">
            <h2 class="text-lg font-semibold">"Messages"</h2>
            chat_thread(attrs: topcoat::view::attributes! { id="admin-messages-list" class="rounded-xl border border-border p-4" },
                if messages.is_empty() {
                    <p class="text-sm text-muted-foreground">"No messages yet."</p>
                }
                for message in messages {
                    host_bubble(message: message)
                }
            )
            <form method="post" action=(action.clone()) hx-post=(action) hx-target="#admin-messages-list" hx-swap="beforeend" data-reset-on-success="" class="flex gap-2">
                <label for="admin-message-text" class="sr-only">"Message"</label>
                <input id="admin-message-text" name="message_text" required=(true) maxlength="2000" autocomplete="off" placeholder="Reply to the guest…" class="h-10 flex-1 rounded-lg border border-border bg-card px-3 text-sm" />
                <button type="submit" class="h-10 rounded-lg bg-primary px-5 text-sm font-medium text-primary-foreground">"Send"</button>
            </form>
        </section>
    })
}

async fn load_thread(conversation_id: Uuid, token: &str) -> Vec<BookingMessageResponse> {
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

#[component]
async fn reply_result(
    sent: std::result::Result<BookingMessageResponse, String>,
) -> Result<impl View> {
    Ok(view! {
        match sent {
            Ok(message) => { host_bubble(message: message) }
            Err(error) => { <p role="alert" class="text-sm text-destructive">(error)</p> }
        }
    })
}

async fn send_reply(
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
            .map_err(|e| notice_message(&notice_code(api_error_code(&e))))
    };
    if !topcoat::htmx::hx_request(cx) {
        return Err(see_other(back).into());
    }
    Ok(sent)
}

// -------------------------------------------------------------- request

async fn managed_request(cx: &Cx, user: &AdminSession) -> Result<(BookingRequestResponse, String)> {
    let id: Uuid = path_param::<Id>(cx).parse().map_err(|_| not_found())?;
    let token = service_token(cx, user)?;
    let request = get_booking_request(id, &token)
        .await
        .map_err(|_| not_found())?;
    if request.guest_id == user.user_id {
        return Err(not_found().into());
    }
    Ok((request, token))
}

/// The custom quote form, shared by requests and enquiries. `preview` is
/// where the HTMX preview posts, if there is one.
#[component]
async fn quote_form(
    action: String,
    preview: Option<String>,
    base_currency: String,
) -> Result<impl View> {
    Ok(view! {
        <form id="quote-form" method="post" action=(action) class="space-y-3 rounded-xl border border-border p-4">
            <h3 class="font-semibold">"Send a custom quote"</h3>
            <label class="block text-sm">(format!("Stay price for the whole stay ({base_currency}), before fees and GCT"))
                <input id="quote-stay-price" name="stay_price" type="text" inputmode="decimal" required=(true) autocomplete="off" class="mt-1 block h-10 w-full rounded-lg border border-border bg-card px-3 text-sm" />
            </label>
            <label class="block text-sm">"Special terms (shown to the guest)"
                <textarea id="quote-terms" name="terms" rows="3" maxlength="2000" class="mt-1 block w-full rounded-lg border border-border bg-card px-3 py-2 text-sm"></textarea>
            </label>
            if let Some(preview) = preview {
                <button id="quote-preview" type="button" hx-post=(preview) hx-include="#quote-form" hx-target="#quote-preview-result" hx-swap="innerHTML" class="h-10 rounded-lg border border-border px-4 text-sm">"Preview guest total"</button>
                <div id="quote-preview-result" aria-live="polite"></div>
            }
            <p class="text-xs text-muted-foreground">"A custom quote replaces any promo code the guest used."</p>
            <button id="quote-send" type="submit" class="h-10 rounded-lg bg-primary px-5 text-sm font-medium text-primary-foreground">"Send quote"</button>
        </form>
    })
}

/// A request with its price, quote, actions and thread.
#[page("/admin/requests/{id}")]
pub async fn request_page(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let (request, token) = managed_request(cx, &user).await?;
    let details = get_api_client(cx)
        .get_listing_by_id(&request.listing_id.to_string(), None)
        .await
        .map_err(|_| not_found())?;
    let messages = load_thread(request.conversation_id, &token).await;
    let notice = notice(cx);
    let id = request.id;
    let actions = host_actions(request.status);
    let requested = PriceBreakdown::from(&request.requested_price);
    let quote = request.quote.clone();
    let expires = request.status.is_live().then(|| {
        request
            .expires_at
            .format("%a %-d %b, %H:%M UTC")
            .to_string()
    });
    Ok(view! {
        <div class="mx-auto max-w-4xl space-y-6">
            <div class="space-y-1">
                <a href="/admin/inbox" class="text-sm">"← Inbox"</a>
                <h1 class="text-2xl font-semibold">(format!("Request · {}", details.listing.name))</h1>
                <p class="text-sm text-muted-foreground">(format!("{} → {} · {} guest(s)", request.check_in.format("%a %-d %b %Y"), request.check_out.format("%a %-d %b %Y"), request.guests))</p>
                <span id="request-status" data-request-status=(request.status.as_str()) class="badge">(request.status.label())</span>
                if let Some(expires) = expires {
                    <p id="request-expiry" class="text-sm">"Held until " (expires)</p>
                }
            </div>
            if let Some(notice) = notice {
                <div id="request-notice" role="status" class="rounded-lg border border-border p-4 text-sm">(notice)</div>
            }
            <div class="grid gap-6 md:grid-cols-2">
                <section class="space-y-2 rounded-xl border border-border p-4">
                    <h2 class="font-semibold">"Requested price"</h2>
                    price_breakdown(quote: requested)
                    if request.promo_applied {
                        <p class="text-xs text-muted-foreground">"Includes the guest's promo code."</p>
                    }
                </section>
                if let Some(q) = quote {
                    <section id="request-quote" class="space-y-2 rounded-xl border border-border p-4">
                        <h2 class="font-semibold">"Your quote"</h2>
                        price_breakdown(quote: PriceBreakdown::from(&q.price))
                        if !q.terms.is_empty() {
                            <p class="whitespace-pre-line text-sm">(q.terms.clone())</p>
                        }
                    </section>
                }
            </div>
            if let Some(reason) = request.decline_reason {
                <p id="request-decline" class="text-sm">"Declined: " (reason.label()) (request.decline_note.clone().map(|n| format!(" — {n}")).unwrap_or_default())</p>
            }
            <div class="grid gap-4 md:grid-cols-2">
                for action in actions {
                    match action {
                        HostAction::Accept => {
                            <form method="post" action=(format!("/admin/requests/{id}/accept"))>
                                <button id="request-accept" type="submit" class="h-10 rounded-lg bg-primary px-5 text-sm font-medium text-primary-foreground">"Accept at this price"</button>
                            </form>
                        }
                        HostAction::Quote => {
                            quote_form(action: format!("/admin/requests/{id}/quote"), preview: Some(format!("/admin/requests/{id}/quote-preview")), base_currency: details.listing.base_currency.clone())
                        }
                        HostAction::Decline => {
                            <form id="decline-form" method="post" action=(format!("/admin/requests/{id}/decline")) class="space-y-3 rounded-xl border border-border p-4">
                                <h3 class="font-semibold">(if request.status == RequestStatus::Open { "Decline" } else { "Withdraw the offer" })</h3>
                                <label class="block text-sm">"Reason"
                                    <select id="decline-reason" name="reason" class="mt-1 block h-10 w-full rounded-lg border border-border bg-card px-3 text-sm">
                                        for reason in DeclineReason::ALL {
                                            <option value=(reason.as_str())>(reason.label())</option>
                                        }
                                    </select>
                                </label>
                                <label class="block text-sm">"Note to the guest (required for Other)"
                                    <textarea id="decline-note" name="note" rows="2" maxlength="1000" class="mt-1 block w-full rounded-lg border border-border bg-card px-3 py-2 text-sm"></textarea>
                                </label>
                                <button id="request-decline-submit" type="submit" class="h-10 rounded-lg border border-destructive px-5 text-sm font-medium text-destructive">"Decline"</button>
                            </form>
                        }
                    }
                }
            </div>
            host_thread(messages: messages, action: format!("/admin/requests/{id}/messages"))
        </div>
    })
}

/// Accepts a request.
#[page(POST "/admin/requests/{id}/accept")]
pub async fn accept_submit(cx: &Cx) -> Result<()> {
    let user = staff(cx).await?;
    let (request, token) = managed_request(cx, &user).await?;
    let code = match accept_booking_request(request.id, &token).await {
        Ok(_) => "accepted".to_string(),
        Err(e) => notice_code(api_error_code(&e)),
    };
    Err(see_other(format!("/admin/requests/{}?notice={code}", request.id)).into())
}

/// Decline form fields.
#[derive(Deserialize)]
pub struct DeclineForm {
    /// Reason, as stored.
    pub reason: String,
    /// Note to the guest.
    #[serde(default)]
    pub note: String,
}

/// Declines a request, or withdraws an offer.
#[page(POST "/admin/requests/{id}/decline")]
pub async fn decline_submit(
    cx: &Cx,
    /// The decline form.
    Form(form): Form<DeclineForm>,
) -> Result<()> {
    let user = staff(cx).await?;
    let (request, token) = managed_request(cx, &user).await?;
    let back = |code: &str| see_other(format!("/admin/requests/{}?notice={code}", request.id));
    let Some(reason) = DeclineReason::parse(&form.reason) else {
        return Err(back("error").into());
    };
    let body = DeclineRequestBody {
        reason,
        note: Some(form.note.trim().to_string()).filter(|n| !n.is_empty()),
    };
    let code = match decline_booking_request(request.id, &body, &token).await {
        Ok(_) => "declined".to_string(),
        Err(e) => notice_code(api_error_code(&e)),
    };
    Err(back(&code).into())
}

/// Quote form fields.
#[derive(Deserialize)]
pub struct QuoteForm {
    /// Stay price as typed, in the listing's base currency.
    pub stay_price: String,
    /// Special terms.
    #[serde(default)]
    pub terms: String,
}

fn quote_body(form: &QuoteForm) -> std::result::Result<QuoteRequestBody, String> {
    parse_stay_price(&form.stay_price).map(|stay_price| QuoteRequestBody {
        stay_price,
        terms: form.terms.trim().to_string(),
        cancellation_policy: None,
    })
}

/// Sends a custom quote on a request.
#[page(POST "/admin/requests/{id}/quote")]
pub async fn quote_submit(
    cx: &Cx,
    /// The quote form.
    Form(form): Form<QuoteForm>,
) -> Result<()> {
    let user = staff(cx).await?;
    let (request, token) = managed_request(cx, &user).await?;
    let code = match quote_body(&form) {
        Err(_) => RequestRefusal::InvalidQuotePrice.code().to_string(),
        Ok(body) => match send_quote(request.id, &body, &token).await {
            Ok(_) => "quoted".to_string(),
            Err(e) => notice_code(api_error_code(&e)),
        },
    };
    Err(see_other(format!("/admin/requests/{}?notice={code}", request.id)).into())
}

#[component]
async fn preview_result(outcome: std::result::Result<PriceBreakdown, String>) -> Result<impl View> {
    Ok(view! {
        match outcome {
            Ok(price) => { <div id="quote-preview-breakdown">price_breakdown(quote: price)</div> }
            Err(error) => { <p role="alert" class="text-sm text-destructive">(error)</p> }
        }
    })
}

/// The guest's total for a custom quote, without sending it (HTMX fragment).
#[page(POST "/admin/requests/{id}/quote-preview")]
pub async fn quote_preview(
    cx: &Cx,
    /// The quote form.
    Form(form): Form<QuoteForm>,
) -> Result<impl View> {
    let user = staff(cx).await?;
    let (request, token) = managed_request(cx, &user).await?;
    let outcome = match quote_body(&form) {
        Err(message) => Err(message),
        Ok(body) => preview_quote(request.id, &body, &token)
            .await
            .map(|price| PriceBreakdown::from(&price))
            .map_err(|e| notice_message(&notice_code(api_error_code(&e)))),
    };
    Ok(view! { preview_result(outcome: outcome) })
}

/// Replies on a request's thread.
#[page(POST "/admin/requests/{id}/messages")]
pub async fn request_reply(
    cx: &Cx,
    /// The reply form.
    Form(form): Form<ReplyForm>,
) -> Result<impl View> {
    let user = staff(cx).await?;
    let (request, token) = managed_request(cx, &user).await?;
    let back = format!("/admin/requests/{}", request.id);
    let sent = send_reply(
        cx,
        request.conversation_id,
        &token,
        &form.message_text,
        back,
    )
    .await?;
    Ok(view! { reply_result(sent: sent) })
}

// -------------------------------------------------------------- enquiry

async fn managed_enquiry(
    cx: &Cx,
    user: &AdminSession,
) -> Result<(common::booking_request::EnquiryResponse, String)> {
    let id: Uuid = path_param::<Id>(cx).parse().map_err(|_| not_found())?;
    let token = service_token(cx, user)?;
    let enquiry = get_enquiry(id, &token).await.map_err(|_| not_found())?;
    if enquiry.guest_id == user.user_id {
        return Err(not_found().into());
    }
    Ok((enquiry, token))
}

/// An enquiry with its thread, a quote form and Close.
#[page("/admin/enquiries/{id}")]
pub async fn enquiry_page(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let (enquiry, token) = managed_enquiry(cx, &user).await?;
    let details = get_api_client(cx)
        .get_listing_by_id(&enquiry.listing_id.to_string(), None)
        .await
        .map_err(|_| not_found())?;
    let messages = load_thread(enquiry.conversation_id, &token).await;
    let notice = notice(cx);
    let open = enquiry.status == EnquiryStatus::Open;
    let id = enquiry.id;
    Ok(view! {
        <div class="mx-auto max-w-4xl space-y-6">
            <div class="space-y-1">
                <a href="/admin/inbox" class="text-sm">"← Inbox"</a>
                <h1 class="text-2xl font-semibold">(format!("Enquiry · {}", details.listing.name))</h1>
                <p class="text-sm text-muted-foreground">(format!("{} → {} · {} guest(s) · {}", enquiry.check_in.format("%a %-d %b %Y"), enquiry.check_out.format("%a %-d %b %Y"), enquiry.guests, enquiry.currency))</p>
                <span id="enquiry-status" class="badge">(if open { "Open" } else { "Closed" })</span>
            </div>
            if let Some(notice) = notice {
                <div id="enquiry-notice" role="status" class="rounded-lg border border-border p-4 text-sm">(notice)</div>
            }
            if open {
                <div class="grid gap-4 md:grid-cols-2">
                    quote_form(action: format!("/admin/enquiries/{id}/quote"), preview: None, base_currency: details.listing.base_currency.clone())
                    <form method="post" action=(format!("/admin/enquiries/{id}/close"))>
                        <button id="enquiry-close" type="submit" class="h-10 rounded-lg border border-border px-5 text-sm">"Close enquiry"</button>
                    </form>
                </div>
            }
            host_thread(messages: messages, action: format!("/admin/enquiries/{id}/messages"))
        </div>
    })
}

/// Quotes straight on an enquiry; on success, shows the request it became.
#[page(POST "/admin/enquiries/{id}/quote")]
pub async fn enquiry_quote_submit(
    cx: &Cx,
    /// The quote form.
    Form(form): Form<QuoteForm>,
) -> Result<()> {
    let user = staff(cx).await?;
    let (enquiry, token) = managed_enquiry(cx, &user).await?;
    let back = |code: &str| see_other(format!("/admin/enquiries/{}?notice={code}", enquiry.id));
    let body = match quote_body(&form) {
        Ok(body) => body,
        Err(_) => return Err(back(RequestRefusal::InvalidQuotePrice.code()).into()),
    };
    match quote_enquiry(enquiry.id, &body, &token).await {
        Ok(request) => {
            Err(see_other(format!("/admin/requests/{}?notice=quoted", request.id)).into())
        }
        Err(e) => Err(back(&notice_code(api_error_code(&e))).into()),
    }
}

/// Closes an enquiry.
#[page(POST "/admin/enquiries/{id}/close")]
pub async fn enquiry_close_submit(cx: &Cx) -> Result<()> {
    let user = staff(cx).await?;
    let (enquiry, token) = managed_enquiry(cx, &user).await?;
    let code = match close_enquiry(enquiry.id, &token).await {
        Ok(_) => "closed".to_string(),
        Err(e) => notice_code(api_error_code(&e)),
    };
    Err(see_other(format!("/admin/enquiries/{}?notice={code}", enquiry.id)).into())
}

/// Replies on an enquiry's thread.
#[page(POST "/admin/enquiries/{id}/messages")]
pub async fn enquiry_reply(
    cx: &Cx,
    /// The reply form.
    Form(form): Form<ReplyForm>,
) -> Result<impl View> {
    let user = staff(cx).await?;
    let (enquiry, token) = managed_enquiry(cx, &user).await?;
    let back = format!("/admin/enquiries/{}", enquiry.id);
    let sent = send_reply(
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
mod admin_request_pages {
    use super::*;

    #[test]
    fn inbox_page_durations_read_naturally() {
        assert_eq!(format_duration(-5), "0m");
        assert_eq!(format_duration(12 * 60), "12m");
        assert_eq!(format_duration(4 * 3600 + 5 * 60), "4h 05m");
        assert_eq!(format_duration(2 * 86400 + 3 * 3600), "2d 3h");
    }

    #[test]
    fn request_detail_page_actions_follow_the_status() {
        use HostAction::*;
        assert_eq!(host_actions(RequestStatus::Open), [Accept, Quote, Decline]);
        assert_eq!(host_actions(RequestStatus::Quoted), [Quote, Decline]);
        assert_eq!(host_actions(RequestStatus::Accepted), [Decline]);
        for terminal in [
            RequestStatus::Declined,
            RequestStatus::Expired,
            RequestStatus::Withdrawn,
            RequestStatus::Cancelled,
            RequestStatus::Converted,
        ] {
            assert!(host_actions(terminal).is_empty(), "{terminal:?}");
        }
    }

    #[test]
    fn request_detail_page_malformed_prices_are_refused() {
        assert_eq!(parse_stay_price(" 1500.50 "), Ok(Decimal::new(150050, 2)));
        assert_eq!(parse_stay_price("1500"), Ok(Decimal::new(1500, 0)));
        for bad in ["1,000", "abc", "1.234", "", "0", "-5", "1.2.3"] {
            assert!(parse_stay_price(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn enquiry_detail_page_a_taken_date_quote_reads_plainly() {
        assert!(notice_message("DATE_UNAVAILABLE").contains("taken"));
        assert_eq!(
            notice_message("REQUEST_ALREADY_LIVE"),
            RequestRefusal::RequestAlreadyLive.message()
        );
        assert!(notice_message("quoted").contains("Quote sent"));
    }
}
