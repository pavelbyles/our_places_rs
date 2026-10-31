use common::models::UserResponse;
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{page, parse_query_params},
    view::{View, view},
};
use web_client::get_api_client;

use super::{INPUT, LABEL, is_admin_account};
use crate::support::admin_only;

/// Directory filters.
#[derive(Debug, Default, Deserialize)]
struct UserQuery {
    /// Matches name or email (server-side).
    q: Option<String>,
    /// `admin`, `host` or `booker`.
    role: Option<String>,
    /// `active` (default), `archived` or `all`.
    status: Option<String>,
    /// Outcome code from an action redirect.
    notice: Option<String>,
}

/// The message for an action outcome code (`None` for unknown codes, so no
/// arbitrary text is reflected) and whether it reports a failure.
fn notice_message(code: &str) -> Option<(&'static str, bool)> {
    match code {
        "updated" => Some(("Account saved.", false)),
        "archived" => Some(("Account archived and deactivated.", false)),
        "restored" => Some(("Account restored and reactivated.", false)),
        "purged" => Some(("Account permanently deleted.", false)),
        "admin-protected" => Some((
            "Administrator accounts cannot be archived or deleted.",
            true,
        )),
        "self" => Some(("You cannot archive or delete your own account.", true)),
        "linked" => Some((
            "This account has bookings, listings or financial records. Archive it instead.",
            true,
        )),
        "failed" => Some(("That action failed. Please try again.", true)),
        _ => None,
    }
}

/// `is_deleted` for the user API: `Some(false)` active, `Some(true)`
/// archived, `None` both.
fn deleted_filter(status: &str) -> Option<bool> {
    match status {
        "archived" => Some(true),
        "all" => None,
        _ => Some(false),
    }
}

