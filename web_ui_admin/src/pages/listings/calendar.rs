//! A listing's calendar sync page (spec §1.7): the export link, imported
//! calendars, blocked dates and conflicts. Each section is an HTMX swap
//! target, and every form posts with the user's service token.

use chrono::NaiveDate;
use common::calendar::conflict::StaleReason;
use common::calendar::dto::{
    BlockedDateResponse, CalendarConflictResponse, CalendarConflictsResponse, CalendarFeedResponse,
    FeedSyncResult, NewBlockedDateRequest, NewCalendarFeedRequest, Page, SyncOutcome,
    UpdateCalendarFeedRequest,
};
use common::calendar::{BlockSource, ConflictKind, FeedStatus};
use common::models::ListingResponse;
use serde::Deserialize;
use std::str::FromStr;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, error::not_found, page, parse_query_params, path_param},
    view::{View, component, view},
};
use uuid::Uuid;
use web_client::client::UserCallError;
use web_client::get_api_client;

use crate::pages::listings::managed_listing;
use crate::support::{service_token, staff};

path_param!(id);
path_param!(fid);
path_param!(bid);
path_param!(cid);

/// Env var: the guest portal's public base URL, for the export link.
pub const GUEST_PORTAL_URL_ENV: &str = "GUEST_PORTAL_URL";

/// The guest portal's base URL (`GUEST_PORTAL_URL`, local default otherwise).
pub fn guest_portal_url() -> String {
    std::env::var(GUEST_PORTAL_URL_ENV)
        .unwrap_or_else(|_| "http://localhost:3010".to_string())
        .trim_end_matches('/')
        .to_string()
}

/// The export URL channels subscribe to.
pub fn export_url(base: &str, listing_id: Uuid) -> String {
    format!("{base}/listings/{listing_id}/calendar.ics")
}

/// Add or edit calendar form.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct FeedForm {
    /// Display name.
    pub name: String,
    /// The other channel's export link.
    pub url: String,
}

/// Add-block form.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct BlockForm {
    /// First night.
    pub date_from: String,
    /// Check-out day (the night after the last blocked night).
    pub date_to: String,
    /// Private note.
    pub note: String,
}

#[derive(Deserialize, Default)]
struct ConflictPageQuery {
    page: Option<i64>,
}

/// What the page needs per request: the listing and the user's token.
async fn load(cx: &Cx) -> Result<(ListingResponse, String)> {
    let user = staff(cx).await?;
    let raw = id(cx);
    Uuid::parse_str(raw).map_err(|_| not_found())?;
    let listing = managed_listing(cx, &user, raw).await?;
    Ok((listing, service_token(cx, &user)?))
}

fn child_id(raw: &str) -> Result<Uuid> {
    Uuid::parse_str(raw).map_err(|_| not_found().into())
}

/// Text for an API refusal; the API's own message where it has one.
pub fn refusal_message(e: &UserCallError) -> String {
    match e {
        UserCallError::Api(r) if r.code == "CALENDAR_SYNC_RATE_LIMITED" => {
            "Synced less than a minute ago. Try again shortly.".to_string()
        }
        UserCallError::Api(r) => r.message.clone(),
        UserCallError::NotFound => "That item no longer exists.".to_string(),
        UserCallError::Unauthorized => "You can't change this listing.".to_string(),
        UserCallError::TimedOut(_) => "The calendar service took too long. Try again.".to_string(),
        _ => "The calendar service is unavailable. Try again.".to_string(),
    }
}

/// Label and badge classes of a feed status.
pub fn status_badge(status: FeedStatus) -> (&'static str, &'static str) {
    match status {
        FeedStatus::Never => ("Not synced yet", "badge badge-ghost"),
        FeedStatus::Ok => ("Synced", "badge badge-success"),
        FeedStatus::Warning => ("Synced with warnings", "badge badge-warning"),
        FeedStatus::Failed => ("Failing", "badge badge-error"),
    }
}

