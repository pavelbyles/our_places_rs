//! Host-scoped cleaning/maintenance staff management (`/admin/staff-users`).

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use common::app_client::encode_query_component;
use common::models::ListingResponse;
use common::turnover::{
    InviteStaffRequest, StaffListQuery, StaffListResponse, StaffMemberSummary,
    UpdateStaffAssignmentsRequest, is_valid_trimmed_email,
};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form,
        error::{forbidden, not_found, see_other, service_unavailable},
        page, parse_query_params, path_param,
    },
    view::{View, component, view},
};
use uuid::Uuid;
use web_client::{client::UserCallError, get_api_client};

use crate::support::{host_or_admin, scoped_listings, service_token};

path_param!(id);

const LABEL: &str = "block text-xs font-semibold tracking-wide text-muted-foreground uppercase";
const INPUT: &str =
    "mt-1 block h-11 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground";
const CARD: &str = "rounded-xl border border-border bg-card p-6 text-card-foreground space-y-4";
const BTN_PRIMARY: &str = "inline-flex h-10 items-center justify-center rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground";
const BTN_SECONDARY: &str = "inline-flex h-10 items-center justify-center rounded-lg border border-border px-4 text-sm font-medium text-foreground";

#[derive(Deserialize, Default)]
struct StaffQuery {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    page: Option<u32>,
    #[serde(default)]
    notice: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

/// Invite form payload decoded via flattened checkbox fields (`listing_<uuid>=on`).
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct InviteStaffForm {
    /// Target cleaner email address.
    pub email: String,
    /// Optional first name for brand-new accounts.
    pub first_name: String,
    /// Optional last name for brand-new accounts.
    pub last_name: String,
    /// Active search query preserved across redirects.
    pub return_q: String,
    /// Flattened checkbox inputs (`listing_<uuid> = "on"`).
    #[serde(flatten)]
    pub fields: BTreeMap<String, String>,
}

/// Assignment update form payload decoded via flattened checkbox fields (`listing_<uuid>=on`).
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct UpdateListingsForm {
    /// Active search query preserved across redirects.
    pub return_q: String,
    /// Flattened checkbox inputs (`listing_<uuid> = "on"`).
    #[serde(flatten)]
    pub fields: BTreeMap<String, String>,
}

/// Optional search query preserved when resending invites or removing staff.
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct RosterActionForm {
    /// Active search query preserved across redirects.
    pub return_q: String,
}

fn selected_listing_ids(fields: &BTreeMap<String, String>) -> Vec<Uuid> {
    let mut ids = Vec::new();
    for (key, value) in fields {
        if let Some(raw_id) = key.strip_prefix("listing_")
            && !value.trim().is_empty()
            && let Ok(parsed) = Uuid::from_str(raw_id)
        {
            ids.push(parsed);
        }
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn redirect_url(q: &str, notice: Option<&str>, error: Option<&str>) -> String {
    let mut params = Vec::new();
    let trimmed_q = q.trim();
    if !trimmed_q.is_empty() {
        params.push(format!("q={}", encode_query_component(trimmed_q)));
    }
    if let Some(code) = notice {
        params.push(format!("notice={}", encode_query_component(code)));
    }
    if let Some(msg) = error {
        params.push(format!("error={}", encode_query_component(msg)));
    }
    if params.is_empty() {
        "/admin/staff-users".to_string()
    } else {
        format!("/admin/staff-users?{}", params.join("&"))
    }
}

fn notice_message(code: Option<&str>) -> Option<&'static str> {
    match code {
        Some("invited") => Some("Staff member saved and onboarding notification queued."),
        Some("listings_updated") => Some("Property assignments updated."),
        Some("invite_resent") => Some("A fresh password setup invite link has been queued."),
        Some("removed") => Some("Staff member removed from your roster."),
        _ => None,
    }
}

fn map_api_error(err: UserCallError) -> String {
    match err {
        UserCallError::Api(rej) => rej.message,
        UserCallError::Unauthorized => {
            "You are not authorized to modify this staff assignment.".to_string()
        }
        UserCallError::NotFound => "That staff member was not found in your roster.".to_string(),
        _ => "Staff service is temporarily unavailable. Please try again.".to_string(),
    }
}

fn staff_display_name(member: &StaffMemberSummary) -> String {
    let full = format!("{} {}", member.first_name, member.last_name)
        .trim()
        .to_string();
    if full.is_empty() {
        member.email.clone()
    } else {
        full
    }
}

#[derive(Clone)]
struct ListingOption {
    id: Uuid,
    name: String,
    checked: bool,
}

#[component]
async fn staff_card(
    cx: &Cx,
    member: StaffMemberSummary,
    owned_listings: Vec<ListingResponse>,
    current_q: String,
) -> Result<impl View> {
    let assigned_set: BTreeSet<Uuid> = member.listings.iter().map(|l| l.listing_id).collect();
    let options: Vec<ListingOption> = owned_listings
        .into_iter()
        .map(|l| ListingOption {
            checked: assigned_set.contains(&l.id),
            id: l.id,
            name: l.name,
        })
        .collect();
    let staff_id = member.user_id;
    let display_name = staff_display_name(&member);
    let status_label = if member.password_pending {
        "Pending password setup"
    } else {
        "Active"
    };
    let listings_action = format!("/admin/staff-users/{staff_id}/listings");
    let resend_action = format!("/admin/staff-users/{staff_id}/resend-invite");
    let remove_action = format!("/admin/staff-users/{staff_id}/remove");

    Ok(view! {
        <article
            id=(format!("staff-card-{staff_id}"))
            class="rounded-xl border border-border bg-card p-6 text-card-foreground space-y-4"
        >
            <div class="flex flex-wrap items-start justify-between gap-4">
                <div>
                    <h3 class="text-lg font-semibold">(display_name)</h3>
                    <p class="text-sm text-muted-foreground">(member.email)</p>
                </div>
                <span
                    id=(format!("staff-status-{staff_id}"))
                    class="inline-flex items-center rounded-full border border-border px-3 py-1 text-xs font-medium"
                >
                    (status_label)
                </span>
            </div>

            <form method="post" action=(listings_action) class="space-y-3">
                <input type="hidden" name="return_q" value=(current_q.clone()) />
                <p class="text-xs font-semibold tracking-wide text-muted-foreground uppercase">
                    "Assigned properties"
                </p>
                if options.is_empty() {
                    <p class="text-sm text-muted-foreground">
                        "You have no active properties to assign."
                    </p>
                } else {
                    <div class="grid gap-2 sm:grid-cols-2">
                        for opt in options {
                            <label class="flex items-center gap-2 text-sm">
                                <input
                                    type="checkbox"
                                    name=(format!("listing_{}", opt.id))
                                    value="on"
                                    checked=(opt.checked)
                                    class="size-4"
                                />
                                <span>(opt.name)</span>
                            </label>
                        }
                    </div>
                    <button type="submit" class=(BTN_SECONDARY)>"Save assignments"</button>
                }
            </form>

            <div class="flex flex-wrap items-center gap-3 border-t border-border pt-4">
                if member.password_pending {
                    <form method="post" action=(resend_action)>
                        <input type="hidden" name="return_q" value=(current_q.clone()) />
                        <button
                            id=(format!("btn-resend-invite-{staff_id}"))
                            type="submit"
                            class=(BTN_SECONDARY)
                        >
                            "Resend setup invite"
                        </button>
                    </form>
                }
                <details id=(format!("staff-remove-confirm-{staff_id}")) class="relative">
                    <summary
                        class="inline-flex h-10 cursor-pointer list-none items-center rounded-lg border border-destructive px-4 text-sm font-medium text-destructive"
                    >
                        "Remove from roster"
                    </summary>
                    <div class="mt-2 space-y-2 rounded-lg border border-destructive bg-card p-3">
                        <p class="text-xs text-muted-foreground">
                            "Unlinks this cleaner from your properties and ends their dispatch access to your listings."
                        </p>
                        <form method="post" action=(remove_action)>
                            <input type="hidden" name="return_q" value=(current_q) />
                            <button
                                id=(format!("btn-confirm-remove-{staff_id}"))
                                type="submit"
                                class="inline-flex h-9 items-center rounded-lg bg-destructive px-3 text-xs font-medium text-destructive-foreground"
                            >
                                "Confirm removal"
                            </button>
                        </form>
                    </div>
                </details>
            </div>
        </article>
    })
}

/// Host-scoped staff & cleaning crew management page.
#[page("/admin/staff-users")]
pub async fn staff_users_page(cx: &Cx) -> Result<impl View> {
    let actor = host_or_admin(cx).await?;
    let query = parse_query_params::<StaffQuery>(cx).unwrap_or_default();
    let search_q = query.q.unwrap_or_default();
    let page = query.page.unwrap_or(1).max(1);
    let token = service_token(cx, &actor)?;

    let mut owned_listings = scoped_listings(cx, &actor).await;
    owned_listings.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));

