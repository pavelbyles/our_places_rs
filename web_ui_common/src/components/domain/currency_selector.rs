//! Display-currency switcher. The offered currencies come from the database
//! (the cached active list); selecting one reloads the page with `?currency=`.

use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

/// A pill showing the active display currency that opens a menu of the
/// active currencies. Hidden when no currency list is available.
#[component]
pub async fn currency_selector(
    /// Request context.
    cx: &topcoat::context::Cx,
    /// Active currency, if one was chosen.
    #[default]
    current_currency: Option<String>,
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    attrs: Attributes,
) -> Result<impl View> {
    let codes = active_currency_codes(cx).await;
    Ok(view! { currency_menu(codes: codes, current_currency: current_currency, attrs: attrs) })
}

/// The menu itself, rendered from a list of codes (no I/O; used by tests).
/// No choice shows USD (prices in their base currency); a code that is not
/// offered (e.g. deactivated) prompts the guest to choose.
#[component]
pub async fn currency_menu(
    /// Codes to offer, in menu order.
    codes: Vec<String>,
    /// Active currency, if one was chosen.
    #[default]
    current_currency: Option<String>,
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    let requested = current_currency.map(|c| c.to_uppercase());
    let active = match &requested {
        None => Some("USD".to_string()),
        Some(c) => codes.iter().find(|code| *code == c).cloned(),
    };
    let summary = active
        .clone()
        .unwrap_or_else(|| "Choose a currency".to_string());
    let label = active
        .as_ref()
        .map(|a| format!("Display currency: {a}"))
        .unwrap_or_else(|| "Choose a display currency".to_string());
    let hidden = codes.is_empty();
    Ok(view! {
        if !hidden {
            <details data-dismiss-outside="" class=(class!("relative", attrs.remove("class"))) (attrs)>
                <summary
                    aria-label=(label)
                    class="flex h-11 cursor-pointer list-none items-center rounded-full border border-border px-3 text-sm font-medium sm:px-4"
                >(summary)</summary>
                <ul class="absolute right-0 z-50 mt-2 w-28 rounded-xl border border-border bg-popover p-1 text-popover-foreground shadow-sm">
                    for curr in codes {
                        <li>
                            <a
                                href=(format!("?currency={curr}"))
                                aria-current=((Some(&curr) == active.as_ref()).then_some("true"))
                                class="block rounded-lg px-3 py-2 text-sm aria-[current=true]:font-semibold"
                            >(curr.clone())</a>
                        </li>
                    }
                </ul>
            </details>
        }
    })
}

/// Codes of the active currencies, from the portal's cache.
pub async fn active_currency_codes(cx: &topcoat::context::Cx) -> Vec<String> {
    active_currency_list(cx)
        .await
        .into_iter()
        .map(|c| c.code.to_string())
        .collect()
}

/// One `<option>` of a currency `<select>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrencyOption {
    /// Submitted value (the code).
    pub value: String,
    /// Visible text.
    pub label: String,
    /// Whether this option is selected.
    pub selected: bool,
}

/// Options for a currency `<select>`: every active currency, plus `current`
/// when it is no longer active (shown as "CODE (inactive)" and kept selected
/// so saving an unrelated change does not silently switch currency).
pub fn currency_options(
    active: &[common::currency::Currency],
    current: &str,
) -> Vec<CurrencyOption> {
    let listed = active.iter().any(|c| c.code.as_str() == current);
    let retired = (!listed && !current.is_empty()).then(|| CurrencyOption {
        value: current.to_string(),
        // With no list at all (backend unreachable) we cannot tell it is inactive.
        label: if active.is_empty() {
            current.to_string()
        } else {
            format!("{current} (inactive)")
        },
        selected: true,
    });
    active
        .iter()
        .map(|c| CurrencyOption {
            value: c.code.to_string(),
            label: c.code.to_string(),
            selected: c.code.as_str() == current,
        })
        .chain(retired)
        .collect()
}

/// The active currencies from the portal's cache. A failure with nothing
/// cached is logged and yields an empty list (display only, never money).
pub async fn active_currency_list(cx: &topcoat::context::Cx) -> Vec<common::currency::Currency> {
    web_client::get_api_client(cx)
        .active_currencies()
        .await
        .map(|list| list.as_ref().clone())
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "active currency list unavailable");
            Vec::new()
        })
}

/// `<option>` elements for a currency `<select>`, from the database.
#[component]
pub async fn currency_option_list(
    /// Request context.
    cx: &topcoat::context::Cx,
    /// The currently stored value.
    current: String,
) -> Result<impl View> {
    let options = currency_options(&active_currency_list(cx).await, &current);
    Ok(view! {
        for o in options {
            <option value=(o.value) selected=(o.selected)>(o.label)</option>
        }
    })
}

#[cfg(test)]
mod currency_select_tests {
    use super::*;
    use common::currency::{Currency, CurrencyCode};

    fn active(codes: &[&str]) -> Vec<Currency> {
        codes
            .iter()
            .map(|c| Currency {
                code: CurrencyCode::try_from(*c).expect("code"),
                name: (*c).to_string(),
                active: true,
            })
            .collect()
    }

    #[test]
    fn lists_active_currencies_and_selects_current() {
        let opts = currency_options(&active(&["JMD", "USD"]), "USD");
        let values: Vec<&str> = opts.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(values, vec!["JMD", "USD"]);
        assert!(opts.iter().any(|o| o.value == "USD" && o.selected));
        assert_eq!(opts.iter().filter(|o| o.selected).count(), 1);
    }

    #[test]
    fn keeps_an_inactive_current_value_selected_and_marked() {
        let opts = currency_options(&active(&["JMD", "USD"]), "CAD");
        let cad = opts.iter().find(|o| o.value == "CAD").expect("CAD kept");
        assert!(cad.selected);
        assert_eq!(cad.label, "CAD (inactive)");
    }

    #[test]
    fn empty_current_selects_nothing_and_adds_nothing() {
        let opts = currency_options(&active(&["USD"]), "");
        assert_eq!(opts.len(), 1);
        assert!(!opts[0].selected);
    }

    #[test]
    fn empty_active_list_still_shows_the_stored_value() {
        let opts = currency_options(&[], "USD");
        assert_eq!(opts.len(), 1);
        assert!(opts[0].selected);
        assert_eq!(
            opts[0].label, "USD",
            "unknown status is not labelled inactive"
        );
    }
}
