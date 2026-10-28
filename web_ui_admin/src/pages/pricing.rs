//! Seasonal pricing overrides for a listing (the legacy page was a stub that
//! never reached `listing_api`; inventory A16–A18).

use std::str::FromStr;

use chrono::NaiveDate;
use common::models::{CreatePriceOverrideRequest, ListingResponse, PriceOverride};
use rust_decimal::Decimal;
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, error::not_found, page, path_param},
    view::{View, component, view},
};
use uuid::Uuid;

use crate::pages::listings::managed_listing;
use crate::support::staff;
use web_ui_common::components::domain::price_breakdown::money;

path_param!(id);

/// Add-override form fields.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct OverrideForm {
    /// First night.
    pub start_date: String,
    /// Last night (inclusive).
    pub end_date: String,
    /// Nightly rate in the listing's currency.
    pub price: String,
    /// Minimum nights.
    pub min_nights: String,
}

/// Remove-override form fields.
#[derive(Deserialize)]
pub struct RemoveForm {
    /// The override to remove.
    pub override_id: Uuid,
}

impl OverrideForm {
    fn parse(&self) -> std::result::Result<CreatePriceOverrideRequest, String> {
        let start = NaiveDate::from_str(self.start_date.trim())
            .map_err(|_| "Choose a start date.".to_string())?;
        let end = NaiveDate::from_str(self.end_date.trim())
            .map_err(|_| "Choose an end date.".to_string())?;
        if end < start {
            return Err("The end date must be on or after the start date.".into());
        }
        let rate = Decimal::from_str(self.price.trim())
            .ok()
            .filter(|r| *r > Decimal::ZERO)
            .ok_or_else(|| "Enter a nightly rate above 0.".to_string())?;
        let min_nights = i32::from_str(self.min_nights.trim())
            .ok()
            .filter(|n| *n >= 1)
            .ok_or_else(|| "Minimum nights must be at least 1.".to_string())?;
        Ok(CreatePriceOverrideRequest {
            start_date: start,
            end_date: end,
            nightly_rate: rate.round_dp(2),
            min_nights,
        })
    }
}

async fn load(cx: &Cx) -> Result<ListingResponse> {
    let user = staff(cx).await?;
    let raw = path_param::<Id>(cx);
    Uuid::parse_str(raw).map_err(|_| not_found())?;
    managed_listing(cx, &user, raw).await
}

async fn overrides(listing: &ListingResponse) -> Vec<PriceOverride> {
    let mut list = common::app_client::get_price_overrides(listing.id)
        .await
        .unwrap_or_default();
    list.sort_by_key(|o| o.start_date);
    list
}

/// The overrides table; HTMX swaps it (outerHTML) after add/remove.
#[component]
async fn overrides_table(
    listing: ListingResponse,
    rows: Vec<PriceOverride>,
    error: Option<String>,
) -> Result<impl View> {
    let remove = format!("/admin/listings/{}/pricing/remove", listing.id);
    Ok(view! {
        <div id="price-overrides-container" class="space-y-3">
            if let Some(error) = error {
                <p role="alert" class="rounded-lg border border-destructive p-3 text-sm text-destructive">(error)</p>
            }
            <table class="w-full text-sm">
                <thead class="text-left text-xs text-muted-foreground uppercase">
                    <tr><th scope="col" class="py-2">"Dates"</th><th scope="col" class="py-2 text-right">"Nightly rate"</th><th scope="col" class="py-2 text-right">"Min stay"</th><th scope="col" class="py-2 text-right">"Action"</th></tr>
                </thead>
                <tbody class="divide-y divide-border">
                    if rows.is_empty() {
                        <tr><td colspan="4" class="py-4 text-muted-foreground">"No seasonal overrides. The base nightly price applies."</td></tr>
                    }
                    for o in rows {
                        <tr id=(format!("override-{}", o.id))>
                            <td class="py-2">(format!("{} – {}", o.start_date.format("%b %d, %Y"), o.end_date.format("%b %d, %Y")))</td>
                            <td class="py-2 text-right tabular-nums">(money(&listing.base_currency, o.nightly_rate))</td>
                            <td class="py-2 text-right">(format!("{} night(s)", o.min_nights))</td>
                            <td class="py-2 text-right">
                                <form method="post" action=(remove.clone()) hx-post=(remove.clone()) hx-target="#price-overrides-container" hx-swap="outerHTML">
                                    <input type="hidden" name="override_id" value=(o.id.to_string()) />
                                    <button type="submit" class="text-destructive">"Remove"</button>
                                </form>
                            </td>
                        </tr>
                    }
                </tbody>
            </table>
        </div>
    })
}

