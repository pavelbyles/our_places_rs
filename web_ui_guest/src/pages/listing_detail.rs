use chrono::{Months, NaiveDate};
use common::booking_request::BookingMode;
use common::listing_details::{
    CuratedTestimonial, Ribbon, ServiceAddon, StructuredListingDetails, SuiteItem,
    amenity_groups_for, drive_time,
};
use common::location::LocationPrecision;
use common::models::{ListingDetails, ListingImageResponse, ListingResponse, ReviewResponse};
use common::query::urlencode;
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{error::not_found, page, parse_query_params, path_param},
    view::{View, attributes, component, view},
};
use web_client::get_api_client;
use web_ui_common::components::{
    amenity_group::{amenity_groups, check_list},
    date_range_calendar::{CalendarProps, date_range_calendar, first_of_month},
    domain::{
        listing_card::ListingSummary,
        price_breakdown::price_breakdown,
        responsive_image::{responsive_image, variant_srcset, variant_url},
    },
    highlights_ribbon::highlights_ribbon,
    listing_hero::listing_hero,
    map::{MapKind, MapPoint, map_container},
    section_nav::{PILL, PILL_SM, section_nav},
    star_rating::star_rating,
    suite_card::suites_grid,
};
use web_ui_common::map_config::map_config;

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

/// The description as paragraphs, split on blank lines.
fn paragraphs(description: Option<&str>) -> Vec<String> {
    description
        .unwrap_or_default()
        .replace("\r\n", "\n")
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

/// Each suite with its photo: the listing image its `image_id` names. An
/// id that isn't one of this listing's images gives no photo.
fn suites_with_photos(
    suites: &[SuiteItem],
    images: &[ListingImageResponse],
) -> Vec<(SuiteItem, Option<String>)> {
    suites
        .iter()
        .map(|suite| {
            let photo = suite.image_id.and_then(|id| {
                images
                    .iter()
                    .find(|image| image.id == id)
                    .map(|image| image.url.clone())
            });
            (suite.clone(), photo)
        })
        .collect()
}

/// `href` with `topic` added as a query parameter.
fn with_topic(href: &str, topic: &str) -> String {
    let sep = if href.contains('?') { '&' } else { '?' };
    format!("{href}{sep}topic={}", urlencode(topic))
}

/// An add-on's price in the listing's currency, e.g. "US$180 each way",
/// "US$220 / day", or "On request" without a price.
fn addon_price(addon: &ServiceAddon, currency: &str) -> String {
    match addon.price {
        Some(price) => {
            let amount = if price.fract().is_zero() {
                common::money::grouped_whole(price)
            } else {
                common::money::grouped(price)
            };
            let unit = addon
                .unit
                .as_deref()
                .map(str::trim)
                .filter(|u| !u.is_empty())
                .map(|u| {
                    let lower = u.to_lowercase();
                    if lower.starts_with("each") || lower.starts_with("per") {
                        format!(" {u}")
                    } else {
                        format!(" / {u}")
                    }
                })
                .unwrap_or_default();
            format!("{}{amount}{unit}", common::money::price_prefix(currency))
        }
        None => "On request".to_string(),
    }
}

/// Most guestbook quotes shown.
const GUESTBOOK_SHOWN: usize = 4;

/// "Danielle · Toronto · March 2026".
fn guestbook_credit(quote: &CuratedTestimonial) -> String {
    [
        Some(quote.guest_name.trim()),
        quote.location.as_deref().map(str::trim),
        Some(quote.stay_date.trim()),
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" · ")
}

/// A photo's alt text: the host's name for it, or "{listing}, photo N".
fn photo_alt(listing_name: &str, index: usize, name: Option<&str>) -> String {
    name.map(str::trim).filter(|n| !n.is_empty()).map_or_else(
        || format!("{listing_name}, photo {}", index + 1),
        str::to_string,
    )
}

/// The listing's public point for the map (spec 103 §1.7). Coordinates
/// outside the mapped area are logged, without the coordinates: the host
/// entered something the map can't show.
fn listing_map_point(listing: &ListingResponse) -> Option<MapPoint> {
    let point = MapPoint::of_listing(listing);
    if point.is_none() && listing.latitude.is_some() && listing.longitude.is_some() {
        tracing::warn!(listing_id = %listing.id, "listing coordinates are outside the mapped area");
    }
    point
}

/// The line under the location heading.
fn location_note(point: Option<&MapPoint>) -> &'static str {
    match point.map(|p| p.precision) {
        Some(LocationPrecision::Exact) => "Exact location shown.",
        _ => "The exact address is shared after you book.",
    }
}