/// The user directory: search, role and status filters, and per-row
/// edit, archive, restore and delete.
#[page("/admin/users")]
pub async fn users_page(cx: &Cx) -> Result<impl View> {
    let admin = admin_only(cx).await?;
    let q = parse_query_params::<UserQuery>(cx).unwrap_or_default();
    let search = q.q.map(|s| s.trim().to_string()).unwrap_or_default();
    let role = q.role.map(|r| r.trim().to_lowercase()).unwrap_or_default();
    let status = q.status.unwrap_or_else(|| "active".to_string());
    let notice = q.notice.as_deref().and_then(notice_message);
    let loaded = get_api_client(cx)
        .get_all_users_filtered(
            Some(1),
            Some(100),
            Some(search.clone()).filter(|s| !s.is_empty()),
            deleted_filter(&status),
        )
        .await;
    let load_failed = loaded.as_ref().is_err_and(|e| {
        tracing::error!("user directory failed to load: {e}");
        true
    });
    let users: Vec<UserResponse> = loaded
        .unwrap_or_default()
        .into_iter()
        .filter(|u| role.is_empty() || u.roles.iter().any(|r| r.eq_ignore_ascii_case(&role)))
        .collect();
    let confirmations: Vec<UserResponse> = users
        .iter()
        .filter(|u| !is_admin_account(u) && u.id != admin.user_id)
        .cloned()
        .collect();
    let statuses = [
        ("active", "Active"),
        ("archived", "Archived"),
        ("all", "All"),
    ];
    let roles = [
        ("", "All roles"),
        ("admin", "Administrators"),
        ("host", "Hosts"),
        ("booker", "Guests"),
    ];
    let button = "h-8 rounded-lg border border-border px-3 text-xs font-medium";
    Ok(view! {
        <div class="space-y-6">
            <div class="flex flex-wrap items-end justify-between gap-4">
                <div>
                    <p class="text-xs font-medium tracking-wide text-muted-foreground uppercase">"Access control and accounts"</p>
                    <h1 class="text-2xl font-semibold">"Users"</h1>
                </div>
                <a href="/admin/users/new" class="inline-flex h-10 items-center rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground no-underline">"New user"</a>
            </div>
            if let Some((message, failed)) = notice {
                <p id="users-notice" role=(if failed { "alert" } else { "status" }) class=(if failed { "rounded-lg border border-destructive p-3 text-sm text-destructive" } else { "rounded-lg border border-border p-3 text-sm" })>(message)</p>
            }
            if load_failed {
                <p role="alert" class="rounded-lg border border-destructive p-3 text-sm text-destructive">"The user directory could not be loaded. Please try again shortly."</p>
            }
            <form method="get" action="/admin/users" role="search" class="flex flex-wrap items-end gap-3">
                <div class="min-w-64 flex-1"><label class=(LABEL) for="user-search-input">"Search name or email"</label>
                    <input id="user-search-input" name="q" type="search" value=(search) class=(INPUT) /></div>
                <div><label class=(LABEL) for="user-role-filter">"Role"</label>
                    <select id="user-role-filter" name="role" class=(INPUT)>
                        for (value, label) in roles { <option value=(value) selected=(role == value)>(label)</option> }
                    </select></div>
                <div><label class=(LABEL) for="user-status-filter">"Status"</label>
                    <select id="user-status-filter" name="status" class=(INPUT)>
                        for (value, label) in statuses { <option value=(value) selected=(status == value)>(label)</option> }
                    </select></div>
                <button type="submit" class="h-10 rounded-lg border border-border px-4 text-sm">"Filter"</button>
            </form>
            <p class="text-xs text-muted-foreground">(format!("{} account(s) shown", users.len()))</p>
            <div class="overflow-x-auto rounded-xl border border-border">
                <table id="users-table" class="w-full text-sm">
                    <thead class="bg-card text-left text-xs text-muted-foreground uppercase">
                        <tr>
                            <th scope="col" class="p-3">"Name"</th><th scope="col" class="p-3">"Email"</th>
                            <th scope="col" class="p-3">"Roles"</th><th scope="col" class="p-3">"Status"</th>
                            <th scope="col" class="p-3"><span class="sr-only">"Actions"</span></th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-border">
                        if users.is_empty() {
                            <tr><td colspan="5" class="p-6 text-center text-muted-foreground">"No accounts match."</td></tr>
                        }
                        for u in users {
                            let archived = u.deleted_at.is_some();
                            let protected = is_admin_account(&u) || u.id == admin.user_id;
                            let state = if archived { "Archived" } else if u.is_active { "Active" } else { "Inactive" };
                            <tr id=(format!("user-{}", u.id))>
                                <td class="p-3 font-medium">(format!("{} {}", u.first_name, u.last_name))</td>
                                <td class="p-3">(u.email.clone())</td>
                                <td class="p-3 capitalize">(u.roles.join(", "))</td>
                                <td class="p-3">(state)</td>
                                <td class="p-3">
                                    <div class="flex flex-wrap justify-end gap-2">
                                        <button type="button" hx-get=(format!("/admin/users/{}/edit", u.id)) hx-target="#user-dialog-slot" hx-swap="innerHTML" class=(button)>"Edit"</button>
                                        if archived {
                                            <form method="post" action=(format!("/admin/users/{}/restore", u.id))>
                                                <button type="submit" class=(button)>"Restore"</button>
                                            </form>
                                        }
                                        if !protected {
                                            if !archived {
                                                <button type="button" data-dialog-open=(format!("archive-user-{}", u.id)) class=(button)>"Archive"</button>
                                            }
                                            <button type="button" data-dialog-open=(format!("purge-user-{}", u.id)) class=(format!("{button} text-destructive"))>"Delete permanently"</button>
                                        }
                                    </div>
                                </td>
                            </tr>
                        }
                    </tbody>
                </table>
            </div>
            for u in confirmations {
                let name = format!("{} {} ({})", u.first_name, u.last_name, u.email);
                for (kind, purge, title, body, confirm) in [
                    ("archive", "false", "Archive this account?", "They will be signed out and unable to sign in until restored.", "Archive account"),
                    ("purge", "true", "Delete this account permanently?", "This cannot be undone. Accounts with bookings, listings or financial records can only be archived.", "Delete permanently"),
                ] {
                    <dialog id=(format!("{kind}-user-{}", u.id)) role="alertdialog" aria-labelledby=(format!("{kind}-user-{}-title", u.id)) class="m-auto w-[min(480px,92vw)] rounded-xl bg-card p-6 text-card-foreground backdrop:bg-black/50">
                        <form method="post" action=(format!("/admin/users/{}/delete", u.id)) class="space-y-4">
                            <input type="hidden" name="purge" value=(purge) />
                            <h2 id=(format!("{kind}-user-{}-title", u.id)) class="text-lg font-semibold">(title)</h2>
                            <p class="text-sm">(name.clone())</p>
                            <p class="text-sm text-muted-foreground">(body)</p>
                            <div class="flex justify-end gap-2">
                                <button type="button" data-dialog-close="" class="h-10 rounded-lg border border-border px-4 text-sm">"Cancel"</button>
                                <button type="submit" class="h-10 rounded-lg bg-destructive px-4 text-sm font-medium text-destructive-foreground">(confirm)</button>
                            </div>
                        </form>
                    </dialog>
                }
            }
            <div id="user-dialog-slot"></div>
        </div>
    })
}

#[cfg(test)]
mod tests {
    use super::{deleted_filter, notice_message};

    #[test]
    fn reflects_only_known_notice_codes() {
        assert!(notice_message("<script>").is_none());
        assert_eq!(
            notice_message("linked").map(|(_, failed)| failed),
            Some(true)
        );
        assert_eq!(
            notice_message("archived").map(|(_, failed)| failed),
            Some(false)
        );
    }

    #[test]
    fn maps_status_to_the_deleted_filter() {
        assert_eq!(deleted_filter("active"), Some(false));
        assert_eq!(deleted_filter("archived"), Some(true));
        assert_eq!(deleted_filter("all"), None);
        assert_eq!(deleted_filter("bogus"), Some(false));
    }
}
