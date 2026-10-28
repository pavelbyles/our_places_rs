use std::collections::HashMap;

use chrono::Duration;
use common::cancellation::RefundQuote;
use common::models::{BookingResponse, ListingResponse, UpdatedBookingRequest};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{error::see_other, page, parse_query_params, path_param},
    view::{View, component, view},
};
use uuid::Uuid;
use web_client::{client::ListingSearchParams, get_api_client};
use web_ui_common::components::domain::price_breakdown::money;

use crate::support::{guest, owned_booking, parse_booking_id, service_token};

path_param!(id);

/// How long a pending hold lasts (the hold-cleanup job's window).
const HOLD_HOURS: i64 = 2;

#[derive(Deserialize, Default)]
struct BookingsQuery {
    new_booking: Option<String>,
}

fn status_of(b: &BookingResponse) -> String {
    b.status.to_lowercase()
}

/// One booking card's data.
#[derive(Clone)]
struct BookingCard {
    booking: BookingResponse,
    listing_name: String,
    listing_slug: Option<String>,
    location: String,
}

#[component]
async fn booking_card(card: BookingCard) -> Result<impl View> {
    let b = card.booking;
    let id = b.id.to_string();
    let status = status_of(&b);
    let is_cancelled = status == "cancelled" || status == "refunded";
    let is_confirmed = status == "confirmed";
    let is_pending = status == "pending";
    let has_receipt = is_confirmed || status == "completed";
    let held_until = (b.created_at + Duration::hours(HOLD_HOURS))
        .format("%-d %b, %H:%M UTC")
        .to_string();
    let receipt_href = format!(
        "/bookings/{}/documents/{}",
        b.id,
        common::tax_document::document_number(
            common::tax_document::TaxDocumentType::PaymentReceipt,
            chrono::Datelike::year(&b.date_from),
            &b.confirmation_code,
            1,
        )
    );
    let review_href = b
        .review_eligibility
        .as_ref()
        .filter(|r| r.is_eligible)
        .and_then(|r| r.token.clone())
        .map(|t| format!("/reviews/submit?token={t}"));
    let book_again = card.listing_slug.as_ref().map(|slug| {
        format!(
            "/listings/{slug}?check_in={}&check_out={}",
            b.date_from, b.date_to
        )
    });
    let action = "inline-flex h-10 items-center rounded-full border border-border px-4 text-sm font-medium no-underline text-foreground";
    Ok(view! {
        <article id=(format!("booking-{id}")) class="space-y-4 rounded-[var(--radius-surface,16px)] border border-border bg-card p-6 text-card-foreground">
            <div class="flex flex-wrap items-start justify-between gap-3">
                <div class="space-y-1">
                    <h2 class="font-serif text-2xl">(card.listing_name)</h2>
                    <p class="text-sm text-muted-foreground">(card.location) " · Reservation " <span class="font-mono">(b.confirmation_code.clone())</span></p>
                    <p class="text-sm">(format!("{} → {} · {} guest(s)", b.date_from.format("%a %-d %b %Y"), b.date_to.format("%a %-d %b %Y"), b.number_of_persons))</p>
                    <p class="text-sm font-medium">(money(&b.currency, b.total_price))</p>
                </div>
                <span id=(format!("status-badge-{id}")) class="rounded-full border border-border px-3 py-1 text-xs font-medium capitalize">(status.clone())</span>
            </div>
            if is_pending {
                <p class="text-sm text-muted-foreground">(format!("Your dates are held until {held_until} while payment is arranged."))</p>
            }
            if is_cancelled {
                if let Some(href) = book_again.clone() {
                    <p class="text-sm">"This reservation is cancelled. " <a href=(href)>"Book again"</a></p>
                }
            }
            <div id=(format!("booking-actions-{id}")) class="flex flex-wrap gap-2">
                if let Some(slug) = card.listing_slug.clone() {
                    <a href=(format!("/listings/{slug}")) class=(action)>"View Listing"</a>
                }
                <a href=(format!("/bookings/{}/messages", b.confirmation_code)) class=(action)>"Message host"</a>
                if is_confirmed {
                    <a id=(format!("btn-change-dates-{id}")) href=(format!("/bookings/{id}/change-dates")) class=(action)>"Modify stay"</a>
                }
                if has_receipt {
                    <a id=(format!("btn-receipt-{id}")) href=(receipt_href) class=(action)>"Tax receipt"</a>
                }
                if let Some(href) = review_href {
                    <a href=(href) class=(action)>"Write a review"</a>
                }
                if !is_cancelled {
                    <button
                        id=(format!("btn-cancel-{id}"))
                        type="button"
                        hx-get=(format!("/bookings/{id}/cancel"))
                        hx-target="#cancel-dialog-slot"
                        hx-swap="innerHTML"
                        class="inline-flex h-10 items-center rounded-full border border-destructive px-4 text-sm font-medium text-destructive"
                    >"Cancel reservation"</button>
                }
            </div>
        </article>
    })
}

