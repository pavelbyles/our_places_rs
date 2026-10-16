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
        .unwrap_or_else(|| "Luxury Jamaican Villa".to_string());
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

    let jamaica_tomorrow =
        (Utc::now() - chrono::Duration::hours(5) + chrono::Duration::days(1)).date_naive();
    let min_checkin = jamaica_tomorrow.to_string();
    let jamaica_day_after =
        (Utc::now() - chrono::Duration::hours(5) + chrono::Duration::days(2)).date_naive();
    let min_checkout = jamaica_day_after.to_string();

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
                    <h1 class="text-2xl font-black text-base-content">"Modify Reservation Dates"</h1>
                    <p class="text-xs text-base-content/70 mt-1">
                        (format!("Update check-in and check-out dates for {}", villa_name))
                    </p>
                </div>

                if !is_confirmed {
                    <div class="alert alert-warning rounded-2xl text-sm">
                        <svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-5 w-5" fill="none" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                        </svg>
                        <span>"Only confirmed reservations are eligible for date modifications. Your current status is: " <strong class="font-bold">(&booking.status)</strong></span>
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
                            <span class="font-bold text-base-content text-sm">(format!("{} — {}", current_start, current_end))</span>
                            <span class="text-base-content/70 block mt-0.5">(format!("Current Total: {}", current_total))</span>
                        </div>
                    </div>

                    <div class="text-xs text-base-content/70 bg-info/10 border border-info/20 text-info-content p-4 rounded-2xl space-y-1">
                        <div class="font-bold flex items-center gap-1.5 text-base-content">
                            <span>"ℹ️"</span>
                            <span>"Statutory Tax Invoicing & Receipt Adjustments"</span>
                        </div>
                        <p class="leading-relaxed">
                            "When dates are modified, the accommodation price is recalculated based on seasonal nightly rates. If the new total differs from your initial payment, your active payment receipt will be superseded and a revised statutory receipt (e.g. REC-YYYY-XXXX-R2) with balance details will be issued."
                        </p>
                    </div>

                    <form id="date-change-form" class="space-y-4 pt-2" onsubmit="return false;">
                        <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                            <div class="form-control">
                                <label class="label pb-1">
                                    <span class="label-text text-xs font-bold uppercase tracking-wider text-base-content/70">"New Check-in Date"</span>
                                </label>
                                <input
                                    type="date"
                                    id="new-date-from"
                                    min=(min_checkin)
                                    value=(current_start.clone())
                                    class="input input-bordered w-full rounded-xl font-medium text-sm"
                                    required=(true)
                                />
                            </div>
                            <div class="form-control">
                                <label class="label pb-1">
                                    <span class="label-text text-xs font-bold uppercase tracking-wider text-base-content/70">"New Check-out Date"</span>
                                </label>
                                <input
                                    type="date"
                                    id="new-date-to"
                                    min=(min_checkout)
                                    value=(current_end.clone())
                                    class="input input-bordered w-full rounded-xl font-medium text-sm"
                                    required=(true)
                                />
                            </div>
                        </div>

                        <div id="date-change-error" class="hidden alert alert-error rounded-2xl text-xs font-semibold">
                        </div>

                        <div id="date-change-success" class="hidden alert alert-success rounded-2xl text-xs font-semibold">
                        </div>

                        <div class="flex items-center justify-end gap-3 pt-4 border-t border-base-200">
                            <a href="/bookings" class="btn btn-ghost btn-sm font-semibold">"Cancel"</a>
                            <button
                                type="button"
                                id="btn-submit-change"
                                class="btn btn-primary btn-sm font-bold px-6 shadow-sm"
                                onclick="submitDateChange()"
                            >
                                "Confirm New Dates"
                            </button>
                        </div>
                    </form>

                    <script>
                        (format!(r#"
                        var bookingId = "{}";
                        function submitDateChange() {{
                            var errorEl = document.getElementById('date-change-error');
                            var successEl = document.getElementById('date-change-success');
                            var btn = document.getElementById('btn-submit-change');
                            var dateFrom = document.getElementById('new-date-from').value;
                            var dateTo = document.getElementById('new-date-to').value;

                            if (!dateFrom || !dateTo) {{
                                if (errorEl) {{
                                    errorEl.innerText = "Please select both check-in and check-out dates.";
                                    errorEl.classList.remove('hidden');
                                }}
                                return;
                            }}

                            if (dateFrom >= dateTo) {{
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
                                btn.innerHTML = '<span class="loading loading-spinner loading-xs"></span> Updating Reservation...';
                            }}

                            fetch('/api/bookings/' + bookingId, {{
                                method: 'PATCH',
                                headers: {{
                                    'Content-Type': 'application/json',
                                    'Accept': 'application/json'
                                }},
                                body: JSON.stringify({{
                                    date_from: dateFrom,
                                    date_to: dateTo
                                }})
                            }})
                            .then(function(res) {{
                                return res.json().then(function(data) {{
                                    return {{ ok: res.ok, status: res.status, data: data }};
                                }});
                            }})
                            .then(function(result) {{
                                if (btn) {{
                                    btn.disabled = false;
                                    btn.innerText = "Confirm New Dates";
                                }}

                                if (!result.ok) {{
                                    var msg = "Failed to update reservation dates.";
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
                                        successEl.innerHTML = '<span>Reservation dates updated successfully! Revised payment receipt issued. Redirecting...</span>';
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
                                    btn.innerText = "Confirm New Dates";
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
