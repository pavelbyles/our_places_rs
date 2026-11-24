use chrono::NaiveDate;
use common::app_client::booking_requests::{api_error_code, create_booking_request};
use common::booking_request::{BookingMode, CreateBookingRequestBody, RequestRefusal};
use common::models::{ListingDetails, NewBookingRequest, StayQuoteRequest};
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
use uuid::Uuid;
use web_client::{
    get_api_client,
    session::{GuardError, GuestSession, login_redirect, require_guest},
};
use web_ui_common::components::domain::currency_selector::active_currency_codes;
use web_ui_common::components::{
    domain::{
        listing_card::ListingSummary,
        price_breakdown::{PriceBreakdown, price_breakdown},
    },
    steps::steps,
};
use web_ui_common::guard::guard_failure;

use crate::quote::{NoQuote, signed_in_quote};
use crate::support::{display_currency, guests_or_default, jamaica_today, service_token};
use common::query::urlencode;

path_param!(slug);
path_param!(id);

/// Display currencies accepted at checkout.
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
    /// The guest's enquiry this request follows from, if any.
    enquiry_id: Option<Uuid>,
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
    /// Promo code as typed.
    #[serde(default)]
    pub promo_code: String,
    /// Which button was pressed: `apply_promo`, `remove_promo`, `request` to
    /// send a request to book, or empty to book.
    #[serde(default)]
    pub intent: String,
    /// The guest's enquiry this request follows from, if any.
    #[serde(default)]
    pub enquiry_id: Option<Uuid>,
}

/// The submit buttons for a listing's booking mode: `(intent, label)`. An
/// empty intent places an instant hold; `request` sends a request to book.
pub fn checkout_actions(mode: BookingMode) -> Vec<(&'static str, &'static str)> {
    match mode {
        BookingMode::Instant => vec![("", "Confirm reservation")],
        BookingMode::Request => vec![("request", "Send request to book")],
        BookingMode::Both => vec![
            ("", "Confirm reservation"),
            ("request", "Request to book instead"),
        ],
    }
}

/// The note under the form for a listing's booking mode.
fn checkout_note(mode: BookingMode) -> &'static str {
    match mode {
        BookingMode::Request => {
            "Your card isn't charged. The host has 24 hours to respond, and your dates are held meanwhile."
        }
        _ => "Confirming places a hold on these dates for 2 hours while payment is arranged.",
    }
}

/// What to tell the guest when a request to book is refused.
pub fn request_error_message(err: &anyhow::Error, max_guests: u32) -> String {
    match api_error_code(err) {
        Some(code) => match RequestRefusal::from_code(code) {
            Some(refusal) => refusal.message(),
            None => booking_error_message(code, max_guests),
        },
        None => "We couldn't send your request. Please try again.".to_string(),
    }
}

/// Legacy `{slug}-jmd` style URLs carry an (active) currency in the slug.
fn split_currency_suffix<'a>(slug: &'a str, codes: &[String]) -> Option<(&'a str, String)> {
    codes.iter().find_map(|c| {
        slug.strip_suffix(&format!("-{}", c.to_lowercase()))
            .map(|base| (base, c.clone()))
    })
}

/// Where the form posts. A valid stay stays in the URL, so a page re-rendered
/// after a refusal still knows the stay when the guest switches currency. The
/// handler books from the form's fields, never from this query.
fn form_action(
    slug: &str,
    stay: Option<(NaiveDate, NaiveDate)>,
    guests: u32,
    currency: &str,
) -> String {
    match stay {
        Some((check_in, check_out)) => checkout_url(slug, check_in, check_out, guests, currency),
        None => format!("/checkout/{slug}"),
    }
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
    /// Why there is no breakdown for the chosen dates, as shown to the guest.
    no_quote: Option<String>,
    first_name: String,
    last_name: String,
    email: String,
    phone: String,
    arrival_time: String,
    message: String,
    error: Option<String>,
    promo_code: String,
    promo_refusal: Option<String>,
    mode: BookingMode,
    enquiry_id: Option<Uuid>,
}

