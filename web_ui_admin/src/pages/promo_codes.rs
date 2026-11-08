//! Promo codes (#106). Admins manage every code; hosts manage their own, for their
//! own listings. The API enforces this too. Code strings are never logged.

use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use common::models::ListingResponse;
use common::promo::{
    CreatePromoCodeRequest, PromoCodeResponse, PromoDiscountType, PromoScope, validate_new_code,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, error::see_other, page, path_param},
    view::{View, component, view},
};
use uuid::Uuid;
use web_client::client::UserCallError;
use web_client::get_api_client;

use crate::support::{scoped_listings, service_token, staff};

path_param!(id);

/// This page.
const PAGE: &str = "/admin/promo-codes";
/// Prefix of a listing checkbox's field name. Each checkbox has its own field,
/// because form decoding doesn't collect repeated keys.
const LISTING_FIELD: &str = "listing:";
/// Jamaica's fixed offset from UTC, in hours. It has no daylight saving.
const JAMAICA_UTC_OFFSET_HOURS: i64 = 5;

/// The create form, kept as typed so it can be shown again after an error.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PromoCodeForm {
    /// The code as typed.
    #[serde(default)]
    pub code: String,
    /// `percentage` or `fixed`.
    #[serde(default)]
    pub discount_type: String,
    /// The percentage or amount.
    #[serde(default)]
    pub discount_value: String,
    /// The stated currency.
    #[serde(default)]
    pub currency: String,
    /// `stay` or `stay_and_cleaning`.
    #[serde(default)]
    pub scope: String,
    /// First valid Jamaica calendar day.
    #[serde(default)]
    pub valid_from: String,
    /// Last valid Jamaica calendar day.
    #[serde(default)]
    pub valid_to: String,
    /// First eligible night.
    #[serde(default)]
    pub stay_from: String,
    /// Last eligible night.
    #[serde(default)]
    pub stay_to: String,
    /// Fewest nights.
    #[serde(default)]
    pub min_nights: String,
    /// Least eligible amount.
    #[serde(default)]
    pub min_booking_value: String,
    /// Most uses in total.
    #[serde(default)]
    pub total_usage_limit: String,
    /// Most uses per guest.
    #[serde(default)]
    pub per_guest_limit: String,
    /// `all` or `selected`.
    #[serde(default)]
    pub applies_to: String,
    /// The ticked listing checkboxes, and anything else submitted.
    #[serde(flatten)]
    pub rest: HashMap<String, String>,
}

impl PromoCodeForm {
    fn is_selected(&self, listing_id: Uuid) -> bool {
        self.rest
            .contains_key(&format!("{LISTING_FIELD}{listing_id}"))
    }

    fn selected_listings(&self) -> Result<Vec<Uuid>, String> {
        self.rest
            .keys()
            .filter_map(|key| key.strip_prefix(LISTING_FIELD))
            .map(|id| {
                Uuid::parse_str(id).map_err(|_| "A selected listing wasn't recognised.".to_string())
            })
            .collect()
    }
}

/// The start of a Jamaica calendar day.
fn jamaica_day_start(day: NaiveDate) -> DateTime<Utc> {
    (day.and_time(NaiveTime::MIN) + Duration::hours(JAMAICA_UTC_OFFSET_HOURS)).and_utc()
}

fn optional<T: FromStr>(raw: &str, invalid: &str) -> Result<Option<T>, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    raw.parse().map(Some).map_err(|_| invalid.to_string())
}

/// Reads the form into a request. Validity runs from the start of the first
/// Jamaica day to the end of the last. The rules themselves are checked by
/// [`validate_new_code`], here and again by the API.
pub fn to_request(form: &PromoCodeForm) -> Result<CreatePromoCodeRequest, String> {
    let discount_type = if form.discount_type == "fixed" {
        PromoDiscountType::Fixed
    } else {
        PromoDiscountType::Percentage
    };
    let scope = if form.scope == "stay_and_cleaning" {
        PromoScope::StayAndCleaning
    } else {
        PromoScope::Stay
    };
    let discount_value: Decimal = form
        .discount_value
        .trim()
        .parse()
        .map_err(|_| "Enter the discount as a number.".to_string())?;
    let day = |raw: &str, name: &str| -> Result<NaiveDate, String> {
        raw.trim()
            .parse()
            .map_err(|_| format!("Choose the {name} date."))
    };
    let valid_from = jamaica_day_start(day(&form.valid_from, "first valid")?);
    let valid_to = jamaica_day_start(day(&form.valid_to, "last valid")? + Duration::days(1));
    let currency = form.currency.trim().to_ascii_uppercase();
    let applies_to_all = form.applies_to != "selected";
    let request = CreatePromoCodeRequest {
        code: form.code.clone(),
        discount_type,
        discount_value,
        currency: (!currency.is_empty()).then_some(currency),
        scope,
        valid_from,
        valid_to,
        stay_from: optional(&form.stay_from, "Enter the stay window as dates.")?,
        stay_to: optional(&form.stay_to, "Enter the stay window as dates.")?,
        min_nights: optional(&form.min_nights, "Enter minimum nights as a whole number.")?,
        min_booking_value: optional(
            &form.min_booking_value,
            "Enter the minimum value as a number.",
        )?,
        total_usage_limit: optional(
            &form.total_usage_limit,
            "Enter the total limit as a whole number.",
        )?,
        per_guest_limit: optional(
            &form.per_guest_limit,
            "Enter the per-guest limit as a whole number.",
        )?,
        applies_to_all,
        listing_ids: if applies_to_all {
            Vec::new()
        } else {
            form.selected_listings()?
        },
    };
    validate_new_code(&request)?;
    Ok(request)
}

