use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::view,
};
use web_app_common_tc::api_client::{ListingSearchParams, search_listings};
use web_app_common_tc::components::villa_card::villa_card;

#[page("/")]
pub async fn home(_cx: &Cx) -> Result {
    let hero_bg = "https://images.unsplash.com/photo-1540541338287-41700207dee6?auto=format&fit=crop&w=2000&q=85";

    // Dynamic relative dates (check-in defaults to tomorrow; past dates disallowed)
    let today = chrono::Utc::now().date_naive();
    let tomorrow = today + chrono::Days::new(1);
    let default_checkout = today + chrono::Days::new(6);
    let min_checkin = tomorrow.format("%Y-%m-%d").to_string();
    let min_checkout = (tomorrow + chrono::Days::new(1)).format("%Y-%m-%d").to_string();
    let val_checkin = tomorrow.format("%Y-%m-%d").to_string();
    let val_checkout = default_checkout.format("%Y-%m-%d").to_string();

    let listings = search_listings(ListingSearchParams {
        per_page: Some(12),
        ..Default::default()
    }).await.unwrap_or_default();

    view! {
        <div class="flex flex-col gap-16">
            // Hero Section with Clean Layered Background and Floating Search Capsule (Edge-to-Edge)
            <div class="relative w-full min-h-[540px] md:min-h-[600px] overflow-hidden flex flex-col justify-between items-center text-center px-4 py-12 md:py-16 shadow-2xl bg-slate-900">
                // Background Photo Asset (z-0)
                <img
                    src=(hero_bg)
                    alt="Our Places Jamaica Luxury Stays"
                    class="absolute inset-0 w-full h-full object-cover object-center z-0 brightness-75"
                />
                // Strong Multi-stop Dark Gradient Overlay (z-10)
                <div class="absolute inset-0 bg-gradient-to-b from-black/70 via-black/35 to-black/85 pointer-events-none z-10"></div>

                // Hero Title & Description (z-20)
                <div class="relative z-20 pt-6 md:pt-10 max-w-3xl space-y-3">
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
                <div class="relative z-20 w-full flex justify-center pb-4 pt-8 px-2 md:px-4">
                    <form
                        action="/listings"
                        method="GET"
                        class="w-full max-w-5xl bg-white/20 dark:bg-slate-900/75 backdrop-blur-xl border border-white/40 dark:border-white/15 rounded-3xl md:rounded-full p-2.5 md:p-3 shadow-2xl text-white flex flex-col md:flex-row items-center justify-between gap-2 md:gap-3"
                    >
                        <div class="w-full md:flex-1 text-left px-3 md:px-4 py-1 md:border-r border-white/25">
                            <label class="block text-[10px] font-bold uppercase tracking-widest text-white/80 mb-0.5">
                                "Destination Parish"
                            </label>
                            <select name="city" class="w-full bg-transparent border-0 text-white font-semibold text-sm outline-none cursor-pointer">
                                <option value="" class="bg-slate-800 text-white">"All Parishes (Montego Bay...)"</option>
                                <option value="Montego Bay" class="bg-slate-800 text-white">"Montego Bay, St. James"</option>
                                <option value="Port Antonio" class="bg-slate-800 text-white">"Port Antonio, Portland"</option>
                                <option value="Negril" class="bg-slate-800 text-white">"Negril, Westmoreland"</option>
                                <option value="Ocho Rios" class="bg-slate-800 text-white">"Ocho Rios, St. Ann"</option>
                                <option value="Kingston" class="bg-slate-800 text-white">"Kingston & St. Andrew"</option>
                            </select>
                        </div>

                        <div class="w-full md:flex-1 text-left px-3 md:px-4 py-1 md:border-r border-white/25">
                            <label class="block text-[10px] font-bold uppercase tracking-widest text-white/80 mb-0.5">
                                "Check-in"
                            </label>
                            <input
                                type="date"
                                name="check_in"
                                id="search-check-in"
                                min=(min_checkin.clone())
                                value=(val_checkin)
                                class="w-full bg-transparent border-0 text-white font-semibold text-sm outline-none cursor-pointer"
                                onchange="var ci = new Date(this.value); if (!isNaN(ci.getTime())) { var coDate = new Date(ci.getTime() + 86400000); var coStr = coDate.toISOString().split('T')[0]; var coEl = document.getElementById('search-check-out'); if (coEl) { coEl.min = coStr; if (coEl.value === this.value || coEl.value.localeCompare(this.value) === -1) coEl.value = coStr; } }"
                            />
                        </div>

                        <div class="w-full md:flex-1 text-left px-3 md:px-4 py-1 md:border-r border-white/25">
                            <label class="block text-[10px] font-bold uppercase tracking-widest text-white/80 mb-0.5">
                                "Check-out"
                            </label>
                            <input
                                type="date"
                                name="check_out"
                                id="search-check-out"
                                min=(min_checkout)
                                value=(val_checkout)
                                class="w-full bg-transparent border-0 text-white font-semibold text-sm outline-none cursor-pointer"
                            />
                        </div>

                        <div class="w-full md:flex-1 text-left px-3 md:px-4 py-1">
                            <label class="block text-[10px] font-bold uppercase tracking-widest text-white/80 mb-0.5">
                                "Guests"
                            </label>
                            <select name="guests" class="w-full bg-transparent border-0 text-white font-semibold text-sm outline-none cursor-pointer">
                                <option value="2" class="bg-slate-800 text-white">"2 Guests"</option>
                                <option value="4" selected=(true) class="bg-slate-800 text-white">"4 Guests"</option>
                                <option value="6" class="bg-slate-800 text-white">"6 Guests"</option>
                                <option value="8" class="bg-slate-800 text-white">"8+ Guests"</option>
                            </select>
                        </div>

                        <button
                            type="submit"
                            class="w-full md:w-auto bg-amber-500 hover:bg-amber-400 text-slate-950 font-bold text-xs uppercase tracking-wider px-7 py-3.5 rounded-full shadow-lg transition-all flex items-center justify-center gap-2 cursor-pointer"
                        >
                            <span>"Discover Stays"</span>
                            <span class="text-sm font-bold">"›"</span>
                        </button>
                    </form>
                </div>
            </div>

            // Curated Featured Showcase Section
            <div class="max-w-7xl mx-auto px-4 w-full space-y-8">
                <div class="flex flex-col md:flex-row justify-between items-start md:items-end gap-4 border-b border-base-content/10 pb-4">
                    <div>
                        <span class="text-primary font-bold uppercase tracking-widest text-xs">"Handpicked Portfolio"</span>
                        <h2 class="text-3xl md:text-4xl font-serif font-bold text-base-content tracking-tight mt-1">
                            "Featured Island Residences"
                        </h2>
                    </div>
                    <a href="/listings" class="btn btn-outline btn-primary btn-sm rounded-full font-bold">
                        "Explore Full Collection →"
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

            // Value Propositions / Luxury Experience Perks
            <div class="bg-base-200/50 py-16 px-4">
                <div class="max-w-6xl mx-auto grid grid-cols-1 md:grid-cols-3 gap-8 text-center">
                    <div class="card bg-base-100 p-8 rounded-3xl border border-base-content/10 shadow-sm space-y-3">
                        <div class="w-12 h-12 rounded-2xl bg-primary/10 text-primary flex items-center justify-center text-2xl mx-auto">
                            "✨"
                        </div>
                        <h3 class="font-serif font-bold text-xl text-base-content">"Dedicated Butler & Staff"</h3>
                        <p class="text-xs text-base-content/70 leading-relaxed">
                            "Every property includes private staff, curated dining, and bespoke concierge arrangements."
                        </p>
                    </div>

                    <div class="card bg-base-100 p-8 rounded-3xl border border-base-content/10 shadow-sm space-y-3">
                        <div class="w-12 h-12 rounded-2xl bg-amber-500/10 text-amber-500 flex items-center justify-center text-2xl mx-auto">
                            "🛡️"
                        </div>
                        <h3 class="font-serif font-bold text-xl text-base-content">"100% Guaranteed Availability"</h3>
                        <p class="text-xs text-base-content/70 leading-relaxed">
                            "PostgreSQL row-level locking ensures zero double bookings and a guaranteed 15-minute checkout hold."
                        </p>
                    </div>

                    <div class="card bg-base-100 p-8 rounded-3xl border border-base-content/10 shadow-sm space-y-3">
                        <div class="w-12 h-12 rounded-2xl bg-secondary/10 text-secondary flex items-center justify-center text-2xl mx-auto">
                            "💱"
                        </div>
                        <h3 class="font-serif font-bold text-xl text-base-content">"Multi-Currency Settlement"</h3>
                        <p class="text-xs text-base-content/70 leading-relaxed">
                            "Pay in USD, JMD, EUR, GBP, or CAD with exact exchange rates and transparent statutory GCT taxes."
                        </p>
                    </div>
                </div>
            </div>
        </div>
    }
}

#[page("/htmx/welcome")]
pub async fn htmx_welcome(_cx: &Cx) -> Result {
    view! {
        <div class="alert alert-success shadow-lg text-sm font-semibold">
            <span>"Topcoat + HTMX Real-time Component Swapping Active"</span>
        </div>
    }
}
