//! Itemised stay price. Amounts are formatted exactly as the legacy portals
//! format them (`"{currency} {amount:.2}"`), and the `breakdown-*` element IDs
//! the e2e suite relies on are kept.

use rust_decimal::Decimal;
use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

/// The figures shown in a [`price_breakdown`], all in `currency`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceBreakdown {
    /// Number of nights.
    pub nights: i64,
    /// Nightly rate after seasonal pricing.
    pub effective_nightly_rate: Decimal,
    /// Nights × rate.
    pub subtotal: Decimal,
    /// Length-of-stay discount, if any.
    pub discount_amount: Option<Decimal>,
    /// Cleaning fee, if any.
    pub cleaning_fee: Option<Decimal>,
    /// Refundable damage deposit, if any.
    pub damage_deposit: Option<Decimal>,
    /// Jamaican GCT.
    pub tax_amount: Decimal,
    /// Amount due.
    pub total_amount: Decimal,
    /// ISO currency code, e.g. `USD`.
    pub currency: String,
}

/// Formats an amount the way every portal shows money: `"USD 1234.50"`.
pub fn money(currency: &str, amount: Decimal) -> String {
    format!("{currency} {amount:.2}")
}

fn positive(amount: Option<Decimal>) -> Option<Decimal> {
    amount.filter(|a| *a > Decimal::ZERO)
}

/// The price breakdown panel for checkout and the listing quote.
#[component]
pub async fn price_breakdown(
    /// The figures to show.
    quote: PriceBreakdown,
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    let cur = quote.currency.as_str();
    let nights_label = format!(
        "{} night(s) × {cur} {:.2}",
        quote.nights, quote.effective_nightly_rate
    );
    let subtotal = money(cur, quote.subtotal);
    let discount = positive(quote.discount_amount).map(|d| format!("-{cur} {d:.2}"));
    let cleaning = positive(quote.cleaning_fee).map(|c| money(cur, c));
    let tax = money(cur, quote.tax_amount);
    let deposit = positive(quote.damage_deposit).map(|d| money(cur, d));
    let total = money(cur, quote.total_amount);
    let row = "flex items-center justify-between";
    Ok(view! {
        <section
            aria-labelledby="price-breakdown-heading"
            class=(class!("space-y-3 rounded-[var(--radius-surface,16px)] border border-border bg-card p-4 text-card-foreground", attrs.remove("class")))
            (attrs)
        >
            <h3 id="price-breakdown-heading" class="text-xs font-semibold tracking-wide text-muted-foreground uppercase">"Price Breakdown"</h3>
            <dl class="space-y-1.5 text-sm">
                <div class=(row)>
                    <dt id="breakdown-nights-label">(nights_label)</dt>
                    <dd id="breakdown-subtotal" class="font-medium tabular-nums">(subtotal)</dd>
                </div>
                if let Some(discount) = discount {
                    <div class=(row)>
                        <dt>"Discount applied"</dt>
                        <dd class="font-medium tabular-nums">(discount)</dd>
                    </div>
                }
                if let Some(cleaning) = cleaning {
                    <div class=(row)>
                        <dt>"Cleaning Fee"</dt>
                        <dd id="breakdown-cleaning-fee" class="font-medium tabular-nums">(cleaning)</dd>
                    </div>
                }
                <div class=(row)>
                    <dt>"Jamaican GCT (15%)"</dt>
                    <dd id="breakdown-tax" class="font-medium tabular-nums">(tax)</dd>
                </div>
                if let Some(deposit) = deposit {
                    <div class=(row)>
                        <dt>"Refundable Damage Deposit"</dt>
                        <dd id="breakdown-damage-deposit" class="font-medium tabular-nums">(deposit)</dd>
                    </div>
                }
            </dl>
            <div class="flex items-center justify-between border-t border-border pt-3 font-semibold">
                <span>"Total Due"</span>
                <span id="breakdown-total" class="text-lg tabular-nums">(total)</span>
            </div>
        </section>
    })
}
