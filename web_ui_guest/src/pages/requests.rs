//! My requests (#105): the guest's requests to book and enquiries, a
//! request's detail with its price, quote and thread, and the guest's
//! actions on it (proceed to checkout, accept a quote, withdraw).

use chrono::{DateTime, Utc};
use common::app_client::booking_requests::{
    api_error_code, checkout_booking_request, get_booking_request, my_booking_requests,
    withdraw_booking_request,
};
use common::booking_request::{
    BookingRequestList, BookingRequestResponse, EnquiryResponse, EnquiryStatus, PriceBreakdown,
    RequestRefusal, RequestStatus,
};
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
use web_ui_common::components::domain::price_breakdown::{
    PriceBreakdown as BreakdownView, price_breakdown,
};

use super::enquiries::{
    ReplyForm, conversation_panel, friendly_error, load_thread, post_reply, reply_result,
};
use crate::support::{guest, service_token};

path_param!(id);

/// What the guest may do with a request in `status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestAction {
    /// Turn an accepted request into a hold and pay.
    ProceedToCheckout,
    /// Accept the host's quote and pay.
    AcceptQuote,
    /// Withdraw the request or offer.
    Withdraw,
    /// Open the booking it became.
    ViewBookings,
}

/// The guest's actions for a request in `status`, main action first.
pub fn guest_actions(status: RequestStatus) -> Vec<GuestAction> {
    use GuestAction::*;
    match status {
        RequestStatus::Open => vec![Withdraw],
        RequestStatus::Accepted => vec![ProceedToCheckout, Withdraw],
        RequestStatus::Quoted => vec![AcceptQuote, Withdraw],
        RequestStatus::Converted => vec![ViewBookings],
        RequestStatus::Declined
        | RequestStatus::Expired
        | RequestStatus::Withdrawn
        | RequestStatus::Cancelled => Vec::new(),
    }
}

/// "in 3h 20m", "in 45m", or "now" for a time that has passed.
pub fn time_left(until: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let minutes = (until - now).num_minutes();
    match minutes {
        m if m <= 0 => "now".to_string(),
        m if m < 60 => format!("in {m}m"),
        m if m < 48 * 60 => format!("in {}h {}m", m / 60, m % 60),
        m => format!("in {} days", m / (24 * 60)),
    }
}

/// The message for a `?notice=CODE` left by a refused action.
pub fn notice_message(code: &str) -> String {
    RequestRefusal::from_code(code)
        .map(RequestRefusal::message)
        .unwrap_or_else(|| match code {
            "LISTING_UNAVAILABLE" => "This property is no longer taking bookings.".to_string(),
            "DATE_UNAVAILABLE" => "Those dates are no longer available.".to_string(),
            "CURRENCY_INACTIVE" => {
                "This currency is no longer offered. Please send a new request.".to_string()
            }
            "withdrawn" => "Your request was withdrawn and the dates were released.".to_string(),
            _ => "Something went wrong. Please try again.".to_string(),
        })
}

/// The breakdown component's view of an API price.
pub fn breakdown_view(price: &PriceBreakdown) -> BreakdownView {
    BreakdownView::from(price)
}

/// The price the guest would pay now: the live quote's, else as requested.
fn current_price(r: &BookingRequestResponse) -> &PriceBreakdown {
    r.quote.as_ref().map_or(&r.requested_price, |q| &q.price)
}

#[component]
async fn request_card(
    cx: &Cx,
    request: BookingRequestResponse,
    listing_name: String,
) -> Result<impl View> {
    let price = current_price(&request);
    let total =
        web_ui_common::components::domain::price_breakdown::money(&price.currency, price.total);
    let expires = request
        .status
        .is_live()
        .then(|| time_left(request.expires_at, Utc::now()));
    Ok(view! {
        <article id=(format!("request-{}", request.id)) class="space-y-2 rounded-[var(--radius-surface,16px)] border border-border bg-card p-5">
            <div class="flex items-center justify-between gap-3">
                <h2 class="font-serif text-xl">(listing_name)</h2>
                <span data-request-status=(request.status.as_str()) class="inline-flex rounded-full border border-border px-3 py-1 text-xs font-medium">(request.status.label())</span>
            </div>
            <p class="text-sm text-muted-foreground">(format!("{} → {} · {} guest(s) · {}", request.check_in.format("%a %-d %b %Y"), request.check_out.format("%a %-d %b %Y"), request.guests, total))</p>
            if let Some(expires) = expires {
                <p class="text-sm">"Expires " (expires)</p>
            }
            <a href=(format!("/requests/{}", request.id)) class="text-sm font-medium">"View request"</a>
        </article>
    })
}

