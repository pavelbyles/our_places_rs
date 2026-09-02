use topcoat::{
    Result,
    context::Cx,
    router::{page, path_param},
    view::view,
};
use web_app_common_tc::{
    client::ListingSearchParams,
    get_api_client,
};

path_param!(id);
path_param!(slug);

#[page("/admin/listings")]
pub async fn admin_listings_page(cx: &Cx) -> Result {
    render_listings_content(cx).await
}

#[page("/listings")]
pub async fn listings_alias_page(cx: &Cx) -> Result {
    render_listings_content(cx).await
}

#[page("/admin/listings/{id}")]
pub async fn admin_listing_detail_page(cx: &Cx) -> Result {
    let id: &str = path_param::<Id>(cx);
    render_edit_listing_content(cx, id.to_string()).await
}

#[page("/listings/{id}")]
pub async fn listings_alias_detail_page(cx: &Cx) -> Result {
    let id: &str = path_param::<Id>(cx);
    render_edit_listing_content(cx, id.to_string()).await
}

async fn render_listings_content(cx: &Cx) -> Result {
    if let Err(_err) = web_app_common_tc::auth::require_admin_auth(cx).await {
        return view! {
            <script>
                r#"window.location.replace('/login?redirect=' + encodeURIComponent(window.location.pathname + window.location.search));"#
            </script>
        };
    }

    let __cx = cx;
    let api = get_api_client(cx);

    let listings = api.search_listings(ListingSearchParams {
        per_page: Some(50),
        ..Default::default()
    }).await.unwrap_or_default();

    view! {
        <div class="space-y-8 py-6 max-w-7xl mx-auto px-4 md:px-6">
            // Header
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 border-b border-base-200 pb-4">
                <div class="space-y-1">
                    <span class="text-primary font-bold tracking-widest uppercase text-xs">"Inventory & Property Catalog"</span>
                    <h1 class="text-3xl font-serif font-bold tracking-tight text-base-content">
                        "Villa Listings Management"
                    </h1>
                    <p class="text-xs text-base-content/60">
                        "Create, edit, duplicate, and configure seasonal pricing overrides for luxury Caribbean properties."
                    </p>
                </div>
                <div class="flex items-center gap-3">
                    <a href="/admin/listings/new" class="btn btn-primary btn-sm rounded-full px-5 font-bold tracking-wide shadow-md">
                        "+ Create New Villa"
                    </a>
                </div>
            </div>

            // Filter Bar
            <div class="bg-base-100 dark:bg-base-200/80 p-4 rounded-2xl border border-base-200 dark:border-base-100/20 shadow-sm flex flex-col md:flex-row items-center justify-between gap-4">
                <div class="flex-1 w-full flex items-center gap-3">
                    <input
                        type="text"
                        placeholder="Search by villa name, parish, or slug..."
                        class="input input-bordered border-2 border-base-300 input-sm w-full max-w-md rounded-xl font-medium"
                    />
                    <select class="select select-bordered border-2 border-base-300 select-sm rounded-xl font-medium">
                        <option value="">"All Structures"</option>
                        <option value="Villa">"Villa"</option>
                        <option value="House">"House"</option>
                        <option value="Apartment">"Apartment"</option>
                        <option value="Townhouse">"Townhouse"</option>
                        <option value="Studio">"Studio"</option>
                    </select>
                </div>
                <div class="text-xs text-base-content/60 font-semibold">
                    (listings.len())" Total Properties Active"
                </div>
            </div>

            // Listings Data Table
            <div class="bg-base-100 dark:bg-base-200 rounded-2xl border border-base-200 dark:border-base-100/20 shadow-md overflow-hidden">
                <div class="overflow-x-auto">
                    <table class="table table-zebra w-full">
                        <thead>
                            <tr class="text-xs text-base-content/60 uppercase tracking-wider">
                                <th>"Property"</th>
                                <th>"Type"</th>
                                <th>"Location"</th>
                                <th>"Nightly Base"</th>
                                <th>"Capacity & Specs"</th>
                                <th>"Discounts"</th>
                                <th>"Min Stay"</th>
                                <th>"Status"</th>
                                <th class="text-right">"Actions"</th>
                            </tr>
                        </thead>
                        <tbody>
                            for item in listings {
                                let item_key = if !item.slug.is_empty() {
                                    item.slug.clone()
                                } else {
                                    item.id.to_string()
                                };
                                <tr>
                                    <td class="font-bold flex items-center gap-3">
                                        if let Some(ref img) = item.primary_image_url {
                                            <div class="avatar">
                                                <div class="w-12 h-12 rounded-xl overflow-hidden shadow-sm">
                                                    <img src=(img) alt=(item.name.clone()) class="object-cover" />
                                                </div>
                                            </div>
                                        }
                                        <div>
                                            <div class="font-serif font-bold text-sm text-base-content">(item.name.clone())</div>
                                            <div class="text-[11px] text-base-content/50 font-mono">(item_key.clone())</div>
                                        </div>
                                    </td>
                                    <td>
                                        <span class="badge badge-outline badge-sm font-semibold uppercase text-[10px] tracking-wider">
                                            (item.listing_structure.clone())
                                        </span>
                                    </td>
                                    <td class="text-xs font-medium">
                                        (item.city.clone().unwrap_or_else(|| "Jamaica".to_string()))", "(item.country.clone())
                                    </td>
                                    <td class="font-semibold text-sm">
                                        (item.base_currency.clone())" "(item.price_per_night.map(|p| format!("{:.0}", p)).unwrap_or_else(|| "0".to_string()))
                                    </td>
                                    <td class="text-xs text-base-content/70">
                                        (item.max_guests)"G · "(item.bedrooms)"B · "(item.full_bathrooms)"Ba"
                                        if item.half_bathrooms > 0 {
                                            " · "(item.half_bathrooms)"½"
                                        }
                                    </td>
                                    <td class="text-xs text-base-content/60">
                                        if let Some(w) = item.weekly_discount_percentage {
                                            (format!("{:.0}% wk", w))
                                        } else {
                                            "-"
                                        }
                                    </td>
                                    <td class="text-xs font-semibold">
                                        (item.minimum_stay)" nights"
                                    </td>
                                    <td>
                                        if item.is_active {
                                            <span class="badge badge-success badge-sm font-semibold">"Active"</span>
                                        } else {
                                            <span class="badge badge-ghost badge-sm">"Draft"</span>
                                        }
                                    </td>
                                    <td class="text-right space-x-1">
                                        <a
                                            href=(format!("/admin/listings/clone/{}", item_key))
                                            class="btn btn-ghost btn-xs text-secondary font-bold"
                                            title="Use this listing to create another listing"
                                        >
                                            "Duplicate"
                                        </a>
                                        <a href=(format!("/admin/listings/{}/pricing", item_key)) class="btn btn-ghost btn-xs text-amber-500 font-bold">
                                            "Pricing"
                                        </a>
                                        <a href=(format!("/admin/listings/{}/edit", item_key)) class="btn btn-ghost btn-xs text-primary font-bold">
                                            "Edit"
                                        </a>
                                        <a href=(format!("/listings/{}", item_key)) target="_blank" class="btn btn-ghost btn-xs text-base-content/60">
                                            "View ↗"
                                        </a>
                                    </td>
                                </tr>
                            }
                        </tbody>
                    </table>
                </div>
            </div>
        </div>
    }
}

