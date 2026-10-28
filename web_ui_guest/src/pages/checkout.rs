use chrono::NaiveDate;
use common::models::{ListingDetails, NewBookingRequest};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form,
        error::{not_found, redirect_permanent, see_other},
        page, parse_query_params, path_param,
    },
    view::{View, component, view},
};
use web_client::{
    get_api_client,
    session::{GuardError, GuestSession, login_redirect, require_guest},
};
use web_ui_common::components::{
    domain::{
        listing_card::ListingSummary,
        price_breakdown::{PriceBreakdown, price_breakdown},
    },
    steps::steps,
};
use web_ui_common::guard::guard_failure;

use crate::pages::register::urlencode;
use crate::quote::quote_for;
use crate::support::{display_currency, jamaica_today};

path_param!(slug);
path_param!(id);

/// Display currencies accepted at checkout.
const CURRENCIES: [&str; 5] = ["USD", "JMD", "EUR", "GBP", "CAD"];
const ARRIVAL_TIMES: [&str; 4] = [
    "3:00 PM (Standard Check-in)",
    "5:00 PM",
    "7:00 PM",
    "9:00 PM (Late arrival)",
];

#[derive(Deserialize, Default, Clone)]
struct StayQuery {
    check_in: Option<NaiveDate>,
    check_out: Option<NaiveDate>,
    guests: Option<u32>,
}

/// Checkout form fields.
#[derive(Deserialize, Clone)]
pub struct CheckoutForm {
    /// Check-in date.
    pub check_in: NaiveDate,
    /// Check-out date.
    pub check_out: NaiveDate,
    /// Number of adults.
    pub guests: u32,
    /// Display currency.
    #[serde(default)]
    pub currency: String,
    /// Guest first name.
    pub first_name: String,
    /// Guest last name.
    pub last_name: String,
    /// Contact email.
    pub email: String,
    /// Contact phone.
    #[serde(default)]
    pub phone: String,
    /// Estimated arrival time.
    #[serde(default)]
    pub arrival_time: String,
    /// Optional note to the host.
    #[serde(default)]
    pub message_to_host: String,
}

/// Legacy `{slug}-jmd` style URLs carry the currency in the slug.
fn split_currency_suffix(slug: &str) -> Option<(&str, &'static str)> {
    CURRENCIES.iter().skip(1).find_map(|c| {
        slug.strip_suffix(&format!("-{}", c.to_lowercase()))
            .map(|base| (base, *c))
    })
}

fn checkout_url(
    slug: &str,
    check_in: NaiveDate,
    check_out: NaiveDate,
    guests: u32,
    currency: &str,
) -> String {
    let mut url =
        format!("/checkout/{slug}?check_in={check_in}&check_out={check_out}&guests={guests}");
    if !currency.is_empty() {
        url.push_str(&format!("&currency={}", urlencode(currency)));
    }
    url
}

fn split_name(name: &str) -> (String, String) {
    let mut parts = name.split_whitespace();
    let first = parts.next().unwrap_or_default().to_string();
    (first, parts.collect::<Vec<_>>().join(" "))
}

/// Everything the checkout page renders.
#[derive(Clone)]
struct CheckoutView {
    summary: ListingSummary,
    slug: String,
    max_guests: u32,
    stay: Option<(NaiveDate, NaiveDate)>,
    guests: u32,
    currency: String,
    breakdown: Option<PriceBreakdown>,
    first_name: String,
    last_name: String,
    email: String,
    phone: String,
    arrival_time: String,
    message: String,
    error: Option<String>,
}

