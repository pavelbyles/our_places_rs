use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use topcoat::{
    Result,
    context::Cx,
    router::{page, path_param},
    view::view,
};
use web_app_common_tc::{
    components::price_breakdown::price_breakdown,
    get_api_client,
};

path_param!(slug);

#[page("/checkout/{slug}")]
pub async fn checkout_page(cx: &Cx) -> Result {
    let slug: &str = path_param::<Slug>(cx);
    let (slug_str, curr) = if let Some(s) = slug.strip_suffix("-jmd") {
        (s.to_string(), "JMD".to_string())
    } else if let Some(s) = slug.strip_suffix("-eur") {
        (s.to_string(), "EUR".to_string())
    } else if let Some(s) = slug.strip_suffix("-gbp") {
        (s.to_string(), "GBP".to_string())
    } else if let Some(s) = slug.strip_suffix("-cad") {
        (s.to_string(), "CAD".to_string())
    } else {
        (slug.to_string(), "USD".to_string())
    };
    render_checkout(cx, slug_str, curr).await
}

#[page("/checkout-jmd/{id}")]
pub async fn checkout_jmd_page(cx: &Cx, id: String) -> Result {
    render_checkout(cx, id, "JMD".to_string()).await
}

#[page("/checkout-eur/{id}")]
pub async fn checkout_eur_page(cx: &Cx, id: String) -> Result {
    render_checkout(cx, id, "EUR".to_string()).await
}

#[page("/checkout-gbp/{id}")]
pub async fn checkout_gbp_page(cx: &Cx, id: String) -> Result {
    render_checkout(cx, id, "GBP".to_string()).await
}

#[page("/checkout-cad/{id}")]
pub async fn checkout_cad_page(cx: &Cx, id: String) -> Result {
    render_checkout(cx, id, "CAD".to_string()).await
}

