//! Shared listing detail presentation helpers, map, and booking/preview CTAs
//! used by both `web_ui_guest` and `web_ui_admin`'s listing preview.

use common::booking_request::BookingMode;
use common::listing_details::{CuratedTestimonial, ServiceAddon, SuiteItem};
use common::location::LocationPrecision;
use common::models::{ListingImageResponse, ListingResponse};
use common::query::urlencode;
use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{View, attributes, component, view},
};

use crate::components::domain::price_breakdown::{NoQuote, PriceBreakdown, price_breakdown};
use crate::components::map::{MapKind, MapPoint, map_container};
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
