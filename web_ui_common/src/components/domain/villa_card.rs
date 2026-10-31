//! Compact villa card and the horizontal carousel that pages through them.
//!
//! The card is a fixed-width portrait photo with a centred, letter-spaced
//! name and a dotted spec line ("Up to 5 Bedrooms · 5 Bathrooms · 10 Guests").
//! The carousel is a scroll-snap track; its arrow buttons are wired by
//! `ui.js` through `data-carousel*` attributes (strict CSP: no inline script).
//! Without JavaScript the arrows stay hidden and the track still scrolls.

use topcoat::{
    Result,
    view::{Attributes, View, attributes, class, component, view},
};

use super::listing_card::ListingSummary;
use super::responsive_image::responsive_image;

/// `"1 Bedroom"` / `"3 Bedrooms"`.
fn count(n: i32, singular: &str) -> String {
    match n {
        1 => format!("1 {singular}"),
        _ => format!("{n} {singular}s"),
    }
}

/// Spec parts shown under a [`villa_card`]'s name, e.g.
/// `["Up to 5 Bedrooms", "5 Bathrooms", "10 Guests"]`.
pub fn villa_specs(listing: &ListingSummary) -> [String; 3] {
    [
        format!("Up to {}", count(listing.bedrooms, "Bedroom")),
        count(listing.full_bathrooms, "Bathroom"),
        count(listing.max_guests, "Guest"),
    ]
}

/// A compact villa card: 4:5 photo, uppercase tracked name, location, a
/// "Hosted by" link to the host's listings, a dotted spec line, and
/// "from {price} / night" with a View Listing link.
#[component]
pub async fn villa_card(
    /// The listing to show.
    listing: ListingSummary,
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    let href = format!("/listings/{}", listing.slug);
    let location = listing.location();
    let specs = villa_specs(&listing);
    let price = format!("{} {}", listing.currency, listing.price);
    let ListingSummary {
        name,
        image_url,
        host,
        ..
    } = listing;
    let alt = name.clone();
    Ok(view! {
        <article class=(class!("group flex w-[240px] shrink-0 snap-start flex-col sm:w-[280px]", attrs.remove("class"))) (attrs)>
            <a href=(href.clone()) tabindex="-1" aria-hidden="true" class="block aspect-[4/5] overflow-hidden rounded-[var(--radius-media,12px)] bg-photo-placeholder">
                if let Some(src) = image_url {
                    // 280px wide at most: 720px covers 2x screens.
                    responsive_image(src: src, alt: alt, widest: Some("mobile".to_string()), attrs: attributes! { class="transition-transform duration-700 ease-out group-hover:scale-[1.04] motion-reduce:transition-none" })
                }
            </a>
            <div class="flex flex-col items-center gap-1.5 px-2 pt-5 text-center">
                <h3 class="text-sm font-semibold tracking-[0.16em] uppercase">
                    <a href=(href.clone()) class="text-foreground hover:underline underline-offset-4">(name)</a>
                </h3>
                <p class="text-xs text-muted-foreground">(location)</p>
                if let Some(host) = host {
                    <p class="text-xs text-muted-foreground">"Hosted by "<a href=(host.href()) data-host-link="" class="font-medium text-foreground hover:underline underline-offset-4">(host.name)</a></p>
                }
                <ul class="flex flex-wrap justify-center gap-x-2 text-xs text-link" aria-label="Villa size">
                    for (i, spec) in specs.into_iter().enumerate() {
                        <li class="flex items-center gap-2">
                            if i > 0 {
                                <span aria-hidden="true" class="size-1 rounded-full bg-muted-foreground/60"></span>
                            }
                            (spec)
                        </li>
                    }
                </ul>
                <p class="mt-2 flex items-center gap-3 text-xs">
                    <span><span class="text-muted-foreground">"from "</span><span class="font-medium">(price)</span><span class="text-muted-foreground">" / night"</span></span>
                    <span aria-hidden="true" class="h-3 w-px bg-border"></span>
                    <a href=(href) class="font-medium tracking-wide uppercase hover:underline underline-offset-4">"View Listing"</a>
                </p>
            </div>
        </article>
    })
}

/// Chevron icon for the carousel arrows; `left` flips it.
fn chevron(left: bool) -> &'static str {
    if left {
        "M15 18l-6-6 6-6"
    } else {
        "M9 18l6-6-6-6"
    }
}

/// A horizontally scrolling row of [`villa_card`]s with previous/next arrow
/// buttons. Each card is wrapped in `#listing-card-{slug}`.
#[component]
pub async fn villa_carousel(
    /// The listings to show, in order.
    listings: Vec<ListingSummary>,
    /// `id` of the scrolling track (the arrows' `aria-controls`).
    track_id: String,
    /// Accessible name of the track.
    #[default("Villas".to_string())]
    label: String,
) -> Result<impl View> {
    // Arrows sit on the middle of the photos: card width × 5/4 ÷ 2.
    let arrow = "absolute top-[150px] z-10 grid size-11 -translate-y-1/2 place-items-center rounded-full border border-border bg-card/85 text-foreground shadow-sm backdrop-blur transition hover:scale-105 hover:bg-card focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary disabled:pointer-events-none disabled:opacity-0 sm:top-[175px]";
    Ok(view! {
        <div class="relative" data-carousel="">
            for (step, aria, side) in [("-1", "Previous villas", "left-2"), ("1", "Next villas", "right-2")] {
                <button type="button" hidden="" data-carousel-step=(step) aria-label=(aria) aria-controls=(track_id.clone()) class=(class!(arrow, side))>
                    <svg viewBox="0 0 24 24" class="size-5" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                        <path d=(chevron(step == "-1")) />
                    </svg>
                </button>
            }
            <div id=(track_id.clone()) data-carousel-track="" role="region" aria-label=(label) tabindex="0" class="flex snap-x snap-mandatory scroll-px-1 gap-6 overflow-x-auto px-1 pb-4 outline-none motion-safe:scroll-smooth [scrollbar-width:none] focus-visible:ring-2 focus-visible:ring-primary [&::-webkit-scrollbar]:hidden">
                for listing in listings {
                    <div id=(format!("listing-card-{}", listing.slug)) class="shrink-0 snap-start">
                        villa_card(listing: listing)
                    </div>
                }
            </div>
        </div>
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(bedrooms: i32, baths: i32, guests: i32) -> ListingSummary {
        ListingSummary {
            slug: "s".into(),
            name: "n".into(),
            image_url: None,
            price: "650".into(),
            currency: "USD".into(),
            country: "Jamaica".into(),
            city: None,
            max_guests: guests,
            bedrooms,
            full_bathrooms: baths,
            rating: None,
            review_count: None,
            host: None,
        }
    }

    #[test]
    fn specs_pluralise() {
        assert_eq!(
            villa_specs(&listing(5, 5, 10)),
            ["Up to 5 Bedrooms", "5 Bathrooms", "10 Guests"]
        );
        assert_eq!(
            villa_specs(&listing(1, 1, 1)),
            ["Up to 1 Bedroom", "1 Bathroom", "1 Guest"]
        );
    }
}
