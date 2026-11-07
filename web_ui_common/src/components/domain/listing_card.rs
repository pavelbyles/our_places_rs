//! Portrait villa card for listing grids (docs/design/guest-theme.md §4).

use common::models::StayTotal;
use common::query::urlencode;
use rust_decimal::Decimal;
use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

use super::price_breakdown::money;
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
    /// Nightly price, already formatted (e.g. `1,650`).
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
    /// Set when search could not quote this listing in the chosen currency:
    /// the card shows "Price unavailable in {code}" instead of an amount.
    pub price_unavailable_in: Option<String>,
    /// What the searched stay costs here, when it could be priced.
    pub stay_total: Option<StayTotal>,
    /// Whether this card comes from a search with dates.
    pub stay_searched: bool,
    /// The search, as a query string, for the card's links to carry.
    pub search_query: Option<String>,
}

/// What a card's links carry from a search, as the guest gave it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchLink {
    /// First night of the stay.
    pub check_in: Option<String>,
    /// Departure date of the stay.
    pub check_out: Option<String>,
    /// Party size.
    pub guests: Option<String>,
    /// Display currency.
    pub currency: Option<String>,
}

impl SearchLink {
    /// The query string, or `None` when there's nothing to carry. Dates count
    /// only as a pair, and a stay with no guest count is priced for one guest,
    /// so the link says so and the next page agrees.
    pub fn query(&self) -> Option<String> {
        let given = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        let dates = given(&self.check_in).zip(given(&self.check_out));
        let guests = given(&self.guests).or_else(|| dates.as_ref().map(|_| "1".to_string()));
        let mut pairs: Vec<(&str, String)> = Vec::new();
        if let Some((check_in, check_out)) = dates {
            pairs.push(("check_in", check_in));
            pairs.push(("check_out", check_out));
        }
        if let Some(guests) = guests {
            pairs.push(("guests", guests));
        }
        if let Some(currency) = given(&self.currency) {
            pairs.push(("currency", currency));
        }
        (!pairs.is_empty()).then(|| {
            pairs
                .iter()
                .map(|(key, value)| format!("{key}={}", urlencode(value)))
                .collect::<Vec<_>>()
                .join("&")
        })
    }
}

/// The lines a card shows about a searched stay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StayTotalLines {
    /// The total, and what it includes.
    Total {
        /// E.g. `USD $3,420.00 total for 5 nights`.
        amount: String,
        /// E.g. `Includes taxes, fees and a USD $500.00 refundable deposit`.
        includes: String,
    },
    /// The stay was searched, but this listing couldn't be priced.
    Unavailable,
}

impl ListingSummary {
    /// The card's price line: `Ok("USD $1,650")`, or `Err("Price unavailable in JMD")`
    /// when no rate could quote it (never a made-up amount).
    pub fn price_text(&self) -> std::result::Result<String, String> {
        match &self.price_unavailable_in {
            Some(code) => Err(format!("Price unavailable in {code}")),
            None => Ok(format!(
                "{} {}{}",
                self.currency,
                common::money::currency_symbol(&self.currency).unwrap_or(""),
                self.price
            )),
        }
    }

    /// `/listings/{slug}`, carrying any search.
    pub fn href(&self) -> String {
        match &self.search_query {
            Some(query) => format!("/listings/{}?{query}", self.slug),
            None => format!("/listings/{}", self.slug),
        }
    }