/// The map box of `#location`: the map when there is a point and maps are on,
/// the decorative placeholder when maps are off, and "Location not mapped"
/// when there is no point. All three keep the same `min-h-56` box.
#[component]
async fn location_map(cx: &Cx, point: Option<MapPoint>) -> Result<impl View> {
    const BOX: &str = "relative min-h-56 overflow-hidden rounded-[var(--radius-media,12px)]";
    let maps_on = map_config(cx).is_some();
    Ok(view! {
        if let Some(point) = point {
            if maps_on {
                map_container(
                    kind: MapKind::Listing,
                    point: Some(point),
                    label: Some("Map of the listing's area".to_string()),
                    attrs: attributes! { class="min-h-56 h-full" }
                )
            } else {
                <div aria-hidden="true" class=(format!("{BOX} bg-linear-to-br from-chip to-photo-placeholder"))>
                    <span
                        class="absolute top-1/2 left-1/2 size-14 -translate-1/2 rounded-full bg-primary/20"
                    ></span>
                    <span
                        class="absolute top-1/2 left-1/2 size-3.5 -translate-1/2 rounded-full bg-primary ring-4 ring-card"
                    ></span>
                </div>
            }
        } else {
            <div class=(format!("{BOX} grid place-items-center bg-muted"))>
                <p data-testid="location-not-mapped" class="text-sm text-muted-foreground">
                    "Location not mapped"
                </p>
            </div>
        }
    })
}

/// Which optional sections the page shows.
#[derive(Default)]
struct Sections {
    suites: bool,
    services: bool,
    policies: bool,
    reviews: bool,
    location: bool,
    photos: bool,
}

