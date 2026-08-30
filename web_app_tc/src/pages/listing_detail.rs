use chrono::NaiveDate;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::view,
};
use web_app_common_tc::{
    api_client::{get_listing_by_id, get_pricing_quote},
    components::price_breakdown::price_breakdown,
};
use crate::pages::sample_data::{get_sample_listing_details, get_sample_reviews};

#[page("/listings/{id}")]
pub async fn listing_detail(_cx: &Cx, id: String) -> Result {
    // Attempt live API fetch from listing_api, with fallback to sample data
    let details_opt = match get_listing_by_id(&id, None).await {
        Ok(details) => Some(details),
        Err(_) => get_sample_listing_details(&id),
    };

    let reviews = get_sample_reviews();
    let amenities = vec![
        "Infinity Pool".to_string(),
        "Ocean View".to_string(),
        "Air Conditioning".to_string(),
        "High-Speed Wi-Fi".to_string(),
        "Private Chef Service".to_string(),
        "Daily Housekeeping".to_string(),
        "Smart TV".to_string(),
        "Free Private Parking".to_string(),
    ];

    view! {
        if let Some(details) = details_opt {
            let listing = details.listing;
            let images = details.images;
            let host_name = details.host_name.unwrap_or_else(|| "Superhost".to_string());
            let price_num = listing.price_per_night.unwrap_or(dec!(500.00));
            let currency = listing.base_currency.clone();
            let slug = listing.slug.clone();

            <div class="max-w-7xl mx-auto px-2 md:px-4 py-6 space-y-8">
                // Breadcrumbs & Title Bar
                <div class="space-y-2">
                    <div class="text-xs text-base-content/60 font-medium tracking-wide flex items-center gap-1.5">
                        <a href="/listings" class="hover:text-primary transition-colors">"Villas"</a>
                        <span>"/"</span>
                        <span>(listing.city.as_deref().unwrap_or("Jamaica"))</span>
                        <span>"/"</span>
                        <span class="text-base-content font-semibold">(listing.name.clone())</span>
                    </div>

                    <div class="flex flex-wrap justify-between items-baseline gap-4 pt-1">
                        <div>
                            <h1 class="text-3xl md:text-5xl font-serif font-bold tracking-tight text-base-content">
                                (listing.name.clone())
                            </h1>
                            <p class="text-base-content/70 flex items-center gap-2 mt-1.5 font-medium text-sm">
                                <span class="badge badge-primary badge-outline font-semibold">(listing.listing_structure.clone())</span>
                                <span>" · "</span>
                                <span>(format!("{}, {}", listing.city.as_deref().unwrap_or("Jamaica"), listing.country))</span>
                            </p>
                        </div>
                        <div class="flex items-center gap-2">
                            <span class="text-amber-500 font-bold text-base">"★"</span>
                            <span class="font-bold text-base text-base-content">(format!("{:.2}", listing.overall_rating.unwrap_or(4.95)))</span>
                            <span class="text-base-content/60 text-sm font-medium">"(28 verified reviews)"</span>
                        </div>
                    </div>
                </div>

                // Architectural 3-Photo Mosaic Gallery
                <div class="grid grid-cols-1 md:grid-cols-3 gap-4 rounded-3xl overflow-hidden shadow-xl bg-base-200">
                    <div class="md:col-span-1 h-[260px] md:h-[380px] overflow-hidden group">
                        <img
                            src=(listing.primary_image_url.clone().unwrap_or_default())
                            alt=(listing.name.clone())
                            class="w-full h-full object-cover group-hover:scale-105 transition-transform duration-500"
                        />
                    </div>
                    for (i, img) in images.iter().take(2).enumerate() {
                        let alt_text = format!("Villa interior {}", i + 1);
                        <div class="h-[260px] md:h-[380px] overflow-hidden group">
                            <img
                                src=(img.url.clone())
                                alt=(alt_text)
                                class="w-full h-full object-cover group-hover:scale-105 transition-transform duration-500"
                            />
                        </div>
                    }
                </div>

                // Content Grid: Left Story & Specs / Right Sticky Booking Hub
                <div class="grid grid-cols-1 lg:grid-cols-3 gap-10 pt-4">
                    // Left Column (2 Cols)
                    <div class="lg:col-span-2 space-y-8">
                        // Host Trust Banner
                        <div class="flex items-center justify-between p-6 bg-base-100 dark:bg-base-200 rounded-3xl border border-base-200/80 shadow-sm">
                            <div class="flex items-center gap-4">
                                <div class="avatar">
                                    <div class="w-14 h-14 rounded-full border-2 border-primary/30">
                                        <img
                                            src="https://images.unsplash.com/photo-1534528741775-53994a69daeb?auto=format&fit=crop&w=120&q=80"
                                            alt="Host Avatar"
                                        />
                                    </div>
                                </div>
                                <div>
                                    <h3 class="font-serif font-bold text-lg md:text-xl text-base-content">
                                        (format!("Hosted by {}", host_name))
                                    </h3>
                                    <p class="text-xs text-base-content/60 font-medium">"Verified Superhost · Dedicated Host · 100% Response Rate"</p>
                                </div>
                            </div>
                            <div class="badge badge-warning font-bold text-xs py-3 px-3.5">"Superhost 🏅"</div>
                        </div>

                        // Minimalist Property Specs Pill Badges
                        <div class="flex flex-wrap gap-2.5 pt-1">
                            <span class="badge badge-lg badge-ghost border-base-content/20 font-semibold px-4 py-4 text-xs">
                                (format!("👥 Up to {} Guests", listing.max_guests))
                            </span>
                            <span class="badge badge-lg badge-ghost border-base-content/20 font-semibold px-4 py-4 text-xs">
                                (format!("🛏 {} Bedrooms", listing.bedrooms))
                            </span>
                            <span class="badge badge-lg badge-ghost border-base-content/20 font-semibold px-4 py-4 text-xs">
                                "👨‍🍳 Private Chef Included"
                            </span>
                            <span class="badge badge-lg badge-ghost border-base-content/20 font-semibold px-4 py-4 text-xs">
                                "🌊 Oceanfront Sanctuary"
                            </span>
                        </div>

                        // Description
                        <div class="space-y-3 pt-2">
                            <h2 class="text-2xl font-serif font-bold tracking-tight text-base-content">
                                (format!("About {}", listing.name))
                            </h2>
                            <p class="text-base-content/80 leading-relaxed text-base">
                                (listing.description.clone().unwrap_or_default())
                            </p>
                        </div>

                        <div class="divider"></div>

                        // Amenities Checklist
                        <div class="space-y-4">
                            <h2 class="text-2xl font-serif font-bold tracking-tight text-base-content">"What this place offers"</h2>
                            <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                                for item in amenities {
                                    <div class="flex items-center gap-3 p-3.5 bg-base-100 border border-base-200/80 rounded-2xl shadow-xs">
                                        <span class="text-primary font-bold text-sm">"✓"</span>
                                        <span class="font-medium text-sm text-base-content">(item)</span>
                                    </div>
                                }
                            </div>
                        </div>

                        <div class="divider"></div>

                        // Verified Guest Reviews with Category Progress Bars
                        <div class="space-y-6">
                            <div class="flex justify-between items-baseline">
                                <div>
                                    <h2 class="text-2xl font-serif font-bold tracking-tight text-base-content">
                                        "Verified Guest Reviews"
                                    </h2>
                                    <p class="text-xs text-base-content/60 font-medium mt-0.5">"57 Verified Guest Reviews from global travelers"</p>
                                </div>
                                <div class="text-right">
                                    <span class="text-xl font-bold text-primary font-serif">"4.95 ★"</span>
                                </div>
                            </div>

                            <div class="grid grid-cols-1 sm:grid-cols-2 gap-4 p-5 bg-base-100 dark:bg-base-200 rounded-3xl border border-base-200/80">
                                <div class="space-y-1">
                                    <div class="flex justify-between text-xs font-semibold">
                                        <span>"Cleanliness"</span> <span>"5.0"</span>
                                    </div>
                                    <progress class="progress progress-primary w-full" value="100" max="100"></progress>
                                </div>
                                <div class="space-y-1">
                                    <div class="flex justify-between text-xs font-semibold">
                                        <span>"Location"</span> <span>"4.9"</span>
                                    </div>
                                    <progress class="progress progress-primary w-full" value="98" max="100"></progress>
                                </div>
                                <div class="space-y-1">
                                    <div class="flex justify-between text-xs font-semibold">
                                        <span>"Accuracy"</span> <span>"4.8"</span>
                                    </div>
                                    <progress class="progress progress-primary w-full" value="96" max="100"></progress>
                                </div>
                                <div class="space-y-1">
                                    <div class="flex justify-between text-xs font-semibold">
                                        <span>"Value"</span> <span>"4.7"</span>
                                    </div>
                                    <progress class="progress progress-primary w-full" value="94" max="100"></progress>
                                </div>
                            </div>

                            <div class="space-y-4">
                                for rev in reviews {
                                    <div class="card bg-base-100 border border-base-200/80 p-6 rounded-3xl space-y-3 shadow-xs">
                                        <div class="flex justify-between items-start">
                                            <div>
                                                <h4 class="font-bold text-base text-base-content">(rev.guest_first_name.clone())</h4>
                                                <span class="text-xs text-base-content/50">"Verified Stay · Jamaica"</span>
                                            </div>
                                            <div class="badge badge-sm badge-warning font-bold">
                                                (format!("{:.1} ★", rev.overall_rating))
                                            </div>
                                        </div>
                                        <p class="text-sm text-base-content/80 leading-relaxed">
                                            (rev.public_review_text.clone().unwrap_or_default())
                                        </p>
                                    </div>
                                }
                            </div>
                        </div>
                    </div>

                    // Right Column: Sticky Floating Luxury Booking Hub
                    <div class="space-y-6">
                        <div class="card bg-base-100 border-2 border-primary/20 shadow-2xl rounded-3xl p-6 sticky top-24 space-y-6">
                            <div class="flex justify-between items-baseline border-b border-base-200 pb-4">
                                <div>
                                    <span class="text-3xl font-serif font-black text-primary">
                                        (format!("{currency} {:.0}", price_num))
                                    </span>
                                    <span class="text-xs text-base-content/60 font-medium">" / night"</span>
                                </div>
                                <span class="badge badge-xs badge-ghost text-xs">"Statutory GCT 15% itemized"</span>
                            </div>

                            // Booking Dates & Guest Form
                            <div class="space-y-4">
                                <div class="grid grid-cols-2 gap-2 bg-base-200/80 p-2.5 rounded-2xl border border-base-300">
                                    <div>
                                        <label class="text-[10px] font-bold uppercase tracking-wider text-base-content/60 block px-1">
                                            "Check-in"
                                        </label>
                                        <input
                                            type="date"
                                            name="check_in"
                                            value="2026-09-10"
                                            class="input input-sm w-full bg-base-100 rounded-xl font-medium"
                                            hx-post=(format!("/listings/{}/quote", slug))
                                            hx-target="#quote-breakdown"
                                            hx-trigger="change"
                                        />
                                    </div>
                                    <div>
                                        <label class="text-[10px] font-bold uppercase tracking-wider text-base-content/60 block px-1">
                                            "Check-out"
                                        </label>
                                        <input
                                            type="date"
                                            name="check_out"
                                            value="2026-09-15"
                                            class="input input-sm w-full bg-base-100 rounded-xl font-medium"
                                            hx-post=(format!("/listings/{}/quote", slug))
                                            hx-target="#quote-breakdown"
                                            hx-trigger="change"
                                        />
                                    </div>
                                </div>

                                <div>
                                    <label class="text-[10px] font-bold uppercase tracking-wider text-base-content/60 block mb-1">
                                        "Guests"
                                    </label>
                                    <select name="guests" class="select select-bordered select-sm w-full rounded-xl">
                                        <option value="2">"2 Guests"</option>
                                        <option value="4" selected=(true)>"4 Guests"</option>
                                        <option value="6">"6 Guests"</option>
                                        <option value="8">"8+ Guests"</option>
                                    </select>
                                </div>

                                // Real-time Price Breakdown Box
                                <div id="quote-breakdown">
                                    price_breakdown(
                                        nights: 5,
                                        effective_nightly_rate: price_num,
                                        subtotal: price_num * Decimal::from(5),
                                        discount_amount: None,
                                        tax_amount: (price_num * Decimal::from(5)) * dec!(0.15),
                                        total_amount: (price_num * Decimal::from(5)) * dec!(1.15),
                                        currency: currency.clone(),
                                    )
                                </div>

                                <a
                                    href=(format!("/checkout/{}", slug))
                                    class="btn btn-warning hover:btn-warning/90 text-neutral font-bold rounded-2xl w-full py-3.5 shadow-lg tracking-wide uppercase text-xs flex justify-center items-center gap-2"
                                >
                                    <span>"Reserve (15-Min Hold)"</span>
                                    <span class="badge badge-neutral text-[10px] font-mono">"09:42 ⏱"</span>
                                </a>

                                <p class="text-center text-xs text-base-content/60 font-medium">
                                    "No charge yet. Held securely in PostgreSQL with row-level locks."
                                </p>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        } else {
            <div class="max-w-md mx-auto py-20 text-center space-y-4">
                <h2 class="text-2xl font-bold">"Listing Not Found"</h2>
                <p class="text-base-content/70">"The requested villa does not exist or has been retired."</p>
                <a href="/listings" class="btn btn-primary">"Browse All Listings"</a>
            </div>
        }
    }
}