#[page("/admin/listings/clone/{slug}")]
pub async fn admin_clone_listing_page(cx: &Cx) -> Result {
    let slug: &str = path_param::<Slug>(cx);
    let api = get_api_client(cx);
    let template = api.get_listing_by_id(slug, None).await.ok().map(|d| d.listing);
    render_new_or_cloned_listing(cx, template).await
}

#[page("/admin/listings/new")]
pub async fn admin_new_listing_page(cx: &Cx) -> Result {
    render_new_or_cloned_listing(cx, None).await
}

async fn render_new_or_cloned_listing(__cx: &Cx, template: Option<common::models::ListingResponse>) -> Result {
    if let Err(_err) = web_app_common_tc::auth::require_admin_auth(__cx).await {
        return view! {
            <script>
                r#"window.location.replace('/login?redirect=' + encodeURIComponent(window.location.pathname + window.location.search));"#
            </script>
        };
    }

    // Pre-populate fields from template if cloning

    let initial_name = template.as_ref()
        .map(|t| format!("{} (Copy)", t.name))
        .unwrap_or_default();
    let initial_desc = template.as_ref()
        .and_then(|t| t.description.clone())
        .unwrap_or_default();
    let initial_structure = template.as_ref()
        .map(|t| t.listing_structure.clone())
        .unwrap_or_else(|| "Villa".to_string());
    let initial_country = template.as_ref()
        .map(|t| t.country.clone())
        .unwrap_or_else(|| "Jamaica".to_string());
    let initial_currency = template.as_ref()
        .map(|t| t.base_currency.clone())
        .unwrap_or_else(|| "USD".to_string());
    let initial_city = template.as_ref()
        .and_then(|t| t.city.clone())
        .unwrap_or_else(|| "Montego Bay".to_string());
    let initial_price = template.as_ref()
        .and_then(|t| t.price_per_night)
        .map(|p| format!("{:.0}", p))
        .unwrap_or_else(|| "1800".to_string());
    let initial_weekly_disc = template.as_ref()
        .and_then(|t| t.weekly_discount_percentage)
        .map(|p| format!("{:.0}", p))
        .unwrap_or_else(|| "10".to_string());
    let initial_monthly_disc = template.as_ref()
        .and_then(|t| t.monthly_discount_percentage)
        .map(|p| format!("{:.0}", p))
        .unwrap_or_else(|| "20".to_string());
    let initial_guests = template.as_ref().map(|t| t.max_guests).unwrap_or(10);
    let initial_bedrooms = template.as_ref().map(|t| t.bedrooms).unwrap_or(5);
    let initial_beds = template.as_ref().map(|t| t.beds).unwrap_or(6);
    let initial_full_baths = template.as_ref().map(|t| t.full_bathrooms).unwrap_or(5);
    let initial_half_baths = template.as_ref().map(|t| t.half_bathrooms).unwrap_or(1);
    let initial_sq_meters = template.as_ref().and_then(|t| t.square_meters).unwrap_or(450);
    let initial_min_stay = template.as_ref().map(|t| t.minimum_stay).unwrap_or(3);
    let initial_days_between = template.as_ref().map(|t| t.days_between_bookings).unwrap_or(1);
    let initial_lat = template.as_ref().and_then(|t| t.latitude).unwrap_or(18.4762);
    let initial_lon = template.as_ref().and_then(|t| t.longitude).unwrap_or(-77.9189);
    let initial_image = template.as_ref()
        .and_then(|t| t.primary_image_url.clone())
        .unwrap_or_else(|| "https://images.unsplash.com/photo-1580587771525-78b9dba3b914?auto=format&fit=crop&w=1200&q=80".to_string());

    view! {
        <div class="max-w-5xl mx-auto py-8 px-4 space-y-8">
            <div class="border-b border-base-200 pb-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                <div>
                    <span class="text-primary font-bold tracking-widest uppercase text-xs">
                        if template.is_some() {
                            "Duplicate & Create New Villa"
                        } else {
                            "Property Creation Studio"
                        }
                    </span>
                    <h1 class="text-3xl font-serif font-bold tracking-tight text-base-content">
                        if let Some(ref t) = template {
                            "Clone from: "(t.name.clone())
                        } else {
                            "Add New Luxury Caribbean Villa"
                        }
                    </h1>
                </div>
                <a href="/admin/listings" class="btn btn-ghost btn-sm font-semibold">
                    "← Back to Listings"
                </a>
            </div>

            // Clone from existing listing selector banner
            <div class="bg-base-100 dark:bg-base-200/90 p-4 rounded-2xl border-2 border-primary/20 flex flex-col sm:flex-row sm:items-center justify-between gap-3 text-xs shadow-sm">
                <div class="flex items-center gap-2">
                    <span class="text-lg">"⚡"</span>
                    <span class="font-bold text-base-content">"Want to clone an existing villa as your starting template?"</span>
                </div>
                <div class="flex items-center gap-2">
                    <select id="template-select" class="select select-bordered border-2 border-base-300 select-xs rounded-lg font-semibold bg-base-100">
                        <option value="the-reef-house">"The Reef House"</option>
                        <option value="blue-lagoon-sanctuary">"Blue Lagoon Sanctuary"</option>
                        <option value="negril-sunset-cliffside-estate">"Negril Sunset Cliffside Estate"</option>
                        <option value="ocho-rios-coastal-haven">"Ocho Rios Coastal Haven"</option>
                        <option value="kingston-skyline-luxury-penthouse">"Kingston Skyline Luxury Penthouse"</option>
                    </select>
                    <button
                        onclick="var slug = document.getElementById('template-select').value; window.location.href = '/admin/listings/clone/' + slug;"
                        type="button"
                        class="btn btn-primary btn-xs rounded-lg font-bold shadow-xs"
                    >
                        "Load Template"
                    </button>
                </div>
            </div>

            <form action="/admin/listings" method="POST" class="space-y-8">
                // 1. Basic Details & Property Classification
                <div class="bg-base-100 dark:bg-base-200/90 p-6 md:p-8 rounded-3xl border-2 border-base-200 dark:border-base-100/30 shadow-md space-y-5">
                    <div class="flex items-center justify-between border-b border-base-200 pb-3">
                        <div class="flex items-center gap-2">
                            <span class="text-xl">"🏷️"</span>
                            <h2 class="text-lg font-serif font-bold text-base-content">
                                "1. Basic Property Details & Classification"
                            </h2>
                        </div>
                        <span class="badge badge-primary badge-sm font-bold">"Core Identity"</span>
                    </div>

                    <div class="grid grid-cols-1 md:grid-cols-3 gap-5">
                        <div class="md:col-span-2">
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Villa Name / Title"</label>
                            <input
                                type="text"
                                name="name"
                                value=(initial_name)
                                placeholder="e.g. Whispering Palms Oceanfront Sanctuary"
                                required=(true)
                                class="input input-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-medium rounded-xl w-full px-4 py-2.5 shadow-xs focus:border-primary focus:ring-4 focus:ring-primary/10 transition-all"
                            />
                        </div>
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Structure / Type"</label>
                            <select
                                name="listing_structure"
                                class="select select-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-bold rounded-xl w-full shadow-xs"
                            >
                                <option value="Villa" selected=(initial_structure == "Villa")>"Villa 🌴"</option>
                                <option value="House" selected=(initial_structure == "House")>"House 🏡"</option>
                                <option value="Apartment" selected=(initial_structure == "Apartment")>"Apartment / Penthouse 🏢"</option>
                                <option value="Townhouse" selected=(initial_structure == "Townhouse")>"Townhouse 🏘️"</option>
                                <option value="Studio" selected=(initial_structure == "Studio")>"Studio 🛏️"</option>
                            </select>
                        </div>
                    </div>

                    <div>
                        <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Editorial Overview & Description"</label>
                        <textarea
                            name="description"
                            rows="4"
                            placeholder="Describe the panoramic ocean views, private butler service, infinity pool, and bespoke luxury amenities..."
                            class="textarea textarea-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-normal rounded-xl w-full p-4 shadow-xs focus:border-primary focus:ring-4 focus:ring-primary/10 transition-all"
                        >(initial_desc)</textarea>
                    </div>
                </div>

                // 2. Location & Geocoding
                <div class="bg-base-100 dark:bg-base-200/90 p-6 md:p-8 rounded-3xl border-2 border-base-200 dark:border-base-100/30 shadow-md space-y-5">
                    <div class="flex items-center justify-between border-b border-base-200 pb-3">
                        <div class="flex items-center gap-2">
                            <span class="text-xl">"📍"</span>
                            <h2 class="text-lg font-serif font-bold text-base-content">
                                "2. Regional Location & GPS Coordinates"
                            </h2>
                        </div>
                        <span class="badge badge-secondary badge-sm font-bold">"Geocoded"</span>
                    </div>

                    <div class="grid grid-cols-1 md:grid-cols-4 gap-5">
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Parish / Destination"</label>
                            <select
                                name="city"
                                class="select select-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-bold rounded-xl w-full shadow-xs"
                            >
                                <option value="Montego Bay" selected=(initial_city == "Montego Bay")>"Montego Bay (St. James)"</option>
                                <option value="Port Antonio" selected=(initial_city == "Port Antonio")>"Port Antonio (Portland)"</option>
                                <option value="Negril" selected=(initial_city == "Negril")>"Negril (Westmoreland)"</option>
                                <option value="Ocho Rios" selected=(initial_city == "Ocho Rios")>"Ocho Rios (St. Ann)"</option>
                                <option value="Kingston" selected=(initial_city == "Kingston")>"Kingston & St. Andrew"</option>
                            </select>
                        </div>
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Country (Editable)"</label>
                            <select
                                name="country"
                                class="select select-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-bold rounded-xl w-full shadow-xs"
                            >
                                <option value="Jamaica" selected=(initial_country == "Jamaica")>"Jamaica 🇯🇲"</option>
                                <option value="Barbados" selected=(initial_country == "Barbados")>"Barbados 🇧🇧"</option>
                                <option value="Bahamas" selected=(initial_country == "Bahamas")>"Bahamas 🇧🇸"</option>
                                <option value="Saint Lucia" selected=(initial_country == "Saint Lucia")>"Saint Lucia 🇱🇨"</option>
                                <option value="Cayman Islands" selected=(initial_country == "Cayman Islands")>"Cayman Islands 🇰🇾"</option>
                                <option value="Turks and Caicos" selected=(initial_country == "Turks and Caicos")>"Turks & Caicos 🇹🇨"</option>
                                <option value="Dominican Republic" selected=(initial_country == "Dominican Republic")>"Dominican Republic 🇩🇴"</option>
                            </select>
                        </div>
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Latitude (GPS)"</label>
                            <input
                                type="number"
                                step="0.0001"
                                name="latitude"
                                value=(initial_lat.to_string())
                                class="input input-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-mono text-xs rounded-xl w-full shadow-xs"
                            />
                        </div>
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Longitude (GPS)"</label>
                            <input
                                type="number"
                                step="0.0001"
                                name="longitude"
                                value=(initial_lon.to_string())
                                class="input input-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-mono text-xs rounded-xl w-full shadow-xs"
                            />
                        </div>
                    </div>
                </div>

                // 3. Capacity & Specifications
                <div class="bg-base-100 dark:bg-base-200/90 p-6 md:p-8 rounded-3xl border-2 border-base-200 dark:border-base-100/30 shadow-md space-y-5">
                    <div class="flex items-center justify-between border-b border-base-200 pb-3">
                        <div class="flex items-center gap-2">
                            <span class="text-xl">"🛏️"</span>
                            <h2 class="text-lg font-serif font-bold text-base-content">
                                "3. Capacity, Accommodations & Floor Space"
                            </h2>
                        </div>
                        <span class="badge badge-accent badge-sm font-bold">"Floor Plan"</span>
                    </div>

                    <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-6 gap-4">
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Max Guests"</label>
                            <input type="number" name="max_guests" value=(initial_guests) min="1" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Bedrooms"</label>
                            <input type="number" name="bedrooms" value=(initial_bedrooms) min="1" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Beds"</label>
                            <input type="number" name="beds" value=(initial_beds) min="1" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Full Baths"</label>
                            <input type="number" name="full_bathrooms" value=(initial_full_baths) min="1" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Half Baths"</label>
                            <input type="number" name="half_bathrooms" value=(initial_half_baths) min="0" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Sq. Meters (m²)"</label>
                            <input type="number" name="square_meters" value=(initial_sq_meters) min="10" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                    </div>
                </div>

                // 4. Nightly Base Pricing & Turnover Policies
                <div class="bg-base-100 dark:bg-base-200/90 p-6 md:p-8 rounded-3xl border-2 border-base-200 dark:border-base-100/30 shadow-md space-y-5">
                    <div class="flex items-center justify-between border-b border-base-200 pb-3">
                        <div class="flex items-center gap-2">
                            <span class="text-xl">"💰"</span>
                            <h2 class="text-lg font-serif font-bold text-base-content">
                                "4. Base Pricing, Stay Discounts & Turnover Policies"
                            </h2>
                        </div>
                        <span class="badge badge-warning badge-sm font-bold">"Financial Yield"</span>
                    </div>

                    <div class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-5 gap-4">
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Base Rate / Night"</label>
                            <div class="join w-full">
                                <select name="base_currency" class="select select-bordered border-2 border-base-300 join-item font-bold bg-base-100">
                                    <option value="USD" selected=(initial_currency == "USD")>"USD"</option>
                                    <option value="JMD" selected=(initial_currency == "JMD")>"JMD"</option>
                                    <option value="EUR" selected=(initial_currency == "EUR")>"EUR"</option>
                                    <option value="GBP" selected=(initial_currency == "GBP")>"GBP"</option>
                                </select>
                                <input type="number" name="price_per_night" value=(initial_price) min="50" step="10" class="input input-bordered border-2 border-base-300 join-item w-full font-bold bg-base-100" />
                            </div>
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Min Stay (Nights)"</label>
                            <input type="number" name="minimum_stay" value=(initial_min_stay) min="1" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Weekly Disc %"</label>
                            <input type="number" name="weekly_discount_percentage" value=(initial_weekly_disc) min="0" max="50" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Monthly Disc %"</label>
                            <input type="number" name="monthly_discount_percentage" value=(initial_monthly_disc) min="0" max="50" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Turnover Buffer"</label>
                            <input type="number" name="days_between_bookings" value=(initial_days_between) min="0" max="7" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                    </div>
                </div>

                // 5. Photo Asset
                <div class="bg-base-100 dark:bg-base-200/90 p-6 md:p-8 rounded-3xl border-2 border-base-200 dark:border-base-100/30 shadow-md space-y-5">
                    <div class="flex items-center justify-between border-b border-base-200 pb-3">
                        <div class="flex items-center gap-2">
                            <span class="text-xl">"🖼️"</span>
                            <h2 class="text-lg font-serif font-bold text-base-content">
                                "5. Hero Photo Asset & Media Management"
                            </h2>
                        </div>
                        <span class="badge badge-info badge-sm font-bold">"GCS Signed Storage"</span>
                    </div>

                    <div>
                        <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Primary Hero Image URL"</label>
                        <input
                            type="url"
                            name="primary_image_url"
                            value=(initial_image)
                            placeholder="https://storage.googleapis.com/our_places_images/..."
                            class="input input-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-medium rounded-xl w-full px-4 py-2.5 shadow-xs"
                        />
                    </div>
                </div>

                // Submit Bar
                <div class="p-6 bg-base-100 dark:bg-base-200 rounded-3xl border-2 border-base-200 flex flex-col sm:flex-row sm:items-center justify-between gap-4 shadow-xl">
                    <label class="flex items-center gap-3 cursor-pointer">
                        <input type="checkbox" name="is_active" checked=(true) class="toggle toggle-primary toggle-md" />
                        <div>
                            <div class="text-sm font-bold text-base-content">"Publish Immediately"</div>
                            <div class="text-xs text-base-content/50">"Make listing visible on public guest booking portal"</div>
                        </div>
                    </label>
                    <div class="flex items-center gap-3">
                        <a href="/admin/listings" class="btn btn-ghost rounded-full px-6 font-semibold">"Cancel"</a>
                        <button type="submit" class="btn btn-primary rounded-full px-8 font-bold tracking-wide shadow-lg">
                            if template.is_some() {
                                "Create Duplicate Villa"
                            } else {
                                "Create Villa Listing"
                            }
                        </button>
                    </div>
                </div>
            </form>
        </div>
    }
}