/// Text for one feed's sync result.
pub fn outcome_text(outcome: SyncOutcome) -> &'static str {
    match outcome {
        SyncOutcome::Ok => "synced",
        SyncOutcome::Warning => "synced with warnings",
        SyncOutcome::Failed => "failed",
        SyncOutcome::Skipped => "synced less than a minute ago",
        SyncOutcome::Deferred => "still running; it will finish on the next sync",
    }
}

/// "3 hours ago" style text.
pub fn relative(
    at: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
    let Some(at) = at else {
        return "never".to_string();
    };
    let secs = (now - at).num_seconds().max(0);
    match secs {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=86_399 => format!("{} h ago", secs / 3600),
        _ => format!("{} d ago", secs / 86_400),
    }
}

/// Label of a stale conflict's reason. A removed event whose feed is gone
/// reads "calendar removed", so a re-added feed's conflict isn't mistaken for
/// a duplicate.
pub fn stale_label(conflict: &CalendarConflictResponse) -> Option<&'static str> {
    conflict.stale_reason.map(|r| match (r, conflict.feed_id) {
        (StaleReason::EventRemoved, None) => "calendar removed",
        (other, _) => other.label(),
    })
}

fn dates(from: NaiveDate, to: NaiveDate) -> String {
    format!("{} – {}", from.format("%b %d, %Y"), to.format("%b %d, %Y"))
}

const INPUT: &str = "mt-1 block h-10 w-full rounded-lg border border-border bg-card px-3 text-sm";
const LABEL: &str = "block text-xs font-medium text-muted-foreground";
const BUTTON: &str = "h-10 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground";

/// The export link with a copy button.
#[component]
pub async fn export_section(
    cx: &Cx,
    /// The export URL.
    url: String,
) -> Result<impl View> {
    Ok(view! {
        <section
            id="calendar-export"
            class="space-y-2 rounded-xl border border-border p-5"
        >
            <h2 class="text-sm font-semibold">"Export link"</h2>
            <div class="flex gap-2">
                <input
                    id="calendar-export-url"
                    type="text"
                    readonly=(true)
                    value=(url)
                    class=(INPUT)
                    aria-label="Export link"
                />
                <button
                    type="button"
                    class=(BUTTON)
                    data-copy-target="#calendar-export-url"
                    data-copied-label="Copied"
                >
                    "Copy"
                </button>
            </div>
            <p class="text-xs text-muted-foreground">
                "Paste this into Airbnb → Availability → Connect calendars."
            </p>
        </section>
    })
}

