use chrono::NaiveDate;
use common::models::{
    BookingResponse, CreateReservationChangeRequest, ReservationChangeQuoteRequest,
    ReservationChangeQuoteResponse, RespondReservationChangeRequest,
};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, error::see_other, page, path_param},
    view::{View, component, view},
};
use web_client::get_api_client;
use web_ui_common::components::domain::price_breakdown::money;

use crate::support::{guest, owned_booking, parse_booking_id, service_token};

path_param!(id);
path_param!(request_id);

/// Change-dates form fields.
#[derive(Deserialize, Clone, Default)]
pub struct ChangeForm {
    /// Proposed check-in.
    pub check_in: Option<NaiveDate>,
    /// Proposed check-out.
    pub check_out: Option<NaiveDate>,
    /// Adults.
    #[serde(default)]
    pub adults: Option<u32>,
    /// Children.
    #[serde(default)]
    pub children: Option<u32>,
    /// Infants.
    #[serde(default)]
    pub infants: Option<u32>,
    /// Pets.
    #[serde(default)]
    pub pets: Option<u32>,
    /// Optional note for the host.
    #[serde(default)]
    pub reason: String,
}

impl ChangeForm {
    /// The dates, if check-out is after check-in.
    fn dates(&self) -> std::result::Result<(NaiveDate, NaiveDate), &'static str> {
        match (self.check_in, self.check_out) {
            (Some(a), Some(b)) if b > a => Ok((a, b)),
            (Some(_), Some(_)) => Err("Check-out date must be after check-in date."),
            _ => Err("Please select both check-in and check-out dates."),
        }
    }
}

#[component]
async fn change_page(
    booking: BookingResponse,
    form: ChangeForm,
    error: Option<String>,
) -> Result<impl View> {
    let id = booking.id;
    let label = "block text-xs font-semibold tracking-wide text-muted-foreground uppercase";
    let input = "mt-1 block h-11 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground normal-case tracking-normal font-normal";
    let check_in = form.check_in.unwrap_or(booking.date_from).to_string();
    let check_out = form.check_out.unwrap_or(booking.date_to).to_string();
    let adults = form.adults.unwrap_or(1).max(1).to_string();
    let count = |v: Option<u32>| v.unwrap_or(0).to_string();
    Ok(view! {
        <div class="mx-auto max-w-2xl space-y-6">
            <div class="space-y-1">
                <p class="text-sm text-muted-foreground">(format!("Reservation #{}", booking.confirmation_code))</p>
                <h1 class="font-serif text-[clamp(28px,4vw,40px)]">"Modify Reservation (Dates & Guests)"</h1>
                <p class="text-sm text-muted-foreground">(format!("Currently {} → {}, {} guest(s). Your host approves changes.", booking.date_from.format("%a %-d %b %Y"), booking.date_to.format("%a %-d %b %Y"), booking.number_of_persons))</p>
            </div>
            if let Some(error) = error {
                <div id="date-change-error" role="alert" class="rounded-lg border border-destructive p-3 text-sm text-destructive">(error)</div>
            }
            <form id="date-change-form" method="post" action=(format!("/bookings/{id}/change-requests")) class="space-y-4 rounded-[var(--radius-surface,16px)] border border-border bg-card p-6">
                <div class="grid gap-4 sm:grid-cols-2">
                    <label class=(label) for="new-check-in">"New check-in"<input id="new-check-in" type="date" name="check_in" value=(check_in) required=(true) class=(input) /></label>
                    <label class=(label) for="new-check-out">"New check-out"<input id="new-check-out" type="date" name="check_out" value=(check_out) required=(true) class=(input) /></label>
                </div>
                <div class="grid grid-cols-2 gap-4 sm:grid-cols-4">
                    <label class=(label) for="new-adults">"Adults"<input id="new-adults" type="number" name="adults" min="1" value=(adults) class=(input) /></label>
                    <label class=(label) for="new-children">"Children"<input id="new-children" type="number" name="children" min="0" value=(count(form.children)) class=(input) /></label>
                    <label class=(label) for="new-infants">"Infants"<input id="new-infants" type="number" name="infants" min="0" value=(count(form.infants)) class=(input) /></label>
                    <label class=(label) for="new-pets">"Pets"<input id="new-pets" type="number" name="pets" min="0" value=(count(form.pets)) class=(input) /></label>
                </div>
                <label class=(label) for="change-reason">"Note for your host (optional)"
                    <textarea id="change-reason" name="reason" rows="2" maxlength="1000" class=(format!("{input} h-auto py-2"))>(form.reason.clone())</textarea>
                </label>
                <div id="change-quote-preview" hidden=(true)></div>
                <div class="flex flex-wrap gap-2">
                    <button id="btn-preview-change-quote" type="button" hx-post=(format!("/bookings/{id}/change-quote")) hx-include="#date-change-form" hx-target="#change-quote-preview" hx-swap="outerHTML" class="h-11 rounded-full border border-border px-5 text-sm font-medium">"Preview new price"</button>
                    <button id="btn-submit-date-change" type="submit" class="h-11 rounded-full bg-primary px-5 text-sm font-medium text-primary-foreground">"Request reservation change"</button>
                </div>
            </form>
        </div>
    })
}

/// The change-dates page for one of the guest's bookings.
#[page("/bookings/{id}/change-dates")]
pub async fn change_dates_page(cx: &Cx) -> Result<impl View> {
    let id = parse_booking_id(path_param::<Id>(cx))?;
    let guest = guest(cx).await?;
    let booking = owned_booking(cx, &guest, id).await?;
    Ok(view! { change_page(booking: booking, form: ChangeForm::default(), error: None) })
}

