use chrono::{Months, NaiveDate};
use common::booking_request::BookingMode;
use common::models::{ListingDetails, ReviewResponse};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{error::not_found, page, parse_query_params, path_param},
    view::{View, component, view},
};
use web_client::get_api_client;
use web_ui_common::components::{
    date_range_calendar::{CalendarProps, date_range_calendar, first_of_month},
    domain::{
        listing_card::ListingSummary,
        price_breakdown::price_breakdown,
        responsive_image::{responsive_image, variant_srcset, variant_url},
    },
    star_rating::star_rating,
};

use crate::quote::{NoQuote, public_quote};
use crate::support::{display_currency, guests_or_default, jamaica_today};
use common::app_client::booking_requests::{ApiCallError, api_error_code};

path_param!(slug);

/// Months shown side by side in the booking calendar.
const CALENDAR_MONTHS: u32 = 2;

#[derive(Deserialize, Default)]
struct StayQuery {
    check_in: Option<NaiveDate>,
    check_out: Option<NaiveDate>,
    guests: Option<i32>,
    month: Option<String>,
}

fn stay_query(cx: &Cx) -> StayQuery {
    parse_query_params::<StayQuery>(cx).unwrap_or_default()
}

/// A valid stay: check-out after check-in, check-in not in the past.
fn stay_dates(q: &StayQuery) -> Option<(NaiveDate, NaiveDate)> {
    match (q.check_in, q.check_out) {
        (Some(a), Some(b)) if b > a && a >= jamaica_today() => Some((a, b)),
        _ => None,
    }
}

/// Builds the calendar for `details`, starting at `month`.
async fn calendar_props(
    cx: &Cx,
    details: &ListingDetails,
    month: NaiveDate,
    q: &StayQuery,
) -> CalendarProps {
    let month = first_of_month(month);
    let until = month
        .checked_add_months(Months::new(CALENDAR_MONTHS))
        .unwrap_or(month);
    let ranges = get_api_client(cx)
        .get_unavailable_ranges(details.listing.id, month, until)
        .await
        .unwrap_or_else(|e| {
            // The server re-checks availability at quote and checkout.
            tracing::warn!("calendar availability unavailable: {e}");
            Vec::new()
        });
    CalendarProps {
        listing_slug: details.listing.slug.clone(),
        month,
        months: CALENDAR_MONTHS,
        today: jamaica_today(),
        ranges,
        check_in: q.check_in,
        check_out: q.check_out,
        check_in_id: "reserve-check-in".into(),
        check_out_id: "reserve-check-out".into(),
    }
}

fn checkout_href(slug: &str, q: &StayQuery, currency: Option<&str>) -> String {
    let mut href = format!("/checkout/{slug}");
    if let Some((a, b)) = stay_dates(q) {
        href.push_str(&format!("?check_in={a}&check_out={b}"));
        if let Some(g) = q.guests {
            href.push_str(&format!("&guests={g}"));
        }
        if let Some(c) = currency {
            href.push_str(&format!("&currency={c}"));
        }
    }
    href
}

/// The booking card's call to action for a listing's booking mode: the main
/// button's label and the note under it. Checkout offers the request option
/// itself, so every mode links to the same checkout page.
pub fn booking_cta(mode: BookingMode) -> (&'static str, &'static str) {
    match mode {
        BookingMode::Instant => (
            "Reserve",
            "Your dates are held while you complete checkout.",
        ),
        BookingMode::Request => (
            "Request to book",
            "The host approves each stay. You're not charged until you confirm.",
        ),
        BookingMode::Both => (
            "Reserve",
            "Book now, or send the host a request at checkout.",
        ),
    }
}

/// The enquiry form's URL, keeping the stay chosen so far.
fn enquire_href(slug: &str, q: &StayQuery) -> String {
    let mut href = format!("/listings/{slug}/enquire");
    if let Some((a, b)) = stay_dates(q) {
        href.push_str(&format!("?check_in={a}&check_out={b}"));
        if let Some(g) = q.guests {
            href.push_str(&format!("&guests={g}"));
        }
    }
    href
}

/// The quote for the chosen dates: `None` until both are chosen.
type StayQuote =
    Option<Result<web_ui_common::components::domain::price_breakdown::PriceBreakdown, NoQuote>>;

