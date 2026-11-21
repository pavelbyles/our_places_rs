//! Host and admin listing preview: renders both active and draft listings in
//! the guest-styled layout with live date/quote previews and disabled booking
//! and enquiry actions.

use chrono::{Months, NaiveDate, Utc};
use common::availability::jamaica_today as jamaica_date;
use common::models::ReviewResponse;
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{error::not_found, page, parse_query_params, path_param},
    view::{View, view},
};
use uuid::Uuid;
use web_client::{get_api_client, session::AdminSession};
use web_ui_common::components::{
    date_range_calendar::{CalendarProps, date_range_calendar, first_of_month},
    domain::{
        listing_detail::{CtaMode, StayQuote, listing_detail_view, quote_panel},
        price_breakdown::{NoQuote, breakdown},
    },
};

use super::managed_listing_details;
use crate::support::{host_or_admin, service_token};

path_param!(id);

/// Months shown side by side in the booking calendar.
const CALENDAR_MONTHS: u32 = 2;

#[derive(Deserialize, Default)]
struct PreviewStayQuery {
    check_in: Option<NaiveDate>,
    check_out: Option<NaiveDate>,
    guests: Option<i32>,
    month: Option<String>,
}

fn today() -> NaiveDate {
    jamaica_date(Utc::now())
}

fn stay_query(cx: &Cx) -> PreviewStayQuery {
    parse_query_params::<PreviewStayQuery>(cx).unwrap_or_default()
}

fn stay_dates(q: &PreviewStayQuery) -> Option<(NaiveDate, NaiveDate)> {
    match (q.check_in, q.check_out) {
        (Some(a), Some(b)) if b > a && a >= today() => Some((a, b)),
        _ => None,
    }
}

async fn calendar_props(
    cx: &Cx,
    listing_id: Uuid,
    month: NaiveDate,
    q: &PreviewStayQuery,
) -> CalendarProps {
    let month = first_of_month(month);
    let until = month
        .checked_add_months(Months::new(CALENDAR_MONTHS))
        .unwrap_or(month);
    let ranges = get_api_client(cx)
        .get_unavailable_ranges(listing_id, month, until)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("preview calendar availability unavailable: {e}");
            Vec::new()
        });
    CalendarProps {
        calendar_url_prefix: format!("/admin/listings/{listing_id}/preview/calendar"),
        month,
        months: CALENDAR_MONTHS,
        today: today(),
        ranges,
        check_in: q.check_in,
        check_out: q.check_out,
        check_in_id: "reserve-check-in".into(),
        check_out_id: "reserve-check-out".into(),
    }
}

async fn preview_quote(
    cx: &Cx,
    user: &AdminSession,
    listing_id: Uuid,
    q: &PreviewStayQuery,
) -> StayQuote {
    let (check_in, check_out) = stay_dates(q)?;
    let guests = u32::try_from(q.guests.unwrap_or(1).max(1)).unwrap_or(1);
    let jwt = match service_token(cx, user) {
        Ok(token) => token,
        Err(e) => {
            tracing::warn!(%listing_id, "preview service token unavailable: {e}");
            return Some(Err(NoQuote::Failed));
        }
    };
    Some(
        get_api_client(cx)
            .preview_listing_quote(listing_id, check_in, check_out, guests, &jwt)
            .await
            .map(|resp| breakdown(&resp))
            .map_err(|e| NoQuote::from_error(&e, &listing_id)),
    )
}

/// Full standalone guest-styled preview of an active or inactive listing.
#[page("/admin/listings/{id}/preview")]
pub async fn listing_preview_page(cx: &Cx) -> Result<impl View> {
    let user = host_or_admin(cx).await?;
    let raw = id(cx);
    Uuid::parse_str(raw).map_err(|_| not_found())?;
    let details = managed_listing_details(cx, &user, raw).await?;
    let listing_id = details.listing.id;
    let is_active = details.listing.is_active;
    let q = stay_query(cx);
    let month = q.check_in.unwrap_or_else(today);
    let (calendar, reviews) = tokio::join!(
        calendar_props(cx, listing_id, month, &q),
        get_api_client(cx).get_listing_reviews(listing_id, 1, 6)
    );
    let reviews: Vec<ReviewResponse> = reviews.unwrap_or_default();
    let quote = if details.listing.price_per_night.is_none() {
        None
    } else {
        preview_quote(cx, &user, listing_id, &q).await
    };
    let default_guests = q.guests.unwrap_or(1).max(1);
    let quote_url = format!("/admin/listings/{listing_id}/preview/quote");
    let back_href = format!("/admin/listings/{listing_id}");
    let pill_label = if is_active {
        "Preview · Active"
    } else {
        "Preview · Draft"
    };
    Ok(view! {
        <div
            data-testid="preview-back-bar"
            class="full-bleed border-b border-border bg-card text-card-foreground"
        >
            <div class="bleed-inner flex min-h-12 items-center justify-between gap-4 py-2">
                <a
                    href=(back_href)
                    class="inline-flex items-center gap-2 text-sm font-medium text-foreground no-underline hover:underline"
                >
                    "← Back to Admin"
                </a>
                <div class="flex items-center gap-3">
                    <span class="hidden text-xs text-muted-foreground sm:inline">
                        "Read-only guest preview"
                    </span>
                    <span
                        data-testid="preview-pill"
                        role="status"
                        class="rounded-full border border-border bg-background px-3.5 py-1 text-xs font-medium text-foreground shadow-sm"
                    >
                        (pill_label)
                    </span>
                </div>
            </div>
        </div>
        listing_detail_view(
            details: details,
            reviews: reviews,
            calendar: calendar,
            quote: quote,
            quote_url: quote_url,
            cta_mode: CtaMode::Preview,
            no_rate_for: None,
            currency: None,
            default_guests: default_guests
        )
    })
}

/// Quote fragment for the preview booking card (`hx-get` on date/guest change).
#[page("/admin/listings/{id}/preview/quote")]
pub async fn listing_preview_quote(cx: &Cx) -> Result<impl View> {
    let user = host_or_admin(cx).await?;
    let raw = id(cx);
    Uuid::parse_str(raw).map_err(|_| not_found())?;
    let details = managed_listing_details(cx, &user, raw).await?;
    let listing_id = details.listing.id;
    let q = stay_query(cx);
    let rate_unset = details.listing.price_per_night.is_none();
    let quote = if rate_unset {
        None
    } else {
        preview_quote(cx, &user, listing_id, &q).await
    };
    Ok(view! {
        quote_panel(
            slug: details.listing.slug,
            quote: quote,
            mode: details.listing.booking_mode,
            cta_mode: CtaMode::Preview,
            rate_unset: rate_unset
        )
    })
}

/// Calendar fragment for prev/next month navigation in preview mode.
#[page("/admin/listings/{id}/preview/calendar")]
pub async fn listing_preview_calendar(cx: &Cx) -> Result<impl View> {
    let user = host_or_admin(cx).await?;
    let raw = id(cx);
    Uuid::parse_str(raw).map_err(|_| not_found())?;
    let details = managed_listing_details(cx, &user, raw).await?;
    let q = stay_query(cx);
    let month = q
        .month
        .as_deref()
        .and_then(|m| NaiveDate::parse_from_str(&format!("{m}-01"), "%Y-%m-%d").ok())
        .unwrap_or_else(today);
    let props = calendar_props(cx, details.listing.id, month, &q).await;
    Ok(view! { date_range_calendar(props: props) })
}
