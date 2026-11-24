use chrono::{Months, NaiveDate};
#[cfg(test)]
use common::booking_request::BookingMode;
#[cfg(test)]
use common::listing_details::{CuratedTestimonial, ServiceAddon, SuiteItem};
#[cfg(test)]
use common::models::ListingImageResponse;
use common::models::{ListingDetails, ListingResponse, ReviewResponse};
use serde::Deserialize;
use topcoat::{
    Result,
    context::{Cx, memoize, try_base_url},
    metadata::{Image, Metadata, OpenGraph},
    router::{error::not_found, page, parse_query_params, path_param},
    view::{View, view},
};
use web_client::get_api_client;
#[cfg(test)]
use web_ui_common::components::domain::listing_detail::{
    Sections, addon_price, booking_cta, core_facts, gallery_layout, guestbook_credit, paragraphs,
    section_links, suites_with_photos, with_topic,
};
use web_ui_common::components::{
    date_range_calendar::{CalendarProps, date_range_calendar, first_of_month},
    domain::{
        listing_detail::{CtaMode, StayQuote, listing_detail_view, photo_alt, quote_panel},
        responsive_image::variant_url,
    },
    json_ld::json_ld,
};
use web_ui_common::site_config::page_title;

use crate::quote::public_quote;
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
        calendar_url_prefix: format!("/listings/{}/calendar", details.listing.slug),
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

/// A listing priced for the page.
#[derive(Clone)]
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
///
/// Memoized per request: the page and [`listing_metadata`] share one API
/// call (spec 102 §1.5.4).
#[memoize(as_ref)]
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

/// The listing's canonical URL, from its slug: a listing also answers at
/// `/listings/<uuid>`, which must not compete with the slug URL (spec 102
/// D10). `None` only without a configured origin (never in a running portal).
pub fn listing_canonical(cx: &Cx, listing: &ListingResponse) -> Option<String> {
    try_base_url(cx).map(|base| base.join(&format!("/listings/{}", listing.slug)))
}

/// The listing's `VacationRental` JSON-LD (spec 102 §1.4), identified by
/// the same slug canonical as the head. `None` without a configured origin,
/// or if it can't be serialized (logged): the page works without it.
fn listing_structured_data(cx: &Cx, details: &ListingDetails) -> Option<serde_json::Value> {
    let canonical = listing_canonical(cx, &details.listing)?;
    serde_json::to_value(crate::seo::json_ld::vacation_rental(details, &canonical))
        .inspect_err(|e| {
            tracing::warn!(listing_id = %details.listing.id, error = %e, "listing JSON-LD not rendered");
        })
        .ok()
}

/// Head metadata for a listing page (spec 102 §1.5.4): its name, a
/// description, the slug canonical and its first photo for previews.
///
/// A load error is not passed on: Topcoat would let it replace the page's
/// own (more accurate) status (plan T1). The page raises the error itself,
/// and the head falls back to the site defaults.
async fn listing_metadata(cx: &Cx) -> Result<Metadata> {
    let raw: &str = slug(cx);
    Ok(match load(cx, raw).await {
        Ok(loaded) => listing_head(cx, &loaded.details),
        Err(e) => {
            tracing::debug!(slug = raw, error = %e, "listing head uses the site defaults");
            Metadata::new()
        }
    })
}

fn listing_head(cx: &Cx, details: &ListingDetails) -> Metadata {
    let listing = &details.listing;
    let head = Metadata::new()
        .title(page_title(&listing.name))
        .description(crate::seo::meta_description(listing));
    let head = match listing_canonical(cx, listing) {
        Some(url) => head.canonical(url),
        None => head,
    };
    match details.images.first() {
        Some(photo) => {
            let url = variant_url(&photo.url, "desktop").unwrap_or_else(|| photo.url.clone());
            head.open_graph(OpenGraph::new().image(Image::new(url).alt(photo_alt(
                &listing.name,
                0,
                photo.name.as_deref(),
            ))))
        }
        None => head,
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
/// Listing detail: gallery, facts, reviews and the booking card.
#[page("/listings/{slug}", with(Metadata::from_fn(listing_metadata)))]
pub async fn listing_detail_page(cx: &Cx) -> Result<impl View> {
    let slug: &str = slug(cx);
    let Loaded {
        details,
        currency,
        no_rate_for,
    } = load(cx, slug).await?.clone();
    let structured_data = listing_structured_data(cx, &details);
    let q = stay_query(cx);
    let month = q.check_in.unwrap_or_else(jamaica_today);
    let (calendar, reviews) = tokio::join!(
        calendar_props(cx, &details, month, &q),
        get_api_client(cx).get_listing_reviews(details.listing.id, 1, 6)
    );
    let reviews: Vec<ReviewResponse> = reviews.unwrap_or_default();
    let default_guests = guests_or_default(q.guests);
    let quote_url = format!("/listings/{}/quote", details.listing.slug);
    let checkout = checkout_href(&details.listing.slug, &q, currency.as_deref());
    let enquire = enquire_href(&details.listing.slug, &q);
    let breakdown = initial_breakdown(cx, &details, &q, currency.as_deref()).await;
    Ok(view! {
        listing_detail_view(
            details: details,
            reviews: reviews,
            calendar: calendar,
            quote: breakdown,
            quote_url: quote_url,
            cta_mode: CtaMode::Live {
                checkout_href: checkout,
                enquire_href: enquire,
            },
            no_rate_for: no_rate_for,
            currency: currency,
            default_guests: default_guests
        )
        if let Some(structured_data) = structured_data {
            json_ld(value: structured_data)
        }
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
    } = load(cx, slug).await?.clone();
    let q = stay_query(cx);
    let breakdown = initial_breakdown(cx, &details, &q, currency.as_deref()).await;
    let href = checkout_href(&details.listing.slug, &q, currency.as_deref());
    let slug = details.listing.slug.clone();
    let mode = details.listing.booking_mode;
    let enquire = enquire_href(&slug, &q);
    let rate_unset = details.listing.price_per_night.is_none();
    Ok(view! {
        quote_panel(
            slug: slug,
            quote: breakdown,
            mode: mode,
            cta_mode: CtaMode::Live {
                checkout_href: href,
                enquire_href: enquire,
            },
            rate_unset: rate_unset
        )
    })
}

/// Calendar fragment for prev/next month navigation.
#[page("/listings/{slug}/calendar")]
pub async fn listing_calendar(cx: &Cx) -> Result<impl View> {
    let slug: &str = slug(cx);
    let details = load(cx, slug).await?.details.clone();
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
#[path = "listing_detail_seo_tests.rs"]
mod listing_detail_seo_tests;