/// The quote area: price breakdown for valid dates (or why there is none),
/// the booking button and the "Ask a question" link.
#[component]
async fn quote_panel(
    cx: &Cx,
    slug: String,
    quote: StayQuote,
    href: String,
    mode: BookingMode,
    enquire: String,
) -> Result<impl View> {
    let (label, note) = booking_cta(mode);
    let (breakdown, no_quote) = match quote {
        Some(Ok(b)) => (Some(b), None),
        Some(Err(why)) => (None, Some(why.message())),
        None => (None, None),
    };
    Ok(view! {
        if let Some(b) = breakdown {
            <div id="quote-breakdown">price_breakdown(quote: b)</div>
        } else if let Some(why) = no_quote {
            <p id="quote-refusal" role="alert" class="text-sm text-destructive">
                (why)
            </p>
        } else {
            <p class="text-sm text-muted-foreground">
                "Choose check-in and check-out dates to see the total."
            </p>
        }
        <a
            id="listing-book-cta"
            href=(href)
            data-listing=(slug)
            data-booking-mode=(mode.as_str())
            class="flex h-11 w-full items-center justify-center rounded-[var(--radius-control,10px)] bg-primary text-[15px] font-medium text-primary-foreground no-underline"
        >
            (label)
        </a>
        <p class="text-center text-xs text-muted-foreground">(note)</p>
        <a
            id="listing-enquire-cta"
            href=(enquire)
            class="flex h-11 w-full items-center justify-center rounded-[var(--radius-control,10px)] border border-border text-[15px] font-medium no-underline"
        >
            "Ask a question"
        </a>
    })
}

/// A listing priced for the page.
struct Loaded {
    details: ListingDetails,
    /// The currency prices are in: the display currency, or `None` (the
    /// listing's own currency) when there is no rate to it.
    currency: Option<String>,
    /// The display currency that had no exchange rate, for the notice.
    no_rate_for: Option<String>,
}

/// The active listing in the guest's display currency. With no exchange rate
/// to that currency right now, the listing is shown in its own currency
/// (labelled as such) with a notice, rather than failing. Only a listing the
/// API cannot find, or an inactive one, is a 404.
async fn load(cx: &Cx, slug: &str) -> Result<Loaded> {
    let currency = display_currency(cx).await;
    let api = get_api_client(cx);
    let loaded = match api.get_listing_by_id(slug, currency.as_deref()).await {
        Ok(details) => Loaded {
            details,
            currency,
            no_rate_for: None,
        },
        Err(e) if api_error_code(&e) == Some("FX_RATE_UNAVAILABLE") => {
            tracing::warn!(slug, currency = ?currency, error = %e, "no exchange rate for the display currency; showing the listing's own currency");
            let details = api
                .get_listing_by_id(slug, None)
                .await
                .map_err(|e| listing_error(slug, None, e))?;
            Loaded {
                details,
                currency: None,
                no_rate_for: currency,
            }
        }
        Err(e) => return Err(listing_error(slug, currency.as_deref(), e)),
    };
    if loaded.details.listing.is_active {
        Ok(loaded)
    } else {
        Err(not_found().into())
    }
}

/// A 404 when the API has no such listing; otherwise the failure is logged
/// and shown as an error page (not a misleading "not found").
fn listing_error(slug: &str, currency: Option<&str>, e: anyhow::Error) -> topcoat::Error {
    let status = e.downcast_ref::<ApiCallError>().map(|c| c.status);
    if status == Some(404) {
        not_found().into()
    } else {
        tracing::error!(slug, currency, status, error = ?e, "failed to load listing");
        web_client::app_error(e).into()
    }
}

/// Most photos shown on the page; the rest are in the gallery dialog.
const GALLERY_TILES: usize = 5;

