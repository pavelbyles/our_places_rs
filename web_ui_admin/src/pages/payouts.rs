//! Host earnings and the payout ledger. Admins may override a payout's
//! status (decision D10); hosts see their own ledger read-only.

use std::str::FromStr;

use common::payout::{PayoutFilter, PayoutLedgerEntry, PayoutStatus, UpdatePayoutStatusRequest};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form,
        error::{not_found, see_other},
        page, path_param,
        response::Response,
        route,
    },
    view::{View, view},
};
use uuid::Uuid;
use web_client::{
    get_api_client,
    session::{AdminRole, AdminSession},
};
use web_ui_common::components::{domain::price_breakdown::money, stat_tile::stat_tile};

use crate::support::{admin_only, csv_cell, service_token, staff};

path_param!(id);

/// Statuses an admin can set.
pub const PAYOUT_STATUSES: [&str; 5] = ["pending", "processing", "paid", "cancelled", "refunded"];

/// Status override form (admins only).
#[derive(Deserialize)]
pub struct StatusForm {
    /// New status.
    pub status: String,
    /// Payment gateway reference (for paid).
    #[serde(default)]
    pub gateway_reference: String,
    /// Reason for the override (required).
    #[serde(default)]
    pub reason: String,
}

fn parse_status(s: &str) -> Option<PayoutStatus> {
    match s {
        "pending" => Some(PayoutStatus::Pending),
        "processing" => Some(PayoutStatus::Processing),
        "paid" => Some(PayoutStatus::Paid),
        "cancelled" => Some(PayoutStatus::Cancelled),
        "refunded" => Some(PayoutStatus::Refunded),
        _ => None,
    }
}

/// The ledger filter for the user: everything for admins, own for hosts.
fn scope(user: &AdminSession) -> PayoutFilter {
    PayoutFilter {
        listing_id: None,
        host_id: (user.role == AdminRole::Host).then_some(user.user_id),
        status: None,
        date_from: None,
        date_to: None,
        page: Some(1),
        per_page: Some(100),
    }
}

async fn ledger(cx: &Cx, user: &AdminSession) -> Vec<PayoutLedgerEntry> {
    let entries = get_api_client(cx)
        .get_host_payout_ledger(&scope(user))
        .await
        .map(|r| r.entries)
        .unwrap_or_default();
    match user.role {
        AdminRole::Admin => entries,
        // Defence in depth: never show a host another host's payouts.
        AdminRole::Host => entries
            .into_iter()
            .filter(|e| e.host_id == user.user_id)
            .collect(),
    }
}

/// The payout ledger.
#[page("/admin/payouts")]
pub async fn payouts_page(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let entries = ledger(cx, &user).await;
    let summary = get_api_client(cx)
        .get_host_payout_summary(&scope(&user))
        .await
        .ok();
    let is_admin = user.role == AdminRole::Admin;
    let (gross, fee, net) = summary
        .map(|s| (s.total_gross, s.total_platform_fee, s.total_net))
        .unwrap_or_default();
    let cur = entries
        .first()
        .map(|e| e.currency.clone())
        .unwrap_or_else(|| "USD".to_string());
    let label = "block text-xs text-muted-foreground";
    let input = "mt-1 block h-9 rounded-lg border border-border bg-card px-2 text-sm";
    Ok(view! {
        <div class="space-y-6">
            <div class="flex flex-wrap items-end justify-between gap-4">
                <div>
                    <p class="text-xs font-medium tracking-wide text-muted-foreground uppercase">"Financial Engine · Tri-Currency Ledger"</p>
                    <h1 class="text-2xl font-semibold">"Host Earnings & Payout Ledger"</h1>
                </div>
                <a href="/admin/payouts/export.csv" download="host_payout_ledger.csv" class="inline-flex h-10 items-center rounded-lg border border-border px-4 text-sm no-underline text-foreground">"Export CSV"</a>
            </div>
            <div class="grid gap-4 sm:grid-cols-3">
                stat_tile(label: "Gross Volume".into(), value: money(&cur, gross))
                stat_tile(label: "Platform Fee".into(), value: money(&cur, fee))
                stat_tile(label: "Net Host Payouts".into(), value: money(&cur, net))
            </div>
            <div class="overflow-x-auto rounded-xl border border-border">
                <table id="payouts-ledger-table" class="w-full text-sm">
                    <thead class="bg-card text-left text-xs text-muted-foreground uppercase">
                        <tr>
                            <th scope="col" class="p-3">"Reservation"</th><th scope="col" class="p-3">"Listing"</th><th scope="col" class="p-3">"Host"</th>
                            <th scope="col" class="p-3 text-right">"Gross"</th><th scope="col" class="p-3 text-right">"Net"</th><th scope="col" class="p-3">"Status"</th>
                            if is_admin { <th scope="col" class="p-3">"Override"</th> }
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-border">
                        if entries.is_empty() {
                            <tr><td colspan="7" class="p-6 text-center text-muted-foreground">"No payouts yet."</td></tr>
                        }
                        for e in entries {
                            let status = e.status.to_string();
                            <tr id=(format!("payout-{}", e.id))>
                                <td class="p-3 font-mono">(e.booking_confirmation_code.clone().unwrap_or_default())</td>
                                <td class="p-3">(e.listing_name.clone().unwrap_or_default())</td>
                                <td class="p-3">(e.host_name.clone().unwrap_or_default())</td>
                                <td class="p-3 text-right tabular-nums">(money(&e.currency, e.gross_amount))</td>
                                <td class="p-3 text-right tabular-nums">(money(&e.currency, e.net_payout_amount))</td>
                                <td class="p-3 capitalize">(status.clone())</td>
                                if is_admin {
                                    <td class="p-3">
                                        <form method="post" action=(format!("/admin/payouts/{}/status", e.id)) class="flex flex-wrap items-end gap-2">
                                            <label class=(label)>"Status"
                                                <select name="status" class=(input)>
                                                    for s in PAYOUT_STATUSES {
                                                        <option value=(s) selected=(status == s)>(s)</option>
                                                    }
                                                </select>
                                            </label>
                                            <label class=(label)>"Gateway ref"<input name="gateway_reference" class=(input) /></label>
                                            <label class=(label)>"Reason"<input name="reason" required=(true) class=(input) /></label>
                                            <button type="submit" class="h-9 rounded-lg border border-border px-3 text-sm">"Override"</button>
                                        </form>
                                    </td>
                                }
                            </tr>
                        }
                    </tbody>
                </table>
            </div>
        </div>
    })
}

