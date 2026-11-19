//! Operational turnover & cleaning crew dispatch board (`/admin/turnovers`).

use std::collections::BTreeMap;
use std::str::FromStr;

use chrono::{NaiveDate, Utc};
use common::app_client::encode_query_component;
use common::reference::listing_today;
use common::turnover::{
    CreateManualTurnoverRequest, CreateTurnoverCommentRequest, TransitionTurnoverRequest,
    TurnoverActorRole, TurnoverCommentResponse, TurnoverListQuery, TurnoverResponse,
    TurnoverStatus, TurnoverWindow, allowed_next_statuses,
};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    htmx::hx_request,
    router::{
        content::Form,
        error::{forbidden, not_found, see_other},
        page, parse_query_params, path_param,
    },
    view::{View, component, view},
};
use uuid::Uuid;
use web_client::{
    client::UserCallError,
    get_api_client,
    redirect::safe_redirect,
    session::{AdminRole, AdminSession},
};
use web_ui_common::components::chat::{chat_bubble, chat_thread};

use crate::support::{dispatch_actor, scoped_listings, service_token};

path_param!(id);

const DEFAULT_PER_PAGE: u32 = 20;

/// Query parameters for `GET /admin/turnovers`.
#[derive(Debug, Default, Deserialize)]
pub struct TurnoversBoardQuery {
    /// Window tab (`today`, `upcoming`, `past`, `all`).
    #[serde(default)]
    pub window: Option<String>,
    /// Optional status slug filter (`needs_cleaning`, ...).
    #[serde(default)]
    pub status: Option<String>,
    /// Optional listing UUID filter string.
    #[serde(default)]
    pub listing_id: Option<String>,
    /// 1-based page number.
    #[serde(default)]
    pub page: Option<u32>,
    /// Transient success banner code.
    #[serde(default)]
    pub notice: Option<String>,
    /// Transient error alert text.
    #[serde(default)]
    pub error: Option<String>,
}

/// Form payload submitted by `POST /admin/turnovers/create`.
#[derive(Debug, Deserialize)]
pub struct CreateTurnoverForm {
    /// Target listing identifier string.
    pub listing_id: String,
    /// Target checkout or maintenance date (`YYYY-MM-DD`).
    pub checkout_date: String,
    /// Optional short descriptive title.
    #[serde(default)]
    pub title: Option<String>,
    /// Optional initial status slug.
    #[serde(default)]
    pub initial_status: Option<String>,
    /// Current board window for clean post-redirect preservation.
    #[serde(default)]
    pub return_to: Option<String>,
}