#[component]
async fn enquiry_card(
    cx: &Cx,
    enquiry: EnquiryResponse,
    listing_name: String,
) -> Result<impl View> {
    let open = enquiry.status == EnquiryStatus::Open;
    Ok(view! {
        <article id=(format!("enquiry-{}", enquiry.id)) class="space-y-2 rounded-[var(--radius-surface,16px)] border border-border bg-card p-5">
            <div class="flex items-center justify-between gap-3">
                <h2 class="font-serif text-xl">"Question: " (listing_name)</h2>
                <span class="inline-flex rounded-full border border-border px-3 py-1 text-xs font-medium">(if open { "Open" } else { "Closed" })</span>
            </div>
            <p class="text-sm text-muted-foreground">(format!("{} → {} · {} guest(s)", enquiry.check_in.format("%a %-d %b %Y"), enquiry.check_out.format("%a %-d %b %Y"), enquiry.guests))</p>
            <a href=(format!("/requests/enquiries/{}", enquiry.id)) class="text-sm font-medium">"View conversation"</a>
        </article>
    })
}

/// Listing names for the cards, looked up once per listing.
async fn listing_names(
    cx: &Cx,
    list: &BookingRequestList,
) -> std::collections::HashMap<Uuid, String> {
    let mut ids: Vec<Uuid> = list
        .requests
        .iter()
        .map(|r| r.listing_id)
        .chain(list.enquiries.iter().map(|e| e.listing_id))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let mut names = std::collections::HashMap::new();
    for id in ids {
        if let Ok(details) = get_api_client(cx)
            .get_listing_by_id(&id.to_string(), None)
            .await
        {
            names.insert(id, details.listing.name);
        }
    }
    names
}

#[component]
async fn requests_list(
    cx: &Cx,
    list: BookingRequestList,
    names: std::collections::HashMap<Uuid, String>,
) -> Result<impl View> {
    let name_of = |id: &Uuid| {
        names
            .get(id)
            .cloned()
            .unwrap_or_else(|| "A property".to_string())
    };
    let empty = list.requests.is_empty() && list.enquiries.is_empty();
    let requests: Vec<(BookingRequestResponse, String)> = list
        .requests
        .into_iter()
        .map(|r| {
            let name = name_of(&r.listing_id);
            (r, name)
        })
        .collect();
    let enquiries: Vec<(EnquiryResponse, String)> = list
        .enquiries
        .into_iter()
        .map(|e| {
            let name = name_of(&e.listing_id);
            (e, name)
        })
        .collect();
    Ok(view! {
        <div id="requests-list" hx-get="/requests/list" hx-trigger="every 60s" hx-swap="outerHTML" class="space-y-4">
            if empty {
                <p class="text-sm text-muted-foreground">"You haven't sent any requests or questions yet."</p>
            }
            for (request, listing_name) in requests {
                request_card(listing_name: listing_name, request: request)
            }
            for (enquiry, listing_name) in enquiries {
                enquiry_card(listing_name: listing_name, enquiry: enquiry)
            }
        </div>
    })
}

async fn load_list(
    cx: &Cx,
    guest: &GuestSession,
) -> Result<(BookingRequestList, std::collections::HashMap<Uuid, String>)> {
    let token = service_token(cx, guest)?;
    let list = my_booking_requests(&token)
        .await
        .map_err(web_client::app_error)?;
    let names = listing_names(cx, &list).await;
    Ok((list, names))
}

/// The guest's requests and enquiries.
#[page("/requests")]
pub async fn requests_page(cx: &Cx) -> Result<impl View> {
    let guest = guest(cx).await?;
    let (list, names) = load_list(cx, &guest).await?;
    Ok(view! {
        <div class="mx-auto max-w-3xl space-y-6">
            <h1 class="font-serif text-[clamp(32px,4vw,48px)]">"My requests"</h1>
            requests_list(list: list, names: names)
        </div>
    })
}

