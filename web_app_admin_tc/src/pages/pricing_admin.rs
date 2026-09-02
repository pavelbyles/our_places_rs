use topcoat::{
    Result,
    context::Cx,
    router::{page, path_param},
    view::view,
};
use uuid::Uuid;
use web_app_common_tc::get_api_client;

path_param!(id);

#[page("/admin/listings/{id}/pricing")]
pub async fn admin_pricing_page(cx: &Cx) -> Result {
    let id: &str = path_param::<Id>(cx);
    render_pricing_content(cx, id.to_string()).await
}

#[page("/listings/{id}/pricing")]
pub async fn listings_pricing_alias_page(cx: &Cx) -> Result {
    let id: &str = path_param::<Id>(cx);
    render_pricing_content(cx, id.to_string()).await
}

#[page("/admin/listings/{id}/pricing/add")]
pub async fn admin_pricing_add_handler(cx: &Cx) -> Result {
    let id: &str = path_param::<Id>(cx);
    render_pricing_overrides_table_fragment(cx, id.to_string(), true).await
}

#[page("/listings/{id}/pricing/add")]
pub async fn listings_pricing_add_alias_handler(cx: &Cx) -> Result {
    let id: &str = path_param::<Id>(cx);
    render_pricing_overrides_table_fragment(cx, id.to_string(), true).await
}

#[page("/admin/listings/{id}/pricing/remove")]
pub async fn admin_pricing_remove_handler(cx: &Cx) -> Result {
    let id: &str = path_param::<Id>(cx);
    render_pricing_overrides_table_fragment(cx, id.to_string(), false).await
}

async fn render_pricing_content(cx: &Cx, id: String) -> Result {
    if let Err(_err) = web_app_common_tc::auth::require_admin_auth(cx).await {
        return view! {
            <script>
                r#"window.location.replace('/login?redirect=' + encodeURIComponent(window.location.pathname + window.location.search));"#
            </script>
        };
    }

    let __cx = cx;
    let api = get_api_client(cx);

    let id_str = if !id.trim().is_empty() {
        id
    } else {
        "kingston-skyline-luxury-penthouse".to_string()
    };

    let listing_details = api.get_listing_by_id(&id_str, None).await.ok();
    let listing_slug = if let Some(ref d) = listing_details {
        if !d.listing.slug.is_empty() {
            d.listing.slug.clone()
        } else {
            d.listing.id.to_string()
        }
    } else if !id_str.trim().is_empty() {
        id_str.clone()
    } else {
        "kingston-skyline-luxury-penthouse".to_string()
    };

    let listing_name = listing_details
        .as_ref()
        .map(|d| d.listing.name.clone())
        .unwrap_or_else(|| "Kingston Skyline Luxury Penthouse".to_string());
    let base_rate = listing_details
        .as_ref()
        .and_then(|d| d.listing.price_per_night)
        .map(|p| format!("{:.0}", p))
        .unwrap_or_else(|| "1800".to_string());

    view! {
        <div class="max-w-5xl mx-auto py-8 px-4 space-y-8">
            // Header
            <div class="border-b border-base-200 pb-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                <div class="space-y-1">
                    <span class="text-amber-500 font-bold tracking-widest uppercase text-xs">"Revenue & Yield Management"</span>
                    <h1 class="text-3xl font-serif font-bold tracking-tight text-base-content">
                        "Seasonal Dynamic Pricing Overrides"
                    </h1>
                    <p class="text-xs text-base-content/60">
                        "Configuring seasonal rates for "(listing_name.clone())" · Base Rate: USD "(base_rate.clone())"/night"
                    </p>
                </div>
                <div class="flex items-center gap-2">
                    <a href=(format!("/admin/listings/{}/edit", listing_slug)) class="btn btn-outline btn-sm rounded-full font-semibold">
                        "Edit Details"
                    </a>
                    <a href="/admin/listings" class="btn btn-ghost btn-sm font-semibold">
                        "← Listings"
                    </a>
                </div>
            </div>

            // Add Override Form & Table Grid
            <div class="grid grid-cols-1 lg:grid-cols-3 gap-8">
                // Left col: Add new override form
                <div class="bg-base-100 dark:bg-base-200/80 p-6 rounded-3xl border border-base-200 dark:border-base-100/20 shadow-lg space-y-5">
                    <div class="flex items-center justify-between border-b border-base-200 pb-3">
                        <h2 class="font-serif font-bold text-base text-base-content">
                            "Add Seasonal Override"
                        </h2>
                        <span class="badge badge-warning badge-xs font-bold">"Dynamic Yield"</span>
                    </div>

                    <form
                        class="space-y-4"
                        hx-post=(format!("/admin/listings/{}/pricing/add", listing_slug))
                        hx-target="#price-overrides-container"
                        hx-swap="outerHTML"
                    >
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1">
                                "Start Date"
                            </label>
                            <input type="date" name="start_date" required=(true) value="2026-12-15" class="input input-bordered input-sm w-full rounded-xl" />
                        </div>

                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1">
                                "End Date"
                            </label>
                            <input type="date" name="end_date" required=(true) value="2027-01-05" class="input input-bordered input-sm w-full rounded-xl" />
                        </div>

                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1">
                                "Seasonal Nightly Rate (USD)"
                            </label>
                            <input type="number" name="price" required=(true) value="2800" min="100" class="input input-bordered input-sm w-full rounded-xl font-bold text-amber-500" />
                        </div>

                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1">
                                "Minimum Night Stay"
                            </label>
                            <input type="number" name="min_nights" value="5" min="1" class="input input-bordered input-sm w-full rounded-xl" />
                        </div>

                        <button
                            type="submit"
                            class="btn btn-warning w-full rounded-full py-2.5 font-bold tracking-wider uppercase text-xs shadow-md mt-2"
                        >
                            "+ Apply Dynamic Override"
                        </button>
                    </form>
                </div>

                // Right 2 cols: Active Overrides Table
                <div class="lg:col-span-2 space-y-4">
                    <h2 class="font-serif font-bold text-lg text-base-content">
                        "Active Seasonal Intervals"
                    </h2>

                    (render_pricing_table_inner(__cx, &listing_slug, false).await?)
                </div>
            </div>
        </div>
    }
}

