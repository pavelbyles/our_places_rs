use topcoat::{
    Result,
    context::Cx,
    router::page,
    runtime::{Event, Signal, shard, signal},
    view::{View, view},
};
use web_app_common_tc::{
    client::ListingSearchParams, components::villa_card::villa_card, get_api_client,
};

#[shard]
pub async fn search_results(
    cx: &Cx,
    query: Signal<String>,
    city: Signal<String>,
    structure: Signal<String>,
) -> Result<impl View> {
    let __cx = cx;
    let name_query = query.get();
    let city_query = city.get();
    let structure_query = structure.get();

    let api = get_api_client(cx);
    let mut listings = api
        .search_listings(ListingSearchParams {
            name: if name_query.trim().is_empty() {
                None
            } else {
                Some(name_query.trim().to_string())
            },
            listing_structure: if structure_query.trim().is_empty() {
                None
            } else {
                Some(vec![structure_query.trim().to_string()])
            },
            per_page: Some(20),
            ..Default::default()
        })
        .await
        .unwrap_or_default();

    if !city_query.trim().is_empty() {
        let city_lower = city_query.trim().to_lowercase();
        listings.retain(|l| {
            l.city
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(&city_lower)
        });
    }

    Ok(view! {
        // 3-Column Editorial Villa Grid with DOM Morphing Pins
        <div id="listings-grid" class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-8 w-full pb-16">
            if listings.is_empty() {
                <div class="col-span-full text-center opacity-50 text-xl py-10">
                    "No listings match your search criteria"
                </div>
            } else {
                for item in listings {
                    <div id=(format!("villa-card-{}", item.slug))>
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
                    </div>
                }
            }
        </div>
    })
}

#[page("/listings")]
pub async fn listings_page(cx: &Cx) -> Result<impl View> {
    let __cx = cx;
    let search_name = signal(cx, String::new);
    let filter_city = signal(cx, String::new);
    let filter_structure = signal(cx, String::new);

    Ok(view! {
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
            <div class="w-full max-w-4xl bg-base-100 dark:bg-base-200 p-2 md:p-3 rounded-2xl md:rounded-full border border-base-300 dark:border-base-100/20 shadow-lg flex flex-col md:flex-row items-center gap-3">
                <div class="flex-1 w-full px-4">
                    <input
                        type="text"
                        name="name"
                        placeholder="Search villa name or keyword..."
                        class="input input-ghost input-sm w-full font-medium placeholder:text-base-content/50 focus:bg-transparent"
                        :value=$(search_name.get())
                        @input=$(|e: Event| search_name.set(e.target.value))
                    />
                </div>

                <div class="w-full md:w-48 px-2 border-t md:border-t-0 md:border-l border-base-content/10">
                    <select
                        name="city"
                        class="select select-ghost select-sm w-full font-medium focus:bg-transparent"
                        :value=$(filter_city.get())
                        @change=$(|e: Event| filter_city.set(e.target.value))
                    >
                        <option value="">"All Parishes"</option>
                        <option value="Montego Bay">"Montego Bay"</option>
                        <option value="Port Antonio">"Port Antonio"</option>
                        <option value="Negril">"Negril"</option>
                        <option value="Ocho Rios">"Ocho Rios"</option>
                        <option value="Kingston">"Kingston"</option>
                    </select>
                </div>

                <div class="w-full md:w-40 px-2 border-t md:border-t-0 md:border-l border-base-content/10">
                    <select
                        name="structure"
                        class="select select-ghost select-sm w-full font-medium focus:bg-transparent"
                        :value=$(filter_structure.get())
                        @change=$(|e: Event| filter_structure.set(e.target.value))
                    >
                        <option value="">"All Types"</option>
                        <option value="Villa">"Villa"</option>
                        <option value="House">"House"</option>
                        <option value="Apartment">"Apartment"</option>
                        <option value="Townhouse">"Townhouse"</option>
                        <option value="Studio">"Studio"</option>
                    </select>
                </div>
            </div>

            // Isolated Shard for the listings grid
            search_results(
                query: $(search_name),
                city: $(filter_city),
                structure: $(filter_structure),
            )
        </div>
    })
}