/// The listing's headline facts. A bed count of 0 means the host never
/// entered one, so it is left out rather than shown as "0 beds".
fn core_facts(max_guests: i32, bedrooms: i32, beds: i32, bathrooms: i32) -> Vec<String> {
    [
        Some(format!("Up to {max_guests} guests")),
        Some(format!("{bedrooms} bedrooms")),
        (beds > 0).then(|| format!("{beds} beds")),
        Some(format!("{bathrooms} bathrooms")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The photo grid for `count` photos: the section's classes and one class
/// list per tile shown. Phones show only the first photo; from `md` the
/// grid has a fixed height and the tiles fill it, so no column is left empty.
fn gallery_layout(count: usize) -> (&'static str, Vec<(&'static str, &'static str)>) {
    const SECTION: &str = "grid gap-3 md:h-[460px] md:grid-cols-4 md:grid-rows-2";
    // `sizes` per tile width: the page is capped at 1540px less 48px padding.
    const FULL: &str = "(min-width: 1540px) 1492px, 100vw";
    const HALF: &str = "(min-width: 1540px) 746px, (min-width: 768px) 50vw, 100vw";
    const QUARTER: &str = "(min-width: 1540px) 373px, (min-width: 768px) 25vw, 100vw";
    const LEAD: (&str, &str) = ("md:col-span-2 md:row-span-2", HALF);
    const HIDDEN_WIDE: (&str, &str) = ("hidden md:block md:col-span-2", HALF);
    const HIDDEN_SMALL: (&str, &str) = ("hidden md:block", QUARTER);
    let tiles = match count.min(GALLERY_TILES) {
        0 => Vec::new(),
        1 => vec![("md:col-span-4 md:row-span-2", FULL)],
        2 => vec![LEAD, ("hidden md:block md:col-span-2 md:row-span-2", HALF)],
        3 => vec![LEAD, HIDDEN_WIDE, HIDDEN_WIDE],
        4 => vec![LEAD, HIDDEN_WIDE, HIDDEN_SMALL, HIDDEN_SMALL],
        n => std::iter::once(LEAD)
            .chain(std::iter::repeat_n(HIDDEN_SMALL, n - 1))
            .collect(),
    };
    (SECTION, tiles)
}

/// Listing detail: gallery, facts, reviews and the booking card.
#[page("/listings/{slug}")]
pub async fn listing_detail_page(cx: &Cx) -> Result<impl View> {
    let slug: &str = slug(cx);
    let Loaded {
        details,
        currency,
        no_rate_for,
    } = load(cx, slug).await?;
    let q = stay_query(cx);
    let month = q.check_in.unwrap_or_else(jamaica_today);
    let (calendar, reviews) = tokio::join!(
        calendar_props(cx, &details, month, &q),
        get_api_client(cx).get_listing_reviews(details.listing.id, 1, 6)
    );
    let reviews: Vec<ReviewResponse> = reviews.unwrap_or_default();
    let summary = ListingSummary::from(&details.listing);
    let listing = details.listing.clone();
    let price = summary
        .price_text()
        .unwrap_or_else(|unavailable| unavailable);
    let max_guests = listing.max_guests.max(1);
    let default_guests = guests_or_default(q.guests).clamp(1, max_guests);
    let rating = details
        .rating_summary
        .as_ref()
        .and_then(|r| r.overall_rating);
    let review_count = details
        .rating_summary
        .as_ref()
        .map(|r| i64::from(r.review_count));
    let mut facts = core_facts(
        listing.max_guests,
        listing.bedrooms,
        listing.beds,
        listing.full_bathrooms,
    );
    facts.extend(
        listing
            .listing_details
            .as_ref()
            .and_then(|d| d.get("amenities"))
            .and_then(|a| a.as_array())
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string)),
    );
    let images: Vec<(String, String)> = details
        .images
        .iter()
        .map(|i| (i.url.clone(), listing.name.clone()))
        .collect();
    let gallery = gallery_layout(images.len());
    let quote_url = format!("/listings/{}/quote", listing.slug);
    let href = checkout_href(&listing.slug, &q, currency.as_deref());
    let enquire = enquire_href(&listing.slug, &q);
    let breakdown = initial_breakdown(cx, &details, &q, currency.as_deref()).await;
    Ok(view! {
        <article class="space-y-10">
            <header class="space-y-2">
                <h1 class="font-serif text-[clamp(36px,5vw,56px)] leading-tight">
                    (listing.name.clone())
                </h1>
                <p class="flex flex-wrap items-center gap-3 text-muted-foreground">
                    <span>(summary.location())</span>
                    star_rating(rating: rating, review_count: review_count)
                </p>
                if let Some(code) = no_rate_for {
                    <p
                        id="currency-notice"
                        role="status"
                        class="rounded-lg border border-border bg-muted/40 px-4 py-3 text-sm"
                    >
                        (format!(
                            "Prices in {code} aren't available right now, so they're shown in {}.",
                            listing.base_currency,
                        ))
                    </p>
                }
            </header>

            if !images.is_empty() {
                <section aria-label="Photos" class=(gallery.0)>
                    for (i, ((src, alt), (span, sizes))) in images
                        .iter()
                        .cloned()
                        .zip(gallery.1.iter().copied())
                        .enumerate() {
                        <button
                            type="button"
                            data-dialog-open="gallery-dialog"
                            aria-label=(format!(
                                "Open photo {} of {}",
                                i + 1,
                                images.len(),
                            ))
                            class=(format!(
                                "overflow-hidden rounded-[var(--radius-media,12px)] bg-photo-placeholder aspect-[4/3] md:aspect-auto {span}",
                            ))
                        >
                            responsive_image(
                                src: src,
                                alt: alt,
                                sizes: Some(sizes.to_string())
                            )
                        </button>
                    }
                </section>
                <dialog
                    id="gallery-dialog"
                    aria-label="Photo gallery"
                    class="m-auto h-[94dvh] max-h-none w-[96vw] max-w-none overflow-y-auto rounded-[var(--radius-surface,16px)] bg-card p-4 text-card-foreground backdrop:bg-black/70 sm:p-6"
                >
                    <div class="flex items-center justify-between gap-4">
                        <p class="text-sm text-muted-foreground">
                            (format!(
                                "{} photos · select one to view it full size",
                                images.len(),
                            ))
                        </p>
                        <button
                            type="button"
                            data-dialog-close=""
                            class="h-11 rounded-full border border-border px-4 text-sm"
                        >
                            "Close"
                        </button>
                    </div>
                    <div
                        id="gallery-grid"
                        data-lightbox-group="photo-viewer"
                        class="mt-4 grid grid-cols-1 gap-3 md:grid-cols-2 md:gap-4"
                    >
                        for (i, (src, alt)) in images.iter().cloned().enumerate() {
                            <button
                                type="button"
                                data-lightbox-index=(i.to_string())
                                data-full-src=(variant_url(&src, "highres").unwrap_or_else(
                                    || src.clone(),
                                ))
                                data-full-srcset=(variant_srcset(&src).unwrap_or_default())
                                data-alt=(alt.clone())
                                aria-label=(format!(
                                    "View photo {} of {} full size",
                                    i + 1,
                                    images.len(),
                                ))
                                class="block aspect-[4/3] cursor-zoom-in overflow-hidden rounded-[var(--radius-media,12px)] bg-photo-placeholder"
                            >
                                responsive_image(
                                    src: src,
                                    alt: alt,
                                    sizes: Some("(min-width: 768px) 48vw, 96vw".to_string())
                                )
                            </button>
                        }
                    </div>
                </dialog>
                <dialog
                    id="photo-viewer"
                    data-lightbox=""
                    aria-label="Photo viewer"
                    class="m-0 h-dvh max-h-none w-screen max-w-none bg-black/95 p-0 text-white backdrop:bg-black/80"
                >
                    <div
                        class="relative flex h-full w-full items-center justify-center"
                    >
                        <img
                            id="photo-viewer-img"
                            data-lightbox-img=""
                            sizes="92vw"
                            alt=""
                            class="max-h-[88dvh] max-w-[92vw] object-contain"
                        />
                        if images.len() > 1 {
                            <button
                                type="button"
                                data-lightbox-step="-1"
                                aria-label="Previous photo"
                                class="absolute top-1/2 left-3 size-12 -translate-y-1/2 rounded-full bg-white/15 text-3xl leading-none hover:bg-white/25"
                            >
                                "‹"
                            </button>
                            <button
                                type="button"
                                data-lightbox-step="1"
                                aria-label="Next photo"
                                class="absolute top-1/2 right-3 size-12 -translate-y-1/2 rounded-full bg-white/15 text-3xl leading-none hover:bg-white/25"
                            >
                                "›"
                            </button>
                        }
                        <p
                            id="photo-viewer-counter"
                            data-lightbox-counter=""
                            aria-live="polite"
                            class="absolute bottom-4 left-1/2 -translate-x-1/2 text-sm"
                        ></p>
                        <button
                            type="button"
                            data-dialog-close=""
                            aria-label="Close photo viewer"
                            class="absolute top-3 right-3 h-11 rounded-full bg-white/15 px-4 text-sm hover:bg-white/25"
                        >
                            "Close"
                        </button>
                    </div>
                </dialog>
            }

            <div class="grid gap-12 lg:grid-cols-[1fr_380px]">
                <div class="space-y-10">
                    <p class="text-muted-foreground">(summary.specs())</p>
                    if let Some(description) = listing.description.clone() {
                        <p class="text-lg leading-relaxed">(description)</p>
                    }
                    if let Some(host) = details.host_name.clone() {
                        <p class="text-sm text-muted-foreground">
                            "Hosted by "
                            (host)
                        </p>
                    }
                    <section
                        aria-labelledby="offers-heading"
                        class="space-y-4 border-t border-border pt-8"
                    >
                        <h2 id="offers-heading" class="font-serif text-2xl">
                            "What this place offers"
                        </h2>
                        <ul class="grid gap-2 sm:grid-cols-2">
                            for fact in facts {
                                <li class="text-sm">(fact)</li>
                            }
                        </ul>
                    </section>
                    <section
                        aria-labelledby="reviews-heading"
                        class="space-y-4 border-t border-border pt-8"
                    >
                        <h2 id="reviews-heading" class="font-serif text-2xl">
                            "Verified Guest Reviews"
                        </h2>
                        if reviews.is_empty() {
                            <p class="text-sm text-muted-foreground">
                                "No reviews yet. Reviews come only from guests who stayed."
                            </p>
                        } else {
                            <ul class="space-y-6">
                                for review in reviews {
                                    <li class="space-y-1">
                                        <p class="flex items-center gap-2 text-sm font-medium">
                                            (review.guest_first_name.clone())
                                            star_rating(rating: Some(review.overall_rating))
                                        </p>
                                        if let Some(text) = review.public_review_text.clone() {
                                            <p class="text-sm">(text)</p>
                                        }
                                        if let Some(reply) = review.host_reply_text.clone() {
                                            <p
                                                class="border-l-2 border-border pl-3 text-sm text-muted-foreground"
                                            >
                                                "Host reply: "
                                                (reply)
                                            </p>
                                        }
                                    </li>
                                }
                            </ul>
                        }
                    </section>
                </div>

                <aside
                    aria-label="Book this stay"
                    class="h-fit space-y-4 rounded-[var(--radius-surface,16px)] border border-border bg-card p-6 text-card-foreground lg:sticky lg:top-6"
                >
                    <p>
                        <span class="text-2xl font-medium">(price)</span>
                        <span class="text-muted-foreground">" per night"</span>
                    </p>
                    if listing.booking_mode == BookingMode::Request {
                        <span
                            id="listing-approval-badge"
                            class="inline-flex rounded-full border border-border px-3 py-1 text-xs font-medium"
                        >
                            "Host approval required"
                        </span>
                    }
                    // novalidate: htmx validates forms even for GET, and a check-in
                    // without a check-out yet must still refresh the quote (which
                    // clears the breakdown for the previous dates).
                    <form
                        id="reserve-form"
                        novalidate=(true)
                        hx-get=(quote_url)
                        hx-trigger="change"
                        hx-target="#quote"
                        hx-swap="innerHTML"
                        class="space-y-4"
                    >
                        if let Some(c) = currency.clone() {
                            <input type="hidden" name="currency" value=(c) />
                        }
                        date_range_calendar(props: calendar)
                        <label
                            class="block text-xs font-semibold tracking-wide text-muted-foreground uppercase"
                        >
                            "Guests"
                            <select
                                name="guests"
                                class="mt-1 block h-11 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground normal-case"
                            >
                                for n in 1..=max_guests {
                                    <option
                                        value=(n.to_string())
                                        selected=(n == default_guests)
                                    >
                                        (if n == 1 {
                                            "1 Guest".to_string()
                                        } else {
                                            format!("{n} Guests")
                                        })
                                    </option>
                                }
                            </select>
                        </label>
                    </form>
                    <div id="quote" aria-live="polite" class="space-y-3">
                        quote_panel(
                            slug: listing.slug.clone(),
                            quote: breakdown,
                            href: href,
                            mode: listing.booking_mode,
                            enquire: enquire
                        )
                    </div>
                </aside>
            </div>
        </article>
    })
}

/// The quote for the request's dates, if they form a valid stay.
async fn initial_breakdown(
    cx: &Cx,
    details: &ListingDetails,
    q: &StayQuery,
    currency: Option<&str>,
) -> StayQuote {
    let (check_in, check_out) = stay_dates(q)?;
    let guests = u32::try_from(guests_or_default(q.guests))
        .unwrap_or(1)
        .max(1);
    Some(
        public_quote(
            cx,
            details.listing.id,
            check_in,
            check_out,
            guests,
            currency,
        )
        .await,
    )
}

/// Quote fragment for the booking card (`hx-get` on date/guest change).
#[page("/listings/{slug}/quote")]
pub async fn listing_quote(cx: &Cx) -> Result<impl View> {
    let slug: &str = slug(cx);
    let Loaded {
        details, currency, ..
    } = load(cx, slug).await?;
    let q = stay_query(cx);
    let breakdown = initial_breakdown(cx, &details, &q, currency.as_deref()).await;
    let href = checkout_href(&details.listing.slug, &q, currency.as_deref());
    let slug = details.listing.slug.clone();
    let mode = details.listing.booking_mode;
    let enquire = enquire_href(&slug, &q);
    Ok(view! {
        quote_panel(
            slug: slug,
            quote: breakdown,
            href: href,
            mode: mode,
            enquire: enquire
        )
    })
}

/// Calendar fragment for prev/next month navigation.
#[page("/listings/{slug}/calendar")]
pub async fn listing_calendar(cx: &Cx) -> Result<impl View> {
    let slug: &str = slug(cx);
    let details = load(cx, slug).await?.details;
    let q = stay_query(cx);
    let month = q
        .month
        .as_deref()
        .and_then(|m| NaiveDate::parse_from_str(&format!("{m}-01"), "%Y-%m-%d").ok())
        .unwrap_or_else(jamaica_today);
    let props = calendar_props(cx, &details, month, &q).await;
    Ok(view! { date_range_calendar(props: props) })
}

#[cfg(test)]
mod booking_mode_cta {
    use super::{BookingMode, booking_cta};

    #[test]
    fn booking_mode_cta_labels_follow_the_mode() {
        assert_eq!(booking_cta(BookingMode::Instant).0, "Reserve");
        assert_eq!(booking_cta(BookingMode::Request).0, "Request to book");
        assert_eq!(booking_cta(BookingMode::Both).0, "Reserve");
        assert!(booking_cta(BookingMode::Both).1.contains("request"));
        assert!(booking_cta(BookingMode::Request).1.contains("not charged"));
    }
}

#[cfg(test)]
mod facts_tests {
    use super::core_facts;

    #[test]
    fn a_missing_bed_count_is_left_out() {
        let facts = core_facts(10, 5, 0, 5);
        assert_eq!(facts, ["Up to 10 guests", "5 bedrooms", "5 bathrooms"]);
        assert!(core_facts(10, 5, 6, 5).contains(&"6 beds".to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::gallery_layout;

    /// Grid cells each tile covers from `md` (4 columns x 2 rows).
    fn cells(tile: &str) -> usize {
        let span = |prefix: &str| {
            tile.split_whitespace()
                .find_map(|c| c.strip_prefix(prefix))
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(1)
        };
        span("md:col-span-") * span("md:row-span-")
    }

    #[test]
    fn every_photo_count_fills_the_grid() {
        for count in 1..=8 {
            let (_, tiles) = gallery_layout(count);
            assert_eq!(tiles.len(), count.min(5), "{count} photos");
            assert_eq!(
                tiles.iter().map(|(t, _)| cells(t)).sum::<usize>(),
                8,
                "{count} photos"
            );
        }
        assert!(gallery_layout(0).1.is_empty());
    }

    #[test]
    fn phones_show_only_the_first_photo() {
        let (_, tiles) = gallery_layout(5);
        assert!(!tiles[0].0.contains("hidden"));
        assert!(tiles[1..].iter().all(|(t, _)| t.starts_with("hidden ")));
    }

    /// Each tile's `sizes` matches its width, so the browser loads the photo
    /// variant nearest the drawn size rather than one for the whole screen.
    #[test]
    fn tile_sizes_match_their_column_span() {
        for count in 1..=8 {
            for (tile, sizes) in gallery_layout(count).1 {
                let cols = tile
                    .split_whitespace()
                    .find_map(|c| c.strip_prefix("md:col-span-"))
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(1);
                let capped = format!("(min-width: 1540px) {}px", 1492 / 4 * cols);
                let md = format!("(min-width: 768px) {}vw", cols * 25);
                assert!(sizes.starts_with(&capped), "{count}: {tile} -> {sizes}");
                assert!(
                    cols == 4 || sizes.contains(&md),
                    "{count}: {tile} -> {sizes}"
                );
                assert!(sizes.ends_with("100vw"), "{count}: {tile} -> {sizes}");
            }
        }
    }
}