/// Form payload submitted by `POST /admin/turnovers/{id}/status`.
#[derive(Debug, Deserialize)]
pub struct TransitionTurnoverForm {
    /// Target operational status slug.
    pub to_status: String,
    /// Expected current status slug for optimistic concurrency guard.
    #[serde(default)]
    pub expected_status: Option<String>,
    /// Optional status change note/reason.
    #[serde(default)]
    pub reason: Option<String>,
    /// Optional board/detail return path.
    #[serde(default)]
    pub return_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ListingOption {
    id: String,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TransitionOption {
    slug: String,
    label: String,
    button_class: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BoardCardVm {
    id: String,
    detail_href: String,
    status_action: String,
    listing_name: String,
    listing_city: Option<String>,
    checkout_date: String,
    next_checkin_text: String,
    policy_summary: Option<String>,
    source_label: &'static str,
    status_slug: String,
    status_label: &'static str,
    status_badge_class: &'static str,
    title: Option<String>,
    assigned_staff_text: Option<String>,
    comment_count: i64,
    transitions: Vec<TransitionOption>,
    return_to: String,
    inline_alert: Option<String>,
    inline_notice: Option<String>,
}

#[derive(Debug, Clone)]
struct WindowTabVm {
    label: &'static str,
    slug: &'static str,
    href: String,
    active: bool,
}

fn non_empty(raw: Option<&str>) -> Option<&str> {
    raw.map(str::trim).filter(|s| !s.is_empty())
}

fn turnover_actor_role(role: AdminRole) -> TurnoverActorRole {
    match role {
        AdminRole::Admin => TurnoverActorRole::Admin,
        AdminRole::Host => TurnoverActorRole::Host,
        AdminRole::Staff => TurnoverActorRole::Staff,
    }
}

fn notice_message(code: Option<&str>) -> Option<&'static str> {
    match code {
        Some("created") => Some("Manual turnover added to the dispatch board."),
        Some("updated") => Some("Turnover status updated."),
        Some("commented") => Some("Note posted to the crew discussion thread."),
        _ => None,
    }
}

fn safe_turnover_return_target(raw: Option<&str>) -> String {
    let candidate = non_empty(raw).unwrap_or("/admin/turnovers");
    let safe = safe_redirect(candidate);
    if safe == "/admin/turnovers"
        || safe.starts_with("/admin/turnovers/")
        || safe.starts_with("/admin/turnovers?")
    {
        safe.to_string()
    } else {
        "/admin/turnovers".to_string()
    }
}

fn append_flash_params(base: &str, notice: Option<&str>, error: Option<&str>) -> String {
    let clean_base = base
        .split('#')
        .next()
        .unwrap_or("/admin/turnovers")
        .to_string();
    let (path, existing_qs) = match clean_base.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (clean_base.as_str(), None),
    };
    let mut params: Vec<String> = existing_qs
        .map(|q| {
            q.split('&')
                .filter(|pair| {
                    !pair.is_empty() && !pair.starts_with("notice=") && !pair.starts_with("error=")
                })
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();
    if let Some(code) = notice.and_then(|s| non_empty(Some(s))) {
        params.push(format!("notice={}", encode_query_component(code)));
    }
    if let Some(msg) = error.and_then(|s| non_empty(Some(s))) {
        params.push(format!("error={}", encode_query_component(msg)));
    }
    if params.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{}", params.join("&"))
    }
}

fn board_url(
    window: TurnoverWindow,
    status: Option<TurnoverStatus>,
    listing_id: Option<Uuid>,
    page: u32,
) -> String {
    let mut params = vec![format!("window={}", window.as_str())];
    if let Some(s) = status {
        params.push(format!("status={}", s.as_str()));
    }
    if let Some(lid) = listing_id {
        params.push(format!("listing_id={lid}"));
    }
    if page > 1 {
        params.push(format!("page={page}"));
    }
    format!("/admin/turnovers?{}", params.join("&"))
}

fn describe_board_error(err: &UserCallError) -> String {
    match err {
        UserCallError::Unauthorized => "Your session does not have dispatch access.".to_string(),
        UserCallError::NotFound => "Turnover dispatch resource was not found.".to_string(),
        UserCallError::Api(rej) => rej.message.clone(),
        UserCallError::TimedOut(_) => "Turnover service timed out. Please retry.".to_string(),
        UserCallError::Transport(_) | UserCallError::Identity(_) | UserCallError::Status(_) => {
            "Unable to reach the turnover dispatch service. Please retry.".to_string()
        }
    }
}

fn describe_mutation_error(err: &UserCallError) -> String {
    match err {
        UserCallError::Unauthorized => {
            "Your role does not allow this turnover change or listing action.".to_string()
        }
        UserCallError::NotFound => {
            "This turnover or property is not available in your dispatch scope.".to_string()
        }
        UserCallError::Api(rej) if rej.code == "TURNOVER_STATUS_CONFLICT" => {
            "Status changed concurrently elsewhere. Card refreshed with latest state.".to_string()
        }
        UserCallError::Api(rej) => rej.message.clone(),
        UserCallError::TimedOut(_) => "Request timed out. Please retry.".to_string(),
        UserCallError::Transport(_) | UserCallError::Identity(_) | UserCallError::Status(_) => {
            "Action failed against the backend service. Please retry.".to_string()
        }
    }
}

fn status_badge_class(status: TurnoverStatus) -> &'static str {
    match status {
        TurnoverStatus::NeedsCleaning => {
            "inline-flex items-center rounded-full bg-amber-100 px-2.5 py-0.5 text-xs font-medium text-amber-900 dark:bg-amber-900/40 dark:text-amber-200"
        }
        TurnoverStatus::CleaningInProgress => {
            "inline-flex items-center rounded-full bg-sky-100 px-2.5 py-0.5 text-xs font-medium text-sky-900 dark:bg-sky-900/40 dark:text-sky-200"
        }
        TurnoverStatus::CleaningCompleted => {
            "inline-flex items-center rounded-full bg-indigo-100 px-2.5 py-0.5 text-xs font-medium text-indigo-900 dark:bg-indigo-900/40 dark:text-indigo-200"
        }
        TurnoverStatus::MaintenanceRequired => {
            "inline-flex items-center rounded-full bg-rose-100 px-2.5 py-0.5 text-xs font-medium text-rose-900 dark:bg-rose-900/40 dark:text-rose-200"
        }
        TurnoverStatus::InspectedAndReady => {
            "inline-flex items-center rounded-full bg-emerald-100 px-2.5 py-0.5 text-xs font-medium text-emerald-900 dark:bg-emerald-900/40 dark:text-emerald-200"
        }
        TurnoverStatus::NotRequired => {
            "inline-flex items-center rounded-full bg-muted px-2.5 py-0.5 text-xs font-medium text-muted-foreground"
        }
    }
}

fn transition_button_class(target: TurnoverStatus) -> &'static str {
    match target {
        TurnoverStatus::InspectedAndReady => {
            "inline-flex min-h-[44px] items-center justify-center rounded-lg bg-emerald-600 px-3 py-2 text-xs font-medium text-white hover:bg-emerald-700"
        }
        TurnoverStatus::MaintenanceRequired => {
            "inline-flex min-h-[44px] items-center justify-center rounded-lg border border-amber-500/50 bg-amber-500/10 px-3 py-2 text-xs font-medium text-amber-900 hover:bg-amber-500/20 dark:text-amber-200"
        }
        TurnoverStatus::NotRequired => {
            "inline-flex min-h-[44px] items-center justify-center rounded-lg border border-destructive/40 px-3 py-2 text-xs font-medium text-destructive hover:bg-destructive/10"
        }
        TurnoverStatus::CleaningInProgress | TurnoverStatus::CleaningCompleted => {
            "inline-flex min-h-[44px] items-center justify-center rounded-lg bg-primary px-3 py-2 text-xs font-medium text-primary-foreground hover:opacity-90"
        }
        TurnoverStatus::NeedsCleaning => {
            "inline-flex min-h-[44px] items-center justify-center rounded-lg border border-border bg-background px-3 py-2 text-xs font-medium hover:bg-muted"
        }
    }
}

fn format_policy_summary(item: &TurnoverResponse) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(co) = non_empty(item.checkout_policy_text.as_deref()) {
        parts.push(format!("Check-out {co}"));
    }
    if let Some(ci) = non_empty(item.checkin_policy_text.as_deref()) {
        parts.push(format!("Check-in {ci}"));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn build_card_vm(
    item: TurnoverResponse,
    role: AdminRole,
    return_to: &str,
    inline_alert: Option<String>,
    inline_notice: Option<String>,
) -> BoardCardVm {
    let actor_role = turnover_actor_role(role);
    let transitions = allowed_next_statuses(actor_role, item.source_kind, item.status)
        .into_iter()
        .map(|next| TransitionOption {
            slug: next.as_str().to_string(),
            label: next.label().to_string(),
            button_class: transition_button_class(next),
        })
        .collect();
    let policy_summary = format_policy_summary(&item);
    let assigned_staff_text = (!item.assigned_staff_names.is_empty())
        .then(|| format!("Crew: {}", item.assigned_staff_names.join(", ")));
    let next_checkin_text = item
        .next_checkin_date
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "Open".to_string());
    let id_str = item.id.to_string();

    BoardCardVm {
        detail_href: format!("/admin/turnovers/{id_str}"),
        status_action: format!("/admin/turnovers/{id_str}/status"),
        id: id_str,
        listing_name: item.listing_name,
        listing_city: item.listing_city,
        checkout_date: item.checkout_date.format("%Y-%m-%d").to_string(),
        next_checkin_text,
        policy_summary,
        source_label: item.source_kind.label(),
        status_slug: item.status.as_str().to_string(),
        status_label: item.status.label(),
        status_badge_class: status_badge_class(item.status),
        title: item.title,
        assigned_staff_text,
        comment_count: item.comment_count,
        transitions,
        return_to: return_to.to_string(),
        inline_alert,
        inline_notice,
    }
}