/// Seasonal pricing for one listing.
#[page("/admin/listings/{id}/pricing")]
pub async fn pricing_page(cx: &Cx) -> Result<impl View> {
    let listing = load(cx).await?;
    let rows = overrides(&listing).await;
    let add = format!("/admin/listings/{}/pricing/add", listing.id);
    let label = "block text-xs font-medium text-muted-foreground";
    let input = "mt-1 block h-10 w-full rounded-lg border border-border bg-card px-3 text-sm";
    let name = listing.name.clone();
    let id = listing.id;
    let base = listing
        .price_per_night
        .map(|p| money(&listing.base_currency, p))
        .unwrap_or_default();
    Ok(view! {
        <div class="space-y-6">
            <div>
                <a href=(format!("/admin/listings/{id}")) class="text-sm">"← Listing"</a>
                <h1 class="text-2xl font-semibold">(format!("Seasonal pricing: {name}"))</h1>
                <p class="text-sm text-muted-foreground">"Base nightly price " (base)</p>
            </div>
            <div class="grid gap-6 lg:grid-cols-[320px_1fr]">
                <form method="post" action=(add.clone()) hx-post=(add) hx-target="#price-overrides-container" hx-swap="outerHTML" class="space-y-3 rounded-xl border border-border p-5">
                    <h2 class="text-sm font-semibold">"Add seasonal override"</h2>
                    <label class=(label)>"Start date"<input type="date" name="start_date" required=(true) class=(input) /></label>
                    <label class=(label)>"End date"<input type="date" name="end_date" required=(true) class=(input) /></label>
                    <label class=(label)>"Nightly rate"<input type="number" name="price" min="0.01" step="0.01" required=(true) class=(input) /></label>
                    <label class=(label)>"Minimum nights"<input type="number" name="min_nights" min="1" step="1" value="1" required=(true) class=(input) /></label>
                    <button type="submit" class="h-10 w-full rounded-lg bg-primary text-sm font-medium text-primary-foreground">"Add override"</button>
                </form>
                <div class="rounded-xl border border-border p-5">
                    overrides_table(listing: listing, rows: rows, error: None)
                </div>
            </div>
        </div>
    })
}

/// Adds an override and returns the refreshed table.
#[page(POST "/admin/listings/{id}/pricing/add")]
pub async fn add_override(
    cx: &Cx,
    /// The add-override form.
    Form(form): Form<OverrideForm>,
) -> Result<impl View> {
    let listing = load(cx).await?;
    let error = match form.parse() {
        Err(message) => Some(message),
        Ok(request) => {
            match common::app_client::create_price_override(listing.id, &request).await {
                Ok(_) => None,
                Err(e) => {
                    tracing::info!("override rejected: {e}");
                    Some(
                        "The override could not be saved. Overlapping seasons are not allowed."
                            .to_string(),
                    )
                }
            }
        }
    };
    let rows = overrides(&listing).await;
    Ok(view! { overrides_table(listing: listing, rows: rows, error: error) })
}

/// Removes an override and returns the refreshed table.
#[page(POST "/admin/listings/{id}/pricing/remove")]
pub async fn remove_override(
    cx: &Cx,
    /// The remove form.
    Form(form): Form<RemoveForm>,
) -> Result<impl View> {
    let listing = load(cx).await?;
    let error = common::app_client::delete_price_override(listing.id, form.override_id)
        .await
        .err()
        .map(|e| {
            tracing::info!("override removal rejected: {e}");
            "The override could not be removed.".to_string()
        });
    let rows = overrides(&listing).await;
    Ok(view! { overrides_table(listing: listing, rows: rows, error: error) })
}
