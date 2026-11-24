//! Currency administration card on the exchange-rates page (#100). Admins
//! add, rename, activate and deactivate currencies; hosts never see it.
//! Actions are HTMX posts that swap the card; without HTMX they redirect back.

use common::currency::{
    CreateCurrencyRequest, Currency, CurrencyCode, ListingRef, UpdateCurrencyRequest,
};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, error::see_other, page, path_param},
    view::{View, component, view},
};
use web_client::client::UserCallError;
use web_client::get_api_client;

use crate::support::{admin_only, service_token};

path_param!(code);

/// Where non-HTMX submissions return to.
const BACK: &str = "/admin/exchange-rates";

/// What the card shows besides the list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CardState {
    /// Messages to show above the list.
    pub errors: Vec<String>,
    /// Listings that block a deactivation (shows the blocked dialog).
    pub blocked: Vec<ListingRef>,
}

/// User-facing text and blocking listings for a failed call.
pub fn describe(err: &UserCallError) -> CardState {
    match err {
        UserCallError::Api(r) if r.code == "CURRENCY_IN_USE" => CardState {
            errors: vec![
                "Listings are still priced in this currency. Change them, then try again."
                    .to_string(),
            ],
            blocked: serde_json::from_value(r.body["listings"].clone()).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "CURRENCY_IN_USE body had no readable listings");
                Vec::new()
            }),
        },
        UserCallError::Api(r) => CardState {
            errors: vec![r.message.clone()],
            blocked: Vec::new(),
        },
        UserCallError::Unauthorized => CardState {
            errors: vec!["Only administrators can change currencies.".to_string()],
            blocked: Vec::new(),
        },
        other => {
            tracing::warn!(error = %other, "currency change failed");
            CardState {
                errors: vec!["The change could not be saved. Try again shortly.".to_string()],
                blocked: Vec::new(),
            }
        }
    }
}

/// The card: every currency (inactive included), forms, and the blocked dialog.
#[component]
pub async fn currencies_card(
    cx: &Cx,
    /// All currencies, sorted by code.
    currencies: Vec<Currency>,
    /// Errors and blocking listings to show.
    state: CardState,
) -> Result<impl View> {
    let field = "h-9 rounded-lg border border-border bg-card px-2 text-sm text-foreground";
    let button = "h-9 rounded-lg border border-border px-3 text-sm font-medium";
    Ok(view! {
        <section
            id="currencies-card"
            class="space-y-4 rounded-xl border border-border p-6"
        >
            <div>
                <h2 class="text-lg font-semibold">"Currencies"</h2>
                <p class="text-sm text-muted-foreground">
                    "Currencies are never deleted. Deactivate one to stop offering it; existing bookings keep it."
                </p>
            </div>
            for message in state.errors {
                <p
                    role="alert"
                    class="rounded-lg border border-destructive p-3 text-sm text-destructive"
                >
                    (message)
                </p>
            }
            <table id="currencies-table" class="w-full text-sm">
                <thead class="text-left text-xs text-muted-foreground uppercase">
                    <tr>
                        <th scope="col" class="py-2">"Code"</th>
                        <th scope="col" class="py-2">"Name"</th>
                        <th scope="col" class="py-2">"Status"</th>
                        <th scope="col" class="py-2">
                            <span class="sr-only">"Actions"</span>
                        </th>
                    </tr>
                </thead>
                <tbody class="divide-y divide-border">
                    for c in currencies {
                        <tr data-currency=(c.code.to_string())>
                            <td class="py-2 font-mono">(c.code.to_string())</td>
                            <td class="py-2">
                                <form
                                    method="post"
                                    action=(format!("/admin/currencies/{}", c.code))
                                    hx-post=(format!("/admin/currencies/{}", c.code))
                                    hx-target="#currencies-card"
                                    hx-swap="outerHTML"
                                    hx-disabled-elt="find button"
                                    class="flex gap-2"
                                >
                                    <input
                                        name="name"
                                        required=(true)
                                        maxlength="64"
                                        value=(c.name.clone())
                                        aria-label=(format!("Name of {}", c.code))
                                        class=(field)
                                    />
                                    <button type="submit" class=(button)>"Rename"</button>
                                </form>
                            </td>
                            <td class="py-2">
                                if c.active {
                                    <span class="badge badge-success">"Active"</span>
                                } else {
                                    <span class="badge badge-ghost">"Inactive"</span>
                                }
                            </td>
                            <td class="py-2 text-right">
                                <form
                                    method="post"
                                    action=(format!("/admin/currencies/{}", c.code))
                                    hx-post=(format!("/admin/currencies/{}", c.code))
                                    hx-target="#currencies-card"
                                    hx-swap="outerHTML"
                                    hx-disabled-elt="find button"
                                >
                                    <input
                                        type="hidden"
                                        name="active"
                                        value=(if c.active { "false" } else { "true" })
                                    />
                                    <button type="submit" class=(button)>
                                        (if c.active { "Deactivate" } else { "Activate" })
                                    </button>
                                </form>
                            </td>
                        </tr>
                    }
                </tbody>
            </table>
            <form
                id="add-currency-form"
                method="post"
                action="/admin/currencies"
                hx-post="/admin/currencies"
                hx-target="#currencies-card"
                hx-swap="outerHTML"
                hx-disabled-elt="find button"
                class="flex flex-wrap items-end gap-2"
            >
                <label class="text-xs text-muted-foreground">
                    "Code"
                    <input
                        name="code"
                        required=(true)
                        pattern="[A-Z]{3}"
                        maxlength="3"
                        placeholder="TTD"
                        class=(format!("{field} mt-1 block w-24 font-mono uppercase"))
                    />
                </label>
                <label class="text-xs text-muted-foreground">
                    "Name"
                    <input
                        name="name"
                        required=(true)
                        maxlength="64"
                        placeholder="Trinidad and Tobago Dollar"
                        class=(format!("{field} mt-1 block"))
                    />
                </label>
                <button
                    type="submit"
                    class="h-9 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground"
                >
                    "Add currency"
                </button>
            </form>
            blocked_listings_dialog(listings: state.blocked)
        </section>
    })
}