fn initial_status_options(role: AdminRole) -> Vec<(&'static str, &'static str)> {
    let mut out = vec![
        (
            TurnoverStatus::NeedsCleaning.as_str(),
            TurnoverStatus::NeedsCleaning.label(),
        ),
        (
            TurnoverStatus::CleaningInProgress.as_str(),
            TurnoverStatus::CleaningInProgress.label(),
        ),
        (
            TurnoverStatus::CleaningCompleted.as_str(),
            TurnoverStatus::CleaningCompleted.label(),
        ),
        (
            TurnoverStatus::MaintenanceRequired.as_str(),
            TurnoverStatus::MaintenanceRequired.label(),
        ),
    ];
    if matches!(role, AdminRole::Host | AdminRole::Admin) {
        out.push((
            TurnoverStatus::InspectedAndReady.as_str(),
            TurnoverStatus::InspectedAndReady.label(),
        ));
    }
    out
}

async fn resolve_listing_options(
    cx: &Cx,
    user: &AdminSession,
    token: &str,
    board_turnovers: &[TurnoverResponse],
    _should_fetch_all: bool,
) -> Vec<ListingOption> {
    let mut by_id: BTreeMap<Uuid, String> = BTreeMap::new();
    if let Ok(accessible) = get_api_client(cx).list_turnover_listings(token).await {
        for item in accessible {
            by_id.insert(item.listing_id, item.listing_name);
        }
    }
    for listing in scoped_listings(cx, user).await {
        by_id.entry(listing.id).or_insert(listing.name);
    }
    for t in board_turnovers {
        by_id
            .entry(t.listing_id)
            .or_insert_with(|| t.listing_name.clone());
    }
    let mut options: Vec<ListingOption> = by_id
        .into_iter()
        .map(|(id, name)| ListingOption {
            id: id.to_string(),
            name,
        })
        .collect();
    options.sort_by_key(|a| a.name.to_lowercase());
    options
}

#[component]
async fn turnover_card(cx: &Cx, card: BoardCardVm) -> Result<impl View> {
    let hx_target = format!("#turnover-card-{}", card.id);
    let test_id = format!("turnover-card-{}", card.id);
    Ok(view! {
        <article
            id=(test_id.clone())
            data-turnover-status=(card.status_slug.clone())
            class="min-w-0 space-y-4 rounded-xl border border-border bg-card p-4 shadow-xs sm:p-5"
        >
            if let Some(alert) = card.inline_alert {
                <div
                    role="alert"
                    class="rounded-lg border border-destructive/40 bg-destructive/10 p-3 text-xs text-destructive"
                >
                    (alert)
                </div>
            }
            if let Some(ok_msg) = card.inline_notice {
                <div
                    role="status"
                    class="rounded-lg border border-emerald-500/40 bg-emerald-500/10 p-3 text-xs text-emerald-800 dark:text-emerald-200"
                >
                    (ok_msg)
                </div>
            }
            <div class="flex flex-wrap items-start justify-between gap-2">
                <div class="min-w-0 space-y-1">
                    <div class="flex flex-wrap items-center gap-2">
                        <h3 class="text-base font-semibold break-words">
                            <a href=(card.detail_href.clone()) class="hover:underline">
                                (card.listing_name)
                            </a>
                        </h3>
                        <span class="inline-flex items-center rounded-md border border-border px-2 py-0.5 text-xs text-muted-foreground">
                            (card.source_label)
                        </span>
                    </div>
                    if let Some(city) = card.listing_city {
                        <p class="text-xs text-muted-foreground">(city)</p>
                    }
                    if let Some(title) = card.title {
                        <p class="text-sm font-medium text-foreground break-words">(title)</p>
                    }
                </div>
                <span class=(card.status_badge_class)>(card.status_label)</span>
            </div>

            <dl class="grid grid-cols-2 gap-3 rounded-lg bg-muted/40 p-3 text-xs sm:text-sm">
                <div class="min-w-0">
                    <dt class="text-muted-foreground">"Checkout"</dt>
                    <dd class="font-medium">(card.checkout_date)</dd>
                </div>
                <div class="min-w-0">
                    <dt class="text-muted-foreground">"Next Check-In"</dt>
                    <dd class="font-medium">(card.next_checkin_text)</dd>
                </div>
            </dl>

            if let Some(policy) = card.policy_summary {
                <p class="text-xs text-muted-foreground">(policy)</p>
            }
            if let Some(crew) = card.assigned_staff_text {
                <p class="text-xs text-muted-foreground break-words">(crew)</p>
            }

            <div class="flex flex-wrap items-center justify-between gap-2 pt-1">
                <div class="flex flex-wrap gap-2">
                    for tr in card.transitions {
                        <form
                            method="post"
                            action=(card.status_action.clone())
                            hx-post=(card.status_action.clone())
                            hx-target=(hx_target.clone())
                            hx-swap="outerHTML"
                        >
                            <input type="hidden" name="to_status" value=(tr.slug.clone()) />
                            <input type="hidden" name="expected_status" value=(card.status_slug.clone()) />
                            <input type="hidden" name="return_to" value=(card.return_to.clone()) />
                            <button
                                type="submit"
                                data-status-target=(tr.slug)
                                class=(tr.button_class)
                            >
                                (tr.label)
                            </button>
                        </form>
                    }
                </div>
                <a
                    href=(card.detail_href)
                    class="inline-flex min-h-[44px] items-center justify-center rounded-lg border border-border px-3 text-xs font-medium hover:bg-muted"
                >
                    (format!("Notes & Details ({})", card.comment_count))
                </a>
            </div>
        </article>
    })
}