/// Section nav links, in page order, for the sections that are shown.
fn section_links(s: &Sections) -> Vec<(&'static str, &'static str)> {
    [
        (true, ("overview", "Overview")),
        (s.suites, ("suites", "Suites")),
        (true, ("amenities", "Amenities")),
        (s.services, ("services", "Services")),
        (s.policies, ("policies", "Policies")),
        (s.reviews, ("reviews", "Reviews")),
        (s.location, ("location", "Location")),
        (s.photos, ("photos", "Photos")),
    ]
    .into_iter()
    .filter_map(|(shown, link)| shown.then_some(link))
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
    let facts = core_facts(
        listing.max_guests,
        listing.bedrooms,
        listing.beds,
        listing.full_bathrooms,
    )
    .join(" · ");
    let structured = StructuredListingDetails::from_json(listing.listing_details.as_ref());
    let ribbon = {
        let _listing = tracing::info_span!("listing_ribbon", listing_id = %listing.id).entered();
        Ribbon::resolve(
            structured.ribbon.clone(),
            Ribbon::fallback(
                listing.bedrooms,
                listing.full_bathrooms,
                listing.half_bathrooms,
                listing.max_guests,
            ),
        )
    };
    let groups = amenity_groups_for(structured.amenity_groups.as_deref(), &structured.amenities);
    let images: Vec<(String, String)> = details
        .images
        .iter()
        .enumerate()
        .map(|(n, i)| {
            (
                i.url.clone(),
                photo_alt(&listing.name, n, i.name.as_deref()),
            )
        })
        .collect();
    // Shown only in the full-size viewer, and only for named photos.
    let captions: Vec<String> = details
        .images
        .iter()
        .map(|i| {
            i.name
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    let gallery = gallery_layout(images.len());
    let intro_photo = images.get(1).cloned();
    let suites = suites_with_photos(&structured.suites, &details.images);
    let suites_heading = match suites.len() {
        1 => "One suite".to_string(),
        n => format!("{n} suites"),
    };
    let tiles_shown = gallery.1.len();
    let eyebrow = format!("The {}", listing.listing_structure.to_lowercase());
    let story = paragraphs(listing.description.as_deref());
    let enquire = enquire_href(&listing.slug, &q);
    let services = structured.staff_services.clone();
    let addons: Vec<(String, String, String)> = services
        .addons
        .iter()
        .map(|a| {
            (
                a.name.clone(),
                addon_price(a, &listing.base_currency),
                with_topic(&enquire, &a.name),
            )
        })
        .collect();
    let arrange_href = with_topic(&enquire, "concierge add-ons");
    let policies = structured.policies.rows();
    let nearby: Vec<(String, String)> = structured
        .nearby
        .iter()
        .map(|p| (p.name.clone(), drive_time(p.drive_minutes)))
        .collect();
    let guestbook: Vec<CuratedTestimonial> = structured
        .curated_testimonials
        .iter()
        .filter(|q| !q.quote.trim().is_empty())
        .take(GUESTBOOK_SHOWN)
        .cloned()
        .collect();
    let map_point = listing_map_point(&details.listing);
    let links = section_links(&Sections {
        services: !services.is_empty(),
        policies: !policies.is_empty(),
        location: !nearby.is_empty() || map_point.is_some(),
        suites: !suites.is_empty(),
        reviews: !reviews.is_empty() || !guestbook.is_empty(),
        photos: !images.is_empty(),
    });
    let quote_url = format!("/listings/{}/quote", listing.slug);
    let href = checkout_href(&listing.slug, &q, currency.as_deref());
    let cta_enquire = enquire.clone();
    let bar_price = price.clone();
    let breakdown = initial_breakdown(cx, &details, &q, currency.as_deref()).await;
    Ok(view! {
        <article class="page-flow">
            listing_hero(
                name: listing.name.clone(),
                location: summary.location(),
                rating: rating,
                review_count: review_count,
                photo: images.first().map(|(src, _)| src.clone()),
                photo_alt: images.first().map(|(_, alt)| alt.clone()).unwrap_or_default(),
                photo_count: images.len()
            )
            <div
                data-mobile-book-bar=""
                class="fixed inset-x-0 bottom-0 z-30 flex items-center justify-between gap-3 border-t border-border bg-card px-6 pt-3 pb-[calc(0.75rem+env(safe-area-inset-bottom,0px))] text-card-foreground lg:hidden"
            >
                <span>
                    <strong class="font-serif text-xl font-normal">(bar_price)</strong>
                    <span class="text-sm text-muted-foreground">" / night"</span>
                </span>
                <a
                    href="#book"
                    class="inline-flex h-10 items-center rounded-full bg-primary px-5 text-sm font-medium text-primary-foreground no-underline"
                >
                    "Check dates"
                </a>
            </div>
            <div id="listing-body" class="full-bleed">
                section_nav(links: links, price: price.clone())
                <div class="page-columns gap-y-12 lg:gap-y-16">
                    if let Some(code) = no_rate_for {
                        <p
                            id="currency-notice"
                            role="status"
                            class="mt-8 rounded-lg border border-border bg-muted/40 px-4 py-3 text-sm"
                        >
                            (format!(
                                "Prices in {code} aren't available right now, so they're shown in {}.",
                                listing.base_currency,
                            ))
                        </p>
                    }
                    <section
                        id="overview"
                        aria-labelledby="overview-heading"
                        class=(format!(
                            "grid scroll-mt-24 items-center gap-10 pt-10 lg:gap-14 lg:pt-16 {}",
                            if intro_photo.is_some() { "lg:grid-cols-[1.05fr_1fr]" } else { "" },
                        ))
                    >
                        <div class="min-w-0">
                            <p
                                class="text-xs font-medium tracking-[0.22em] text-primary uppercase"
                            >
                                (eyebrow)
                            </p>
                            if let Some(quote) = structured.pull_quote.clone() {
                                <h2
                                    id="overview-heading"
                                    class="mt-3.5 mb-6 font-serif text-[clamp(30px,3.6vw,46px)] leading-tight font-light text-balance italic"
                                >
                                    (quote)
                                </h2>
                            } else {
                                <h2
                                    id="overview-heading"
                                    class="mt-3.5 mb-6 font-serif text-[clamp(28px,3vw,38px)] leading-tight"
                                >
                                    (format!("About {}", listing.name))
                                </h2>
                            }
                            <div
                                data-testid="listing-story"
                                class="grid max-w-[60ch] gap-4 text-[17px] leading-relaxed text-muted-foreground"
                            >
                                for paragraph in story {
                                    <p>(paragraph)</p>
                                }
                            </div>
                            <div class="mt-8 flex flex-wrap gap-3.5">
                                if !structured.suites.is_empty() {
                                    <a href="#suites" class=(PILL)>"Explore suites"</a>
                                }
                                <a href="#book" class=(PILL)>"Check availability"</a>
                            </div>
                            if let Some(host) = details.host_name.clone() {
                                <p class="mt-7 text-sm text-muted-foreground">
                                    "Hosted by "
                                    <span class="text-foreground">(host)</span>
                                </p>
                            }
                        </div>
                        if let Some((src, alt)) = intro_photo {
                            <div
                                class="relative aspect-[4/3] max-w-full overflow-hidden rounded-[var(--radius-media,12px)] bg-photo-placeholder lg:aspect-[4/5]"
                            >
                                responsive_image(
                                    src: src,
                                    alt: alt,
                                    sizes: Some("(min-width: 1024px) 45vw, 92vw".to_string())
                                )
                            </div>
                        }
                    </section>

            highlights_ribbon(ribbon: ribbon)

            <div class="grid gap-12 lg:grid-cols-[1fr_380px]">
                <div class="min-w-0 space-y-16">
                    if !suites.is_empty() {
                        <section
                            id="suites"
                            aria-labelledby="suites-heading"
                            class="scroll-mt-24 space-y-6"
                        >
                            <div class="space-y-2.5">
                                <p
                                    class="text-xs font-medium tracking-[0.22em] text-primary uppercase"
                                >
                                    "Sleeping arrangements"
                                </p>
                                <h2
                                    id="suites-heading"
                                    class="font-serif text-[clamp(28px,3vw,38px)]"
                                >
                                    (suites_heading)
                                </h2>
                            </div>
                            suites_grid(suites: suites)
                        </section>
                    }
                    <section
                        id="amenities"
                        aria-labelledby="amenities-heading"
                        class="scroll-mt-24 space-y-4"
                    >
                        <h2 id="amenities-heading" class="font-serif text-2xl">
                            "What this place offers"
                        </h2>
                        <p id="listing-facts" class="text-sm text-muted-foreground">(facts)</p>
                        amenity_groups(groups: groups)
                    </section>
                    if !services.is_empty() {
                        <section
                            id="services"
                            aria-labelledby="services-heading"
                            class="scroll-mt-24 space-y-6 border-t border-border pt-12"
                        >
                            <div class="space-y-2.5">
                                <p
                                    class="text-xs font-medium tracking-[0.22em] text-primary uppercase"
                                >
                                    "Staff & services"
                                </p>
                                <h2
                                    id="services-heading"
                                    class="font-serif text-[clamp(28px,3vw,38px)]"
                                >
                                    "Looked after from arrival to departure"
                                </h2>
                            </div>
                            <div class="grid gap-5 md:grid-cols-2">
                                if !services.included.is_empty() {
                                    <div
                                        data-testid="services-included"
                                        class="grid content-start gap-4 rounded-[var(--radius-surface,16px)] border border-border bg-card p-6 text-card-foreground"
                                    >
                                        <h3 class="font-serif text-[22px]">
                                            "Included with your stay"
                                        </h3>
                                        check_list(items: services.included.clone())
                                        if let Some(note) = services.included_note.clone() {
                                            <p class="text-[13px] text-muted-foreground">(note)</p>
                                        }
                                    </div>
                                }
                                if !addons.is_empty() {
                                    <div
                                        data-testid="services-addons"
                                        class="grid content-start gap-3 rounded-[var(--radius-surface,16px)] border border-dashed border-border p-6"
                                    >
                                        <h3 class="font-serif text-[22px]">"Concierge add-ons"</h3>
                                        <ul class="divide-y divide-divider text-sm">
                                            for (name, price, ask) in addons {
                                                <li
                                                    class="flex items-baseline justify-between gap-3 py-2.5"
                                                >
                                                    <a href=(ask) class="text-foreground no-underline hover:underline">
                                                        (name)
                                                    </a>
                                                    <span class="shrink-0 font-medium tabular-nums">
                                                        (price)
                                                    </span>
                                                </li>
                                            }
                                        </ul>
                                        <a href=(arrange_href) class=(format!("{PILL_SM} w-fit"))>
                                            "Ask us to arrange"
                                        </a>
                                    </div>
                                }
                            </div>
                        </section>
                    }
                    if !policies.is_empty() {
                        <section
                            id="policies"
                            aria-labelledby="policies-heading"
                            class="scroll-mt-24 space-y-6 border-t border-border pt-12"
                        >
                            <div class="space-y-2.5">
                                <p
                                    class="text-xs font-medium tracking-[0.22em] text-primary uppercase"
                                >
                                    "Before you book"
                                </p>
                                <h2
                                    id="policies-heading"
                                    class="font-serif text-[clamp(28px,3vw,38px)]"
                                >
                                    "House policies"
                                </h2>
                            </div>
                            <dl
                                data-testid="house-policies"
                                class="grid overflow-hidden rounded-[var(--radius-surface,16px)] border border-border bg-card text-card-foreground sm:grid-cols-2"
                            >
                                for (label, text) in policies {
                                    <div
                                        class="grid gap-1 border-b border-divider px-5 py-4 last:border-b-0 sm:odd:border-r sm:[&:nth-last-child(2):nth-child(odd)]:border-b-0"
                                    >
                                        <dt
                                            class="text-xs tracking-[0.14em] text-muted-foreground uppercase"
                                        >
                                            (label)
                                        </dt>
                                        <dd>(text)</dd>
                                    </div>
                                }
                            </dl>
                        </section>
                    }
                    if !reviews.is_empty() || !guestbook.is_empty() {
                        <section
                            id="reviews"
                            aria-labelledby="reviews-heading"
                            class="scroll-mt-24 space-y-6 border-t border-border pt-12"
                        >
                            <div class="space-y-2.5">
                                <p
                                    class="text-xs font-medium tracking-[0.22em] text-primary uppercase"
                                >
                                    "Guests"
                                </p>
                                <h2
                                    id="reviews-heading"
                                    class="font-serif text-[clamp(28px,3vw,38px)]"
                                >
                                    if reviews.is_empty() {
                                        "What guests say"
                                    } else {
                                        "Verified Guest Reviews"
                                    }
                                </h2>
                            </div>
                            if reviews.is_empty() {
                                <div data-testid="guestbook" class="grid gap-6 sm:grid-cols-2">
                                    for quote in guestbook {
                                        <figure
                                            class="m-0 grid content-start gap-2.5 border-l-2 border-primary py-1 pl-5"
                                        >
                                            <blockquote
                                                class="m-0 font-serif text-xl leading-snug font-light italic"
                                            >
                                                (format!("\u{201c}{}\u{201d}", quote.quote.trim()))
                                            </blockquote>
                                            <figcaption class="flex flex-wrap items-center gap-2 text-[13px] text-muted-foreground">
                                                (guestbook_credit(&quote))
                                                star_rating(rating: Some(f64::from(quote.rating.clamp(1, 5))))
                                            </figcaption>
                                        </figure>
                                    }
                                </div>
                                <p
                                    data-testid="guestbook-notice"
                                    class="rounded-[var(--radius-control,10px)] bg-chip px-3.5 py-2.5 text-[13px] text-muted-foreground"
                                >
                                    "From the villa guestbook. Verified reviews from bookings on Our Places will appear here as they come in."
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
                    }
                    if !nearby.is_empty() || map_point.is_some() {
                        <section
                            id="location"
                            aria-labelledby="location-heading"
                            class="scroll-mt-24 space-y-6 border-t border-border pt-12"
                        >
                            <div class="space-y-2.5">
                                <p
                                    class="text-xs font-medium tracking-[0.22em] text-primary uppercase"
                                >
                                    "Location"
                                </p>
                                <h2
                                    id="location-heading"
                                    class="font-serif text-[clamp(28px,3vw,38px)]"
                                >
                                    (summary.location())
                                </h2>
                                <p class="text-sm text-muted-foreground">
                                    (location_note(map_point.as_ref()))
                                </p>
                            </div>
                            <div class="grid gap-6 md:grid-cols-[1.2fr_1fr]">
                                location_map(point: map_point)
                                if !nearby.is_empty() {
                                    <ul data-testid="drive-times" class="divide-y divide-divider text-sm">
                                        for (name, time) in nearby {
                                            <li class="flex justify-between gap-3 py-3">
                                                <span>(name)</span>
                                                <span class="shrink-0 font-medium tabular-nums">(time)</span>
                                            </li>
                                        }
                                    </ul>
                                }
                            </div>
                        </section>
                    }
                </div>

                <aside
                    id="book"
                    aria-label="Book this stay"
                    class="h-fit space-y-4 rounded-[var(--radius-surface,16px)] border border-border bg-card p-6 text-card-foreground scroll-mt-24 lg:sticky lg:top-20"
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
            if !images.is_empty() {
                <section
                    id="photos"
                    aria-labelledby="photos-heading"
                    class="scroll-mt-24 space-y-6 border-t border-border pt-12"
                >
                    <div class="flex flex-wrap items-end justify-between gap-4">
                        <div class="space-y-2.5">
                            <p
                                class="text-xs font-medium tracking-[0.22em] text-primary uppercase"
                            >
                                "Gallery"
                            </p>
                            <h2 id="photos-heading" class="font-serif text-[clamp(28px,3vw,38px)]">
                                "Around the house"
                            </h2>
                        </div>
                        <button type="button" data-dialog-open="gallery-dialog" class=(PILL_SM)>
                            (format!("All {} photos", images.len()))
                        </button>
                    </div>
                    <div data-testid="photo-mosaic" class=(gallery.0)>
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
                                    "group relative overflow-hidden rounded-[var(--radius-media,12px)] bg-photo-placeholder aspect-[4/3] md:aspect-auto {span}",
                                ))
                            >
                                responsive_image(
                                    src: src,
                                    alt: alt,
                                    sizes: Some(sizes.to_string()),
                                    attrs: attributes! {
                                        class="transition-transform duration-700 ease-out group-hover:scale-105 motion-reduce:transition-none"
                                    }
                                )
                                if i + 1 == tiles_shown && images.len() > tiles_shown {
                                    <span
                                        class="absolute right-3 bottom-3 rounded-full bg-card px-3.5 py-1.5 text-sm text-card-foreground"
                                    >
                                        (format!("All {} photos", images.len()))
                                    </span>
                                }
                            </button>
                        }
                    </div>
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
                                data-caption=(captions.get(i).cloned().unwrap_or_default())
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
                            id="photo-viewer-caption"
                            data-lightbox-caption=""
                            aria-live="polite"
                            class="absolute bottom-11 left-1/2 max-w-[80vw] -translate-x-1/2 text-center text-base empty:hidden"
                        ></p>
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

                    <section
                        data-testid="plan-your-stay"
                        aria-labelledby="plan-heading"
                        class="full-bleed bg-primary text-primary-foreground"
                    >
                        <div
                            class="bleed-inner grid justify-items-center gap-5 py-16 text-center lg:py-24"
                        >
                            <p class="text-xs font-medium tracking-[0.22em] uppercase opacity-80">
                                "Plan your stay"
                            </p>
                            <h2
                                id="plan-heading"
                                class="max-w-[22ch] font-serif text-[clamp(30px,4vw,50px)] leading-tight font-light text-balance italic"
                            >
                                "Tell us your dates and we'll take care of the rest"
                            </h2>
                            <div class="flex flex-wrap justify-center gap-3.5">
                                <a
                                    href="#book"
                                    class="inline-flex h-11 items-center rounded-full border border-primary-foreground px-6 text-xs font-medium tracking-[0.16em] text-primary-foreground uppercase no-underline transition-colors hover:bg-primary-foreground hover:text-primary"
                                >
                                    "Check availability"
                                </a>
                                <a
                                    href=(cta_enquire)
                                    class="inline-flex h-11 items-center rounded-full px-6 text-xs font-medium tracking-[0.16em] text-primary-foreground uppercase no-underline underline-offset-4 hover:underline"
                                >
                                    "Ask a question"
                                </a>
                            </div>
                        </div>
                    </section>
                </div>
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
mod photo_alt_tests {
    use super::photo_alt;

    #[test]
    fn named_photos_use_their_name_and_others_fall_back() {
        assert_eq!(
            photo_alt("Sea Grape", 0, Some(" Ocean terrace ")),
            "Ocean terrace"
        );
        assert_eq!(photo_alt("Sea Grape", 2, None), "Sea Grape, photo 3");
        assert_eq!(photo_alt("Sea Grape", 0, Some("  ")), "Sea Grape, photo 1");
    }
}

#[cfg(test)]
mod guestbook_tests {
    use super::{CuratedTestimonial, guestbook_credit};

    #[test]
    fn guestbook_credits_skip_missing_parts() {
        let quote = |location: Option<&str>| CuratedTestimonial {
            guest_name: "Danielle".into(),
            location: location.map(str::to_string),
            stay_date: "March 2026".into(),
            quote: "Lovely".into(),
            rating: 5,
        };
        assert_eq!(
            guestbook_credit(&quote(Some("Toronto"))),
            "Danielle · Toronto · March 2026"
        );
        assert_eq!(guestbook_credit(&quote(None)), "Danielle · March 2026");
        assert_eq!(guestbook_credit(&quote(Some(" "))), "Danielle · March 2026");
    }
}

#[cfg(test)]
mod services_tests {
    use super::{ServiceAddon, addon_price, with_topic};
    use rust_decimal::Decimal;

    fn addon(price: Option<Decimal>, unit: Option<&str>) -> ServiceAddon {
        ServiceAddon {
            name: "Airport transfer".into(),
            price,
            unit: unit.map(str::to_string),
        }
    }

    #[test]
    fn addon_prices_use_the_listing_currency_and_unit() {
        assert_eq!(
            addon_price(&addon(Some(Decimal::new(180, 0)), Some("each way")), "USD"),
            "US$180 each way"
        );
        assert_eq!(
            addon_price(&addon(Some(Decimal::new(1250, 0)), None), "JMD"),
            "J$1,250"
        );
        assert_eq!(
            addon_price(&addon(Some(Decimal::new(9950, 2)), Some(" ")), "USD"),
            "US$99.50"
        );
        assert_eq!(addon_price(&addon(None, Some("day")), "USD"), "On request");
        assert_eq!(
            addon_price(&addon(Some(Decimal::new(220, 0)), Some("day")), "USD"),
            "US$220 / day"
        );
        assert_eq!(
            addon_price(&addon(Some(Decimal::new(40, 0)), Some("per guest")), "USD"),
            "US$40 per guest"
        );
    }

    #[test]
    fn topics_are_added_to_the_enquiry_link() {
        assert_eq!(
            with_topic("/listings/x/enquire", "Airport transfer"),
            "/listings/x/enquire?topic=Airport%20transfer"
        );
        assert_eq!(
            with_topic("/listings/x/enquire?check_in=2027-01-10", "Chef & butler"),
            "/listings/x/enquire?check_in=2027-01-10&topic=Chef%20%26%20butler"
        );
    }
}

#[cfg(test)]
mod layout_tests {
    use super::{
        ListingImageResponse, Sections, SuiteItem, paragraphs, section_links, suites_with_photos,
    };
    use uuid::Uuid;

    #[test]
    fn suite_photos_come_only_from_the_listings_own_images() {
        let own = Uuid::now_v7();
        let images = vec![ListingImageResponse {
            id: own,
            url: "https://storage.googleapis.com/b/listings/own".into(),
            name: None,
        }];
        let suite = |image_id| SuiteItem {
            name: "Master".into(),
            bed_configuration: "1 king".into(),
            max_guests: 2,
            features: vec![],
            en_suite: None,
            image_id,
        };
        let resolved = suites_with_photos(
            &[suite(Some(own)), suite(Some(Uuid::now_v7())), suite(None)],
            &images,
        );
        let photos: Vec<_> = resolved.iter().map(|(_, p)| p.as_deref()).collect();
        assert_eq!(
            photos,
            [
                Some("https://storage.googleapis.com/b/listings/own"),
                None,
                None
            ]
        );
    }

    #[test]
    fn descriptions_split_into_paragraphs_on_blank_lines() {
        assert_eq!(
            paragraphs(Some(
                "First line\nstill first.\r\n\r\nSecond.\n\n\n  \nThird."
            )),
            ["First line\nstill first.", "Second.", "Third."]
        );
        assert!(paragraphs(None).is_empty());
        assert!(paragraphs(Some("  ")).is_empty());
    }

    #[test]
    fn the_section_nav_links_only_shown_sections_in_page_order() {
        let ids = |s: &Sections| {
            section_links(s)
                .into_iter()
                .map(|(id, _)| id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&Sections::default()), ["overview", "amenities"]);
        let all = Sections {
            suites: true,
            services: true,
            policies: true,
            reviews: true,
            location: true,
            photos: true,
        };
        assert_eq!(
            ids(&all),
            [
                "overview",
                "suites",
                "amenities",
                "services",
                "policies",
                "reviews",
                "location",
                "photos"
            ]
        );
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

#[cfg(test)]
mod listing_map_tests {
    //! Spec 103 §1.7: the listing page map.
    use super::{ListingResponse, listing_map_point, location_map, location_note};
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use topcoat::context::Cx;
    use topcoat::view::{View, ViewExt, view};
    use uuid::Uuid;
    use web_ui_common::map_config::testing::maps_on;

    fn listing(coords: Option<(f64, f64)>, precision: Option<&str>) -> ListingResponse {
        let response: Result<ListingResponse, _> = serde_json::from_value(serde_json::json!({
            "id": Uuid::now_v7(),
            "user_id": Uuid::now_v7(),
            "name": "Sea Grape",
            "listing_structure": "Villa",
            "country": "Jamaica",
            "price_per_night": "650",
            "is_active": true,
            "added_at": "2030-01-01T00:00:00Z",
            "max_guests": 4,
            "bedrooms": 2,
            "beds": 2,
            "full_bathrooms": 2,
            "half_bathrooms": 0,
            "slug": "sea-grape",
            "base_currency": "USD",
            "minimum_stay": 1,
            "days_between_bookings": 0,
            "guests_included": 1,
            "latitude": coords.map(|c| c.0),
            "longitude": coords.map(|c| c.1),
            "location_precision": precision,
        }));
        match response {
            Ok(listing) => listing,
            Err(e) => panic!("{e}"),
        }
    }

    async fn html(v: impl View, cx: &Cx) -> String {
        match v.first().await {
            Ok(handle) => handle.render(cx),
            Err(e) => panic!("render failed: {e}"),
        }
    }

    async fn render(listing: &ListingResponse, cx: &Cx) -> (String, &'static str) {
        let point = listing_map_point(listing);
        let note = location_note(point.as_ref());
        (html(view! { location_map(point: point) }, cx).await, note)
    }

    #[derive(Clone, Default)]
    struct Logged(Arc<Mutex<Vec<u8>>>);

    impl Write for Logged {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if let Ok(mut lines) = self.0.lock() {
                lines.extend_from_slice(buf);
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Logged {
        type Writer = Self;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[tokio::test]
    async fn approximate_shows_the_public_point_and_the_after_you_book_copy() {
        let cx = &maps_on();
        let (out, note) = render(
            &listing(Some((18.40741, -77.10313)), Some("approximate")),
            cx,
        )
        .await;
        for expected in [
            r#"data-map="listing""#,
            r#"data-lat="18.40741""#,
            r#"data-lng="-77.10313""#,
            r#"data-precision="approximate""#,
        ] {
            assert!(out.contains(expected), "missing {expected}: {out}");
        }
        assert_eq!(note, "The exact address is shared after you book.");
    }

    #[tokio::test]
    async fn exact_shows_a_pin_and_says_so() {
        let cx = &maps_on();
        let (out, note) = render(&listing(Some((18.4, -77.1)), Some("exact")), cx).await;
        assert!(out.contains(r#"data-precision="exact""#), "{out}");
        assert_eq!(note, "Exact location shown.");
    }

    #[tokio::test]
    async fn unknown_precision_is_never_shown_as_exact() {
        let cx = &maps_on();
        let (out, note) = render(&listing(Some((18.4, -77.1)), None), cx).await;
        assert!(out.contains(r#"data-precision="approximate""#), "{out}");
        assert_eq!(note, "The exact address is shared after you book.");
    }

    #[tokio::test]
    async fn no_coordinates_means_not_mapped_and_no_map() {
        let cx = &maps_on();
        let (out, _) = render(&listing(None, None), cx).await;
        assert!(out.contains("Location not mapped"), "{out}");
        assert!(!out.contains("data-map"), "{out}");
    }

    #[tokio::test]
    async fn out_of_bounds_is_not_mapped_and_logged_with_the_listing_id() {
        let cx = &maps_on();
        let far = listing(Some((40.7128, -74.006)), Some("exact"));
        let logs = Logged::default();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(logs.clone())
            .finish();
        let point = tracing::subscriber::with_default(subscriber, || {
            tracing::callsite::rebuild_interest_cache();
            listing_map_point(&far)
        });
        assert_eq!(point, None);
        let text = match logs.0.lock() {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(e) => panic!("{e}"),
        };
        assert!(text.contains("WARN"), "{text}");
        assert!(text.contains(&far.id.to_string()), "{text}");
        assert!(
            !text.contains("40.71"),
            "coordinates must not be logged: {text}"
        );
        let (out, _) = render(&far, cx).await;
        assert!(out.contains("Location not mapped"), "{out}");
        assert!(!out.contains("data-map"), "{out}");
    }

    #[tokio::test]
    async fn without_map_config_no_map_but_the_box_stays() {
        let cx = &Cx::default();
        let (out, note) = render(&listing(Some((18.4, -77.1)), Some("approximate")), cx).await;
        assert!(!out.contains("data-map"), "{out}");
        assert!(out.contains("min-h-56"), "{out}");
        assert_eq!(note, "The exact address is shared after you book.");
    }
}