#[component]
async fn checkout_view(v: CheckoutView) -> Result<impl View> {
    let label = "block text-xs font-semibold tracking-wide text-muted-foreground uppercase";
    let input = "mt-1 block h-11 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground normal-case tracking-normal font-normal";
    let today = jamaica_today().to_string();
    let current_step = if v.stay.is_some() { 1 } else { 0 };
    let action = format!("/checkout/{}", v.slug);
    let (check_in, check_out) = v
        .stay
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .unwrap_or_default();
    Ok(view! {
        <div class="space-y-8">
            <div class="space-y-3">
                <h1 class="font-serif text-[clamp(32px,4vw,48px)]">"Confirm and book"</h1>
                steps(labels: vec!["Dates".into(), "Your details".into(), "Confirm".into()], current: current_step)
            </div>
            if let Some(error) = v.error.clone() {
                <div id="checkout-error-banner" role="alert" class="rounded-lg border border-destructive p-4 text-sm text-destructive">
                    <span id="checkout-error-text">(error)</span>
                </div>
            }
            <div class="grid gap-10 lg:grid-cols-[1fr_380px]">
                if v.stay.is_none() {
                    <form method="get" action=(action.clone()) class="space-y-4 rounded-[var(--radius-surface,16px)] border border-border bg-card p-6">
                        <h2 class="font-serif text-2xl">"Choose your dates"</h2>
                        <div class="grid gap-3 sm:grid-cols-3">
                            <label class=(label)>"Check in"<input id="checkout-check-in" type="date" name="check_in" min=(today.clone()) required=(true) class=(input) /></label>
                            <label class=(label)>"Check out"<input id="checkout-check-out" type="date" name="check_out" min=(today) required=(true) class=(input) /></label>
                            <label class=(label)>"Guests"<input type="number" name="guests" min="1" max=(v.max_guests.to_string()) value=(v.guests.to_string()) class=(input) /></label>
                        </div>
                        if !v.currency.is_empty() {
                            <input type="hidden" name="currency" value=(v.currency.clone()) />
                        }
                        <button type="submit" class="h-11 rounded-[var(--radius-control,10px)] bg-primary px-6 text-[15px] font-medium text-primary-foreground">"See price"</button>
                    </form>
                } else {
                    <form id="checkout-form" method="post" action=(action) class="space-y-6 rounded-[var(--radius-surface,16px)] border border-border bg-card p-6">
                        <input type="hidden" name="check_in" value=(check_in.clone()) />
                        <input type="hidden" name="check_out" value=(check_out.clone()) />
                        <input type="hidden" name="guests" value=(v.guests.to_string()) />
                        <input type="hidden" name="currency" value=(v.currency.clone()) />
                        <h2 class="font-serif text-2xl">"Your details"</h2>
                        <div class="grid gap-4 sm:grid-cols-2">
                            <label class=(label) for="guest-first-name">"First name"<input id="guest-first-name" name="first_name" required=(true) autocomplete="given-name" value=(v.first_name.clone()) class=(input) /></label>
                            <label class=(label) for="guest-last-name">"Last name"<input id="guest-last-name" name="last_name" required=(true) autocomplete="family-name" value=(v.last_name.clone()) class=(input) /></label>
                            <label class=(label) for="guest-email">"Email"<input id="guest-email" name="email" type="email" required=(true) autocomplete="email" value=(v.email.clone()) class=(input) /></label>
                            <label class=(label) for="guest-phone">"Phone"<input id="guest-phone" name="phone" type="tel" autocomplete="tel" value=(v.phone.clone()) class=(input) /></label>
                        </div>
                        <label class=(label) for="guest-arrival-time">"Estimated arrival"
                            <select id="guest-arrival-time" name="arrival_time" class=(input)>
                                for t in ARRIVAL_TIMES {
                                    <option value=(t) selected=(v.arrival_time == t)>(t)</option>
                                }
                            </select>
                        </label>
                        <label class=(label) for="guest-message">"Message to your host (optional)"
                            <textarea id="guest-message" name="message_to_host" rows="3" maxlength="2000" class=(format!("{input} h-auto py-2"))>(v.message.clone())</textarea>
                        </label>
                        <p class="text-sm text-muted-foreground">"Confirming places a hold on these dates for 2 hours while payment is arranged."</p>
                        <button id="checkout-submit-btn" type="submit" class="h-11 w-full rounded-[var(--radius-control,10px)] bg-primary text-[15px] font-medium text-primary-foreground">"Confirm reservation"</button>
                    </form>
                }
                <aside aria-label="Your stay" class="h-fit space-y-4 rounded-[var(--radius-surface,16px)] border border-border bg-card p-6">
                    <p class="font-serif text-xl">(v.summary.name.clone())</p>
                    <p class="text-sm text-muted-foreground">(v.summary.location())</p>
                    if let Some((a, b)) = v.stay {
                        <p class="text-sm">(format!("{} → {} · {} guest(s)", a.format("%a %-d %b %Y"), b.format("%a %-d %b %Y"), v.guests))</p>
                    }
                    if let Some(b) = v.breakdown.clone() {
                        price_breakdown(quote: b)
                    }
                </aside>
            </div>
        </div>
    })
}