/// Mobile-first turnover dispatch board for Hosts, Staff, and Administrators (`/admin/turnovers`).
#[page("/admin/turnovers")]
pub async fn turnovers_page(cx: &Cx) -> Result<impl View> {
    let user = dispatch_actor(cx).await?;
    let query = parse_query_params::<TurnoversBoardQuery>(cx).unwrap_or_default();
    let token = service_token(cx, &user)?;
    let api = get_api_client(cx);

    let window = non_empty(query.window.as_deref())
        .and_then(|s| TurnoverWindow::from_str(s).ok())
        .unwrap_or(TurnoverWindow::Today);
    let status_filter =
        non_empty(query.status.as_deref()).and_then(|s| TurnoverStatus::from_str(s).ok());
    let listing_filter =
        non_empty(query.listing_id.as_deref()).and_then(|s| Uuid::parse_str(s).ok());
    let page = query.page.unwrap_or(1).max(1);
    let return_to = board_url(window, status_filter, listing_filter, page);

    let api_query = TurnoverListQuery {
        window: Some(window),
        status: status_filter,
        listing_id: listing_filter,
        page: Some(page),
        per_page: Some(DEFAULT_PER_PAGE),
    };

    let (board_res, load_error) = match api.list_turnovers(&api_query, &token).await {
        Ok(data) => (Some(data), None),
        Err(err) => {
            tracing::error!(
                user_id = %user.user_id,
                window = window.as_str(),
                error = ?err,
                "failed to load turnover dispatch board"
            );
            (None, Some(describe_board_error(&err)))
        }
    };

    let board_turnovers = board_res
        .as_ref()
        .map(|b| b.turnovers.clone())
        .unwrap_or_default();
    let total = board_res.as_ref().map(|b| b.total).unwrap_or(0);
    let should_fetch_all =
        !(window == TurnoverWindow::All && status_filter.is_none() && listing_filter.is_none());
    let listing_options =
        resolve_listing_options(cx, &user, &token, &board_turnovers, should_fetch_all).await;

    let cards: Vec<BoardCardVm> = board_turnovers
        .into_iter()
        .map(|item| build_card_vm(item, user.role, &return_to, None, None))
        .collect();

    let tabs: Vec<WindowTabVm> = [
        (TurnoverWindow::Today, "Today"),
        (TurnoverWindow::Upcoming, "Upcoming"),
        (TurnoverWindow::Past, "Past"),
        (TurnoverWindow::All, "All"),
    ]
    .into_iter()
    .map(|(w, label)| WindowTabVm {
        label,
        slug: w.as_str(),
        href: board_url(w, status_filter, listing_filter, 1),
        active: w == window,
    })
    .collect();

    let status_options: Vec<(&'static str, &'static str, bool)> = TurnoverStatus::ALL
        .into_iter()
        .map(|s| (s.as_str(), s.label(), status_filter == Some(s)))
        .collect();
    let selected_listing_str = listing_filter.map(|id| id.to_string()).unwrap_or_default();
    let manual_status_opts = initial_status_options(user.role);
    let default_date = listing_today(Utc::now()).format("%Y-%m-%d").to_string();
    let total_pages = u32::try_from(
        (total.max(0) + i64::from(DEFAULT_PER_PAGE) - 1) / i64::from(DEFAULT_PER_PAGE),
    )
    .unwrap_or(1)
    .max(1);
    let prev_url = (page > 1).then(|| board_url(window, status_filter, listing_filter, page - 1));
    let next_url =
        (page < total_pages).then(|| board_url(window, status_filter, listing_filter, page + 1));
    let flash_notice = notice_message(non_empty(query.notice.as_deref())).map(ToString::to_string);
    let flash_error = non_empty(query.error.as_deref()).map(ToString::to_string);
    let has_listings = !listing_options.is_empty();

    Ok(view! {
        <div class="mx-auto max-w-6xl min-w-0 space-y-6 overflow-x-hidden px-3 py-4 sm:px-6">
            <header class="flex flex-wrap items-center justify-between gap-3">
                <div class="min-w-0">
                    <h1 class="text-2xl font-semibold tracking-tight">"Turnover Dispatch"</h1>
                    <p class="text-sm text-muted-foreground">
                        "Cleaning schedules, maintenance handoffs, and crew notes."
                    </p>
                </div>
                <span class="inline-flex items-center rounded-full border border-border px-3 py-1 text-xs font-medium text-muted-foreground">
                    (format!("{total} scheduled"))
                </span>
            </header>

            if let Some(msg) = flash_notice {
                <div
                    role="status"
                    class="rounded-xl border border-emerald-500/40 bg-emerald-500/10 p-4 text-sm text-emerald-800 dark:text-emerald-200"
                >
                    (msg)
                </div>
            }
            if let Some(err_msg) = flash_error {
                <div
                    role="alert"
                    class="rounded-xl border border-destructive/40 bg-destructive/10 p-4 text-sm text-destructive"
                >
                    (err_msg)
                </div>
            }
            if let Some(err_msg) = load_error {
                <div
                    role="alert"
                    data-testid="turnover-load-error"
                    class="rounded-xl border border-destructive/40 bg-destructive/10 p-4 text-sm text-destructive"
                >
                    (err_msg)
                </div>
            }

            <nav aria-label="Turnover windows" class="flex flex-wrap gap-2">
                for tab in tabs {
                    <a
                        href=(tab.href)
                        data-window-tab=(tab.slug)
                        class=(if tab.active {
                            "inline-flex min-h-[44px] items-center justify-center rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground"
                        } else {
                            "inline-flex min-h-[44px] items-center justify-center rounded-lg border border-border bg-card px-4 text-sm font-medium hover:bg-muted"
                        })
                    >
                        (tab.label)
                    </a>
                }
            </nav>

            <form
                method="get"
                action="/admin/turnovers"
                class="grid grid-cols-1 gap-3 rounded-xl border border-border bg-card p-4 sm:grid-cols-3"
            >
                <input type="hidden" name="window" value=(window.as_str()) />
                <div class="min-w-0 space-y-1">
                    <label for="filter-status" class="block text-xs font-medium text-muted-foreground">
                        "Status"
                    </label>
                    <select
                        id="filter-status"
                        name="status"
                        class="min-h-[44px] w-full min-w-0 rounded-lg border border-border bg-background px-3 text-sm"
                    >
                        <option value="" selected=(status_filter.is_none())>"All statuses"</option>
                        for (slug, label, selected) in status_options {
                            <option value=(slug) selected=(selected)>(label)</option>
                        }
                    </select>
                </div>
                <div class="min-w-0 space-y-1">
                    <label for="filter-listing" class="block text-xs font-medium text-muted-foreground">
                        "Property"
                    </label>
                    <select
                        id="filter-listing"
                        name="listing_id"
                        class="min-h-[44px] w-full min-w-0 rounded-lg border border-border bg-background px-3 text-sm"
                    >
                        <option value="" selected=(listing_filter.is_none())>"All properties"</option>
                        for opt in listing_options.clone() {
                            <option
                                value=(opt.id.clone())
                                selected=(opt.id == selected_listing_str)
                            >
                                (opt.name)
                            </option>
                        }
                    </select>
                </div>
                <div class="flex items-end">
                    <button
                        type="submit"
                        class="inline-flex min-h-[44px] w-full items-center justify-center rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90"
                    >
                        "Apply Filters"
                    </button>
                </div>
            </form>

            <details class="rounded-xl border border-border bg-card p-4">
                <summary class="cursor-pointer list-none font-medium text-sm">
                    "+ Schedule manual clean / maintenance turnover"
                </summary>
                <form
                    id="manual-turnover-form"
                    method="post"
                    action="/admin/turnovers/create"
                    class="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2"
                >
                    <input type="hidden" name="return_to" value=(return_to.clone()) />
                    <div class="min-w-0 space-y-1">
                        <label for="manual-listing-id" class="block text-xs font-medium text-muted-foreground">
                            "Property"
                        </label>
                        if has_listings {
                            <select
                                id="manual-listing-id"
                                name="listing_id"
                                required=(true)
                                class="min-h-[44px] w-full min-w-0 rounded-lg border border-border bg-background px-3 text-sm"
                            >
                                for opt in listing_options {
                                    <option value=(opt.id)>(opt.name)</option>
                                }
                            </select>
                        } else {
                            <p class="text-xs text-muted-foreground">
                                "No assigned properties available to schedule."
                            </p>
                        }
                    </div>
                    <div class="min-w-0 space-y-1">
                        <label for="manual-checkout-date" class="block text-xs font-medium text-muted-foreground">
                            "Target date"
                        </label>
                        <input
                            id="manual-checkout-date"
                            name="checkout_date"
                            type="date"
                            required=(true)
                            value=(default_date)
                            class="min-h-[44px] w-full min-w-0 rounded-lg border border-border bg-background px-3 text-sm"
                        />
                    </div>
                    <div class="min-w-0 space-y-1">
                        <label for="manual-title" class="block text-xs font-medium text-muted-foreground">
                            "Task title"
                        </label>
                        <input
                            id="manual-title"
                            name="title"
                            type="text"
                            maxlength="120"
                            placeholder="Deep clean, AC service, pool shock..."
                            class="min-h-[44px] w-full min-w-0 rounded-lg border border-border bg-background px-3 text-sm"
                        />
                    </div>
                    <div class="min-w-0 space-y-1">
                        <label for="manual-initial-status" class="block text-xs font-medium text-muted-foreground">
                            "Initial status"
                        </label>
                        <select
                            id="manual-initial-status"
                            name="initial_status"
                            class="min-h-[44px] w-full min-w-0 rounded-lg border border-border bg-background px-3 text-sm"
                        >
                            for (slug, label) in manual_status_opts {
                                <option value=(slug)>(label)</option>
                            }
                        </select>
                    </div>
                    <div class="sm:col-span-2">
                        <button
                            id="btn-create-manual-turnover"
                            type="submit"
                            class="inline-flex min-h-[44px] items-center justify-center rounded-lg bg-primary px-5 text-sm font-medium text-primary-foreground hover:opacity-90"
                        >
                            "Create Turnover"
                        </button>
                    </div>
                </form>
            </details>

            <section aria-label="Turnover cards" class="grid grid-cols-1 gap-4 lg:grid-cols-2">
                if cards.is_empty() {
                    <div class="rounded-xl border border-dashed border-border p-8 text-center text-sm text-muted-foreground lg:col-span-2">
                        "No turnovers match the selected window and filters."
                    </div>
                }
                for card in cards {
                    turnover_card(card: card)
                }
            </section>

            if total_pages > 1 {
                <nav aria-label="Turnover pagination" class="flex flex-wrap items-center justify-between gap-2 pt-2">
                    if let Some(prev) = prev_url {
                        <a
                            href=(prev)
                            class="inline-flex min-h-[44px] items-center rounded-lg border border-border px-4 text-sm font-medium hover:bg-muted"
                        >
                            "Previous"
                        </a>
                    } else {
                        <span class="inline-flex min-h-[44px] items-center rounded-lg border border-border/40 px-4 text-sm text-muted-foreground">
                            "Previous"
                        </span>
                    }
                    <span class="text-sm text-muted-foreground">
                        (format!("Page {page} of {total_pages}"))
                    </span>
                    if let Some(next) = next_url {
                        <a
                            href=(next)
                            class="inline-flex min-h-[44px] items-center rounded-lg border border-border px-4 text-sm font-medium hover:bg-muted"
                        >
                            "Next"
                        </a>
                    } else {
                        <span class="inline-flex min-h-[44px] items-center rounded-lg border border-border/40 px-4 text-sm text-muted-foreground">
                            "Next"
                        </span>
                    }
                </nav>
            }
        </div>
    })
}

