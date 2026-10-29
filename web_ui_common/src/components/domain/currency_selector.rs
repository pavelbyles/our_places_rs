//! Display-currency switcher (tri-currency display: USD base plus JMD, EUR,
//! GBP and CAD). Selecting a currency reloads the page with `?currency=`.

use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

/// Currencies offered, in menu order.
pub const DISPLAY_CURRENCIES: [&str; 5] = ["USD", "JMD", "EUR", "GBP", "CAD"];

/// A pill showing the active currency that opens a menu of the others.
/// Closes on outside click / Escape via `ui.js` (`data-dismiss-outside`).
#[component]
pub async fn currency_selector(
    /// Active currency; defaults to USD.
    #[default]
    current_currency: Option<String>,
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    let active = current_currency
        .map(|c| c.to_uppercase())
        .filter(|c| DISPLAY_CURRENCIES.contains(&c.as_str()))
        .unwrap_or_else(|| "USD".to_string());
    Ok(view! {
        <details data-dismiss-outside="" class=(class!("relative", attrs.remove("class"))) (attrs)>
            <summary
                aria-label=(format!("Display currency: {active}"))
                class="flex h-11 cursor-pointer list-none items-center rounded-full border border-border px-3 text-sm font-medium sm:px-4"
            >(active.clone())</summary>
            <ul class="absolute right-0 z-50 mt-2 w-28 rounded-xl border border-border bg-popover p-1 text-popover-foreground shadow-sm">
                for curr in DISPLAY_CURRENCIES {
                    <li>
                        <a
                            href=(format!("?currency={curr}"))
                            aria-current=((curr == active).then_some("true"))
                            class="block rounded-lg px-3 py-2 text-sm aria-[current=true]:font-semibold"
                        >(curr)</a>
                    </li>
                }
            </ul>
        </details>
    })
}