async fn load(cx: &Cx, slug: &str, currency: Option<&str>) -> Result<ListingDetails> {
    get_api_client(cx)
        .get_listing_by_id(slug, currency)
        .await
        .map_err(|_| not_found().into())
}

fn valid_stay(
    check_in: Option<NaiveDate>,
    check_out: Option<NaiveDate>,
) -> Option<(NaiveDate, NaiveDate)> {
    match (check_in, check_out) {
        (Some(a), Some(b)) if b > a && a >= jamaica_today() => Some((a, b)),
        _ => None,
    }
}

async fn build_view(
    cx: &Cx,
    details: &ListingDetails,
    guest: &GuestSession,
    stay: Option<(NaiveDate, NaiveDate)>,
    guests: u32,
    currency: String,
) -> CheckoutView {
    let breakdown = match stay {
        Some((a, b)) => {
            let cur = (!currency.is_empty()).then_some(currency.as_str());
            quote_for(cx, details, a, b, cur).await
        }
        None => None,
    };
    let (first_name, last_name) = split_name(&guest.name);
    let max_guests = u32::try_from(details.listing.max_guests.max(1)).unwrap_or(1);
    CheckoutView {
        summary: ListingSummary::from(&details.listing),
        slug: details.listing.slug.clone(),
        max_guests,
        stay,
        guests: guests.clamp(1, max_guests),
        currency,
        breakdown,
        first_name,
        last_name,
        email: guest.email.clone(),
        phone: String::new(),
        arrival_time: ARRIVAL_TIMES[0].to_string(),
        message: String::new(),
        error: None,
    }
}

/// Checkout. Signed-in guests only; without dates it asks for them first.
#[page("/checkout/{slug}")]
pub async fn checkout_page(cx: &Cx) -> Result<impl View> {
    let slug: &str = path_param::<Slug>(cx);
    if let Some((base, currency)) = split_currency_suffix(slug) {
        return Err(redirect_permanent(format!("/checkout/{base}?currency={currency}")).into());
    }
    let guest = require_guest(cx).await.map_err(|e| guard_failure(cx, e))?;
    let currency = display_currency(cx).unwrap_or_default();
    let details = load(
        cx,
        slug,
        (!currency.is_empty()).then_some(currency.as_str()),
    )
    .await?;
    let q = parse_query_params::<StayQuery>(cx).unwrap_or_default();
    let stay = valid_stay(q.check_in, q.check_out);
    let view_data = build_view(cx, &details, &guest, stay, q.guests.unwrap_or(2), currency).await;
    Ok(view! { checkout_view(v: view_data) })
}