/// Handles manual turnover creation (`POST /admin/turnovers/create`).
#[page(POST "/admin/turnovers/create")]
pub async fn create_turnover_action(
    cx: &Cx,
    /// Submitted manual turnover fields.
    Form(form): Form<CreateTurnoverForm>,
) -> Result<()> {
    let user = dispatch_actor(cx).await?;
    let token = service_token(cx, &user)?;
    let return_to = safe_turnover_return_target(form.return_to.as_deref());

    let Ok(listing_id) = Uuid::parse_str(form.listing_id.trim()) else {
        return Err(see_other(append_flash_params(
            &return_to,
            None,
            Some("Please select a valid property."),
        ))
        .into());
    };
    let Ok(checkout_date) = NaiveDate::parse_from_str(form.checkout_date.trim(), "%Y-%m-%d") else {
        return Err(see_other(append_flash_params(
            &return_to,
            None,
            Some("Please provide a valid date (YYYY-MM-DD)."),
        ))
        .into());
    };
    let initial_status =
        non_empty(form.initial_status.as_deref()).and_then(|s| TurnoverStatus::from_str(s).ok());
    let title = non_empty(form.title.as_deref()).map(ToString::to_string);

    let req = CreateManualTurnoverRequest {
        listing_id,
        checkout_date,
        title,
        initial_status,
    };

    match get_api_client(cx)
        .create_manual_turnover(&req, &token)
        .await
    {
        Ok(_) => Err(see_other(append_flash_params(&return_to, Some("created"), None)).into()),
        Err(err) => {
            let msg = describe_mutation_error(&err);
            tracing::warn!(user_id = %user.user_id, error = ?err, "manual turnover creation failed");
            Err(see_other(append_flash_params(&return_to, None, Some(&msg))).into())
        }
    }
}