/// The list fragment, refreshed every minute so countdowns stay current.
#[page("/requests/list")]
pub async fn requests_list_fragment(cx: &Cx) -> Result<impl View> {
    let guest = guest(cx).await?;
    let (list, names) = load_list(cx, &guest).await?;
    Ok(view! { requests_list(list: list, names: names) })
}

async fn own_request(cx: &Cx, guest: &GuestSession) -> Result<(BookingRequestResponse, String)> {
    let id: Uuid = id(cx).parse().map_err(|_| not_found())?;
    let token = service_token(cx, guest)?;
    let request = get_booking_request(id, &token)
        .await
        .map_err(|_| not_found())?;
    if request.guest_id != guest.user_id {
        return Err(not_found().into());
    }
    Ok((request, token))
}

#[derive(Deserialize, Default)]
struct NoticeQuery {
    notice: Option<String>,
}

/// A request: status, price or quote, the host's decline, actions and thread.
#[page("/requests/{id}")]
pub async fn request_page(cx: &Cx) -> Result<impl View> {
    let guest = guest(cx).await?;
    let (request, token) = own_request(cx, &guest).await?;
    let listing_name = get_api_client(cx)
        .get_listing_by_id(&request.listing_id.to_string(), None)
        .await
        .map(|d| d.listing.name)
        .unwrap_or_else(|_| "A property".to_string());
    let messages = load_thread(request.conversation_id, &token).await;
    let notice = parse_query_params::<NoticeQuery>(cx)
        .ok()
        .and_then(|q| q.notice)
        .map(|code| notice_message(&code));
    let actions = guest_actions(request.status);
    let price = breakdown_view(current_price(&request));
    let promo_dropped = request.promo_applied && request.quote.is_some();
    let id = request.id;
    let reply_action = format!("/requests/{id}/messages");
    let quote_id = request
        .quote
        .as_ref()
        .map(|q| q.id.to_string())
        .unwrap_or_default();
    let expires = request.status.is_live().then(|| {
        (
            request
                .expires_at
                .format("%a %-d %b, %H:%M UTC")
                .to_string(),
            time_left(request.expires_at, Utc::now()),
        )
    });
    Ok(view! {
        <div class="mx-auto max-w-3xl space-y-6">
            <div class="space-y-1">
                <a href="/requests" class="text-sm">"← My requests"</a>
                <h1 class="font-serif text-[clamp(28px,4vw,40px)]">(listing_name)</h1>
                <p class="text-sm text-muted-foreground">(format!("{} → {} · {} guest(s)", request.check_in.format("%a %-d %b %Y"), request.check_out.format("%a %-d %b %Y"), request.guests))</p>
                <span id="request-status" data-request-status=(request.status.as_str()) class="inline-flex rounded-full border border-border px-3 py-1 text-xs font-medium">(request.status.label())</span>
            </div>
            if let Some(notice) = notice {
                <div id="request-notice" role="alert" class="rounded-lg border border-border p-4 text-sm">(notice)</div>
            }
            if let Some((at, left)) = expires {
                <p id="request-expiry" class="text-sm">(match request.status { RequestStatus::Open => "The host has until ", _ => "This offer is held for you until " }) (at) " (" (left) ")."</p>
            }
            if let Some(reason) = request.decline_reason {
                <div id="request-decline" class="space-y-1 rounded-lg border border-border p-4 text-sm">
                    <p class="font-medium">"The host declined: " (reason.label())</p>
                    if let Some(note) = request.decline_note.clone() {
                        <p>(note)</p>
                    }
                </div>
            }
            if let Some(quote) = request.quote.clone() {
                <section id="request-quote" class="space-y-2 rounded-lg border border-border p-4">
                    <h2 class="font-serif text-xl">"The host's quote"</h2>
                    if !quote.terms.is_empty() {
                        <p id="request-quote-terms" class="whitespace-pre-line text-sm">(quote.terms.clone())</p>
                    }
                    if promo_dropped {
                        <p id="request-promo-dropped" class="text-sm text-muted-foreground">"Your promo code doesn't apply to a custom quote. The quoted price is final."</p>
                    }
                </section>
            }
            <div class="rounded-[var(--radius-surface,16px)] border border-border bg-card p-6">
                price_breakdown(quote: price)
            </div>
            <div class="flex flex-wrap gap-3">
                for action in actions {
                    match action {
                        GuestAction::ProceedToCheckout => {
                            <form method="post" action=(format!("/requests/{id}/checkout"))>
                                <button id="request-checkout" type="submit" class="h-11 rounded-[var(--radius-control,10px)] bg-primary px-6 text-[15px] font-medium text-primary-foreground">"Proceed to checkout"</button>
                            </form>
                        }
                        GuestAction::AcceptQuote => {
                            <form method="post" action=(format!("/requests/{id}/checkout"))>
                                <input type="hidden" name="quote_id" value=(quote_id.clone()) />
                                <button id="request-accept-quote" type="submit" class="h-11 rounded-[var(--radius-control,10px)] bg-primary px-6 text-[15px] font-medium text-primary-foreground">"Accept quote and pay"</button>
                            </form>
                        }
                        GuestAction::Withdraw => {
                            <button id="request-withdraw" type="button" hx-get=(format!("/requests/{id}/withdraw")) hx-target="#withdraw-dialog-slot" hx-swap="innerHTML" class="h-11 rounded-[var(--radius-control,10px)] border border-destructive px-6 text-[15px] font-medium text-destructive">"Withdraw request"</button>
                        }
                        GuestAction::ViewBookings => {
                            <a id="request-view-booking" href="/bookings" class="inline-flex h-11 items-center rounded-[var(--radius-control,10px)] bg-primary px-6 text-[15px] font-medium text-primary-foreground no-underline">"View your booking"</a>
                        }
                    }
                }
            </div>
            <div id="withdraw-dialog-slot"></div>
            conversation_panel(messages: messages, action: reply_action)
        </div>
    })
}