/// The imported calendars table, add form and "Sync now".
#[component]
pub async fn feeds_section(
    cx: &Cx,
    /// The listing.
    listing_id: Uuid,
    /// Its feeds.
    feeds: Vec<CalendarFeedResponse>,
    /// Results of the last sync, if one just ran.
    results: Vec<FeedSyncResult>,
    /// An error to show inline.
    error: Option<String>,
) -> Result<impl View> {
    let base = format!("/admin/listings/{listing_id}/calendar");
    let now = chrono::Utc::now();
    let names: std::collections::HashMap<Uuid, String> =
        feeds.iter().map(|f| (f.id, f.name.clone())).collect();
    Ok(view! {
        <section
            id="calendar-feeds"
            class="space-y-4 rounded-xl border border-border p-5"
        >
            <div class="flex items-center justify-between">
                <h2 class="text-sm font-semibold">"Imported calendars"</h2>
                <form
                    method="post"
                    action=(format!("{base}/sync"))
                    hx-post=(format!("{base}/sync"))
                    hx-target="#calendar-feeds"
                    hx-swap="outerHTML"
                    hx-indicator="#calendar-sync-spinner"
                    hx-disabled-elt="find button"
                >
                    <button type="submit" class=(BUTTON) id="calendar-sync-now">
                        "Sync now"
                    </button>
                    <span
                        id="calendar-sync-spinner"
                        class="htmx-indicator loading loading-spinner loading-sm"
                        aria-hidden="true"
                    ></span>
                </form>
            </div>
            if let Some(error) = error {
                <div role="alert" class="alert alert-error text-sm">(error)</div>
            }
            if !results.is_empty() {
                <ul
                    id="calendar-sync-results"
                    class="space-y-1 text-sm"
                    aria-live="polite"
                >
                    for r in results {
                        <li data-outcome=(format!("{:?}", r.outcome).to_lowercase())>
                            (names
                                .get(&r.feed_id)
                                .cloned()
                                .unwrap_or_else(|| "Calendar".to_string()))
                            ": "
                            (outcome_text(r.outcome))
                            if let Some(code) = r.error_code {
                                " ("
                                (code)
                                ")"
                            }
                        </li>
                    }
                </ul>
            }
            <table class="w-full text-sm">
                <thead class="text-left text-xs text-muted-foreground uppercase">
                    <tr>
                        <th scope="col" class="py-2">"Name"</th>
                        <th scope="col" class="py-2">"Status"</th>
                        <th scope="col" class="py-2">"Last success"</th>
                        <th scope="col" class="py-2 text-right">"Events"</th>
                        <th scope="col" class="py-2 text-right">"Actions"</th>
                    </tr>
                </thead>
                <tbody class="divide-y divide-border">
                    if feeds.is_empty() {
                        <tr>
                            <td colspan="5" class="py-4 text-muted-foreground">
                                "No imported calendars yet."
                            </td>
                        </tr>
                    }
                    for f in feeds {
                        <tr
                            id=(format!("feed-{}", f.id))
                            data-status=(format!("{:?}", f.last_status).to_lowercase())
                        >
                            <td class="py-2">
                                (f.name.clone())
                                if !f.is_active {
                                    " "
                                    <span class="badge badge-ghost">"Off"</span>
                                }
                            </td>
                            <td class="py-2">
                                <span
                                    class=(status_badge(f.last_status).1)
                                    title=(f
                                        .last_error
                                        .clone()
                                        .or(f.last_warning.clone())
                                        .unwrap_or_default())
                                >
                                    (status_badge(f.last_status).0)
                                </span>
                                if let Some(err) = f.last_error.clone() {
                                    <span class="ml-2 text-xs text-destructive">(err)</span>
                                }
                            </td>
                            <td class="py-2">(relative(f.last_success_at, now))</td>
                            <td class="py-2 text-right tabular-nums">
                                (f.event_count.to_string())
                            </td>
                            <td class="py-2 text-right space-x-2">
                                <details class="inline-block text-left">
                                    <summary class="cursor-pointer">"Edit"</summary>
                                    <form
                                        method="post"
                                        action=(format!("{base}/feeds/{}/edit", f.id))
                                        hx-post=(format!("{base}/feeds/{}/edit", f.id))
                                        hx-target="#calendar-feeds"
                                        hx-swap="outerHTML"
                                        hx-disabled-elt="find button"
                                        class="mt-2 space-y-2"
                                    >
                                        <label class=(LABEL)>
                                            "Name"
                                            <input
                                                type="text"
                                                name="name"
                                                value=(f.name.clone())
                                                maxlength="60"
                                                required=(true)
                                                class=(INPUT)
                                            />
                                        </label>
                                        <label class=(LABEL)>
                                            "Calendar link"
                                            <input
                                                type="url"
                                                name="url"
                                                value=(f.url.clone())
                                                required=(true)
                                                class=(INPUT)
                                            />
                                        </label>
                                        <button type="submit" class=(BUTTON)>"Save"</button>
                                    </form>
                                </details>
                                <form
                                    method="post"
                                    class="inline"
                                    action=(format!("{base}/feeds/{}/toggle", f.id))
                                    hx-post=(format!("{base}/feeds/{}/toggle", f.id))
                                    hx-target="#calendar-feeds"
                                    hx-swap="outerHTML"
                                >
                                    <button type="submit">
                                        (if f.is_active { "Switch off" } else { "Switch on" })
                                    </button>
                                </form>
                                <form
                                    method="post"
                                    class="inline"
                                    action=(format!("{base}/feeds/{}/remove", f.id))
                                    hx-post=(format!("{base}/feeds/{}/remove", f.id))
                                    hx-target="#calendar-feeds"
                                    hx-swap="outerHTML"
                                    hx-confirm="Remove this calendar? Its blocked dates are released."
                                >
                                    <button type="submit" class="text-destructive">
                                        "Remove"
                                    </button>
                                </form>
                            </td>
                        </tr>
                    }
                </tbody>
            </table>
            <form
                method="post"
                action=(format!("{base}/feeds"))
                hx-post=(format!("{base}/feeds"))
                hx-target="#calendar-feeds"
                hx-swap="outerHTML"
                hx-disabled-elt="find button"
                class="grid gap-3 sm:grid-cols-[1fr_2fr_auto] sm:items-end"
            >
                <label class=(LABEL)>
                    "Name"
                    <input
                        type="text"
                        name="name"
                        placeholder="Airbnb"
                        maxlength="60"
                        required=(true)
                        class=(INPUT)
                    />
                </label>
                <label class=(LABEL)>
                    "Calendar link (.ics)"
                    <input
                        type="url"
                        name="url"
                        placeholder="https://…"
                        required=(true)
                        class=(INPUT)
                    />
                </label>
                <button type="submit" class=(BUTTON)>"Add calendar"</button>
            </form>
        </section>
    })
}