    /// The lines about a searched stay. `None` with no dates, and when a missing
    /// rate already says "Price unavailable" on the card.
    pub fn stay_total_lines(&self) -> Option<StayTotalLines> {
        match &self.stay_total {
            Some(total) => {
                let nights = match total.nights {
                    1 => "1 night".to_string(),
                    n => format!("{n} nights"),
                };
                let includes = if total.damage_deposit > Decimal::ZERO {
                    format!(
                        "Includes taxes, fees and a {} refundable deposit",
                        money(&total.currency, total.damage_deposit)
                    )
                } else {
                    "Includes taxes and fees".to_string()
                };
                Some(StayTotalLines::Total {
                    amount: format!(
                        "{} total for {nights}",
                        money(&total.currency, total.total_amount)
                    ),
                    includes,
                })
            }
            None => (self.stay_searched && self.price_unavailable_in.is_none())
                .then_some(StayTotalLines::Unavailable),
        }
    }

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
    let href = listing.href();
    let location = listing.location();
    let specs = listing.specs();
    let price = listing.price_text();
    let stay_lines = listing.stay_total_lines();
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
                match price {
                    Ok(price) => {
                        <p class="text-sm"><span class="text-muted-foreground">"from "</span><span class="font-medium">(price)</span><span class="text-muted-foreground">" per night"</span></p>
                    }
                    Err(unavailable) => {
                        <p class="text-sm text-muted-foreground" data-price-unavailable="">(unavailable)</p>
                    }
                }
                <a href=(href) class="text-sm font-medium">"View Listing"</a>
            </div>
            match stay_lines {
                Some(StayTotalLines::Total { amount, includes }) => {
                    <div data-stay-total="">
                        <p class="text-sm font-medium">(amount)</p>
                        <p class="text-xs text-muted-foreground">(includes)</p>
                    </div>
                }
                Some(StayTotalLines::Unavailable) => {
                    <p class="text-sm text-muted-foreground" data-stay-total-unavailable="">"Total unavailable for these dates"</p>
                }
                None => {}
            }
        </article>
    })
}

impl From<&common::models::ListingResponse> for ListingSummary {
    fn from(l: &common::models::ListingResponse) -> Self {
        Self {
            slug: l.slug.clone(),
            name: l.name.clone(),
            image_url: l.primary_image_url.clone(),
            // Whole units, grouped ("1,650").
            price: l
                .price_per_night
                .map(common::money::grouped_whole)
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
            price_unavailable_in: l.price_unavailable_in.clone(),
            stay_total: l.stay_total.clone(),
            stay_searched: false,
            search_query: None,
        }
    }
}

#[cfg(test)]
mod stay_card_tests {
    use super::super::villa_card::villa_card;
    use super::*;
    use topcoat::context::Cx;
    use topcoat::view::ViewExt;

    fn summary() -> ListingSummary {
        ListingSummary {
            slug: "sea-grape".into(),
            name: "Sea Grape".into(),
            image_url: None,
            price: "650".into(),
            currency: "USD".into(),
            country: "Jamaica".into(),
            city: None,
            max_guests: 4,
            bedrooms: 2,
            full_bathrooms: 2,
            rating: None,
            review_count: None,
            host: None,
            price_unavailable_in: None,
            stay_total: None,
            stay_searched: false,
            search_query: None,
        }
    }

    fn total(nights: i32, deposit: Decimal) -> StayTotal {
        StayTotal {
            nights,
            total_amount: Decimal::new(342_050, 2),
            damage_deposit: deposit,
            currency: "USD".into(),
        }
    }

    async fn html(v: impl View, cx: &Cx) -> String {
        match v.first().await {
            Ok(handle) => handle.render(cx),
            Err(e) => panic!("render failed: {e}"),
        }
    }

    async fn both_cards(listing: ListingSummary) -> [String; 2] {
        let __cx = &Cx::default();
        let other = listing.clone();
        [
            html(view! { listing_card(listing: listing) }, __cx).await,
            html(view! { villa_card(listing: other) }, __cx).await,
        ]
    }

    #[tokio::test]
    async fn a_total_with_a_deposit_keeps_its_cents_and_names_the_deposit() {
        let listing = ListingSummary {
            stay_total: Some(total(5, Decimal::new(50_000, 2))),
            stay_searched: true,
            ..summary()
        };
        for out in both_cards(listing).await {
            assert!(out.contains("USD $3,420.50 total for 5 nights"), "{out}");
            assert!(
                out.contains("Includes taxes, fees and a USD $500.00 refundable deposit"),
                "{out}"
            );
            assert!(out.contains("USD $650"), "the nightly line stays: {out}");
        }
    }

