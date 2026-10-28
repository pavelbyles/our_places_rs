use common::models::{
    BookingMessageResponse, BookingMessagesWrapper, BookingResponse, CreateBookingMessageRequest,
    CreateReservationChangeRequest, MarkMessagesReadResponse, ReservationChangeQuoteRequest,
    ReservationChangeQuoteResponse, ReservationChangeRequestResponse,
    RespondReservationChangeRequest, UpdatedBookingRequest,
};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Json, page, path_param, route},
    view::{View, view},
};
use uuid::Uuid;
use web_app_common_tc::{client::ListingSearchParams, get_api_client};

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    exp: usize,
}

fn generate_jwt_for_user(user_id: Uuid) -> String {
    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());
    let claims = Claims {
        sub: user_id,
        exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap_or_default()
}

path_param!(id);
path_param!(request_id);

#[page("/bookings")]
pub async fn bookings_alias_page(cx: &Cx) -> Result<impl View> {
    render_bookings_content(cx).await
}

#[page("/admin/bookings")]
pub async fn admin_bookings_page(cx: &Cx) -> Result<impl View> {
    render_bookings_content(cx).await
}

async fn render_bookings_content(cx: &Cx) -> Result<impl View> {
    let __cx = cx;
    let admin_user = web_app_common_tc::auth::get_admin_session(cx).await;
    let is_authed = admin_user.is_some();
    let is_admin = admin_user.as_ref().is_some_and(|u| u.is_admin());
    let host_user_id = admin_user.as_ref().and_then(|u| u.id);
    let host_email = admin_user.as_ref().map(|u| u.email.clone());
    let api = get_api_client(cx);

    let raw_listings = if is_admin {
        match api
            .search_listings(ListingSearchParams {
                per_page: Some(100),
                ..Default::default()
            })
            .await
        {
            Ok(l) => l,
            Err(err) => {
                tracing::error!("Failed to fetch listings in bookings admin: {:?}", err);
                Vec::new()
            }
        }
    } else {
        let mut results = if let Some(uid) = host_user_id {
            match api
                .search_listings(ListingSearchParams {
                    user_id: Some(uid),
                    per_page: Some(100),
                    ..Default::default()
                })
                .await
            {
                Ok(l) => l,
                Err(err) => {
                    tracing::warn!(
                        "Failed to fetch bookings listings by user_id {:?}: {:?}",
                        uid,
                        err
                    );
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        if results.is_empty()
            && let Some(ref email) = host_email
            && !email.trim().is_empty()
        {
            match api
                .search_listings(ListingSearchParams {
                    owner_email: Some(email.clone()),
                    per_page: Some(100),
                    ..Default::default()
                })
                .await
            {
                Ok(l) => {
                    results = l;
                }
                Err(err) => {
                    tracing::error!(
                        "Failed to fetch bookings listings by owner_email {:?}: {:?}",
                        email,
                        err
                    );
                }
            }
        }
        results
    };

    let host_listing_ids: std::collections::HashSet<uuid::Uuid> =
        raw_listings.iter().map(|l| l.id).collect();

    let raw_bookings = match api.get_all_bookings(Some(1), Some(100)).await {
        Ok(b) => b,
        Err(err) => {
            tracing::error!("Failed to fetch bookings in bookings admin: {:?}", err);
            Vec::new()
        }
    };

    let bookings: Vec<common::models::BookingResponse> = if is_admin {
        raw_bookings
    } else {
        raw_bookings
            .into_iter()
            .filter(|b| host_listing_ids.contains(&b.listing_id))
            .collect()
    };

    let listings = raw_listings;
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

    let pending_bookings: Vec<common::models::BookingResponse> = bookings
        .iter()
        .filter(|b| {
            b.status.eq_ignore_ascii_case("pending")
                || b.status.eq_ignore_ascii_case("pending_payment")
        })
        .cloned()
        .collect();
    let pending_count = pending_bookings.len();

    let total_count = bookings.len();

    Ok(view! {
        if !is_authed {
            <script>
                r#"window.location.replace('/login?redirect=' + encodeURIComponent(window.location.pathname + window.location.search));"#
            </script>
        } else {
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
                            let (v_name, _) = listing_map.get(&pb.listing_id).cloned().unwrap_or_else(|| ("Luxury Listing".to_string(), "Jamaica".to_string()));
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
                                        id=(format!("btn-direct-reject-{}", pb_id))
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
                        placeholder="Search by guest name, booking UUID, or listing..."
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
                                        .unwrap_or_else(|| ("Luxury Listing".to_string(), "Jamaica".to_string()));
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
                                    let status_lower = b.status.to_lowercase();
                                    let is_cancelled = status_lower == "cancelled" || status_lower == "refunded";
                                    let is_completed = status_lower == "completed";
                                    let is_pending = status_lower == "pending" || status_lower == "pending_payment";
                                    let can_cancel = !is_cancelled && !is_completed;

                                    let status_display = match status_lower.as_str() {
                                        "confirmed" => "Confirmed",
                                        "pending" | "pending_payment" => "Pending Hold",
                                        "completed" => "Completed Stay",
                                        "cancelled" | "refunded" => "Cancelled",
                                        _ => b.status.as_str(),
                                    };
                                    let badge_class = match status_lower.as_str() {
                                        "confirmed" => "badge badge-success badge-sm font-bold",
                                        "pending" | "pending_payment" => "badge badge-warning badge-sm font-bold",
                                        "completed" => "badge badge-neutral badge-sm font-semibold",
                                        _ => "badge badge-error badge-sm font-semibold",
                                    };

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
                                                data-id=(b_id.clone())
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
                                                data-door-code=(b.door_access_code.clone().unwrap_or_default())
                                                data-date-from=(b.date_from.to_string())
                                                data-date-to=(b.date_to.to_string())
                                                data-guests=(b.number_of_persons.to_string())
                                                onclick="openAdminBookingDetailsFromBtn(this)"
                                            >
                                                "Details"
                                            </button>
                                            if can_cancel {
                                                <button
                                                    type="button"
                                                    class="btn btn-ghost btn-xs text-error font-bold"
                                                    id=(format!("btn-cancel-{}", idx + 1))
                                                    onclick=(format!("openAdminCancelDialog('{}', '{}', '{}', '{}')", b_id, conf_code, villa_name, idx + 1))
                                                >
                                                    "Reject / Cancel"
                                                </button>
                                            }
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
                            <h3 class="font-serif font-bold text-xl text-base-content" id="detail-villa-name">"Listing Name"</h3>
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

                    // Keyless Access & Door Code Section
                    <div class="bg-base-200/50 p-4 rounded-2xl space-y-3 text-xs">
                        <div class="flex items-center justify-between">
                            <div>
                                <span class="text-base-content/50 font-bold uppercase block text-[10px]">"Keyless Access & Arrival Guide"</span>
                                <h4 class="font-bold text-sm text-base-content">"Door Access Code"</h4>
                            </div>
                            <span id="detail-door-code-badge" class="badge badge-neutral badge-xs font-semibold">"Not Set"</span>
                        </div>
                        <p class="text-xs text-base-content/70">
                            "Configured statically by host or admin. Included automatically in the 48-Hour Pre-Arrival Guide sent to the guest."
                        </p>
                        <div class="flex items-center gap-2">
                            <input
                                type="text"
                                id="detail-door-code-input"
                                placeholder="e.g. 4829 or #KEY123"
                                class="input input-bordered input-sm flex-1 font-mono font-bold rounded-xl"
                            />
                            <button
                                type="button"
                                id="btn-save-door-code"
                                onclick="saveAdminDoorCode()"
                                class="btn btn-primary btn-sm rounded-xl font-bold"
                            >
                                "Save Code"
                            </button>
                        </div>
                        <div id="door-code-status-msg" class="text-[11px] text-success hidden font-semibold"></div>
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

                    // Tax & Compliance Documents Section
                    <div class="bg-base-200/80 p-4 rounded-2xl space-y-3 text-xs mt-4" id="admin-booking-documents-section">
                        <div class="flex items-center justify-between border-b border-base-300 pb-1.5">
                            <span class="font-bold text-xs uppercase tracking-wider text-base-content/70">
                                "Statutory Tax Documents"
                            </span>
                            <span id="detail-docs-count-badge" class="badge badge-primary badge-xs font-semibold">"0 Documents"</span>
                        </div>
                        <div id="admin-booking-documents-list" class="space-y-2">
                            <div class="text-xs text-base-content/50 italic py-1" id="detail-docs-empty-msg">"Loading documents..."</div>
                        </div>
                    </div>

                    // Reservation Change Requests & Modification Controls
                    <div class="bg-base-200/80 p-4 rounded-2xl space-y-3 text-xs mt-4" id="admin-booking-change-requests-section">
                        <div class="flex items-center justify-between border-b border-base-300 pb-1.5">
                            <span class="font-bold text-xs uppercase tracking-wider text-base-content/70">
                                "Reservation Change Requests & Modifications"
                            </span>
                            <span id="admin-change-req-count-badge" class="badge badge-primary badge-xs font-semibold">"0 Pending"</span>
                        </div>
                        <div id="admin-booking-change-requests-list" class="space-y-2">
                            <div class="text-xs text-base-content/50 italic py-1" id="detail-changes-empty-msg">"Loading change requests..."</div>
                        </div>

                        // Form for Host / Admin to propose or force-apply changes
                        <div class="border-t border-base-300 pt-3 space-y-2">
                            <div class="font-bold text-xs text-base-content">"Propose or Apply Stay Modification"</div>
                            <div class="grid grid-cols-2 gap-2">
                                <div>
                                    <label class="block text-[10px] font-bold text-base-content/60 uppercase">"Check-in"</label>
                                    <input type="date" id="admin-change-date-from" class="input input-bordered input-xs w-full rounded-lg" onchange="previewAdminChangeQuote()" />
                                </div>
                                <div>
                                    <label class="block text-[10px] font-bold text-base-content/60 uppercase">"Check-out"</label>
                                    <input type="date" id="admin-change-date-to" class="input input-bordered input-xs w-full rounded-lg" onchange="previewAdminChangeQuote()" />
                                </div>
                            </div>
                            <div class="grid grid-cols-2 gap-2">
                                <div>
                                    <label class="block text-[10px] font-bold text-base-content/60 uppercase">"Adults (13+)"</label>
                                    <input type="number" id="admin-change-adults" min="1" value="1" class="input input-bordered input-xs w-full rounded-lg font-bold" onchange="previewAdminChangeQuote()" />
                                </div>
                                <div>
                                    <label class="block text-[10px] font-bold text-base-content/60 uppercase">"Children (2–12)"</label>
                                    <input type="number" id="admin-change-children" min="0" value="0" class="input input-bordered input-xs w-full rounded-lg font-bold" onchange="previewAdminChangeQuote()" />
                                </div>
                            </div>
                            <div>
                                <label class="block text-[10px] font-bold text-base-content/60 uppercase">"Reason / Host Note"</label>
                                <input type="text" id="admin-change-reason" placeholder="e.g. Host approved room upgrade and date adjustment" class="input input-bordered input-xs w-full rounded-lg" />
                            </div>
                            <div class="flex items-center gap-2 pt-1">
                                <input type="checkbox" id="admin-change-force-apply" class="checkbox checkbox-primary checkbox-xs" />
                                <label for="admin-change-force-apply" class="text-[11px] font-semibold text-base-content cursor-pointer">
                                    "Force-apply immediately (Admin override, bypass guest confirmation)"
                                </label>
                            </div>

                            <div id="admin-change-quote-preview" class="hidden bg-base-100 p-2.5 rounded-xl border border-base-300 text-[11px] space-y-1"></div>
                            <div id="admin-change-feedback-msg" class="hidden text-[11px] font-semibold"></div>

                            <div class="flex items-center justify-between pt-1">
                                <button type="button" class="btn btn-outline btn-xs font-semibold" onclick="previewAdminChangeQuote()">
                                    "Preview Price Delta"
                                </button>
                                <button type="button" id="btn-admin-submit-change" class="btn btn-primary btn-xs font-bold px-3" onclick="submitAdminChangeRequest()">
                                    "Submit Modification"
                                </button>
                            </div>
                        </div>
                    </div>

                    // Messaging Section
                    <div class="bg-base-200/80 p-4 rounded-2xl space-y-2 text-xs mt-4 flex flex-row items-center justify-between">
                        <div class="font-bold text-xs uppercase tracking-wider text-base-content/70">
                            "Host/Guest Communications"
                        </div>
                        <a id="admin-msg-link" href="#" class="btn btn-primary btn-sm rounded-xl font-bold">"💬 Open Messages"</a>
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
                            <span class="font-semibold" id="invoice-villa">"Listing"</span>
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
                        <h3 class="font-extrabold text-lg text-base-content">"Cancel Reservation"</h3>
                    </div>
                    <p class="text-sm text-base-content/80">
                        "Are you sure you want to cancel the reservation for "
                        <strong class="font-bold" id="admin-dialog-villa-name">"Listing"</strong>
                        " (Booking Code: "<code id="admin-dialog-booking-ref" class="font-mono text-primary font-bold">"REF"</code>")?"
                    </p>
                    <div class="bg-base-200/60 p-4 rounded-2xl text-xs space-y-1">
                        <div class="font-bold text-base-content">"Status Update"</div>
                        <div class="text-base-content/70">"This action will release the dates back to availability and notify the guest via email."</div>
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
                var activeAdminCancelBookingId = null;
                var activeDetailBookingId = null;

                function approveAdminBookingDirect(bookingId) {
                    try {
                        var btn = document.getElementById('btn-direct-approve-' + bookingId);
                        if (btn) {
                            btn.disabled = true;
                            btn.innerText = 'Approving...';
                        }

                        fetch('/api/bookings/' + bookingId, {
                            method: 'PATCH',
                            headers: {
                                'Content-Type': 'application/json',
                                'Accept': 'application/json'
                            },
                            body: JSON.stringify({ status: 'confirmed' })
                        })
                        .then(function(res) {
                            if (!res.ok) {
                                return res.text().then(function(t) {
                                    throw new Error(t || ('Server error (' + res.status + ')'));
                                });
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

                        var btn = document.getElementById('btn-direct-reject-' + bookingId);
                        if (btn) {
                            btn.disabled = true;
                            btn.replaceChildren();
                            var sp = document.createElement('span');
                            sp.className = 'loading loading-spinner loading-xs text-error mr-1';
                            btn.appendChild(sp);
                            btn.appendChild(document.createTextNode(' Rejecting...'));
                        }

                        fetch('/api/bookings/' + bookingId, {
                            method: 'PATCH',
                            headers: {
                                'Content-Type': 'application/json',
                                'Accept': 'application/json'
                            },
                            body: JSON.stringify({ status: 'cancelled' })
                        })
                        .then(function(res) {
                            if (!res.ok) {
                                return res.text().then(function(t) {
                                    throw new Error(t || ('Server error (' + res.status + ')'));
                                });
                            }
                            return res.json();
                        })
                        .then(function() {
                            window.location.reload();
                        })
                        .catch(function(err) {
                            console.error('Failed to reject booking:', err);
                            alert('Rejection failed: ' + err.message);
                            if (btn) {
                                btn.disabled = false;
                                btn.textContent = 'Reject';
                            }
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

                        fetch('/api/bookings/' + bookingId, {
                            method: 'PATCH',
                            headers: {
                                'Content-Type': 'application/json',
                                'Accept': 'application/json'
                            },
                            body: JSON.stringify({ status: 'confirmed' })
                        })
                        .then(function(res) {
                            if (!res.ok) {
                                return res.text().then(function(t) {
                                    throw new Error(t || ('Server error (' + res.status + ')'));
                                });
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

                function openAdminCancelDialog(bookingId, confCode, villaName, rowId) {
                    try {
                        activeAdminCancelRow = rowId;
                        activeAdminCancelBookingId = bookingId;
                        activeAdminCancelRef = confCode;
                        
                        var villaEl = document.getElementById('admin-dialog-villa-name');
                        var refEl = document.getElementById('admin-dialog-booking-ref');
                        if (villaEl) villaEl.innerText = villaName;
                        if (refEl) refEl.innerText = confCode;
                        
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
                        
                        var targetId = activeAdminCancelBookingId;
                        var targetRow = activeAdminCancelRow;
                        if (!targetId) return;

                        var cancelBtn = document.getElementById('btn-cancel-' + targetRow);
                        if (cancelBtn) {
                            cancelBtn.disabled = true;
                            cancelBtn.replaceChildren();
                            var sp = document.createElement('span');
                            sp.className = 'loading loading-spinner loading-xs text-error mr-1';
                            cancelBtn.appendChild(sp);
                            cancelBtn.appendChild(document.createTextNode(' Cancelling...'));
                        }

                        fetch('/api/bookings/' + targetId, {
                            method: 'PATCH',
                            headers: {
                                'Content-Type': 'application/json',
                                'Accept': 'application/json'
                            },
                            body: JSON.stringify({ status: 'cancelled' })
                        })
                        .then(function(res) {
                            if (!res.ok) {
                                return res.text().then(function(t) {
                                    throw new Error(t || ('Server error (' + res.status + ')'));
                                });
                            }
                            return res.json();
                        })
                        .then(function() {
                            if (targetRow) {
                                var statusEl = document.getElementById('admin-status-' + targetRow);
                                var rowEl = document.getElementById('admin-row-' + targetRow);
                                
                                if (statusEl) {
                                    statusEl.className = 'badge badge-error badge-sm font-semibold';
                                    statusEl.innerText = 'Cancelled';
                                }
                                if (cancelBtn) {
                                    cancelBtn.remove();
                                }
                                var approveBtn = document.getElementById('btn-approve-' + targetRow);
                                if (approveBtn) {
                                    approveBtn.remove();
                                }
                                if (rowEl) {
                                    rowEl.style.opacity = '0.7';
                                }
                            }
                        })
                        .catch(function(err) {
                            console.error('Failed to cancel booking via booking_api:', err);
                            alert('Cancellation failed: ' + err.message);
                            if (cancelBtn) {
                                cancelBtn.disabled = false;
                                cancelBtn.textContent = 'Reject / Cancel';
                            }
                        });
                    } catch(e) {
                        console.error('Failed to execute admin cancellation:', e);
                    }
                }

                function openAdminBookingDetailsFromBtn(btn) {
                    try {
                        if (!btn) return;
                        var b_id = btn.getAttribute('data-id') || '';
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

                        activeDetailBookingId = b_id;
                        var doorCode = btn.getAttribute('data-door-code') || '';
                        var dcInput = document.getElementById('detail-door-code-input');
                        var dcBadge = document.getElementById('detail-door-code-badge');
                        if (dcInput) dcInput.value = doorCode;
                        if (dcBadge) {
                            if (doorCode) {
                                dcBadge.innerText = 'Configured (' + doorCode + ')';
                                dcBadge.className = 'badge badge-success badge-xs font-bold';
                            } else {
                                dcBadge.innerText = 'Not Set';
                                dcBadge.className = 'badge badge-warning badge-xs font-semibold';
                            }
                        }
                        var statusMsg = document.getElementById('door-code-status-msg');
                        if (statusMsg) {
                            statusMsg.className = 'text-[11px] text-success hidden font-semibold';
                            statusMsg.innerText = '';
                        }

                        var msgLink = document.getElementById('admin-msg-link');
                        if (msgLink) {
                            if (ref || b_id) {
                                msgLink.href = '/admin/bookings/' + (ref || b_id) + '/messages';
                            }
                        }

                        var docsContainer = document.getElementById('admin-booking-documents-list');
                        var docsBadge = document.getElementById('detail-docs-count-badge');
                        if (docsContainer) {
                            docsContainer.innerHTML = '<div class="text-xs text-base-content/50 italic py-1">Loading documents...</div>';
                        }
                        if (b_id) {
                            fetch('/api/bookings/' + b_id + '/documents')
                                .then(function(res) {
                                    if (!res.ok) throw new Error('Status ' + res.status);
                                    return res.json();
                                })
                                .then(function(docs) {
                                    if (!docsContainer) return;
                                    if (!Array.isArray(docs) || docs.length === 0) {
                                        docsContainer.innerHTML = '<div class="text-xs text-base-content/50 italic py-1">No statutory tax documents issued yet.</div>';
                                        if (docsBadge) docsBadge.innerText = '0 Documents';
                                        return;
                                    }
                                    if (docsBadge) docsBadge.innerText = docs.length + ' Document' + (docs.length === 1 ? '' : 's');
                                    var html = '';
                                    docs.forEach(function(doc) {
                                        var isSuperseded = (doc.status === 'superseded' || doc.status === 'Superseded');
                                        var statusBadge = isSuperseded
                                            ? '<span class="badge badge-neutral badge-xs font-semibold">Superseded</span>'
                                            : '<span class="badge badge-success badge-xs font-semibold">Issued</span>';
                                        var typeLabel = (doc.document_type === 'tax_invoice' || doc.document_type === 'TaxInvoice')
                                            ? 'Tax Invoice'
                                            : ((doc.document_type === 'refund_receipt' || doc.document_type === 'RefundReceipt')
                                                ? 'Refund Receipt'
                                                : 'Payment Receipt');
                                        var docUrl = '/bookings/' + b_id + '/documents/' + doc.document_number;
                                        html += '<div class="flex items-center justify-between p-2.5 rounded-xl bg-base-100 border border-base-300/60 text-xs">';
                                        html += '  <div class="space-y-0.5">';
                                        html += '    <div class="flex items-center gap-1.5">';
                                        html += '      <span class="font-mono font-bold text-primary">' + doc.document_number + '</span>';
                                        html += '      ' + statusBadge;
                                        html += '    </div>';
                                        html += '    <div class="text-base-content/60 text-[11px]">' + typeLabel + ' • Rev ' + doc.revision + ' • ' + (doc.currency || 'USD') + ' ' + (doc.total_amount || '0.00') + '</div>';
                                        html += '  </div>';
                                        html += '  <a href="' + docUrl + '" target="_blank" class="btn btn-ghost btn-xs text-primary font-bold">View ↗</a>';
                                        html += '</div>';
                                    });
                                    docsContainer.innerHTML = html;
                                })
                                .catch(function(err) {
                                    if (docsContainer) {
                                        docsContainer.innerHTML = '<div class="text-xs text-error py-1">Failed to load documents.</div>';
                                    }
                                });
                        }

                        var dateFrom = btn.getAttribute('data-date-from') || '';
                        var dateTo = btn.getAttribute('data-date-to') || '';
                        var guestsCount = btn.getAttribute('data-guests') || '1';
                        var cfInput = document.getElementById('admin-change-date-from');
                        var ctInput = document.getElementById('admin-change-date-to');
                        var caInput = document.getElementById('admin-change-adults');
                        var ccInput = document.getElementById('admin-change-children');
                        var crInput = document.getElementById('admin-change-reason');
                        var cfaInput = document.getElementById('admin-change-force-apply');
                        var cpDiv = document.getElementById('admin-change-quote-preview');
                        var cfbDiv = document.getElementById('admin-change-feedback-msg');

                        if (cfInput) cfInput.value = dateFrom;
                        if (ctInput) ctInput.value = dateTo;
                        if (caInput) caInput.value = guestsCount;
                        if (ccInput) ccInput.value = '0';
                        if (crInput) crInput.value = '';
                        if (cfaInput) cfaInput.checked = false;
                        if (cpDiv) cpDiv.classList.add('hidden');
                        if (cfbDiv) cfbDiv.classList.add('hidden');

                        if (b_id) {
                            loadAdminChangeRequests(b_id);
                        }

                        var dialog = document.getElementById('admin-booking-details-dialog');
                        if (dialog) dialog.showModal();
                    } catch(e) {
                        console.error('Failed to open details modal:', e);
                    }
                }

                function extendAdminHold(ref, rowId) {
                    try {
                        fetch('/api/bookings/' + ref, {
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
                            ['Reservation Ref', 'Listing', 'Lead Guest', 'Email', 'Dates', 'Nights', 'Total Amount', 'Statutory GCT (15%)', 'Status']
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

                function saveAdminDoorCode() {
                    try {
                        var bookingId = activeDetailBookingId;
                        if (!bookingId) return;

                        var input = document.getElementById('detail-door-code-input');
                        var btn = document.getElementById('btn-save-door-code');
                        var badge = document.getElementById('detail-door-code-badge');
                        var msg = document.getElementById('door-code-status-msg');
                        var newCode = input ? input.value.trim() : '';

                        if (btn) {
                            btn.disabled = true;
                            btn.innerText = 'Saving...';
                        }

                        fetch('/api/bookings/' + bookingId, {
                            method: 'PATCH',
                            headers: {
                                'Content-Type': 'application/json',
                                'Accept': 'application/json'
                            },
                            body: JSON.stringify({ door_access_code: newCode })
                        })
                        .then(function(res) {
                            if (!res.ok) {
                                return res.text().then(function(t) {
                                    throw new Error(t || ('Server error (' + res.status + ')'));
                                });
                            }
                            return res.json();
                        })
                        .then(function(data) {
                            if (btn) {
                                btn.disabled = false;
                                btn.innerText = 'Save Code';
                            }
                            if (badge) {
                                if (newCode) {
                                    badge.innerText = 'Configured (' + newCode + ')';
                                    badge.className = 'badge badge-success badge-xs font-bold';
                                } else {
                                    badge.innerText = 'Not Set';
                                    badge.className = 'badge badge-warning badge-xs font-semibold';
                                }
                            }
                            var triggerBtn = document.querySelector('button[data-id="' + bookingId + '"]');
                            if (triggerBtn) {
                                triggerBtn.setAttribute('data-door-code', newCode);
                            }
                            if (msg) {
                                msg.innerText = '✓ Door access code updated successfully.';
                                msg.className = 'text-[11px] text-success font-semibold';
                                setTimeout(function() {
                                    msg.className = 'text-[11px] text-success hidden font-semibold';
                                }, 8000);
                            }
                        })
                        .catch(function(err) {
                            console.error('Failed to update door access code:', err);
                            alert('Failed to save door access code: ' + err.message);
                            if (btn) {
                                btn.disabled = false;
                                btn.innerText = 'Save Code';
                            }
                        });
                    } catch(e) {
                        console.error('Error in saveAdminDoorCode:', e);
                    }
                }

                function loadAdminChangeRequests(bookingId) {
                    var container = document.getElementById('admin-booking-change-requests-list');
                    var badge = document.getElementById('admin-change-req-count-badge');
                    if (!container || !bookingId) return;
                    container.innerHTML = '<div class="text-xs text-base-content/50 italic py-1">Loading change requests...</div>';
                    fetch('/api/bookings/' + bookingId + '/change-requests')
                        .then(function(res) {
                            if (!res.ok) throw new Error('Status ' + res.status);
                            return res.json();
                        })
                        .then(function(reqs) {
                            if (!Array.isArray(reqs) || reqs.length === 0) {
                                container.innerHTML = '<div class="text-xs text-base-content/50 italic py-1">No modification requests on record.</div>';
                                if (badge) badge.innerText = '0 Pending';
                                return;
                            }
                            var pending = reqs.filter(function(r) { return r.status === 'pending'; });
                            if (badge) badge.innerText = pending.length + ' Pending';

                            var html = '';
                            reqs.forEach(function(r) {
                                var isPending = r.status === 'pending';
                                var statusBadge = isPending
                                    ? '<span class="badge badge-warning badge-xs font-bold">Pending Review</span>'
                                    : (r.status === 'approved'
                                        ? '<span class="badge badge-success badge-xs font-bold">Approved</span>'
                                        : '<span class="badge badge-neutral badge-xs font-semibold">' + r.status + '</span>');
                                var roleLabel = r.initiated_by_role === 'guest' ? 'Guest Proposed' : 'Host/Admin Proposed';
                                var balVal = parseFloat(r.balance_due || '0');
                                var balBadge = balVal > 0
                                    ? '<span class="text-warning font-bold">Extra Charge: ' + r.currency + ' ' + r.balance_due + '</span>'
                                    : (balVal < 0
                                        ? '<span class="text-success font-bold">Refund: ' + r.currency + ' ' + Math.abs(balVal).toFixed(2) + '</span>'
                                        : '<span class="text-base-content/70 font-medium">No Price Diff</span>');

                                html += '<div class="p-3 rounded-xl bg-base-100 border border-base-300 space-y-1.5 text-xs">';
                                html += '  <div class="flex items-center justify-between">';
                                html += '    <span class="font-bold text-base-content">' + roleLabel + '</span>';
                                html += '    ' + statusBadge;
                                html += '  </div>';
                                html += '  <div class="text-base-content/80">';
                                html += '    <strong>Stay:</strong> ' + r.proposed_date_from + ' → ' + r.proposed_date_to + ' (' + r.proposed_total_days + ' nights, ' + r.proposed_number_of_persons + ' guests)';
                                html += '  </div>';
                                html += '  <div class="flex items-center justify-between text-[11px]">';
                                html += '    <span>Total: <strong>' + r.currency + ' ' + r.proposed_total_price + '</strong> (' + balBadge + ')</span>';
                                html += '    <span>Payout Delta: <strong class="font-mono text-primary">' + r.currency + ' ' + r.host_payout_delta + '</strong></span>';
                                html += '  </div>';
                                if (r.reason) {
                                    html += '  <div class="text-[11px] text-base-content/60 italic">"' + r.reason + '"</div>';
                                }
                                if (isPending) {
                                    html += '  <div class="flex items-center gap-2 pt-1 border-t border-base-200 mt-1">';
                                    if (r.initiated_by_role === 'guest') {
                                        html += '    <button type="button" class="btn btn-success btn-xs text-white font-bold" onclick="respondToAdminChangeRequest(\'' + bookingId + '\', \'' + r.id + '\', \'approve\')">✓ Approve Change</button>';
                                        html += '    <button type="button" class="btn btn-outline btn-error btn-xs font-bold" onclick="respondToAdminChangeRequest(\'' + bookingId + '\', \'' + r.id + '\', \'decline\')">✕ Decline</button>';
                                    } else {
                                        html += '    <button type="button" class="btn btn-outline btn-xs font-bold" onclick="respondToAdminChangeRequest(\'' + bookingId + '\', \'' + r.id + '\', \'cancel\')">Withdraw Proposal</button>';
                                    }
                                    html += '  </div>';
                                }
                                html += '</div>';
                            });
                            container.innerHTML = html;
                        })
                        .catch(function(err) {
                            container.innerHTML = '<div class="text-xs text-error py-1">Failed to load change requests.</div>';
                        });
                }

                function respondToAdminChangeRequest(bookingId, requestId, action) {
                    try {
                        var url = '/api/bookings/' + bookingId + '/change-requests/' + requestId + '/' + action;
                        var opts = {
                            method: 'POST',
                            headers: {
                                'Content-Type': 'application/json',
                                'Accept': 'application/json'
                            }
                        };
                        if (action === 'decline') {
                            opts.body = JSON.stringify({ reason: 'Declined by host/admin' });
                        }
                        fetch(url, opts)
                            .then(function(res) {
                                if (!res.ok) {
                                    return res.text().then(function(t) { throw new Error(t || ('Status ' + res.status)); });
                                }
                                return res.json();
                            })
                            .then(function() {
                                loadAdminChangeRequests(bookingId);
                                setTimeout(function() { window.location.reload(); }, 600);
                            })
                            .catch(function(err) {
                                console.error('Failed to process change request action:', err);
                                alert('Action failed: ' + err.message);
                            });
                    } catch(e) {
                        console.error('Change request response error:', e);
                    }
                }

                function gatherAdminChangePayload() {
                    var dateFrom = document.getElementById('admin-change-date-from').value;
                    var dateTo = document.getElementById('admin-change-date-to').value;
                    var adults = parseInt(document.getElementById('admin-change-adults').value || '1', 10);
                    var children = parseInt(document.getElementById('admin-change-children').value || '0', 10);
                    var reasonEl = document.getElementById('admin-change-reason');
                    var reason = reasonEl && reasonEl.value.trim() ? reasonEl.value.trim() : null;
                    var forceApply = document.getElementById('admin-change-force-apply').checked;

                    return {
                        proposed_date_from: dateFrom,
                        proposed_date_to: dateTo,
                        proposed_adults: isNaN(adults) ? 1 : adults,
                        proposed_children: isNaN(children) ? 0 : children,
                        proposed_infants: 0,
                        proposed_pets: 0,
                        reason: reason,
                        force_apply: forceApply
                    };
                }

                function previewAdminChangeQuote() {
                    var bookingId = activeDetailBookingId;
                    if (!bookingId) return;
                    var preview = document.getElementById('admin-change-quote-preview');
                    var feedback = document.getElementById('admin-change-feedback-msg');
                    var payload = gatherAdminChangePayload();

                    if (!payload.proposed_date_from || !payload.proposed_date_to || payload.proposed_date_from >= payload.proposed_date_to) {
                        return;
                    }
                    if (preview) {
                        preview.classList.remove('hidden');
                        preview.innerHTML = '<span class="loading loading-spinner loading-xs mr-1"></span> Calculating quote...';
                    }
                    if (feedback) feedback.classList.add('hidden');

                    fetch('/api/bookings/' + bookingId + '/change-quote', {
                        method: 'POST',
                        headers: {
                            'Content-Type': 'application/json',
                            'Accept': 'application/json'
                        },
                        body: JSON.stringify(payload)
                    })
                    .then(function(res) {
                        return res.json().then(function(data) { return { ok: res.ok, data: data }; });
                    })
                    .then(function(res) {
                        if (!res.ok) {
                            if (preview) preview.classList.add('hidden');
                            if (feedback) {
                                feedback.className = 'text-[11px] text-error font-semibold';
                                feedback.innerText = (res.data && res.data.message) ? res.data.message : 'Unable to quote proposed changes.';
                                feedback.classList.remove('hidden');
                            }
                            return;
                        }
                        var q = res.data;
                        var bal = parseFloat(q.balance_due || '0');
                        var balBadge = bal > 0
                            ? '<span class="text-warning font-bold">Extra Charge: ' + q.currency + ' ' + q.balance_due + '</span>'
                            : (bal < 0
                                ? '<span class="text-success font-bold">Refund: ' + q.currency + ' ' + Math.abs(bal).toFixed(2) + '</span>'
                                : '<span class="text-base-content/70 font-semibold">No Price Change</span>');

                        if (preview) {
                            preview.innerHTML =
                                '<div class="flex justify-between font-bold border-b border-base-200 pb-1"><span>Quote (' + q.proposed_total_days + ' nights, ' + q.proposed_number_of_persons + ' guests)</span>' + balBadge + '</div>' +
                                '<div class="flex justify-between pt-0.5"><span>Proposed Total:</span><span class="font-mono font-bold">' + q.currency + ' ' + q.proposed_total_price + '</span></div>' +
                                '<div class="flex justify-between"><span>Host Payout Delta:</span><span class="font-mono font-bold text-primary">' + q.currency + ' ' + q.host_payout_delta + '</span></div>';
                        }
                    })
                    .catch(function(err) {
                        if (preview) preview.classList.add('hidden');
                    });
                }

                function submitAdminChangeRequest() {
                    var bookingId = activeDetailBookingId;
                    if (!bookingId) return;
                    var btn = document.getElementById('btn-admin-submit-change');
                    var feedback = document.getElementById('admin-change-feedback-msg');
                    var payload = gatherAdminChangePayload();

                    if (!payload.proposed_date_from || !payload.proposed_date_to) {
                        if (feedback) {
                            feedback.className = 'text-[11px] text-error font-semibold';
                            feedback.innerText = 'Please select valid check-in and check-out dates.';
                            feedback.classList.remove('hidden');
                        }
                        return;
                    }

                    if (btn) {
                        btn.disabled = true;
                        btn.innerHTML = '<span class="loading loading-spinner loading-xs mr-1"></span> Submitting...';
                    }
                    if (feedback) feedback.classList.add('hidden');

                    fetch('/api/bookings/' + bookingId + '/change-requests', {
                        method: 'POST',
                        headers: {
                            'Content-Type': 'application/json',
                            'Accept': 'application/json'
                        },
                        body: JSON.stringify(payload)
                    })
                    .then(function(res) {
                        return res.json().then(function(data) { return { ok: res.ok, data: data }; });
                    })
                    .then(function(res) {
                        if (btn) {
                            btn.disabled = false;
                            btn.innerText = 'Submit Modification';
                        }
                        if (!res.ok) {
                            if (feedback) {
                                feedback.className = 'text-[11px] text-error font-semibold';
                                feedback.innerText = (res.data && res.data.message) ? res.data.message : 'Failed to submit modification.';
                                feedback.classList.remove('hidden');
                            }
                            return;
                        }
                        if (feedback) {
                            feedback.className = 'text-[11px] text-success font-semibold';
                            feedback.innerText = payload.force_apply ? '✓ Modification applied immediately!' : '✓ Change proposal sent to guest for confirmation.';
                            feedback.classList.remove('hidden');
                        }
                        loadAdminChangeRequests(bookingId);
                        if (payload.force_apply) {
                            setTimeout(function() { window.location.reload(); }, 900);
                        }
                    })
                    .catch(function(err) {
                        if (btn) {
                            btn.disabled = false;
                            btn.innerText = 'Submit Modification';
                        }
                        if (feedback) {
                            feedback.className = 'text-[11px] text-error font-semibold';
                            feedback.innerText = 'Network error: ' + err.message;
                            feedback.classList.remove('hidden');
                        }
                    });
                }

                window.approveAdminBookingDirect = approveAdminBookingDirect;
                window.rejectAdminBookingDirect = rejectAdminBookingDirect;
                window.approveAdminBooking = approveAdminBooking;
                window.openAdminCancelDialog = openAdminCancelDialog;
                window.executeAdminCancellation = executeAdminCancellation;
                window.openAdminBookingDetailsFromBtn = openAdminBookingDetailsFromBtn;
                window.extendAdminHold = extendAdminHold;
                window.openAdminInvoiceDialog = openAdminInvoiceDialog;
                window.exportBookingsCsv = exportBookingsCsv;
                window.saveAdminDoorCode = saveAdminDoorCode;
                window.loadAdminChangeRequests = loadAdminChangeRequests;
                window.respondToAdminChangeRequest = respondToAdminChangeRequest;
                window.previewAdminChangeQuote = previewAdminChangeQuote;
                window.submitAdminChangeRequest = submitAdminChangeRequest;
                "#
            </script>
        </div>
        }
    })
}

#[route(PATCH "/api/bookings/{id}")]
pub async fn update_booking_admin_api(
    cx: &Cx,
    Json(payload): Json<UpdatedBookingRequest>,
) -> Result<Json<BookingResponse>> {
    let user = web_app_common_tc::auth::require_admin_auth(cx).await?;
    let id_str: &str = path_param::<Id>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;

    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let url = format!(
        "{}/api/v1/bookings/{}",
        common::app_client::booking_api_url(),
        id
    );
    let client = reqwest::Client::new();
    let res = client
        .patch(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&payload)
        .send()
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(web_app_common_tc::app_error(format!(
            "Failed to update booking ({}): {}",
            status, err_text
        ))
        .into());
    }

    let resp = res
        .json::<BookingResponse>()
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Parse error: {}", e)))?;
    Ok(Json(resp))
}

#[route(GET "/api/bookings/{id}/messages")]
pub async fn get_booking_messages_admin_api(cx: &Cx) -> Result<Json<BookingMessagesWrapper>> {
    let user = web_app_common_tc::auth::require_admin_auth(cx).await?;
    let id_str: &str = path_param::<Id>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;

    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let url = format!(
        "{}/api/v1/bookings/{}/messages",
        common::app_client::booking_api_url(),
        id
    );
    let client = reqwest::Client::new();
    let res = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;

    if !res.status().is_success() {
        return Err(web_app_common_tc::app_error(format!(
            "Failed to fetch messages: {}",
            res.status()
        ))
        .into());
    }

    let resp = res
        .json::<BookingMessagesWrapper>()
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Parse error: {}", e)))?;
    Ok(Json(resp))
}

#[route(POST "/api/bookings/{id}/messages")]
pub async fn send_booking_message_admin_api(
    cx: &Cx,
    Json(payload): Json<CreateBookingMessageRequest>,
) -> Result<Json<BookingMessageResponse>> {
    let user = web_app_common_tc::auth::require_admin_auth(cx).await?;
    let id_str: &str = path_param::<Id>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;

    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let url = format!(
        "{}/api/v1/bookings/{}/messages",
        common::app_client::booking_api_url(),
        id
    );
    let client = reqwest::Client::new();
    let res = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&payload)
        .send()
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;

    if !res.status().is_success() {
        return Err(web_app_common_tc::app_error(format!(
            "Failed to send message: {}",
            res.status()
        ))
        .into());
    }

    let resp = res
        .json::<BookingMessageResponse>()
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Parse error: {}", e)))?;
    Ok(Json(resp))
}

#[route(PATCH "/api/bookings/{id}/messages/read")]
pub async fn mark_messages_read_admin_api(cx: &Cx) -> Result<Json<MarkMessagesReadResponse>> {
    let user = web_app_common_tc::auth::require_admin_auth(cx).await?;
    let id_str: &str = path_param::<Id>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;

    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let url = format!(
        "{}/api/v1/bookings/{}/messages/read",
        common::app_client::booking_api_url(),
        id
    );
    let client = reqwest::Client::new();
    let res = client
        .patch(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;

    if !res.status().is_success() {
        return Err(
            web_app_common_tc::app_error(format!("Failed to mark read: {}", res.status())).into(),
        );
    }

    let resp = res
        .json::<MarkMessagesReadResponse>()
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Parse error: {}", e)))?;
    Ok(Json(resp))
}

#[route(GET "/api/bookings/{id}/documents")]
pub async fn get_booking_documents_admin_api(
    cx: &Cx,
) -> Result<Json<Vec<common::tax_document::TaxDocumentResponse>>> {
    let user = web_app_common_tc::auth::require_admin_auth(cx).await?;
    let id_str: &str = path_param::<Id>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;

    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let url = format!(
        "{}/api/v1/bookings/{}/documents",
        common::app_client::booking_api_url(),
        id
    );
    let client = reqwest::Client::new();
    let res = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(web_app_common_tc::app_error(format!(
            "Failed to fetch booking documents ({}): {}",
            status, err_text
        ))
        .into());
    }

    let docs = res
        .json::<Vec<common::tax_document::TaxDocumentResponse>>()
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Parse error: {}", e)))?;
    Ok(Json(docs))
}

#[route(POST "/api/bookings/{id}/change-quote")]
pub async fn quote_reservation_change_admin_api(
    cx: &Cx,
    Json(payload): Json<ReservationChangeQuoteRequest>,
) -> Result<Json<ReservationChangeQuoteResponse>> {
    let user = web_app_common_tc::auth::require_admin_auth(cx).await?;
    let id_str: &str = path_param::<Id>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let api = get_api_client(cx);
    let resp = api
        .quote_reservation_change(id, &payload, Some(&token))
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;
    Ok(Json(resp))
}

#[route(POST "/api/bookings/{id}/change-requests")]
pub async fn create_reservation_change_request_admin_api(
    cx: &Cx,
    Json(payload): Json<CreateReservationChangeRequest>,
) -> Result<Json<ReservationChangeRequestResponse>> {
    let user = web_app_common_tc::auth::require_admin_auth(cx).await?;
    let id_str: &str = path_param::<Id>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let api = get_api_client(cx);
    let resp = api
        .create_reservation_change_request(id, &payload, Some(&token))
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;
    Ok(Json(resp))
}

#[route(GET "/api/bookings/{id}/change-requests")]
pub async fn list_reservation_change_requests_admin_api(
    cx: &Cx,
) -> Result<Json<Vec<ReservationChangeRequestResponse>>> {
    let user = web_app_common_tc::auth::require_admin_auth(cx).await?;
    let id_str: &str = path_param::<Id>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let api = get_api_client(cx);
    let resp = api
        .list_reservation_change_requests(id, Some(&token))
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;
    Ok(Json(resp))
}

#[route(POST "/api/bookings/{id}/change-requests/{request_id}/approve")]
pub async fn approve_reservation_change_request_admin_api(
    cx: &Cx,
) -> Result<Json<ReservationChangeRequestResponse>> {
    let user = web_app_common_tc::auth::require_admin_auth(cx).await?;
    let id_str: &str = path_param::<Id>(cx);
    let req_str: &str = path_param::<RequestId>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let req_id =
        Uuid::parse_str(req_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let api = get_api_client(cx);
    let default_payload = RespondReservationChangeRequest::default();
    let resp = api
        .approve_reservation_change_request(id, req_id, &default_payload, Some(&token))
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;
    Ok(Json(resp))
}

#[route(POST "/api/bookings/{id}/change-requests/{request_id}/decline")]
pub async fn decline_reservation_change_request_admin_api(
    cx: &Cx,
    Json(payload): Json<RespondReservationChangeRequest>,
) -> Result<Json<ReservationChangeRequestResponse>> {
    let user = web_app_common_tc::auth::require_admin_auth(cx).await?;
    let id_str: &str = path_param::<Id>(cx);
    let req_str: &str = path_param::<RequestId>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let req_id =
        Uuid::parse_str(req_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let api = get_api_client(cx);
    let resp = api
        .decline_reservation_change_request(id, req_id, &payload, Some(&token))
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;
    Ok(Json(resp))
}

#[route(POST "/api/bookings/{id}/change-requests/{request_id}/cancel")]
pub async fn cancel_reservation_change_request_admin_api(
    cx: &Cx,
) -> Result<Json<ReservationChangeRequestResponse>> {
    let user = web_app_common_tc::auth::require_admin_auth(cx).await?;
    let id_str: &str = path_param::<Id>(cx);
    let req_str: &str = path_param::<RequestId>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let req_id =
        Uuid::parse_str(req_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let api = get_api_client(cx);
    let resp = api
        .cancel_reservation_change_request(id, req_id, Some(&token))
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;
    Ok(Json(resp))
}
