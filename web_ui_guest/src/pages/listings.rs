use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{page, parse_query_params},
    view::{View, view},
};
use web_client::{client::ListingSearchParams, get_api_client};
use web_ui_common::components::domain::listing_card::{ListingSummary, listing_card};

#[derive(Deserialize, Default)]
struct SearchQuery {
    q: Option<String>,
    currency: Option<String>,
}

/// All listings, optionally filtered by name.
#[page("/listings")]
pub async fn listings_page(cx: &Cx) -> Result<impl View> {
    let query = parse_query_params::<SearchQuery>(cx).unwrap_or_default();
    let q = query.q.filter(|s| !s.trim().is_empty());
    let listings: Vec<ListingSummary> = get_api_client(cx)
        .search_listings(ListingSearchParams {
            name: q.clone(),
            currency: query.currency,
            per_page: Some(50),
            ..Default::default()
        })
        .await
        .unwrap_or_default()
        .iter()
        .map(ListingSummary::from)
        .collect();
    let count = listings.len();
    Ok(view! {
        <div class="space-y-10">
            <div class="flex flex-col gap-6 md:flex-row md:items-end md:justify-between">
                <div class="space-y-2">
                    <h1 class="font-serif text-[clamp(36px,5vw,56px)] leading-tight">"Explore All Listings"</h1>
                    <p class="text-muted-foreground">(format!("{count} stays across Jamaica"))</p>
                </div>
                <form method="get" action="/listings" role="search" class="flex gap-2">
                    <label for="listing-search" class="sr-only">"Search listings"</label>
                    <input id="listing-search" name="q" type="search" value=(q.unwrap_or_default()) placeholder="Search listing name or keyword..." class="h-11 w-72 rounded-full border border-border bg-card px-4 text-sm" />
                    <button type="submit" class="h-11 rounded-full bg-primary px-5 text-sm font-medium text-primary-foreground">"Search"</button>
                </form>
            </div>
            <div id="listings-grid" class="grid grid-cols-[repeat(auto-fit,minmax(300px,1fr))] gap-8">
                if listings.is_empty() {
                    <p class="col-span-full py-10 text-center text-muted-foreground">"No listings match your search criteria"</p>
                } else {
                    for listing in listings {
                        <div id=(format!("listing-card-{}", listing.slug))>
                            listing_card(listing: listing)
                        </div>
                    }
                }
            </div>
        </div>
    })
}
