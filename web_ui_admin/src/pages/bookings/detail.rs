//! The booking-details dialog ("audit view") and its actions.

use common::models::{BookingResponse, UpdatedBookingRequest};
use common::tax_document::TaxDocumentResponse;
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, error::not_found, page, path_param},
    view::{View, component, view},
};
use uuid::Uuid;
use web_client::{get_api_client, session::AdminRole};
use web_ui_common::components::domain::{
    price_breakdown::money, tax_document_view::tax_document_view,
};

use super::{STATUSES, managed_booking};
use crate::support::{admin_only, service_token, staff};

path_param!(id);
path_param!(number);

/// Door code form.
#[derive(Deserialize)]
pub struct DoorCodeForm {
    /// The keyless entry code (empty clears it).
    #[serde(default)]
    pub door_access_code: String,
}

/// Status form.
#[derive(Deserialize)]
pub struct StatusForm {
    /// The new booking status.
    pub status: String,
}

/// The row's Details button. `data-door-code` mirrors the saved code, and
/// the door-code action swaps a fresh copy in out-of-band.
#[component]
pub async fn details_button(
    /// The booking.
    booking_id: Uuid,
    /// Saved door code (empty if none).
    door_code: String,
    /// Set when sent as an out-of-band swap.
    #[default]
    oob: bool,
) -> Result<impl View> {
    Ok(view! {
        <button
            id=(format!("btn-details-{booking_id}"))
            type="button"
            data-door-code=(door_code)
            hx-get=(format!("/admin/bookings/{booking_id}"))
            hx-target="#booking-dialog-slot"
            hx-swap="innerHTML"
            hx-swap-oob=(oob.then_some("true"))
            class="font-medium"
        >"Details"</button>
    })
}

/// The door-code panel inside the dialog.
#[component]
async fn door_code_panel(
    booking_id: Uuid,
    code: String,
    message: Option<String>,
) -> Result<impl View> {
    let configured = !code.is_empty();
    let action = format!("/admin/bookings/{booking_id}/door-code");
    Ok(view! {
        <section id="door-code-panel" class="space-y-2 rounded-lg border border-border p-4">
            <div class="flex items-center justify-between">
                <h4 class="text-sm font-semibold">"Door Access Code"</h4>
                <span id="detail-door-code-badge" data-state=(if configured { "configured" } else { "unset" }) class="rounded-full border border-border px-2 py-0.5 text-xs">
                    (if configured { format!("Configured ({code})") } else { "Not set".to_string() })
                </span>
            </div>
            <form method="post" action=(action.clone()) hx-post=(action) hx-target="#door-code-panel" hx-swap="outerHTML" class="flex gap-2">
                <label for="detail-door-code-input" class="sr-only">"Door access code"</label>
                <input id="detail-door-code-input" name="door_access_code" value=(code.clone()) maxlength="32" autocomplete="off" class="h-10 flex-1 rounded-lg border border-border bg-card px-3 font-mono text-sm" />
                <button id="btn-save-door-code" type="submit" class="h-10 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground">"Save code"</button>
            </form>
            if let Some(message) = message {
                <p id="door-code-status-msg" role="status" class="text-sm">(message)</p>
            }
        </section>
    })
}

/// Booking + its documents, as the user sees them.
#[derive(Clone)]
struct DialogData {
    booking: BookingResponse,
    documents: Vec<TaxDocumentResponse>,
    show_quote: bool,
}

