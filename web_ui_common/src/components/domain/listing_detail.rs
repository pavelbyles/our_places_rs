//! Shared listing detail presentation helpers, map, and booking/preview CTAs
//! used by both `web_ui_guest` and `web_ui_admin`'s listing preview.

use common::booking_request::BookingMode;
use common::listing_details::{
    CuratedTestimonial, Ribbon, ServiceAddon, StructuredListingDetails, SuiteItem,
    amenity_groups_for, drive_time,
};
use common::location::LocationPrecision;
use common::models::{ListingDetails, ListingImageResponse, ListingResponse, ReviewResponse};
use common::query::urlencode;
use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{View, attributes, component, view},
};

use crate::components::amenity_group::{amenity_groups, check_list};
use crate::components::date_range_calendar::{CalendarProps, date_range_calendar};
use crate::components::domain::listing_card::ListingSummary;
use crate::components::domain::price_breakdown::{NoQuote, PriceBreakdown, price_breakdown};
use crate::components::domain::responsive_image::{responsive_image, variant_srcset, variant_url};
use crate::components::highlights_ribbon::highlights_ribbon;
use crate::components::listing_hero::listing_hero;
use crate::components::map::{MapKind, MapPoint, map_container};
use crate::components::section_nav::{PILL, PILL_SM, section_nav};
use crate::components::star_rating::star_rating;
use crate::components::suite_card::suites_grid;
use crate::map_config::map_config;

/// Maximum gallery tiles shown in the hero grid.
pub const GALLERY_TILES: usize = 5;

/// Most guestbook quotes shown.
pub const GUESTBOOK_SHOWN: usize = 4;

/// Controls whether booking and enquiry actions link to live checkout/enquiry
/// routes (`Live`) or render as disabled buttons (`Preview`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CtaMode {
    /// Guest portal mode with active checkout and enquiry links.
    Live {
        /// Checkout URL (with selected dates/guests/currency when present).
        checkout_href: String,
        /// Enquiry URL (with selected dates/guests when present).
        enquire_href: String,
    },
    /// Admin preview mode: booking and enquiry actions are disabled.
    Preview,
}

impl CtaMode {
    /// Returns `true` when in admin preview mode.
    pub fn is_preview(&self) -> bool {
        matches!(self, Self::Preview)
    }

    /// Returns the enquiry href in `Live` mode, or `None` in `Preview` mode.
    pub fn enquire_href(&self) -> Option<&str> {
        match self {
            Self::Live { enquire_href, .. } => Some(enquire_href.as_str()),
            Self::Preview => None,
        }
    }
}

/// Result of quoting the currently selected stay dates, if any.
pub type StayQuote = Option<std::result::Result<PriceBreakdown, NoQuote>>;

/// The booking card's call to action for a listing's booking mode: the main
/// button's label and the note under it.
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

