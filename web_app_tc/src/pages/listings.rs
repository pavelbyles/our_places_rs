use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::view,
};
use web_app_common_tc::api_client::{ListingSearchParams, search_listings};
use web_app_common_tc::components::villa_card::villa_card;
use crate::pages::sample_data::get_sample_listings;

#[page("/listings")]
pub async fn listings_page(_cx: &Cx) -> Result {
    let api_listings = search_listings(ListingSearchParams {
        per_page: Some(20),
        ..Default::default()
    }).await.unwrap_or_default();

    let listings = if !api_listings.is_empty() {
        api_listings
    } else {
        get_sample_listings()
    };

    view! {
        <div class="flex flex-col items-center w-full max-w-7xl mx-auto px-2 md:px-4 py-8 gap-10">
            // Header Title
            <div class="text-center space-y-2 max-w-xl">
                <span class="text-primary font-bold tracking-widest uppercase text-xs">"Jamaican Sanctuary Collection"</span>
                <h1 class="text-3xl md:text-5xl font-serif font-bold tracking-tight text-base-content">
                    "Explore All Villas"
                </h1>
                <p class="text-base-content/60 text-sm font-medium">
                    "Filter by parish, structure type, and bespoke amenities."
                </p>
            </div>

            // Editorial Filter Capsule
            <form
                class="w-full max-w-4xl bg-base-100 dark:bg-base-200 p-2 md:p-3 rounded-2xl md:rounded-full border border-base-300 dark:border-base-100/20 shadow-lg flex flex-col md:flex-row items-center gap-3"
                hx-get="/listings/filter"
                hx-target="#listings-grid"
                hx-swap="outerHTML"
                hx-trigger="submit, change from:select"
            >
                <div class="flex-1 w-full px-4">
                    <input
                        type="text"
                        name="name"
                        placeholder="Search villa name or keyword..."
                        class="input input-ghost input-sm w-full font-medium placeholder:text-base-content/50 focus:bg-transparent"
                    />
                </div>

                <div class="w-full md:w-48 px-2 border-t md:border-t-0 md:border-l border-base-content/10">
                    <select name="city" class="select select-ghost select-sm w-full font-medium focus:bg-transparent">
                        <option value="">"All Parishes"</option>
                        <option value="Montego Bay">"Montego Bay"</option>
                        <option value="Port Antonio">"Port Antonio"</option>
                        <option value="Negril">"Negril"</option>
                        <option value="Ocho Rios">"Ocho Rios"</option>
                        <option value="Kingston">"Kingston"</option>
                    </select>
                </div>

                <div class="w-full md:w-40 px-2 border-t md:border-t-0 md:border-l border-base-content/10">
                    <select name="structure" class="select select-ghost select-sm w-full font-medium focus:bg-transparent">
                        <option value="" selected=(true)>"All Types"</option>
                        <option value="Villa">"Villa"</option>
                        <option value="Apartment">"Apartment"</option>
                        <option value="Estate">"Estate"</option>
                    </select>
                </div>

                <button type="submit" class="btn btn-primary btn-sm rounded-full px-6 w-full md:w-auto font-bold tracking-wide shadow-md">
                    "Filter Stays"
                </button>
            </form>

            // 3-Column Editorial Villa Grid
            <div id="listings-grid" class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-8 w-full pb-16">
                for item in listings {
                    villa_card(
                        id: item.slug.clone(),
                        title: item.name.clone(),
                        image_url: item.primary_image_url.clone().unwrap_or_default(),
                        price: item.price_per_night.map(|p| format!("{:.0}", p)).unwrap_or_else(|| "0".to_string()),
                        currency: item.base_currency.clone(),
                        country: item.country.clone(),
                        city: item.city.clone(),
                        max_guests: item.max_guests,
                        bedrooms: item.bedrooms,
                        full_bathrooms: item.full_bathrooms,
                        rating: item.overall_rating,
                        review_count: None,
                    )
                }
            </div>
        </div>
    }
}

#[page("/listings/filter")]
pub async fn listings_filter(_cx: &Cx) -> Result {
    let api_listings = search_listings(ListingSearchParams {
        per_page: Some(20),
        ..Default::default()
    }).await.unwrap_or_default();

    let listings = if !api_listings.is_empty() {
        api_listings
    } else {
        get_sample_listings()
    };

    view! {
        <div id="listings-grid" class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-8 w-full pb-16">
            if listings.is_empty() {
                <div class="col-span-full text-center opacity-50 text-xl py-10">
                    "No listings match your search criteria"
                </div>
            } else {
                for item in listings {
                    villa_card(
                        id: item.slug.clone(),
                        title: item.name.clone(),
                        image_url: item.primary_image_url.clone().unwrap_or_default(),
                        price: item.price_per_night.map(|p| format!("{:.0}", p)).unwrap_or_else(|| "0".to_string()),
                        currency: item.base_currency.clone(),
                        country: item.country.clone(),
                        city: item.city.clone(),
                        max_guests: item.max_guests,
                        bedrooms: item.bedrooms,
                        full_bathrooms: item.full_bathrooms,
                        rating: item.overall_rating,
                        review_count: None,
                    )
                }
            }
        </div>
    }
}
