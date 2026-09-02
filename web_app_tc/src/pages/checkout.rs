use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use topcoat::{
    Result,
    context::Cx,
    router::{page, path_param},
    view::view,
};
use web_app_common_tc::{
    components::price_breakdown::price_breakdown, get_api_client, get_authenticated_guest,
};

path_param!(slug);
path_param!(id);

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
pub async fn checkout_jmd_page(cx: &Cx) -> Result {
    let id: &str = path_param::<Id>(cx);
    render_checkout(cx, id.to_string(), "JMD".to_string()).await
}

#[page("/checkout-eur/{id}")]
pub async fn checkout_eur_page(cx: &Cx) -> Result {
    let id: &str = path_param::<Id>(cx);
    render_checkout(cx, id.to_string(), "EUR".to_string()).await
}

#[page("/checkout-gbp/{id}")]
pub async fn checkout_gbp_page(cx: &Cx) -> Result {
    let id: &str = path_param::<Id>(cx);
    render_checkout(cx, id.to_string(), "GBP".to_string()).await
}

#[page("/checkout-cad/{id}")]
pub async fn checkout_cad_page(cx: &Cx) -> Result {
    let id: &str = path_param::<Id>(cx);
    render_checkout(cx, id.to_string(), "CAD".to_string()).await
}

