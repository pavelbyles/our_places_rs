use topcoat::{
    Result,
    router::page,
    view::view,
};

#[page("/")]
pub async fn dashboard() -> Result {
    view! {
        <div class="space-y-6">
            <div class="flex items-center justify-between border-b border-base-300 pb-4">
                <div>
                    <h1 class="text-3xl font-bold tracking-tight">"Admin Dashboard"</h1>
                    <p class="text-base-content/70">"Manage villas, reservations, pricing, and system health."</p>
                </div>
                <div class="badge badge-primary badge-lg">"Topcoat SSR + HTMX 4"</div>
            </div>

            <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
                <div class="stat bg-base-200 rounded-box shadow">
                    <div class="stat-title">"Total Properties"</div>
                    <div class="stat-value text-primary">"5"</div>
                    <div class="stat-desc">"Jamaica Luxury Portfolio"</div>
                </div>

                <div class="stat bg-base-200 rounded-box shadow">
                    <div class="stat-title">"Active Bookings"</div>
                    <div class="stat-value text-secondary">"12"</div>
                    <div class="stat-desc">"Current Season"</div>
                </div>

                <div class="stat bg-base-200 rounded-box shadow">
                    <div class="stat-title">"System Status"</div>
                    <div class="stat-value text-success text-2xl">"Operational"</div>
                    <div class="stat-desc">"All APIs Connected"</div>
                </div>
            </div>

            <div class="card bg-base-200 shadow-xl">
                <div class="card-body">
                    <h2 class="card-title">"Quick Actions (HTMX Fragment)"</h2>
                    <p class="text-sm text-base-content/70">"Test dynamic administrative stats loading:"</p>
                    <div class="card-actions justify-start mt-2">
                        <button
                            class="btn btn-secondary btn-sm"
                            hx-get="/admin/htmx/stats"
                            hx-target="#admin-stats-container"
                            hx-swap="innerHTML"
                        >
                            "Refresh Live Metrics (HTMX 4)"
                        </button>
                    </div>
                    <div id="admin-stats-container" class="mt-4">
                        <div class="text-xs text-base-content/50 italic">"Click refresh to load live server telemetry."</div>
                    </div>
                </div>
            </div>
        </div>
    }
}

#[page("/admin/htmx/stats")]
pub async fn admin_htmx_stats() -> Result {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();
    view! {
        <div class="alert alert-info">
            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" class="stroke-current shrink-0 w-6 h-6">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path>
            </svg>
            <div>
                <h3 class="font-bold">"Telemetry Refreshed"</h3>
                <div class="text-xs">"Timestamp: " (now) " | Scale-to-Zero Cloud Run Ready"</div>
            </div>
        </div>
    }
}