async fn listing_index(cx: &Cx) -> HashMap<Uuid, ListingResponse> {
    get_api_client(cx)
        .search_listings(ListingSearchParams {
            per_page: Some(100),
            ..Default::default()
        })
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|l| (l.id, l))
        .collect()
}

/// The signed-in guest's bookings (their own only).
#[page("/bookings")]
pub async fn bookings_page(cx: &Cx) -> Result<impl View> {
    let guest = guest(cx).await?;
    let token = service_token(cx, &guest)?;
    let (bookings, listings) = tokio::join!(
        get_api_client(cx).get_user_bookings_as(guest.user_id, &token),
        listing_index(cx)
    );
    let bookings = bookings.map_err(web_client::app_error)?;
    let cards: Vec<BookingCard> = bookings
        .into_iter()
        .map(|b| {
            let listing = listings.get(&b.listing_id);
            BookingCard {
                listing_name: listing
                    .map(|l| l.name.clone())
                    .unwrap_or_else(|| "Your stay".to_string()),
                listing_slug: listing.map(|l| l.slug.clone()),
                location: listing
                    .map(|l| match &l.city {
                        Some(city) => format!("{city}, {}", l.country),
                        None => l.country.clone(),
                    })
                    .unwrap_or_default(),
                booking: b,
            }
        })
        .collect();
    let just_booked = parse_query_params::<BookingsQuery>(cx)
        .ok()
        .and_then(|q| q.new_booking)
        .is_some();
    Ok(view! {
        <div class="space-y-8">
            if just_booked {
                <div id="booking-success-toast" role="status" class="rounded-lg border border-border bg-card p-4 text-sm">
                    "Your dates are held. We'll confirm your reservation once payment is arranged."
                </div>
            }
            <div class="flex flex-wrap items-end justify-between gap-4">
                <h1 class="font-serif text-[clamp(32px,4vw,48px)]">"My bookings"</h1>
                <a href="/listings" class="text-sm font-medium">"Book another stay"</a>
            </div>
            <div id="upcoming-bookings-container" class="space-y-6">
                if cards.is_empty() {
                    <div id="no-bookings-card" class="rounded-[var(--radius-surface,16px)] border border-border bg-card p-10 text-center">
                        <p class="font-serif text-xl">"No bookings yet"</p>
                        <p class="mt-2 text-sm text-muted-foreground"><a href="/listings">"Explore our villas"</a></p>
                    </div>
                } else {
                    for card in cards {
                        booking_card(card: card)
                    }
                }
            </div>
            <div id="cancel-dialog-slot"></div>
        </div>
    })
}

async fn refund_quote(cx: &Cx, id: Uuid) -> Result<RefundQuote> {
    let guest = guest(cx).await?;
    owned_booking(cx, &guest, id).await?;
    let token = service_token(cx, &guest)?;
    get_api_client(cx)
        .get_cancellation_quote_as(id, &token)
        .await
        .map_err(|e| web_client::app_error(e).into())
}

