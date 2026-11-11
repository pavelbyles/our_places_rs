//! Choosing the display currency: remembered in a cookie (and a signed-in
//! guest's profile) so every page after this one shows prices in it.

use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{error::see_other, page, parse_query_params, path_param},
};
use web_client::session::require_guest;
use web_ui_common::components::domain::currency_selector::active_currency_codes;

use crate::support::{remember_currency, save_profile_currency};

path_param!(code);

#[derive(Deserialize, Default)]
struct Next {
    next: Option<String>,
}

/// `next` if it is a path on this site, else the home page, so the link
/// cannot send guests to another site.
pub fn local_path(next: Option<&str>) -> &str {
    next.filter(|n| {
        n.starts_with('/')
            && !n.starts_with("//")
            && !n.starts_with("/\\")
            && !n.chars().any(char::is_control)
    })
    .unwrap_or("/")
}

/// `code` upper-cased, if it is a well-formed code that is offered (any
/// well-formed code when the offered list could not be loaded; the API then
/// validates it).
pub fn chosen_code(code: &str, offered: &[String]) -> Option<String> {
    let code = code.trim().to_uppercase();
    let well_formed = code.len() == 3 && code.chars().all(|c| c.is_ascii_uppercase());
    (well_formed && (offered.is_empty() || offered.contains(&code))).then_some(code)
}

/// Remembers the display currency (and saves it to a signed-in guest's
/// profile), then returns to `?next=`.
#[page("/currency/{code}")]
pub async fn choose_currency(cx: &Cx) -> Result<()> {
    let requested: &str = code(cx);
    let next = parse_query_params::<Next>(cx).unwrap_or_default().next;
    let offered = active_currency_codes(cx).await;
    match chosen_code(requested, &offered) {
        Some(code) => {
            remember_currency(cx, &code);
            if let Ok(session) = require_guest(cx).await {
                save_profile_currency(cx, session.user_id, &code).await;
            }
        }
        None => {
            tracing::warn!(requested, "display currency not offered; choice ignored");
        }
    }
    Err(see_other(local_path(next.as_deref())).into())
}

#[cfg(test)]
mod tests {
    use super::{chosen_code, local_path};

    #[test]
    fn only_paths_on_this_site_are_returned_to() {
        assert_eq!(
            local_path(Some("/listings/villa?check_in=2030-01-01")),
            "/listings/villa?check_in=2030-01-01"
        );
        assert_eq!(local_path(Some("/")), "/");
        for unsafe_next in [
            "https://evil.example",
            "//evil.example",
            "/\\evil.example",
            "evil",
            "/a\nb",
            "",
        ] {
            assert_eq!(local_path(Some(unsafe_next)), "/", "{unsafe_next:?}");
        }
        assert_eq!(local_path(None), "/");
    }

    #[test]
    fn only_offered_well_formed_codes_are_remembered() {
        let offered = vec!["CAD".to_string(), "USD".to_string()];
        assert_eq!(chosen_code("cad", &offered), Some("CAD".to_string()));
        assert_eq!(chosen_code("EUR", &offered), None);
        assert_eq!(chosen_code("CA", &[]), None);
        assert_eq!(chosen_code("C$D", &[]), None);
        assert_eq!(
            chosen_code("EUR", &[]),
            Some("EUR".to_string()),
            "list unavailable"
        );
    }
}