/// A dialog listing live listings that block a currency change (opens on
/// render; nothing when empty).
#[component]
pub async fn blocked_listings_dialog(
    cx: &Cx,
    /// The blocking listings.
    listings: Vec<ListingRef>,
) -> Result<impl View> {
    Ok(view! {
        if !listings.is_empty() {
            <dialog
                id="currency-blocked-dialog"
                data-auto-open=""
                aria-labelledby="currency-blocked-title"
                class="m-auto w-[min(560px,95vw)] rounded-xl bg-card p-6 text-card-foreground backdrop:bg-black/50"
            >
                <h3 id="currency-blocked-title" class="text-lg font-semibold">
                    "Still in use"
                </h3>
                <p class="mt-1 text-sm text-muted-foreground">
                    "These listings depend on this currency or pair. Change their base currency first."
                </p>
                <ul class="mt-4 space-y-2">
                    for l in listings {
                        <li>
                            <a
                                href=(format!("/admin/listings/{}/edit", l.id))
                                class="underline"
                            >
                                (l.title)
                            </a>
                        </li>
                    }
                </ul>
                <button
                    type="button"
                    data-dialog-close=""
                    class="mt-4 h-9 rounded-lg border border-border px-3 text-sm font-medium"
                >
                    "Close"
                </button>
            </dialog>
        }
    })
}

/// The card with the current list, after an optional failed change.
#[component]
pub async fn currencies_section(
    /// Request context.
    cx: &Cx,
    /// Service token for the signed-in admin.
    token: String,
    /// Errors and blocking listings from the last action.
    #[default]
    state: CardState,
) -> Result<impl View> {
    let mut state = state;
    let currencies = get_api_client(cx)
        .list_all_currencies_as(&token)
        .await
        .map(|r| r.currencies)
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "currency list unavailable");
            state
                .errors
                .push("Currencies could not be loaded. Try again shortly.".to_string());
            Vec::new()
        });
    Ok(view! { currencies_card(currencies: currencies, state: state) })
}

/// Without HTMX, actions redirect back to the page.
fn redirect_back(outcome: &CardState) -> topcoat::Error {
    let notice = if outcome.errors.is_empty() {
        "saved"
    } else {
        "rejected"
    };
    see_other(format!("{BACK}?currency_notice={notice}")).into()
}

/// The add-currency form.
#[derive(Debug, Deserialize)]
pub struct AddCurrencyForm {
    /// New code.
    #[serde(default)]
    pub code: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
}

/// Adds a currency (admins only).
#[page(POST "/admin/currencies")]
pub async fn add_currency(
    cx: &Cx,
    /// The submitted form.
    Form(form): Form<AddCurrencyForm>,
) -> Result<impl View> {
    let admin = admin_only(cx).await?;
    let token = service_token(cx, &admin)?;
    let outcome = match CurrencyCode::try_from(form.code.trim()) {
        Err(_) => CardState {
            errors: vec!["Codes are three capital letters, e.g. TTD.".to_string()],
            blocked: Vec::new(),
        },
        Ok(code) => {
            let req = CreateCurrencyRequest {
                code,
                name: form.name,
            };
            match get_api_client(cx).create_currency_as(&token, &req).await {
                Ok(created) => {
                    tracing::info!(code = %created.code, admin_id = %admin.user_id, "currency added");
                    CardState::default()
                }
                Err(e) => describe(&e),
            }
        }
    };
    if !topcoat::htmx::hx_request(cx) {
        return Err(redirect_back(&outcome));
    }
    Ok(view! { currencies_section(token: token, state: outcome) })
}