/// Manual blocks with their add form, and imported blocks grouped by feed.
#[component]
pub async fn blocks_section(
    cx: &Cx,
    /// The listing.
    listing_id: Uuid,
    /// Blocks from today to a year ahead.
    blocks: Vec<BlockedDateResponse>,
    /// An error to show inline.
    error: Option<String>,
) -> Result<impl View> {
    let base = format!("/admin/listings/{listing_id}/calendar");
    let manual: Vec<BlockedDateResponse> = blocks
        .iter()
        .filter(|b| b.source == BlockSource::Manual)
        .cloned()
        .collect();
    let mut imported: Vec<(String, bool, Vec<BlockedDateResponse>)> = Vec::new();
    for b in blocks
        .into_iter()
        .filter(|b| b.source == BlockSource::IcalFeed)
    {
        let name = b
            .feed_name
            .clone()
            .unwrap_or_else(|| "Imported".to_string());
        let active = b.feed_active.unwrap_or(true);
        match imported.iter_mut().find(|(n, _, _)| *n == name) {
            Some((_, _, list)) => list.push(b),
            None => imported.push((name, active, vec![b])),
        }
    }
    Ok(view! {
        <section
            id="calendar-blocks"
            class="space-y-4 rounded-xl border border-border p-5"
        >
            <h2 class="text-sm font-semibold">"Blocked dates"</h2>
            if let Some(error) = error {
                <div role="alert" class="alert alert-error text-sm">(error)</div>
            }
            <form
                method="post"
                action=(format!("{base}/blocks"))
                hx-post=(format!("{base}/blocks"))
                hx-target="#calendar-blocks"
                hx-swap="outerHTML"
                hx-disabled-elt="find button"
                class="grid gap-3 sm:grid-cols-[1fr_1fr_2fr_auto] sm:items-end"
            >
                <label class=(LABEL)>
                    "First night"
                    <input
                        type="text"
                        data-date-picker=""
                        placeholder="yyyy-mm-dd"
                        pattern="\\d{4}-\\d{2}-\\d{2}"
                        name="date_from"
                        required=(true)
                        class=(INPUT)
                    />
                </label>
                <label class=(LABEL)>
                    "Check-out day"
                    <input
                        type="text"
                        data-date-picker=""
                        placeholder="yyyy-mm-dd"
                        pattern="\\d{4}-\\d{2}-\\d{2}"
                        name="date_to"
                        required=(true)
                        class=(INPUT)
                    />
                </label>
                <label class=(LABEL)>
                    "Private note"
                    <input type="text" name="note" maxlength="500" class=(INPUT) />
                </label>
                <button type="submit" class=(BUTTON)>"Block dates"</button>
            </form>
            <ul id="manual-blocks" class="divide-y divide-border text-sm">
                if manual.is_empty() {
                    <li class="py-2 text-muted-foreground">
                        "No dates blocked by hand."
                    </li>
                }
                for b in manual {
                    <li
                        id=(format!("block-{}", b.id))
                        class="flex items-center justify-between py-2"
                    >
                        <span>
                            (dates(b.date_from, b.date_to))
                            if let Some(note) = b.note.clone() {
                                " · "
                                <span class="text-muted-foreground">(note)</span>
                            }
                        </span>
                        <form
                            method="post"
                            action=(format!("{base}/blocks/{}/remove", b.id))
                            hx-post=(format!("{base}/blocks/{}/remove", b.id))
                            hx-target="#calendar-blocks"
                            hx-swap="outerHTML"
                        >
                            <button type="submit" class="text-destructive">
                                "Remove"
                            </button>
                        </form>
                    </li>
                }
            </ul>
            for (name, active, list) in imported {
                <div class="space-y-1" data-feed-group=(name.clone())>
                    <h3 class="text-xs font-semibold uppercase text-muted-foreground">
                        (format!("From {name}"))
                        if !active {
                            " "
                            <span class="badge badge-ghost">
                                "from a disabled calendar"
                            </span>
                        }
                    </h3>
                    <ul class="text-sm">
                        for b in list {
                            <li id=(format!("block-{}", b.id))>
                                (dates(b.date_from, b.date_to))
                            </li>
                        }
                    </ul>
                </div>
            }
        </section>
    })
}

