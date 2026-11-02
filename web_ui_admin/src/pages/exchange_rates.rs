//! Exchange rates (#100). Admins and hosts see the current rate per pair;
//! admins also publish rates (with the 5% confirmation step), see coverage
//! gaps, open pair history and manage currencies. Everything comes from the
//! database; no currency or pair is hardcoded.

use chrono::{NaiveDateTime, Utc};
use common::currency::CurrencyCode;
use common::exchange_rate::{ExchangeRate, PublishRateRequest, RateCoverageGap};
use rust_decimal::Decimal;
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form,
        error::see_other,
        header::{HeaderName, HeaderValue},
        page,
        response::response_headers,
    },
    view::{View, component, view},
};
use web_client::client::UserCallError;
use web_client::get_api_client;
use web_client::session::AdminRole;

use crate::support::{admin_only, service_token, staff};

/// One table row: a pair and its current rate.
#[derive(Debug, Clone, PartialEq)]
pub struct RateRow {
    /// Base currency code.
    pub base: String,
    /// Target currency code.
    pub target: String,
    /// Current rate.
    pub rate: Decimal,
    /// When it took effect.
    pub effective: String,
}

/// The stored current rates, sorted by pair. Nothing is invented for pairs
/// without a rate; those show up in the coverage alert instead.
pub fn rows(rates: Vec<ExchangeRate>) -> Vec<RateRow> {
    let mut rows: Vec<RateRow> = rates
        .into_iter()
        .map(|r| RateRow {
            base: r.base_currency,
            target: r.target_currency,
            rate: r.rate,
            effective: r.effective_at.format("%Y-%m-%d %H:%M UTC").to_string(),
        })
        .collect();
    rows.sort_by(|a, b| (&a.base, &a.target).cmp(&(&b.base, &b.target)));
    rows
}

/// The current-rate table. `show_admin_actions` adds history links.
#[component]
pub async fn rates_table(
    /// Rows to show.
    rows: Vec<RateRow>,
    /// Whether to show admin-only actions.
    show_admin_actions: bool,
) -> Result<impl View> {
    Ok(view! {
        <div class="overflow-x-auto rounded-xl border border-border">
            <table id="exchange-rates-table" class="w-full text-sm">
                <thead class="bg-card text-left text-xs text-muted-foreground uppercase">
                    <tr>
                        <th scope="col" class="p-3">"Currency pair"</th>
                        <th scope="col" class="p-3 text-right">"Rate"</th>
                        <th scope="col" class="p-3">"Effective"</th>
                        if show_admin_actions {
                            <th scope="col" class="p-3"><span class="sr-only">"Actions"</span></th>
                        }
                    </tr>
                </thead>
                <tbody class="divide-y divide-border">
                    for row in rows {
                        <tr>
                            <td class="p-3 font-medium">(format!("{} / {}", row.base, row.target))</td>
                            <td class="p-3 text-right font-mono tabular-nums">(format!("{:.6}", row.rate))</td>
                            <td class="p-3 text-muted-foreground">(row.effective)</td>
                            if show_admin_actions {
                                <td class="p-3 text-right">
                                    <a href=(format!("/admin/exchange-rates/{}/{}", row.base, row.target)) class="text-sm underline">"History"</a>
                                </td>
                            }
                        </tr>
                    }
                </tbody>
            </table>
        </div>
    })
}

/// Combinations live listings cannot be quoted in (admins only).
#[component]
pub async fn coverage_alert(
    /// The gaps.
    gaps: Vec<RateCoverageGap>,
) -> Result<impl View> {
    Ok(view! {
        if !gaps.is_empty() {
            <div id="rate-coverage-alert" role="status" class="alert alert-warning text-sm">
                <div>
                    <p class="font-medium">"Some listings cannot be priced in every currency"</p>
                    <ul class="mt-1 list-disc pl-5">
                        for g in gaps {
                            <li>(format!("{} → {}: no rate", g.listing_base_currency, g.target_currency))</li>
                        }
                    </ul>
                </div>
            </div>
        }
    })
}

