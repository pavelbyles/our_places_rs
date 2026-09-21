use common::payout::{PayoutFilter, PayoutLedgerEntry, PayoutStatus, UpdatePayoutStatusRequest};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Json, page, parse_query_params, path_param, route},
    runtime::shard,
    view::{View, view},
};
use uuid::Uuid;
use web_app_common_tc::{client::ListingSearchParams, get_api_client};

path_param!(id);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PayoutFilterQuery {
    pub listing_id: Option<String>,
    pub status: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub page: Option<u32>,
}

#[page("/admin/payouts")]
pub async fn admin_payouts_page(cx: &Cx) -> Result<impl View> {
    render_payouts_content(cx).await
}

#[page("/payouts")]
pub async fn payouts_alias_page(cx: &Cx) -> Result<impl View> {
    render_payouts_content(cx).await
}

async fn render_payouts_content(cx: &Cx) -> Result<impl View> {
    let __cx = cx;
    let is_authed = web_app_common_tc::auth::require_admin_auth(cx)
        .await
        .is_ok();
    let api = get_api_client(cx);

    let filter_query = parse_query_params::<PayoutFilterQuery>(cx).unwrap_or_default();

    let listing_id = filter_query
        .listing_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok());

    let status = filter_query
        .status
        .as_deref()
        .and_then(|s| match s.to_lowercase().as_str() {
            "pending" => Some(PayoutStatus::Pending),
            "processing" => Some(PayoutStatus::Processing),
            "paid" => Some(PayoutStatus::Paid),
            "cancelled" => Some(PayoutStatus::Cancelled),
            "refunded" => Some(PayoutStatus::Refunded),
            _ => None,
        });

    let date_from = filter_query
        .date_from
        .as_deref()
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    let date_to = filter_query
        .date_to
        .as_deref()
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    let page_num = filter_query.page.unwrap_or(1);
    let per_page = 50u32;

    let filter = PayoutFilter {
        listing_id,
        host_id: None,
        status,
        date_from,
        date_to,
        page: Some(page_num),
        per_page: Some(per_page),
    };

    // Fetch listings for filter dropdown
    let listings = api
        .search_listings(ListingSearchParams {
            per_page: Some(50),
            ..Default::default()
        })
        .await
        .unwrap_or_default();

    // Fetch financial summary metrics
    let summary = api
        .get_host_payout_summary(&filter)
        .await
        .unwrap_or_default();

    let selected_status_str = filter_query.status.clone().unwrap_or_default();
    let selected_listing_str = filter_query.listing_id.clone().unwrap_or_default();
    let selected_date_from_str = filter_query.date_from.clone().unwrap_or_default();
    let selected_date_to_str = filter_query.date_to.clone().unwrap_or_default();

    // Construct CSV export query
    let mut export_params = Vec::new();
    if let Some(lid) = listing_id {
        export_params.push(format!("listing_id={}", lid));
    }
    if let Some(st) = status {
        export_params.push(format!("status={}", st));
    }
    if let Some(df) = date_from {
        export_params.push(format!("date_from={}", df));
    }
    if let Some(dt) = date_to {
        export_params.push(format!("date_to={}", dt));
    }
    let export_query_str = if export_params.is_empty() {
        String::new()
    } else {
        format!("?{}", export_params.join("&"))
    };
    let export_csv_url = format!("/api/v1/hosts/ledger/export{}", export_query_str);

    let filter_listing_id = filter_query.listing_id.clone();
    let filter_status = filter_query.status.clone();
    let filter_date_from = filter_query.date_from.clone();
    let filter_date_to = filter_query.date_to.clone();
    let page_num_f64 = page_num as f64;

    Ok(view! {
        if !is_authed {
            <script>
                r#"window.location.replace('/login?redirect=' + encodeURIComponent(window.location.pathname + window.location.search));"#
            </script>
        } else {
            <div class="space-y-8 py-6 max-w-7xl mx-auto px-4 md:px-6">
                // Page Header
                <div class="flex flex-col md:flex-row md:items-center justify-between gap-4 border-b border-base-200 pb-6">
                    <div class="space-y-1">
                        <div class="flex items-center gap-2">
                            <span class="badge badge-primary badge-sm font-bold uppercase tracking-wider">
                                "Financial Engine"
                            </span>
                            <span class="text-xs text-base-content/50 font-mono">"Tri-Currency Ledger"</span>
                        </div>
                        <h1 class="text-3xl md:text-4xl font-serif font-bold tracking-tight text-base-content">
                            "Host Earnings & Payout Ledger"
                        </h1>
                        <p class="text-sm text-base-content/60">
                            "Double-entry ready financial ledger tracking gross booking revenue, platform commissions, statutory tax withholding, and host disbursements."
                        </p>
                    </div>

                    <div class="flex items-center gap-3 flex-wrap">
                        <a
                            href=(export_csv_url)
                            class="btn btn-outline btn-sm rounded-full px-4 font-semibold shadow-xs hover:bg-base-200 gap-2"
                            download="host_payout_ledger.csv"
                        >
                            <span>"📥"</span>
                            <span>"Export CSV"</span>
                        </a>
                        <button
                            class="btn btn-primary btn-sm rounded-full px-5 font-bold tracking-wide shadow-md gap-2"
                            onclick="window.location.reload()"
                        >
                            <span>"🔄"</span>
                            <span>"Refresh"</span>
                        </button>
                    </div>
                </div>

                // Financial KPI Metrics Summary
                <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-5">
                    // Card 1: Gross Rental Revenue
                    <div class="card bg-base-100 dark:bg-base-200/80 border border-base-300 dark:border-base-100/30 rounded-2xl p-5 shadow-sm hover:shadow-md transition-shadow">
                        <div class="flex items-center justify-between text-base-content/60 mb-2">
                            <span class="text-xs font-bold uppercase tracking-wider">"Gross Volume"</span>
                            <span class="p-2 rounded-xl bg-primary/10 text-primary text-sm">"💰"</span>
                        </div>
                        <div class="text-2xl lg:text-3xl font-bold font-mono text-base-content">
                            (format!("${:.2}", summary.total_gross))
                        </div>
                        <div class="text-xs text-base-content/50 mt-1 flex items-center justify-between">
                            <span>"Total bookings"</span>
                            <span class="font-bold font-mono">(summary.count_entries.to_string())</span>
                        </div>
                    </div>

                    // Card 2: Platform Commission
                    <div class="card bg-base-100 dark:bg-base-200/80 border border-base-300 dark:border-base-100/30 rounded-2xl p-5 shadow-sm hover:shadow-md transition-shadow">
                        <div class="flex items-center justify-between text-base-content/60 mb-2">
                            <span class="text-xs font-bold uppercase tracking-wider">"Platform Fee"</span>
                            <span class="p-2 rounded-xl bg-secondary/10 text-secondary text-sm">"📊"</span>
                        </div>
                        <div class="text-2xl lg:text-3xl font-bold font-mono text-secondary">
                            (format!("${:.2}", summary.total_platform_fee))
                        </div>
                        <div class="text-xs text-base-content/50 mt-1 flex items-center justify-between">
                            <span>"Commission retained"</span>
                            <span class="font-bold font-mono">"Platform Net"</span>
                        </div>
                    </div>

                    // Card 3: Net Host Earnings
                    <div class="card bg-base-100 dark:bg-base-200/80 border border-base-300 dark:border-base-100/30 rounded-2xl p-5 shadow-sm hover:shadow-md transition-shadow">
                        <div class="flex items-center justify-between text-base-content/60 mb-2">
                            <span class="text-xs font-bold uppercase tracking-wider">"Net Host Payouts"</span>
                            <span class="p-2 rounded-xl bg-success/10 text-success text-sm">"🌴"</span>
                        </div>
                        <div class="text-2xl lg:text-3xl font-bold font-mono text-success">
                            (format!("${:.2}", summary.total_net))
                        </div>
                        <div class="text-xs text-base-content/50 mt-1 flex items-center justify-between">
                            <span>"Settled paid"</span>
                            <span class="font-bold font-mono text-success">(format!("${:.2}", summary.total_paid))</span>
                        </div>
                    </div>

                    // Card 4: Pending / In Flight
                    <div class="card bg-base-100 dark:bg-base-200/80 border border-base-300 dark:border-base-100/30 rounded-2xl p-5 shadow-sm hover:shadow-md transition-shadow">
                        <div class="flex items-center justify-between text-base-content/60 mb-2">
                            <span class="text-xs font-bold uppercase tracking-wider">"Pending / Processing"</span>
                            <span class="p-2 rounded-xl bg-warning/10 text-warning text-sm">"⏳"</span>
                        </div>
                        <div class="text-2xl lg:text-3xl font-bold font-mono text-warning">
                            (format!("${:.2}", summary.total_pending + summary.total_processing))
                        </div>
                        <div class="text-xs text-base-content/50 mt-1 flex items-center justify-between">
                            <span>"Pending: "(format!("${:.2}", summary.total_pending))</span>
                            <span>"Proc: "(format!("${:.2}", summary.total_processing))</span>
                        </div>
                    </div>
                </div>

                // Filter & Search Bar
                <div class="card bg-base-100 dark:bg-base-200/50 border border-base-300 dark:border-base-100/30 rounded-2xl p-4 shadow-sm">
                    <form method="GET" action="/admin/payouts" class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-5 gap-3 items-end">
                        // Property selector
                        <div class="form-control w-full">
                            <label class="label pb-1">
                                <span class="label-text text-xs font-bold uppercase text-base-content/70">"Villa Property"</span>
                            </label>
                            <select name="listing_id" class="select select-bordered select-sm w-full rounded-xl">
                                <option value="" selected=(selected_listing_str.is_empty())>"All Properties"</option>
                                for l in &listings {
                                    let id_str = l.id.to_string();
                                    let is_selected = id_str == selected_listing_str;
                                    <option value=(id_str) selected=(is_selected)>
                                        (l.name.clone())
                                    </option>
                                }
                            </select>
                        </div>

                        // Status filter
                        <div class="form-control w-full">
                            <label class="label pb-1">
                                <span class="label-text text-xs font-bold uppercase text-base-content/70">"Payout Status"</span>
                            </label>
                            <select name="status" class="select select-bordered select-sm w-full rounded-xl">
                                <option value="" selected=(selected_status_str.is_empty())>"All Statuses"</option>
                                <option value="pending" selected=(selected_status_str == "pending")>"Pending"</option>
                                <option value="processing" selected=(selected_status_str == "processing")>"Processing"</option>
                                <option value="paid" selected=(selected_status_str == "paid")>"Paid"</option>
                                <option value="cancelled" selected=(selected_status_str == "cancelled")>"Cancelled"</option>
                                <option value="refunded" selected=(selected_status_str == "refunded")>"Refunded"</option>
                            </select>
                        </div>

                        // Date From
                        <div class="form-control w-full">
                            <label class="label pb-1">
                                <span class="label-text text-xs font-bold uppercase text-base-content/70">"From Date"</span>
                            </label>
                            <input
                                type="date"
                                name="date_from"
                                value=(selected_date_from_str)
                                class="input input-bordered input-sm w-full rounded-xl"
                            />
                        </div>

                        // Date To
                        <div class="form-control w-full">
                            <label class="label pb-1">
                                <span class="label-text text-xs font-bold uppercase text-base-content/70">"To Date"</span>
                            </label>
                            <input
                                type="date"
                                name="date_to"
                                value=(selected_date_to_str)
                                class="input input-bordered input-sm w-full rounded-xl"
                            />
                        </div>

                        // Action buttons
                        <div class="flex items-center gap-2">
                            <button type="submit" class="btn btn-primary btn-sm rounded-xl font-bold flex-1 shadow-sm">
                                "Filter"
                            </button>
                            <a href="/admin/payouts" class="btn btn-ghost btn-sm rounded-xl text-base-content/60">
                                "Reset"
                            </a>
                        </div>
                    </form>
                </div>

                // Payout Ledger Table Shard
                payouts_ledger_table(
                    listing_id: $(filter_listing_id),
                    status: $(filter_status),
                    date_from: $(filter_date_from),
                    date_to: $(filter_date_to),
                    page_num: $(page_num_f64),
                )

                // Modal for Managing Payout Status
                <dialog id="payout-status-modal" class="modal">
                    <div class="modal-box max-w-md rounded-2xl bg-base-100 border border-base-200 shadow-2xl space-y-4">
                        <div class="flex items-center justify-between border-b border-base-200 pb-3">
                            <h3 class="font-serif font-bold text-lg text-base-content">
                                "Update Payout Disbursement"
                            </h3>
                            <button class="btn btn-sm btn-circle btn-ghost" onclick="document.getElementById('payout-status-modal').close()">
                                "✕"
                            </button>
                        </div>

                        <form id="payout-status-form" onsubmit="handlePayoutStatusSubmit(event)" class="space-y-4">
                            <input type="hidden" id="modal-payout-id" name="id" />

                            <div class="form-control">
                                <label class="label">
                                    <span class="label-text text-xs font-bold uppercase text-base-content/70">"Target Status"</span>
                                </label>
                                <select id="modal-target-status" name="status" class="select select-bordered select-sm rounded-xl w-full">
                                    <option value="Processing">"Processing (In Transit to Gateway)"</option>
                                    <option value="Paid">"Paid (Settled / Remitted)"</option>
                                    <option value="Cancelled">"Cancelled (Booking Cancelled)"</option>
                                    <option value="Refunded">"Refunded (Dispute / Chargeback)"</option>
                                </select>
                            </div>

                            <div class="form-control">
                                <label class="label">
                                    <span class="label-text text-xs font-bold uppercase text-base-content/70">"Gateway / Bank Reference"</span>
                                </label>
                                <input
                                    type="text"
                                    id="modal-gateway-ref"
                                    name="gateway_reference"
                                    placeholder="e.g. MPGS_TRX_98241 or ACH_9238"
                                    class="input input-bordered input-sm rounded-xl w-full"
                                />
                            </div>

                            <div class="form-control">
                                <label class="label">
                                    <span class="label-text text-xs font-bold uppercase text-base-content/70">"Notes / Reason"</span>
                                </label>
                                <textarea
                                    id="modal-failure-reason"
                                    name="failure_reason"
                                    placeholder="Optional notes or cancellation reason..."
                                    class="textarea textarea-bordered textarea-sm rounded-xl w-full"
                                    rows="2"
                                ></textarea>
                            </div>

                            <div class="modal-action pt-2 flex items-center justify-end gap-2">
                                <button type="button" class="btn btn-ghost btn-sm rounded-xl" onclick="document.getElementById('payout-status-modal').close()">
                                    "Cancel"
                                </button>
                                <button type="submit" id="modal-submit-btn" class="btn btn-primary btn-sm rounded-xl font-bold px-5 shadow-sm">
                                    "Confirm Transition"
                                </button>
                            </div>
                        </form>
                    </div>
                </dialog>

                // Script for managing modal interactions and sending status updates
                <script>
                    r#"
                    function openPayoutModal(btn) {
                        var id = btn.getAttribute('data-id');
                        var status = btn.getAttribute('data-status');
                        var gateway = btn.getAttribute('data-gateway') || '';
                        
                        document.getElementById('modal-payout-id').value = id;
                        document.getElementById('modal-gateway-ref').value = gateway;
                        document.getElementById('modal-failure-reason').value = '';
                        
                        var selectEl = document.getElementById('modal-target-status');
                        if (status === 'Pending') {
                            selectEl.value = 'Processing';
                        } else if (status === 'Processing') {
                            selectEl.value = 'Paid';
                        }
                        
                        document.getElementById('payout-status-modal').showModal();
                    }

                    async function handlePayoutStatusSubmit(e) {
                        e.preventDefault();
                        var id = document.getElementById('modal-payout-id').value;
                        var status = document.getElementById('modal-target-status').value;
                        var gatewayRef = document.getElementById('modal-gateway-ref').value;
                        var failureReason = document.getElementById('modal-failure-reason').value;
                        var submitBtn = document.getElementById('modal-submit-btn');

                        submitBtn.disabled = true;
                        submitBtn.textContent = 'Updating...';

                        try {
                            var resp = await fetch('/api/payouts/' + id + '/status', {
                                method: 'PATCH',
                                headers: { 'Content-Type': 'application/json' },
                                body: JSON.stringify({
                                    status: status,
                                    gateway_reference: gatewayRef || null,
                                    failure_reason: failureReason || null
                                })
                            });

                            if (!resp.ok) {
                                var errData = await resp.text();
                                alert('Failed to update payout status: ' + errData);
                            } else {
                                window.location.reload();
                            }
                        } catch (err) {
                            alert('Network error updating payout status: ' + err);
                        } finally {
                            submitBtn.disabled = false;
                            submitBtn.textContent = 'Confirm Transition';
                            document.getElementById('payout-status-modal').close();
                        }
                    }
                    "#
                </script>
            </div>
        }
    })
}

