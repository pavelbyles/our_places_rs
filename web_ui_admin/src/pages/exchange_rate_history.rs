//! One currency pair's rate history (#100, admins only): past, current and
//! scheduled versions, deleting a scheduled version and retiring the pair.

use common::currency::CurrencyCode;
use common::exchange_rate::{PairHistoryResponse, RateState};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form,
        error::{not_found, see_other},
        page, path_param,
    },
    view::{View, component, view},
};
use uuid::Uuid;
use web_client::client::UserCallError;
use web_client::get_api_client;

use super::currencies::{CardState, blocked_listings_dialog, describe};
use crate::support::{admin_only, service_token};

path_param!(base);
path_param!(target);
path_param!(id);

fn page_url(base: &CurrencyCode, target: &CurrencyCode) -> String {
    format!("/admin/exchange-rates/{base}/{target}")
}

fn badge(state: RateState) -> (&'static str, &'static str) {
    match state {
        RateState::Current => ("Current", "badge badge-success"),
        RateState::Scheduled => ("Scheduled", "badge badge-info"),
        RateState::Past => ("Past", "badge badge-ghost"),
    }
}

/// The history fragment (swapped in place by its own actions).
#[component]
pub async fn pair_history_view(
    cx: &Cx,
    /// The pair and its versions.
    history: PairHistoryResponse,
    /// Errors and blocking listings from the last action.
    state: CardState,
) -> Result<impl View> {
    let base = history.base_currency.clone();
    let target = history.target_currency.clone();
    let retire_url = format!("{}/retire", page_url(&base, &target));
    let retired = history.retired_at.is_some();
    Ok(view! {
        <section id="pair-history" class="space-y-4">
            for message in state.errors {
                <p role="alert" class="rounded-lg border border-destructive p-3 text-sm text-destructive">(message)</p>
            }
            if let Some(at) = history.retired_at {
                <p class="alert alert-info text-sm">(format!("Retired {}. Publish a new rate to reinstate it.", at.format("%Y-%m-%d %H:%M UTC")))</p>
            }
            <div class="overflow-x-auto rounded-xl border border-border">
                <table id="pair-history-table" class="w-full text-sm">
                    <thead class="bg-card text-left text-xs text-muted-foreground uppercase">
                        <tr>
                            <th scope="col" class="p-3">"Effective"</th>
                            <th scope="col" class="p-3 text-right">"Rate"</th>
                            <th scope="col" class="p-3">"Status"</th>
                            <th scope="col" class="p-3"><span class="sr-only">"Actions"</span></th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-border">
                        for v in history.versions {
                            <tr data-rate-id=(v.id.to_string())>
                                <td class="p-3">(v.effective_at.format("%Y-%m-%d %H:%M UTC").to_string())</td>
                                <td class="p-3 text-right font-mono tabular-nums">(format!("{:.6}", v.rate))</td>
                                <td class="p-3"><span class=(badge(v.state).1)>(badge(v.state).0)</span></td>
                                <td class="p-3 text-right">
                                    if v.state == RateState::Scheduled {
                                        <form method="post" action=(format!("/admin/exchange-rates/rates/{}/delete", v.id)) hx-post=(format!("/admin/exchange-rates/rates/{}/delete", v.id)) hx-target="#pair-history" hx-swap="outerHTML" hx-confirm="Delete this scheduled rate?" hx-disabled-elt="find button">
                                            <input type="hidden" name="base" value=(base.to_string()) />
                                            <input type="hidden" name="target" value=(target.to_string()) />
                                            <button type="submit" class="h-9 rounded-lg border border-border px-3 text-sm">"Delete"</button>
                                        </form>
                                    }
                                </td>
                            </tr>
                        }
                    </tbody>
                </table>
            </div>
            if !retired {
                <form method="post" action=(retire_url.clone()) hx-post=(retire_url) hx-target="#pair-history" hx-swap="outerHTML" hx-confirm="Retire this pair? Its history is kept." hx-disabled-elt="find button">
                    <button type="submit" class="h-10 rounded-lg border border-destructive px-4 text-sm font-medium text-destructive">"Retire pair"</button>
                </form>
            }
            blocked_listings_dialog(listings: state.blocked)
        </section>
    })
}

/// The pair from the URL (404 when malformed).
fn pair_from_path(cx: &Cx) -> Result<(CurrencyCode, CurrencyCode)> {
    let base = CurrencyCode::try_from(base(cx)).map_err(|_| not_found())?;
    let target = CurrencyCode::try_from(target(cx)).map_err(|_| not_found())?;
    Ok((base, target))
}

/// Loads the history or maps "no such pair" to 404.
async fn load(
    cx: &Cx,
    token: &str,
    base: &CurrencyCode,
    target: &CurrencyCode,
) -> Result<PairHistoryResponse> {
    get_api_client(cx)
        .pair_history_as(token, base, target)
        .await
        .map_err(|e| match e {
            UserCallError::NotFound => not_found().into(),
            other => {
                tracing::warn!(error = %other, %base, %target, "pair history unavailable");
                web_client::app_error(other).into()
            }
        })
}

/// The pair history page.
#[page("/admin/exchange-rates/{base}/{target}")]
pub async fn pair_history_page(cx: &Cx) -> Result<impl View> {
    let admin = admin_only(cx).await?;
    let token = service_token(cx, &admin)?;
    let (base, target) = pair_from_path(cx)?;
    let history = load(cx, &token, &base, &target).await?;
    Ok(view! {
        <div class="space-y-6">
            <div>
                <a href="/admin/exchange-rates" class="text-sm">"Back to exchange rates"</a>
                <h1 class="text-2xl font-semibold">(format!("{base} / {target}"))</h1>
                <p class="text-sm text-muted-foreground">"Every version of this rate. Scheduled versions can be deleted before they take effect."</p>
            </div>
            pair_history_view(history: history, state: CardState::default())
        </div>
    })
}