/// Values of the publish form.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct PublishForm {
    /// Base currency code.
    #[serde(default)]
    pub base: String,
    /// Target currency code.
    #[serde(default)]
    pub target: String,
    /// Rate as typed.
    #[serde(default)]
    pub rate: String,
    /// `datetime-local` value in UTC; empty means now.
    #[serde(default)]
    pub effective_at: String,
    /// `"true"` once the admin confirmed a large change.
    #[serde(default)]
    pub confirm: String,
}

/// What the publish form shows besides its fields.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum FormAlert {
    /// Nothing.
    #[default]
    None,
    /// An error message.
    Error(String),
    /// The 5% guard tripped: ask to confirm.
    Confirm {
        /// Rate it is compared with.
        baseline: String,
        /// Rate submitted.
        proposed: String,
        /// Deviation in percent.
        deviation: String,
    },
}

/// The publish form (swapped in place by HTMX).
#[component]
pub async fn publish_rate_form(
    /// Active currency codes for the selects.
    codes: Vec<String>,
    /// Current values.
    form: PublishForm,
    /// Error or confirmation prompt.
    alert: FormAlert,
) -> Result<impl View> {
    let field = "mt-1 block h-10 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground";
    let label = "block text-xs font-medium text-muted-foreground";
    let confirm = matches!(alert, FormAlert::Confirm { .. });
    let base_codes = codes.clone();
    Ok(view! {
        <form id="publish-rate-form" method="post" action="/admin/exchange-rates" hx-post="/admin/exchange-rates" hx-target="this" hx-swap="outerHTML" hx-disabled-elt="find button" class="space-y-4 rounded-xl border border-border p-6">
            <h2 class="text-lg font-semibold">"Publish a rate"</h2>
            match alert.clone() {
                FormAlert::None => {}
                FormAlert::Error(message) => {
                    <p role="alert" class="rounded-lg border border-destructive p-3 text-sm text-destructive">(message)</p>
                }
                FormAlert::Confirm { baseline, proposed, deviation } => {
                    <div id="rate-confirm-alert" role="alert" class="alert alert-warning text-sm">
                        <p>(format!("{baseline} → {proposed} is a {deviation}% change. Check the rate, then confirm to save it."))</p>
                    </div>
                }
            }
            <div class="grid gap-4 sm:grid-cols-4">
                <label class=(label)>"From"
                    <select name="base" required=(true) class=(field)>
                        for c in base_codes {
                            <option value=(c.clone()) selected=(form.base == c)>(c.clone())</option>
                        }
                    </select>
                </label>
                <label class=(label)>"To"
                    <select name="target" required=(true) class=(field)>
                        for c in codes {
                            <option value=(c.clone()) selected=(form.target == c)>(c.clone())</option>
                        }
                    </select>
                </label>
                <label class=(label)>"Rate"
                    <input name="rate" required=(true) inputmode="decimal" pattern="[0-9]+(\\.[0-9]+)?" value=(form.rate.clone()) class=(field) />
                </label>
                <label class=(label)>"Effective from (UTC, blank = now)"
                    <input name="effective_at" type="datetime-local" value=(form.effective_at.clone()) class=(field) />
                </label>
            </div>
            if confirm {
                <input type="hidden" name="confirm" value="true" />
            }
            <button type="submit" class="h-10 rounded-lg bg-primary px-5 text-sm font-medium text-primary-foreground">(if confirm { "Confirm and publish" } else { "Publish rate" })</button>
        </form>
    })
}