/// Places the booking hold for the signed-in guest. The guest ID comes from
/// the session only. If the session has expired, the guest signs in again and
/// returns to this checkout with the same stay (no hold is created or
/// extended in the meantime).
#[page(POST "/checkout/{slug}")]
pub async fn checkout_submit(
    cx: &Cx,
    /// The submitted checkout form.
    Form(form): Form<CheckoutForm>,
) -> Result<impl View> {
    let slug: &str = path_param::<Slug>(cx);
    let guest = match require_guest(cx).await {
        Ok(guest) => guest,
        Err(GuardError::Unauthenticated) => {
            let back = checkout_url(
                slug,
                form.check_in,
                form.check_out,
                form.guests,
                &form.currency,
            );
            return Err(see_other(login_redirect(&back)).into());
        }
        Err(e) => return Err(guard_failure(cx, e)),
    };
    let currency = if CURRENCIES.contains(&form.currency.as_str()) {
        form.currency.clone()
    } else {
        String::new()
    };
    let details = load(
        cx,
        slug,
        (!currency.is_empty()).then_some(currency.as_str()),
    )
    .await?;
    let stay = valid_stay(Some(form.check_in), Some(form.check_out));
    let mut view_data = build_view(cx, &details, &guest, stay, form.guests, currency.clone()).await;
    view_data.first_name = form.first_name.clone();
    view_data.last_name = form.last_name.clone();
    view_data.email = form.email.clone();
    view_data.phone = form.phone.clone();
    view_data.arrival_time = form.arrival_time.clone();
    view_data.message = form.message_to_host.clone();

    match place_booking(
        cx,
        &details,
        &guest,
        &form,
        stay,
        view_data.max_guests,
        currency,
    )
    .await
    {
        Ok(()) => return Err(see_other("/bookings?new_booking=true").into()),
        Err(message) => view_data.error = Some(message),
    }
    Ok(view! { checkout_view(v: view_data) })
}

/// Validates the stay and asks `booking_api` for a hold. `Err` carries the
/// message to show the guest.
async fn place_booking(
    cx: &Cx,
    details: &ListingDetails,
    guest: &GuestSession,
    form: &CheckoutForm,
    stay: Option<(NaiveDate, NaiveDate)>,
    max_guests: u32,
    currency: String,
) -> std::result::Result<(), String> {
    let (check_in, check_out) = stay.ok_or_else(|| {
        "Please choose valid dates: check-out must be after check-in, starting today or later."
            .to_string()
    })?;
    if form.guests == 0 || form.guests > max_guests {
        return Err(format!("This stay allows 1 to {max_guests} guests."));
    }
    let request = NewBookingRequest {
        guest_id: guest.user_id,
        listing_id: details.listing.id,
        check_in,
        check_out,
        num_adults: form.guests,
        num_children: 0,
        num_infants: 0,
        num_pets: 0,
        message_to_host: Some(form.message_to_host.trim().to_string()).filter(|m| !m.is_empty()),
        estimated_arrival_time: ARRIVAL_TIMES
            .contains(&form.arrival_time.as_str())
            .then(|| form.arrival_time.clone()),
        is_business_trip: false,
        business_company_name: None,
        business_tax_id: None,
        currency: if currency.is_empty() {
            details.listing.base_currency.clone()
        } else {
            currency
        },
        agreed_cancellation_policy: "Flexible".to_string(),
    };
    match get_api_client(cx).create_booking(&request).await {
        Ok(booking) => {
            tracing::info!(booking_id = %booking.id, "booking hold placed");
            Ok(())
        }
        Err(e) => {
            tracing::info!("booking rejected: {e}");
            let text = e.to_string();
            Err(
                if text.contains("available")
                    || text.contains("overlap")
                    || text.contains("conflict")
                {
                    "Those dates are no longer available. Please choose different dates."
                        .to_string()
                } else if text.contains("max_guests") {
                    format!("This stay allows 1 to {max_guests} guests.")
                } else {
                    "We couldn't place your booking. Please try again.".to_string()
                },
            )
        }
    }
}

macro_rules! legacy_currency_checkout {
    ($name:ident, $path:literal, $currency:literal) => {
        /// Legacy per-currency checkout URL; redirects permanently.
        #[page($path)]
        pub async fn $name(cx: &Cx) -> Result<()> {
            let id: &str = path_param::<Id>(cx);
            Err(redirect_permanent(format!("/checkout/{id}?currency={}", $currency)).into())
        }
    };
}

legacy_currency_checkout!(checkout_jmd, "/checkout-jmd/{id}", "JMD");
legacy_currency_checkout!(checkout_eur, "/checkout-eur/{id}", "EUR");
legacy_currency_checkout!(checkout_gbp, "/checkout-gbp/{id}", "GBP");
legacy_currency_checkout!(checkout_cad, "/checkout-cad/{id}", "CAD");