/// Open conflicts (live, then stale), then acknowledged history.
#[component]
pub async fn conflicts_section(
    cx: &Cx,
    /// The listing.
    listing_id: Uuid,
    /// Open conflicts and one history page.
    conflicts: CalendarConflictsResponse,
    /// An error to show inline.
    error: Option<String>,
) -> Result<impl View> {
    let base = format!("/admin/listings/{listing_id}/calendar");
    let CalendarConflictsResponse { open, history } = conflicts;
    let Page {
        items,
        page,
        per_page,
        total,
    } = history;
    let has_next = page * per_page < total;
    Ok(view! {
        <section
            id="calendar-conflicts"
            class="space-y-4 rounded-xl border border-border p-5"
        >
            <h2 class="text-sm font-semibold">"Conflicts"</h2>
            if let Some(error) = error {
                <div role="alert" class="alert alert-error text-sm">(error)</div>
            }
            <ul id="open-conflicts" class="divide-y divide-border text-sm">
                if open.is_empty() {
                    <li class="py-2 text-muted-foreground">"No open conflicts."</li>
                }
                for c in open {
                    <li
                        id=(format!("conflict-{}", c.id))
                        class=(if c.stale_reason.is_some() {
                            "flex items-center justify-between py-2 opacity-60"
                        } else {
                            "flex items-center justify-between py-2"
                        })
                        data-stale=(c.stale_reason.map(|r| r.code()).unwrap_or(""))
                    >
                        <span>
                            <strong>(c.feed_name.clone())</strong>
                            " "
                            (dates(c.block_date_from, c.block_date_to))
                            " vs. reservation "
                            <span class="font-mono">(c.confirmation_code.clone())</span>
                            " · "
                            (match c.kind {
                                ConflictKind::Overlap => "overlap",
                                ConflictKind::Turnover => "no turnover time",
                            })
                            if c.reopened {
                                " "
                                <span class="badge badge-warning">"Reopened"</span>
                            }
                            if let Some(label) = stale_label(&c) {
                                " "
                                <span class="badge badge-ghost">(label)</span>
                            }
                        </span>
                        <form
                            method="post"
                            action=(format!("{base}/conflicts/{}/ack", c.id))
                            hx-post=(format!("{base}/conflicts/{}/ack", c.id))
                            hx-target="#calendar-conflicts"
                            hx-swap="outerHTML"
                        >
                            <button type="submit">"Acknowledge"</button>
                        </form>
                    </li>
                }
            </ul>
            <details id="conflict-history" open=(page > 1)>
                <summary class="cursor-pointer text-sm">
                    (format!("Acknowledged ({total})"))
                </summary>
                <ul class="divide-y divide-border text-sm text-muted-foreground">
                    for c in items {
                        <li id=(format!("conflict-{}", c.id)) class="py-2">
                            (c.feed_name.clone())
                            " "
                            (dates(c.block_date_from, c.block_date_to))
                            " · "
                            (c.confirmation_code.clone())
                        </li>
                    }
                </ul>
                <div class="flex gap-3 text-sm">
                    if page > 1 {
                        <a
                            href=(format!("{base}/conflicts?page={}", page - 1))
                            hx-get=(format!("{base}/conflicts?page={}", page - 1))
                            hx-target="#calendar-conflicts"
                            hx-swap="outerHTML"
                        >
                            "Newer"
                        </a>
                    }
                    if has_next {
                        <a
                            href=(format!("{base}/conflicts?page={}", page + 1))
                            hx-get=(format!("{base}/conflicts?page={}", page + 1))
                            hx-target="#calendar-conflicts"
                            hx-swap="outerHTML"
                        >
                            "Older"
                        </a>
                    }
                </div>
            </details>
        </section>
    })
}