fn describe(err: &UserCallError) -> String {
    match err {
        UserCallError::Api(r) => r.message.clone(),
        UserCallError::Unauthorized => "You can't manage that promo code.".to_string(),
        other => {
            tracing::warn!(error = %other, "promo code change failed");
            "The change could not be saved. Try again shortly.".to_string()
        }
    }
}

fn discount_text(code: &PromoCodeResponse) -> String {
    match code.discount_type {
        PromoDiscountType::Percentage => format!("{}%", code.discount_value.normalize()),
        PromoDiscountType::Fixed => common::money::total_amount(
            code.currency.as_deref().unwrap_or_default(),
            code.discount_value,
        ),
    }
}

fn status_text(code: &PromoCodeResponse, now: DateTime<Utc>) -> &'static str {
    if code.deactivated_at.is_some() {
        "Deactivated"
    } else if now < code.valid_from {
        "Scheduled"
    } else if now >= code.valid_to {
        "Expired"
    } else {
        "Active"
    }
}

fn listings_text(code: &PromoCodeResponse) -> String {
    if code.applies_to_all {
        "All".to_string()
    } else {
        format!("{} selected", code.listing_ids.len())
    }
}

/// Every code the user manages, with its usage and a deactivate action.
#[component]
pub async fn promo_codes_card(
    /// The codes to list.
    codes: Vec<PromoCodeResponse>,
    /// Errors to show above the list.
    errors: Vec<String>,
) -> Result<impl View> {
    let button = "h-9 rounded-lg border border-border px-3 text-sm font-medium";
    let now = Utc::now();
    Ok(view! {
        <section id="promo-codes-card" class="space-y-4 rounded-xl border border-border p-6">
            for message in errors {
                <p role="alert" class="rounded-lg border border-destructive p-3 text-sm text-destructive">(message)</p>
            }
            <div class="overflow-x-auto">
                <table id="promo-codes-table" class="w-full text-sm">
                    <thead class="text-left text-xs text-muted-foreground uppercase">
                        <tr>
                            <th scope="col" class="py-2">"Code"</th><th scope="col" class="py-2">"Discount"</th><th scope="col" class="py-2">"Valid"</th>
                            <th scope="col" class="py-2">"Listings"</th><th scope="col" class="py-2">"Status"</th>
                            <th scope="col" class="py-2 text-right">"Active holds"</th><th scope="col" class="py-2 text-right">"Confirmed"</th><th scope="col" class="py-2 text-right">"Remaining"</th>
                            <th scope="col" class="py-2"><span class="sr-only">"Actions"</span></th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-border">
                        if codes.is_empty() {
                            <tr><td colspan="9" class="py-6 text-center text-muted-foreground">"No promo codes yet."</td></tr>
                        }
                        for code in codes {
                            let status = status_text(&code, now);
                            let action = format!("{PAGE}/{}/deactivate", code.id);
                            <tr data-promo-code=(code.code.clone())>
                                <td class="py-2 font-mono">(code.code.clone())</td>
                                <td class="py-2">(discount_text(&code))</td>
                                <td class="py-2">(format!("{} to {}", code.valid_from.format("%-d %b %Y"), code.valid_to.format("%-d %b %Y")))</td>
                                <td class="py-2">(listings_text(&code))</td>
                                <td class="py-2" data-promo-status="">(status)</td>
                                <td class="py-2 text-right tabular-nums">(code.active_holds.to_string())</td>
                                <td class="py-2 text-right tabular-nums">(code.confirmed_uses.to_string())</td>
                                <td class="py-2 text-right tabular-nums">(code.remaining_uses.map_or_else(|| "No limit".to_string(), |n| n.to_string()))</td>
                                <td class="py-2 text-right">
                                    if code.deactivated_at.is_none() {
                                        <form method="post" action=(action.clone()) hx-post=(action) hx-target="#promo-codes-card" hx-swap="outerHTML" hx-disabled-elt="find button">
                                            <button type="submit" class=(button)>"Deactivate"</button>
                                        </form>
                                    }
                                </td>
                            </tr>
                        }
                    </tbody>
                </table>
            </div>
        </section>
    })
}