#[component]
async fn quote_details(quote: RefundQuote, currency: String) -> Result<impl View> {
    let pct = (quote.policy_pct * rust_decimal::Decimal::ONE_HUNDRED).round();
    Ok(view! {
        <dl id="cancel-modal-quote-details" class="space-y-1 rounded-lg border border-border p-4 text-sm">
            <div class="flex justify-between"><dt>"Refund schedule"</dt><dd>(format!("{pct}% of accommodation"))</dd></div>
            <div class="flex justify-between font-medium"><dt>"Refund Total"</dt><dd>(money(&currency, quote.refund_total))</dd></div>
            <div class="flex justify-between text-muted-foreground"><dt>"Retained / fees"</dt><dd>(money(&currency, quote.retained_total))</dd></div>
        </dl>
    })
}

/// Refund quote for one of the guest's bookings (decision D8: 404 for
/// anyone else's).
#[page("/bookings/{id}/cancellation-quote")]
pub async fn cancellation_quote(cx: &Cx) -> Result<impl View> {
    let id = parse_booking_id(path_param::<Id>(cx))?;
    let guest = guest(cx).await?;
    let booking = owned_booking(cx, &guest, id).await?;
    let quote = refund_quote(cx, id).await?;
    let currency = booking.currency;
    Ok(view! { quote_details(quote: quote, currency: currency) })
}

/// The cancel confirmation dialog, swapped into the page and opened by
/// `ui.js` (`data-auto-open`).
#[page("/bookings/{id}/cancel")]
pub async fn cancel_dialog(cx: &Cx) -> Result<impl View> {
    let id = parse_booking_id(path_param::<Id>(cx))?;
    let guest = guest(cx).await?;
    let booking = owned_booking(cx, &guest, id).await?;
    let quote = refund_quote(cx, id).await.ok();
    let currency = booking.currency.clone();
    let action = format!("/bookings/{id}/cancel");
    Ok(view! {
        <dialog id="cancel-booking-modal" data-auto-open="" aria-labelledby="cancel-title" class="m-auto w-[min(480px,92vw)] rounded-[var(--radius-surface,16px)] bg-card p-6 text-card-foreground backdrop:bg-black/50">
            <div class="space-y-4">
                <h2 id="cancel-title" class="font-serif text-2xl">"Cancel reservation?"</h2>
                <p class="text-sm">"Reservation " <span class="font-mono">(booking.confirmation_code.clone())</span></p>
                if let Some(quote) = quote {
                    quote_details(quote: quote, currency: currency)
                } else {
                    <p id="cancel-modal-quote-details" class="text-sm text-muted-foreground">"Your refund will follow the property's cancellation schedule."</p>
                }
                <form method="post" action=(action) class="flex justify-end gap-2">
                    <button type="button" data-dialog-close="" class="h-10 rounded-full border border-border px-4 text-sm">"Keep Reservation"</button>
                    <button type="submit" class="h-10 rounded-full bg-destructive px-4 text-sm font-medium text-destructive-foreground">"Yes, Cancel Booking"</button>
                </form>
            </div>
        </dialog>
    })
}

/// Cancels one of the guest's bookings.
#[page(POST "/bookings/{id}/cancel")]
pub async fn cancel_booking(cx: &Cx) -> Result<()> {
    let id = parse_booking_id(path_param::<Id>(cx))?;
    let guest = guest(cx).await?;
    owned_booking(cx, &guest, id).await?;
    let token = service_token(cx, &guest)?;
    let request = UpdatedBookingRequest {
        status: Some("cancelled".to_string()),
        ..Default::default()
    };
    get_api_client(cx)
        .update_booking_with_auth(id, &request, Some(&token))
        .await
        .map_err(web_client::app_error)?;
    tracing::info!(booking_id = %id, "booking cancelled by guest");
    Err(see_other("/bookings").into())
}