fn empty_conflicts() -> CalendarConflictsResponse {
    CalendarConflictsResponse {
        open: Vec::new(),
        history: Page {
            items: Vec::new(),
            page: 1,
            per_page: 20,
            total: 0,
        },
    }
}

async fn load_feeds(
    cx: &Cx,
    listing_id: Uuid,
    jwt: &str,
) -> (Vec<CalendarFeedResponse>, Option<String>) {
    match get_api_client(cx)
        .list_calendar_feeds(listing_id, jwt)
        .await
    {
        Ok(f) => (f, None),
        Err(e) => {
            tracing::warn!(%listing_id, error = %e, "calendar feeds unavailable");
            (Vec::new(), Some(refusal_message(&e)))
        }
    }
}

async fn load_blocks(
    cx: &Cx,
    listing_id: Uuid,
    jwt: &str,
) -> (Vec<BlockedDateResponse>, Option<String>) {
    match get_api_client(cx).list_blocked_dates(listing_id, jwt).await {
        Ok(b) => (b, None),
        Err(e) => {
            tracing::warn!(%listing_id, error = %e, "blocked dates unavailable");
            (Vec::new(), Some(refusal_message(&e)))
        }
    }
}

async fn load_conflicts(
    cx: &Cx,
    listing_id: Uuid,
    jwt: &str,
    page: i64,
) -> (CalendarConflictsResponse, Option<String>) {
    match get_api_client(cx)
        .list_calendar_conflicts(listing_id, page, jwt)
        .await
    {
        Ok(c) => (c, None),
        Err(e) => {
            tracing::warn!(%listing_id, error = %e, "calendar conflicts unavailable");
            (empty_conflicts(), Some(refusal_message(&e)))
        }
    }
}

/// The calendar page.
#[page("/admin/listings/{id}/calendar")]
pub async fn calendar_page(cx: &Cx) -> Result<impl View> {
    let (listing, jwt) = load(cx).await?;
    let id = listing.id;
    let name = listing.name.clone();
    let url = export_url(&guest_portal_url(), id);
    let (feeds, feeds_error) = load_feeds(cx, id, &jwt).await;
    let (blocks, blocks_error) = load_blocks(cx, id, &jwt).await;
    let (conflicts, conflicts_error) = load_conflicts(cx, id, &jwt, 1).await;
    Ok(view! {
        <div class="space-y-6">
            <div>
                <a href=(format!("/admin/listings/{id}")) class="text-sm">
                    "← Listing"
                </a>
                <h1 class="text-2xl font-semibold">
                    (format!("Calendar sync: {name}"))
                </h1>
            </div>
            export_section(url: url)
            feeds_section(
                listing_id: id,
                feeds: feeds,
                results: Vec::new(),
                error: feeds_error
            )
            blocks_section(listing_id: id, blocks: blocks, error: blocks_error)
            conflicts_section(
                listing_id: id,
                conflicts: conflicts,
                error: conflicts_error
            )
        </div>
    })
}