/// The form for creating a code.
#[component]
pub async fn promo_code_form(
    /// Listings the user may select.
    listings: Vec<ListingResponse>,
    /// The form as typed.
    form: PromoCodeForm,
    /// Why the last submission was refused.
    error: Option<String>,
) -> Result<impl View> {
    let label = "text-xs text-muted-foreground";
    let field = "mt-1 block h-9 w-full rounded-lg border border-border bg-card px-2 text-sm text-foreground";
    Ok(view! {
        <form id="promo-code-form" method="post" action=(PAGE) class="space-y-4 rounded-xl border border-border p-6">
            <h2 class="text-lg font-semibold">"Create a promo code"</h2>
            <p class="text-sm text-muted-foreground">"The host funds the discount. It can't be changed after creation: deactivate the code and create a new one."</p>
            if let Some(message) = error {
                <p id="promo-code-error" role="alert" class="rounded-lg border border-destructive p-3 text-sm text-destructive">(message)</p>
            }
            <div class="grid gap-3 sm:grid-cols-3">
                <label class=(label)>"Code"<input id="promo-code" name="code" required=(true) maxlength="32" value=(form.code.clone()) class=(format!("{field} font-mono uppercase")) /></label>
                <label class=(label)>"Discount type"
                    <select id="promo-discount-type" name="discount_type" class=(field)>
                        <option value="percentage" selected=(form.discount_type != "fixed")>"Percentage"</option>
                        <option value="fixed" selected=(form.discount_type == "fixed")>"Fixed amount"</option>
                    </select>
                </label>
                <label class=(label)>"Discount"<input id="promo-discount-value" name="discount_value" required=(true) inputmode="decimal" value=(form.discount_value.clone()) class=(field) /></label>
                <label class=(label)>"Currency (fixed amount or minimum value)"<input id="promo-currency" name="currency" maxlength="3" value=(form.currency.clone()) class=(format!("{field} uppercase")) /></label>
                <label class=(label)>"Applies to"
                    <select id="promo-scope" name="scope" class=(field)>
                        <option value="stay" selected=(form.scope != "stay_and_cleaning")>"The stay"</option>
                        <option value="stay_and_cleaning" selected=(form.scope == "stay_and_cleaning")>"The stay and cleaning fee"</option>
                    </select>
                </label>
                <label class=(label)>"First valid day"<input id="promo-valid-from" type="date" name="valid_from" required=(true) value=(form.valid_from.clone()) class=(field) /></label>
                <label class=(label)>"Last valid day"<input id="promo-valid-to" type="date" name="valid_to" required=(true) value=(form.valid_to.clone()) class=(field) /></label>
                <label class=(label)>"First eligible night (optional)"<input type="date" name="stay_from" value=(form.stay_from.clone()) class=(field) /></label>
                <label class=(label)>"Last eligible night (optional)"<input type="date" name="stay_to" value=(form.stay_to.clone()) class=(field) /></label>
                <label class=(label)>"Minimum nights (optional)"<input type="number" min="1" name="min_nights" value=(form.min_nights.clone()) class=(field) /></label>
                <label class=(label)>"Minimum booking value (optional)"<input name="min_booking_value" inputmode="decimal" value=(form.min_booking_value.clone()) class=(field) /></label>
                <label class=(label)>"Total usage limit (optional)"<input id="promo-total-limit" type="number" min="1" name="total_usage_limit" value=(form.total_usage_limit.clone()) class=(field) /></label>
                <label class=(label)>"Per-guest limit (optional)"<input type="number" min="1" name="per_guest_limit" value=(form.per_guest_limit.clone()) class=(field) /></label>
            </div>
            <fieldset class="space-y-2">
                <legend class=(label)>"Listings"</legend>
                <label class="flex items-center gap-2 text-sm"><input id="promo-applies-all" type="radio" name="applies_to" value="all" checked=(form.applies_to != "selected") />"All, including listings added later"</label>
                <label class="flex items-center gap-2 text-sm"><input id="promo-applies-selected" type="radio" name="applies_to" value="selected" checked=(form.applies_to == "selected") />"Selected listings"</label>
                <div id="promo-listing-picker" class="grid gap-1 sm:grid-cols-2">
                    for listing in listings {
                        <label class="flex items-center gap-2 text-sm"><input type="checkbox" name=(format!("{LISTING_FIELD}{}", listing.id)) value="on" checked=(form.is_selected(listing.id)) />(listing.name.clone())</label>
                    }
                </div>
            </fieldset>
            <button id="promo-code-submit" type="submit" class="h-10 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground">"Create promo code"</button>
        </form>
    })
}