    let api = get_api_client(cx);
    let list_req = StaffListQuery {
        q: (!search_q.trim().is_empty()).then(|| search_q.trim().to_string()),
        page: Some(page),
        per_page: Some(20),
    };
    let roster: StaffListResponse =
        api.list_staff_users(&list_req, &token)
            .await
            .map_err(|err| -> topcoat::Error {
                match err {
                    UserCallError::Unauthorized => forbidden().into(),
                    other => {
                        tracing::error!(error = %other, "failed to load staff roster");
                        service_unavailable(5).into()
                    }
                }
            })?;

    let notice = notice_message(query.notice.as_deref()).map(str::to_string);
    let error = query
        .error
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let empty_roster = roster.staff.is_empty();
    let no_listings = owned_listings.is_empty();

    Ok(view! {
        <div class="mx-auto max-w-5xl space-y-6">
            <div>
                <p class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
                    "Operations & housekeeping"
                </p>
                <h1 class="text-2xl font-semibold">"Staff & Crew"</h1>
                <p class="text-sm text-muted-foreground">
                    "Invite cleaners or maintenance staff by email and choose which properties they can view on Turnover Dispatch."
                </p>
            </div>

            if let Some(notice) = notice {
                <div
                    id="staff-users-notice"
                    role="status"
                    class="rounded-lg border border-border bg-muted/40 p-3 text-sm text-foreground"
                >
                    (notice)
                </div>
            }
            if let Some(error) = error {
                <div
                    id="staff-users-error"
                    role="alert"
                    class="rounded-lg border border-destructive p-3 text-sm text-destructive"
                >
                    (error)
                </div>
            }

            <section class=(CARD)>
                <h2 class="text-lg font-semibold">"Invite staff member"</h2>
                <form
                    id="staff-invite-form"
                    method="post"
                    action="/admin/staff-users"
                    class="space-y-4"
                >
                    <input type="hidden" name="return_q" value=(search_q.clone()) />
                    <div class="grid gap-4 sm:grid-cols-3">
                        <div>
                            <label class=(LABEL) for="staff-invite-email">"Email address"</label>
                            <input
                                id="staff-invite-email"
                                name="email"
                                type="email"
                                required=(true)
                                autocomplete="email"
                                class=(INPUT)
                            />
                        </div>
                        <div>
                            <label class=(LABEL) for="staff-invite-first-name">"First name"</label>
                            <input
                                id="staff-invite-first-name"
                                name="first_name"
                                type="text"
                                class=(INPUT)
                            />
                        </div>
                        <div>
                            <label class=(LABEL) for="staff-invite-last-name">"Last name"</label>
                            <input
                                id="staff-invite-last-name"
                                name="last_name"
                                type="text"
                                class=(INPUT)
                            />
                        </div>
                    </div>

                    <div class="space-y-2">
                        <p class=(LABEL)>"Assign properties (at least one)"</p>
                        if no_listings {
                            <p class="text-sm text-muted-foreground">
                                "Create an active listing first before inviting staff."
                            </p>
                        } else {
                            <div class="grid gap-2 sm:grid-cols-2">
                                for listing in owned_listings.clone() {
                                    <label class="flex items-center gap-2 text-sm">
                                        <input
                                            type="checkbox"
                                            name=(format!("listing_{}", listing.id))
                                            value="on"
                                            class="size-4"
                                        />
                                        <span>(listing.name)</span>
                                    </label>
                                }
                            </div>
                        }
                    </div>

                    <button
                        id="btn-staff-invite-submit"
                        type="submit"
                        class=(BTN_PRIMARY)
                    >
                        "Send invite & assign"
                    </button>
                </form>
            </section>

            <section class="space-y-4">
                <div class="flex flex-wrap items-center justify-between gap-3">
                    <h2 class="text-lg font-semibold">"Your crew roster"</h2>
                    <form method="get" action="/admin/staff-users" class="flex items-center gap-2">
                        <input
                            id="staff-search-q"
                            name="q"
                            type="search"
                            value=(search_q.clone())
                            placeholder="Search name or email"
                            class="h-10 rounded-lg border border-border bg-card px-3 text-sm text-foreground"
                        />
                        <button type="submit" class=(BTN_SECONDARY)>"Search"</button>
                    </form>
                </div>

                if empty_roster {
                    <p
                        id="staff-roster-empty"
                        class="rounded-xl border border-border bg-card p-6 text-sm text-muted-foreground"
                    >
                        "No staff members found in this view."
                    </p>
                } else {
                    <div class="space-y-4">
                        for member in roster.staff {
                            staff_card(
                                member: member,
                                owned_listings: owned_listings.clone(),
                                current_q: search_q.clone(),
                            )
                        }
                    </div>
                }
            </section>
        </div>
    })
}

