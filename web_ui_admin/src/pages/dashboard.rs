use std::collections::BTreeMap;

use rust_decimal::Decimal;
use topcoat::{
    Result,
    context::Cx,
    router::{error::redirect_permanent, page},
    view::{View, view},
};
use web_client::session::AdminRole;
use web_ui_common::components::{domain::price_breakdown::money, stat_tile::stat_tile};

use crate::support::{scoped_bookings, scoped_listings, staff};

/// The console dashboard: listings, holds and revenue in the user's scope.
#[page("/admin")]
pub async fn dashboard_page(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let listings = scoped_listings(cx, &user).await;
    let bookings = scoped_bookings(cx, &user, &listings).await;
    let active = listings.iter().filter(|l| l.is_active).count();
    let holds = bookings
        .iter()
        .filter(|b| b.status.eq_ignore_ascii_case("pending"))
        .count();
    let mut revenue: BTreeMap<String, Decimal> = BTreeMap::new();
    for b in bookings.iter().filter(|b| {
        b.status.eq_ignore_ascii_case("confirmed") || b.status.eq_ignore_ascii_case("completed")
    }) {
        *revenue.entry(b.currency.clone()).or_default() += b.total_price;
    }
    let revenue_text = if revenue.is_empty() {
        money("USD", Decimal::ZERO)
    } else {
        revenue
            .iter()
            .map(|(c, v)| money(c, *v))
            .collect::<Vec<_>>()
            .join(" · ")
    };
    let scope = match user.role {
        AdminRole::Admin => "All listings on the platform",
        AdminRole::Host => "Your listings",
    };
    let action = "inline-flex h-10 items-center rounded-lg border border-border px-4 text-sm font-medium no-underline text-foreground";
    Ok(view! {
        <div class="space-y-8">
            <div class="flex flex-wrap items-end justify-between gap-4">
                <div>
                    <h1 class="text-2xl font-semibold">"Executive Dashboard"</h1>
                    <p class="text-sm text-muted-foreground">(scope)</p>
                </div>
                <div class="flex flex-wrap gap-2">
                    <a href="/admin/listings/new" class="inline-flex h-10 items-center rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground no-underline">"New listing"</a>
                    <a href="/admin/bookings" class=(action)>"Booking schedule"</a>
                    <a href="/admin/payouts" class=(action)>"Payouts"</a>
                </div>
            </div>
            <div class="grid gap-4 sm:grid-cols-3">
                stat_tile(label: "Active Listings".into(), value: active.to_string(), hint: Some(format!("{} total", listings.len())))
                stat_tile(label: "Active Holds".into(), value: holds.to_string(), hint: Some("Pending payment".into()))
                stat_tile(label: "Bookings Revenue".into(), value: revenue_text, hint: Some("Confirmed and completed".into()))
            </div>
        </div>
    })
}

/// `/` goes to the dashboard.
#[page("/")]
pub async fn root_page(cx: &Cx) -> Result<()> {
    let _ = cx;
    Err(redirect_permanent("/admin").into())
}
