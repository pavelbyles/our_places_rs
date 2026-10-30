use chrono::{Months, NaiveDate};
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
        responsive_image::{responsive_image, variant_url},
    },
    star_rating::star_rating,
};

use crate::quote::quote_for;
use crate::support::{display_currency, jamaica_today};

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

/// The quote area: price breakdown for valid dates, and the Reserve link.
#[component]
async fn quote_panel(
    slug: String,
    breakdown: Option<web_ui_common::components::domain::price_breakdown::PriceBreakdown>,
    href: String,
) -> Result<impl View> {
    Ok(view! {
        if let Some(b) = breakdown {
            <div id="quote-breakdown">price_breakdown(quote: b)</div>
        } else {
            <p class="text-sm text-muted-foreground">"Choose check-in and check-out dates to see the total."</p>
        }
        <a href=(href) data-listing=(slug) class="flex h-11 w-full items-center justify-center rounded-[var(--radius-control,10px)] bg-primary text-[15px] font-medium text-primary-foreground no-underline">"Reserve"</a>
        <p class="text-center text-xs text-muted-foreground">"Your dates are held while you complete checkout."</p>
    })
}

async fn load(cx: &Cx, slug: &str) -> Result<ListingDetails> {
    let currency = display_currency(cx);
    get_api_client(cx)
        .get_listing_by_id(slug, currency.as_deref())
        .await
        .map_err(|_| not_found().into())
}

/// Most photos shown on the page; the rest are in the gallery dialog.
const GALLERY_TILES: usize = 5;

