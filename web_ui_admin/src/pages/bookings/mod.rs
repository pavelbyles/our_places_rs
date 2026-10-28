//! The master booking schedule, its CSV export and the booking-details
//! dialog (door codes, status, documents, financial audit).

use std::collections::HashMap;

use common::models::{BookingResponse, ListingResponse};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{error::not_found, page, parse_query_params, response::Response, route},
    view::{View, view},
};
use uuid::Uuid;
use web_client::{
    get_api_client,
    session::{AdminRole, AdminSession},
};
use web_ui_common::components::domain::price_breakdown::money;

use crate::support::{scoped_bookings, scoped_listings, service_token, staff};

/// The booking-details dialog and its actions.
pub mod detail;

/// Booking statuses an admin or host can filter and set.
pub const STATUSES: [&str; 4] = ["pending", "confirmed", "completed", "cancelled"];

#[derive(Deserialize, Default)]
struct ScheduleQuery {
    q: Option<String>,
    status: Option<String>,
}

/// Loads a booking the user may manage. `booking_api` already limits it to
/// the guest, the listing's host and admins; hosts additionally must own
/// the listing here. Anything else is a 404.
pub async fn managed_booking(
    cx: &Cx,
    user: &AdminSession,
    raw_id: &str,
) -> Result<BookingResponse> {
    let id = Uuid::parse_str(raw_id).map_err(|_| not_found())?;
    let token = service_token(cx, user)?;
    let booking = get_api_client(cx)
        .get_booking_as(id, &token)
        .await
        .map_err(|_| not_found())?;
    if user.role == AdminRole::Host {
        let owns = scoped_listings(cx, user)
            .await
            .iter()
            .any(|l| l.id == booking.listing_id);
        if !owns {
            tracing::info!(booking_id = %id, "host denied a booking on another listing");
            return Err(not_found().into());
        }
    }
    Ok(booking)
}

/// Bookings in scope, filtered by the query.
async fn filtered(
    cx: &Cx,
    user: &AdminSession,
    q: &ScheduleQuery,
) -> (Vec<BookingResponse>, HashMap<Uuid, ListingResponse>) {
    let listings = scoped_listings(cx, user).await;
    let needle = q.q.clone().unwrap_or_default().to_lowercase();
    let status = q.status.clone().unwrap_or_default();
    let bookings = scoped_bookings(cx, user, &listings)
        .await
        .into_iter()
        .filter(|b| needle.is_empty() || b.confirmation_code.to_lowercase().contains(&needle))
        .filter(|b| status.is_empty() || b.status.eq_ignore_ascii_case(&status))
        .collect();
    (bookings, listings.into_iter().map(|l| (l.id, l)).collect())
}