    #[tokio::test]
    async fn a_total_without_a_deposit_doesnt_mention_one() {
        let listing = ListingSummary {
            stay_total: Some(total(1, Decimal::ZERO)),
            stay_searched: true,
            ..summary()
        };
        for out in both_cards(listing).await {
            assert!(out.contains("USD $3,420.50 total for 1 night"), "{out}");
            assert!(!out.contains("1 nights"), "{out}");
            assert!(out.contains("Includes taxes and fees"), "{out}");
            assert!(!out.contains("deposit"), "{out}");
        }
    }

    #[tokio::test]
    async fn a_searched_stay_with_no_total_says_so() {
        let listing = ListingSummary {
            stay_searched: true,
            ..summary()
        };
        for out in both_cards(listing).await {
            assert!(out.contains("Total unavailable for these dates"), "{out}");
        }
    }

    #[tokio::test]
    async fn a_missing_rate_says_price_unavailable_and_not_total_unavailable() {
        let listing = ListingSummary {
            stay_searched: true,
            price_unavailable_in: Some("JMD".into()),
            ..summary()
        };
        for out in both_cards(listing).await {
            assert!(out.contains("Price unavailable in JMD"), "{out}");
            assert!(!out.contains("Total unavailable"), "{out}");
        }
    }

    #[tokio::test]
    async fn with_no_search_the_cards_dont_change() {
        for out in both_cards(summary()).await {
            assert!(!out.contains("total for"), "{out}");
            assert!(!out.contains("Total unavailable"), "{out}");
            assert!(!out.contains("data-stay-total"), "{out}");
            assert!(out.contains(r#"href="/listings/sea-grape""#), "{out}");
        }
    }

    #[tokio::test]
    async fn links_carry_the_search() {
        let listing = ListingSummary {
            search_query: Some("check_in=2030-08-01&check_out=2030-08-04&guests=2".into()),
            ..summary()
        };
        for out in both_cards(listing).await {
            assert!(
                out.contains(
                    r#"href="/listings/sea-grape?check_in=2030-08-01&amp;check_out=2030-08-04&amp;guests=2""#
                ),
                "{out}"
            );
        }
    }

    fn link(check_in: &str, check_out: &str, guests: &str, currency: &str) -> SearchLink {
        let some = |v: &str| Some(v.to_string());
        SearchLink {
            check_in: some(check_in),
            check_out: some(check_out),
            guests: some(guests),
            currency: some(currency),
        }
    }

    #[test]
    fn dates_with_no_guests_link_as_one_guest() {
        assert_eq!(
            link("2030-08-01", "2030-08-04", "", "").query().as_deref(),
            Some("check_in=2030-08-01&check_out=2030-08-04&guests=1")
        );
    }

    #[test]
    fn empty_values_are_left_out() {
        assert_eq!(link("", "", "", "").query(), None);
        assert_eq!(SearchLink::default().query(), None);
        assert_eq!(link("", "", "2", "").query().as_deref(), Some("guests=2"));
        assert_eq!(
            link("", "", "", "JMD").query().as_deref(),
            Some("currency=JMD")
        );
        // One date isn't a stay.
        assert_eq!(link("2030-08-01", "", "", "").query(), None);
    }

    #[test]
    fn hostile_values_stay_inside_their_own_parameter() {
        for value in ["a&currency=XXX#", "a+b", "a=b", "a\"b", "a<b"] {
            let query = link("", "", value, "").query().unwrap_or_default();
            assert_eq!(query.matches('&').count(), 0, "{query}");
            assert_eq!(query.matches('=').count(), 1, "{query}");
            assert!(!query.contains('#'), "{query}");
            assert!(!query.contains('<'), "{query}");
            assert!(!query.contains('"'), "{query}");
            let encoded = query.trim_start_matches("guests=");
            assert_eq!(encoded, urlencode(value));
        }
    }
}