async fn load_codes(cx: &Cx, token: &str) -> (Vec<PromoCodeResponse>, Vec<String>) {
    match get_api_client(cx).list_promo_codes_as(token, 1).await {
        Ok(page) => (page.codes, Vec::new()),
        Err(e) => {
            tracing::error!(error = %e, "could not load promo codes");
            (
                Vec::new(),
                vec!["Promo codes could not be loaded. Try again shortly.".to_string()],
            )
        }
    }
}

/// The whole page.
#[component]
async fn promo_codes_view(
    /// The codes to list.
    codes: Vec<PromoCodeResponse>,
    /// Errors loading the list.
    errors: Vec<String>,
    /// Listings the user may select.
    listings: Vec<ListingResponse>,
    /// The form as typed.
    form: PromoCodeForm,
    /// Why the last submission was refused.
    error: Option<String>,
) -> Result<impl View> {
    Ok(view! {
        <div class="space-y-6">
            <div>
                <h1 class="text-2xl font-semibold">"Promo codes"</h1>
                <p class="text-sm text-muted-foreground">"Codes guests enter at checkout. Deactivating one never changes bookings already made."</p>
            </div>
            promo_codes_card(codes: codes, errors: errors)
            promo_code_form(listings: listings, form: form, error: error)
        </div>
    })
}

/// The promo codes page.
#[page("/admin/promo-codes")]
pub async fn promo_codes_page(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let token = service_token(cx, &user)?;
    let (codes, errors) = load_codes(cx, &token).await;
    let listings = scoped_listings(cx, &user).await;
    let form = PromoCodeForm::default();
    Ok(
        view! { promo_codes_view(codes: codes, errors: errors, listings: listings, form: form, error: None) },
    )
}

/// Creates a code. An error shows the form again, as typed.
#[page(POST "/admin/promo-codes")]
pub async fn create_promo_code(
    cx: &Cx,
    /// The submitted form.
    Form(form): Form<PromoCodeForm>,
) -> Result<impl View> {
    let user = staff(cx).await?;
    let token = service_token(cx, &user)?;
    let outcome = match to_request(&form) {
        Err(message) => Err(message),
        Ok(request) => get_api_client(cx)
            .create_promo_code_as(&token, &request)
            .await
            .map(|created| {
                tracing::info!(promo_code_id = %created.id, actor_id = %user.user_id, "promo code created");
            })
            .map_err(|e| describe(&e)),
    };
    if outcome.is_ok() {
        return Err(see_other(PAGE).into());
    }
    let error = outcome.err();
    let (codes, errors) = load_codes(cx, &token).await;
    let listings = scoped_listings(cx, &user).await;
    Ok(
        view! { promo_codes_view(codes: codes, errors: errors, listings: listings, form: form, error: error) },
    )
}