/// Renders the `#quote` fragment inner body: price breakdown, refusal, prompt,
/// or `"Rate not set"`, plus `#listing-book-cta` and `#listing-enquire-cta`.
#[component]
pub async fn quote_panel(
    cx: &Cx,
    /// Listing slug for `data-listing`.
    slug: String,
    /// Quoted stay breakdown or refusal, if dates were evaluated.
    quote: StayQuote,
    /// Instant vs. request booking mode.
    mode: BookingMode,
    /// Live links vs. disabled preview buttons.
    cta_mode: CtaMode,
    /// True when `listing.price_per_night` is `None` (draft without rate).
    rate_unset: bool,
) -> Result<impl View> {
    let _ = cx;
    let (label, note) = booking_cta(mode);
    let (breakdown, no_quote) = match quote {
        Some(Ok(b)) => (Some(b), None),
        Some(Err(why)) => (None, Some(why.message())),
        None => (None, None),
    };
    Ok(view! {
        if rate_unset {
            <p id="quote-rate-unset" class="text-sm text-muted-foreground">
                "Rate not set"
            </p>
        } else if let Some(b) = breakdown {
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
        match cta_mode {
            CtaMode::Live { checkout_href, enquire_href } => {
                <a
                    id="listing-book-cta"
                    href=(checkout_href)
                    data-listing=(slug)
                    data-booking-mode=(mode.as_str())
                    class="flex h-11 w-full items-center justify-center rounded-[var(--radius-control,10px)] bg-primary text-[15px] font-medium text-primary-foreground no-underline"
                >
                    (label)
                </a>
                <p class="text-center text-xs text-muted-foreground">(note)</p>
                <a
                    id="listing-enquire-cta"
                    href=(enquire_href)
                    class="flex h-11 w-full items-center justify-center rounded-[var(--radius-control,10px)] border border-border text-[15px] font-medium no-underline"
                >
                    "Ask a question"
                </a>
            }
            CtaMode::Preview => {
                <button
                    type="button"
                    id="listing-book-cta"
                    disabled=(true)
                    aria-disabled="true"
                    data-listing=(slug)
                    data-booking-mode=(mode.as_str())
                    class="flex h-11 w-full items-center justify-center rounded-[var(--radius-control,10px)] bg-primary/50 text-[15px] font-medium text-primary-foreground cursor-not-allowed opacity-60"
                >
                    (label)
                </button>
                <p class="text-center text-xs text-muted-foreground">(note)</p>
                <button
                    type="button"
                    id="listing-enquire-cta"
                    disabled=(true)
                    aria-disabled="true"
                    class="flex h-11 w-full items-center justify-center rounded-[var(--radius-control,10px)] border border-border text-[15px] font-medium text-muted-foreground cursor-not-allowed opacity-60"
                >
                    "Ask a question"
                </button>
                <p
                    data-testid="preview-disabled-notice"
                    class="text-center text-xs text-muted-foreground"
                >
                    "Booking and enquiries are disabled in preview mode."
                </p>
            }
        }
    })
}

/// The listing's headline facts. A bed count of 0 means the host never
/// entered one, so it is left out rather than shown as "0 beds".
pub fn core_facts(max_guests: i32, bedrooms: i32, beds: i32, bathrooms: i32) -> Vec<String> {
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
pub fn paragraphs(description: Option<&str>) -> Vec<String> {
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
pub fn suites_with_photos(
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
pub fn with_topic(href: &str, topic: &str) -> String {
    let sep = if href.contains('?') { '&' } else { '?' };
    format!("{href}{sep}topic={}", urlencode(topic))
}

/// An add-on's price in the listing's currency, e.g. "US$180 each way",
/// "US$220 / day", or "On request" without a price.
pub fn addon_price(addon: &ServiceAddon, currency: &str) -> String {
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

/// "Danielle · Toronto · March 2026".
pub fn guestbook_credit(quote: &CuratedTestimonial) -> String {
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
pub fn photo_alt(listing_name: &str, index: usize, name: Option<&str>) -> String {
    name.map(str::trim).filter(|n| !n.is_empty()).map_or_else(
        || format!("{listing_name}, photo {}", index + 1),
        str::to_string,
    )
}

/// The listing's public point for the map (spec 103 §1.7). Coordinates
/// outside the mapped area are logged, without the coordinates: the host
/// entered something the map can't show.
pub fn listing_map_point(listing: &ListingResponse) -> Option<MapPoint> {
    let point = MapPoint::of_listing(listing);
    if point.is_none() && listing.latitude.is_some() && listing.longitude.is_some() {
        tracing::warn!(listing_id = %listing.id, "listing coordinates are outside the mapped area");
    }
    point
}

/// The line under the location heading.
pub fn location_note(point: Option<&MapPoint>) -> &'static str {
    match point.map(|p| p.precision) {
        Some(LocationPrecision::Exact) => "Exact location shown.",
        _ => "The exact address is shared after you book.",
    }
}

/// The map box of `#location`: the map when there is a point and maps are on,
/// the decorative placeholder when maps are off, and "Location not mapped"
/// when there is no point. All three keep the same `min-h-56` box.
#[component]
pub async fn location_map(
    cx: &Cx,
    /// Public coordinate point, if valid.
    point: Option<MapPoint>,
) -> Result<impl View> {
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
                <div
                    aria-hidden="true"
                    class=(format!("{BOX} bg-linear-to-br from-chip to-photo-placeholder"))
                >
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Sections {
    /// Whether suites section is shown.
    pub suites: bool,
    /// Whether services section is shown.
    pub services: bool,
    /// Whether policies section is shown.
    pub policies: bool,
    /// Whether reviews section is shown.
    pub reviews: bool,
    /// Whether location section is shown.
    pub location: bool,
    /// Whether photos section is shown.
    pub photos: bool,
}

/// Section nav links, in page order, for the sections that are shown.
pub fn section_links(s: &Sections) -> Vec<(&'static str, &'static str)> {
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
pub fn gallery_layout(count: usize) -> (&'static str, Vec<(&'static str, &'static str)>) {
    const SECTION: &str = "grid gap-3 md:h-[460px] md:grid-cols-4 md:grid-rows-2";
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

/// Shared listing detail presentation used by both `web_ui_guest` (`CtaMode::Live`)
/// and `web_ui_admin` preview (`CtaMode::Preview`).
#[component]
pub async fn listing_detail_view(
    cx: &Cx,
    /// Full listing details and images.
    details: ListingDetails,
    /// Verified guest reviews to display.
    reviews: Vec<ReviewResponse>,
    /// Availability calendar props.
    calendar: CalendarProps,
    /// Initial stay quote, if dates were preselected.
    quote: StayQuote,
    /// HTMX endpoint for `#quote` updates.
    quote_url: String,
    /// Live links vs. disabled preview buttons.
    cta_mode: CtaMode,
    /// Display currency without an exchange rate, if any.
    no_rate_for: Option<String>,
    /// Active display currency, if any.
    currency: Option<String>,
    /// Default guest count selected in the form.
    default_guests: i32,
) -> Result<impl View> {
    let _ = cx;
    let summary = ListingSummary::from(&details.listing);
    let listing = details.listing.clone();
    let rate_unset = listing.price_per_night.is_none();
    let price = if rate_unset {
        "Rate not set".to_string()
    } else {
        summary
            .price_text()
            .unwrap_or_else(|unavailable| unavailable)
    };
    let max_guests = listing.max_guests.max(1);
    let default_guests = default_guests.clamp(1, max_guests);
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
    let services = structured.staff_services.clone();
    let base_enquire = cta_mode.enquire_href().map(str::to_string);
    let addons: Vec<(String, String, Option<String>)> = services
        .addons
        .iter()
        .map(|a| {
            (
                a.name.clone(),
                addon_price(a, &listing.base_currency),
                base_enquire.as_deref().map(|e| with_topic(e, &a.name)),
            )
        })
        .collect();
    let arrange_href = base_enquire
        .as_deref()
        .map(|e| with_topic(e, "concierge add-ons"));
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
    let bar_price = price.clone();
    let panel_cta_mode = cta_mode.clone();

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
                    if !rate_unset {
                        <span class="text-sm text-muted-foreground">" / night"</span>
                    }
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
                                                    for (name, addon_cost, ask) in addons {
                                                        <li
                                                            class="flex items-baseline justify-between gap-3 py-2.5"
                                                        >
                                                            if let Some(ask_href) = ask {
                                                                <a href=(ask_href) class="text-foreground no-underline hover:underline">
                                                                    (name)
                                                                </a>
                                                            } else {
                                                                <button
                                                                    type="button"
                                                                    disabled=(true)
                                                                    aria-disabled="true"
                                                                    class="text-left text-muted-foreground cursor-not-allowed"
                                                                >
                                                                    (name)
                                                                </button>
                                                            }
                                                            <span class="shrink-0 font-medium tabular-nums">
                                                                (addon_cost)
                                                            </span>
                                                        </li>
                                                    }
                                                </ul>
                                                if let Some(href) = arrange_href {
                                                    <a href=(href) class=(format!("{PILL_SM} w-fit"))>
                                                        "Ask us to arrange"
                                                    </a>
                                                } else {
                                                    <button
                                                        type="button"
                                                        disabled=(true)
                                                        aria-disabled="true"
                                                        class=(format!("{PILL_SM} w-fit cursor-not-allowed opacity-60"))
                                                    >
                                                        "Ask us to arrange"
                                                    </button>
                                                }
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
                                if !rate_unset {
                                    <span class="text-muted-foreground">" per night"</span>
                                }
                            </p>
                            if listing.booking_mode == BookingMode::Request {
                                <span
                                    id="listing-approval-badge"
                                    class="inline-flex rounded-full border border-border px-3 py-1 text-xs font-medium"
                                >
                                    "Host approval required"
                                </span>
                            }
                            <form
                                id="reserve-form"
                                novalidate=(true)
                                hx-get=(quote_url)
                                hx-trigger="change"
                                hx-target="#quote"
                                hx-swap="innerHTML"
                                class="space-y-4"
                            >
                                if let Some(c) = currency {
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
                                    quote: quote,
                                    mode: listing.booking_mode,
                                    cta_mode: panel_cta_mode,
                                    rate_unset: rate_unset
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
                                if let Some(enquire) = base_enquire {
                                    <a
                                        href=(enquire)
                                        class="inline-flex h-11 items-center rounded-full px-6 text-xs font-medium tracking-[0.16em] text-primary-foreground uppercase no-underline underline-offset-4 hover:underline"
                                    >
                                        "Ask a question"
                                    </a>
                                } else {
                                    <button
                                        type="button"
                                        disabled=(true)
                                        aria-disabled="true"
                                        class="inline-flex h-11 items-center rounded-full px-6 text-xs font-medium tracking-[0.16em] text-primary-foreground/60 uppercase cursor-not-allowed"
                                    >
                                        "Ask a question"
                                    </button>
                                }
                            </div>
                        </div>
                    </section>
                </div>
            </div>
        </article>
    })
}

#[cfg(test)]
mod listing_map_tests {
    //! Spec 103 §1.7: the listing page map.
    use super::{ListingResponse, listing_map_point, location_map, location_note};
    use crate::map_config::testing::maps_on;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use topcoat::context::Cx;
    use topcoat::view::{View, ViewExt, view};
    use uuid::Uuid;

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

#[cfg(test)]
mod quote_panel_tests {
    use super::{CtaMode, quote_panel};
    use common::booking_request::BookingMode;
    use topcoat::context::Cx;
    use topcoat::view::{View, ViewExt, view};

    async fn render(v: impl View, cx: &Cx) -> String {
        match v.first().await {
            Ok(handle) => handle.render(cx),
            Err(e) => panic!("render failed: {e}"),
        }
    }

    #[tokio::test]
    async fn preview_disables_book_and_enquire_buttons_and_shows_rate_unset() {
        let cx = &Cx::default();
        let out = render(
            view! {
                quote_panel(
                    slug: "draft-villa".to_string(),
                    quote: None,
                    mode: BookingMode::Instant,
                    cta_mode: CtaMode::Preview,
                    rate_unset: true,
                )
            },
            cx,
        )
        .await;
        assert!(out.contains("Rate not set"), "{out}");
        assert!(
            out.contains(r#"<button type="button" id="listing-book-cta" disabled"#),
            "{out}"
        );
        assert!(
            out.contains(r#"<button type="button" id="listing-enquire-cta" disabled"#),
            "{out}"
        );
        assert!(
            out.contains(r#"data-testid="preview-disabled-notice""#),
            "{out}"
        );
        assert!(!out.contains("href="), "{out}");
    }

    #[tokio::test]
    async fn live_mode_renders_checkout_and_enquire_links() {
        let cx = &Cx::default();
        let out = render(
            view! {
                quote_panel(
                    slug: "active-villa".to_string(),
                    quote: None,
                    mode: BookingMode::Request,
                    cta_mode: CtaMode::Live {
                        checkout_href: "/checkout/active-villa".to_string(),
                        enquire_href: "/listings/active-villa/enquire".to_string(),
                    },
                    rate_unset: false,
                )
            },
            cx,
        )
        .await;
        assert!(out.contains(r#"href="/checkout/active-villa""#), "{out}");
        assert!(
            out.contains(r#"href="/listings/active-villa/enquire""#),
            "{out}"
        );
        assert!(!out.contains("preview-disabled-notice"), "{out}");
    }
}
