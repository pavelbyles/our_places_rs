use chrono::Utc;
use topcoat::{
    Result,
    context::Cx,
    router::{error::RouterErrorExt, page, path_param},
    view::{View, view},
};
use uuid::Uuid;
use web_app_common_tc::client::ListingSearchParams;
use web_app_common_tc::get_api_client;

path_param!(id);

#[page("/bookings/{id}/change-dates")]
pub async fn booking_date_change_page(cx: &Cx) -> Result<impl View> {
    render_booking_date_change_page(cx).await
}

pub async fn render_booking_date_change_page(cx: &Cx) -> Result<impl View> {
    let __cx = cx;
    let _user = web_app_common_tc::auth::get_guest_session(cx)
        .await
        .ok_or_redirect("/login?redirect=%2Fbookings")?;

    let id_str: &str = path_param::<Id>(cx);
    let booking_id = Uuid::parse_str(id_str).map_err(|_| topcoat::router::error::not_found())?;

    let api = get_api_client(cx);
    let bookings = api
        .get_all_bookings(Some(1), Some(100))
        .await
        .unwrap_or_default();

    let booking = bookings
        .into_iter()
        .find(|b| b.id == booking_id)
        .ok_or_not_found()?;

    let listings = api
        .search_listings(ListingSearchParams {
            per_page: Some(50),
            ..Default::default()
        })
        .await
        .unwrap_or_default();
    let listing = listings.into_iter().find(|l| l.id == booking.listing_id);

    let villa_name = listing
        .as_ref()
        .map(|l| l.name.clone())
        .unwrap_or_else(|| "Luxury Jamaican Listing".to_string());
    let location = listing
        .as_ref()
        .map(|l| {
            if let Some(city) = &l.city {
                format!("{}, {}", city, l.country)
            } else {
                l.country.clone()
            }
        })
        .unwrap_or_else(|| "Jamaica".to_string());

    let is_confirmed = booking.status.eq_ignore_ascii_case("confirmed");
    let current_start = booking.date_from.to_string();
    let current_end = booking.date_to.to_string();
    let current_total = format!("{} {}", booking.currency, booking.total_price);
    let confirmation_code = booking.confirmation_code.clone();

    let max_guests = listing.as_ref().map(|l| l.max_guests).unwrap_or(10);
    let guests_included = listing
        .as_ref()
        .map(|l| l.guests_included.max(1))
        .unwrap_or(1);
    let extra_guest_fee = listing
        .as_ref()
        .and_then(|l| l.extra_guest_fee)
        .unwrap_or(rust_decimal::Decimal::ZERO);

    let current_adults = if booking.metadata.num_adults > 0 {
        booking.metadata.num_adults as i32
    } else {
        booking.number_of_persons.max(1)
    };
    let current_children = booking.metadata.num_children as i32;
    let current_infants = booking.metadata.num_infants as i32;
    let current_pets = booking.metadata.num_pets as i32;

    let jamaica_today = (Utc::now() - chrono::Duration::hours(5)).date_naive();
    let is_mid_stay = jamaica_today >= booking.date_from && jamaica_today < booking.date_to;
    let jamaica_tomorrow = jamaica_today + chrono::Duration::days(1);
    let min_checkin = if is_mid_stay {
        current_start.clone()
    } else {
        jamaica_tomorrow.to_string()
    };
    let jamaica_day_after = jamaica_today + chrono::Duration::days(2);
    let min_checkout = if is_mid_stay {
        jamaica_tomorrow.to_string()
    } else {
        jamaica_day_after.to_string()
    };

    Ok(view! {
        <div class="max-w-3xl mx-auto px-4 py-8 space-y-6">
            <div class="flex items-center justify-between">
                <a href="/bookings" class="btn btn-ghost btn-sm gap-2 font-medium">
                    "← Back to Bookings"
                </a>
                <span class="badge badge-neutral text-xs font-mono">
                    (format!("Reservation #{}", confirmation_code))
                </span>
            </div>

            <div class="card bg-base-100 border border-base-300 shadow-xl rounded-3xl p-6 space-y-6">
                <div class="border-b border-base-200 pb-4">
                    <h1 class="text-2xl font-black text-base-content">"Modify Reservation (Dates & Guests)"</h1>
                    <p class="text-xs text-base-content/70 mt-1">
                        (format!("Propose updated dates or guest counts for {} (subject to host approval)", villa_name))
                    </p>
                </div>

                if !is_confirmed {
                    <div class="alert alert-warning rounded-2xl text-sm">
                        <svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-5 w-5" fill="none" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                        </svg>
                        <span>"Only confirmed reservations are eligible for modifications. Your current status is: " <strong class="font-bold">(&booking.status)</strong></span>
                    </div>
                } else {
                    <div class="grid grid-cols-1 md:grid-cols-2 gap-4 bg-base-200/50 p-4 rounded-2xl text-xs">
                        <div>
                            <span class="text-base-content/60 font-semibold block">"Property"</span>
                            <span class="font-bold text-base-content text-sm">(villa_name)</span>
                            <span class="text-base-content/70 block mt-0.5">(location)</span>
                        </div>
                        <div>
                            <span class="text-base-content/60 font-semibold block">"Current Confirmed Stay"</span>
                            <span class="font-bold text-base-content text-sm">(format!("{} — {} ({} guests)", current_start, current_end, booking.number_of_persons))</span>
                            <span class="text-base-content/70 block mt-0.5">(format!("Current Total: {}", current_total))</span>
                        </div>
                    </div>

                    if is_mid_stay {
                        <div class="alert alert-info rounded-2xl text-xs" id="mid-stay-notice">
                            <span>"🏖️ Your stay is currently in progress. Check-in date is locked, but you may request a check-out date extension/shortening or update guest counts."</span>
                        </div>
                    }

                    <div class="text-xs text-base-content/70 bg-info/10 border border-info/20 text-info-content p-4 rounded-2xl space-y-1">
                        <div class="font-bold flex items-center gap-1.5 text-base-content">
                            <span>"ℹ️"</span>
                            <span>"Bilateral Approval & Statutory Tax Invoicing"</span>
                        </div>
                        <p class="leading-relaxed">
                            "Reservation changes require counterparty approval. Once approved by the host, seasonal nightly rates and extra-guest fees are recalculated and a revised statutory receipt (e.g. REC-YYYY-XXXX-R2) with any balance due or refund is issued."
                        </p>
                        if extra_guest_fee > rust_decimal::Decimal::ZERO {
                            <p class="font-semibold text-primary pt-1">
                                (format!("Base rate includes up to {} guest{}. Additional guests: {} {:.2}/night (Max {} guests).", guests_included, if guests_included == 1 { "" } else { "s" }, booking.currency, extra_guest_fee, max_guests))
                            </p>
                        }
                    </div>

                    <form id="date-change-form" class="space-y-4 pt-2" onsubmit="return false;">
                        <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                            <div class="form-control">
                                <label class="label pb-1">
                                    <span class="label-text text-xs font-bold uppercase tracking-wider text-base-content/70">"New Check-in Date"</span>
                                </label>
                                <input
                                    type="date"
                                    id="new-check-in"
                                    min=(min_checkin)
                                    value=(current_start.clone())
                                    readonly=(is_mid_stay)
                                    class="input input-bordered w-full rounded-xl font-medium text-sm"
                                    onchange="previewChangeQuote()"
                                    required=(true)
                                />
                            </div>
                            <div class="form-control">
                                <label class="label pb-1">
                                    <span class="label-text text-xs font-bold uppercase tracking-wider text-base-content/70">"New Check-out Date"</span>
                                </label>
                                <input
                                    type="date"
                                    id="new-check-out"
                                    min=(min_checkout)
                                    value=(current_end.clone())
                                    class="input input-bordered w-full rounded-xl font-medium text-sm"
                                    onchange="previewChangeQuote()"
                                    required=(true)
                                />
                            </div>
                        </div>

                        <div class="grid grid-cols-2 sm:grid-cols-4 gap-3">
                            <div class="form-control">
                                <label class="label pb-1">
                                    <span class="label-text text-xs font-bold uppercase tracking-wider text-base-content/70">"Adults (13+)"</span>
                                </label>
                                <input
                                    type="number"
                                    id="new-adults"
                                    min="1"
                                    max=(max_guests.to_string())
                                    value=(current_adults.to_string())
                                    onchange="previewChangeQuote()"
                                    class="input input-bordered w-full rounded-xl font-bold text-sm"
                                />
                            </div>
                            <div class="form-control">
                                <label class="label pb-1">
                                    <span class="label-text text-xs font-bold uppercase tracking-wider text-base-content/70">"Children (2–12)"</span>
                                </label>
                                <input
                                    type="number"
                                    id="new-children"
                                    min="0"
                                    max=(max_guests.to_string())
                                    value=(current_children.to_string())
                                    onchange="previewChangeQuote()"
                                    class="input input-bordered w-full rounded-xl font-bold text-sm"
                                />
                            </div>
                            <div class="form-control">
                                <label class="label pb-1">
                                    <span class="label-text text-xs font-bold uppercase tracking-wider text-base-content/70">"Infants (<2)"</span>
                                </label>
                                <input
                                    type="number"
                                    id="new-infants"
                                    min="0"
                                    max="10"
                                    value=(current_infants.to_string())
                                    onchange="previewChangeQuote()"
                                    class="input input-bordered w-full rounded-xl font-bold text-sm"
                                />
                            </div>
                            <div class="form-control">
                                <label class="label pb-1">
                                    <span class="label-text text-xs font-bold uppercase tracking-wider text-base-content/70">"Pets"</span>
                                </label>
                                <input
                                    type="number"
                                    id="new-pets"
                                    min="0"
                                    max="5"
                                    value=(current_pets.to_string())
                                    onchange="previewChangeQuote()"
                                    class="input input-bordered w-full rounded-xl font-bold text-sm"
                                />
                            </div>
                        </div>

                        <div class="form-control">
                            <label class="label pb-1">
                                <span class="label-text text-xs font-bold uppercase tracking-wider text-base-content/70">"Note to Host (Optional)"</span>
                            </label>
                            <input
                                type="text"
                                id="change-reason"
                                placeholder="e.g. Adding 2 family members and extending our stay by 1 night"
                                class="input input-bordered w-full rounded-xl text-sm"
                            />
                        </div>

                        <div id="change-quote-preview" class="hidden bg-base-200/70 border border-base-300 rounded-2xl p-4 text-xs space-y-1.5">
                        </div>

                        <div id="date-change-error" class="hidden alert alert-error rounded-2xl text-xs font-semibold">
                        </div>

                        <div id="date-change-success" class="hidden alert alert-success rounded-2xl text-xs font-semibold">
                        </div>

                        <div class="flex flex-wrap items-center justify-between gap-3 pt-4 border-t border-base-200">
                            <button
                                type="button"
                                id="btn-preview-change-quote"
                                class="btn btn-outline btn-sm font-semibold"
                                onclick="previewChangeQuote()"
                            >
                                "Preview Price Impact"
                            </button>
                            <div class="flex items-center gap-3">
                                <a href="/bookings" class="btn btn-ghost btn-sm font-semibold">"Cancel"</a>
                                <button
                                    type="button"
                                    id="btn-submit-date-change"
                                    class="btn btn-primary btn-sm font-bold px-6 shadow-sm"
                                    onclick="submitDateChange()"
                                >
                                    "Request Reservation Change"
                                </button>
                            </div>
                        </div>
                    </form>

                    <script>
                        (format!(r#"
                        var bookingId = "{}";
                        function gatherChangePayload() {{
                            var dateFrom = document.getElementById('new-check-in').value;
                            var dateTo = document.getElementById('new-check-out').value;
                            var adults = parseInt(document.getElementById('new-adults').value || '1', 10);
                            var children = parseInt(document.getElementById('new-children').value || '0', 10);
                            var infants = parseInt(document.getElementById('new-infants').value || '0', 10);
                            var pets = parseInt(document.getElementById('new-pets').value || '0', 10);
                            var reasonEl = document.getElementById('change-reason');
                            var reason = reasonEl && reasonEl.value.trim() ? reasonEl.value.trim() : null;
                            return {{
                                proposed_date_from: dateFrom,
                                proposed_date_to: dateTo,
                                proposed_adults: isNaN(adults) ? 1 : adults,
                                proposed_children: isNaN(children) ? 0 : children,
                                proposed_infants: isNaN(infants) ? 0 : infants,
                                proposed_pets: isNaN(pets) ? 0 : pets,
                                reason: reason,
                                force_apply: false
                            }};
                        }}

                        function previewChangeQuote() {{
                            var errorEl = document.getElementById('date-change-error');
                            var quoteEl = document.getElementById('change-quote-preview');
                            var payload = gatherChangePayload();
                            if (!payload.proposed_date_from || !payload.proposed_date_to || payload.proposed_date_from >= payload.proposed_date_to) {{
                                return;
                            }}
                            if (errorEl) errorEl.classList.add('hidden');
                            if (quoteEl) {{
                                quoteEl.classList.remove('hidden');
                                quoteEl.innerHTML = '<span class="loading loading-spinner loading-xs mr-2"></span> Calculating live price delta...';
                            }}
                            fetch('/api/bookings/' + bookingId + '/change-quote', {{
                                method: 'POST',
                                headers: {{
                                    'Content-Type': 'application/json',
                                    'Accept': 'application/json'
                                }},
                                body: JSON.stringify(payload)
                            }})
                            .then(function(res) {{
                                return res.json().then(function(data) {{
                                    return {{ ok: res.ok, data: data }};
                                }});
                            }})
                            .then(function(result) {{
                                if (!result.ok) {{
                                    if (quoteEl) quoteEl.classList.add('hidden');
                                    if (errorEl) {{
                                        errorEl.innerText = (result.data && result.data.message) ? result.data.message : 'Unable to quote requested modification.';
                                        errorEl.classList.remove('hidden');
                                    }}
                                    return;
                                }}
                                var q = result.data;
                                var bal = parseFloat(q.balance_due || '0');
                                var balBadge = bal > 0
                                    ? '<span class="badge badge-warning badge-sm font-bold">Extra Charge: ' + q.currency + ' ' + q.balance_due + '</span>'
                                    : (bal < 0
                                        ? '<span class="badge badge-success badge-sm font-bold">Refund: ' + q.currency + ' ' + Math.abs(bal).toFixed(2) + '</span>'
                                        : '<span class="badge badge-neutral badge-sm font-bold">No Price Difference</span>');
                                if (quoteEl) {{
                                    quoteEl.innerHTML =
                                        '<div class="flex justify-between items-center font-bold text-base-content border-b border-base-300 pb-1">' +
                                        '<span>Live Modification Quote (' + q.proposed_total_days + ' nights, ' + q.proposed_number_of_persons + ' guests)</span>' +
                                        balBadge +
                                        '</div>' +
                                        '<div class="flex justify-between pt-1"><span>Previous Total:</span><span class="font-mono">' + q.currency + ' ' + q.previous_total_price + '</span></div>' +
                                        '<div class="flex justify-between"><span>Extra-Guest Surcharge:</span><span class="font-mono">' + q.currency + ' ' + q.extra_guest_surcharge + '</span></div>' +
                                        '<div class="flex justify-between font-bold text-base-content"><span>Proposed New Total (incl. 15% GCT):</span><span class="font-mono">' + q.currency + ' ' + q.proposed_total_price + '</span></div>';
                                }}
                            }})
                            .catch(function() {{
                                if (quoteEl) quoteEl.classList.add('hidden');
                            }});
                        }}

                        function submitDateChange() {{
                            var errorEl = document.getElementById('date-change-error');
                            var successEl = document.getElementById('date-change-success');
                            var btn = document.getElementById('btn-submit-date-change');
                            var payload = gatherChangePayload();

                            if (!payload.proposed_date_from || !payload.proposed_date_to) {{
                                if (errorEl) {{
                                    errorEl.innerText = "Please select both check-in and check-out dates.";
                                    errorEl.classList.remove('hidden');
                                }}
                                return;
                            }}

                            if (payload.proposed_date_from >= payload.proposed_date_to) {{
                                if (errorEl) {{
                                    errorEl.innerText = "Check-out date must be after check-in date.";
                                    errorEl.classList.remove('hidden');
                                }}
                                return;
                            }}

                            if (errorEl) errorEl.classList.add('hidden');
                            if (successEl) successEl.classList.add('hidden');
                            if (btn) {{
                                btn.disabled = true;
                                btn.innerHTML = '<span class="loading loading-spinner loading-xs"></span> Submitting Change Request...';
                            }}

                            fetch('/api/bookings/' + bookingId + '/change-requests', {{
                                method: 'POST',
                                headers: {{
                                    'Content-Type': 'application/json',
                                    'Accept': 'application/json'
                                }},
                                body: JSON.stringify(payload)
                            }})
                            .then(function(res) {{
                                return res.json().then(function(data) {{
                                    return {{ ok: res.ok, status: res.status, data: data }};
                                }});
                            }})
                            .then(function(result) {{
                                if (btn) {{
                                    btn.disabled = false;
                                    btn.innerText = "Request Reservation Change";
                                }}

                                if (!result.ok) {{
                                    var msg = "Failed to submit reservation change request.";
                                    if (result.data) {{
                                        if (result.data.code === 'DATE_UNAVAILABLE' || (result.data.message && result.data.message.indexOf("DATE_UNAVAILABLE") !== -1)) {{
                                            msg = "The requested dates are no longer available. Please select alternative dates.";
                                        }} else if (result.data.code === 'BOOKING_NOT_MODIFIABLE' || (result.data.message && result.data.message.indexOf("BOOKING_NOT_MODIFIABLE") !== -1)) {{
                                            msg = result.data.message || "This reservation cannot be modified.";
                                        }} else if (result.data.message) {{
                                            msg = result.data.message;
                                        }}
                                    }}
                                    if (errorEl) {{
                                        errorEl.innerText = msg;
                                        errorEl.classList.remove('hidden');
                                    }}
                                }} else {{
                                    if (successEl) {{
                                        successEl.innerHTML = '<span>Reservation change request submitted for host approval! Redirecting to your bookings...</span>';
                                        successEl.classList.remove('hidden');
                                    }}
                                    setTimeout(function() {{
                                        window.location.href = '/bookings';
                                    }}, 1500);
                                }}
                            }})
                            .catch(function(err) {{
                                if (btn) {{
                                    btn.disabled = false;
                                    btn.innerText = "Request Reservation Change";
                                }}
                                if (errorEl) {{
                                    errorEl.innerText = "Network error: Unable to connect to server.";
                                    errorEl.classList.remove('hidden');
                                }}
                            }});
                        }}
                        "#, booking_id))
                    </script>
                }
            </div>
        </div>
    })
}