/// Invites or attaches a staff member from `/admin/staff-users`.
#[page(POST "/admin/staff-users")]
pub async fn invite_staff_submit(
    cx: &Cx,
    /// Submitted invite form.
    Form(form): Form<InviteStaffForm>,
) -> Result<()> {
    let actor = host_or_admin(cx).await?;
    let email = form.email.trim().to_string();
    if !is_valid_trimmed_email(&email) {
        return Err(see_other(redirect_url(
            &form.return_q,
            None,
            Some("Enter a valid staff email address."),
        ))
        .into());
    }
    let listing_ids = selected_listing_ids(&form.fields);
    if listing_ids.is_empty() {
        return Err(see_other(redirect_url(
            &form.return_q,
            None,
            Some("Select at least one listing to assign."),
        ))
        .into());
    }
    let req = InviteStaffRequest {
        email,
        first_name: (!form.first_name.trim().is_empty())
            .then(|| form.first_name.trim().to_string()),
        last_name: (!form.last_name.trim().is_empty()).then(|| form.last_name.trim().to_string()),
        listing_ids,
    };
    let token = service_token(cx, &actor)?;
    let target = match get_api_client(cx).invite_staff_user(&req, &token).await {
        Ok(_) => redirect_url(&form.return_q, Some("invited"), None),
        Err(err) => redirect_url(&form.return_q, None, Some(&map_api_error(err))),
    };
    Err(see_other(target).into())
}