/// The exchange-rate page.
#[page("/admin/exchange-rates")]
pub async fn exchange_rates_page(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let token = service_token(cx, &user)?;
    let api = get_api_client(cx);
    let loaded = api.get_exchange_rates_as(&token).await;
    let error = loaded.as_ref().err().map(|e| {
        tracing::warn!("exchange rates unavailable: {e}");
        "Exchange rates could not be loaded. Try again shortly.".to_string()
    });
    let table = rows(loaded.map(|r| r.rates).unwrap_or_default());
    let is_admin = user.role == AdminRole::Admin;
    let gaps = if is_admin {
        api.rate_coverage_as(&token).await.unwrap_or_else(|e| {
            tracing::warn!("rate coverage unavailable: {e}");
            Vec::new()
        })
    } else {
        Vec::new()
    };
    let codes =
        web_ui_common::components::domain::currency_selector::active_currency_codes(cx).await;
    Ok(view! {
        <div class="space-y-6">
            <div>
                <p class="text-xs font-medium tracking-wide text-muted-foreground uppercase">"Tri-Currency Financial Engine"</p>
                <h1 class="text-2xl font-semibold">"Exchange Rates"</h1>
                <p class="text-sm text-muted-foreground">"The rate in effect now for each pair; pricing converts with these."</p>
            </div>
            if let Some(message) = error {
                <p role="alert" class="rounded-lg border border-destructive p-3 text-sm text-destructive">(message)</p>
            }
            if is_admin {
                coverage_alert(gaps: gaps)
            }
            rates_table(rows: table, show_admin_actions: is_admin)
            if is_admin {
                publish_rate_form(codes: codes, form: PublishForm::default(), alert: FormAlert::None)
                super::currencies::currencies_section(token: token.clone())
            }
        </div>
    })
}

/// Parses a `datetime-local` value as UTC; empty means "now" (`None`).
fn parse_effective_at(raw: &str) -> std::result::Result<Option<chrono::DateTime<Utc>>, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S"))
        .map(|naive| Some(naive.and_utc()))
        .map_err(|_| "Enter the effective time as a date and time.".to_string())
}

/// The alert for a failed publish.
fn publish_alert(err: &UserCallError) -> FormAlert {
    match err {
        UserCallError::Api(r) if r.code == "RATE_CHANGE_CONFIRMATION_REQUIRED" => {
            let field = |k: &str| r.body[k].as_str().unwrap_or_default().to_string();
            FormAlert::Confirm {
                baseline: field("baseline_rate"),
                proposed: field("proposed_rate"),
                deviation: field("deviation_pct"),
            }
        }
        UserCallError::Api(r) => FormAlert::Error(r.message.clone()),
        UserCallError::Unauthorized => {
            FormAlert::Error("Only administrators can publish rates.".to_string())
        }
        other => {
            tracing::warn!(error = %other, "rate publish failed");
            FormAlert::Error("The rate could not be saved. Try again shortly.".to_string())
        }
    }
}

/// Publishes a rate (admins only). HTMX gets the form back (with an error or
/// the confirmation prompt) or a redirect to the refreshed page.
#[page(POST "/admin/exchange-rates")]
pub async fn publish_rate(
    cx: &Cx,
    /// The submitted form.
    Form(form): Form<PublishForm>,
) -> Result<impl View> {
    let admin = admin_only(cx).await?;
    let token = service_token(cx, &admin)?;
    let parsed = CurrencyCode::try_from(form.base.as_str())
        .and_then(|b| CurrencyCode::try_from(form.target.as_str()).map(|t| (b, t)))
        .map_err(|_| "Choose both currencies.".to_string())
        .and_then(|(base, target)| {
            parse_effective_at(&form.effective_at).map(|at| PublishRateRequest {
                base_currency: base,
                target_currency: target,
                rate: form.rate.trim().to_string(),
                effective_at: at,
                confirm_large_change: form.confirm == "true",
            })
        });
    let alert = match parsed {
        Err(message) => FormAlert::Error(message),
        Ok(req) => match get_api_client(cx).publish_rate_as(&token, &req).await {
            Ok(version) => {
                tracing::info!(base = %req.base_currency, target = %req.target_currency, rate = %version.rate, admin_id = %admin.user_id, "rate published");
                const DONE: &str = "/admin/exchange-rates?notice=published";
                if !topcoat::htmx::hx_request(cx) {
                    return Err(see_other(DONE).into());
                }
                response_headers(cx).append(
                    HeaderName::from_static("hx-redirect"),
                    HeaderValue::from_static(DONE),
                );
                FormAlert::None
            }
            Err(e) => publish_alert(&e),
        },
    };
    if !topcoat::htmx::hx_request(cx) {
        return Err(see_other("/admin/exchange-rates?notice=rejected").into());
    }
    let codes =
        web_ui_common::components::domain::currency_selector::active_currency_codes(cx).await;
    Ok(view! { publish_rate_form(codes: codes, form: form, alert: alert) })
}

