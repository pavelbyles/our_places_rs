//! Display-currency switcher. The offered currencies come from the database
//! (the cached active list). Choosing one goes through `/currency/{code}`,
//! which remembers it in the [`CURRENCY_COOKIE`] and returns to the page, so
//! every later page shows prices in it too.

use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

/// The cookie holding the guest's chosen display currency (a code such as
/// `CAD`). Read on every page; a `?currency=` in the URL takes precedence.
pub const CURRENCY_COOKIE: &str = "op_currency";

/// The search values a currency link keeps. Nothing else in the URL carries over.
const KEPT_SEARCH_PARAMS: [&str; 5] = ["q", "host", "check_in", "check_out", "guests"];

/// The search in a query string, as `key=value` pairs still encoded as they
/// arrived, so nothing needs decoding and encoding again. Empty values are left out.
pub fn search_params_from_query(query: &str) -> Vec<String> {
    query
        .split('&')
        .filter(|pair| {
            pair.split_once('=')
                .is_some_and(|(key, value)| !value.is_empty() && KEPT_SEARCH_PARAMS.contains(&key))
        })
        .map(str::to_string)
        .collect()
}

/// Where a currency link returns to: `path` with the search from `query`
/// (anything else, including an old `currency=`, is dropped so the new
/// choice applies).
pub fn return_to(path: &str, query: &str) -> String {
    let kept = search_params_from_query(query);
    if kept.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{}", kept.join("&"))
    }
}

/// The link that makes `currency` the display currency, then returns to
/// `return_to` (a path from [`return_to`]).
pub fn currency_href(currency: &str, return_to: &str) -> String {
    let next: String = form_urlencoded::byte_serialize(return_to.as_bytes()).collect();
    format!("/currency/{currency}?next={next}")
}

/// A pill showing the active display currency that opens a menu of the
/// active currencies. Hidden when no currency list is available.
#[component]
pub async fn currency_selector(
    /// Request context.
    cx: &topcoat::context::Cx,
    /// Active currency, if one was chosen.
    #[default]
    current_currency: Option<String>,
    /// The page each link returns to, from [`return_to`]; the home page if empty.
    #[default]
    return_to: String,
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    attrs: Attributes,
) -> Result<impl View> {
    let codes = active_currency_codes(cx).await;
    Ok(view! {
        currency_menu(
            codes: codes,
            current_currency: current_currency,
            return_to: return_to,
            attrs: attrs
        )
    })
}

/// The menu itself, rendered from a list of codes (no I/O; used by tests).
/// No choice shows USD (prices in their base currency); a code that is not
/// offered (e.g. deactivated) prompts the guest to choose.
#[component]
pub async fn currency_menu(
    cx: &Cx,
    /// Codes to offer, in menu order.
    codes: Vec<String>,
    /// Active currency, if one was chosen.
    #[default]
    current_currency: Option<String>,
    /// The page each link returns to, from [`return_to`]; the home page if empty.
    #[default]
    return_to: String,
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
    let return_to = if return_to.is_empty() {
        "/".to_string()
    } else {
        return_to
    };
    Ok(view! {
        if !hidden {
            <details
                data-dismiss-outside=""
                class=(class!("relative", attrs.remove("class")))
                (attrs)
            >
                <summary
                    aria-label=(label)
                    class="flex h-11 cursor-pointer list-none items-center rounded-full border border-border px-3 text-sm font-medium sm:px-4"
                >
                    (summary)
                </summary>
                <ul
                    class="absolute right-0 z-50 mt-2 w-28 rounded-xl border border-border bg-popover p-1 text-popover-foreground shadow-sm"
                >
                    for curr in codes {
                        <li>
                            <a
                                href=(currency_href(&curr, &return_to))
                                aria-current=((Some(&curr) == active.as_ref()).then_some(
                                    "true",
                                ))
                                class="block rounded-lg px-3 py-2 text-sm aria-[current=true]:font-semibold"
                            >
                                (curr.clone())
                            </a>
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

#[cfg(test)]
mod currency_menu_search_tests {
    use super::*;
    use topcoat::context::Cx;
    use topcoat::view::ViewExt;

    #[test]
    fn keeps_the_search_and_drops_everything_else() {
        assert_eq!(
            search_params_from_query(
                "q=Runaway%20Bay&currency=USD&check_in=2030-08-01&check_out=2030-08-04&guests=2&host=sea&new_booking=true&page=3"
            ),
            [
                "q=Runaway%20Bay",
                "check_in=2030-08-01",
                "check_out=2030-08-04",
                "guests=2",
                "host=sea"
            ]
        );
    }

    #[test]
    fn empty_values_and_an_empty_query_keep_nothing() {
        assert!(search_params_from_query("").is_empty());
        assert!(search_params_from_query("q=&check_in=&guests").is_empty());
    }

    #[test]
    fn hostile_values_stay_inside_next_and_add_no_parameter() {
        for value in ["a%26currency%3DXXX%23", "a%2Bb", "a%3Db", "a%22b", "a%3Cb"] {
            let back = return_to("/listings", &format!("q={value}&currency=USD"));
            assert_eq!(back, format!("/listings?q={value}"));
            let href = currency_href("JMD", &back);
            assert!(
                href.starts_with("/currency/JMD?next=%2Flistings%3Fq%3D"),
                "{href}"
            );
            assert_eq!(href.matches('&').count(), 0, "{href}");
            assert_eq!(href.matches('?').count(), 1, "{href}");
        }
    }

    #[test]
    fn the_old_currency_is_dropped_so_the_new_choice_applies() {
        assert_eq!(
            return_to(
                "/listings/villa",
                "currency=XXX&check_in=2030-08-01&currency=YYY"
            ),
            "/listings/villa?check_in=2030-08-01"
        );
    }

    #[test]
    fn with_no_search_links_return_to_the_path() {
        assert_eq!(return_to("/", ""), "/");
        assert_eq!(currency_href("JMD", "/"), "/currency/JMD?next=%2F");
    }

    #[tokio::test]
    async fn the_menu_links_return_to_the_page_and_its_search() {
        let cx = &Cx::default();
        let back = return_to("/listings", "q=x&check_in=2030-08-01&check_out=2030-08-04");
        let codes = vec!["JMD".to_string()];
        let out = match view! { currency_menu(codes: codes, return_to: back) }
            .first()
            .await
        {
            Ok(handle) => handle.render(cx),
            Err(e) => panic!("render failed: {e}"),
        };
        assert!(
            out.contains(
                r#"href="/currency/JMD?next=%2Flistings%3Fq%3Dx%26check_in%3D2030-08-01%26check_out%3D2030-08-04""#
            ),
            "{out}"
        );
    }
}
