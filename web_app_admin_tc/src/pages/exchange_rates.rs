use topcoat::{Result, context::Cx, router::page, view::view};

#[page("/admin/exchange-rates")]
pub async fn admin_exchange_rates_page(cx: &Cx) -> Result {
    if let Err(_err) = web_app_common_tc::auth::require_admin_auth(cx).await {
        return view! {
            <script>
                r#"window.location.replace('/login?redirect=' + encodeURIComponent(window.location.pathname + window.location.search));"#
            </script>
        };
    }

    view! {

        <div class="space-y-8 py-6 max-w-5xl mx-auto px-4 md:px-6">
            // Header
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 border-b border-base-200 pb-4">
                <div class="space-y-1">
                    <span class="text-primary font-bold tracking-widest uppercase text-xs">"Tri-Currency Financial Engine"</span>
                    <h1 class="text-3xl font-serif font-bold tracking-tight text-base-content">
                        "Exchange Rates Synchronization"
                    </h1>
                    <p class="text-xs text-base-content/60">
                        "Manage statutory base currency conversions between USD, JMD, EUR, GBP, and CAD with rust_decimal precision."
                    </p>
                </div>
                <button class="btn btn-primary btn-sm rounded-full px-5 font-bold tracking-wide shadow-md">
                    "🔄 Sync from Fixer API"
                </button>
            </div>

            // Exchange Rates Table & Override Grid
            <div class="grid grid-cols-1 lg:grid-cols-3 gap-8">
                // Left 2 cols: Rates table
                <div class="lg:col-span-2 space-y-4">
                    <div class="bg-base-100 dark:bg-base-200 rounded-2xl border border-base-200 dark:border-base-100/20 shadow-md overflow-hidden">
                        <div class="overflow-x-auto">
                            <table class="table table-zebra w-full">
                                <thead>
                                    <tr class="text-xs text-base-content/60 uppercase tracking-wider">
                                        <th>"Currency Pair"</th>
                                        <th>"Multiplier Rate"</th>
                                        <th>"Inverse (1 Unit = USD)"</th>
                                        <th>"Last Synced"</th>
                                        <th class="text-right">"Action"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    <tr>
                                        <td>
                                            <div class="font-bold text-sm text-base-content">"USD / JMD"</div>
                                            <div class="text-[11px] text-base-content/50">"Jamaican Dollar (Primary Settlement)"</div>
                                        </td>
                                        <td class="font-mono font-bold text-primary text-sm">"158.4200"</td>
                                        <td class="text-xs font-mono text-base-content/70">"$0.00631"</td>
                                        <td class="text-xs text-base-content/50">"10 mins ago"</td>
                                        <td class="text-right">
                                            <button class="btn btn-ghost btn-xs text-primary font-bold">"Adjust"</button>
                                        </td>
                                    </tr>

                                    <tr>
                                        <td>
                                            <div class="font-bold text-sm text-base-content">"USD / EUR"</div>
                                            <div class="text-[11px] text-base-content/50">"Euro (European Guest Checkout)"</div>
                                        </td>
                                        <td class="font-mono font-bold text-primary text-sm">"0.9245"</td>
                                        <td class="text-xs font-mono text-base-content/70">"$1.0817"</td>
                                        <td class="text-xs text-base-content/50">"10 mins ago"</td>
                                        <td class="text-right">
                                            <button class="btn btn-ghost btn-xs text-primary font-bold">"Adjust"</button>
                                        </td>
                                    </tr>

                                    <tr>
                                        <td>
                                            <div class="font-bold text-sm text-base-content">"USD / GBP"</div>
                                            <div class="text-[11px] text-base-content/50">"British Pound Sterling"</div>
                                        </td>
                                        <td class="font-mono font-bold text-primary text-sm">"0.7890"</td>
                                        <td class="text-xs font-mono text-base-content/70">"$1.2674"</td>
                                        <td class="text-xs text-base-content/50">"10 mins ago"</td>
                                        <td class="text-right">
                                            <button class="btn btn-ghost btn-xs text-primary font-bold">"Adjust"</button>
                                        </td>
                                    </tr>

                                    <tr>
                                        <td>
                                            <div class="font-bold text-sm text-base-content">"USD / CAD"</div>
                                            <div class="text-[11px] text-base-content/50">"Canadian Dollar"</div>
                                        </td>
                                        <td class="font-mono font-bold text-primary text-sm">"1.3680"</td>
                                        <td class="text-xs font-mono text-base-content/70">"$0.7309"</td>
                                        <td class="text-xs text-base-content/50">"10 mins ago"</td>
                                        <td class="text-right">
                                            <button class="btn btn-ghost btn-xs text-primary font-bold">"Adjust"</button>
                                        </td>
                                    </tr>
                                </tbody>
                            </table>
                        </div>
                    </div>
                </div>

                // Right col: Manual Override Box
                <div class="bg-base-100 dark:bg-base-200/80 p-6 rounded-3xl border border-base-200 dark:border-base-100/20 shadow-lg space-y-4">
                    <div class="border-b border-base-200 pb-3">
                        <h2 class="font-serif font-bold text-base text-base-content">
                            "Manual Exchange Rate Buffer"
                        </h2>
                        <p class="text-xs text-base-content/60">
                            "Apply statutory volatility margins or fixed rate pegs."
                        </p>
                    </div>

                    <form class="space-y-4" action="/admin/exchange-rates" method="POST">
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1">
                                "Target Currency"
                            </label>
                            <select name="currency" class="select select-bordered select-sm w-full rounded-xl font-medium">
                                <option value="JMD">"JMD - Jamaican Dollar"</option>
                                <option value="EUR">"EUR - Euro"</option>
                                <option value="GBP">"GBP - British Pound"</option>
                                <option value="CAD">"CAD - Canadian Dollar"</option>
                            </select>
                        </div>

                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1">
                                "Manual Rate Multiplier"
                            </label>
                            <input type="number" name="rate" step="0.0001" value="158.42" class="input input-bordered input-sm w-full rounded-xl font-mono font-bold" />
                        </div>

                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1">
                                "Safety Spread Margin %"
                            </label>
                            <input type="number" name="margin" step="0.1" value="1.5" class="input input-bordered input-sm w-full rounded-xl" />
                        </div>

                        <button type="submit" class="btn btn-primary w-full rounded-full font-bold uppercase tracking-wider text-xs shadow-md">
                            "Update Rate Peg"
                        </button>
                    </form>
                </div>
            </div>
        </div>
    }
}