#[component]
async fn dialog_view(data: DialogData) -> Result<impl View> {
    let b = data.booking;
    let id = b.id;
    let cur = b.currency.clone();
    let rate = b
        .exchange_rate
        .map(|r| format!("{r:.4}"))
        .unwrap_or_else(|| "—".into());
    let status_action = format!("/admin/bookings/{id}/status");
    let current = b.status.to_lowercase();
    let code = b.door_access_code.clone().unwrap_or_default();
    let doc_count = data.documents.len();
    Ok(view! {
        <dialog id="admin-booking-details-dialog" data-auto-open="" aria-labelledby="booking-dialog-title" class="m-auto max-h-[90vh] w-[min(760px,95vw)] overflow-y-auto rounded-xl bg-card p-6 text-card-foreground backdrop:bg-black/50">
            <div class="space-y-5">
                <div class="flex items-start justify-between gap-4">
                    <div>
                        <h2 id="booking-dialog-title" class="text-xl font-semibold">(format!("Reservation {}", b.confirmation_code))</h2>
                        <p class="text-sm text-muted-foreground">(format!("{} → {} · {} guest(s) · {}", b.date_from, b.date_to, b.number_of_persons, current))</p>
                    </div>
                    <button type="button" data-dialog-close="" class="h-10 rounded-lg border border-border px-4 text-sm">"Close Audit View"</button>
                </div>

                <section class="space-y-2 rounded-lg border border-border p-4">
                    <h3 class="text-sm font-semibold">"Statutory Tri-Currency Financial Audit"</h3>
                    <dl class="grid grid-cols-2 gap-x-6 gap-y-1 text-sm">
                        <dt>"Subtotal"</dt><dd id="detail-subtotal" class="text-right tabular-nums">(money(&cur, b.sub_total_price))</dd>
                        <dt>"Discount"</dt><dd class="text-right tabular-nums">(b.discount_value.map(|d| money(&cur, d)).unwrap_or_else(|| "—".into()))</dd>
                        <dt>"Cleaning fee"</dt><dd class="text-right tabular-nums">(b.cleaning_fee.map(|d| money(&cur, d)).unwrap_or_else(|| "—".into()))</dd>
                        <dt>"Jamaican GCT"</dt><dd id="detail-tax" class="text-right tabular-nums">(b.tax_value.map(|d| money(&cur, d)).unwrap_or_else(|| money(&cur, rust_decimal::Decimal::ZERO)))</dd>
                        <dt>"Platform fee"</dt><dd class="text-right tabular-nums">(b.platform_fee.map(|d| money(&cur, d)).unwrap_or_else(|| "—".into()))</dd>
                        <dt>"Damage deposit"</dt><dd class="text-right tabular-nums">(b.damage_deposit.map(|d| money(&cur, d)).unwrap_or_else(|| "—".into()))</dd>
                        <dt class="font-semibold">"Total"</dt><dd id="detail-total" class="text-right font-semibold tabular-nums">(money(&cur, b.total_price))</dd>
                        <dt class="text-muted-foreground">"Exchange rate at booking"</dt><dd class="text-right text-muted-foreground">(rate)</dd>
                    </dl>
                </section>

                door_code_panel(booking_id: id, code: code, message: None)

                <section class="space-y-2 rounded-lg border border-border p-4">
                    <h4 class="text-sm font-semibold">"Status"</h4>
                    <form method="post" action=(status_action) class="flex gap-2">
                        <label for="detail-status" class="sr-only">"Booking status"</label>
                        <select id="detail-status" name="status" class="h-10 rounded-lg border border-border bg-card px-3 text-sm">
                            for s in STATUSES {
                                <option value=(s) selected=(current == s)>(s)</option>
                            }
                        </select>
                        <button type="submit" class="h-10 rounded-lg border border-border px-4 text-sm">"Update status"</button>
                    </form>
                    if data.show_quote {
                        <button type="button" hx-get=(format!("/admin/bookings/{id}/cancellation-quote")) hx-target="#admin-quote-slot" hx-swap="innerHTML" class="text-sm font-medium">"Show refund quote"</button>
                        <div id="admin-quote-slot"></div>
                    }
                </section>

                <section id="admin-booking-documents-section" class="space-y-2 rounded-lg border border-border p-4">
                    <div class="flex items-center justify-between">
                        <h4 class="text-sm font-semibold">"Statutory tax documents"</h4>
                        <span id="detail-docs-count-badge" class="rounded-full border border-border px-2 py-0.5 text-xs">(format!("{doc_count} document(s)"))</span>
                    </div>
                    <ul id="admin-booking-documents-list" class="space-y-1 text-sm">
                        if data.documents.is_empty() {
                            <li class="text-muted-foreground">"No documents issued yet."</li>
                        }
                        for d in data.documents {
                            <li><a href=(format!("/admin/bookings/{id}/documents/{}", d.document_number))>(d.document_number.clone())</a> " · " (format!("{:?}", d.status))</li>
                        }
                    </ul>
                </section>
            </div>
        </dialog>
    })
}

/// The booking-details dialog fragment.
#[page("/admin/bookings/{id}")]
pub async fn booking_dialog(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let booking = managed_booking(cx, &user, path_param::<Id>(cx)).await?;
    let token = service_token(cx, &user)?;
    let documents = get_api_client(cx)
        .list_booking_documents(booking.id, Some(&token))
        .await
        .unwrap_or_default();
    let data = DialogData {
        booking,
        documents,
        show_quote: user.role == AdminRole::Admin,
    };
    Ok(view! { dialog_view(data: data) })
}