#[cfg(test)]
mod exchange_rates_page_tests {
    use super::*;
    use chrono::Utc;
    use topcoat::view::ViewExt;

    async fn html(v: impl View, cx: &Cx) -> String {
        match v.first().await {
            Ok(handle) => handle.render(cx),
            Err(e) => panic!("render failed: {e}"),
        }
    }

    fn rate(base: &str, target: &str, r: i64) -> ExchangeRate {
        ExchangeRate {
            base_currency: base.into(),
            target_currency: target.into(),
            rate: Decimal::new(r, 0),
            effective_at: Utc::now(),
        }
    }

    /// Replaces `lists_missing_usd_pairs_as_not_set`: the table now shows only
    /// stored pairs (no hardcoded USD targets, no "Not set" rows).
    #[test]
    fn lists_db_rows_only() {
        let table = rows(vec![rate("USD", "JMD", 160), rate("GBP", "USD", 1)]);
        let pairs: Vec<_> = table
            .iter()
            .map(|r| (r.base.as_str(), r.target.as_str()))
            .collect();
        assert_eq!(pairs, [("GBP", "USD"), ("USD", "JMD")]);
    }

    #[tokio::test]
    async fn admin_table_links_history_but_host_table_does_not() {
        let __cx = &Cx::default();
        let admin_rows = rows(vec![rate("USD", "JMD", 160)]);
        let host_rows = admin_rows.clone();
        let admin = html(
            view! { rates_table(rows: admin_rows, show_admin_actions: true) },
            __cx,
        )
        .await;
        assert!(
            admin.contains(r#"href="/admin/exchange-rates/USD/JMD""#),
            "{admin}"
        );
        let host = html(
            view! { rates_table(rows: host_rows, show_admin_actions: false) },
            __cx,
        )
        .await;
        assert!(!host.contains("History"), "{host}");
        assert!(host.contains("160.000000"), "{host}");
    }

    #[tokio::test]
    async fn confirm_prompt_resubmits_with_confirmation() {
        let __cx = &Cx::default();
        let codes = vec!["JMD".to_string(), "USD".to_string()];
        let form = PublishForm {
            base: "USD".into(),
            target: "JMD".into(),
            rate: "176".into(),
            ..Default::default()
        };
        let alert = FormAlert::Confirm {
            baseline: "160.000000".into(),
            proposed: "176".into(),
            deviation: "10.00".into(),
        };
        let out = html(
            view! { publish_rate_form(codes: codes, form: form, alert: alert) },
            __cx,
        )
        .await;
        assert!(out.contains("rate-confirm-alert"), "{out}");
        assert!(out.contains("10.00%"), "{out}");
        assert!(out.contains(r#"name="confirm" value="true""#), "{out}");
        assert!(out.contains("Confirm and publish"), "{out}");
        assert!(out.contains(r#"hx-disabled-elt="find button""#), "{out}");
    }

    #[tokio::test]
    async fn coverage_alert_lists_gaps_and_hides_when_none() {
        let __cx = &Cx::default();
        let gaps = vec![RateCoverageGap {
            listing_base_currency: CurrencyCode::try_from("USD").expect("code"),
            target_currency: CurrencyCode::try_from("CAD").expect("code"),
        }];
        let shown = html(view! { coverage_alert(gaps: gaps) }, __cx).await;
        assert!(shown.contains("USD → CAD: no rate"), "{shown}");
        let none = html(view! { coverage_alert(gaps: Vec::new()) }, __cx).await;
        assert!(!none.contains("rate-coverage-alert"), "{none}");
    }

    #[test]
    fn effective_time_is_parsed_as_utc_or_now() {
        assert_eq!(parse_effective_at(""), Ok(None));
        let at = parse_effective_at("2030-01-02T03:04").expect("parses");
        assert_eq!(
            at.map(|d| d.to_rfc3339()),
            Some("2030-01-02T03:04:00+00:00".into())
        );
        assert!(parse_effective_at("tomorrow").is_err());
    }
}
