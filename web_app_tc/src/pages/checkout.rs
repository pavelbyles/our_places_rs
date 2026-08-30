use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::view,
};
use web_app_common_tc::api_client::get_listing_by_id;
use web_app_common_tc::components::price_breakdown::price_breakdown;
use crate::pages::sample_data::get_sample_listing_details;

#[page("/checkout/{id}")]
pub async fn checkout_page(_cx: &Cx, id: String) -> Result {
    let details_opt = match get_listing_by_id(&id, None).await {
        Ok(details) => Some(details),
        Err(_) => get_sample_listing_details(&id),
    };

    view! {
        if let Some(details) = details_opt {
            let listing = details.listing;
            let price_num = listing.price_per_night.unwrap_or(dec!(500.00));
            let currency = listing.base_currency.clone();
            let nights = 5;
            let subtotal = price_num * Decimal::from(nights);
            let tax = subtotal * dec!(0.15);
            let total = subtotal + tax;

            <div class="max-w-5xl mx-auto px-2 md:px-4 py-8 space-y-8">
                // Reservation Hold Banner with 15-Min Timer
                <div class="bg-amber-500/15 border border-amber-500/40 text-amber-600 dark:text-amber-400 p-4 rounded-3xl shadow-sm flex flex-col sm:flex-row justify-between items-center gap-4">
                    <div class="flex items-center gap-3">
                        <span class="text-2xl">"⏱"</span>
                        <div>
                            <h3 class="font-bold text-sm">"Temporary Date Hold Active"</h3>
                            <div class="text-xs opacity-90">"These dates are held exclusively for you in PostgreSQL for 15 minutes."</div>
                        </div>
                    </div>
                    <div class="badge badge-lg bg-neutral text-neutral-content font-mono font-bold px-4 py-3">
                        "14:45 Remaining"
                    </div>
                </div>

                <div class="grid grid-cols-1 md:grid-cols-3 gap-8">
                    // Left Column: Guest Details Form
                    <div class="md:col-span-2 space-y-6">
                        <div class="card bg-base-100 border border-base-200/80 shadow-md p-6 md:p-8 rounded-3xl space-y-6">
                            <div>
                                <h2 class="text-2xl font-serif font-bold tracking-tight text-base-content">
                                    "Guest Information"
                                </h2>
                                <p class="text-xs text-base-content/60 font-medium mt-1">
                                    "Enter your contact details to finalize the reservation."
                                </p>
                            </div>
                            
                            <form action="/bookings" method="GET" class="space-y-5">
                                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                                    <div>
                                        <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70">"First Name"</label>
                                        <input type="text" name="first_name" placeholder="Marcus" class="input input-bordered w-full rounded-xl" required=(true) />
                                    </div>
                                    <div>
                                        <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70">"Last Name"</label>
                                        <input type="text" name="last_name" placeholder="Sterling" class="input input-bordered w-full rounded-xl" required=(true) />
                                    </div>
                                </div>

                                <div>
                                    <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70">"Email Address"</label>
                                    <input type="email" name="email" placeholder="marcus@example.com" class="input input-bordered w-full rounded-xl" required=(true) />
                                    <span class="text-[11px] text-base-content/60 mt-1 block">"We'll send your booking confirmation and host direct-line here."</span>
                                </div>

                                <div>
                                    <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70">"Phone Number"</label>
                                    <input type="tel" name="phone" placeholder="+1 (876) 555-0199" class="input input-bordered w-full rounded-xl" />
                                </div>

                                <div class="divider"></div>

                                <div class="space-y-4">
                                    <h3 class="font-serif font-bold text-lg text-base-content">"Arrival & Special Requests"</h3>
                                    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                                        <div>
                                            <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70">"Estimated Arrival"</label>
                                            <select name="arrival_time" class="select select-bordered w-full rounded-xl">
                                                <option value="15:00">"3:00 PM (Standard Check-in)"</option>
                                                <option value="17:00">"5:00 PM"</option>
                                                <option value="19:00">"7:00 PM"</option>
                                                <option value="custom">"Later (Notify Concierge)"</option>
                                            </select>
                                        </div>
                                        <div>
                                            <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70">"Stay Purpose"</label>
                                            <select name="is_business" class="select select-bordered w-full rounded-xl">
                                                <option value="false">"Vacation / Leisure"</option>
                                                <option value="true">"Work / Corporate Retreat"</option>
                                            </select>
                                        </div>
                                    </div>

                                    <div>
                                        <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70">"Special Requests for Host"</label>
                                        <textarea
                                            name="message_to_host"
                                            placeholder="Dietary preferences for private chef, airport transfer details, or celebration arrangements..."
                                            class="textarea textarea-bordered w-full h-24 rounded-xl"
                                        ></textarea>
                                    </div>
                                </div>

                                <div class="pt-4">
                                    <button type="submit" class="btn btn-primary btn-block py-4 text-base font-bold rounded-2xl shadow-xl tracking-wide uppercase">
                                        "Confirm & Finalize Reservation →"
                                    </button>
                                </div>
                            </form>
                        </div>
                    </div>

                    // Right Column: Order Summary
                    <div class="space-y-6">
                        <div class="card bg-base-100 border border-base-200/80 shadow-lg p-6 rounded-3xl space-y-4">
                            <div class="flex gap-4 items-center">
                                <img
                                    src=(listing.primary_image_url.clone().unwrap_or_default())
                                    alt=(listing.name.clone())
                                    class="w-20 h-20 rounded-2xl object-cover shadow-sm"
                                />
                                <div>
                                    <h4 class="font-serif font-bold text-base leading-snug text-base-content">
                                        (listing.name.clone())
                                    </h4>
                                    <span class="text-xs text-base-content/60 font-medium">
                                        (format!("{}, {}", listing.city.as_deref().unwrap_or("Jamaica"), listing.country))
                                    </span>
                                </div>
                            </div>

                            <div class="divider my-1"></div>

                            <div class="text-xs space-y-2 text-base-content/80 font-medium">
                                <div class="flex justify-between">
                                    <span class="font-semibold text-base-content">"Dates:"</span>
                                    <span>"Sep 10 – Sep 15, 2026 (5 nights)"</span>
                                </div>
                                <div class="flex justify-between">
                                    <span class="font-semibold text-base-content">"Guests:"</span>
                                    <span>"4 Guests"</span>
                                </div>
                                <div class="flex justify-between">
                                    <span class="font-semibold text-base-content">"Cancellation:"</span>
                                    <span class="badge badge-xs badge-success font-semibold">"Flexible 48h"</span>
                                </div>
                            </div>

                            <div class="divider my-1"></div>

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
                    </div>
                </div>
            </div>
        } else {
            <div class="max-w-md mx-auto py-20 text-center space-y-4">
                <h2 class="text-2xl font-bold">"Checkout Session Expired"</h2>
                <a href="/listings" class="btn btn-primary">"Find a Place"</a>
            </div>
        }
    }
}