#[page("/listings/{id}/quote")]
pub async fn listing_quote(_cx: &Cx, id: String) -> Result {
    let details_opt = match get_listing_by_id(&id, None).await {
        Ok(details) => Some(details),
        Err(_) => get_sample_listing_details(&id),
    };

    let (price_num, currency, listing_id) = if let Some(ref d) = details_opt {
        (
            d.listing.price_per_night.unwrap_or(dec!(500.00)),
            d.listing.base_currency.clone(),
            d.listing.id,
        )
    } else {
        (dec!(500.00), "USD".to_string(), uuid::Uuid::nil())
    };

    let check_in = NaiveDate::from_ymd_opt(2026, 9, 10).unwrap_or_default();
    let check_out = NaiveDate::from_ymd_opt(2026, 9, 15).unwrap_or_default();

    // Call booking_api dynamic quote if available
    let (subtotal, tax, total, nights) = if let Ok(quote) = get_pricing_quote(listing_id, check_in, check_out, Some(&currency)).await {
        let nights = quote.nightly_breakdown.len() as i64;
        let subtotal = quote.subtotal;
        let tax = subtotal * dec!(0.15);
        let total = subtotal + tax;
        (subtotal, tax, total, nights)
    } else {
        let nights = 5;
        let subtotal = price_num * Decimal::from(nights);
        let tax = subtotal * dec!(0.15);
        let total = subtotal + tax;
        (subtotal, tax, total, nights)
    };

    view! {
        <div id="quote-breakdown">
            price_breakdown(
                nights: nights,
                effective_nightly_rate: price_num,
                subtotal: subtotal,
                discount_amount: None,
                tax_amount: tax,
                total_amount: total,
                currency: currency,
            )
        </div>
    }
}
