use rust_decimal::Decimal;
use topcoat::{
    Result,
    view::{component, view},
};

#[allow(clippy::too_many_arguments)]
#[component]
pub async fn price_breakdown(
    nights: i64,
    effective_nightly_rate: Decimal,
    subtotal: Decimal,
    discount_amount: Option<Decimal>,
    tax_amount: Decimal,
    total_amount: Decimal,
    currency: String,
) -> Result {
    let curr = currency.clone();
    let nights_label = format!("{nights} night(s) × {curr} {effective_nightly_rate:.2}");
    let subtotal_str = format!("{curr} {subtotal:.2}");
    let tax_label = "Jamaican GCT (15%)";
    let tax_str = format!("{curr} {tax_amount:.2}");
    let total_str = format!("{curr} {total_amount:.2}");

    view! {
        <div class="card bg-base-200 border border-base-300 rounded-box p-4 space-y-3">
            <h3 class="font-bold text-sm text-base-content/80 uppercase tracking-wider">"Price Breakdown"</h3>
            <div class="space-y-1.5 text-sm">
                <div class="flex justify-between items-center text-base-content/80">
                    <span id="breakdown-nights-label">(nights_label)</span>
                    <span class="font-medium" id="breakdown-subtotal">(subtotal_str)</span>
                </div>
                if let Some(discount) = discount_amount {
                    if discount > Decimal::ZERO {
                        <div class="flex justify-between items-center text-success font-medium">
                            <span>"Discount applied"</span>
                            <span>(format!("-{} {:.2}", curr, discount))</span>
                        </div>
                    }
                }
                <div class="flex justify-between items-center text-base-content/80">
                    <span class="flex items-center gap-1">
                        (tax_label)
                        <span class="tooltip" data-tip="Statutory Jamaican General Consumption Tax">
                            <svg xmlns="http://www.w3.org/2000/svg" class="w-3.5 h-3.5 text-base-content/50" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                            </svg>
                        </span>
                    </span>
                    <span class="font-medium" id="breakdown-tax">(tax_str)</span>
                </div>
            </div>
            <div class="divider my-1"></div>
            <div class="flex justify-between items-center text-base font-bold text-primary">
                <span>"Total Due"</span>
                <span class="text-lg font-black" id="breakdown-total">(total_str)</span>
            </div>
        </div>
    }
}