#[component]
async fn quote_preview(
    quote: Option<ReservationChangeQuoteResponse>,
    error: Option<String>,
) -> Result<impl View> {
    Ok(view! {
        <div id="change-quote-preview" aria-live="polite" class="space-y-1 rounded-lg border border-border p-4 text-sm">
            if let Some(q) = quote {
                <p class="font-medium">(format!("{} → {}, {} guest(s)", q.proposed_date_from.format("%a %-d %b %Y"), q.proposed_date_to.format("%a %-d %b %Y"), q.proposed_number_of_persons))</p>
                <p>"New total: " (money(&q.currency, q.proposed_total_price))</p>
                <p class="text-muted-foreground">"Balance due: " (money(&q.currency, q.balance_due))</p>
            } else if let Some(error) = error {
                <p role="alert" class="text-destructive">(error)</p>
            }
        </div>
    })
}

/// Price preview for a proposed change.
#[page(POST "/bookings/{id}/change-quote")]
pub async fn change_quote(
    cx: &Cx,
    /// The current change-dates form.
    Form(form): Form<ChangeForm>,
) -> Result<impl View> {
    let id = parse_booking_id(path_param::<Id>(cx))?;
    let guest = guest(cx).await?;
    owned_booking(cx, &guest, id).await?;
    let (quote, error) = match form.dates() {
        Err(message) => (None, Some(message.to_string())),
        Ok((date_from, date_to)) => {
            let token = service_token(cx, &guest)?;
            let request = ReservationChangeQuoteRequest {
                date_from: Some(date_from),
                date_to: Some(date_to),
                num_adults: form.adults,
                num_children: form.children,
                num_infants: form.infants,
                num_pets: form.pets,
            };
            match get_api_client(cx)
                .quote_reservation_change(id, &request, Some(&token))
                .await
            {
                Ok(q) => (Some(q), None),
                Err(e) => {
                    tracing::info!("change quote rejected: {e}");
                    (
                        None,
                        Some(
                            "Those dates can't be quoted. Check availability and the stay rules."
                                .to_string(),
                        ),
                    )
                }
            }
        }
    };
    Ok(view! { quote_preview(quote: quote, error: error) })
}

/// Files the guest's change request; the host approves or declines it.
#[page(POST "/bookings/{id}/change-requests")]
pub async fn create_change_request(
    cx: &Cx,
    /// The change-dates form.
    Form(form): Form<ChangeForm>,
) -> Result<impl View> {
    let id = parse_booking_id(path_param::<Id>(cx))?;
    let guest = guest(cx).await?;
    let booking = owned_booking(cx, &guest, id).await?;
    let error = match form.dates() {
        Err(message) => message.to_string(),
        Ok((date_from, date_to)) => {
            let token = service_token(cx, &guest)?;
            let request = CreateReservationChangeRequest {
                date_from: Some(date_from),
                date_to: Some(date_to),
                num_adults: form.adults,
                num_children: form.children,
                num_infants: form.infants,
                num_pets: form.pets,
                reason: Some(form.reason.trim().to_string()).filter(|r| !r.is_empty()),
            };
            match get_api_client(cx)
                .create_reservation_change_request(id, &request, Some(&token))
                .await
            {
                Ok(_) => return Err(see_other("/bookings").into()),
                Err(e) => {
                    tracing::info!("change request rejected: {e}");
                    "We couldn't submit that change. Check availability and the stay rules."
                        .to_string()
                }
            }
        }
    };
    Ok(view! { change_page(booking: booking, form: form, error: Some(error)) })
}

/// What a guest can do with a pending change request.
#[derive(Clone, Copy)]
enum Response {
    Approve,
    Decline,
    Withdraw,
}

async fn respond(cx: &Cx, action: Response) -> Result<()> {
    let id = parse_booking_id(path_param::<Id>(cx))?;
    let request_id = parse_booking_id(path_param::<RequestId>(cx))?;
    let guest = guest(cx).await?;
    owned_booking(cx, &guest, id).await?;
    let token = service_token(cx, &guest)?;
    let api = get_api_client(cx);
    let note = RespondReservationChangeRequest::default();
    let result = match action {
        Response::Approve => {
            api.approve_reservation_change_request(id, request_id, &note, Some(&token))
                .await
        }
        Response::Decline => {
            api.decline_reservation_change_request(id, request_id, &note, Some(&token))
                .await
        }
        Response::Withdraw => {
            api.cancel_reservation_change_request(id, request_id, Some(&token))
                .await
        }
    };
    result.map_err(web_client::app_error)?;
    Err(see_other("/bookings").into())
}

/// Approves a change the host proposed.
#[page(POST "/bookings/{id}/change-requests/{request_id}/approve")]
pub async fn approve_change(cx: &Cx) -> Result<()> {
    respond(cx, Response::Approve).await
}

/// Declines a change the host proposed.
#[page(POST "/bookings/{id}/change-requests/{request_id}/decline")]
pub async fn decline_change(cx: &Cx) -> Result<()> {
    respond(cx, Response::Decline).await
}

/// Withdraws the guest's own change request.
#[page(POST "/bookings/{id}/change-requests/{request_id}/cancel")]
pub async fn withdraw_change(cx: &Cx) -> Result<()> {
    respond(cx, Response::Withdraw).await
}