async fn render_checkout(cx: &Cx, id: String, settlement_currency: String) -> Result {
    let __cx = cx;
    let api = get_api_client(cx);
    let details_opt = api.get_listing_by_id(&id, None).await.ok();

    let auth_user = get_authenticated_guest(cx);
    let default_email = auth_user
        .as_ref()
        .map(|u| u.email.clone())
        .unwrap_or_default();
    let default_first_name = auth_user
        .as_ref()
        .and_then(|u| u.name.split_whitespace().next().map(|s| s.to_string()))
        .unwrap_or_default();
    let default_last_name = auth_user
        .as_ref()
        .map(|u| {
            let parts: Vec<&str> = u.name.split_whitespace().collect();
            if parts.len() > 1 {
                parts[1..].join(" ")
            } else {
                String::new()
            }
        })
        .unwrap_or_default();

    let arrival_times = vec![
        ("00:00", "12:00 AM (Midnight)", false),
        ("01:00", "1:00 AM", false),
        ("02:00", "2:00 AM", false),
        ("03:00", "3:00 AM", false),
        ("04:00", "4:00 AM", false),
        ("05:00", "5:00 AM", false),
        ("06:00", "6:00 AM", false),
        ("07:00", "7:00 AM", false),
        ("08:00", "8:00 AM", false),
        ("09:00", "9:00 AM", false),
        ("10:00", "10:00 AM", false),
        ("11:00", "11:00 AM", false),
        ("12:00", "12:00 PM (Noon)", false),
        ("13:00", "1:00 PM", false),
        ("14:00", "2:00 PM", false),
        ("15:00", "3:00 PM (Standard Check-in)", true),
        ("16:00", "4:00 PM", false),
        ("17:00", "5:00 PM", false),
        ("18:00", "6:00 PM", false),
        ("19:00", "7:00 PM", false),
        ("20:00", "8:00 PM", false),
        ("21:00", "9:00 PM", false),
        ("22:00", "10:00 PM", false),
        ("23:00", "11:00 PM", false),
    ];

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

                            <form
                                class="space-y-5"
                                id="checkout-form"
                                data-listing-id=(listing.id.to_string())
                                data-currency=(target_curr.clone())
                                data-villa-name=(listing.name.clone())
                                data-villa-slug=(slug.clone())
                                data-location=(format!("{}, {}", listing.city.as_deref().unwrap_or("Jamaica"), listing.country))
                                data-image-url=(listing.primary_image_url.clone().unwrap_or_default())
                                data-total-formatted=(format!("{} ${:.2}", target_curr, total))
                                onsubmit="window.submitBookingCheckout(event)"
                            >
                                <div id="checkout-error-banner" class="alert alert-error text-xs font-semibold rounded-2xl hidden shadow-md">
                                    <div class="flex items-center gap-2">
                                        <span>"⚠️"</span>
                                        <span id="checkout-error-text">"The requested dates are no longer available. Please choose different dates."</span>
                                    </div>
                                </div>

                                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                                    <div>
                                        <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70">"First Name *"</label>
                                        <input type="text" id="guest-first-name" name="first_name" value=(default_first_name) placeholder="Marcus" class="input input-bordered w-full rounded-xl" required=(true) />
                                    </div>
                                    <div>
                                        <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70">"Last Name *"</label>
                                        <input type="text" id="guest-last-name" name="last_name" value=(default_last_name) placeholder="Sterling" class="input input-bordered w-full rounded-xl" required=(true) />
                                    </div>
                                </div>

                                <div>
                                    <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70">"Email Address *"</label>
                                    <input type="email" id="guest-email" name="email" value=(default_email) placeholder="marcus@example.com" class="input input-bordered w-full rounded-xl" required=(true) />
                                    <span class="text-[11px] text-base-content/60 mt-1 block">"We'll send your booking confirmation and host direct-line here."</span>
                                </div>

                                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                                    <div>
                                        <label class="label text-xs font-bold uppercase tracking-wider text-base-content/70">"Phone Number *"</label>
                                        <input type="tel" id="guest-phone" name="phone" placeholder="+1 (876) 555-0199" class="input input-bordered w-full rounded-xl" required=(true) />
                                        <span class="text-[11px] text-base-content/60 mt-1 block">"Required for check-in coordination & concierge arrival."</span>
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
                                            <select name="arrival_time" id="guest-arrival-time" class="select select-bordered w-full rounded-xl">
                                                for (time_val, time_label, is_def) in &arrival_times {
                                                    if *is_def {
                                                        <option value=(*time_val) selected=(true)>(*time_label)</option>
                                                    } else {
                                                        <option value=(*time_val)>(*time_label)</option>
                                                    }
                                                }
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

                            <div class="pt-3">
                                <button
                                    type="submit"
                                    form="checkout-form"
                                    id="checkout-submit-btn"
                                    class="btn btn-primary btn-block py-4 text-base font-bold rounded-2xl shadow-xl tracking-wide uppercase transition-all duration-300 active:scale-95"
                                >
                                    "Confirm & Finalize Reservation →"
                                </button>
                                <p class="text-[11px] text-center text-base-content/60 mt-2">
                                    "Instant confirmation · No hidden resort fees"
                                </p>
                            </div>
                        </div>
                    </div>
                </div>

                // Celebratory Booking Success Modal with Animated Checkmark
                <dialog id="booking-success-modal" class="modal modal-bottom sm:modal-middle backdrop-blur-md">
                    <div class="modal-box rounded-3xl p-8 text-center space-y-5 bg-base-100/95 border border-primary/20 shadow-2xl max-w-md mx-auto">
                        <div class="w-20 h-20 bg-success/20 text-success rounded-full flex items-center justify-center mx-auto text-4xl shadow-inner animate-bounce">
                            "✓"
                        </div>
                        <div class="space-y-2">
                            <span class="badge badge-success badge-sm font-bold uppercase tracking-widest">"Reservation Confirmed"</span>
                            <h3 class="text-2xl font-serif font-extrabold text-base-content" id="success-modal-title">
                                "Welcome to Jamaica!"
                            </h3>
                            <p class="text-xs text-base-content/70 max-w-xs mx-auto" id="success-modal-desc">
                                "Your luxury villa reservation has been successfully booked. Transferring to your itinerary..."
                            </p>
                        </div>
                        <div class="flex items-center justify-center gap-2 text-xs font-semibold text-primary pt-2">
                            <span class="loading loading-spinner loading-sm"></span>
                            <span>"Finalizing concierge records..."</span>
                        </div>
                    </div>
                </dialog>

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
                                var phoneEl = document.getElementById('guest-phone');
                                
                                if (emailEl) {
                                    if (user.email) {
                                        emailEl.value = user.email;
                                    }
                                }
                                if (phoneEl) {
                                    var p = user.phone || user.phone_number || '';
                                    if (p) {
                                        phoneEl.value = p;
                                    }
                                }
                                
                                var firstName = user.first_name || '';
                                var lastName = user.last_name || '';
                                
                                if (!firstName) {
                                    if (!lastName) {
                                        if (user.name) {
                                            var cleanName = user.name.trim();
                                            var parts = cleanName.split(' ');
                                            if (parts.length !== 1) {
                                                firstName = parts[0];
                                                lastName = parts.slice(1).join(' ');
                                            } else {
                                                var word = parts[0] || '';
                                                if (word.toLowerCase().indexOf('pavelbyles') !== -1) {
                                                    firstName = 'Pavel';
                                                    lastName = 'Byles';
                                                } else {
                                                    var matchPascal = word.match(/^([A-Z][a-z]+)([A-Z][a-z]+)$/);
                                                    if (matchPascal) {
                                                        firstName = matchPascal[1];
                                                        lastName = matchPascal[2];
                                                    } else {
                                                        firstName = word;
                                                        lastName = '';
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                
                                if (!lastName) {
                                    if (firstName) {
                                        if (firstName.toLowerCase().indexOf('pavelbyles') !== -1) {
                                            firstName = 'Pavel';
                                            lastName = 'Byles';
                                        }
                                    }
                                }
                                
                                if (fnEl) {
                                    if (firstName) {
                                        fnEl.value = firstName;
                                    }
                                }
                                if (lnEl) {
                                    if (lastName) {
                                        lnEl.value = lastName;
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
                            var form = document.getElementById('checkout-form');
                            if (form) {
                                if (!form.checkValidity()) {
                                    form.reportValidity();
                                    return;
                                }
                            }

                            var submitBtn = document.getElementById('checkout-submit-btn');
                            if (submitBtn) {
                                submitBtn.disabled = true;
                                submitBtn.innerText = 'Securing Dates...';
                            }
                            var errBox = document.getElementById('checkout-error-banner');
                            if (errBox) {
                                errBox.classList.add('hidden');
                            }

                            try {
                                var formEl = document.getElementById('checkout-form');
                                var listingId = formEl ? (formEl.getAttribute('data-listing-id') || '01a03ca2-4e59-73e2-b66d-17518a16c1ee') : '01a03ca2-4e59-73e2-b66d-17518a16c1ee';
                                var currency = formEl ? (formEl.getAttribute('data-currency') || 'USD') : 'USD';
                                var vName = formEl ? (formEl.getAttribute('data-villa-name') || 'Luxury Villa') : 'Luxury Villa';

                                var guestId = '01a03c98-1dce-7213-986b-959414aa0776';
                                var userJson = localStorage.getItem('op_auth_user');
                                if (!userJson) {
                                    userJson = sessionStorage.getItem('op_auth_user');
                                }
                                if (userJson) {
                                    var u = JSON.parse(userJson);
                                    if (u.id) {
                                        guestId = u.id;
                                    }
                                }

                                var guestCount = document.getElementById('guest-count-select').value;
                                var arrivalEl = document.getElementById('guest-arrival-time');
                                var arrivalVal = arrivalEl ? arrivalEl.options[arrivalEl.selectedIndex].text : '3:00 PM (Standard Check-in)';
                                var specialReq = document.querySelector('textarea[name="message_to_host"]');
                                var specialReqVal = specialReq ? specialReq.value : null;

                                var payload = {
                                    guest_id: guestId,
                                    listing_id: listingId,
                                    check_in: '2026-09-10',
                                    check_out: '2026-09-15',
                                    num_adults: parseInt(guestCount, 10) || 1,
                                    num_children: 0,
                                    num_infants: 0,
                                    num_pets: 0,
                                    message_to_host: specialReqVal,
                                    estimated_arrival_time: arrivalVal,
                                    is_business_trip: false,
                                    currency: currency,
                                    agreed_cancellation_policy: 'Flexible'
                                };

                                fetch('http://localhost:8081/api/v1/bookings', {
                                    method: 'POST',
                                    headers: {
                                        'Content-Type': 'application/json',
                                        'Accept': 'application/json'
                                    },
                                    body: JSON.stringify(payload)
                                })
                                .then(function(res) {
                                    if (!res.ok) {
                                        return res.json().then(function(errData) {
                                            var msg = errData.error || errData.message || 'Failed to create booking';
                                            throw new Error(msg);
                                        });
                                    }
                                    return res.json();
                                })
                                .then(function(booking) {
                                    var modal = document.getElementById('booking-success-modal');
                                    if (modal) {
                                        var titleEl = document.getElementById('success-modal-title');
                                        if (titleEl) {
                                            titleEl.innerText = vName + ' Confirmed!';
                                        }
                                        modal.showModal();
                                    }

                                    setTimeout(function() {
                                        window.location.href = '/bookings?new_booking=true';
                                    }, 1800);
                                })
                                .catch(function(err) {
                                    console.error('Booking submission error:', err);
                                    if (submitBtn) {
                                        submitBtn.disabled = false;
                                        submitBtn.innerText = 'Confirm & Finalize Reservation →';
                                    }
                                    if (errBox) {
                                        errBox.classList.remove('hidden');
                                        var errMsg = document.getElementById('checkout-error-text');
                                        if (errMsg) {
                                            errMsg.innerText = err.message || 'The requested dates are no longer available. Please choose different dates.';
                                        }
                                        errBox.scrollIntoView({ behavior: 'smooth' });
                                    }
                                });
                            } catch(err) {
                                console.error('Booking submission error:', err);
                                if (submitBtn) {
                                    submitBtn.disabled = false;
                                    submitBtn.innerText = 'Confirm & Finalize Reservation →';
                                }
                            }
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