/// The master booking schedule.
#[page("/admin/bookings")]
pub async fn schedule_page(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let q = parse_query_params::<ScheduleQuery>(cx).unwrap_or_default();
    let (bookings, listings) = filtered(cx, &user, &q).await;
    let status = q.status.clone().unwrap_or_default();
    let input = "h-10 rounded-lg border border-border bg-card px-3 text-sm";
    Ok(view! {
        <div class="space-y-6">
            <div class="flex flex-wrap items-end justify-between gap-4">
                <div>
                    <h1 class="text-2xl font-semibold">"Master Booking Schedule"</h1>
                    <p class="text-sm text-muted-foreground">(format!("{} bookings", bookings.len()))</p>
                </div>
                <form method="get" action="/admin/bookings/export.csv">
                    if let Some(code) = q.q.clone() { <input type="hidden" name="q" value=(code) /> }
                    if !status.is_empty() { <input type="hidden" name="status" value=(status.clone()) /> }
                    <button type="submit" class="h-10 rounded-lg border border-border px-4 text-sm">"Export CSV"</button>
                </form>
            </div>
            <form method="get" action="/admin/bookings" role="search" class="flex flex-wrap gap-2">
                <label for="booking-search" class="sr-only">"Search by reservation code"</label>
                <input id="booking-search" type="search" name="q" value=(q.q.clone().unwrap_or_default()) placeholder="Search reservation code" class=(format!("{input} w-64")) />
                <label for="booking-status" class="sr-only">"Status"</label>
                <select id="booking-status" name="status" class=(input)>
                    <option value="" selected=(status.is_empty())>"All statuses"</option>
                    for s in STATUSES {
                        <option value=(s) selected=(status == s)>(s)</option>
                    }
                </select>
                <button type="submit" class="h-10 rounded-lg border border-border px-4 text-sm">"Filter"</button>
            </form>
            <div class="overflow-x-auto rounded-xl border border-border">
                <table id="admin-bookings-table" class="w-full text-sm">
                    <thead class="bg-card text-left text-xs text-muted-foreground uppercase">
                        <tr>
                            <th scope="col" class="p-3">"Reservation"</th><th scope="col" class="p-3">"Listing"</th><th scope="col" class="p-3">"Stay"</th>
                            <th scope="col" class="p-3">"Status"</th><th scope="col" class="p-3 text-right">"Total"</th><th scope="col" class="p-3 text-right">"Actions"</th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-border">
                        if bookings.is_empty() {
                            <tr><td colspan="6" class="p-6 text-center text-muted-foreground">"No bookings match."</td></tr>
                        }
                        for b in bookings {
                            let listing = listings.get(&b.listing_id).map(|l| l.name.clone()).unwrap_or_default();
                            <tr id=(format!("booking-row-{}", b.id))>
                                <td class="p-3 font-mono">(b.confirmation_code.clone())</td>
                                <td class="p-3">(listing)</td>
                                <td class="p-3 whitespace-nowrap">(format!("{} → {}", b.date_from, b.date_to))</td>
                                <td class="p-3 capitalize">(b.status.to_lowercase())</td>
                                <td class="p-3 text-right tabular-nums">(money(&b.currency, b.total_price))</td>
                                <td class="p-3 text-right whitespace-nowrap">
                                    detail::details_button(booking_id: b.id, door_code: b.door_access_code.clone().unwrap_or_default())
                                    <a href=(format!("/admin/bookings/{}/messages", b.confirmation_code)) class="ml-3">"Messages"</a>
                                </td>
                            </tr>
                        }
                    </tbody>
                </table>
            </div>
            <div id="booking-dialog-slot"></div>
        </div>
    })
}

/// Escapes one CSV cell, and neutralises spreadsheet formulas
/// (`=`, `+`, `-`, `@` prefixes) so exported data cannot run in Excel/Sheets.
pub fn csv_cell(value: &str) -> String {
    let guarded = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_string()
    };
    format!("\"{}\"", guarded.replace('"', "\"\""))
}

/// The schedule as CSV (same scope and filters as the page).
#[route(GET "/admin/bookings/export.csv")]
pub async fn export_csv(cx: &Cx) -> Result<Response> {
    let user = staff(cx).await?;
    let q = parse_query_params::<ScheduleQuery>(cx).unwrap_or_default();
    let (bookings, listings) = filtered(cx, &user, &q).await;
    let mut csv = String::from(
        "confirmation_code,listing,check_in,check_out,guests,status,currency,total\r\n",
    );
    for b in bookings {
        let listing = listings
            .get(&b.listing_id)
            .map(|l| l.name.clone())
            .unwrap_or_default();
        let row = [
            b.confirmation_code.clone(),
            listing,
            b.date_from.to_string(),
            b.date_to.to_string(),
            b.number_of_persons.to_string(),
            b.status.to_lowercase(),
            b.currency.clone(),
            format!("{:.2}", b.total_price),
        ];
        csv.push_str(
            &row.iter()
                .map(|v| csv_cell(v))
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push_str("\r\n");
    }
    topcoat::router::response::Response::builder()
        .header("content-type", "text/csv; charset=utf-8")
        .header(
            "content-disposition",
            "attachment; filename=\"bookings.csv\"",
        )
        .body(csv.into())
        .map_err(|e| web_client::app_error(e).into())
}

#[cfg(test)]
mod tests {
    use super::csv_cell;

    #[test]
    fn csv_cells_are_quoted_and_formula_safe() {
        assert_eq!(csv_cell("Villa \"A\""), "\"Villa \"\"A\"\"\"");
        assert_eq!(csv_cell("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(csv_cell("-1"), "\"'-1\"");
        assert_eq!(csv_cell("ABC123"), "\"ABC123\"");
    }
}