/// The hidden pair fields on the delete form.
#[derive(Debug, Deserialize)]
pub struct PairForm {
    /// Base currency code.
    #[serde(default)]
    pub base: String,
    /// Target currency code.
    #[serde(default)]
    pub target: String,
}

/// Deletes a scheduled rate version (admins only).
#[page(POST "/admin/exchange-rates/rates/{id}/delete")]
pub async fn delete_scheduled_rate(
    cx: &Cx,
    /// The submitted form.
    Form(form): Form<PairForm>,
) -> Result<impl View> {
    let admin = admin_only(cx).await?;
    let token = service_token(cx, &admin)?;
    let id = Uuid::parse_str(id(cx)).map_err(|_| not_found())?;
    let base = CurrencyCode::try_from(form.base.as_str()).map_err(|_| not_found())?;
    let target = CurrencyCode::try_from(form.target.as_str()).map_err(|_| not_found())?;
    let state = match get_api_client(cx)
        .delete_scheduled_rate_as(&token, id)
        .await
    {
        Ok(()) => {
            tracing::info!(rate_id = %id, admin_id = %admin.user_id, "scheduled rate deleted");
            CardState::default()
        }
        Err(e) => describe(&e),
    };
    if !topcoat::htmx::hx_request(cx) {
        return Err(see_other(page_url(&base, &target)).into());
    }
    let history = load(cx, &token, &base, &target).await?;
    Ok(view! { pair_history_view(history: history, state: state) })
}

/// Retires the pair (admins only); blocked while listings depend on it.
#[page(POST "/admin/exchange-rates/{base}/{target}/retire")]
pub async fn retire_pair(cx: &Cx) -> Result<impl View> {
    let admin = admin_only(cx).await?;
    let token = service_token(cx, &admin)?;
    let (base, target) = pair_from_path(cx)?;
    let state = match get_api_client(cx)
        .retire_pair_as(&token, &base, &target)
        .await
    {
        Ok(_) => {
            tracing::info!(%base, %target, admin_id = %admin.user_id, "pair retired");
            CardState::default()
        }
        Err(e) => describe(&e),
    };
    if !topcoat::htmx::hx_request(cx) {
        return Err(see_other(page_url(&base, &target)).into());
    }
    let history = load(cx, &token, &base, &target).await?;
    Ok(view! { pair_history_view(history: history, state: state) })
}

#[cfg(test)]
mod history_page_tests {
    use super::*;
    use chrono::{Duration, Utc};
    use common::currency::ListingRef;
    use common::exchange_rate::RateVersion;
    use rust_decimal::Decimal;
    use topcoat::view::ViewExt;

    async fn html(v: impl View, cx: &Cx) -> String {
        match v.first().await {
            Ok(handle) => handle.render(cx),
            Err(e) => panic!("render failed: {e}"),
        }
    }

    fn history(retired: bool) -> PairHistoryResponse {
        let version = |days: i64, state| RateVersion {
            id: Uuid::now_v7(),
            rate: Decimal::new(160 + days, 0),
            effective_at: Utc::now() + Duration::days(days),
            created_by: None,
            state,
        };
        PairHistoryResponse {
            base_currency: CurrencyCode::try_from("USD").expect("code"),
            target_currency: CurrencyCode::try_from("JMD").expect("code"),
            retired_at: retired.then(Utc::now),
            versions: vec![
                version(2, RateState::Scheduled),
                version(0, RateState::Current),
                version(-3, RateState::Past),
            ],
        }
    }

    #[tokio::test]
    async fn badges_and_delete_only_on_scheduled_rows() {
        let cx = &Cx::default();
        let h = history(false);
        let out = html(
            view! { pair_history_view(history: h, state: CardState::default()) },
            cx,
        )
        .await;
        assert!(
            out.contains("Scheduled") && out.contains("Current") && out.contains("Past"),
            "{out}"
        );
        assert_eq!(out.matches(">Delete<").count(), 1, "{out}");
        assert!(out.contains("Retire pair"), "{out}");
        assert!(
            out.contains(r#"hx-confirm="Delete this scheduled rate?""#),
            "{out}"
        );
        assert!(!out.contains("hx-delete"), "deletes are POSTs: {out}");
    }

    #[tokio::test]
    async fn retired_pair_shows_banner_and_no_retire_button() {
        let cx = &Cx::default();
        let h = history(true);
        let out = html(
            view! { pair_history_view(history: h, state: CardState::default()) },
            cx,
        )
        .await;
        assert!(out.contains("Publish a new rate to reinstate it"), "{out}");
        assert!(!out.contains("Retire pair"), "{out}");
    }

    #[tokio::test]
    async fn blocked_retire_lists_listings() {
        let cx = &Cx::default();
        let state = CardState {
            errors: vec!["Listings are still priced in this currency.".into()],
            blocked: vec![ListingRef {
                id: Uuid::nil(),
                title: "Villa Azul".into(),
                slug: "villa-azul".into(),
            }],
        };
        let h = history(false);
        let out = html(view! { pair_history_view(history: h, state: state) }, cx).await;
        assert!(out.contains("currency-blocked-dialog"), "{out}");
        assert!(out.contains("Villa Azul"), "{out}");
    }
}