/// Overrides a payout status: administrators only (hosts get 403). A
/// reason is required and logged with the admin's user ID.
#[page(POST "/admin/payouts/{id}/status")]
pub async fn override_status(
    cx: &Cx,
    /// The override form.
    Form(form): Form<StatusForm>,
) -> Result<()> {
    let user = admin_only(cx).await?;
    let id = Uuid::from_str(path_param::<Id>(cx)).map_err(|_| not_found())?;
    let status =
        parse_status(&form.status).ok_or_else(|| web_client::app_error("unknown payout status"))?;
    let reason = form.reason.trim().to_string();
    if reason.is_empty() {
        return Err(web_client::app_error("a reason is required to override a payout").into());
    }
    let token = service_token(cx, &user)?;
    let request = UpdatePayoutStatusRequest {
        status,
        gateway_reference: Some(form.gateway_reference.trim().to_string())
            .filter(|g| !g.is_empty()),
        failure_reason: Some(reason.clone()),
    };
    get_api_client(cx)
        .update_payout_status_as(id, &request, &token)
        .await
        .map_err(web_client::app_error)?;
    tracing::info!(payout_id = %id, admin_id = %user.user_id, status = %form.status, reason = %reason, "payout status overridden");
    Err(see_other("/admin/payouts").into())
}

/// The ledger as CSV (same scope as the page).
#[route(GET "/admin/payouts/export.csv")]
pub async fn export_payouts(cx: &Cx) -> Result<Response> {
    let user = staff(cx).await?;
    let mut csv = String::from(
        "reservation,listing,host,currency,gross,platform_fee,tax_withheld,net,status\r\n",
    );
    for e in ledger(cx, &user).await {
        let row = [
            e.booking_confirmation_code.clone().unwrap_or_default(),
            e.listing_name.clone().unwrap_or_default(),
            e.host_name.clone().unwrap_or_default(),
            e.currency.clone(),
            format!("{:.2}", e.gross_amount),
            format!("{:.2}", e.platform_fee_amount),
            format!("{:.2}", e.tax_withheld_amount),
            format!("{:.2}", e.net_payout_amount),
            e.status.to_string(),
        ];
        csv.push_str(
            &row.iter()
                .map(|v| csv_cell(v))
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push_str("\r\n");
    }
    Response::builder()
        .header("content-type", "text/csv; charset=utf-8")
        .header(
            "content-disposition",
            "attachment; filename=\"host_payout_ledger.csv\"",
        )
        .body(csv.into())
        .map_err(|e| web_client::app_error(e).into())
}

#[cfg(test)]
mod tests {
    use super::{PAYOUT_STATUSES, parse_status};

    #[test]
    fn every_offered_status_round_trips() {
        for s in PAYOUT_STATUSES {
            assert_eq!(parse_status(s).map(|p| p.to_string()), Some(s.to_string()));
        }
        assert!(parse_status("bogus").is_none());
    }
}