async fn render_checkout(cx: &Cx, id: String, settlement_currency: String) -> Result {
    let __cx = cx;
    let api = get_api_client(cx);
    let details_opt = api.get_listing_by_id(&id, None).await.ok();

    let target_curr = settlement_currency.to_uppercase();

    // Standard tri-currency exchange rates relative to USD
    let fx_rate = match target_curr.as_str() {
        "JMD" => dec!(155.50),
        "EUR" => dec!(0.92),
        "GBP" => dec!(0.79),
        "CAD" => dec!(1.36),
        _ => dec!(1.00),
    };

    view! {
        if let Some(details) = details_opt {
            let listing = details.listing;
            let slug = listing.slug.clone();
            let base_price_usd = listing.price_per_night.unwrap_or(dec!(500.00));
            let converted_nightly_rate = base_price_usd * fx_rate;
            let nights = 5;
            let subtotal = converted_nightly_rate * Decimal::from(nights);
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
                    <div class="badge badge-lg bg-neutral text-neutral-content font-mono font-bold px-4 py-3" id="hold-timer-display">
                        "15:00 Remaining"
                    </div>
                </div>

                <script>
                    r#"
                    (function() {
                        var duration = 15 * 60;
                        var storageKey = 'ourplaces_hold_expires_at';
                        var now = Math.floor(Date.now() / 1000);
                        var expiresAt = parseInt(sessionStorage.getItem(storageKey), 10);

                        if (!expiresAt || expiresAt - now === 0 || Math.sign(expiresAt - now) === -1) {
                            expiresAt = now + duration;
                            sessionStorage.setItem(storageKey, expiresAt.toString());
                        }

                        function updateTimer() {
                            var currentNow = Math.floor(Date.now() / 1000);
                            var remaining = expiresAt - currentNow;
                            var el = document.getElementById('hold-timer-display');
                            if (!el) return;

                            if (remaining === 0 || Math.sign(remaining) === -1) {
                                el.innerText = '00:00 Expired';
                                el.className = 'badge badge-lg bg-error text-error-content font-mono font-bold px-4 py-3 animate-pulse';
                                return;
                            }

                            var mins = Math.floor(remaining / 60);
                            var secs = remaining % 60;
                            var minStr = String(mins).padStart(2, '0');
                            var secStr = String(secs).padStart(2, '0');
                            el.innerText = minStr + ':' + secStr + ' Remaining';
                        }

                        updateTimer();
                        setInterval(updateTimer, 1000);
                    })();
                    "#
                </script>

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
                            
                            <form class="space-y-5" id="checkout-form" onsubmit="window.submitBookingCheckout(event)">
                                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                                    <div>
                                        <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70">"First Name"</label>
                                        <input type="text" id="guest-first-name" name="first_name" placeholder="Marcus" class="input input-bordered w-full rounded-xl" required=(true) />
                                    </div>
                                    <div>
                                        <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70">"Last Name"</label>
                                        <input type="text" id="guest-last-name" name="last_name" placeholder="Sterling" class="input input-bordered w-full rounded-xl" required=(true) />
                                    </div>
                                </div>

                                <div>
                                    <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70">"Email Address"</label>
                                    <input type="email" id="guest-email" name="email" placeholder="marcus@example.com" class="input input-bordered w-full rounded-xl" required=(true) />
                                    <span class="text-[11px] text-base-content/60 mt-1 block">"We'll send your booking confirmation and host direct-line here."</span>
                                </div>

                                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                                    <div>
                                        <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70">"Phone Number"</label>
                                        <input type="tel" id="guest-phone" name="phone" placeholder="+1 (876) 555-0199" class="input input-bordered w-full rounded-xl" />
                                    </div>
                                    <div>
                                        <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70">"Total Guests"</label>
                                        <select
                                            id="guest-count-select"
                                            name="number_of_persons"
                                            class="select select-bordered w-full rounded-xl"
                                            onchange="renderAdditionalGuestInputs()"
                                        >
                                            <option value="1">"1 Guest (Lead Guest Only)"</option>
                                            <option value="2">"2 Guests"</option>
                                            <option value="3">"3 Guests"</option>
                                            <option value="4" selected=(true)>"4 Guests"</option>
                                            <option value="5">"5 Guests"</option>
                                            <option value="6">"6 Guests"</option>
                                            <option value="8">"8 Guests"</option>
                                            <option value="10">"10 Guests"</option>
                                        </select>
                                    </div>
                                </div>

                                // Dynamic Additional Guests Section
                                <div id="additional-guests-section" class="space-y-3 pt-2">
                                    <div class="flex items-center justify-between border-b border-base-200 pb-2">
                                        <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70 p-0">
                                            "Accompanying Guests"
                                        </label>
                                        <span class="badge badge-sm badge-ghost text-[10px] font-semibold" id="additional-guests-badge">
                                            "3 additional guests"
                                        </span>
                                    </div>
                                    <div id="additional-guests-fields" class="grid grid-cols-1 sm:grid-cols-2 gap-3"></div>
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

                            // Settlement Currency Switcher
                            <div class="bg-base-200/60 p-3 rounded-2xl border border-base-300 space-y-2">
                                <div class="flex justify-between items-center text-xs">
                                    <span class="font-bold text-base-content/70 uppercase tracking-wider text-[10px]">"Settlement Currency"</span>
                                    <span class="badge badge-sm badge-ghost font-bold font-mono">(target_curr.clone())</span>
                                </div>
                                <div class="grid grid-cols-5 gap-1 text-center">
                                    <a
                                        href=(format!("/checkout/{}", slug))
                                        class=(if target_curr == "USD" { "btn btn-primary btn-xs rounded-lg font-bold" } else { "btn btn-ghost btn-xs rounded-lg font-semibold" })
                                    >
                                        "USD"
                                    </a>
                                    <a
                                        href=(format!("/checkout-jmd/{}", slug))
                                        class=(if target_curr == "JMD" { "btn btn-primary btn-xs rounded-lg font-bold" } else { "btn btn-ghost btn-xs rounded-lg font-semibold" })
                                    >
                                        "JMD"
                                    </a>
                                    <a
                                        href=(format!("/checkout-eur/{}", slug))
                                        class=(if target_curr == "EUR" { "btn btn-primary btn-xs rounded-lg font-bold" } else { "btn btn-ghost btn-xs rounded-lg font-semibold" })
                                    >
                                        "EUR"
                                    </a>
                                    <a
                                        href=(format!("/checkout-gbp/{}", slug))
                                        class=(if target_curr == "GBP" { "btn btn-primary btn-xs rounded-lg font-bold" } else { "btn btn-ghost btn-xs rounded-lg font-semibold" })
                                    >
                                        "GBP"
                                    </a>
                                    <a
                                        href=(format!("/checkout-cad/{}", slug))
                                        class=(if target_curr == "CAD" { "btn btn-primary btn-xs rounded-lg font-bold" } else { "btn btn-ghost btn-xs rounded-lg font-semibold" })
                                    >
                                        "CAD"
                                    </a>
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

                            // Tri-Currency Settlement Guarantee Callout
                            if target_curr != "USD" {
                                <div class="bg-info/10 border border-info/30 rounded-xl p-3 text-[11px] space-y-1">
                                    <div class="font-bold text-info flex items-center gap-1">
                                        <span>"💱"</span>
                                        <span>"Tri-Currency FX Guarantee"</span>
                                    </div>
                                    <div class="text-base-content/70">
                                        (format!("Base Rate: USD ${:.2} · FX Lock: 1 USD = {:.2} {}", base_price_usd, fx_rate, target_curr))
                                    </div>
                                </div>
                            }

                            price_breakdown(
                                nights: nights,
                                effective_nightly_rate: converted_nightly_rate,
                                subtotal: subtotal,
                                discount_amount: None,
                                tax_amount: tax,
                                total_amount: total,
                                currency: target_curr,
                            )
                        </div>
                    </div>
                </div>

                <script>
                    r#"
                    (function() {
                        try {
                            var userJson = localStorage.getItem('op_auth_user');
                            if (!userJson) {
                                userJson = sessionStorage.getItem('op_auth_user');
                            }
                            if (userJson) {
                                var user = JSON.parse(userJson);
                                var emailEl = document.getElementById('guest-email');
                                var fnEl = document.getElementById('guest-first-name');
                                var lnEl = document.getElementById('guest-last-name');
                                
                                if (emailEl) {
                                    if (user.email) {
                                        emailEl.value = user.email;
                                    }
                                }
                                if (user.name) {
                                    var parts = user.name.split(' ');
                                    if (fnEl) {
                                        if (parts[0]) fnEl.value = parts[0];
                                    }
                                    if (lnEl) {
                                        if (parts.length === 1) {
                                            lnEl.value = '';
                                        } else {
                                            lnEl.value = parts.slice(1).join(' ');
                                        }
                                    }
                                }
                            }
                        } catch(e) {}

                        window.renderAdditionalGuestInputs = function() {
                            try {
                                var countSelect = document.getElementById('guest-count-select');
                                var container = document.getElementById('additional-guests-fields');
                                var section = document.getElementById('additional-guests-section');
                                var badge = document.getElementById('additional-guests-badge');
                                if (!countSelect) return;
                                if (!container) return;
                                if (!section) return;
                                
                                var totalGuests = parseInt(countSelect.value, 10) || 1;
                                container.innerHTML = '';
                                
                                if (totalGuests === 1) {
                                    section.style.display = 'none';
                                } else {
                                    section.style.display = 'block';
                                    var extraCount = totalGuests - 1;
                                    if (badge) {
                                        badge.innerText = extraCount + ' additional guest' + (extraCount === 1 ? '' : 's');
                                    }
                                    
                                    var g = 2;
                                    while (g !== totalGuests + 1) {
                                        var wrapper = document.createElement('div');
                                        wrapper.className = 'space-y-1';
                                        
                                        var label = document.createElement('label');
                                        label.className = 'text-[11px] font-bold text-base-content/70 uppercase block px-1';
                                        label.innerText = 'Guest ' + g + ' Full Name *';
                                        
                                        var input = document.createElement('input');
                                        input.type = 'text';
                                        input.name = 'guest_name_' + g;
                                        input.placeholder = 'e.g. Guest ' + g + ' Full Name';
                                        input.required = true;
                                        input.className = 'input input-bordered input-sm w-full rounded-xl';
                                        
                                        wrapper.appendChild(label);
                                        wrapper.appendChild(input);
                                        container.appendChild(wrapper);
                                        g = g + 1;
                                    }
                                }
                            } catch(e) {
                                console.error('Failed to render additional guests:', e);
                            }
                        };

                        window.submitBookingCheckout = function(e) {
                            if (e) {
                                try { e.preventDefault(); } catch(err) {}
                            }
                            try {
                                var guestCount = document.getElementById('guest-count-select').value;
                                var fn = document.getElementById('guest-first-name').value;
                                var ln = document.getElementById('guest-last-name').value;
                                var em = document.getElementById('guest-email').value;
                                var newBookingId = 'OP-2026-' + Math.floor(1000 + Math.random() * 9000);
                                
                                var newBooking = {
                                    id: newBookingId,
                                    villa_name: 'The Reef House',
                                    villa_slug: 'the-reef-house',
                                    location: 'Discovery Bay, St. Ann, Jamaica',
                                    image_url: 'https://images.unsplash.com/photo-1613490493576-7fde63acd811?auto=format&fit=crop&w=800&q=80',
                                    guest_name: fn + ' ' + ln,
                                    guest_email: em,
                                    check_in: 'Sep 10, 2026 (3:00 PM)',
                                    check_out: 'Sep 15, 2026 (11:00 AM)',
                                    guests: guestCount + ' Guests',
                                    total_formatted: 'USD $14,490.00',
                                    status: 'Confirmed Stay',
                                    booked_at: new Date().toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })
                                };
                                
                                var existingJson = localStorage.getItem('op_guest_bookings');
                                var bookings = [];
                                if (existingJson) {
                                    try { bookings = JSON.parse(existingJson); } catch(err) {}
                                }
                                bookings.unshift(newBooking);
                                localStorage.setItem('op_guest_bookings', JSON.stringify(bookings));
                            } catch(err) {
                                console.error('Failed to save booking:', err);
                            }
                            window.location.href = '/bookings';
                        };

                        renderAdditionalGuestInputs();
                    })();
                    "#
                </script>
            </div>
        } else {
            <div class="max-w-md mx-auto py-20 text-center space-y-4">
                <h2 class="text-2xl font-bold">"Checkout Session Expired"</h2>
                <a href="/listings" class="btn btn-primary">"Find a Place"</a>
            </div>
        }
    }
}