/// Adds an imported calendar and shows its first sync.
#[page(POST "/admin/listings/{id}/calendar/feeds")]
pub async fn add_feed(
    cx: &Cx,
    /// The add-calendar form.
    Form(form): Form<FeedForm>,
) -> Result<impl View> {
    let (listing, jwt) = load(cx).await?;
    let req = NewCalendarFeedRequest {
        name: form.name,
        url: form.url,
    };
    let (results, error) = match get_api_client(cx)
        .create_calendar_feed(listing.id, &req, &jwt)
        .await
    {
        Ok(created) => {
            tracing::info!(listing_id = %listing.id, feed_id = %created.feed.id, "calendar added");
            (vec![created.sync], None)
        }
        Err(e) => {
            tracing::info!(listing_id = %listing.id, error = %e, "calendar add refused");
            (Vec::new(), Some(refusal_message(&e)))
        }
    };
    let (feeds, load_error) = load_feeds(cx, listing.id, &jwt).await;
    let (results, error) = (results, error.or(load_error));
    let listing_id = listing.id;
    Ok(view! {
        feeds_section(
            listing_id: listing_id,
            feeds: feeds,
            results: results,
            error: error
        )
    })
}

/// Renames or re-points an imported calendar.
#[page(POST "/admin/listings/{id}/calendar/feeds/{fid}/edit")]
pub async fn edit_feed(
    cx: &Cx,
    /// The edit-calendar form.
    Form(form): Form<FeedForm>,
) -> Result<impl View> {
    let (listing, jwt) = load(cx).await?;
    let feed_id = child_id(fid(cx))?;
    let req = UpdateCalendarFeedRequest {
        name: Some(form.name),
        url: Some(form.url),
        is_active: None,
    };
    let (results, error) = match get_api_client(cx)
        .update_calendar_feed(listing.id, feed_id, &req, &jwt)
        .await
    {
        Ok(updated) => (updated.sync.into_iter().collect(), None),
        Err(e) => (Vec::new(), Some(refusal_message(&e))),
    };
    let (feeds, load_error) = load_feeds(cx, listing.id, &jwt).await;
    let (results, error) = (results, error.or(load_error));
    let listing_id = listing.id;
    Ok(view! {
        feeds_section(
            listing_id: listing_id,
            feeds: feeds,
            results: results,
            error: error
        )
    })
}

/// Switches an imported calendar on or off.
#[page(POST "/admin/listings/{id}/calendar/feeds/{fid}/toggle")]
pub async fn toggle_feed(cx: &Cx) -> Result<impl View> {
    let (listing, jwt) = load(cx).await?;
    let feed_id = child_id(fid(cx))?;
    let client = get_api_client(cx);
    let current = client
        .list_calendar_feeds(listing.id, &jwt)
        .await
        .ok()
        .and_then(|feeds| feeds.into_iter().find(|f| f.id == feed_id));
    let error = match current {
        None => Some("That calendar no longer exists.".to_string()),
        Some(feed) => {
            let req = UpdateCalendarFeedRequest {
                is_active: Some(!feed.is_active),
                ..Default::default()
            };
            client
                .update_calendar_feed(listing.id, feed_id, &req, &jwt)
                .await
                .err()
                .map(|e| refusal_message(&e))
        }
    };
    let (feeds, load_error) = load_feeds(cx, listing.id, &jwt).await;
    let (results, error) = (Vec::new(), error.or(load_error));
    let listing_id = listing.id;
    Ok(view! {
        feeds_section(
            listing_id: listing_id,
            feeds: feeds,
            results: results,
            error: error
        )
    })
}

/// Removes an imported calendar.
#[page(POST "/admin/listings/{id}/calendar/feeds/{fid}/remove")]
pub async fn remove_feed(cx: &Cx) -> Result<impl View> {
    let (listing, jwt) = load(cx).await?;
    let feed_id = child_id(fid(cx))?;
    let error = get_api_client(cx)
        .delete_calendar_feed(listing.id, feed_id, &jwt)
        .await
        .err()
        .map(|e| refusal_message(&e));
    let (feeds, load_error) = load_feeds(cx, listing.id, &jwt).await;
    let (results, error) = (Vec::new(), error.or(load_error));
    let listing_id = listing.id;
    Ok(view! {
        feeds_section(
            listing_id: listing_id,
            feeds: feeds,
            results: results,
            error: error
        )
    })
}

