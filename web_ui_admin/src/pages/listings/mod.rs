//! Listings studio: inventory, detail, create, clone and edit.

use common::models::ListingResponse;
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{error::not_found, page, parse_query_params, path_param},
    view::{View, view},
};
use uuid::Uuid;
use web_client::{
    get_api_client,
    session::{AdminRole, AdminSession},
};
use web_ui_common::components::domain::price_breakdown::money;

use crate::support::{scoped_listings, staff};

/// New listing, clone and create.
pub mod create;
/// Edit an existing listing.
pub mod edit;
/// The studio form shared by create, clone and edit.
pub mod form;
/// Signed-URL photo uploads.
pub mod images;

path_param!(id);

#[derive(Deserialize, Default)]
struct ListQuery {
    q: Option<String>,
    status: Option<String>,
}

/// Loads a listing the user may manage (admins: any; hosts: their own).
/// Anything else is a 404.
pub async fn managed_listing(
    cx: &Cx,
    user: &AdminSession,
    id_or_slug: &str,
) -> Result<ListingResponse> {
    let listing = get_api_client(cx)
        .get_listing_by_id(id_or_slug, None)
        .await
        .map_err(|_| not_found())?
        .listing;
    if user.role == AdminRole::Host && listing.user_id != user.user_id {
        tracing::info!(listing_id = %listing.id, "host denied another host's listing");
        return Err(not_found().into());
    }
    Ok(listing)
}

/// Owner choices for admins (hosts and admins).
pub async fn owner_options(cx: &Cx, user: &AdminSession) -> Vec<form::OwnerOption> {
    if user.role != AdminRole::Admin {
        return Vec::new();
    }
    let mut owners: Vec<form::OwnerOption> = get_api_client(cx)
        .get_all_users(Some(1), Some(100), Some("host".to_string()))
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|u| {
            (
                u.id.to_string(),
                format!("{} {} ({})", u.first_name, u.last_name, u.email),
            )
        })
        .collect();
    if !owners.iter().any(|(id, _)| *id == user.user_id.to_string()) {
        owners.insert(
            0,
            (user.user_id.to_string(), format!("{} (you)", user.name)),
        );
    }
    owners
}

/// The listings inventory, searchable by name and filterable by status.
#[page("/admin/listings")]
pub async fn listings_page(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let q = parse_query_params::<ListQuery>(cx).unwrap_or_default();
    let needle = q.q.clone().unwrap_or_default().to_lowercase();
    let status = q.status.clone().unwrap_or_default();
    let listings: Vec<ListingResponse> = scoped_listings(cx, &user)
        .await
        .into_iter()
        .filter(|l| {
            needle.is_empty()
                || l.name.to_lowercase().contains(&needle)
                || l.city
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&needle)
        })
        .filter(|l| match status.as_str() {
            "active" => l.is_active,
            "inactive" => !l.is_active,
            _ => true,
        })
        .collect();
    let input = "h-10 rounded-lg border border-border bg-card px-3 text-sm";
    Ok(view! {
        <div class="space-y-6">
            <div class="flex flex-wrap items-end justify-between gap-4">
                <div>
                    <h1 class="text-2xl font-semibold">"Listings Management"</h1>
                    <p class="text-sm text-muted-foreground">(format!("{} listings", listings.len()))</p>
                </div>
                <a href="/admin/listings/new" class="inline-flex h-10 items-center rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground no-underline">"New listing"</a>
            </div>
            <form method="get" action="/admin/listings" role="search" class="flex flex-wrap gap-2">
                <label for="listing-search" class="sr-only">"Search listings"</label>
                <input id="listing-search" type="search" name="q" value=(q.q.unwrap_or_default()) placeholder="Search name or city" class=(format!("{input} w-64")) />
                <label for="listing-status" class="sr-only">"Status"</label>
                <select id="listing-status" name="status" class=(input)>
                    <option value="" selected=(status.is_empty())>"All statuses"</option>
                    <option value="active" selected=(status == "active")>"Active"</option>
                    <option value="inactive" selected=(status == "inactive")>"Inactive"</option>
                </select>
                <button type="submit" class="h-10 rounded-lg border border-border px-4 text-sm">"Filter"</button>
            </form>
            <div class="overflow-x-auto rounded-xl border border-border">
                <table id="admin-listings-table" class="w-full text-sm">
                    <thead class="bg-card text-left text-xs text-muted-foreground uppercase">
                        <tr><th scope="col" class="p-3">"Listing"</th><th scope="col" class="p-3">"Location"</th><th scope="col" class="p-3 text-right">"Nightly"</th><th scope="col" class="p-3">"Status"</th><th scope="col" class="p-3 text-right">"Actions"</th></tr>
                    </thead>
                    <tbody class="divide-y divide-border">
                        if listings.is_empty() {
                            <tr><td colspan="5" class="p-6 text-center text-muted-foreground">"No listings match."</td></tr>
                        }
                        for l in listings {
                            let id = l.id;
                            <tr id=(format!("listing-row-{id}"))>
                                <td class="p-3"><a href=(format!("/admin/listings/{id}")) class="font-medium">(l.name.clone())</a><div class="text-xs text-muted-foreground">(l.listing_structure.clone())</div></td>
                                <td class="p-3">(l.city.clone().map(|c| format!("{c}, {}", l.country)).unwrap_or_else(|| l.country.clone()))</td>
                                <td class="p-3 text-right tabular-nums">(l.price_per_night.map(|p| money(&l.base_currency, p)).unwrap_or_default())</td>
                                <td class="p-3">(if l.is_active { "Active" } else { "Inactive" })</td>
                                <td class="p-3 text-right whitespace-nowrap">
                                    <a href=(format!("/admin/listings/{id}/edit")) class="mr-3">"Edit"</a>
                                    <a href=(format!("/admin/listings/{id}/pricing")) class="mr-3">"Pricing"</a>
                                    <a href=(format!("/admin/listings/clone/{}", l.slug))>"Clone"</a>
                                </td>
                            </tr>
                        }
                    </tbody>
                </table>
            </div>
        </div>
    })
}