/// The withdraw confirmation, opened by `ui.js` (`data-auto-open`).
#[page("/requests/{id}/withdraw")]
pub async fn withdraw_dialog(cx: &Cx) -> Result<impl View> {
    let guest = guest(cx).await?;
    let (request, _) = own_request(cx, &guest).await?;
    let action = format!("/requests/{}/withdraw", request.id);
    Ok(view! {
        <dialog id="withdraw-request-modal" data-auto-open="" aria-labelledby="withdraw-title" class="m-auto w-[min(480px,92vw)] rounded-[var(--radius-surface,16px)] bg-card p-6 text-card-foreground backdrop:bg-black/50">
            <div class="space-y-4">
                <h2 id="withdraw-title" class="font-serif text-2xl">"Withdraw this request?"</h2>
                <p class="text-sm">"The dates are released straight away, and the host is told."</p>
                <form method="post" action=(action) class="flex justify-end gap-2">
                    <button type="button" data-dialog-close="" class="h-10 rounded-full border border-border px-4 text-sm">"Keep request"</button>
                    <button id="request-withdraw-confirm" type="submit" class="h-10 rounded-full bg-destructive px-4 text-sm font-medium text-destructive-foreground">"Yes, withdraw"</button>
                </form>
            </div>
        </dialog>
    })
}

/// Withdraws the request, then shows it again with a notice.
#[page(POST "/requests/{id}/withdraw")]
pub async fn withdraw_submit(cx: &Cx) -> Result<()> {
    let guest = guest(cx).await?;
    let (request, token) = own_request(cx, &guest).await?;
    let notice = match withdraw_booking_request(request.id, &token).await {
        Ok(_) => "withdrawn".to_string(),
        Err(e) => api_error_code(&e).unwrap_or("error").to_string(),
    };
    Err(see_other(format!("/requests/{}?notice={notice}", request.id)).into())
}

/// The accept-quote form.
#[derive(Deserialize, Default)]
pub struct CheckoutForm {
    /// The quote the guest saw; empty for an accepted request.
    #[serde(default)]
    pub quote_id: String,
}

