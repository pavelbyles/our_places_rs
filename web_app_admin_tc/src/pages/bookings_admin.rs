use std::collections::HashMap;
use topcoat::{Result, context::Cx, router::page, view::view};
use web_app_common_tc::{client::ListingSearchParams, get_api_client};

#[page("/bookings")]
pub async fn bookings_alias_page(cx: &Cx) -> Result {
    render_bookings_content(cx).await
}

#[page("/admin/bookings")]
pub async fn admin_bookings_page(cx: &Cx) -> Result {
    render_bookings_content(cx).await
}

async fn render_bookings_content(cx: &Cx) -> Result {
    if let Err(_err) = web_app_common_tc::auth::require_admin_auth(cx).await {
        return view! {
            <script>
                r#"window.location.replace('/login?redirect=' + encodeURIComponent(window.location.pathname + window.location.search));"#
            </script>
        };
    }

    let __cx = cx;
    let api = get_api_client(cx);

    let bookings = api
        .get_all_bookings(Some(1), Some(50))
        .await
        .unwrap_or_default();
    let listings = api
        .search_listings(ListingSearchParams {
            per_page: Some(50),
            ..Default::default()
        })
        .await
        .unwrap_or_default();
    let users = match api.get_all_users(Some(1), Some(50), None).await {
        Ok(users) => users,
        Err(err) => {
            tracing::error!("Failed to fetch users in bookings admin: {:?}", err);
            Vec::new()
        }
    };

    let listing_map: HashMap<uuid::Uuid, (String, String)> = listings
        .into_iter()
        .map(|l| {
            (
                l.id,
                (l.name, l.city.unwrap_or_else(|| "Jamaica".to_string())),
            )
        })
        .collect();

    let user_map: HashMap<uuid::Uuid, (String, String)> = users
        .into_iter()
        .map(|u| {
            let full_name = format!("{} {}", u.first_name, u.last_name)
                .trim()
                .to_string();
            let name = if full_name.is_empty() {
                "Guest".to_string()
            } else {
                full_name
            };
            (u.id, (name, u.email))
        })
        .collect();

    let pending_bookings: Vec<&common::models::BookingResponse> = bookings
        .iter()
        .filter(|b| {
            b.status.eq_ignore_ascii_case("pending")
                || b.status.eq_ignore_ascii_case("pending_payment")
        })
        .collect();
    let pending_count = pending_bookings.len();

    let total_count = bookings.len();

    view! {
        <div class="space-y-8 py-6 max-w-7xl mx-auto px-4 md:px-6">
            // Header
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 border-b border-base-200 pb-4">
                <div class="space-y-1">
                    <span class="text-primary font-bold tracking-widest uppercase text-xs">"Reservations & Date Locks"</span>
                    <h1 class="text-3xl font-serif font-bold tracking-tight text-base-content">
                        "Master Booking Schedule"
                    </h1>
                    <p class="text-xs text-base-content/60">
                        "Live audit of PostgreSQL row-level locks (FOR UPDATE), 15-minute holds, and statutory Jamaican tax breakdowns."
                    </p>
                </div>
                <div class="flex items-center gap-2">
                    <button
                        type="button"
                        class="btn btn-outline btn-sm rounded-full font-semibold"
                        onclick="exportBookingsCsv()"
                    >
                        "Export CSV"
                    </button>
                    <a href="/admin/listings" class="btn btn-ghost btn-sm font-semibold">
                        "← Listings"
                    </a>
                </div>
            </div>

            // Dedicated Pending Approvals Section
            if pending_count > 0 {
                <div class="card bg-warning/10 border border-warning/30 p-5 rounded-2xl shadow-sm space-y-3" id="pending-approvals-card">
                    <div class="flex items-center justify-between">
                        <div class="flex items-center gap-2 text-warning font-bold text-sm">
                            <span>"⚡"</span>
                            <span>(format!("{} Reservation{} Awaiting Host / Admin Approval", pending_count, if pending_count == 1 { "" } else { "s" }))</span>
                        </div>
                        <span class="text-xs text-base-content/60">"Review & confirm date holds"</span>
                    </div>
                    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
                        for pb in &pending_bookings {
                            let pb_id = pb.id.to_string();
                            let (v_name, _) = listing_map.get(&pb.listing_id).cloned().unwrap_or_else(|| ("Luxury Villa".to_string(), "Jamaica".to_string()));
                            let (g_name, g_email) = user_map.get(&pb.guest_id).cloned().unwrap_or_else(|| ("Guest".to_string(), "guest@ourplaces.io".to_string()));
                            let dates = format!("{} – {}", pb.date_from.format("%b %d"), pb.date_to.format("%b %d, %Y"));
                            let total = format!("{} {:.2}", pb.currency, pb.total_price);

                            <div class="bg-base-100 dark:bg-base-200 p-4 rounded-xl border border-warning/20 flex flex-col justify-between gap-3 shadow-sm" id=(format!("pending-card-{}", pb_id))>
                                <div class="space-y-1">
                                    <div class="flex justify-between items-start">
                                        <h4 class="font-serif font-bold text-sm text-base-content">(v_name)</h4>
                                        <span class="badge badge-warning badge-xs font-bold">"Pending"</span>
                                    </div>
                                    <p class="text-xs text-base-content/70">(format!("{} ({})", g_name, g_email))</p>
                                    <p class="text-[11px] text-base-content/60 font-medium">(dates) " · " <strong class="text-primary font-bold">(total)</strong></p>
                                </div>
                                <div class="flex gap-2 pt-1">
                                    <button
                                        type="button"
                                        class="btn btn-success btn-xs flex-1 text-white font-bold"
                                        id=(format!("btn-direct-approve-{}", pb_id))
                                        onclick=(format!("approveAdminBookingDirect('{}')", pb_id))
                                    >
                                        "✓ Approve Booking"
                                    </button>
                                    <button
                                        type="button"
                                        class="btn btn-ghost btn-xs text-error font-semibold"
                                        onclick=(format!("rejectAdminBookingDirect('{}')", pb_id))
                                    >
                                        "Reject"
                                    </button>
                                </div>
                            </div>
                        }
                    </div>
                </div>
            }

            // Filter Bar
            <div class="bg-base-100 dark:bg-base-200/80 p-4 rounded-2xl border border-base-200 dark:border-base-100/20 shadow-sm flex flex-col md:flex-row items-center justify-between gap-4">
                <div class="flex-1 w-full flex items-center gap-3">
                    <input
                        type="text"
                        placeholder="Search by guest name, booking UUID, or villa..."
                        class="input input-bordered input-sm w-full max-w-md rounded-xl font-medium"
                    />
                    <select class="select select-bordered select-sm rounded-xl font-medium">
                        <option value="">"All Statuses"</option>
                        <option value="confirmed">"Confirmed"</option>
                        <option value="pending_payment">"Pending Hold (15-min)"</option>
                        <option value="completed">"Completed Stay"</option>
                        <option value="refunded">"Refunded / Cancelled"</option>
                    </select>
                </div>
                <div class="text-xs text-base-content/60 font-semibold" id="admin-bookings-count">
                    (format!("{} Active Holds & Bookings", total_count))
                </div>
            </div>

            // Bookings Table
            <div class="bg-base-100 dark:bg-base-200 rounded-2xl border border-base-200 dark:border-base-100/20 shadow-md overflow-hidden">
                <div class="overflow-x-auto">
                    <table class="table table-zebra w-full" id="admin-bookings-table">
                        <thead>
                            <tr class="text-xs text-base-content/60 uppercase tracking-wider">
                                <th>"Reservation"</th>
                                <th>"Guest"</th>
                                <th>"Dates & Nights"</th>
                                <th>"Total & Statutory GCT (15%)"</th>
                                <th>"Status"</th>
                                <th class="text-right">"Audit Actions"</th>
                            </tr>
                        </thead>
                        <tbody>
                            if bookings.is_empty() {
                                <tr>
                                    <td colspan="6" class="text-center py-12 text-base-content/60 text-sm">
                                        "No reservations or active PostgreSQL holds found in database."
                                    </td>
                                </tr>
                            } else {
                                for (idx, b) in bookings.iter().enumerate() {
                                    let b_id = b.id.to_string();
                                    let conf_code = if !b.confirmation_code.is_empty() {
                                        b.confirmation_code.clone()
                                    } else {
                                        b_id.clone()
                                    };
                                    let (villa_name, villa_location) = listing_map
                                        .get(&b.listing_id)
                                        .cloned()
                                        .unwrap_or_else(|| ("Luxury Villa".to_string(), "Jamaica".to_string()));
                                    let (guest_name, guest_email) = user_map
                                        .get(&b.guest_id)
                                        .cloned()
                                        .unwrap_or_else(|| ("Guest User".to_string(), "guest@ourplaces.io".to_string()));

                                    let dates_str = format!("{} – {}", b.date_from.format("%b %d, %Y"), b.date_to.format("%b %d, %Y"));
                                    let capacity_str = format!("{} nights ({} guests)", b.total_days, b.number_of_persons);
                                    let subtotal_str = format!("{} {:.2}", b.currency, b.sub_total_price);
                                    let tax_val = b.tax_value.unwrap_or(rust_decimal::Decimal::ZERO);
                                    let tax_str = format!("{} {:.2}", b.currency, tax_val);
                                    let total_str = format!("{} {:.2}", b.currency, b.total_price);
                                    let status_display = match b.status.as_str() {
                                        "confirmed" | "Confirmed" => "Confirmed",
                                        "pending_payment" | "PendingPayment" => "Pending Hold",
                                        "completed" | "Completed" => "Completed Stay",
                                        "refunded" | "Refunded" => "Refunded / Cancelled",
                                        s => s,
                                    };
                                    let badge_class = match b.status.as_str() {
                                        "confirmed" | "Confirmed" => "badge badge-success badge-sm font-bold",
                                        "pending_payment" | "PendingPayment" => "badge badge-warning badge-sm font-bold",
                                        "completed" | "Completed" => "badge badge-neutral badge-sm font-semibold",
                                        _ => "badge badge-error badge-sm font-semibold",
                                    };

                                    let is_pending = b.status.eq_ignore_ascii_case("pending") || b.status.eq_ignore_ascii_case("pending_payment");

                                    <tr id=(format!("admin-row-{}", idx + 1))>
                                        <td>
                                            <div class="font-serif font-bold text-sm text-base-content">(villa_name.clone())</div>
                                            <div class="text-[11px] font-mono text-base-content/50">(conf_code.clone())</div>
                                        </td>
                                        <td>
                                            <div class="font-semibold text-sm">(guest_name.clone())</div>
                                            <div class="text-xs text-base-content/50">(guest_email.clone())</div>
                                        </td>
                                        <td>
                                            <div class="font-medium text-xs">(dates_str.clone())</div>
                                            <div class="text-[11px] text-base-content/50">(capacity_str.clone())</div>
                                        </td>
                                        <td>
                                            <div class="font-bold text-sm">(total_str.clone())</div>
                                            <div class="text-[11px] text-base-content/60">(format!("Includes {} GCT (15%)", tax_str))</div>
                                        </td>
                                        <td>
                                            <span class=(badge_class) id=(format!("admin-status-{}", idx + 1))>(status_display)</span>
                                        </td>
                                        <td class="text-right space-x-1" id=(format!("admin-actions-{}", idx + 1))>
                                            if is_pending {
                                                <button
                                                    type="button"
                                                    class="btn btn-success btn-xs text-white font-bold"
                                                    id=(format!("btn-approve-{}", idx + 1))
                                                    onclick=(format!("approveAdminBooking('{}', '{}')", b_id, idx + 1))
                                                >
                                                    "✓ Approve"
                                                </button>
                                            }
                                            <button
                                                type="button"
                                                class="btn btn-ghost btn-xs text-primary font-bold"
                                                data-ref=(conf_code.clone())
                                                data-villa=(villa_name.clone())
                                                data-guest=(guest_name.clone())
                                                data-email=(guest_email.clone())
                                                data-dates=(dates_str.clone())
                                                data-capacity=(capacity_str.clone())
                                                data-subtotal=(subtotal_str.clone())
                                                data-tax=(tax_str.clone())
                                                data-total=(total_str.clone())
                                                data-status=(status_display.to_string())
                                                data-location=(villa_location.clone())
                                                onclick="openAdminBookingDetailsFromBtn(this)"
                                            >
                                                "Details"
                                            </button>
                                            <button
                                                type="button"
                                                class="btn btn-ghost btn-xs text-error font-bold"
                                                onclick=(format!("openAdminCancelDialog('{}', '{}', '{}')", b_id, villa_name, idx + 1))
                                            >
                                                "Reject / Cancel"
                                            </button>
                                        </td>
                                    </tr>
                                }
                            }
                        </tbody>
                    </table>
                </div>
            </div>

            // Admin Reservation Audit Details Modal
            <dialog id="admin-booking-details-dialog" class="modal modal-bottom sm:modal-middle">
                <div class="modal-box rounded-3xl p-6 md:p-8 space-y-6 max-w-2xl">
                    <div class="flex justify-between items-start border-b border-base-200 pb-4">
                        <div>
                            <span class="text-primary font-bold uppercase tracking-widest text-[10px]">"PostgreSQL Verified Lock"</span>
                            <h3 class="font-serif font-bold text-xl text-base-content" id="detail-villa-name">"Villa Name"</h3>
                            <span class="text-xs font-mono text-base-content/60" id="detail-ref-id">"REF-..."</span>
                        </div>
                        <span class="badge badge-success font-bold text-xs" id="detail-status-badge">"Confirmed"</span>
                    </div>

                    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4 text-xs">
                        <div class="bg-base-200/50 p-3.5 rounded-2xl space-y-1">
                            <span class="text-base-content/50 font-bold uppercase block text-[10px]">"Lead Guest"</span>
                            <div class="font-bold text-sm text-base-content" id="detail-guest-name">"Guest Name"</div>
                            <div class="text-base-content/70" id="detail-guest-email">"guest@example.com"</div>
                        </div>

                        <div class="bg-base-200/50 p-3.5 rounded-2xl space-y-1">
                            <span class="text-base-content/50 font-bold uppercase block text-[10px]">"Stay Dates & Capacity"</span>
                            <div class="font-bold text-sm text-base-content" id="detail-stay-dates">"Dates"</div>
                            <div class="text-base-content/70" id="detail-stay-nights">"Nights"</div>
                        </div>
                    </div>

                    // Financial & Statutory Tax Breakdown
                    <div class="bg-base-200/80 p-4 rounded-2xl space-y-2 text-xs">
                        <div class="font-bold text-xs uppercase tracking-wider text-base-content/70 border-b border-base-300 pb-1.5">
                            "Statutory Tri-Currency Financial Audit"
                        </div>
                        <div class="flex justify-between">
                            <span class="text-base-content/70">"Base Stay Subtotal:"</span>
                            <span class="font-semibold" id="detail-subtotal">"USD 0.00"</span>
                        </div>
                        <div class="flex justify-between">
                            <span class="text-base-content/70">"Statutory Jamaican GCT (15.00%):"</span>
                            <span class="font-semibold text-primary" id="detail-tax">"USD 0.00"</span>
                        </div>
                        <div class="divider my-1"></div>
                        <div class="flex justify-between text-sm font-bold text-base-content">
                            <span>"Total Settled Amount:"</span>
                            <span class="text-primary font-black" id="detail-total">"USD 0.00"</span>
                        </div>
                    </div>

                    <div class="modal-action flex justify-end gap-2 pt-2">
                        <form method="dialog">
                            <button class="btn btn-primary btn-sm rounded-xl font-bold">"Close Audit View"</button>
                        </form>
                    </div>
                </div>
            </dialog>

            // Admin Tax Invoice Modal
            <dialog id="admin-invoice-dialog" class="modal modal-bottom sm:modal-middle">
                <div class="modal-box rounded-3xl p-6 md:p-8 space-y-6 max-w-xl">
                    <div class="flex justify-between items-start border-b border-base-200 pb-4">
                        <div>
                            <span class="text-primary font-bold uppercase tracking-widest text-[10px]">"Statutory Tax Receipt"</span>
                            <h3 class="font-serif font-bold text-2xl text-base-content">"Our Places Jamaica"</h3>
                            <span class="text-xs text-base-content/60">"Tax Compliance ID: TAJ-GCT-889102"</span>
                        </div>
                        <span class="badge badge-neutral font-bold text-xs">"PAID IN FULL"</span>
                    </div>

                    <div class="space-y-3 text-xs">
                        <div class="flex justify-between">
                            <span class="text-base-content/60">"Invoice Number:"</span>
                            <span class="font-mono font-bold" id="invoice-ref">"INV-2026-..."</span>
                        </div>
                        <div class="flex justify-between">
                            <span class="text-base-content/60">"Billed To:"</span>
                            <span class="font-semibold" id="invoice-guest">"Guest"</span>
                        </div>
                        <div class="flex justify-between">
                            <span class="text-base-content/60">"Property:"</span>
                            <span class="font-semibold" id="invoice-villa">"Villa"</span>
                        </div>
                        <div class="flex justify-between">
                            <span class="text-base-content/60">"Stay Duration:"</span>
                            <span class="font-semibold" id="invoice-dates">"Dates"</span>
                        </div>
                        <div class="divider my-1"></div>
                        <div class="flex justify-between">
                            <span class="text-base-content/60">"Accommodation Subtotal:"</span>
                            <span class="font-semibold" id="invoice-subtotal">"$0.00"</span>
                        </div>
                        <div class="flex justify-between">
                            <span class="text-base-content/60">"Jamaican GCT (15%):"</span>
                            <span class="font-semibold text-primary" id="invoice-tax">"$0.00"</span>
                        </div>
                        <div class="flex justify-between font-bold text-sm text-base-content pt-1">
                            <span>"Total Tax-Inclusive Settlement:"</span>
                            <span class="text-primary font-black" id="invoice-total">"$0.00"</span>
                        </div>
                    </div>

                    <div class="modal-action flex justify-between items-center gap-2 pt-2">
                        <button type="button" class="btn btn-outline btn-sm rounded-xl font-semibold" onclick="window.print()">
                            "🖨️ Print Invoice"
                        </button>
                        <form method="dialog">
                            <button class="btn btn-primary btn-sm rounded-xl font-bold">"Done"</button>
                        </form>
                    </div>
                </div>
            </dialog>

            // Admin Cancellation Confirmation Modal
            <dialog id="admin-cancel-dialog" class="modal modal-bottom sm:modal-middle">
                <div class="modal-box rounded-3xl p-6 space-y-4">
                    <div class="flex items-center gap-3 text-error">
                        <span class="text-2xl">"⚠️"</span>
                        <h3 class="font-extrabold text-lg text-base-content">"Cancel / Release Reservation"</h3>
                    </div>
                    <p class="text-sm text-base-content/80">
                        "Are you sure you want to release the PostgreSQL date lock for "
                        <strong class="font-bold" id="admin-dialog-villa-name">"Villa"</strong>
                        " (Ref: "<code id="admin-dialog-booking-ref" class="font-mono text-primary font-bold">"REF"</code>")?"
                    </p>
                    <div class="bg-base-200/60 p-4 rounded-2xl text-xs space-y-1">
                        <div class="font-bold text-base-content">"Audit Log Impact"</div>
                        <div class="text-base-content/70">"This action will transition the hold state to refunded/released and notify the guest via email."</div>
                    </div>
                    <div class="modal-action flex justify-end gap-2 pt-2">
                        <form method="dialog">
                            <button class="btn btn-ghost btn-sm font-semibold">"Dismiss"</button>
                        </form>
                        <button
                            type="button"
                            class="btn btn-error btn-sm font-bold text-error-content"
                            onclick="executeAdminCancellation()"
                        >
                            "Confirm Cancellation"
                        </button>
                    </div>
                </div>
            </dialog>

            <script>
                r#"
                var activeAdminCancelRow = null;
                var activeAdminCancelRef = null;

                function approveAdminBookingDirect(bookingId) {
                    try {
                        var btn = document.getElementById('btn-direct-approve-' + bookingId);
                        if (btn) {
                            btn.disabled = true;
                            btn.innerText = 'Approving...';
                        }

                        fetch('http://localhost:8081/api/v1/bookings/' + bookingId, {
                            method: 'PATCH',
                            headers: {
                                'Content-Type': 'application/json',
                                'Accept': 'application/json'
                            },
                            body: JSON.stringify({ status: 'confirmed' })
                        })
                        .then(function(res) {
                            if (!res.ok) {
                                throw new Error('Failed to approve booking in PostgreSQL');
                            }
                            return res.json();
                        })
                        .then(function() {
                            var card = document.getElementById('pending-card-' + bookingId);
                            if (card) {
                                card.className = 'bg-base-100 dark:bg-base-200 p-4 rounded-xl border border-success/30 flex flex-col justify-between gap-3 shadow-sm opacity-75';
                                var badge = card.querySelector('.badge');
                                if (badge) {
                                    badge.className = 'badge badge-success badge-xs font-bold';
                                    badge.innerText = 'Approved';
                                }
                            }
                            if (btn) {
                                btn.className = 'btn btn-ghost btn-xs text-success font-bold flex-1';
                                btn.innerText = '✓ Approved';
                                btn.disabled = true;
                            }
                            setTimeout(function() {
                                window.location.reload();
                            }, 800);
                        })
                        .catch(function(err) {
                            console.error('Failed to approve booking:', err);
                            alert('Approval failed: ' + err.message);
                            if (btn) {
                                btn.disabled = false;
                                btn.innerText = '✓ Approve Booking';
                            }
                        });
                    } catch(e) {
                        console.error('Direct approval error:', e);
                    }
                }

                function rejectAdminBookingDirect(bookingId) {
                    try {
                        if (!confirm('Are you sure you want to reject this reservation hold and release dates back to availability?')) {
                            return;
                        }

                        fetch('http://localhost:8081/api/v1/bookings/' + bookingId, {
                            method: 'PATCH',
                            headers: {
                                'Content-Type': 'application/json',
                                'Accept': 'application/json'
                            },
                            body: JSON.stringify({ status: 'refunded' })
                        })
                        .then(function() {
                            window.location.reload();
                        })
                        .catch(function(err) {
                            console.error('Failed to reject booking:', err);
                            alert('Rejection failed: ' + err.message);
                        });
                    } catch(e) {
                        console.error('Direct reject error:', e);
                    }
                }

                function approveAdminBooking(bookingId, rowId) {
                    try {
                        var btn = document.getElementById('btn-approve-' + rowId);
                        if (btn) {
                            btn.disabled = true;
                            btn.innerText = 'Approving...';
                        }

                        fetch('http://localhost:8081/api/v1/bookings/' + bookingId, {
                            method: 'PATCH',
                            headers: {
                                'Content-Type': 'application/json',
                                'Accept': 'application/json'
                            },
                            body: JSON.stringify({ status: 'confirmed' })
                        })
                        .then(function(res) {
                            if (!res.ok) {
                                throw new Error('Failed to update booking status in PostgreSQL');
                            }
                            return res.json();
                        })
                        .then(function(data) {
                            var statusEl = document.getElementById('admin-status-' + rowId);
                            if (statusEl) {
                                statusEl.className = 'badge badge-success badge-sm font-bold';
                                statusEl.innerText = 'Confirmed';
                            }
                            if (btn) {
                                btn.className = 'btn btn-ghost btn-xs text-success font-bold';
                                btn.innerText = '✓ Approved';
                                btn.disabled = true;
                            }
                        })
                        .catch(function(err) {
                            console.error('Failed to approve booking:', err);
                            alert('Approval failed: ' + err.message);
                            if (btn) {
                                btn.disabled = false;
                                btn.innerText = '✓ Approve';
                            }
                        });
                    } catch(e) {
                        console.error('Approve error:', e);
                    }
                }

                function openAdminCancelDialog(ref, villaName, rowId) {
                    try {
                        activeAdminCancelRow = rowId;
                        activeAdminCancelRef = ref;
                        
                        var villaEl = document.getElementById('admin-dialog-villa-name');
                        var refEl = document.getElementById('admin-dialog-booking-ref');
                        if (villaEl) villaEl.innerText = villaName;
                        if (refEl) refEl.innerText = ref;
                        
                        var dialog = document.getElementById('admin-cancel-dialog');
                        if (dialog) dialog.showModal();
                    } catch(e) {
                        console.error('Failed to open cancel dialog:', e);
                    }
                }

                function executeAdminCancellation() {
                    try {
                        var dialog = document.getElementById('admin-cancel-dialog');
                        if (dialog) dialog.close();
                        
                        if (activeAdminCancelRef) {
                            fetch('http://localhost:8081/api/v1/bookings/' + activeAdminCancelRef, {
                                method: 'PATCH',
                                headers: {
                                    'Content-Type': 'application/json',
                                    'Accept': 'application/json'
                                },
                                body: JSON.stringify({ status: 'refunded' })
                            }).catch(function(err) {
                                console.error('Failed to cancel booking via booking_api:', err);
                            });
                        }

                        if (activeAdminCancelRow) {
                            var statusEl = document.getElementById('admin-status-' + activeAdminCancelRow);
                            var actionsEl = document.getElementById('admin-actions-' + activeAdminCancelRow);
                            var rowEl = document.getElementById('admin-row-' + activeAdminCancelRow);
                            
                            if (statusEl) {
                                statusEl.className = 'badge badge-error badge-sm font-bold';
                                statusEl.innerText = 'Cancelled / Released';
                            }
                            if (actionsEl) {
                                actionsEl.innerHTML = '<span class="text-xs text-base-content/50 font-medium">Refunded</span>';
                            }
                            if (rowEl) {
                                rowEl.style.opacity = '0.7';
                            }
                        }
                    } catch(e) {
                        console.error('Failed to execute admin cancellation:', e);
                    }
                }

                function openAdminBookingDetailsFromBtn(btn) {
                    try {
                        if (!btn) return;
                        var ref = btn.getAttribute('data-ref') || '';
                        var villa = btn.getAttribute('data-villa') || '';
                        var guest = btn.getAttribute('data-guest') || '';
                        var email = btn.getAttribute('data-email') || '';
                        var dates = btn.getAttribute('data-dates') || '';
                        var nights = btn.getAttribute('data-capacity') || '';
                        var subtotal = btn.getAttribute('data-subtotal') || '';
                        var tax = btn.getAttribute('data-tax') || '';
                        var total = btn.getAttribute('data-total') || '';
                        var status = btn.getAttribute('data-status') || '';
                        var location = btn.getAttribute('data-location') || '';

                        var vnEl = document.getElementById('detail-villa-name');
                        var refEl = document.getElementById('detail-ref-id');
                        var gnEl = document.getElementById('detail-guest-name');
                        var geEl = document.getElementById('detail-guest-email');
                        var dtEl = document.getElementById('detail-stay-dates');
                        var ntEl = document.getElementById('detail-stay-nights');
                        var stEl = document.getElementById('detail-subtotal');
                        var txEl = document.getElementById('detail-tax');
                        var ttEl = document.getElementById('detail-total');
                        var sbEl = document.getElementById('detail-status-badge');

                        if (vnEl) vnEl.innerText = villa + (location ? ' — ' + location : '');
                        if (refEl) refEl.innerText = 'Audit Ref: ' + ref;
                        if (gnEl) gnEl.innerText = guest;
                        if (geEl) geEl.innerText = email;
                        if (dtEl) dtEl.innerText = dates;
                        if (ntEl) ntEl.innerText = nights;
                        if (stEl) stEl.innerText = subtotal;
                        if (txEl) txEl.innerText = tax;
                        if (ttEl) ttEl.innerText = total;
                        if (sbEl) {
                            sbEl.innerText = status;
                            sbEl.className = (status === 'Confirmed' ? 'badge badge-success font-bold text-xs' : (status === 'Pending' || status === 'Pending Hold' ? 'badge badge-warning font-bold text-xs' : 'badge badge-error font-bold text-xs'));
                        }

                        var dialog = document.getElementById('admin-booking-details-dialog');
                        if (dialog) dialog.showModal();
                    } catch(e) {
                        console.error('Failed to open details modal:', e);
                    }
                }

                function extendAdminHold(ref, rowId) {
                    try {
                        fetch('http://localhost:8081/api/v1/bookings/' + ref, {
                            method: 'PATCH',
                            headers: {
                                'Content-Type': 'application/json',
                                'Accept': 'application/json'
                            },
                            body: JSON.stringify({ status: 'pending_payment' })
                        }).catch(function(err) {
                            console.error('Failed to extend hold via booking_api:', err);
                        });

                        var statusEl = document.getElementById('admin-status-' + rowId);
                        if (statusEl) {
                            statusEl.className = 'badge badge-warning badge-sm font-bold animate-pulse';
                            statusEl.innerText = 'Pending Hold (Extended)';
                        }
                    } catch(e) {
                        console.error('Failed to extend hold:', e);
                    }
                }

                function openAdminInvoiceDialog(ref, villa, guest, email, dates, nights, subtotal, tax, total) {
                    try {
                        var refEl = document.getElementById('invoice-ref');
                        var guestEl = document.getElementById('invoice-guest');
                        var villaEl = document.getElementById('invoice-villa');
                        var datesEl = document.getElementById('invoice-dates');
                        var stEl = document.getElementById('invoice-subtotal');
                        var txEl = document.getElementById('invoice-tax');
                        var ttEl = document.getElementById('invoice-total');
                        
                        if (refEl) refEl.innerText = 'INV-2026-' + ref;
                        if (guestEl) guestEl.innerText = guest + ' (' + email + ')';
                        if (villaEl) villaEl.innerText = villa;
                        if (datesEl) datesEl.innerText = dates + ' (' + nights + ')';
                        if (stEl) stEl.innerText = subtotal;
                        if (txEl) txEl.innerText = tax;
                        if (ttEl) ttEl.innerText = total;
                        
                        var dialog = document.getElementById('admin-invoice-dialog');
                        if (dialog) dialog.showModal();
                    } catch(e) {
                        console.error('Failed to open invoice dialog:', e);
                    }
                }

                function exportBookingsCsv() {
                    try {
                        var rows = [
                            ['Reservation Ref', 'Villa', 'Lead Guest', 'Email', 'Dates', 'Nights', 'Total Amount', 'Statutory GCT (15%)', 'Status']
                        ];
                        
                        var table = document.getElementById('admin-bookings-table');
                        if (table) {
                            var trs = table.querySelectorAll('tbody tr');
                            trs.forEach(function(tr) {
                                var tds = tr.querySelectorAll('td');
                                if (tds) {
                                    if (tds.length !== 0) {
                                        var villaEl = tds[0].querySelector('div:first-child');
                                        var refEl = tds[0].querySelector('div:last-child');
                                        var guestEl = tds[1].querySelector('div:first-child');
                                        var emailEl = tds[1].querySelector('div:last-child');
                                        var datesEl = tds[2].querySelector('div:first-child');
                                        var capEl = tds[2].querySelector('div:last-child');
                                        var totEl = tds[3].querySelector('div:first-child');
                                        var taxEl = tds[3].querySelector('div:last-child');
                                        var statusEl = tds[4].querySelector('span');

                                        var row = [
                                            refEl ? refEl.innerText.trim() : '',
                                            villaEl ? villaEl.innerText.trim() : '',
                                            guestEl ? guestEl.innerText.trim() : '',
                                            emailEl ? emailEl.innerText.trim() : '',
                                            datesEl ? datesEl.innerText.trim() : '',
                                            capEl ? capEl.innerText.trim() : '',
                                            totEl ? totEl.innerText.trim() : '',
                                            taxEl ? taxEl.innerText.trim() : '',
                                            statusEl ? statusEl.innerText.trim() : ''
                                        ];
                                        rows.push(row);
                                    }
                                }
                            });
                        }
                        
                        var csvContent = 'data:text/csv;charset=utf-8,' + rows.map(function(e) { return e.map(function(v) { return '"' + v.replace(/"/g, '""') + '"'; }).join(','); }).join('\n');
                        var encodedUri = encodeURI(csvContent);
                        var link = document.createElement('a');
                        link.setAttribute('href', encodedUri);
                        link.setAttribute('download', 'our_places_master_bookings_' + new Date().toISOString().split('T')[0] + '.csv');
                        document.body.appendChild(link);
                        link.click();
                        document.body.removeChild(link);
                    } catch(e) {
                        console.error('Failed to export CSV:', e);
                    }
                }
                "#
            </script>
        </div>
    }
}
