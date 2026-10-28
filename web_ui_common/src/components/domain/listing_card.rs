//! Portrait villa card for listing grids (docs/design/guest-theme.md §4).

use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

use super::responsive_image::responsive_image;
use crate::components::star_rating::star_rating;

/// What a [`listing_card`] shows.
#[derive(Debug, Clone, PartialEq)]
pub struct ListingSummary {
    /// URL slug, used for `/listings/{slug}`.
    pub slug: String,
    /// Listing name.
    pub name: String,
    /// Primary photo URL, if any.
    pub image_url: Option<String>,
    /// Nightly price, already formatted (e.g. `650`).
    pub price: String,
    /// Currency of `price`.
    pub currency: String,
    /// Country.
    pub country: String,
    /// City, if known.
    pub city: Option<String>,
    /// Maximum guests.
    pub max_guests: i32,
    /// Bedrooms.
    pub bedrooms: i32,
    /// Full bathrooms.
    pub full_bathrooms: i32,
    /// Average rating, if the listing has reviews.
    pub rating: Option<f64>,
    /// Number of reviews.
    pub review_count: Option<i64>,
}

impl ListingSummary {
    /// "City, Country", or just the country.
    pub fn location(&self) -> String {
        match &self.city {
            Some(city) => format!("{city}, {}", self.country),
            None => self.country.clone(),
        }
    }

    /// "4 Guests · 2 Beds · 2 Baths".
    pub fn specs(&self) -> String {
        format!(
            "{} Guests · {} Beds · {} Baths",
            self.max_guests, self.bedrooms, self.full_bathrooms
        )
    }
}

/// A listing card: 4:5 photo, serif name, location and specs, rating, and
/// "from {price} per night" with a View Listing link. Unlike the legacy card,
/// it shows no invented rating or amenities.
#[component]
pub async fn listing_card(
    /// The listing to show.
    listing: ListingSummary,
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    let href = format!("/listings/{}", listing.slug);
    let location = listing.location();
    let specs = listing.specs();
    let price = format!("{} {}", listing.currency, listing.price);
    let ListingSummary {
        name,
        image_url,
        rating,
        review_count,
        ..
    } = listing;
    let alt = name.clone();
    Ok(view! {
        <article class=(class!("flex flex-col gap-3", attrs.remove("class"))) (attrs)>
            <a href=(href.clone()) tabindex="-1" aria-hidden="true" class="block overflow-hidden rounded-[var(--radius-media,12px)] bg-photo-placeholder aspect-[4/5]">
                if let Some(src) = image_url {
                    responsive_image(src: src, alt: alt)
                }
            </a>
            <div class="flex items-start justify-between gap-3">
                <h3 class="font-serif text-[22px] leading-tight">
                    <a href=(href.clone()) class="text-foreground hover:underline">(name)</a>
                </h3>
                star_rating(rating: rating, review_count: review_count)
            </div>
            <p class="text-sm text-muted-foreground">(location) " · " (specs)</p>
            <div class="flex items-center justify-between">
                <p class="text-sm"><span class="text-muted-foreground">"from "</span><span class="font-medium">(price)</span><span class="text-muted-foreground">" per night"</span></p>
                <a href=(href) class="text-sm font-medium">"View Listing"</a>
            </div>
        </article>
    })
}