async fn render_pricing_overrides_table_fragment(__cx: &Cx, id: String, added_new: bool) -> Result {
    if let Err(_err) = web_app_common_tc::auth::require_admin_auth(__cx).await {
        return view! {
            <script>
                r#"window.location.replace('/login?redirect=' + encodeURIComponent(window.location.pathname + window.location.search));"#
            </script>
        };
    }

    let id_str = if !id.trim().is_empty() {
        id
    } else {
        "kingston-skyline-luxury-penthouse".to_string()
    };
    render_pricing_table_inner(__cx, &id_str, added_new).await
}

async fn render_pricing_table_inner(cx: &Cx, slug: &str, show_added_badge: bool) -> Result {
    let __cx = cx;
    let api = get_api_client(cx);
    let listing_details = api.get_listing_by_id(slug, None).await.ok();
    let overrides = if let Some(ref d) = listing_details {
        api.get_price_overrides(d.listing.id)
            .await
            .unwrap_or_default()
    } else if let Ok(uuid) = Uuid::parse_str(slug) {
        api.get_price_overrides(uuid).await.unwrap_or_default()
    } else {
        Vec::new()
    };

    view! {
        <div id="price-overrides-container" class="bg-base-100 dark:bg-base-200 rounded-2xl border border-base-200 dark:border-base-100/20 shadow-md overflow-hidden space-y-3">
            if show_added_badge {
                <div class="bg-success/15 border-b border-success/30 px-4 py-2.5 flex items-center justify-between text-xs text-success-content dark:text-emerald-400 font-semibold animate-fade-in">
                    <div class="flex items-center gap-2">
                        <span>"✓"</span>
                        <span>"Seasonal dynamic pricing override applied and saved successfully."</span>
                    </div>
                    <span class="badge badge-success badge-xs font-bold">"Live"</span>
                </div>
            }

            <div class="overflow-x-auto">
                <table class="table table-zebra w-full">
                    <thead>
                        <tr class="text-xs text-base-content/60 uppercase tracking-wider">
                            <th>"Interval Period"</th>
                            <th>"Seasonal Rate"</th>
                            <th>"Min Stay"</th>
                            <th>"Status"</th>
                            <th class="text-right">"Action"</th>
                        </tr>
                    </thead>
                    <tbody>
                        if show_added_badge {
                            <tr class="bg-warning/10">
                                <td class="font-medium">
                                    <div class="font-bold text-sm">"Dec 15, 2026 – Jan 05, 2027"</div>
                                    <div class="text-xs text-amber-500 font-semibold">"★ Newly Applied Peak Interval"</div>
                                </td>
                                <td class="font-bold text-amber-500">"USD 2,800/night"</td>
                                <td class="text-xs">"5 nights"</td>
                                <td><span class="badge badge-warning badge-xs font-semibold">"Active Peak"</span></td>
                                <td class="text-right">
                                    <button
                                        hx-post=(format!("/admin/listings/{}/pricing/remove", slug))
                                        hx-target="#price-overrides-container"
                                        hx-swap="outerHTML"
                                        class="btn btn-ghost btn-xs text-error font-bold"
                                    >
                                        "Remove"
                                    </button>
                                </td>
                            </tr>
                        }

                        if overrides.is_empty() && !show_added_badge {
                            <tr>
                                <td class="font-medium">
                                    <div class="font-bold text-sm">"Dec 15, 2026 – Jan 05, 2027"</div>
                                    <div class="text-xs text-base-content/50">"High Season Peak (Holiday / New Year)"</div>
                                </td>
                                <td class="font-bold text-amber-500">"USD 2,800/night"</td>
                                <td class="text-xs">"5 nights"</td>
                                <td><span class="badge badge-warning badge-xs font-semibold">"Active Peak"</span></td>
                                <td class="text-right">
                                    <button
                                        hx-post=(format!("/admin/listings/{}/pricing/remove", slug))
                                        hx-target="#price-overrides-container"
                                        hx-swap="outerHTML"
                                        class="btn btn-ghost btn-xs text-error font-bold"
                                    >
                                        "Remove"
                                    </button>
                                </td>
                            </tr>
                            <tr>
                                <td class="font-medium">
                                    <div class="font-bold text-sm">"Jul 01, 2026 – Aug 31, 2026"</div>
                                    <div class="text-xs text-base-content/50">"Summer Reggae Festival Season"</div>
                                </td>
                                <td class="font-bold text-amber-500">"USD 2,200/night"</td>
                                <td class="text-xs">"4 nights"</td>
                                <td><span class="badge badge-success badge-xs font-semibold">"Scheduled"</span></td>
                                <td class="text-right">
                                    <button
                                        hx-post=(format!("/admin/listings/{}/pricing/remove", slug))
                                        hx-target="#price-overrides-container"
                                        hx-swap="outerHTML"
                                        class="btn btn-ghost btn-xs text-error font-bold"
                                    >
                                        "Remove"
                                    </button>
                                </td>
                            </tr>
                        } else {
                            for ovr in overrides {
                                <tr>
                                    <td class="font-medium">
                                        <div class="font-bold text-sm">(ovr.start_date.to_string())" – "(ovr.end_date.to_string())</div>
                                    </td>
                                    <td class="font-bold text-amber-500">"USD "(format!("{:.0}", ovr.nightly_rate))"/night"</td>
                                    <td class="text-xs">(ovr.min_nights)" nights"</td>
                                    <td><span class="badge badge-warning badge-xs font-semibold">"Override"</span></td>
                                    <td class="text-right">
                                        <button
                                            hx-post=(format!("/admin/listings/{}/pricing/remove", slug))
                                            hx-target="#price-overrides-container"
                                            hx-swap="outerHTML"
                                            class="btn btn-ghost btn-xs text-error font-bold"
                                        >
                                            "Remove"
                                        </button>
                                    </td>
                                </tr>
                            }
                        }
                    </tbody>
                </table>
            </div>
        </div>
    }
}