#[shard]
pub async fn payouts_ledger_table(
    cx: &Cx,
    listing_id: Option<String>,
    status: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    page_num: f64,
) -> Result<impl View> {
    let __cx = cx;
    let api = get_api_client(cx);
    let per_page = 50u32;
    let page_u32 = if page_num >= 1.0 {
        page_num as u32
    } else {
        1u32
    };

    let listing_uuid = listing_id.as_deref().and_then(|s| Uuid::parse_str(s).ok());

    let status_enum = status
        .as_deref()
        .and_then(|s| match s.to_lowercase().as_str() {
            "pending" => Some(PayoutStatus::Pending),
            "processing" => Some(PayoutStatus::Processing),
            "paid" => Some(PayoutStatus::Paid),
            "cancelled" => Some(PayoutStatus::Cancelled),
            "refunded" => Some(PayoutStatus::Refunded),
            _ => None,
        });

    let date_from_parsed = date_from
        .as_deref()
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    let date_to_parsed = date_to
        .as_deref()
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    let filter = PayoutFilter {
        listing_id: listing_uuid,
        host_id: None,
        status: status_enum,
        date_from: date_from_parsed,
        date_to: date_to_parsed,
        page: Some(page_u32),
        per_page: Some(per_page),
    };

    let ledger_resp =
        api.get_host_payout_ledger(&filter)
            .await
            .unwrap_or(common::payout::PayoutLedgerResponse {
                entries: Vec::new(),
                total_count: 0,
                page: page_u32,
                per_page,
            });

    let total_pages = if ledger_resp.total_count == 0 {
        1u32
    } else {
        ((ledger_resp.total_count as f64) / (per_page as f64)).ceil() as u32
    };

    Ok(view! {
        <div id="payouts-ledger-table-shard" class="card bg-base-100 dark:bg-base-200 rounded-2xl border border-base-300 dark:border-base-100/20 shadow-sm overflow-hidden">
            if ledger_resp.entries.is_empty() {
                <div class="p-12 text-center space-y-3">
                    <div class="text-4xl">"📑"</div>
                    <div class="text-lg font-serif font-bold text-base-content">
                        "No Payout Ledger Records Found"
                    </div>
                    <p class="text-sm text-base-content/60 max-w-md mx-auto">
                        "No host payout entries match the selected filters. Confirmed bookings will automatically create pending ledger entries."
                    </p>
                </div>
            } else {
                <div class="overflow-x-auto">
                    <table class="table table-zebra w-full text-xs">
                        <thead>
                            <tr class="text-base-content/60 uppercase tracking-wider text-[11px] bg-base-200/50">
                                <th>"Booking / Host"</th>
                                <th>"Villa Listing"</th>
                                <th>"Stay Dates"</th>
                                <th class="text-right">"Gross Amount"</th>
                                <th class="text-right">"Platform Fee"</th>
                                <th class="text-right">"Net Payout"</th>
                                <th>"Status"</th>
                                <th>"Settlement"</th>
                                <th class="text-right">"Actions"</th>
                            </tr>
                        </thead>
                        <tbody>
                            for entry in &ledger_resp.entries {
                                let entry_id = entry.id.to_string();
                                let status_badge_class = match entry.status {
                                    PayoutStatus::Pending => "badge badge-warning badge-sm font-semibold",
                                    PayoutStatus::Processing => "badge badge-info badge-sm font-semibold",
                                    PayoutStatus::Paid => "badge badge-success badge-sm font-semibold",
                                    PayoutStatus::Cancelled => "badge badge-error badge-sm font-semibold",
                                    PayoutStatus::Refunded => "badge badge-secondary badge-sm font-semibold",
                                };
                                let stay_dates = match (entry.check_in_date, entry.check_out_date) {
                                    (Some(ci), Some(co)) => format!("{} → {}", ci.format("%b %d"), co.format("%b %d, %Y")),
                                    _ => "N/A".to_string(),
                                };
                                let booking_code = entry.booking_confirmation_code.clone().unwrap_or_else(|| {
                                    format!("{:.8}...", entry.booking_id.to_string())
                                });
                                let listing_title = entry.listing_name.clone().unwrap_or_else(|| "Luxury Villa".to_string());
                                let host_display = entry.host_name.clone().unwrap_or_else(|| "Villa Host".to_string());
                                let fee_pct_display = format!("{:.1}%", entry.platform_fee_pct * Decimal::from(100));

                                <tr id=(format!("payout-row-{}", entry.id)) class="hover:bg-base-200/60 transition-colors">
                                    // Booking / Host
                                    <td>
                                        <div class="font-bold text-sm font-mono text-base-content">
                                            (booking_code)
                                        </div>
                                        <div class="text-[11px] text-base-content/60">
                                            (host_display)
                                        </div>
                                    </td>

                                    // Villa
                                    <td>
                                        <div class="font-medium text-sm text-base-content max-w-[200px] truncate">
                                            (listing_title)
                                        </div>
                                        <div class="text-[10px] text-base-content/50 font-mono">
                                            (format!("{:.8}...", entry.listing_id.to_string()))
                                        </div>
                                    </td>

                                    // Stay Dates
                                    <td class="font-mono text-[11px] text-base-content/80 whitespace-nowrap">
                                        (stay_dates)
                                    </td>

                                    // Gross Amount
                                    <td class="text-right font-mono font-bold text-sm text-base-content">
                                        (format!("{:.2} {}", entry.gross_amount, entry.currency))
                                    </td>

                                    // Platform Fee
                                    <td class="text-right font-mono text-base-content/70">
                                        <div class="font-semibold text-secondary">
                                            (format!("-{:.2}", entry.platform_fee_amount))
                                        </div>
                                        <div class="text-[10px] text-base-content/50">
                                            (fee_pct_display)
                                        </div>
                                    </td>

                                    // Net Payout
                                    <td class="text-right font-mono font-bold text-sm text-success">
                                        (format!("{:.2} {}", entry.net_payout_amount, entry.currency))
                                    </td>

                                    // Status
                                    <td>
                                        <span class=(status_badge_class)>
                                            (entry.status.to_string())
                                        </span>
                                        if let Some(ref ref_code) = entry.gateway_reference {
                                            <div class="text-[10px] font-mono text-base-content/50 truncate max-w-[120px]" title=(ref_code.clone())>
                                                (ref_code.clone())
                                            </div>
                                        }
                                    </td>

                                    // Settlement Date
                                    <td class="text-xs text-base-content/60 whitespace-nowrap">
                                        (entry.payout_date.map(|dt| dt.format("%b %d, %Y").to_string()).unwrap_or_else(|| "Pending".to_string()))
                                    </td>

                                    // Action
                                    <td class="text-right">
                                        <button
                                            class="btn btn-ghost btn-xs text-primary font-bold hover:bg-primary/10 rounded-lg"
                                            data-id=(entry_id.clone())
                                            data-status=(entry.status.to_string())
                                            data-gateway=(entry.gateway_reference.clone().unwrap_or_default())
                                            onclick="openPayoutModal(this)"
                                        >
                                            "Manage ▾"
                                        </button>
                                    </td>
                                </tr>
                            }
                        </tbody>
                    </table>
                </div>

                // Pagination Footer
                if total_pages > 1 {
                    <div class="flex items-center justify-between px-6 py-4 border-t border-base-200">
                        <div class="text-xs text-base-content/60">
                            (format!("Showing page {} of {}", page_u32, total_pages))
                        </div>
                        <div class="join">
                            if page_u32 > 1 {
                                <a
                                    href=(format!("/admin/payouts?page={}", page_u32 - 1))
                                    class="join-item btn btn-sm btn-outline rounded-l-xl"
                                >
                                    "« Prev"
                                </a>
                            }
                            <button class="join-item btn btn-sm btn-active pointer-events-none">
                                (page_u32.to_string())
                            </button>
                            if page_u32 < total_pages {
                                <a
                                    href=(format!("/admin/payouts?page={}", page_u32 + 1))
                                    class="join-item btn btn-sm btn-outline rounded-r-xl"
                                >
                                    "Next »"
                                </a>
                            }
                        </div>
                    </div>
                }
            }
        </div>
    })
}

#[route(PATCH "/api/payouts/{id}/status")]
pub async fn update_payout_status_api(
    cx: &Cx,
    Json(payload): Json<UpdatePayoutStatusRequest>,
) -> Result<Json<PayoutLedgerEntry>> {
    web_app_common_tc::auth::require_admin_auth(cx).await?;
    let id_str: &str = path_param::<Id>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| anyhow::anyhow!("Invalid UUID"))?;
    let api = get_api_client(cx);
    let resp = api
        .update_admin_payout_status(id, &payload)
        .await
        .map_err(|e| anyhow::anyhow!("Server error: {}", e))?;
    Ok(Json(resp))
}