/// "Sync now".
#[page(POST "/admin/listings/{id}/calendar/sync")]
pub async fn sync_now(cx: &Cx) -> Result<impl View> {
    let (listing, jwt) = load(cx).await?;
    let (results, error) = match get_api_client(cx).sync_calendar(listing.id, &jwt).await {
        Ok(results) => (results, None),
        Err(e) => (Vec::new(), Some(refusal_message(&e))),
    };
    let (feeds, load_error) = load_feeds(cx, listing.id, &jwt).await;
    let (results, error) = (results, error.or(load_error));
    let listing_id = listing.id;
    Ok(view! {
        feeds_section(
            listing_id: listing_id,
            feeds: feeds,
            results: results,
            error: error
        )
    })
}

fn parse_block(form: &BlockForm) -> std::result::Result<NewBlockedDateRequest, String> {
    let from = NaiveDate::from_str(form.date_from.trim())
        .map_err(|_| "Choose the first night.".to_string())?;
    let to = NaiveDate::from_str(form.date_to.trim())
        .map_err(|_| "Choose the check-out day.".to_string())?;
    let note = Some(form.note.trim().to_string()).filter(|n| !n.is_empty());
    Ok(NewBlockedDateRequest {
        date_from: from,
        date_to: to,
        note,
    })
}

/// Blocks dates by hand.
#[page(POST "/admin/listings/{id}/calendar/blocks")]
pub async fn add_block(
    cx: &Cx,
    /// The block-dates form.
    Form(form): Form<BlockForm>,
) -> Result<impl View> {
    let (listing, jwt) = load(cx).await?;
    let error = match parse_block(&form) {
        Err(message) => Some(message),
        Ok(req) => get_api_client(cx)
            .create_blocked_dates(listing.id, &req, &jwt)
            .await
            .err()
            .map(|e| refusal_message(&e)),
    };
    let (blocks, load_error) = load_blocks(cx, listing.id, &jwt).await;
    let error = error.or(load_error);
    let listing_id = listing.id;
    Ok(view! { blocks_section(listing_id: listing_id, blocks: blocks, error: error) })
}

/// Removes a manual block.
#[page(POST "/admin/listings/{id}/calendar/blocks/{bid}/remove")]
pub async fn remove_block(cx: &Cx) -> Result<impl View> {
    let (listing, jwt) = load(cx).await?;
    let block_id = child_id(bid(cx))?;
    let error = get_api_client(cx)
        .delete_blocked_dates(listing.id, block_id, &jwt)
        .await
        .err()
        .map(|e| refusal_message(&e));
    let (blocks, load_error) = load_blocks(cx, listing.id, &jwt).await;
    let error = error.or(load_error);
    let listing_id = listing.id;
    Ok(view! { blocks_section(listing_id: listing_id, blocks: blocks, error: error) })
}

/// Acknowledges a conflict.
#[page(POST "/admin/listings/{id}/calendar/conflicts/{cid}/ack")]
pub async fn acknowledge(cx: &Cx) -> Result<impl View> {
    let (listing, jwt) = load(cx).await?;
    let conflict_id = child_id(cid(cx))?;
    let error = get_api_client(cx)
        .acknowledge_calendar_conflict(listing.id, conflict_id, &jwt)
        .await
        .err()
        .map(|e| refusal_message(&e));
    let (conflicts, load_error) = load_conflicts(cx, listing.id, &jwt, 1).await;
    let error = error.or(load_error);
    let listing_id = listing.id;
    Ok(view! {
        conflicts_section(listing_id: listing_id, conflicts: conflicts, error: error)
    })
}

/// A page of the conflicts section.
#[page("/admin/listings/{id}/calendar/conflicts")]
pub async fn conflicts_page(cx: &Cx) -> Result<impl View> {
    let (listing, jwt) = load(cx).await?;
    let page = parse_query_params::<ConflictPageQuery>(cx)
        .ok()
        .and_then(|q| q.page)
        .unwrap_or(1)
        .max(1);
    let (conflicts, load_error) = load_conflicts(cx, listing.id, &jwt, page).await;
    let error = None.or(load_error);
    let listing_id = listing.id;
    Ok(view! {
        conflicts_section(listing_id: listing_id, conflicts: conflicts, error: error)
    })
}