/// Deactivates a code. Swaps the card, or returns to the page without HTMX.
#[page(POST "/admin/promo-codes/{id}/deactivate")]
pub async fn deactivate_promo_code(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let token = service_token(cx, &user)?;
    let error = match Uuid::parse_str(path_param::<Id>(cx)) {
        Err(_) => Some("Unknown promo code.".to_string()),
        Ok(id) => match get_api_client(cx)
            .deactivate_promo_code_as(&token, id)
            .await
        {
            Ok(_) => {
                tracing::info!(promo_code_id = %id, actor_id = %user.user_id, "promo code deactivated");
                None
            }
            Err(e) => Some(describe(&e)),
        },
    };
    if !topcoat::htmx::hx_request(cx) {
        return Err(see_other(PAGE).into());
    }
    let (codes, mut errors) = load_codes(cx, &token).await;
    errors.extend(error);
    Ok(view! { promo_codes_card(codes: codes, errors: errors) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use topcoat::view::ViewExt;

    async fn html(v: impl View, cx: &Cx) -> String {
        match v.first().await {
            Ok(handle) => handle.render(cx),
            Err(e) => panic!("render failed: {e}"),
        }
    }

    fn form() -> PromoCodeForm {
        PromoCodeForm {
            code: " summer ".to_string(),
            discount_type: "percentage".to_string(),
            discount_value: "10".to_string(),
            scope: "stay".to_string(),
            valid_from: "2030-06-01".to_string(),
            valid_to: "2030-06-30".to_string(),
            applies_to: "all".to_string(),
            ..PromoCodeForm::default()
        }
    }

    fn code(deactivated: bool) -> PromoCodeResponse {
        let start = jamaica_day_start(NaiveDate::from_ymd_opt(2020, 1, 1).expect("valid date"));
        let end = jamaica_day_start(NaiveDate::from_ymd_opt(2040, 1, 1).expect("valid date"));
        PromoCodeResponse {
            id: Uuid::nil(),
            code: "SUMMER".to_string(),
            discount_type: PromoDiscountType::Percentage,
            discount_value: Decimal::new(1000, 2),
            currency: None,
            scope: PromoScope::Stay,
            valid_from: start,
            valid_to: end,
            stay_from: None,
            stay_to: None,
            min_nights: None,
            min_booking_value: None,
            total_usage_limit: Some(5),
            per_guest_limit: None,
            applies_to_all: true,
            listing_ids: vec![],
            owner_host_id: None,
            deactivated_at: deactivated.then_some(start),
            active_holds: 1,
            confirmed_uses: 2,
            remaining_uses: Some(2),
            created_at: start,
        }
    }

    #[test]
    fn promo_codes_validity_covers_whole_jamaica_days() {
        let request = to_request(&form()).expect("a valid form");
        assert_eq!(request.valid_from.to_rfc3339(), "2030-06-01T05:00:00+00:00");
        assert_eq!(request.valid_to.to_rfc3339(), "2030-07-01T05:00:00+00:00");
        assert!(request.applies_to_all);
    }

    #[test]
    fn promo_codes_form_reads_selected_listings_and_optional_fields() {
        let id = Uuid::now_v7();
        let mut rest = HashMap::new();
        rest.insert(format!("{LISTING_FIELD}{id}"), "on".to_string());
        rest.insert("unrelated".to_string(), "x".to_string());
        let request = to_request(&PromoCodeForm {
            applies_to: "selected".to_string(),
            min_nights: "3".to_string(),
            total_usage_limit: "10".to_string(),
            rest,
            ..form()
        })
        .expect("a valid form");
        assert!(!request.applies_to_all);
        assert_eq!(request.listing_ids, vec![id]);
        assert_eq!(request.min_nights, Some(3));
        assert_eq!(request.total_usage_limit, Some(10));
    }

    #[test]
    fn promo_codes_form_refuses_what_the_rules_refuse() {
        for bad in [
            PromoCodeForm {
                discount_value: "ten".to_string(),
                ..form()
            },
            PromoCodeForm {
                discount_value: "33.335".to_string(),
                ..form()
            },
            PromoCodeForm {
                valid_from: String::new(),
                ..form()
            },
            PromoCodeForm {
                min_nights: "two".to_string(),
                ..form()
            },
            PromoCodeForm {
                applies_to: "selected".to_string(),
                ..form()
            },
            PromoCodeForm {
                code: "SUM_MER".to_string(),
                ..form()
            },
        ] {
            assert!(to_request(&bad).is_err(), "{bad:?}");
        }
    }

    #[tokio::test]
    async fn promo_codes_list_shows_usage_and_deactivate_stops_double_submits() {
        let __cx = &Cx::default();
        let out = html(
            view! { promo_codes_card(codes: vec![code(false)], errors: Vec::new()) },
            __cx,
        )
        .await;
        assert!(out.contains(r#"data-promo-code="SUMMER""#), "{out}");
        assert!(out.contains("10%"), "{out}");
        assert!(out.contains("Active"), "{out}");
        assert!(out.contains(r#"hx-disabled-elt="find button""#), "{out}");
        assert!(out.contains("Deactivate"), "{out}");
    }

    #[tokio::test]
    async fn promo_codes_deactivated_code_has_no_action() {
        let __cx = &Cx::default();
        let out = html(
            view! { promo_codes_card(codes: vec![code(true)], errors: Vec::new()) },
            __cx,
        )
        .await;
        assert!(out.contains("Deactivated"), "{out}");
        assert!(!out.contains("hx-post"), "{out}");
    }

    #[tokio::test]
    async fn promo_codes_errors_are_escaped() {
        let __cx = &Cx::default();
        let out = html(
            view! { promo_codes_card(codes: Vec::new(), errors: vec!["<script>x</script>".to_string()]) },
            __cx,
        )
        .await;
        assert!(!out.contains("<script>x"), "{out}");
    }
}