/// Sets or clears the keyless door code (hosts of the listing and admins).
#[page(POST "/admin/bookings/{id}/door-code")]
pub async fn save_door_code(
    cx: &Cx,
    /// The door code form.
    Form(form): Form<DoorCodeForm>,
) -> Result<impl View> {
    let user = staff(cx).await?;
    let booking = managed_booking(cx, &user, path_param::<Id>(cx)).await?;
    let token = service_token(cx, &user)?;
    let code: String = form.door_access_code.trim().chars().take(32).collect();
    let request = UpdatedBookingRequest {
        door_access_code: Some(code.clone()),
        ..Default::default()
    };
    let (saved, message) = match get_api_client(cx)
        .update_booking_with_auth(booking.id, &request, Some(&token))
        .await
    {
        Ok(updated) => (
            updated.door_access_code.unwrap_or_default(),
            "Door access code updated successfully.".to_string(),
        ),
        Err(e) => {
            tracing::info!("door code rejected: {e}");
            (
                booking.door_access_code.clone().unwrap_or_default(),
                "The door code could not be saved.".to_string(),
            )
        }
    };
    let id = booking.id;
    let oob_code = saved.clone();
    Ok(view! {
        door_code_panel(booking_id: id, code: saved, message: Some(message))
        details_button(booking_id: id, door_code: oob_code, oob: true)
    })
}

/// Changes the booking status (hosts of the listing and admins), then
/// returns to the schedule.
#[page(POST "/admin/bookings/{id}/status")]
pub async fn update_status(
    cx: &Cx,
    /// The status form.
    Form(form): Form<StatusForm>,
) -> Result<()> {
    let user = staff(cx).await?;
    let booking = managed_booking(cx, &user, path_param::<Id>(cx)).await?;
    if !STATUSES.contains(&form.status.as_str()) {
        return Err(web_client::app_error("unknown status").into());
    }
    let token = service_token(cx, &user)?;
    let request = UpdatedBookingRequest {
        status: Some(form.status.clone()),
        ..Default::default()
    };
    get_api_client(cx)
        .update_booking_with_auth(booking.id, &request, Some(&token))
        .await
        .map_err(web_client::app_error)?;
    tracing::info!(booking_id = %booking.id, status = %form.status, "booking status changed");
    Err(topcoat::router::error::see_other("/admin/bookings").into())
}

/// Refund quote for a booking: administrators only (decision D8).
#[page("/admin/bookings/{id}/cancellation-quote")]
pub async fn admin_cancellation_quote(cx: &Cx) -> Result<impl View> {
    let user = admin_only(cx).await?;
    let booking = managed_booking(cx, &user, path_param::<Id>(cx)).await?;
    let token = service_token(cx, &user)?;
    let quote = get_api_client(cx)
        .get_cancellation_quote_as(booking.id, &token)
        .await
        .map_err(web_client::app_error)?;
    let cur = booking.currency;
    let pct = (quote.policy_pct * rust_decimal::Decimal::ONE_HUNDRED).round();
    Ok(view! {
        <dl id="admin-cancellation-quote" class="mt-2 space-y-1 text-sm">
            <div class="flex justify-between"><dt>"Refund schedule"</dt><dd>(format!("{pct}% of accommodation"))</dd></div>
            <div class="flex justify-between font-medium"><dt>"Refund total"</dt><dd>(money(&cur, quote.refund_total))</dd></div>
            <div class="flex justify-between text-muted-foreground"><dt>"Retained"</dt><dd>(money(&cur, quote.retained_total))</dd></div>
        </dl>
    })
}

/// A booking's tax document, for its host or an admin.
#[page("/admin/bookings/{id}/documents/{number}")]
pub async fn admin_document(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let booking = managed_booking(cx, &user, path_param::<Id>(cx)).await?;
    let token = service_token(cx, &user)?;
    let number = path_param::<Number>(cx).to_string();
    let doc = get_api_client(cx)
        .get_booking_document_by_number(booking.id, &number, Some(&token))
        .await
        .map_err(|_| not_found())?;
    Ok(view! {
        <div class="mx-auto max-w-4xl space-y-4">
            <div class="no-print flex justify-between">
                <a href="/admin/bookings" class="text-sm">"← Bookings"</a>
                <button type="button" data-print="" class="h-10 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground">"Print / Download PDF"</button>
            </div>
            tax_document_view(doc: doc)
        </div>
    })
}