/// The rename / (de)activate forms.
#[derive(Debug, Deserialize)]
pub struct UpdateCurrencyForm {
    /// New display name, when renaming.
    #[serde(default)]
    pub name: Option<String>,
    /// `"true"` or `"false"`, when toggling.
    #[serde(default)]
    pub active: Option<String>,
}

/// Renames or (de)activates a currency (admins only).
#[page(POST "/admin/currencies/{code}")]
pub async fn change_currency(
    cx: &Cx,
    /// The submitted form.
    Form(form): Form<UpdateCurrencyForm>,
) -> Result<impl View> {
    let admin = admin_only(cx).await?;
    let token = service_token(cx, &admin)?;
    let outcome = match CurrencyCode::try_from(code(cx)) {
        Err(_) => CardState {
            errors: vec!["Unknown currency.".to_string()],
            blocked: Vec::new(),
        },
        Ok(code) => {
            let req = UpdateCurrencyRequest {
                name: form.name.filter(|n| !n.trim().is_empty()),
                active: form.active.as_deref().map(|a| a == "true"),
            };
            match get_api_client(cx)
                .update_currency_as(&token, &code, &req)
                .await
            {
                Ok(updated) => {
                    tracing::info!(code = %updated.code, active = updated.active, admin_id = %admin.user_id, "currency changed");
                    CardState::default()
                }
                Err(e) => describe(&e),
            }
        }
    };
    if !topcoat::htmx::hx_request(cx) {
        return Err(redirect_back(&outcome));
    }
    Ok(view! { currencies_section(token: token, state: outcome) })
}

#[cfg(test)]
mod currencies_card_tests {
    use super::*;
    use topcoat::view::ViewExt;

    async fn html(v: impl View, cx: &Cx) -> String {
        match v.first().await {
            Ok(handle) => handle.render(cx),
            Err(e) => panic!("render failed: {e}"),
        }
    }

    fn currency(code: &str, active: bool) -> Currency {
        Currency {
            code: CurrencyCode::try_from(code).expect("code"),
            name: format!("{code} name"),
            active,
        }
    }

    #[tokio::test]
    async fn lists_currencies_with_status_and_toggle() {
        let cx = &Cx::default();
        let list = vec![currency("CAD", false), currency("USD", true)];
        let out = html(
            view! { currencies_card(currencies: list, state: CardState::default()) },
            cx,
        )
        .await;
        assert!(out.contains(r#"data-currency="CAD""#), "{out}");
        assert!(
            out.contains("Inactive") && out.contains("Activate"),
            "{out}"
        );
        assert!(out.contains("Deactivate"), "{out}");
        assert!(out.contains(r#"hx-post="/admin/currencies""#), "{out}");
        assert!(!out.contains("currency-blocked-dialog"), "{out}");
    }

    #[tokio::test]
    async fn blocked_dialog_lists_listings_with_edit_links() {
        let cx = &Cx::default();
        let state = CardState {
            errors: vec!["Listings are still priced in this currency.".into()],
            blocked: vec![ListingRef {
                id: uuid::Uuid::nil(),
                title: "Villa Azul".into(),
                slug: "villa-azul".into(),
            }],
        };
        let list = vec![currency("USD", true)];
        let out = html(
            view! { currencies_card(currencies: list, state: state) },
            cx,
        )
        .await;
        assert!(out.contains("currency-blocked-dialog"), "{out}");
        assert!(
            out.contains(&format!(
                r#"href="/admin/listings/{}/edit""#,
                uuid::Uuid::nil()
            )),
            "{out}"
        );
        assert!(out.contains("Villa Azul"), "{out}");
    }

    #[tokio::test]
    async fn listing_titles_are_escaped() {
        let cx = &Cx::default();
        let state = CardState {
            errors: Vec::new(),
            blocked: vec![ListingRef {
                id: uuid::Uuid::nil(),
                title: "<script>alert(1)</script>".into(),
                slug: "x".into(),
            }],
        };
        let out = html(
            view! { currencies_card(currencies: Vec::new(), state: state) },
            cx,
        )
        .await;
        assert!(!out.contains("<script>alert(1)</script>"), "{out}");
    }

    #[test]
    fn in_use_rejection_extracts_blocking_listings() {
        let err = UserCallError::Api(web_client::client::ApiRejection {
            status: 409,
            code: "CURRENCY_IN_USE".into(),
            message: "in use".into(),
            body: serde_json::json!({
                "code": "CURRENCY_IN_USE",
                "listings": [{ "id": uuid::Uuid::nil(), "title": "Villa Azul", "slug": "villa-azul" }]
            }),
        });
        let state = describe(&err);
        assert_eq!(state.blocked.len(), 1);
        assert_eq!(state.blocked[0].slug, "villa-azul");
    }
}