/// One listing's summary with links to edit, pricing, clone and the guest page.
#[page("/admin/listings/{id}")]
pub async fn listing_detail_page(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let raw = path_param::<Id>(cx);
    Uuid::parse_str(raw).map_err(|_| not_found())?;
    let l = managed_listing(cx, &user, raw).await?;
    let id = l.id;
    let facts = [
        ("Structure", l.listing_structure.clone()),
        (
            "Location",
            l.city
                .clone()
                .map(|c| format!("{c}, {}", l.country))
                .unwrap_or_else(|| l.country.clone()),
        ),
        (
            "Nightly price",
            l.price_per_night
                .map(|p| money(&l.base_currency, p))
                .unwrap_or_default(),
        ),
        (
            "Cleaning fee",
            l.cleaning_fee
                .map(|p| money(&l.base_currency, p))
                .unwrap_or_else(|| "—".into()),
        ),
        (
            "Damage deposit",
            l.damage_deposit
                .map(|p| money(&l.base_currency, p))
                .unwrap_or_else(|| "—".into()),
        ),
        (
            "Capacity",
            format!(
                "{} guests · {} bedrooms · {} beds · {} baths",
                l.max_guests, l.bedrooms, l.beds, l.full_bathrooms
            ),
        ),
        ("Minimum stay", format!("{} night(s)", l.minimum_stay)),
        (
            "Status",
            if l.is_active {
                "Active".into()
            } else {
                "Inactive".into()
            },
        ),
    ];
    let action = "inline-flex h-10 items-center rounded-lg border border-border px-4 text-sm font-medium no-underline text-foreground";
    Ok(view! {
        <div class="space-y-6">
            <div class="flex flex-wrap items-end justify-between gap-4">
                <div>
                    <a href="/admin/listings" class="text-sm">"← Listings"</a>
                    <h1 class="text-2xl font-semibold">(l.name.clone())</h1>
                </div>
                <div class="flex flex-wrap gap-2">
                    <a href=(format!("/admin/listings/{id}/edit")) class="inline-flex h-10 items-center rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground no-underline">"Edit"</a>
                    <a href=(format!("/admin/listings/{id}/pricing")) class=(action)>"Seasonal pricing"</a>
                    <a href=(format!("/admin/listings/clone/{}", l.slug)) class=(action)>"Clone"</a>
                </div>
            </div>
            <dl class="grid gap-4 rounded-xl border border-border p-6 sm:grid-cols-2">
                for (k, v) in facts {
                    <div><dt class="text-xs text-muted-foreground">(k)</dt><dd class="text-sm">(v)</dd></div>
                }
            </dl>
            if let Some(d) = l.description.clone() {
                <p class="max-w-3xl text-sm">(d)</p>
            }
        </div>
    })
}