/// Handles status transition submissions (`POST /admin/turnovers/{id}/status`).
/// Returns a refreshed card fragment for HTMX requests or redirects for plain form POSTs.
#[page(POST "/admin/turnovers/{id}/status")]
pub async fn transition_turnover_action(
    cx: &Cx,
    /// Status transition form fields.
    Form(form): Form<TransitionTurnoverForm>,
) -> Result<impl View> {
    let user = dispatch_actor(cx).await?;
    let token = service_token(cx, &user)?;
    let turnover_id = Uuid::parse_str(id(cx)).map_err(|_| not_found())?;
    let return_to = safe_turnover_return_target(form.return_to.as_deref());
    let is_htmx = hx_request(cx);

    let expected_status =
        non_empty(form.expected_status.as_deref()).and_then(|s| TurnoverStatus::from_str(s).ok());
    let reason = non_empty(form.reason.as_deref()).map(ToString::to_string);
    let api = get_api_client(cx);

    let parsed_outcome = match TurnoverStatus::from_str(form.to_status.trim()) {
        Ok(to_status) => Ok(api
            .transition_turnover_status(
                turnover_id,
                &TransitionTurnoverRequest {
                    to_status,
                    expected_status,
                    reason,
                },
                &token,
            )
            .await),
        Err(_) => Err("Unknown target turnover status.".to_string()),
    };

    if !is_htmx {
        return match parsed_outcome {
            Ok(Ok(_)) => {
                Err(see_other(append_flash_params(&return_to, Some("updated"), None)).into())
            }
            Ok(Err(err)) => {
                let msg = describe_mutation_error(&err);
                Err(see_other(append_flash_params(&return_to, None, Some(&msg))).into())
            }
            Err(msg) => Err(see_other(append_flash_params(&return_to, None, Some(&msg))).into()),
        };
    }

    let (refreshed_card, standalone_error) = match parsed_outcome {
        Ok(Ok(updated)) => (
            Some(build_card_vm(
                updated,
                user.role,
                &return_to,
                None,
                Some("Status updated.".to_string()),
            )),
            None,
        ),
        Ok(Err(err)) => {
            let msg = describe_mutation_error(&err);
            tracing::warn!(user_id = %user.user_id, %turnover_id, error = ?err, "turnover transition failed");
            match api.get_turnover_detail(turnover_id, &token).await {
                Ok(detail) => (
                    Some(build_card_vm(
                        detail.turnover,
                        user.role,
                        &return_to,
                        Some(msg),
                        None,
                    )),
                    None,
                ),
                Err(_) => (None, Some(msg)),
            }
        }
        Err(msg) => (None, Some(msg)),
    };

    Ok(view! {
        if let Some(card) = refreshed_card {
            turnover_card(card: card)
        } else if let Some(err_msg) = standalone_error {
            <div role="alert" class="rounded-xl border border-destructive/40 bg-destructive/10 p-4 text-sm text-destructive">
                (err_msg)
            </div>
        }
    })
}

/// Form payload submitted by `POST /admin/turnovers/{id}/comments`.
#[derive(Debug, Deserialize)]
pub struct CommentForm {
    /// Comment message body.
    pub body: String,
}