#[component]
async fn checkout_view(cx: &Cx, v: CheckoutView) -> Result<impl View> {
    let label = "block text-xs font-semibold tracking-wide text-muted-foreground uppercase";
    let input = "mt-1 block h-11 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground normal-case tracking-normal font-normal";
    let today = jamaica_today().to_string();
    let current_step = if v.stay.is_some() { 1 } else { 0 };
    let action = form_action(&v.slug, v.stay, v.guests, &v.currency);
    let (check_in, check_out) = v
        .stay
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .unwrap_or_default();
    Ok(view! {
        <div class="space-y-8">
            <div class="space-y-3">
                <h1 class="font-serif text-[clamp(32px,4vw,48px)]">
                    "Confirm and book"
                </h1>
                steps(
                    labels: vec!["Dates".into(), "Your details".into(), "Confirm".into()],
                    current: current_step
                )
            </div>
            if let Some(error) = v.error.clone() {
                <div
                    id="checkout-error-banner"
                    role="alert"
                    class="rounded-lg border border-destructive p-4 text-sm text-destructive"
                >
                    <span id="checkout-error-text">(error)</span>
                </div>
            }
            <div class="grid gap-10 lg:grid-cols-[1fr_380px]">
                if v.stay.is_none() {
                    <form
                        method="get"
                        action=(action.clone())
                        class="space-y-4 rounded-[var(--radius-surface,16px)] border border-border bg-card p-6"
                    >
                        <h2 class="font-serif text-2xl">"Choose your dates"</h2>
                        <div class="grid gap-3 sm:grid-cols-3">
                            <label class=(label)>
                                "Check in"
                                <input
                                    id="checkout-check-in"
                                    type="text"
                                    data-date-picker=""
                                    placeholder="yyyy-mm-dd"
                                    pattern="\\d{4}-\\d{2}-\\d{2}"
                                    name="check_in"
                                    min=(today.clone())
                                    required=(true)
                                    class=(input)
                                />
                            </label>
                            <label class=(label)>
                                "Check out"
                                <input
                                    id="checkout-check-out"
                                    type="text"
                                    data-date-picker=""
                                    placeholder="yyyy-mm-dd"
                                    pattern="\\d{4}-\\d{2}-\\d{2}"
                                    name="check_out"
                                    min=(today)
                                    required=(true)
                                    class=(input)
                                />
                            </label>
                            <label class=(label)>
                                "Guests"
                                <input
                                    type="number"
                                    name="guests"
                                    min="1"
                                    max=(v.max_guests.to_string())
                                    value=(v.guests.to_string())
                                    class=(input)
                                />
                            </label>
                        </div>
                        if !v.currency.is_empty() {
                            <input
                                type="hidden"
                                name="currency"
                                value=(v.currency.clone())
                            />
                        }
                        <button
                            type="submit"
                            class="h-11 rounded-[var(--radius-control,10px)] bg-primary px-6 text-[15px] font-medium text-primary-foreground"
                        >
                            "See price"
                        </button>
                    </form>
                } else {
                    <form
                        id="checkout-form"
                        method="post"
                        action=(action)
                        class="space-y-6 rounded-[var(--radius-surface,16px)] border border-border bg-card p-6"
                    >
                        <input type="hidden" name="check_in" value=(check_in.clone()) />
                        <input
                            type="hidden"
                            name="check_out"
                            value=(check_out.clone())
                        />
                        <input
                            type="hidden"
                            name="guests"
                            value=(v.guests.to_string())
                        />
                        <input
                            type="hidden"
                            name="currency"
                            value=(v.currency.clone())
                        />
                        if let Some(enquiry) = v.enquiry_id {
                            <input
                                type="hidden"
                                name="enquiry_id"
                                value=(enquiry.to_string())
                            />
                        }
                        <h2 class="font-serif text-2xl">"Your details"</h2>
                        <div class="grid gap-4 sm:grid-cols-2">
                            <label class=(label) for="guest-first-name">
                                "First name"
                                <input
                                    id="guest-first-name"
                                    name="first_name"
                                    required=(true)
                                    autocomplete="given-name"
                                    value=(v.first_name.clone())
                                    class=(input)
                                />
                            </label>
                            <label class=(label) for="guest-last-name">
                                "Last name"
                                <input
                                    id="guest-last-name"
                                    name="last_name"
                                    required=(true)
                                    autocomplete="family-name"
                                    value=(v.last_name.clone())
                                    class=(input)
                                />
                            </label>
                            <label class=(label) for="guest-email">
                                "Email"
                                <input
                                    id="guest-email"
                                    name="email"
                                    type="email"
                                    required=(true)
                                    autocomplete="email"
                                    value=(v.email.clone())
                                    class=(input)
                                />
                            </label>
                            <label class=(label) for="guest-phone">
                                "Phone"
                                <input
                                    id="guest-phone"
                                    name="phone"
                                    type="tel"
                                    autocomplete="tel"
                                    value=(v.phone.clone())
                                    class=(input)
                                />
                            </label>
                        </div>
                        <label class=(label) for="guest-arrival-time">
                            "Estimated arrival"
                            <select
                                id="guest-arrival-time"
                                name="arrival_time"
                                class=(input)
                            >
                                for t in ARRIVAL_TIMES {
                                    <option value=(t) selected=(v.arrival_time == t)>
                                        (t)
                                    </option>
                                }
                            </select>
                        </label>
                        <div class="space-y-2">
                            <label class=(label) for="checkout-promo-code">
                                "Promo code"
                            </label>
                            <div class="flex gap-2">
                                <input
                                    id="checkout-promo-code"
                                    name="promo_code"
                                    maxlength="64"
                                    autocomplete="off"
                                    value=(v.promo_code.clone())
                                    class=(format!("{input} mt-0 uppercase"))
                                />
                                <button
                                    id="checkout-promo-apply"
                                    type="submit"
                                    name="intent"
                                    value="apply_promo"
                                    formnovalidate=(true)
                                    class="h-11 shrink-0 rounded-[var(--radius-control,10px)] border border-border px-4 text-sm font-medium"
                                >
                                    "Apply"
                                </button>
                                if !v.promo_code.is_empty() {
                                    <button
                                        id="checkout-promo-remove"
                                        type="submit"
                                        name="intent"
                                        value="remove_promo"
                                        formnovalidate=(true)
                                        class="h-11 shrink-0 rounded-[var(--radius-control,10px)] border border-border px-4 text-sm font-medium"
                                    >
                                        "Remove"
                                    </button>
                                }
                            </div>
                            if let Some(refusal) = v.promo_refusal.clone() {
                                <p
                                    id="checkout-promo-refusal"
                                    role="alert"
                                    class="text-sm text-destructive"
                                >
                                    (refusal)
                                </p>
                            }
                        </div>
                        <label class=(label) for="guest-message">
                            "Message to your host (optional)"
                            <textarea
                                id="guest-message"
                                name="message_to_host"
                                rows="3"
                                maxlength="2000"
                                class=(format!("{input} h-auto py-2"))
                            >
                                (v.message.clone())
                            </textarea>
                        </label>
                        <p
                            id="checkout-mode-note"
                            class="text-sm text-muted-foreground"
                        >
                            (checkout_note(v.mode))
                        </p>
                        if let Some(why) = v.no_quote.clone() {
                            <p
                                id="checkout-no-quote"
                                role="alert"
                                class="text-sm text-destructive"
                            >
                                (why)
                            </p>
                        }
                        for (i, (intent, button_label)) in checkout_actions(v.mode)
                            .into_iter()
                            .enumerate() {
                            if intent.is_empty() {
                                <button
                                    id="checkout-submit-btn"
                                    type="submit"
                                    disabled=(v.breakdown.is_none())
                                    class="h-11 w-full rounded-[var(--radius-control,10px)] bg-primary text-[15px] font-medium text-primary-foreground disabled:opacity-50"
                                >
                                    (button_label)
                                </button>
                            } else {
                                <button
                                    id="checkout-request-btn"
                                    type="submit"
                                    name="intent"
                                    value=(intent)
                                    disabled=(v.breakdown.is_none())
                                    class=(if i == 0 {
                                        "h-11 w-full rounded-[var(--radius-control,10px)] bg-primary text-[15px] font-medium text-primary-foreground disabled:opacity-50"
                                    } else {
                                        "h-11 w-full rounded-[var(--radius-control,10px)] border border-border text-[15px] font-medium disabled:opacity-50"
                                    })
                                >
                                    (button_label)
                                </button>
                            }
                        }
                    </form>
                }
                <aside
                    aria-label="Your stay"
                    class="h-fit space-y-4 rounded-[var(--radius-surface,16px)] border border-border bg-card p-6"
                >
                    <p class="font-serif text-xl">(v.summary.name.clone())</p>
                    <p class="text-sm text-muted-foreground">(v.summary.location())</p>
                    if let Some((a, b)) = v.stay {
                        <p class="text-sm">
                            (format!(
                                "{} → {} · {} guest(s)",
                                a.format("%a %-d %b %Y"),
                                b.format("%a %-d %b %Y"),
                                v.guests,
                            ))
                        </p>
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
        .ok()
        .filter(|details| details.listing.is_active)
        .ok_or_else(|| not_found().into())
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
    promo_code: String,
) -> CheckoutView {
    let (first_name, last_name) = split_name(&guest.name);
    let max_guests = u32::try_from(details.listing.max_guests.max(1)).unwrap_or(1);
    let quoted = match stay {
        Some((check_in, check_out)) => {
            let request = StayQuoteRequest {
                listing_id: details.listing.id,
                check_in,
                check_out,
                guests: guests.clamp(1, max_guests),
                currency: Some(currency.clone()).filter(|c| !c.is_empty()),
                promo_code: Some(promo_code.clone()).filter(|code| !code.trim().is_empty()),
            };
            let quote = match service_token(cx, guest) {
                Ok(token) => signed_in_quote(cx, &token, &request).await,
                Err(e) => {
                    tracing::error!(guest_id = %guest.user_id, error = ?e, "no checkout quote: could not mint the guest's API token");
                    Err(NoQuote::Failed)
                }
            };
            Some(quote)
        }
        None => None,
    };
    let (breakdown, promo_refusal, no_quote) = match quoted {
        Some(Ok((breakdown, refusal))) => (Some(breakdown), refusal, None),
        Some(Err(why)) => (None, None, Some(why.message())),
        None => (None, None, None),
    };
    CheckoutView {
        summary: ListingSummary::from(&details.listing),
        slug: details.listing.slug.clone(),
        max_guests,
        stay,
        guests: guests.clamp(1, max_guests),
        currency,
        breakdown,
        no_quote,
        first_name,
        last_name,
        email: guest.email.clone(),
        phone: String::new(),
        arrival_time: ARRIVAL_TIMES
            .first()
            .copied()
            .unwrap_or_default()
            .to_string(),
        message: String::new(),
        error: None,
        promo_code,
        promo_refusal,
        mode: details.listing.booking_mode,
        enquiry_id: None,
    }
}

/// Checkout. Signed-in guests only; without dates it asks for them first.
#[page("/checkout/{slug}")]
pub async fn checkout_page(cx: &Cx) -> Result<impl View> {
    let slug: &str = slug(cx);
    let offered = active_currency_codes(cx).await;
    if let Some((base, currency)) = split_currency_suffix(slug, &offered) {
        return Err(redirect_permanent(format!("/checkout/{base}?currency={currency}")).into());
    }
    let guest = require_guest(cx).await.map_err(|e| guard_failure(cx, e))?;
    let currency = display_currency(cx).await.unwrap_or_default();
    let details = load(
        cx,
        slug,
        (!currency.is_empty()).then_some(currency.as_str()),
    )
    .await?;
    let q = parse_query_params::<StayQuery>(cx).unwrap_or_default();
    let stay = valid_stay(q.check_in, q.check_out);
    let mut view_data = build_view(
        cx,
        &details,
        &guest,
        stay,
        guests_or_default(q.guests),
        currency,
        String::new(),
    )
    .await;
    view_data.enquiry_id = q.enquiry_id;
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
    let slug: &str = slug(cx);
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
    let currency = if active_currency_codes(cx).await.contains(&form.currency) {
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
    let promo_code = if form.intent == "remove_promo" {
        String::new()
    } else {
        form.promo_code.trim().to_string()
    };
    let mut view_data = build_view(
        cx,
        &details,
        &guest,
        stay,
        form.guests,
        currency.clone(),
        promo_code,
    )
    .await;
    view_data.first_name = form.first_name.clone();
    view_data.last_name = form.last_name.clone();
    view_data.email = form.email.clone();
    view_data.phone = form.phone.clone();
    view_data.arrival_time = form.arrival_time.clone();
    view_data.message = form.message_to_host.clone();
    view_data.enquiry_id = form.enquiry_id;

    // Applying or removing a code only reprices the page. Pressing Enter in any
    // field also lands here, because Apply is the form's first button, so Enter
    // can never place a booking by accident.
    if form.intent.is_empty() {
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
    } else if form.intent == "request" {
        match place_request(
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
            Ok(id) => return Err(see_other(format!("/requests/{id}")).into()),
            Err(message) => view_data.error = Some(message),
        }
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
        promo_code: Some(form.promo_code.trim().to_string()).filter(|code| !code.is_empty()),
    };
    let token = service_token(cx, guest).map_err(|_| {
        tracing::error!(guest_id = %guest.user_id, "booking not placed: could not mint the guest's API token");
        "We couldn't place your booking. Please try again.".to_string()
    })?;
    match get_api_client(cx).create_booking(&request, &token).await {
        Ok(booking) => {
            tracing::info!(booking_id = %booking.id, "booking hold placed");
            Ok(())
        }
        Err(e) => {
            tracing::info!("booking rejected: {e}");
            Err(booking_error_message(&e.to_string(), max_guests))
        }
    }
}

/// Sends a request to book for the signed-in guest. `Err` carries the
/// message to show; `Ok` is the new request's id.
async fn place_request(
    cx: &Cx,
    details: &ListingDetails,
    guest: &GuestSession,
    form: &CheckoutForm,
    stay: Option<(NaiveDate, NaiveDate)>,
    max_guests: u32,
    currency: String,
) -> std::result::Result<Uuid, String> {
    let (check_in, check_out) = stay.ok_or_else(|| {
        "Please choose valid dates: check-out must be after check-in, starting today or later."
            .to_string()
    })?;
    if form.guests == 0 || form.guests > max_guests {
        return Err(format!("This stay allows 1 to {max_guests} guests."));
    }
    let body = CreateBookingRequestBody {
        listing_id: details.listing.id,
        check_in,
        check_out,
        num_adults: form.guests,
        num_children: 0,
        num_infants: 0,
        num_pets: 0,
        message: Some(form.message_to_host.trim().to_string()).filter(|m| !m.is_empty()),
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
        promo_code: Some(form.promo_code.trim().to_string()).filter(|code| !code.is_empty()),
        enquiry_id: form.enquiry_id,
    };
    let token = service_token(cx, guest).map_err(|_| {
        tracing::error!(guest_id = %guest.user_id, "request not sent: could not mint the guest's API token");
        "We couldn't send your request. Please try again.".to_string()
    })?;
    match create_booking_request(&body, &token).await {
        Ok(request) => {
            tracing::info!(request_id = %request.id, "request to book sent");
            Ok(request.id)
        }
        Err(e) => {
            tracing::info!(
                code = api_error_code(&e).unwrap_or("none"),
                "request to book refused"
            );
            Err(request_error_message(&e, max_guests))
        }
    }
}

/// What to tell the guest when `booking_api` refuses a hold. A refused promo code
/// comes first: nothing else should hide why the discount wasn't given.
fn booking_error_message(text: &str, max_guests: u32) -> String {
    // Codes come first: "unavailable" in a currency error's message would also
    // match the word check below.
    if let Some(refusal) = common::app_client::promo_refusal_in_error(text) {
        refusal.message()
    } else if text.contains("DATE_UNAVAILABLE") {
        "Those dates are no longer available. Please choose different dates.".to_string()
    } else if text.contains("BOOKING_MODE_NOT_ALLOWED") {
        "This property takes requests to book. Please send a request instead.".to_string()
    } else if text.contains("LISTING_UNAVAILABLE") {
        "This property is no longer taking bookings.".to_string()
    } else if text.contains("INVALID_STAY_DATES")
        || text.contains(common::pricing::MINIMUM_STAY_NOT_MET)
    {
        common::availability::stay_refusal_message_in_error(text)
            .unwrap_or_else(|| "Please choose valid dates.".to_string())
    } else if text.contains("CURRENCY_INACTIVE") {
        "This currency is no longer offered. Please choose another currency.".to_string()
    } else if text.contains("FX_RATE_UNAVAILABLE") {
        "We can't take payment in this currency right now. Please choose another currency."
            .to_string()
    } else if text.contains("available") || text.contains("overlap") || text.contains("conflict") {
        "Those dates are no longer available. Please choose different dates.".to_string()
    } else if text.contains("max_guests") {
        format!("This stay allows 1 to {max_guests} guests.")
    } else {
        "We couldn't place your booking. Please try again.".to_string()
    }
}

macro_rules! legacy_currency_checkout {
    ($name:ident, $path:literal, $currency:literal) => {
        /// Legacy per-currency checkout URL; redirects permanently.
        #[page($path)]
        pub async fn $name(cx: &Cx) -> Result<()> {
            let id: &str = id(cx);
            Err(redirect_permanent(format!("/checkout/{id}?currency={}", $currency)).into())
        }
    };
}

legacy_currency_checkout!(checkout_jmd, "/checkout-jmd/{id}", "JMD");
legacy_currency_checkout!(checkout_eur, "/checkout-eur/{id}", "EUR");
legacy_currency_checkout!(checkout_gbp, "/checkout-gbp/{id}", "GBP");
legacy_currency_checkout!(checkout_cad, "/checkout-cad/{id}", "CAD");

#[cfg(test)]
mod promo_tests {
    use super::*;
    use common::promo::PromoRefusal;

    #[test]
    fn each_promo_refusal_shows_its_own_message() {
        for refusal in PromoRefusal::ALL {
            if refusal == PromoRefusal::FxRateUnavailable {
                continue;
            }
            let text = format!(
                r#"Failed to create booking (422): {{"code":"{}","message":"x"}}"#,
                refusal.code()
            );
            assert_eq!(booking_error_message(&text, 4), refusal.message());
        }
    }

    #[test]
    fn other_refusals_keep_their_messages() {
        assert!(
            booking_error_message("Listing is not available", 4).contains("no longer available")
        );
        assert!(
            booking_error_message(r#"{"code":"LISTING_UNAVAILABLE"}"#, 4)
                .contains("no longer taking bookings")
        );
        assert!(booking_error_message("exceeds listing max_guests", 4).contains("1 to 4"));
        assert!(booking_error_message("FX_RATE_UNAVAILABLE", 4).contains("currency"));
        assert!(booking_error_message("boom", 4).contains("try again"));
        let short = r#"Failed to create booking (400 Bad Request): {"code":"MINIMUM_STAY_NOT_MET","message":"These dates need a stay of at least 5 nights.","required_nights":5}"#;
        assert_eq!(
            booking_error_message(short, 4),
            "These dates need a stay of at least 5 nights."
        );
    }
}

#[cfg(test)]
mod booking_error_codes {
    use super::booking_error_message;

    #[test]
    fn a_currency_problem_never_says_the_dates_are_gone() {
        let text = r#"Failed to create booking (422 Unprocessable Entity): {"code":"FX_RATE_UNAVAILABLE","message":"Foreign exchange rate unavailable for currency pair: USD -> CAD"}"#;
        let message = booking_error_message(text, 4);
        assert!(message.contains("currency"), "{message}");
        assert!(!message.contains("dates"), "{message}");
    }

    #[test]
    fn taken_dates_ask_for_other_dates() {
        let text = r#"Failed to create booking (409 Conflict): {"code":"DATE_UNAVAILABLE","message":"The requested dates are no longer available."}"#;
        assert!(booking_error_message(text, 4).contains("no longer available"));
    }

    #[test]
    fn invalid_stay_dates_show_their_own_message() {
        let text = r#"Failed to create booking (400 Bad Request): {"code":"INVALID_STAY_DATES","message":"Invalid date range: dates can't be in the past."}"#;
        assert_eq!(
            booking_error_message(text, 4),
            "Invalid date range: dates can't be in the past."
        );
    }

    #[test]
    fn an_unreadable_invalid_stay_dates_body_still_says_something_useful() {
        let message = booking_error_message("INVALID_STAY_DATES", 4);
        assert!(message.contains("valid dates"), "{message}");
    }
}

#[cfg(test)]
mod checkout_form_action {
    use super::form_action;
    use chrono::NaiveDate;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2030, 8, d).unwrap_or_default()
    }

    #[test]
    fn a_valid_stay_stays_in_the_url() {
        assert_eq!(
            form_action("sea-grape", Some((day(1), day(4))), 2, "JMD"),
            "/checkout/sea-grape?check_in=2030-08-01&check_out=2030-08-04&guests=2&currency=JMD"
        );
    }

    #[test]
    fn no_stay_posts_to_the_plain_url() {
        assert_eq!(
            form_action("sea-grape", None, 2, "JMD"),
            "/checkout/sea-grape"
        );
    }

    #[test]
    fn the_currency_menu_keeps_the_stay_from_that_url() {
        let action = form_action("sea-grape", Some((day(1), day(4))), 2, "JMD");
        let query = action.split_once('?').map(|(_, q)| q).unwrap_or_default();
        let kept =
            web_ui_common::components::domain::currency_selector::search_params_from_query(query);
        assert_eq!(
            kept,
            ["check_in=2030-08-01", "check_out=2030-08-04", "guests=2"]
        );
    }
}

#[cfg(test)]
mod checkout_request {
    use super::*;
    use common::app_client::booking_requests::ApiCallError;

    #[test]
    fn checkout_request_buttons_follow_the_booking_mode() {
        let intents = |mode| {
            checkout_actions(mode)
                .into_iter()
                .map(|(intent, _)| intent)
                .collect::<Vec<_>>()
        };
        assert_eq!(intents(BookingMode::Instant), [""]);
        assert_eq!(intents(BookingMode::Request), ["request"]);
        assert_eq!(intents(BookingMode::Both), ["", "request"]);
    }

    #[test]
    fn checkout_request_refusals_become_friendly_messages() {
        let err = |code: &str| -> anyhow::Error {
            ApiCallError::from_body(409, &format!(r#"{{"code":"{code}","message":"raw"}}"#)).into()
        };
        assert_eq!(
            request_error_message(&err("REQUEST_ALREADY_LIVE"), 4),
            RequestRefusal::RequestAlreadyLive.message()
        );
        assert_eq!(
            request_error_message(&err("REQUEST_WINDOW_TOO_SHORT"), 4),
            RequestRefusal::RequestWindowTooShort.message()
        );
        assert!(request_error_message(&err("DATE_UNAVAILABLE"), 4).contains("no longer available"));
        assert_eq!(
            request_error_message(&err("PROMO_EXPIRED"), 4),
            "That promo code has expired."
        );
        assert!(request_error_message(&anyhow::anyhow!("network down"), 4).contains("try again"));
    }

    #[test]
    fn checkout_request_a_hold_refused_by_mode_says_to_request() {
        assert!(
            booking_error_message(r#"{"code":"BOOKING_MODE_NOT_ALLOWED"}"#, 4).contains("request")
        );
    }
}