#[page("/admin/listings/{id}/edit")]
pub async fn admin_edit_listing_page(cx: &Cx) -> Result {
    let id: &str = path_param::<Id>(cx);
    render_edit_listing_content(cx, id.to_string()).await
}

#[page("/listings/{id}/edit")]
pub async fn listings_edit_alias_page(cx: &Cx) -> Result {
    let id: &str = path_param::<Id>(cx);
    render_edit_listing_content(cx, id.to_string()).await
}

async fn render_edit_listing_content(cx: &Cx, id: String) -> Result {
    if let Err(_err) = web_app_common_tc::auth::require_admin_auth(cx).await {
        return view! {
            <script>
                r#"window.location.replace('/login?redirect=' + encodeURIComponent(window.location.pathname + window.location.search));"#
            </script>
        };
    }

    let __cx = cx;
    let api = get_api_client(cx);

    let id_str = if !id.trim().is_empty() { id } else { "the-reef-house".to_string() };

    let listing_details = api.get_listing_by_id(&id_str, None).await.ok();

    let listing = listing_details.as_ref().map(|d| &d.listing);
    let title = listing.map(|l| l.name.clone()).unwrap_or_else(|| "The Reef House".to_string());
    let desc = listing.and_then(|l| l.description.clone()).unwrap_or_else(|| "Luxury oceanfront sanctuary".to_string());
    let structure = listing.map(|l| l.listing_structure.clone()).unwrap_or_else(|| "Villa".to_string());
    let currency = listing.map(|l| l.base_currency.clone()).unwrap_or_else(|| "USD".to_string());
    let city = listing.and_then(|l| l.city.clone()).unwrap_or_else(|| "Port Antonio".to_string());
    let country = listing.map(|l| l.country.clone()).unwrap_or_else(|| "Jamaica".to_string());
    let price = listing.and_then(|l| l.price_per_night).map(|p| format!("{:.0}", p)).unwrap_or_else(|| "1800".to_string());
    let weekly_disc = listing.and_then(|l| l.weekly_discount_percentage).map(|p| format!("{:.0}", p)).unwrap_or_else(|| "10".to_string());
    let monthly_disc = listing.and_then(|l| l.monthly_discount_percentage).map(|p| format!("{:.0}", p)).unwrap_or_else(|| "20".to_string());
    let guests = listing.map(|l| l.max_guests).unwrap_or(10);
    let bedrooms = listing.map(|l| l.bedrooms).unwrap_or(5);
    let beds = listing.map(|l| l.beds).unwrap_or(6);
    let full_baths = listing.map(|l| l.full_bathrooms).unwrap_or(5);
    let half_baths = listing.map(|l| l.half_bathrooms).unwrap_or(1);
    let sq_meters = listing.and_then(|l| l.square_meters).unwrap_or(450);
    let min_stay = listing.map(|l| l.minimum_stay).unwrap_or(3);
    let days_between = listing.map(|l| l.days_between_bookings).unwrap_or(1);
    let is_active = listing.map(|l| l.is_active).unwrap_or(true);
    let lat = listing.and_then(|l| l.latitude).unwrap_or(18.1760);
    let lon = listing.and_then(|l| l.longitude).unwrap_or(-76.4520);
    let image_url = listing.and_then(|l| l.primary_image_url.clone()).unwrap_or_else(|| "https://images.unsplash.com/photo-1580587771525-78b9dba3b914?auto=format&fit=crop&w=1200&q=80".to_string());

    view! {
        <div class="max-w-5xl mx-auto py-8 px-4 space-y-8">
            // Studio Header with Hero Preview
            <div class="bg-base-100 dark:bg-base-200/90 rounded-3xl border-2 border-base-200 dark:border-base-100/30 p-6 shadow-md flex flex-col md:flex-row md:items-center justify-between gap-6">
                <div class="flex items-center gap-4">
                    <div class="avatar">
                        <div class="w-16 h-16 rounded-2xl overflow-hidden shadow-md border-2 border-primary/30">
                            <img src=(image_url.clone()) alt=(title.clone()) class="object-cover" />
                        </div>
                    </div>
                    <div>
                        <div class="flex items-center gap-2 flex-wrap">
                            <span class="badge badge-primary badge-xs font-bold uppercase tracking-wider">
                                (structure.clone())
                            </span>
                            <span class="badge badge-outline badge-xs font-mono">
                                (id_str.clone())
                            </span>
                            if is_active {
                                <span class="badge badge-success badge-xs font-bold">"Live on Portal"</span>
                            }
                        </div>
                        <h1 class="text-2xl md:text-3xl font-serif font-bold text-base-content mt-1">
                            (title.clone())
                        </h1>
                        <p class="text-xs text-base-content/60">
                            (city.clone())", "(country.clone())" · Base Rate: "(currency.clone())" "(price.clone())"/night"
                        </p>
                    </div>
                </div>

                <div class="flex items-center gap-2 flex-wrap">
                    <a
                        href=(format!("/admin/listings/clone/{}", id_str))
                        class="btn btn-secondary btn-sm rounded-full font-bold shadow-xs"
                        title="Duplicate this listing to create a new property"
                    >
                        "Duplicate Villa 📑"
                    </a>
                    <a href=(format!("/admin/listings/{}/pricing", id_str)) class="btn btn-warning btn-sm rounded-full font-bold shadow-xs">
                        "Dynamic Pricing ⏱"
                    </a>
                    <a href="/admin/listings" class="btn btn-ghost btn-sm font-semibold">
                        "← Catalog"
                    </a>
                </div>
            </div>

            <form action="/admin/listings" method="POST" class="space-y-8">
                // 1. Basic Details & Property Classification
                <div class="bg-base-100 dark:bg-base-200/90 p-6 md:p-8 rounded-3xl border-2 border-base-200 dark:border-base-100/30 shadow-md space-y-5">
                    <div class="flex items-center justify-between border-b border-base-200 pb-3">
                        <div class="flex items-center gap-2">
                            <span class="text-xl">"🏷️"</span>
                            <h2 class="text-lg font-serif font-bold text-base-content">
                                "1. Basic Property Details & Classification"
                            </h2>
                        </div>
                        <span class="badge badge-primary badge-sm font-bold">"Core Identity"</span>
                    </div>

                    <div class="grid grid-cols-1 md:grid-cols-3 gap-5">
                        <div class="md:col-span-2">
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Villa Name / Title"</label>
                            <input
                                type="text"
                                name="name"
                                value=(title)
                                class="input input-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-medium rounded-xl w-full px-4 py-2.5 shadow-xs focus:border-primary focus:ring-4 focus:ring-primary/10 transition-all"
                            />
                        </div>
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Structure / Type"</label>
                            <select
                                name="listing_structure"
                                class="select select-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-bold rounded-xl w-full shadow-xs"
                            >
                                <option value="Villa" selected=(structure == "Villa")>"Villa 🌴"</option>
                                <option value="House" selected=(structure == "House")>"House 🏡"</option>
                                <option value="Apartment" selected=(structure == "Apartment")>"Apartment / Penthouse 🏢"</option>
                                <option value="Townhouse" selected=(structure == "Townhouse")>"Townhouse 🏘️"</option>
                                <option value="Studio" selected=(structure == "Studio")>"Studio 🛏️"</option>
                            </select>
                        </div>
                    </div>

                    <div>
                        <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Editorial Overview & Description"</label>
                        <textarea
                            name="description"
                            rows="4"
                            class="textarea textarea-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-normal rounded-xl w-full p-4 shadow-xs focus:border-primary focus:ring-4 focus:ring-primary/10 transition-all"
                        >(desc)</textarea>
                    </div>
                </div>

                // 2. Location & Regional Geocoding
                <div class="bg-base-100 dark:bg-base-200/90 p-6 md:p-8 rounded-3xl border-2 border-base-200 dark:border-base-100/30 shadow-md space-y-5">
                    <div class="flex items-center justify-between border-b border-base-200 pb-3">
                        <div class="flex items-center gap-2">
                            <span class="text-xl">"📍"</span>
                            <h2 class="text-lg font-serif font-bold text-base-content">
                                "2. Regional Location & GPS Coordinates"
                            </h2>
                        </div>
                        <span class="badge badge-secondary badge-sm font-bold">"Geocoded"</span>
                    </div>

                    <div class="grid grid-cols-1 md:grid-cols-4 gap-5">
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Parish / Destination"</label>
                            <select
                                name="city"
                                class="select select-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-bold rounded-xl w-full shadow-xs"
                            >
                                <option value="Montego Bay" selected=(city == "Montego Bay")>"Montego Bay (St. James)"</option>
                                <option value="Port Antonio" selected=(city == "Port Antonio")>"Port Antonio (Portland)"</option>
                                <option value="Negril" selected=(city == "Negril")>"Negril (Westmoreland)"</option>
                                <option value="Ocho Rios" selected=(city == "Ocho Rios")>"Ocho Rios (St. Ann)"</option>
                                <option value="Kingston" selected=(city == "Kingston")>"Kingston & St. Andrew"</option>
                            </select>
                        </div>
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Country (Editable)"</label>
                            <select
                                name="country"
                                class="select select-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-bold rounded-xl w-full shadow-xs"
                            >
                                <option value="Jamaica" selected=(country == "Jamaica")>"Jamaica 🇯🇲"</option>
                                <option value="Barbados" selected=(country == "Barbados")>"Barbados 🇧🇧"</option>
                                <option value="Bahamas" selected=(country == "Bahamas")>"Bahamas 🇧🇸"</option>
                                <option value="Saint Lucia" selected=(country == "Saint Lucia")>"Saint Lucia 🇱🇨"</option>
                                <option value="Cayman Islands" selected=(country == "Cayman Islands")>"Cayman Islands 🇰🇾"</option>
                                <option value="Turks and Caicos" selected=(country == "Turks and Caicos")>"Turks & Caicos 🇹🇨"</option>
                                <option value="Dominican Republic" selected=(country == "Dominican Republic")>"Dominican Republic 🇩🇴"</option>
                            </select>
                        </div>
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Latitude (GPS)"</label>
                            <input
                                type="number"
                                step="0.0001"
                                name="latitude"
                                value=(lat.to_string())
                                class="input input-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-mono text-xs rounded-xl w-full shadow-xs"
                            />
                        </div>
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Longitude (GPS)"</label>
                            <input
                                type="number"
                                step="0.0001"
                                name="longitude"
                                value=(lon.to_string())
                                class="input input-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-mono text-xs rounded-xl w-full shadow-xs"
                            />
                        </div>
                    </div>
                </div>

                // 3. Capacity & Specifications
                <div class="bg-base-100 dark:bg-base-200/90 p-6 md:p-8 rounded-3xl border-2 border-base-200 dark:border-base-100/30 shadow-md space-y-5">
                    <div class="flex items-center justify-between border-b border-base-200 pb-3">
                        <div class="flex items-center gap-2">
                            <span class="text-xl">"🛏️"</span>
                            <h2 class="text-lg font-serif font-bold text-base-content">
                                "3. Capacity, Accommodations & Floor Space"
                            </h2>
                        </div>
                        <span class="badge badge-accent badge-sm font-bold">"Floor Plan"</span>
                    </div>

                    <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-6 gap-4">
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Max Guests"</label>
                            <input type="number" name="max_guests" value=(guests) min="1" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Bedrooms"</label>
                            <input type="number" name="bedrooms" value=(bedrooms) min="1" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Beds"</label>
                            <input type="number" name="beds" value=(beds) min="1" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Full Baths"</label>
                            <input type="number" name="full_bathrooms" value=(full_baths) min="1" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Half Baths"</label>
                            <input type="number" name="half_bathrooms" value=(half_baths) min="0" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Sq. Meters (m²)"</label>
                            <input type="number" name="square_meters" value=(sq_meters) min="10" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                    </div>
                </div>

                // 4. Nightly Base Pricing & Discounts
                <div class="bg-base-100 dark:bg-base-200/90 p-6 md:p-8 rounded-3xl border-2 border-base-200 dark:border-base-100/30 shadow-md space-y-5">
                    <div class="flex items-center justify-between border-b border-base-200 pb-3">
                        <div class="flex items-center gap-2">
                            <span class="text-xl">"💰"</span>
                            <h2 class="text-lg font-serif font-bold text-base-content">
                                "4. Base Pricing, Stay Discounts & Turnover Policies"
                            </h2>
                        </div>
                        <span class="badge badge-warning badge-sm font-bold">"Financial Yield"</span>
                    </div>

                    <div class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-5 gap-4">
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Base Rate / Night"</label>
                            <div class="join w-full">
                                <select name="base_currency" class="select select-bordered border-2 border-base-300 join-item font-bold bg-base-100">
                                    <option value="USD" selected=(currency == "USD")>"USD"</option>
                                    <option value="JMD" selected=(currency == "JMD")>"JMD"</option>
                                    <option value="EUR" selected=(currency == "EUR")>"EUR"</option>
                                    <option value="GBP" selected=(currency == "GBP")>"GBP"</option>
                                </select>
                                <input type="number" name="price" value=(price) class="input input-bordered border-2 border-base-300 join-item w-full font-bold bg-base-100" />
                            </div>
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Min Stay (Nights)"</label>
                            <input type="number" name="minimum_stay" value=(min_stay) min="1" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Weekly Disc %"</label>
                            <input type="number" name="weekly_discount_percentage" value=(weekly_disc) min="0" max="50" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Monthly Disc %"</label>
                            <input type="number" name="monthly_discount_percentage" value=(monthly_disc) min="0" max="50" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                        <div class="bg-base-200/50 p-3.5 rounded-2xl border border-base-300 dark:border-base-content/10">
                            <label class="block text-[11px] font-bold uppercase tracking-wider text-base-content/70 mb-1">"Turnover Buffer"</label>
                            <input type="number" name="days_between_bookings" value=(days_between) min="0" max="7" class="input input-bordered border-2 border-base-300 bg-base-100 font-bold text-center w-full rounded-xl" />
                        </div>
                    </div>
                </div>

                // 5. Photos & Assets
                <div class="bg-base-100 dark:bg-base-200/90 p-6 md:p-8 rounded-3xl border-2 border-base-200 dark:border-base-100/30 shadow-md space-y-5">
                    <div class="flex items-center justify-between border-b border-base-200 pb-3">
                        <div class="flex items-center gap-2">
                            <span class="text-xl">"🖼️"</span>
                            <h2 class="text-lg font-serif font-bold text-base-content">
                                "5. Hero Photo Asset & Media Management"
                            </h2>
                        </div>
                        <span class="badge badge-info badge-sm font-bold">"GCS Signed Storage"</span>
                    </div>

                    <div>
                        <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Primary Hero Image URL"</label>
                        <input
                            type="url"
                            name="primary_image_url"
                            value=(image_url)
                            class="input input-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-medium rounded-xl w-full px-4 py-2.5 shadow-xs"
                        />
                    </div>
                </div>

                // 6. Bespoke Amenities
                <div class="bg-base-100 dark:bg-base-200/90 p-6 md:p-8 rounded-3xl border-2 border-base-200 dark:border-base-100/30 shadow-md space-y-5">
                    <div class="flex items-center justify-between border-b border-base-200 pb-3">
                        <div class="flex items-center gap-2">
                            <span class="text-xl">"✨"</span>
                            <h2 class="text-lg font-serif font-bold text-base-content">
                                "6. Bespoke Luxury Amenities Checklist"
                            </h2>
                        </div>
                        <span class="badge badge-success badge-sm font-bold">"Guest Features"</span>
                    </div>

                    <div class="grid grid-cols-2 sm:grid-cols-3 gap-3">
                        <label class="flex items-center gap-3 p-3.5 bg-base-200/60 hover:bg-base-200 border border-base-300 dark:border-base-content/10 rounded-2xl cursor-pointer transition-colors">
                            <input type="checkbox" checked=(true) class="checkbox checkbox-primary checkbox-sm" />
                            <span class="text-xs font-bold">"🏊 Infinity Pool"</span>
                        </label>
                        <label class="flex items-center gap-3 p-3.5 bg-base-200/60 hover:bg-base-200 border border-base-300 dark:border-base-content/10 rounded-2xl cursor-pointer transition-colors">
                            <input type="checkbox" checked=(true) class="checkbox checkbox-primary checkbox-sm" />
                            <span class="text-xs font-bold">"🏖 Private Beach Access"</span>
                        </label>
                        <label class="flex items-center gap-3 p-3.5 bg-base-200/60 hover:bg-base-200 border border-base-300 dark:border-base-content/10 rounded-2xl cursor-pointer transition-colors">
                            <input type="checkbox" checked=(true) class="checkbox checkbox-primary checkbox-sm" />
                            <span class="text-xs font-bold">"👨‍🍳 Dedicated Butler / Chef"</span>
                        </label>
                        <label class="flex items-center gap-3 p-3.5 bg-base-200/60 hover:bg-base-200 border border-base-300 dark:border-base-content/10 rounded-2xl cursor-pointer transition-colors">
                            <input type="checkbox" checked=(true) class="checkbox checkbox-primary checkbox-sm" />
                            <span class="text-xs font-bold">"📶 Starlink High-Speed WiFi"</span>
                        </label>
                        <label class="flex items-center gap-3 p-3.5 bg-base-200/60 hover:bg-base-200 border border-base-300 dark:border-base-content/10 rounded-2xl cursor-pointer transition-colors">
                            <input type="checkbox" checked=(true) class="checkbox checkbox-primary checkbox-sm" />
                            <span class="text-xs font-bold">"❄️ Central Air Conditioning"</span>
                        </label>
                        <label class="flex items-center gap-3 p-3.5 bg-base-200/60 hover:bg-base-200 border border-base-300 dark:border-base-content/10 rounded-2xl cursor-pointer transition-colors">
                            <input type="checkbox" checked=(true) class="checkbox checkbox-primary checkbox-sm" />
                            <span class="text-xs font-bold">"🍹 Sunset Cocktail Terrace"</span>
                        </label>
                    </div>
                </div>

                // Submit Bar
                <div class="p-6 bg-base-100 dark:bg-base-200 rounded-3xl border-2 border-base-200 flex flex-col sm:flex-row sm:items-center justify-between gap-4 shadow-xl">
                    <label class="flex items-center gap-3 cursor-pointer">
                        <input type="checkbox" name="is_active" checked=(is_active) class="toggle toggle-primary toggle-md" />
                        <div>
                            <div class="text-sm font-bold text-base-content">"Listing Active & Published"</div>
                            <div class="text-xs text-base-content/50">"Visible on public booking platform"</div>
                        </div>
                    </label>
                    <div class="flex items-center gap-3">
                        <a href="/admin/listings" class="btn btn-ghost rounded-full px-6 font-semibold">"Cancel"</a>
                        <button type="submit" class="btn btn-primary rounded-full px-8 font-bold tracking-wide shadow-lg">
                            "Save Changes"
                        </button>
                    </div>
                </div>
            </form>
        </div>
    }
}
