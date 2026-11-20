//! Itemised stay price. Line items show the amount with the currency's symbol
//! and thousands separators (`$11,240.00`); only the total names the currency
//! (`USD $14,133.00`). The `breakdown-*` element IDs the e2e suite relies on
//! are kept.

use common::models::StayQuoteResponse;
use rust_decimal::Decimal;
use topcoat::context::Cx;
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
    /// The promo code applied, if any.
    pub promo_code: Option<String>,
    /// The whole promo discount, if any.
    pub promo_discount: Option<Decimal>,
    /// Cleaning fee before any promo, if any.
    pub cleaning_fee: Option<Decimal>,
    /// Refundable damage deposit, if any.
    pub damage_deposit: Option<Decimal>,
    /// Guest platform fee, if any.
    pub platform_fee: Option<Decimal>,
    /// Jamaican GCT.
    pub tax_amount: Decimal,
    /// Amount due.
    pub total_amount: Decimal,
    /// ISO currency code, e.g. `USD`.
    pub currency: String,
}

/// The figures for the breakdown panel. `None` for amounts that are zero, so
/// their rows don't show.
pub fn breakdown(quote: &StayQuoteResponse) -> PriceBreakdown {
    let positive = |amount: Decimal| Some(amount).filter(|a| *a > Decimal::ZERO);
    PriceBreakdown {
        nights: i64::from(quote.nights),
        effective_nightly_rate: quote.effective_nightly_rate,
        subtotal: quote.subtotal,
        discount_amount: positive(quote.stay_discount),
        promo_code: quote.promo.as_ref().map(|promo| promo.code.clone()),
        promo_discount: quote.promo.as_ref().map(|promo| promo.discount),
        cleaning_fee: positive(quote.cleaning_fee),
        damage_deposit: positive(quote.damage_deposit),
        platform_fee: positive(quote.platform_fee),
        tax_amount: quote.tax_amount,
        total_amount: quote.total_amount,
        currency: quote.currency.clone(),
    }
}

/// Why a stay has no price.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoQuote {
    /// `booking_api` refused the stay as asked; the message tells the guest
    /// why and what to change, e.g. "These dates need a stay of at least 5
    /// nights."
    Refused(String),
    /// The quote failed for another reason, such as the API being down.
    Failed,
}

impl NoQuote {
    /// What the guest is told. Only a failure says to try again: retrying
    /// never fixes a refusal.
    pub fn message(&self) -> String {
        match self {
            Self::Refused(message) => message.clone(),
            Self::Failed => {
                "We couldn't price this stay right now. Please try again shortly.".to_string()
            }
        }
    }

    /// Sorts a failed quote by its error text, logging it: a refusal is the
    /// guest's input, a failure is ours.
    pub fn from_error(e: &dyn std::fmt::Display, listing_id: &dyn std::fmt::Display) -> Self {
        let text = e.to_string();
        match common::availability::stay_refusal_message_in_error(&text) {
            Some(message) => {
                tracing::info!(%listing_id, "stay not quoted: {text}");
                Self::Refused(message)
            }
            None => {
                tracing::warn!(%listing_id, "no stay quote: {text}");
                Self::Failed
            }
        }
    }
}

impl From<&common::booking_request::PriceBreakdown> for PriceBreakdown {
    /// A request's or quote's price; zero amounts are left off.
    fn from(price: &common::booking_request::PriceBreakdown) -> Self {
        let some = |amount: Decimal| (amount > Decimal::ZERO).then_some(amount);
        Self {
            nights: i64::from(price.nights),
            effective_nightly_rate: price.nightly_rate,
            subtotal: price.subtotal,
            discount_amount: some(price.stay_discount),
            promo_code: None,
            promo_discount: some(price.promo_discount),
            cleaning_fee: some(price.cleaning_fee),
            damage_deposit: some(price.damage_deposit),
            platform_fee: some(price.platform_fee),
            tax_amount: price.tax,
            total_amount: price.total,
            currency: price.currency.clone(),
        }
    }
}

pub use common::money::{currency_symbol, grouped, item_amount, total_amount};

/// Formats a standalone amount the way every portal shows money:
/// `"USD $1,234.50"`. Line items inside a breakdown use [`item_amount`].
pub fn money(currency: &str, amount: Decimal) -> String {
    total_amount(currency, amount)
}

