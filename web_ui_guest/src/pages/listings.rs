use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{page, parse_query_params},
    view::{View, view},
};
use web_client::{client::ListingSearchParams, get_api_client};
use web_ui_common::components::domain::{listing_card::ListingSummary, villa_card::villa_carousel};

#[derive(Deserialize, Default)]
struct SearchQuery {
    q: Option<String>,
    currency: Option<String>,
    /// A host's slug: only their listings.
    host: Option<String>,
}

/// All listings, optionally filtered by name and/or host (`?host={slug}`).
#[page("/listings")]
pub async fn listings_page(cx: &Cx) -> Result<impl View> {
    let query = parse_query_params::<SearchQuery>(cx).unwrap_or_default();
    let q = query.q.filter(|s| !s.trim().is_empty());
    let host = query
        .host
        .map(|h| h.trim().to_lowercase())
        .filter(|h| !h.is_empty());
    let listings: Vec<ListingSummary> = get_api_client(cx)
        .search_listings(ListingSearchParams {
            name: q.clone(),
            currency: query.currency,
            host: host.clone(),
            per_page: Some(50),
            ..Default::default()
        })
        .await
        .unwrap_or_default()
        .iter()
        .map(ListingSummary::from)
        .collect();
    let count = listings.len();
    // The host's display name comes from their listings; an unknown slug
    // simply finds none.
    let host_name = host.as_ref().and_then(|slug| {
        listings
            .iter()
            .filter_map(|l| l.host.as_ref())
            .find(|h| &h.slug == slug)
            .map(|h| h.name.clone())
    });
    let heading = match (&host, &host_name) {
        (Some(_), Some(name)) => format!("Listings by {name}"),
        (Some(_), None) => "Host listings".to_string(),
        (None, _) => "Explore All Listings".to_string(),
    };
    let empty = if host.is_some() && q.is_none() {
        "This host has no listings yet"
    } else {
        "No listings match your search criteria"
    };
    Ok(view! {
        <div class="space-y-10">
            <div class="flex flex-col gap-6 md:flex-row md:items-end md:justify-between">
                <div class="space-y-2">
                    <h1 class="font-serif text-[clamp(36px,5vw,56px)] leading-tight">(heading)</h1>
                    <p class="text-muted-foreground">
                        (format!("{count} stays across Jamaica"))
                        if host.is_some() {
                            " · "<a id="clear-host-filter" href="/listings" class="font-medium text-foreground hover:underline underline-offset-4">"Show all listings"</a>
                        }
                    </p>
                </div>
                <form method="get" action="/listings" role="search" class="flex gap-2">
                    if let Some(host) = host.clone() {
                        <input type="hidden" name="host" value=(host) />
                    }
                    <label for="listing-search" class="sr-only">"Search listings"</label>
                    <input id="listing-search" name="q" type="search" value=(q.unwrap_or_default()) placeholder="Search listing name or keyword..." class="h-11 w-72 rounded-full border border-border bg-card px-4 text-sm" />
                    <button type="submit" class="h-11 rounded-full bg-primary px-5 text-sm font-medium text-primary-foreground">"Search"</button>
                </form>
            </div>
            if listings.is_empty() {
                <p class="py-10 text-center text-muted-foreground">(empty)</p>
            } else {
                villa_carousel(listings: listings, track_id: "listings-grid".to_string(), label: "Available villas".to_string())
            }
        </div>
    })
}