/// Turns the offer into a hold and continues to payment, or explains why not.
#[page(POST "/requests/{id}/checkout")]
pub async fn checkout_submit(
    cx: &Cx,
    /// The accept form.
    Form(form): Form<CheckoutForm>,
) -> Result<()> {
    let guest = guest(cx).await?;
    let (request, token) = own_request(cx, &guest).await?;
    let quote_id = form.quote_id.parse::<Uuid>().ok();
    match checkout_booking_request(request.id, quote_id, &token).await {
        Ok(booking) => {
            tracing::info!(request_id = %request.id, booking_id = %booking.id, "offer taken up");
            Err(see_other("/bookings?new_booking=true").into())
        }
        Err(e) => {
            let code = api_error_code(&e).unwrap_or("error");
            tracing::info!(request_id = %request.id, code, "checkout refused: {}", friendly_error(&e));
            Err(see_other(format!("/requests/{}?notice={code}", request.id)).into())
        }
    }
}

/// Posts a reply to the request's thread.
#[page(POST "/requests/{id}/messages")]
pub async fn request_reply(
    cx: &Cx,
    /// The reply form.
    Form(form): Form<ReplyForm>,
) -> Result<impl View> {
    let guest = guest(cx).await?;
    let (request, token) = own_request(cx, &guest).await?;
    let back = format!("/requests/{}", request.id);
    let sent = post_reply(
        cx,
        request.conversation_id,
        &token,
        &form.message_text,
        back,
    )
    .await?;
    Ok(view! { reply_result(sent: sent) })
}

#[cfg(test)]
mod request_pages {
    use super::*;
    use chrono::Duration;
    use rust_decimal::Decimal;

    #[test]
    fn request_pages_actions_follow_the_status() {
        use GuestAction::*;
        assert_eq!(guest_actions(RequestStatus::Open), [Withdraw]);
        assert_eq!(
            guest_actions(RequestStatus::Accepted),
            [ProceedToCheckout, Withdraw]
        );
        assert_eq!(
            guest_actions(RequestStatus::Quoted),
            [AcceptQuote, Withdraw]
        );
        assert_eq!(guest_actions(RequestStatus::Converted), [ViewBookings]);
        for status in [
            RequestStatus::Declined,
            RequestStatus::Expired,
            RequestStatus::Withdrawn,
            RequestStatus::Cancelled,
        ] {
            assert!(guest_actions(status).is_empty(), "{status:?}");
        }
    }

    #[test]
    fn request_pages_time_left_reads_naturally() {
        let now = Utc::now();
        assert_eq!(time_left(now - Duration::minutes(1), now), "now");
        assert_eq!(time_left(now + Duration::minutes(45), now), "in 45m");
        assert_eq!(time_left(now + Duration::minutes(200), now), "in 3h 20m");
        assert_eq!(time_left(now + Duration::days(5), now), "in 5 days");
    }

    #[test]
    fn request_pages_stale_and_expired_quotes_get_a_prompt_not_an_error_page() {
        assert_eq!(
            notice_message("QUOTE_SUPERSEDED"),
            RequestRefusal::QuoteSuperseded.message()
        );
        assert_eq!(
            notice_message("QUOTE_EXPIRED"),
            RequestRefusal::QuoteExpired.message()
        );
        assert!(notice_message("withdrawn").contains("released"));
        assert!(notice_message("whatever").contains("try again"));
    }

    #[test]
    fn request_pages_a_zero_amount_is_left_off_the_breakdown() {
        let price = PriceBreakdown {
            currency: "USD".into(),
            nights: 3,
            nightly_rate: Decimal::new(20000, 2),
            subtotal: Decimal::new(60000, 2),
            stay_discount: Decimal::ZERO,
            promo_discount: Decimal::ZERO,
            cleaning_fee: Decimal::ZERO,
            tax: Decimal::new(9000, 2),
            platform_fee: Decimal::new(3000, 2),
            damage_deposit: Decimal::ZERO,
            total: Decimal::new(72000, 2),
        };
        let view = breakdown_view(&price);
        assert_eq!(view.cleaning_fee, None);
        assert_eq!(view.discount_amount, None);
        assert_eq!(view.platform_fee, Some(Decimal::new(3000, 2)));
        assert_eq!(view.total_amount, Decimal::new(72000, 2));
    }
}
