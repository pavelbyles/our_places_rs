//! Portrait villa card for listing grids (docs/design/guest-theme.md §4).

use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

use super::responsive_image::responsive_image;
use crate::components::star_rating::star_rating;

/// A listing's host, for "Hosted by" links to `/listings?host={slug}`.
#[derive(Debug, Clone, PartialEq)]
pub struct HostLink {
    /// Display name.
    pub name: String,
    /// Unique public handle.
    pub slug: String,
}

impl HostLink {
    /// `/listings?host={slug}`.
    pub fn href(&self) -> String {
        format!("/listings?host={}", self.slug)
    }
}

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
    /// The host, when known.
    pub host: Option<HostLink>,
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

impl From<&common::models::ListingResponse> for ListingSummary {
    fn from(l: &common::models::ListingResponse) -> Self {
        Self {
            slug: l.slug.clone(),
            name: l.name.clone(),
            image_url: l.primary_image_url.clone(),
            // Same rounding as the legacy grid ("USD 650").
            price: l
                .price_per_night
                .map(|p| format!("{p:.0}"))
                .unwrap_or_else(|| "0".to_string()),
            currency: l.base_currency.clone(),
            country: l.country.clone(),
            city: l.city.clone(),
            max_guests: l.max_guests,
            bedrooms: l.bedrooms,
            full_bathrooms: l.full_bathrooms,
            rating: l.overall_rating,
            review_count: None,
            host: l
                .owner_name
                .clone()
                .zip(l.owner_slug.clone())
                .map(|(name, slug)| HostLink { name, slug }),
        }
    }
}