/// Updates assigned owned listings for an existing roster member.
#[page(POST "/admin/staff-users/{id}/listings")]
pub async fn update_staff_listings_submit(
    cx: &Cx,
    /// Submitted property assignment checkboxes.
    Form(form): Form<UpdateListingsForm>,
) -> Result<()> {
    let actor = host_or_admin(cx).await?;
    let staff_id = Uuid::from_str(id(cx)).map_err(|_| not_found())?;
    let listing_ids = selected_listing_ids(&form.fields);
    if listing_ids.is_empty() {
        return Err(see_other(redirect_url(
            &form.return_q,
            None,
            Some("Select at least one listing, or use Remove from roster."),
        ))
        .into());
    }
    let token = service_token(cx, &actor)?;
    let req = UpdateStaffAssignmentsRequest { listing_ids };
    let target = match get_api_client(cx)
        .update_staff_listings(staff_id, &req, &token)
        .await
    {
        Ok(_) => redirect_url(&form.return_q, Some("listings_updated"), None),
        Err(err) => redirect_url(&form.return_q, None, Some(&map_api_error(err))),
    };
    Err(see_other(target).into())
}

/// Resends the password-setup onboarding link when the staff member has not yet set a password.
#[page(POST "/admin/staff-users/{id}/resend-invite")]
pub async fn resend_staff_invite_submit(
    cx: &Cx,
    /// Optional preserved search query.
    Form(form): Form<RosterActionForm>,
) -> Result<()> {
    let actor = host_or_admin(cx).await?;
    let staff_id = Uuid::from_str(id(cx)).map_err(|_| not_found())?;
    let token = service_token(cx, &actor)?;
    let target = match get_api_client(cx)
        .resend_staff_invite(staff_id, &token)
        .await
    {
        Ok(()) => redirect_url(&form.return_q, Some("invite_resent"), None),
        Err(err) => redirect_url(&form.return_q, None, Some(&map_api_error(err))),
    };
    Err(see_other(target).into())
}

/// Unlinks the staff member from the caller's roster and owned properties.
#[page(POST "/admin/staff-users/{id}/remove")]
pub async fn remove_staff_submit(
    cx: &Cx,
    /// Optional preserved search query.
    Form(form): Form<RosterActionForm>,
) -> Result<()> {
    let actor = host_or_admin(cx).await?;
    let staff_id = Uuid::from_str(id(cx)).map_err(|_| not_found())?;
    let token = service_token(cx, &actor)?;
    let target = match get_api_client(cx).remove_staff_user(staff_id, &token).await {
        Ok(()) => redirect_url(&form.return_q, Some("removed"), None),
        Err(err) => redirect_url(&form.return_q, None, Some(&map_api_error(err))),
    };
    Err(see_other(target).into())
}
