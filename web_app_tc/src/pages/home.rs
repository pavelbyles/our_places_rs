use topcoat::{
    Result,
    asset::asset,
    context::Cx,
    router::page,
    view::view,
};
use web_app_common_tc::api_client::{ListingSearchParams, search_listings};
use web_app_common_tc::components::villa_card::villa_card;
use crate::pages::sample_data::get_sample_listings;

#[page("/")]
pub async fn home(_cx: &Cx) -> Result {
    let hero_bg = asset!("../assets/hero_image.jpg");

    // Fetch from listing_api or fallback to sample data
    let api_listings = search_listings(ListingSearchParams {
        per_page: Some(6),
        ..Default::default()
    }).await.unwrap_or_default();

    let listings = if !api_listings.is_empty() {
        api_listings
    } else {
        get_sample_listings()
    };

    view! {
        <div class="flex flex-col gap-16">
            // Hero Section with Clean Layered Background and Floating Search Capsule (Edge-to-Edge)
            <div class="hero-luxury">
                // Background Photo Asset (z-0)
                <img
                    src=(hero_bg)
                    alt="Our Places Jamaica Luxury Stays"
                    class="absolute inset-0 w-full h-full object-cover z-0"
                />
                // Strong Multi-stop Dark Gradient Overlay (z-10)
                <div class="hero-luxury-overlay"></div>

                // Hero Title & Description (z-20)
                <div class="relative z-20 pt-6 md:pt-12 max-w-3xl space-y-3">
                    <span class="inline-block text-xs md:text-sm font-bold tracking-[0.25em] uppercase text-amber-400 drop-shadow-sm">
                        "Villas & Private Residences"
                    </span>
                    <h1 class="text-3xl sm:text-5xl md:text-6xl font-serif font-bold tracking-tight text-white drop-shadow-lg">
                        "Jamaica's Finest Escapes"
                    </h1>
                    <p class="text-white/90 text-sm md:text-base max-w-xl mx-auto font-light leading-relaxed drop-shadow-sm">
                        "Curated private villas with dedicated butler service, infinity pools, and oceanfront sanctuaries."
                    </p>
                </div>

                // Floating Frosted Search Capsule (z-20)
                <div class="w-full flex justify-center pb-4 pt-8 px-4">
                    <form
                        action="/listings"
                        method="GET"
                        class="search-capsule"
                    >
                        <div class="search-field">
                            <label class="search-label">
                                "Destination Parish"
                            </label>
                            <div class="search-select-wrapper">
                                <select name="city" class="search-input">
                                    <option value="">"All Parishes (Montego Bay...)"</option>
                                    <option value="Montego Bay">"Montego Bay, St. James"</option>
                                    <option value="Port Antonio">"Port Antonio, Portland"</option>
                                    <option value="Negril">"Negril, Westmoreland"</option>
                                    <option value="Ocho Rios">"Ocho Rios, St. Ann"</option>
                                    <option value="Kingston">"Kingston & St. Andrew"</option>
                                </select>
                            </div>
                        </div>

                        <div class="search-field">
                            <label class="search-label">
                                "Check-in"
                            </label>
                            <input
                                type="date"
                                name="check_in"
                                value="2026-06-12"
                                class="search-date-input"
                            />
                        </div>

                        <div class="search-field">
                            <label class="search-label">
                                "Check-out"
                            </label>
                            <input
                                type="date"
                                name="check_out"
                                value="2026-06-19"
                                class="search-date-input"
                            />
                        </div>

                        <div class="search-field-last">
                            <label class="search-label">
                                "Guests"
                            </label>
                            <div class="search-select-wrapper">
                                <select name="guests" class="search-input">
                                    <option value="2">"2 Guests"</option>
                                    <option value="4" selected=(true)>"4 Guests"</option>
                                    <option value="6">"6 Guests"</option>
                                    <option value="8">"8+ Guests"</option>
                                </select>
                            </div>
                        </div>

                        <button
                            type="submit"
                            class="btn-discover"
                        >
                            <span>"Discover Stays"</span>
                            <span class="text-sm font-bold">"›"</span>
                        </button>
                    </form>
                </div>
            </div>

            // Main Featured Section with 3-column Editorial Card Grid
            <div class="py-6 max-w-7xl mx-auto px-4 md:px-6 flex flex-col gap-10 w-full">
                <div class="flex flex-col md:flex-row justify-between items-baseline gap-4 border-b border-base-200 pb-4">
                    <div>
                        <span class="text-primary font-bold tracking-widest uppercase text-xs">"Handpicked Collections"</span>
                        <h2 class="text-3xl md:text-4xl font-serif font-bold tracking-tight text-base-content">
                            "Our Featured Villas"
                        </h2>
                    </div>
                    <a href="/listings" class="text-sm font-semibold text-primary hover:underline flex items-center gap-1">
                        "View All Sanctuaries" <span>"›"</span>
                    </a>
                </div>

                <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-8">
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
        </div>
    }
}

#[page("/htmx/welcome")]
pub async fn htmx_welcome(_cx: &Cx) -> Result {
    view! {
        <div id="htmx-demo" class="card bg-primary text-primary-content p-8 max-w-md mx-auto text-center space-y-4 rounded-2xl shadow-xl transition-all duration-300">
            <h3 class="font-extrabold text-2xl">"Interactive Demo: 1"</h3>
            <p class="text-sm opacity-90">
                "Updated via HTMX 4 server fragment swap without page reload or heavy client WASM runtime."
            </p>
            <a href="/" class="btn btn-secondary btn-sm">"Reset Demo"</a>
        </div>
    }
}