#[derive(Debug, Default, Deserialize)]
struct DetailNoticeQuery {
    #[serde(default)]
    notice: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct HistoryEntryVm {
    transition_label: String,
    actor_summary: String,
    timestamp: String,
    reason: Option<String>,
}

#[component]
async fn turnover_comment_bubble(
    cx: &Cx,
    comment: TurnoverCommentResponse,
    viewer_id: Uuid,
) -> Result<impl View> {
    let mine = comment.author_user_id == Some(viewer_id);
    let sent_at = comment.created_at.format("%b %d, %H:%M").to_string();
    let sender = format!("{} ({})", comment.author_name, comment.author_role);
    Ok(view! {
        chat_bubble(
            sender: sender,
            sent_at: sent_at,
            text: comment.body.clone(),
            mine: mine,
            attrs: topcoat::view::attributes! { id=(format!("turnover-comment-{}", comment.id)) }
        )
    })
}

/// Detailed view of a single turnover event including status history timeline and crew comments (`/admin/turnovers/{id}`).
#[page("/admin/turnovers/{id}")]
pub async fn turnover_detail_page(cx: &Cx) -> Result<impl View> {
    let user = dispatch_actor(cx).await?;
    let token = service_token(cx, &user)?;
    let turnover_id = Uuid::parse_str(id(cx)).map_err(|_| not_found())?;
    let q = parse_query_params::<DetailNoticeQuery>(cx).unwrap_or_default();

    let detail = match get_api_client(cx)
        .get_turnover_detail(turnover_id, &token)
        .await
    {
        Ok(detail) => detail,
        Err(UserCallError::NotFound) => return Err(not_found().into()),
        Err(UserCallError::Unauthorized) => return Err(forbidden().into()),
        Err(other) => {
            tracing::error!(user_id = %user.user_id, %turnover_id, error = ?other, "failed to load turnover detail");
            return Err(web_client::app_error(describe_board_error(&other)).into());
        }
    };

    let detail_url = format!("/admin/turnovers/{turnover_id}");
    let card = build_card_vm(detail.turnover, user.role, &detail_url, None, None);
    let comment_action = format!("/admin/turnovers/{turnover_id}/comments");
    let history_items: Vec<HistoryEntryVm> = detail
        .status_history
        .into_iter()
        .map(|entry| {
            let from_text = entry
                .from_status
                .map(|s| s.label().to_string())
                .unwrap_or_else(|| "Created".to_string());
            let who = entry
                .actor_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(|| entry.actor_role.clone());
            HistoryEntryVm {
                transition_label: format!("{from_text} -> {}", entry.to_status.label()),
                actor_summary: format!("{who} ({}, {})", entry.actor_role, entry.entry_point),
                timestamp: entry.created_at.format("%Y-%m-%d %H:%M UTC").to_string(),
                reason: entry.reason,
            }
        })
        .collect();
    let comments = detail.comments;
    let flash_notice = notice_message(non_empty(q.notice.as_deref())).map(ToString::to_string);
    let flash_error = non_empty(q.error.as_deref()).map(ToString::to_string);
    let has_transitions = !card.transitions.is_empty();
    let quick_action = card.status_action.clone();
    let quick_expected = card.status_slug.clone();
    let quick_return = detail_url.clone();
    let quick_transitions = card.transitions.clone();
    let note_action = card.status_action;
    let note_expected = card.status_slug;
    let note_transitions = card.transitions;

    Ok(view! {
        <div class="mx-auto max-w-4xl min-w-0 space-y-6 overflow-x-hidden px-3 py-4 sm:px-6">
            <div>
                <a
                    href="/admin/turnovers"
                    class="inline-flex min-h-[44px] items-center text-sm font-medium text-muted-foreground hover:text-foreground"
                >
                    "← Back to Turnover Dispatch"
                </a>
                <div class="mt-2 flex flex-wrap items-start justify-between gap-3">
                    <div class="min-w-0 space-y-1">
                        <div class="flex flex-wrap items-center gap-2">
                            <h1 class="text-2xl font-semibold tracking-tight break-words">
                                (card.listing_name)
                            </h1>
                            <span class="inline-flex items-center rounded-md border border-border px-2 py-0.5 text-xs text-muted-foreground">
                                (card.source_label)
                            </span>
                        </div>
                        if let Some(city) = card.listing_city {
                            <p class="text-sm text-muted-foreground">(city)</p>
                        }
                        if let Some(title) = card.title {
                            <p class="text-sm font-medium break-words">(title)</p>
                        }
                    </div>
                    <span class=(card.status_badge_class)>(card.status_label)</span>
                </div>
            </div>

            if let Some(msg) = flash_notice {
                <div
                    role="status"
                    class="rounded-xl border border-emerald-500/40 bg-emerald-500/10 p-4 text-sm text-emerald-800 dark:text-emerald-200"
                >
                    (msg)
                </div>
            }
            if let Some(err_msg) = flash_error {
                <div
                    role="alert"
                    class="rounded-xl border border-destructive/40 bg-destructive/10 p-4 text-sm text-destructive"
                >
                    (err_msg)
                </div>
            }

            <section class="rounded-xl border border-border bg-card p-4 sm:p-5 space-y-3">
                <dl class="grid grid-cols-1 gap-4 text-sm sm:grid-cols-3">
                    <div>
                        <dt class="text-xs text-muted-foreground">"Checkout Date"</dt>
                        <dd class="font-medium">(card.checkout_date)</dd>
                    </div>
                    <div>
                        <dt class="text-xs text-muted-foreground">"Next Check-In"</dt>
                        <dd class="font-medium">(card.next_checkin_text)</dd>
                    </div>
                    <div>
                        <dt class="text-xs text-muted-foreground">"Assigned Crew"</dt>
                        <dd class="font-medium break-words">
                            (card.assigned_staff_text.unwrap_or_else(|| "Unassigned".to_string()))
                        </dd>
                    </div>
                </dl>
                if let Some(policy) = card.policy_summary {
                    <p class="text-xs text-muted-foreground">(policy)</p>
                }

                if has_transitions {
                    <div class="space-y-3 border-t border-border pt-3">
                        <p class="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                            "Update Status"
                        </p>
                        <div class="flex flex-wrap gap-2">
                            for tr in quick_transitions {
                                <form method="post" action=(quick_action.clone())>
                                    <input type="hidden" name="to_status" value=(tr.slug.clone()) />
                                    <input type="hidden" name="expected_status" value=(quick_expected.clone()) />
                                    <input type="hidden" name="return_to" value=(quick_return.clone()) />
                                    <button
                                        type="submit"
                                        data-status-target=(tr.slug)
                                        class=(tr.button_class)
                                    >
                                        (tr.label)
                                    </button>
                                </form>
                            }
                        </div>
                        <form
                            method="post"
                            action=(note_action)
                            class="flex flex-col gap-2 sm:flex-row"
                        >
                            <input type="hidden" name="expected_status" value=(note_expected) />
                            <input type="hidden" name="return_to" value=(detail_url) />
                            <select
                                name="to_status"
                                aria-label="Target status with note"
                                class="min-h-[44px] rounded-lg border border-border bg-background px-3 text-sm"
                            >
                                for tr in note_transitions {
                                    <option value=(tr.slug)>(tr.label)</option>
                                }
                            </select>
                            <input
                                name="reason"
                                type="text"
                                maxlength="500"
                                placeholder="Optional status note (e.g. broken AC valve)"
                                class="min-h-[44px] flex-1 min-w-0 rounded-lg border border-border bg-background px-3 text-sm"
                            />
                            <button
                                type="submit"
                                class="inline-flex min-h-[44px] items-center justify-center rounded-lg border border-border px-4 text-sm font-medium hover:bg-muted"
                            >
                                "Update with Note"
                            </button>
                        </form>
                    </div>
                }
            </section>

            <section class="space-y-3">
                <h2 class="text-lg font-semibold">"Crew Comments"</h2>
                chat_thread(
                    attrs: topcoat::view::attributes! {
                        id="turnover-comments-list"
                        class="rounded-xl border border-border bg-card p-4"
                    },
                    if comments.is_empty() {
                        <p class="text-sm text-muted-foreground">"No crew comments yet."</p>
                    }
                    for comment in comments {
                        turnover_comment_bubble(comment: comment, viewer_id: user.user_id)
                    }
                )
                <form
                    id="turnover-comment-form"
                    method="post"
                    action=(comment_action.clone())
                    hx-post=(comment_action)
                    hx-target="#turnover-comments-list"
                    hx-swap="beforeend"
                    data-reset-on-success=""
                    class="flex flex-col gap-2 sm:flex-row"
                >
                    <label for="turnover-comment-body" class="sr-only">"Comment"</label>
                    <input
                        id="turnover-comment-body"
                        name="body"
                        required=(true)
                        maxlength="2000"
                        autocomplete="off"
                        placeholder="Add a note for the host & cleaning team..."
                        class="min-h-[44px] flex-1 min-w-0 rounded-lg border border-border bg-background px-3 text-sm"
                    />
                    <button
                        id="btn-post-turnover-comment"
                        type="submit"
                        class="inline-flex min-h-[44px] items-center justify-center rounded-lg bg-primary px-5 text-sm font-medium text-primary-foreground hover:opacity-90"
                    >
                        "Post Comment"
                    </button>
                </form>
            </section>

            <section class="space-y-3">
                <h2 class="text-lg font-semibold">"Status History"</h2>
                if history_items.is_empty() {
                    <p class="text-sm text-muted-foreground">"No status transitions recorded."</p>
                } else {
                    <ol class="space-y-2 rounded-xl border border-border bg-card p-4">
                        for h in history_items {
                            <li class="flex flex-col gap-0.5 border-b border-border/50 pb-2 text-sm last:border-b-0 last:pb-0">
                                <div class="flex flex-wrap items-center justify-between gap-2">
                                    <span class="font-medium">(h.transition_label)</span>
                                    <span class="text-xs text-muted-foreground">(h.timestamp)</span>
                                </div>
                                <span class="text-xs text-muted-foreground">(h.actor_summary)</span>
                                if let Some(reason) = h.reason {
                                    <p class="text-xs text-foreground break-words">(reason)</p>
                                }
                            </li>
                        }
                    </ol>
                }
            </section>
        </div>
    })
}

/// Posts a crew/host comment onto a turnover thread (`POST /admin/turnovers/{id}/comments`).
/// HTMX requests receive a single rendered `chat_bubble` fragment; plain form POSTs redirect back to detail.
#[page(POST "/admin/turnovers/{id}/comments")]
pub async fn post_turnover_comment_action(
    cx: &Cx,
    /// Submitted comment form payload.
    Form(form): Form<CommentForm>,
) -> Result<impl View> {
    let user = dispatch_actor(cx).await?;
    let token = service_token(cx, &user)?;
    let turnover_id = Uuid::parse_str(id(cx)).map_err(|_| not_found())?;
    let detail_url = format!("/admin/turnovers/{turnover_id}");
    let is_htmx = hx_request(cx);
    let trimmed = form.body.trim().to_string();

    let sent = if trimmed.is_empty() {
        Err("Write a comment before posting.".to_string())
    } else {
        match get_api_client(cx)
            .post_turnover_comment(
                turnover_id,
                &CreateTurnoverCommentRequest { body: trimmed },
                &token,
            )
            .await
        {
            Ok(comment) => Ok(comment),
            Err(UserCallError::NotFound) => return Err(not_found().into()),
            Err(UserCallError::Unauthorized) => return Err(forbidden().into()),
            Err(err) => {
                tracing::warn!(user_id = %user.user_id, %turnover_id, error = ?err, "failed to post turnover comment");
                Err(describe_mutation_error(&err))
            }
        }
    };

    if !is_htmx {
        return match sent {
            Ok(_) => {
                Err(see_other(append_flash_params(&detail_url, Some("commented"), None)).into())
            }
            Err(msg) => Err(see_other(append_flash_params(&detail_url, None, Some(&msg))).into()),
        };
    }

    Ok(view! {
        if let Ok(comment) = sent {
            turnover_comment_bubble(comment: comment, viewer_id: user.user_id)
        } else if let Err(error) = sent {
            <p role="alert" class="text-sm text-destructive">(error)</p>
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::turnover::TurnoverSourceKind;

    #[test]
    fn staff_card_omits_inspected_and_ready_and_booking_not_required_while_unsetting_inspected() {
        let now = Utc::now();
        let sample = TurnoverResponse {
            id: Uuid::now_v7(),
            listing_id: Uuid::now_v7(),
            listing_name: "Blue Lagoon".to_string(),
            listing_city: Some("Port Antonio".to_string()),
            checkin_policy_text: Some("3:00 PM".to_string()),
            checkout_policy_text: Some("11:00 AM".to_string()),
            source_kind: TurnoverSourceKind::Booking,
            checkout_date: NaiveDate::from_ymd_opt(2026, 12, 10).unwrap_or_default(),
            next_checkin_date: Some(NaiveDate::from_ymd_opt(2026, 12, 12).unwrap_or_default()),
            status: TurnoverStatus::CleaningCompleted,
            title: None,
            assigned_staff_names: vec!["Alex Cleaner".to_string()],
            comment_count: 1,
            updated_at: now,
        };

        let staff_vm = build_card_vm(
            sample.clone(),
            AdminRole::Staff,
            "/admin/turnovers",
            None,
            None,
        );
        let staff_targets: Vec<&str> = staff_vm
            .transitions
            .iter()
            .map(|t| t.slug.as_str())
            .collect();
        assert!(!staff_targets.contains(&"inspected_and_ready"));
        assert!(!staff_targets.contains(&"not_required"));
        assert!(staff_targets.contains(&"maintenance_required"));

        let mut inspected = sample.clone();
        inspected.status = TurnoverStatus::InspectedAndReady;
        let staff_inspected =
            build_card_vm(inspected, AdminRole::Staff, "/admin/turnovers", None, None);
        let unset_targets: Vec<&str> = staff_inspected
            .transitions
            .iter()
            .map(|t| t.slug.as_str())
            .collect();
        assert!(unset_targets.contains(&"maintenance_required"));
        assert!(unset_targets.contains(&"needs_cleaning"));
        assert!(!unset_targets.contains(&"inspected_and_ready"));

        let host_vm = build_card_vm(sample, AdminRole::Host, "/admin/turnovers", None, None);
        let host_targets: Vec<&str> = host_vm
            .transitions
            .iter()
            .map(|t| t.slug.as_str())
            .collect();
        assert!(host_targets.contains(&"inspected_and_ready"));
        assert!(host_targets.contains(&"not_required"));
    }

    #[test]
    fn return_target_and_flash_params_preserve_filters_and_reject_external_redirects() {
        assert_eq!(
            safe_turnover_return_target(Some("https://evil.example/phish")),
            "/admin/turnovers"
        );
        assert_eq!(
            safe_turnover_return_target(Some("/admin/bookings")),
            "/admin/turnovers"
        );
        let updated = append_flash_params(
            "/admin/turnovers?window=upcoming&error=stale",
            Some("updated"),
            None,
        );
        assert_eq!(updated, "/admin/turnovers?window=upcoming&notice=updated");
    }
}