/// The photo grid for `count` photos: the section's classes and one class
/// list per tile shown. Phones show only the first photo; from `md` the
/// grid has a fixed height and the tiles fill it, so no column is left empty.
fn gallery_layout(count: usize) -> (&'static str, Vec<&'static str>) {
    const SECTION: &str = "grid gap-3 md:h-[460px] md:grid-cols-4 md:grid-rows-2";
    const LEAD: &str = "md:col-span-2 md:row-span-2";
    const HIDDEN_WIDE: &str = "hidden md:block md:col-span-2";
    const HIDDEN_SMALL: &str = "hidden md:block";
    let tiles = match count.min(GALLERY_TILES) {
        0 => Vec::new(),
        1 => vec!["md:col-span-4 md:row-span-2"],
        2 => vec![LEAD, "hidden md:block md:col-span-2 md:row-span-2"],
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
    let slug: &str = path_param::<Slug>(cx);
    let details = load(cx, slug).await?;
    let q = stay_query(cx);
    let month = q.check_in.unwrap_or_else(jamaica_today);
    let (calendar, reviews) = tokio::join!(
        calendar_props(cx, &details, month, &q),
        get_api_client(cx).get_listing_reviews(details.listing.id, 1, 6)
    );
    let reviews: Vec<ReviewResponse> = reviews.unwrap_or_default();
    let summary = ListingSummary::from(&details.listing);
    let listing = details.listing.clone();
    let currency = display_currency(cx);
    let price = format!("{} {}", summary.currency, summary.price);
    let max_guests = listing.max_guests.max(1);
    let default_guests = q.guests.unwrap_or(2).clamp(1, max_guests);
    let rating = details
        .rating_summary
        .as_ref()
        .and_then(|r| r.overall_rating);
    let review_count = details
        .rating_summary
        .as_ref()
        .map(|r| i64::from(r.review_count));
    let mut facts = vec![
        format!("Up to {} guests", listing.max_guests),
        format!("{} bedrooms", listing.bedrooms),
        format!("{} beds", listing.beds),
        format!("{} bathrooms", listing.full_bathrooms),
    ];
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
    let breakdown = initial_breakdown(cx, &details, &q, currency.as_deref()).await;
    Ok(view! {
        <article class="space-y-10">
            <header class="space-y-2">
                <h1 class="font-serif text-[clamp(36px,5vw,56px)] leading-tight">(listing.name.clone())</h1>
                <p class="flex flex-wrap items-center gap-3 text-muted-foreground">
                    <span>(summary.location())</span>
                    star_rating(rating: rating, review_count: review_count)
                </p>
            </header>

            if !images.is_empty() {
                <section aria-label="Photos" class=(gallery.0)>
                    for (i, ((src, alt), span)) in images.iter().cloned().zip(gallery.1.iter().copied()).enumerate() {
                        <button type="button" data-dialog-open="gallery-dialog" aria-label=(format!("Open photo {} of {}", i + 1, images.len())) class=(format!("overflow-hidden rounded-[var(--radius-media,12px)] bg-photo-placeholder aspect-[4/3] md:aspect-auto {span}"))>
                            responsive_image(src: src, alt: alt)
                        </button>
                    }
                </section>
                <dialog id="gallery-dialog" aria-label="Photo gallery" class="m-auto max-h-[90vh] w-[min(1240px,95vw)] rounded-[var(--radius-surface,16px)] bg-card p-4 text-card-foreground backdrop:bg-black/60">
                    <div class="flex items-center justify-between gap-4">
                        <p class="text-sm text-muted-foreground">(format!("{} photos · select one to view it full size", images.len()))</p>
                        <button type="button" data-dialog-close="" class="h-11 rounded-full border border-border px-4 text-sm">"Close"</button>
                    </div>
                    <div id="gallery-grid" data-lightbox-group="photo-viewer" class="mt-3 grid grid-cols-1 gap-3 md:grid-cols-2">
                        for (i, (src, alt)) in images.iter().cloned().enumerate() {
                            <button type="button" data-lightbox-index=(i.to_string()) data-full-src=(variant_url(&src, "highres").unwrap_or_else(|| src.clone())) data-alt=(alt.clone()) aria-label=(format!("View photo {} of {} full size", i + 1, images.len())) class="block aspect-[4/3] cursor-zoom-in overflow-hidden rounded-[var(--radius-media,12px)] bg-photo-placeholder">
                                responsive_image(src: src, alt: alt)
                            </button>
                        }
                    </div>
                </dialog>
                <dialog id="photo-viewer" data-lightbox="" aria-label="Photo viewer" class="m-0 h-dvh max-h-none w-screen max-w-none bg-black/95 p-0 text-white backdrop:bg-black/80">
                    <div class="relative flex h-full w-full items-center justify-center">
                        <img id="photo-viewer-img" data-lightbox-img="" alt="" class="max-h-[88dvh] max-w-[92vw] object-contain" />
                        if images.len() > 1 {
                            <button type="button" data-lightbox-step="-1" aria-label="Previous photo" class="absolute top-1/2 left-3 size-12 -translate-y-1/2 rounded-full bg-white/15 text-3xl leading-none hover:bg-white/25">"‹"</button>
                            <button type="button" data-lightbox-step="1" aria-label="Next photo" class="absolute top-1/2 right-3 size-12 -translate-y-1/2 rounded-full bg-white/15 text-3xl leading-none hover:bg-white/25">"›"</button>
                        }
                        <p id="photo-viewer-counter" data-lightbox-counter="" aria-live="polite" class="absolute bottom-4 left-1/2 -translate-x-1/2 text-sm"></p>
                        <button type="button" data-dialog-close="" aria-label="Close photo viewer" class="absolute top-3 right-3 h-11 rounded-full bg-white/15 px-4 text-sm hover:bg-white/25">"Close"</button>
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
                        <p class="text-sm text-muted-foreground">"Hosted by " (host)</p>
                    }
                    <section aria-labelledby="offers-heading" class="space-y-4 border-t border-border pt-8">
                        <h2 id="offers-heading" class="font-serif text-2xl">"What this place offers"</h2>
                        <ul class="grid gap-2 sm:grid-cols-2">
                            for fact in facts {
                                <li class="text-sm">(fact)</li>
                            }
                        </ul>
                    </section>
                    <section aria-labelledby="reviews-heading" class="space-y-4 border-t border-border pt-8">
                        <h2 id="reviews-heading" class="font-serif text-2xl">"Verified Guest Reviews"</h2>
                        if reviews.is_empty() {
                            <p class="text-sm text-muted-foreground">"No reviews yet. Reviews come only from guests who stayed."</p>
                        } else {
                            <ul class="space-y-6">
                                for review in reviews {
                                    <li class="space-y-1">
                                        <p class="flex items-center gap-2 text-sm font-medium">(review.guest_first_name.clone()) star_rating(rating: Some(review.overall_rating))</p>
                                        if let Some(text) = review.public_review_text.clone() {
                                            <p class="text-sm">(text)</p>
                                        }
                                        if let Some(reply) = review.host_reply_text.clone() {
                                            <p class="border-l-2 border-border pl-3 text-sm text-muted-foreground">"Host reply: " (reply)</p>
                                        }
                                    </li>
                                }
                            </ul>
                        }
                    </section>
                </div>

                <aside aria-label="Book this stay" class="h-fit space-y-4 rounded-[var(--radius-surface,16px)] border border-border bg-card p-6 text-card-foreground lg:sticky lg:top-6">
                    <p><span class="text-2xl font-medium">(price)</span><span class="text-muted-foreground">" per night"</span></p>
                    <form id="reserve-form" hx-get=(quote_url) hx-trigger="change" hx-target="#quote" hx-swap="innerHTML" class="space-y-4">
                        if let Some(c) = currency.clone() {
                            <input type="hidden" name="currency" value=(c) />
                        }
                        date_range_calendar(props: calendar)
                        <label class="block text-xs font-semibold tracking-wide text-muted-foreground uppercase">
                            "Guests"
                            <select name="guests" class="mt-1 block h-11 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground normal-case">
                                for n in 1..=max_guests {
                                    <option value=(n.to_string()) selected=(n == default_guests)>(if n == 1 { "1 Guest".to_string() } else { format!("{n} Guests") })</option>
                                }
                            </select>
                        </label>
                    </form>
                    <div id="quote" aria-live="polite" class="space-y-3">
                        quote_panel(slug: listing.slug.clone(), breakdown: breakdown, href: href)
                    </div>
                </aside>
            </div>
        </article>
    })
}

/// The breakdown for the request's dates, if they form a valid stay.
async fn initial_breakdown(
    cx: &Cx,
    details: &ListingDetails,
    q: &StayQuery,
    currency: Option<&str>,
) -> Option<web_ui_common::components::domain::price_breakdown::PriceBreakdown> {
    let (check_in, check_out) = stay_dates(q)?;
    quote_for(cx, details, check_in, check_out, currency).await
}

/// Quote fragment for the booking card (`hx-get` on date/guest change).
#[page("/listings/{slug}/quote")]
pub async fn listing_quote(cx: &Cx) -> Result<impl View> {
    let slug: &str = path_param::<Slug>(cx);
    let details = load(cx, slug).await?;
    let q = stay_query(cx);
    let currency = display_currency(cx);
    let breakdown = initial_breakdown(cx, &details, &q, currency.as_deref()).await;
    let href = checkout_href(&details.listing.slug, &q, currency.as_deref());
    let slug = details.listing.slug.clone();
    Ok(view! { quote_panel(slug: slug, breakdown: breakdown, href: href) })
}

/// Calendar fragment for prev/next month navigation.
#[page("/listings/{slug}/calendar")]
pub async fn listing_calendar(cx: &Cx) -> Result<impl View> {
    let slug: &str = path_param::<Slug>(cx);
    let details = load(cx, slug).await?;
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
                tiles.iter().map(|t| cells(t)).sum::<usize>(),
                8,
                "{count} photos"
            );
        }
        assert!(gallery_layout(0).1.is_empty());
    }

    #[test]
    fn phones_show_only_the_first_photo() {
        let (_, tiles) = gallery_layout(5);
        assert!(!tiles[0].contains("hidden"));
        assert!(tiles[1..].iter().all(|t| t.starts_with("hidden ")));
    }
}