fn positive(amount: Option<Decimal>) -> Option<Decimal> {
    amount.filter(|a| *a > Decimal::ZERO)
}

/// The price breakdown panel for checkout and the listing quote.
#[component]
pub async fn price_breakdown(
    cx: &Cx,
    /// The figures to show.
    quote: PriceBreakdown,
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    let cur = quote.currency.as_str();
    let item = |amount: Decimal| item_amount(cur, amount);
    let nights_label = format!(
        "{} × {} {}",
        item(quote.effective_nightly_rate),
        quote.nights,
        if quote.nights == 1 { "night" } else { "nights" }
    );
    let subtotal = item(quote.subtotal);
    let discount = positive(quote.discount_amount).map(|d| format!("−{}", item(d)));
    let promo = positive(quote.promo_discount).map(|d| {
        let code = quote.promo_code.clone().unwrap_or_default();
        (format!("Promo code {code}"), format!("−{}", item(d)))
    });
    let platform_fee = positive(quote.platform_fee).map(item);
    let cleaning = positive(quote.cleaning_fee).map(item);
    let tax = item(quote.tax_amount);
    let deposit = positive(quote.damage_deposit).map(item);
    let total = total_amount(cur, quote.total_amount);
    let row = "flex items-baseline justify-between gap-4";
    Ok(view! {
        <section
            aria-labelledby="price-breakdown-heading"
            class=(class!(
                "space-y-3 rounded-[var(--radius-surface,16px)] border border-border bg-card p-4 text-card-foreground",
                attrs.remove("class"),
            ))
            (attrs)
        >
            <h3
                id="price-breakdown-heading"
                class="text-xs font-semibold tracking-wide text-muted-foreground uppercase"
            >
                "Price Breakdown"
            </h3>
            <dl class="space-y-1.5 text-sm">
                <div class=(row)>
                    <dt id="breakdown-nights-label">(nights_label)</dt>
                    <dd
                        id="breakdown-subtotal"
                        class="shrink-0 text-right font-medium whitespace-nowrap tabular-nums"
                    >
                        (subtotal)
                    </dd>
                </div>
                if let Some(discount) = discount {
                    <div class=(row)>
                        <dt>"Discount applied"</dt>
                        <dd
                            class="shrink-0 text-right font-medium whitespace-nowrap tabular-nums"
                        >
                            (discount)
                        </dd>
                    </div>
                }
                if let Some((label, amount)) = promo {
                    <div class=(row)>
                        <dt>(label)</dt>
                        <dd
                            id="breakdown-promo"
                            class="shrink-0 text-right font-medium whitespace-nowrap tabular-nums"
                        >
                            (amount)
                        </dd>
                    </div>
                }
                if let Some(cleaning) = cleaning {
                    <div class=(row)>
                        <dt>"Cleaning Fee"</dt>
                        <dd
                            id="breakdown-cleaning-fee"
                            class="shrink-0 text-right font-medium whitespace-nowrap tabular-nums"
                        >
                            (cleaning)
                        </dd>
                    </div>
                }
                <div class=(row)>
                    <dt>"Jamaican GCT (15%)"</dt>
                    <dd
                        id="breakdown-tax"
                        class="shrink-0 text-right font-medium whitespace-nowrap tabular-nums"
                    >
                        (tax)
                    </dd>
                </div>
                if let Some(platform_fee) = platform_fee {
                    <div class=(row)>
                        <dt>"Platform Fee"</dt>
                        <dd
                            id="breakdown-platform-fee"
                            class="shrink-0 text-right font-medium whitespace-nowrap tabular-nums"
                        >
                            (platform_fee)
                        </dd>
                    </div>
                }
                if let Some(deposit) = deposit {
                    <div class=(row)>
                        <dt>"Refundable Damage Deposit"</dt>
                        <dd
                            id="breakdown-damage-deposit"
                            class="shrink-0 text-right font-medium whitespace-nowrap tabular-nums"
                        >
                            (deposit)
                        </dd>
                    </div>
                }
            </dl>
            <div
                class="flex items-baseline justify-between gap-4 border-t border-border pt-3 font-semibold"
            >
                <span>"Total Due"</span>
                <span
                    id="breakdown-total"
                    class="shrink-0 text-right text-lg whitespace-nowrap tabular-nums"
                >
                    (total)
                </span>
            </div>
        </section>
    })
}
