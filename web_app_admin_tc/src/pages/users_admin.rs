use common::models::{UpdateUserRequest, UserResponse};
use serde::{Deserialize, Serialize};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Json, page, parse_query_params, route},
    view::view,
};
use uuid::Uuid;
use web_app_common_tc::get_api_client;

#[derive(Debug, Default, Deserialize)]
pub struct UserFilterQuery {
    pub q: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AdminUpdateUserPayload {
    pub id: Uuid,
    pub email: Option<String>,
    pub password: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub phone_number: Option<String>,
    pub is_active: Option<bool>,
    pub is_verified: Option<bool>,
    pub roles: Option<Vec<String>>,
    pub can_manage_bookings: Option<bool>,
    pub can_manage_listings: Option<bool>,
    pub default_currency: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminUpdateUserResponse {
    pub success: bool,
    pub message: Option<String>,
}

#[route(POST "/api/admin/users/update")]
pub async fn admin_update_user_api(
    cx: &Cx,
    Json(payload): Json<AdminUpdateUserPayload>,
) -> Result<Json<AdminUpdateUserResponse>> {
    if let Err(err) = web_app_common_tc::auth::require_admin_role_auth(cx).await {
        return Ok(Json(AdminUpdateUserResponse {
            success: false,
            message: Some(format!("Forbidden: {:?}", err)),
        }));
    }

    let api = get_api_client(cx);

    let mut attrs = serde_json::Map::new();
    if let Some(cmb) = payload.can_manage_bookings {
        attrs.insert("can_manage_bookings".to_string(), serde_json::Value::Bool(cmb));
    }
    if let Some(cml) = payload.can_manage_listings {
        attrs.insert("can_manage_listings".to_string(), serde_json::Value::Bool(cml));
    }

    let pwd = payload.password.filter(|p| !p.trim().is_empty());

    let req = UpdateUserRequest {
        email: payload.email.filter(|e| !e.trim().is_empty()),
        password: pwd,
        first_name: payload.first_name,
        last_name: payload.last_name,
        phone_number: payload.phone_number,
        is_active: payload.is_active,
        is_verified: payload.is_verified,
        attributes: Some(serde_json::Value::Object(attrs)),
        roles: payload.roles,
        booker_profile: None,
        host_profile: None,
        default_currency: payload.default_currency,
    };

    match api.update_user(payload.id, &req).await {
        Ok(_) => Ok(Json(AdminUpdateUserResponse {
            success: true,
            message: Some("User profile & credentials updated successfully.".to_string()),
        })),
        Err(err) => {
            tracing::error!("Failed to update user via user_api: {:?}", err);
            Ok(Json(AdminUpdateUserResponse {
                success: false,
                message: Some(format!("Update failed: {}", err)),
            }))
        }
    }
}

#[page("/admin/users")]
pub async fn admin_users_page(cx: &Cx) -> Result {
    render_users_directory(cx).await
}

#[page("/users")]
pub async fn users_alias_page(cx: &Cx) -> Result {
    render_users_directory(cx).await
}

async fn render_users_directory(cx: &Cx) -> Result {
    match web_app_common_tc::auth::require_admin_role_auth(cx).await {
        Ok(_) => {},
        Err(web_app_common_tc::auth::AdminAuthError::Forbidden(_)) => {
            return view! {
                <div class="p-8 text-center">
                    <div class="alert alert-error max-w-md mx-auto shadow-lg">
                        <span>"Access Denied: Administrative privileges are required to manage users."</span>
                    </div>
                </div>
                <script>
                    r#"window.location.replace('/admin');"#
                </script>
            };
        }
        Err(_) => {
            return view! {
                <script>
                    r#"window.location.replace('/login?redirect=' + encodeURIComponent(window.location.pathname + window.location.search));"#
                </script>
            };
        }
    }

    let filter_query = parse_query_params::<UserFilterQuery>(cx).unwrap_or_default();
    let search_filter = filter_query
        .q
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let role_filter = filter_query
        .role
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());

    let __cx = cx;
    let api = get_api_client(cx);

    let all_users = match api.get_all_users(Some(1), Some(100), None).await {
        Ok(users) => users,
        Err(err) => {
            tracing::error!("Failed to fetch users from user_api: {:?}", err);
            Vec::new()
        }
    };
    let total_count = all_users.len();

    let users: Vec<UserResponse> = all_users
        .into_iter()
        .filter(|u| {
            let matches_role = match &role_filter {
                Some(r) if !r.is_empty() => u.roles.iter().any(|ur| ur.to_lowercase() == *r),
                _ => true,
            };

            let matches_search = match &search_filter {
                Some(q) if !q.is_empty() => {
                    let q_lower = q.to_lowercase();
                    let full_name = format!("{} {}", u.first_name, u.last_name).to_lowercase();
                    full_name.contains(&q_lower)
                        || u.email.to_lowercase().contains(&q_lower)
                        || u.id.to_string().contains(&q_lower)
                }
                _ => true,
            };

            matches_role && matches_search
        })
        .collect();
    let filtered_count = users.len();

    view! {
        <div class="space-y-8 py-6 max-w-7xl mx-auto px-4 md:px-6">
            // Header
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 border-b border-base-200 pb-4">
                <div class="space-y-1">
                    <span class="text-primary font-bold tracking-widest uppercase text-xs">"Access Control and Accounts"</span>
                    <h1 class="text-3xl font-serif font-bold tracking-tight text-base-content">
                        "User Directory and Roles"
                    </h1>
                    <p class="text-xs text-base-content/60">
                        "Manage host profiles, administrator permissions, credentials, and user account status."
                    </p>
                </div>
                <a href="/admin/users/new" class="btn btn-primary btn-sm rounded-full px-5 font-bold tracking-wide shadow-md">
                    "+ Invite / Create User"
                </a>
            </div>

            // Filter Bar with Search Button
            <div class="bg-base-100 dark:bg-base-200/80 p-4 rounded-2xl border border-base-200 dark:border-base-100/20 shadow-sm">
                <form id="user-filter-form" method="GET" action="/admin/users" class="w-full flex flex-col md:flex-row items-center justify-between gap-4">
                    <div class="flex-1 w-full flex flex-col sm:flex-row items-center gap-3">
                        <div class="relative w-full max-w-md">
                            <span class="absolute inset-y-0 left-0 flex items-center pl-3 pointer-events-none opacity-50">"🔍"</span>
                            <input
                                type="text"
                                name="q"
                                id="user-search-input"
                                value=(search_filter.as_deref().unwrap_or_default())
                                placeholder="Search by name, email, or user UUID..."
                                class="input input-bordered input-sm w-full pl-9 rounded-xl font-medium"
                                oninput="applyClientFilter()"
                            />
                        </div>
                        <select
                            name="role"
                            id="user-role-select"
                            class="select select-bordered select-sm rounded-xl font-medium w-full sm:w-auto"
                            onchange="applyClientFilter()"
                        >
                            <option value="" selected=(role_filter.is_none())>"All Roles"</option>
                            <option value="admin" selected=(role_filter.as_deref() == Some("admin"))>"Administrator"</option>
                            <option value="host" selected=(role_filter.as_deref() == Some("host"))>"Host / Property Owner"</option>
                            <option value="booker" selected=(role_filter.as_deref() == Some("booker"))>"Guest / Booker"</option>
                        </select>
                        <div class="flex items-center gap-2 w-full sm:w-auto">
                            <button
                                type="submit"
                                id="user-search-submit-btn"
                                class="btn btn-primary btn-sm rounded-xl font-bold px-4 gap-1.5 shadow-sm"
                            >
                                <span>"Filter"</span>
                            </button>
                            if search_filter.is_some() || role_filter.is_some() {
                                <a href="/admin/users" class="btn btn-ghost btn-sm rounded-xl font-semibold text-xs">
                                    "✕ Reset"
                                </a>
                            }
                        </div>
                    </div>
                    <div class="text-xs text-base-content/60 font-semibold whitespace-nowrap" id="user-results-count">
                        (format!("{} Matching of {} Accounts", filtered_count, total_count))
                    </div>
                </form>
            </div>

            // Users Table
            <div class="bg-base-100 dark:bg-base-200 rounded-2xl border border-base-200 dark:border-base-100/20 shadow-md overflow-hidden">
                <div class="overflow-x-auto">
                    <table class="table table-zebra w-full">
                        <thead>
                            <tr class="text-xs text-base-content/60 uppercase tracking-wider">
                                <th>"User"</th>
                                <th>"Roles and Permissions"</th>
                                <th>"Verification"</th>
                                <th>"Properties and Bookings"</th>
                                <th>"Account Type"</th>
                                <th class="text-right">"Actions"</th>
                            </tr>
                        </thead>
                        <tbody>
                            if users.is_empty() {
                                <tr>
                                    <td colspan="6" class="text-center py-12 text-base-content/60 text-sm">
                                        "No user accounts found in database."
                                    </td>
                                </tr>
                            } else {
                                for u in &users {
                                    let full_name = format!("{} {}", u.first_name, u.last_name).trim().to_string();
                                    let display_name = if full_name.is_empty() { "Registered User".to_string() } else { full_name };
                                    let initials = display_name
                                        .split_whitespace()
                                        .filter_map(|w| w.chars().next())
                                        .take(2)
                                        .collect::<String>()
                                        .to_uppercase();
                                    let initials = if initials.is_empty() { "U".to_string() } else { initials };
                                    let roles = if u.roles.is_empty() {
                                        vec!["guest".to_string()]
                                    } else {
                                        u.roles.clone()
                                    };
                                    let is_verified = u.is_verified;

                                    let can_manage_bookings = u.attributes.get("can_manage_bookings").and_then(|v| v.as_bool()).unwrap_or(false);
                                    let can_manage_listings = u.attributes.get("can_manage_listings").and_then(|v| v.as_bool()).unwrap_or(false);
                                    let created_str = u.created_at.format("%Y-%m-%d %H:%M:%S UTC").to_string();
                                    let updated_str = u.updated_at.format("%Y-%m-%d %H:%M:%S UTC").to_string();
                                    let roles_str = roles.join(", ");
                                    let phone_str = u.phone_number.as_deref().unwrap_or("None provided");

                                    <tr
                                        class="user-table-row"
                                        data-search-text=(format!("{} {} {} {}", u.first_name, u.last_name, u.email, u.id).to_lowercase())
                                        data-roles=(roles_str.to_lowercase())
                                    >
                                        <td class="font-bold flex items-center gap-3">
                                            <div class="avatar placeholder">
                                                <div class="w-10 h-10 rounded-full bg-primary/20 text-primary border-2 border-primary/40 font-bold text-sm flex items-center justify-center">
                                                    <span>(initials.clone())</span>
                                                </div>
                                            </div>
                                            <div>
                                                <div class="font-bold text-sm text-base-content">(display_name.clone())</div>
                                                <div class="text-xs text-base-content/50">(u.email.clone())</div>
                                            </div>
                                        </td>
                                        <td>
                                            <div class="flex items-center gap-1.5 flex-wrap">
                                                for role in roles {
                                                    let badge_class = match role.to_lowercase().as_str() {
                                                        "admin" | "superadmin" => "badge badge-primary badge-sm font-bold",
                                                        "host" => "badge badge-warning badge-sm font-semibold",
                                                        _ => "badge badge-ghost badge-sm font-semibold",
                                                    };
                                                    let role_cap = role.to_uppercase();
                                                    <span class=(badge_class)>(role_cap)</span>
                                                }
                                            </div>
                                        </td>
                                        <td>
                                            if is_verified {
                                                <span class="badge badge-success badge-xs font-semibold">"✓ Verified"</span>
                                            } else {
                                                <span class="badge badge-ghost badge-xs font-semibold">"Unverified"</span>
                                            }
                                        </td>
                                        <td class="text-xs font-medium">"Active Account"</td>
                                        <td class="text-xs font-mono text-base-content/60">"Standard"</td>
                                        <td class="text-right space-x-1 whitespace-nowrap">
                                            <button
                                                type="button"
                                                class="btn btn-ghost btn-xs text-primary font-bold hover:bg-primary/10"
                                                data-id=(u.id.to_string())
                                                data-first-name=(u.first_name.clone())
                                                data-last-name=(u.last_name.clone())
                                                data-display-name=(display_name.clone())
                                                data-email=(u.email.clone())
                                                data-phone=(phone_str.to_string())
                                                data-roles=(roles_str.clone())
                                                data-verified=(if is_verified { "true" } else { "false" })
                                                data-active=(if u.is_active { "true" } else { "false" })
                                                data-currency=(u.default_currency.clone())
                                                data-created=(created_str)
                                                data-updated=(updated_str)
                                                data-can-manage-bookings=(if can_manage_bookings { "true" } else { "false" })
                                                data-can-manage-listings=(if can_manage_listings { "true" } else { "false" })
                                                data-initials=(initials.clone())
                                                onclick="openUserDetailsModal(this)"
                                            >
                                                "View Details"
                                            </button>
                                            <button
                                                type="button"
                                                class="btn btn-outline btn-primary btn-xs font-bold rounded-lg hover:bg-primary hover:text-white"
                                                data-id=(u.id.to_string())
                                                data-first-name=(u.first_name.clone())
                                                data-last-name=(u.last_name.clone())
                                                data-email=(u.email.clone())
                                                data-phone=(phone_str.to_string())
                                                data-roles=(roles_str)
                                                data-verified=(if is_verified { "true" } else { "false" })
                                                data-active=(if u.is_active { "true" } else { "false" })
                                                data-currency=(u.default_currency.clone())
                                                data-can-manage-bookings=(if can_manage_bookings { "true" } else { "false" })
                                                data-can-manage-listings=(if can_manage_listings { "true" } else { "false" })
                                                onclick="openUserEditModal(this)"
                                            >
                                                "✏️ Edit"
                                            </button>
                                        </td>
                                    </tr>
                                }
                            }
                        </tbody>
                    </table>
                </div>
            </div>

            // User Details Audit Modal Dialog
            <dialog id="admin-user-details-dialog" class="modal modal-bottom sm:modal-middle">
                <div class="modal-box rounded-3xl p-6 md:p-8 space-y-6 max-w-2xl border border-base-200 dark:border-base-100/20 shadow-2xl">
                    // Header with Avatar and Key Badges
                    <div class="flex justify-between items-start border-b border-base-200 pb-5 gap-4">
                        <div class="flex items-center gap-4">
                            <div class="avatar placeholder">
                                <div class="w-14 h-14 rounded-2xl bg-primary/20 text-primary border-2 border-primary/40 font-black text-xl flex items-center justify-center shadow-inner">
                                    <span id="modal-user-initials">"U"</span>
                                </div>
                            </div>
                            <div>
                                <span class="text-primary font-bold uppercase tracking-widest text-[10px] block">"User Profile and Governance"</span>
                                <h3 class="font-serif font-bold text-2xl text-base-content" id="modal-user-name">"User Profile"</h3>
                                <div class="text-xs text-base-content/60 font-medium" id="modal-user-email">"user@ourplaces.io"</div>
                            </div>
                        </div>
                        <div class="flex flex-col items-end gap-1.5">
                            <span id="modal-user-status-badge" class="badge badge-success font-bold text-xs">"Active"</span>
                            <span id="modal-user-verified-badge" class="badge badge-info font-bold text-xs">"✓ Verified"</span>
                        </div>
                    </div>

                    // Assigned Roles
                    <div class="space-y-2">
                        <span class="text-base-content/50 font-bold uppercase block text-[10px] tracking-wider">"Assigned Security Roles"</span>
                        <div id="modal-user-roles-container" class="flex items-center gap-1.5 flex-wrap">
                            <span class="badge badge-ghost badge-sm">"GUEST"</span>
                        </div>
                    </div>

                    // Identity & Account Grid
                    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4 text-xs">
                        <div class="bg-base-200/50 p-4 rounded-2xl space-y-2 border border-base-200">
                            <span class="text-base-content/50 font-bold uppercase block text-[10px] tracking-wider">"Personal Information"</span>
                            <div class="flex justify-between">
                                <span class="text-base-content/70">"First Name:"</span>
                                <span class="font-bold text-base-content" id="modal-user-first-name">"—"</span>
                            </div>
                            <div class="flex justify-between">
                                <span class="text-base-content/70">"Last Name:"</span>
                                <span class="font-bold text-base-content" id="modal-user-last-name">"—"</span>
                            </div>
                            <div class="flex justify-between">
                                <span class="text-base-content/70">"Phone Number:"</span>
                                <span class="font-semibold text-base-content" id="modal-user-phone">"—"</span>
                            </div>
                            <div class="flex justify-between">
                                <span class="text-base-content/70">"Default Currency:"</span>
                                <span class="font-bold text-primary" id="modal-user-currency">"USD"</span>
                            </div>
                        </div>

                        <div class="bg-base-200/50 p-4 rounded-2xl space-y-2 border border-base-200">
                            <span class="text-base-content/50 font-bold uppercase block text-[10px] tracking-wider">"System Timestamps"</span>
                            <div class="flex justify-between">
                                <span class="text-base-content/70">"Account Created:"</span>
                                <span class="font-mono text-base-content/80 text-[11px]" id="modal-user-created">"—"</span>
                            </div>
                            <div class="flex justify-between">
                                <span class="text-base-content/70">"Last Updated:"</span>
                                <span class="font-mono text-base-content/80 text-[11px]" id="modal-user-updated">"—"</span>
                            </div>
                            <div class="divider my-1"></div>
                            <div>
                                <span class="text-base-content/70 block mb-1">"Internal Unique ID:"</span>
                                <div class="flex items-center gap-1.5">
                                    <code class="text-[11px] font-mono bg-base-300 px-2 py-1 rounded text-base-content/80 truncate block grow select-all" id="modal-user-id">"—"</code>
                                    <button type="button" id="modal-user-copy-btn" class="btn btn-ghost btn-xs text-primary font-bold" onclick="copyUserIdToClipboard()">"Copy"</button>
                                </div>
                            </div>
                        </div>
                    </div>

                    // Granular Capabilities
                    <div class="bg-base-200/60 p-4 rounded-2xl space-y-2 border border-base-200">
                        <span class="text-base-content/50 font-bold uppercase block text-[10px] tracking-wider">
                            "Granular Role Capabilities and Attributes"
                        </span>
                        <div class="flex items-center gap-2 flex-wrap pt-1">
                            <span id="modal-user-can-bookings" class="badge badge-ghost badge-sm">"Can Manage Bookings"</span>
                            <span id="modal-user-can-listings" class="badge badge-ghost badge-sm">"Can Manage Listings"</span>
                        </div>
                    </div>

                    // Modal Action Buttons
                    <div class="modal-action flex flex-wrap justify-between items-center gap-3 pt-2">
                        <div class="flex items-center gap-2">
                            <a id="modal-user-mail-link" href="#" class="btn btn-outline btn-sm rounded-xl font-bold gap-1.5">
                                <span>"✉️"</span>
                                <span>"Contact via Email"</span>
                            </a>
                            <button
                                type="button"
                                class="btn btn-secondary btn-sm rounded-xl font-bold gap-1.5"
                                onclick="openUserEditFromDetails()"
                            >
                                <span>"✏️"</span>
                                <span>"Edit Credentials"</span>
                            </button>
                        </div>
                        <form method="dialog">
                            <button class="btn btn-primary btn-sm rounded-xl font-bold px-5">"Close Audit View"</button>
                        </form>
                    </div>
                </div>
            </dialog>

            // Edit User Profile and Credentials Dialog
            <dialog id="admin-user-edit-dialog" class="modal modal-bottom sm:modal-middle">
                <div class="modal-box rounded-3xl p-6 md:p-8 space-y-6 max-w-2xl border border-base-200 dark:border-base-100/20 shadow-2xl">
                    <div class="flex justify-between items-start border-b border-base-200 pb-4">
                        <div>
                            <span class="text-primary font-bold uppercase tracking-widest text-[10px] block">"Account Credentials and Governance"</span>
                            <h3 class="font-serif font-bold text-2xl text-base-content">"Edit User Account"</h3>
                            <span class="text-xs font-mono text-base-content/60" id="edit-user-uuid-header">"UUID"</span>
                        </div>
                        <form method="dialog">
                            <button class="btn btn-sm btn-circle btn-ghost">"✕"</button>
                        </form>
                    </div>

                    <div id="edit-user-feedback" class="alert text-xs py-2.5 px-4 rounded-xl font-semibold" style="display: none;"></div>

                    <form id="edit-user-form" onsubmit="submitUserEdit(event)" class="space-y-5">
                        <input type="hidden" id="edit-user-id" />

                        <div class="bg-base-200/50 p-4 rounded-2xl space-y-3 border border-base-200">
                            <span class="text-base-content/70 font-bold uppercase block text-[10px] tracking-wider">
                                "1. Security Credentials"
                            </span>
                            <div class="space-y-1">
                                <label class="text-xs font-bold text-base-content/80">"Login Email Address *"</label>
                                <input
                                    type="email"
                                    id="edit-user-email"
                                    required="required"
                                    placeholder="user@ourplaces.io"
                                    class="input input-bordered input-sm w-full rounded-xl font-medium"
                                />
                            </div>
                            <div class="space-y-1">
                                <label class="text-xs font-bold text-base-content/80">"Reset Password (Optional)"</label>
                                <input
                                    type="password"
                                    id="edit-user-password"
                                    placeholder="Leave blank to keep existing password"
                                    autocomplete="new-password"
                                    class="input input-bordered input-sm w-full rounded-xl font-medium"
                                />
                                <p class="text-[11px] text-base-content/60">
                                    "Enter 8+ characters to change password. Leave empty to leave password unchanged."
                                </p>
                            </div>
                        </div>

                        <div class="bg-base-200/50 p-4 rounded-2xl space-y-3 border border-base-200">
                            <span class="text-base-content/70 font-bold uppercase block text-[10px] tracking-wider">
                                "2. Identity and Contact"
                            </span>
                            <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                                <div class="space-y-1">
                                    <label class="text-xs font-bold text-base-content/80">"First Name"</label>
                                    <input type="text" id="edit-user-first-name" class="input input-bordered input-sm w-full rounded-xl font-medium" />
                                </div>
                                <div class="space-y-1">
                                    <label class="text-xs font-bold text-base-content/80">"Last Name"</label>
                                    <input type="text" id="edit-user-last-name" class="input input-bordered input-sm w-full rounded-xl font-medium" />
                                </div>
                            </div>
                            <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                                <div class="space-y-1">
                                    <label class="text-xs font-bold text-base-content/80">"Phone Number"</label>
                                    <input type="text" id="edit-user-phone" placeholder="+1 876 555 0100" class="input input-bordered input-sm w-full rounded-xl font-medium" />
                                </div>
                                <div class="space-y-1">
                                    <label class="text-xs font-bold text-base-content/80">"Default Currency"</label>
                                    <select id="edit-user-currency" class="select select-bordered select-sm w-full rounded-xl font-medium">
                                        <option value="USD">"USD - US Dollar"</option>
                                        <option value="JMD">"JMD - Jamaican Dollar"</option>
                                        <option value="GBP">"GBP - British Pound"</option>
                                        <option value="EUR">"EUR - Euro"</option>
                                        <option value="CAD">"CAD - Canadian Dollar"</option>
                                    </select>
                                </div>
                            </div>
                        </div>

                        <div class="bg-base-200/50 p-4 rounded-2xl space-y-3 border border-base-200">
                            <span class="text-base-content/70 font-bold uppercase block text-[10px] tracking-wider">
                                "3. Roles and Status"
                            </span>
                            <div class="flex items-center gap-4 flex-wrap">
                                <label class="label cursor-pointer gap-2">
                                    <input type="checkbox" id="edit-role-admin" class="checkbox checkbox-primary checkbox-sm rounded-md" />
                                    <span class="label-text text-xs font-bold">"Administrator"</span>
                                </label>
                                <label class="label cursor-pointer gap-2">
                                    <input type="checkbox" id="edit-role-host" class="checkbox checkbox-warning checkbox-sm rounded-md" />
                                    <span class="label-text text-xs font-bold">"Host / Owner"</span>
                                </label>
                                <label class="label cursor-pointer gap-2">
                                    <input type="checkbox" id="edit-role-booker" class="checkbox checkbox-neutral checkbox-sm rounded-md" />
                                    <span class="label-text text-xs font-bold">"Booker / Guest"</span>
                                </label>
                            </div>
                            <div class="divider my-1"></div>
                            <div class="flex items-center gap-6 flex-wrap">
                                <label class="label cursor-pointer gap-2">
                                    <input type="checkbox" id="edit-user-active" class="checkbox checkbox-success checkbox-sm rounded-md" />
                                    <span class="label-text text-xs font-bold">"Account Active"</span>
                                </label>
                                <label class="label cursor-pointer gap-2">
                                    <input type="checkbox" id="edit-user-verified" class="checkbox checkbox-info checkbox-sm rounded-md" />
                                    <span class="label-text text-xs font-bold">"Email Verified"</span>
                                </label>
                            </div>
                            <div class="divider my-1"></div>
                            <div class="flex items-center gap-6 flex-wrap">
                                <label class="label cursor-pointer gap-2">
                                    <input type="checkbox" id="edit-user-can-bookings" class="checkbox checkbox-sm rounded-md" />
                                    <span class="label-text text-xs font-semibold">"Can Manage Bookings"</span>
                                </label>
                                <label class="label cursor-pointer gap-2">
                                    <input type="checkbox" id="edit-user-can-listings" class="checkbox checkbox-sm rounded-md" />
                                    <span class="label-text text-xs font-semibold">"Can Manage Listings"</span>
                                </label>
                            </div>
                        </div>

                        <div class="modal-action flex justify-end gap-2 pt-2">
                            <button type="button" class="btn btn-ghost btn-sm rounded-xl font-semibold" onclick="closeUserEditModal()">
                                "Cancel"
                            </button>
                            <button type="submit" id="edit-user-save-btn" class="btn btn-primary btn-sm rounded-xl font-bold px-6 shadow-md">
                                "Save Credentials and Profile"
                            </button>
                        </div>
                    </form>
                </div>
            </dialog>

            <script>
                r#"
                function applyClientFilter() {
                    var qInput = document.getElementById('user-search-input');
                    var rSelect = document.getElementById('user-role-select');
                    var q = qInput ? qInput.value.toLowerCase().trim() : '';
                    var r = rSelect ? rSelect.value.toLowerCase().trim() : '';

                    var rows = document.querySelectorAll('.user-table-row');
                    var visibleCount = 0;
                    rows.forEach(function(row) {
                        var text = (row.dataset.searchText || '').toLowerCase();
                        var roles = (row.dataset.roles || '').toLowerCase();
                        var matchesQ = true;
                        if (q) {
                            matchesQ = (text.indexOf(q) !== -1);
                        }
                        var matchesR = true;
                        if (r) {
                            matchesR = (roles.indexOf(r) !== -1);
                        }
                        if (matchesQ) {
                            if (matchesR) {
                                row.style.removeProperty('display');
                                visibleCount = visibleCount + 1;
                            } else {
                                row.style.setProperty('display', 'none', 'important');
                            }
                        } else {
                            row.style.setProperty('display', 'none', 'important');
                        }
                    });

                    var countEl = document.getElementById('user-results-count');
                    if (countEl) {
                        countEl.innerText = visibleCount + ' Matching Account' + (visibleCount === 1 ? '' : 's');
                    }
                }

                function openUserDetailsModal(btn) {
                    try {
                        var ds = btn.dataset;
                        var name = ds.displayName || ((ds.firstName || '') + ' ' + (ds.lastName || '')).trim() || ds.email || 'User';
                        var initials = ds.initials || 'U';
                        var email = ds.email || '';
                        var userId = ds.id || '';
                        var phone = ds.phone || 'None provided';
                        var currency = ds.currency || 'USD';
                        var created = ds.created || 'N/A';
                        var updated = ds.updated || 'N/A';
                        var isActive = (ds.active === 'true');
                        var isVerified = (ds.verified === 'true');
                        var canBookings = (ds.canManageBookings === 'true');
                        var canListings = (ds.canManageListings === 'true');
                        var roles = (ds.roles || 'guest').split(',');

                        var nameEl = document.getElementById('modal-user-name');
                        if (nameEl) nameEl.innerText = name;

                        var emailEl = document.getElementById('modal-user-email');
                        if (emailEl) emailEl.innerText = email;

                        var idEl = document.getElementById('modal-user-id');
                        if (idEl) idEl.innerText = userId;

                        var initialsEl = document.getElementById('modal-user-initials');
                        if (initialsEl) initialsEl.innerText = initials;

                        var fnEl = document.getElementById('modal-user-first-name');
                        if (fnEl) fnEl.innerText = ds.firstName || '—';

                        var lnEl = document.getElementById('modal-user-last-name');
                        if (lnEl) lnEl.innerText = ds.lastName || '—';

                        var phoneEl = document.getElementById('modal-user-phone');
                        if (phoneEl) phoneEl.innerText = phone;

                        var currEl = document.getElementById('modal-user-currency');
                        if (currEl) currEl.innerText = currency.toUpperCase();

                        var crEl = document.getElementById('modal-user-created');
                        if (crEl) crEl.innerText = created;

                        var upEl = document.getElementById('modal-user-updated');
                        if (upEl) upEl.innerText = updated;

                        var statusBadge = document.getElementById('modal-user-status-badge');
                        if (statusBadge) {
                            if (isActive) {
                                statusBadge.className = 'badge badge-success font-bold text-xs';
                                statusBadge.innerText = 'Active Account';
                            } else {
                                statusBadge.className = 'badge badge-error font-bold text-xs';
                                statusBadge.innerText = 'Suspended Account';
                            }
                        }

                        var verifiedBadge = document.getElementById('modal-user-verified-badge');
                        if (verifiedBadge) {
                            if (isVerified) {
                                verifiedBadge.className = 'badge badge-info font-bold text-xs';
                                verifiedBadge.innerText = '✓ Verified Email';
                            } else {
                                verifiedBadge.className = 'badge badge-ghost font-bold text-xs';
                                verifiedBadge.innerText = 'Unverified Email';
                            }
                        }

                        var rolesCont = document.getElementById('modal-user-roles-container');
                        if (rolesCont) {
                            rolesCont.innerHTML = '';
                            roles.forEach(function(r) {
                                var clean = r.trim();
                                if (clean) {
                                    var span = document.createElement('span');
                                    var lower = clean.toLowerCase();
                                    if (lower === 'admin') {
                                        span.className = 'badge badge-primary badge-sm font-bold';
                                    } else if (lower === 'superadmin') {
                                        span.className = 'badge badge-primary badge-sm font-bold';
                                    } else if (lower === 'host') {
                                        span.className = 'badge badge-warning badge-sm font-semibold';
                                    } else {
                                        span.className = 'badge badge-ghost badge-sm font-semibold';
                                    }
                                    span.innerText = clean.toUpperCase();
                                    rolesCont.appendChild(span);
                                }
                            });
                        }

                        var permBookings = document.getElementById('modal-user-can-bookings');
                        if (permBookings) {
                            if (canBookings) {
                                permBookings.className = 'badge badge-success badge-sm font-semibold gap-1';
                                permBookings.innerText = '✓ Granted: Can Manage Bookings';
                            } else {
                                permBookings.className = 'badge badge-ghost badge-sm font-normal gap-1 opacity-60';
                                permBookings.innerText = '✗ Denied: Cannot Manage Bookings';
                            }
                        }

                        var permListings = document.getElementById('modal-user-can-listings');
                        if (permListings) {
                            if (canListings) {
                                permListings.className = 'badge badge-success badge-sm font-semibold gap-1';
                                permListings.innerText = '✓ Granted: Can Manage Listings';
                            } else {
                                permListings.className = 'badge badge-ghost badge-sm font-normal gap-1 opacity-60';
                                permListings.innerText = '✗ Denied: Cannot Manage Listings';
                            }
                        }

                        var mailLink = document.getElementById('modal-user-mail-link');
                        if (mailLink) {
                            mailLink.href = 'mailto:' + encodeURIComponent(email);
                        }

                        var dialog = document.getElementById('admin-user-details-dialog');
                        if (dialog) {
                            dialog.showModal();
                        }
                    } catch(err) {
                        console.error('Failed to open user details modal:', err);
                    }
                }

                function copyUserIdToClipboard() {
                    try {
                        var idEl = document.getElementById('modal-user-id');
                        if (idEl) {
                            var text = idEl.innerText || idEl.textContent || '';
                            if (navigator.clipboard) {
                                navigator.clipboard.writeText(text);
                            }
                            var btn = document.getElementById('modal-user-copy-btn');
                            if (btn) {
                                var prev = btn.innerText;
                                btn.innerText = 'Copied!';
                                setTimeout(function() { btn.innerText = prev; }, 1500);
                            }
                        }
                    } catch(err) {}
                }

                function openUserEditModal(btn) {
                    try {
                        var ds = btn.dataset;
                        var id = ds.id || '';
                        var email = ds.email || '';
                        var firstName = ds.firstName || '';
                        var lastName = ds.lastName || '';
                        var phone = ds.phone || '';
                        if (phone === 'None provided') { phone = ''; }
                        var currency = (ds.currency || 'USD').toUpperCase();
                        var rolesStr = (ds.roles || '').toLowerCase();
                        var isActive = (ds.active === 'true');
                        var isVerified = (ds.verified === 'true');
                        var canBookings = (ds.canManageBookings === 'true');
                        var canListings = (ds.canManageListings === 'true');

                        document.getElementById('edit-user-id').value = id;
                        document.getElementById('edit-user-uuid-header').innerText = id;
                        document.getElementById('edit-user-email').value = email;
                        document.getElementById('edit-user-password').value = '';
                        document.getElementById('edit-user-first-name').value = firstName;
                        document.getElementById('edit-user-last-name').value = lastName;
                        document.getElementById('edit-user-phone').value = phone;
                        document.getElementById('edit-user-currency').value = currency;

                        document.getElementById('edit-role-admin').checked = (rolesStr.indexOf('admin') !== -1);
                        document.getElementById('edit-role-host').checked = (rolesStr.indexOf('host') !== -1);
                        document.getElementById('edit-role-booker').checked = (rolesStr.indexOf('booker') !== -1);

                        document.getElementById('edit-user-active').checked = isActive;
                        document.getElementById('edit-user-verified').checked = isVerified;
                        document.getElementById('edit-user-can-bookings').checked = canBookings;
                        document.getElementById('edit-user-can-listings').checked = canListings;

                        var fb = document.getElementById('edit-user-feedback');
                        if (fb) { fb.style.setProperty('display', 'none', 'important'); }

                        var editDialog = document.getElementById('admin-user-edit-dialog');
                        if (editDialog) { editDialog.showModal(); }
                    } catch(err) {
                        console.error('Failed to open edit user modal:', err);
                    }
                }

                function openUserEditFromDetails() {
                    var detailsDialog = document.getElementById('admin-user-details-dialog');
                    if (detailsDialog) { detailsDialog.close(); }
                    var activeId = document.getElementById('modal-user-id').innerText;
                    var btn = document.querySelector('button[data-id="' + activeId + '"][onclick*="openUserEditModal"]');
                    if (btn) {
                        openUserEditModal(btn);
                    }
                }

                function closeUserEditModal() {
                    var dialog = document.getElementById('admin-user-edit-dialog');
                    if (dialog) { dialog.close(); }
                }

                function showEditFeedback(msg, typeClass) {
                    var fb = document.getElementById('edit-user-feedback');
                    if (fb) {
                        fb.className = 'alert text-xs py-2.5 px-4 rounded-xl font-semibold ' + typeClass;
                        fb.innerText = msg;
                        fb.style.removeProperty('display');
                    }
                }

                function submitUserEdit(e) {
                    if (e) {
                        try { e.preventDefault(); } catch(err) {}
                    }
                    var fb = document.getElementById('edit-user-feedback');
                    if (fb) { fb.style.setProperty('display', 'none', 'important'); }

                    var id = document.getElementById('edit-user-id').value;
                    var email = (document.getElementById('edit-user-email').value || '').trim();
                    var password = (document.getElementById('edit-user-password').value || '').trim();
                    var firstName = (document.getElementById('edit-user-first-name').value || '').trim();
                    var lastName = (document.getElementById('edit-user-last-name').value || '').trim();
                    var phone = (document.getElementById('edit-user-phone').value || '').trim();
                    var currency = document.getElementById('edit-user-currency').value;

                    if (!email) {
                        showEditFeedback('Please enter a valid email address.', 'alert-error');
                        return false;
                    }
                    if (email.indexOf('@') === -1) {
                        showEditFeedback('Please enter a valid email address.', 'alert-error');
                        return false;
                    }
                    if (password) {
                        if (!password.slice(7)) {
                            showEditFeedback('Password must be at least 8 characters long.', 'alert-error');
                            return false;
                        }
                    }

                    var roles = [];
                    if (document.getElementById('edit-role-admin').checked) { roles.push('admin'); }
                    if (document.getElementById('edit-role-host').checked) { roles.push('host'); }
                    if (document.getElementById('edit-role-booker').checked) { roles.push('booker'); }
                    if (roles.length === 0) { roles.push('booker'); }

                    var isActive = document.getElementById('edit-user-active').checked;
                    var isVerified = document.getElementById('edit-user-verified').checked;
                    var canBookings = document.getElementById('edit-user-can-bookings').checked;
                    var canListings = document.getElementById('edit-user-can-listings').checked;

                    var saveBtn = document.getElementById('edit-user-save-btn');
                    if (saveBtn) {
                        saveBtn.disabled = true;
                        saveBtn.innerText = 'Saving Changes...';
                    }

                    var payload = {
                        id: id,
                        email: email,
                        password: password || null,
                        first_name: firstName || null,
                        last_name: lastName || null,
                        phone_number: phone || null,
                        is_active: isActive,
                        is_verified: isVerified,
                        roles: roles,
                        can_manage_bookings: canBookings,
                        can_manage_listings: canListings,
                        default_currency: currency
                    };

                    fetch('/api/admin/users/update', {
                        method: 'POST',
                        credentials: 'include',
                        headers: {
                            'Content-Type': 'application/json',
                            'Accept': 'application/json'
                        },
                        body: JSON.stringify(payload)
                    })
                    .then(function(res) {
                        return res.json().then(function(data) {
                            if (saveBtn) {
                                saveBtn.disabled = false;
                                saveBtn.innerText = 'Save Credentials and Profile';
                            }
                            if (data.success) {
                                showEditFeedback('User credentials and profile updated successfully!', 'alert-success');
                                setTimeout(function() {
                                    window.location.reload();
                                }, 800);
                            } else {
                                showEditFeedback(data.message || 'Failed to update user.', 'alert-error');
                            }
                        });
                    })
                    .catch(function(err) {
                        if (saveBtn) {
                            saveBtn.disabled = false;
                            saveBtn.innerText = 'Save Credentials and Profile';
                        }
                        showEditFeedback('Network error while updating user credentials.', 'alert-error');
                    });

                    return false;
                }
                "#
            </script>
        </div>
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserRoleType {
    Booker,
    Host,
    Admin,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GranularPermissions {
    pub can_manage_listings: bool,
    pub can_manage_bookings: bool,
    pub can_configure_rates: bool,
    pub can_manage_users: bool,
}

impl GranularPermissions {
    pub fn has_any_permission(&self) -> bool {
        self.can_manage_listings || self.can_manage_bookings || self.can_configure_rates || self.can_manage_users
    }
}

/// Type-state pattern: Booker roles are strictly unprivileged and cannot hold granular capabilities
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivilegedScope {
    Host(GranularPermissions),
    Admin(GranularPermissions),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleCapabilityProfile {
    StandardGuest,
    Privileged(PrivilegedScope),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PermissionTypeConstraintError {
    #[error("Granular permissions are restricted: Booker/Guest accounts cannot hold administrative or host capabilities")]
    BookerCannotHoldPrivileges,
}

impl RoleCapabilityProfile {
    /// Enforces at the Rust type level that granular permissions can ONLY be assigned if Host or Admin is set.
    pub fn build(
        is_host: bool,
        is_admin: bool,
        is_booker: bool,
        perms: GranularPermissions,
    ) -> std::result::Result<Self, PermissionTypeConstraintError> {
        if is_admin {
            Ok(Self::Privileged(PrivilegedScope::Admin(perms)))
        } else if is_host {
            Ok(Self::Privileged(PrivilegedScope::Host(perms)))
        } else if is_booker {
            if perms.has_any_permission() {
                Err(PermissionTypeConstraintError::BookerCannotHoldPrivileges)
            } else {
                Ok(Self::StandardGuest)
            }
        } else {
            Ok(Self::StandardGuest)
        }
    }

    pub fn is_privileged(&self) -> bool {
        matches!(self, Self::Privileged(_))
    }
}

#[page("/admin/users/new")]
pub async fn admin_new_user_page(cx: &Cx) -> Result {
    match web_app_common_tc::auth::require_admin_role_auth(cx).await {
        Ok(_) => {},
        Err(web_app_common_tc::auth::AdminAuthError::Forbidden(_)) => {
            return view! {
                <div class="p-8 text-center">
                    <div class="alert alert-error max-w-md mx-auto shadow-lg">
                        <span>"Access Denied: Administrative privileges are required to configure user accounts."</span>
                    </div>
                </div>
                <script>
                    r#"window.location.replace('/admin');"#
                </script>
            };
        }
        Err(_) => {
            return view! {
                <script>
                    r#"window.location.replace('/login?redirect=' + encodeURIComponent(window.location.pathname + window.location.search));"#
                </script>
            };
        }
    }

    view! {

        <div class="max-w-4xl mx-auto py-8 px-4 space-y-8">
            <div class="border-b border-base-200 pb-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                <div>
                    <span class="text-primary font-bold tracking-widest uppercase text-xs">"User Access Control"</span>
                    <h1 class="text-3xl font-serif font-bold tracking-tight text-base-content">
                        "Invite and Configure User Account"
                    </h1>
                    <p class="text-xs text-base-content/60">
                        "Assign roles and enforce granular capabilities according to strict role-based access rules."
                    </p>
                </div>
                <a href="/admin/users" class="btn btn-ghost btn-sm font-semibold">
                    "← Back to Users"
                </a>
            </div>

            <form action="/admin/users" method="POST" class="space-y-8">
                // 1. Personal & Contact Details
                <div class="bg-base-100 dark:bg-base-200/90 p-6 md:p-8 rounded-3xl border-2 border-base-200 dark:border-base-100/30 shadow-md space-y-5">
                    <div class="flex items-center justify-between border-b border-base-200 pb-3">
                        <div class="flex items-center gap-2">
                            <span class="text-xl">"👤"</span>
                            <h2 class="text-lg font-serif font-bold text-base-content">
                                "1. Identity and Contact Information"
                            </h2>
                        </div>
                        <span class="badge badge-primary badge-sm font-bold">"Profile"</span>
                    </div>

                    <div class="grid grid-cols-1 md:grid-cols-2 gap-5">
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"First Name"</label>
                            <input
                                type="text"
                                name="first_name"
                                required=(true)
                                placeholder="Jane"
                                class="input input-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-medium rounded-xl w-full px-4 py-2.5 shadow-xs focus:border-primary focus:ring-4 focus:ring-primary/10 transition-all"
                            />
                        </div>
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Last Name"</label>
                            <input
                                type="text"
                                name="last_name"
                                required=(true)
                                placeholder="Doe"
                                class="input input-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-medium rounded-xl w-full px-4 py-2.5 shadow-xs focus:border-primary focus:ring-4 focus:ring-primary/10 transition-all"
                            />
                        </div>
                    </div>
                    <div class="grid grid-cols-1 md:grid-cols-2 gap-5">
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Corporate or Guest Email"</label>
                            <input
                                type="email"
                                name="email"
                                required=(true)
                                placeholder="jane@ourplaces.com"
                                class="input input-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-medium rounded-xl w-full px-4 py-2.5 shadow-xs focus:border-primary focus:ring-4 focus:ring-primary/10 transition-all"
                            />
                        </div>
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Mobile Phone (Optional)"</label>
                            <input
                                type="tel"
                                name="phone"
                                placeholder="+1 (876) 555-0199"
                                class="input input-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-medium rounded-xl w-full px-4 py-2.5 shadow-xs focus:border-primary focus:ring-4 focus:ring-primary/10 transition-all"
                            />
                        </div>
                    </div>
                    <div>
                        <label class="block text-xs font-bold uppercase tracking-wider text-base-content/80 mb-1.5">"Initial Password / Temporary Key"</label>
                        <input
                            type="password"
                            name="password"
                            required=(true)
                            value="temporaryPass123!"
                            class="input input-bordered border-2 border-base-300 dark:border-base-content/20 bg-base-100 dark:bg-base-300/40 text-base-content font-mono text-sm rounded-xl w-full px-4 py-2.5 shadow-xs"
                        />
                    </div>
                </div>

                // 2. Roles Assignment
                <div class="bg-base-100 dark:bg-base-200/90 p-6 md:p-8 rounded-3xl border-2 border-base-200 dark:border-base-100/30 shadow-md space-y-5">
                    <div class="flex items-center justify-between border-b border-base-200 pb-3">
                        <div class="flex items-center gap-2">
                            <span class="text-xl">"🛡️"</span>
                            <h2 class="text-lg font-serif font-bold text-base-content">
                                "2. Roles and Privilege Hierarchy"
                            </h2>
                        </div>
                        <span class="badge badge-warning badge-sm font-bold">"Type Enforced"</span>
                    </div>

                    <div class="grid grid-cols-1 sm:grid-cols-3 gap-4">
                        <label class="flex items-center gap-3 p-4 bg-base-200/60 hover:bg-base-200 border-2 border-base-300 dark:border-base-content/10 rounded-2xl cursor-pointer transition-all">
                            <input
                                type="checkbox"
                                id="role-host"
                                name="is_host"
                                checked=(true)
                                onchange="updatePermissionState()"
                                class="checkbox checkbox-warning checkbox-md"
                            />
                            <div>
                                <div class="font-bold text-sm text-base-content">"Host / Owner 🌴"</div>
                                <div class="text-[11px] text-base-content/60">"Can list and manage properties"</div>
                            </div>
                        </label>
                        <label class="flex items-center gap-3 p-4 bg-base-200/60 hover:bg-base-200 border-2 border-base-300 dark:border-base-content/10 rounded-2xl cursor-pointer transition-all">
                            <input
                                type="checkbox"
                                id="role-admin"
                                name="is_admin"
                                onchange="updatePermissionState()"
                                class="checkbox checkbox-error checkbox-md"
                            />
                            <div>
                                <div class="font-bold text-sm text-base-content">"Administrator 👑"</div>
                                <div class="text-[11px] text-base-content/60">"Full system management"</div>
                            </div>
                        </label>
                        <label class="flex items-center gap-3 p-4 bg-base-200/60 hover:bg-base-200 border-2 border-base-300 dark:border-base-content/10 rounded-2xl cursor-pointer transition-all">
                            <input
                                type="checkbox"
                                id="role-booker"
                                name="is_booker"
                                checked=(true)
                                onchange="updatePermissionState()"
                                class="checkbox checkbox-primary checkbox-md"
                            />
                            <div>
                                <div class="font-bold text-sm text-base-content">"Booker / Guest 🧳"</div>
                                <div class="text-[11px] text-base-content/60">"Can book villas"</div>
                            </div>
                        </label>
                    </div>
                </div>

                // 3. Granular Capabilities (Conditional on Privileged Role)
                <div class="bg-base-100 dark:bg-base-200/90 p-6 md:p-8 rounded-3xl border-2 border-base-200 dark:border-base-100/30 shadow-md space-y-5">
                    <div class="flex items-center justify-between border-b border-base-200 pb-3">
                        <div class="flex items-center gap-2">
                            <span class="text-xl">"⚡"</span>
                            <h2 class="text-lg font-serif font-bold text-base-content">
                                "3. Granular Capabilities and Permissions"
                            </h2>
                        </div>
                        <span class="badge badge-accent badge-sm font-bold">"Host and Admin Only"</span>
                    </div>

                    // Info banner explaining the role requirement
                    <div id="permissions-role-notice" class="hidden bg-warning/15 border-2 border-warning/30 p-4 rounded-2xl flex items-center gap-3 text-xs text-warning-content font-medium">
                        <span class="text-lg">"🔒"</span>
                        <span>"Granular permissions are disabled. You must select <strong>Host / Owner</strong> or <strong>Administrator</strong> to grant specific administrative capabilities."</span>
                    </div>

                    <div id="granular-permissions-container" class="space-y-3 transition-opacity duration-300">
                        <label class="flex items-center gap-3 p-3.5 bg-base-200/50 border border-base-300 rounded-2xl cursor-pointer">
                            <input
                                type="checkbox"
                                name="can_manage_listings"
                                checked=(true)
                                class="granular-perm-checkbox checkbox checkbox-primary checkbox-sm"
                            />
                            <div>
                                <div class="text-xs font-bold text-base-content">"can_manage_listings"</div>
                                <div class="text-[11px] text-base-content/60">"Create, edit, duplicate, and set seasonal pricing on properties"</div>
                            </div>
                        </label>
                        <label class="flex items-center gap-3 p-3.5 bg-base-200/50 border border-base-300 rounded-2xl cursor-pointer">
                            <input
                                type="checkbox"
                                name="can_manage_bookings"
                                checked=(true)
                                class="granular-perm-checkbox checkbox checkbox-primary checkbox-sm"
                            />
                            <div>
                                <div class="text-xs font-bold text-base-content">"can_manage_bookings"</div>
                                <div class="text-[11px] text-base-content/60">"Confirm date holds, approve refunds, and view occupancy schedules"</div>
                            </div>
                        </label>
                        <label class="flex items-center gap-3 p-3.5 bg-base-200/50 border border-base-300 rounded-2xl cursor-pointer">
                            <input
                                type="checkbox"
                                name="can_configure_rates"
                                class="granular-perm-checkbox checkbox checkbox-primary checkbox-sm"
                            />
                            <div>
                                <div class="text-xs font-bold text-base-content">"can_configure_rates"</div>
                                <div class="text-[11px] text-base-content/60">"Update statutory tax overrides and sync foreign exchange currency rates"</div>
                            </div>
                        </label>
                        <label class="flex items-center gap-3 p-3.5 bg-base-200/50 border border-base-300 rounded-2xl cursor-pointer">
                            <input
                                type="checkbox"
                                name="can_manage_users"
                                class="granular-perm-checkbox checkbox checkbox-primary checkbox-sm"
                            />
                            <div>
                                <div class="text-xs font-bold text-base-content">"can_manage_users"</div>
                                <div class="text-[11px] text-base-content/60">"Invite operators, promote shadow accounts, and modify permission roles"</div>
                            </div>
                        </label>
                    </div>
                </div>

                // Submit Bar
                <div class="p-6 bg-base-100 dark:bg-base-200 rounded-3xl border-2 border-base-200 flex flex-col sm:flex-row sm:items-center justify-between gap-4 shadow-xl">
                    <label class="flex items-center gap-3 cursor-pointer">
                        <input type="checkbox" name="is_active" checked=(true) class="toggle toggle-primary toggle-md" />
                        <div>
                            <div class="text-sm font-bold text-base-content">"Account Active"</div>
                            <div class="text-xs text-base-content/50">"User can immediately sign in with verified credentials"</div>
                        </div>
                    </label>
                    <div class="flex items-center gap-3">
                        <a href="/admin/users" class="btn btn-ghost rounded-full px-6 font-semibold">"Cancel"</a>
                        <button type="submit" class="btn btn-primary rounded-full px-8 font-bold tracking-wide shadow-lg">
                            "Create User Account"
                        </button>
                    </div>
                </div>
            </form>

            <script>
                r#"
                function updatePermissionState() {
                    var hostCb = document.getElementById('role-host');
                    var adminCb = document.getElementById('role-admin');
                    var isHost = hostCb ? hostCb.checked : false;
                    var isAdmin = adminCb ? adminCb.checked : false;
                    var canSelect = isHost || isAdmin;

                    var container = document.getElementById('granular-permissions-container');
                    var notice = document.getElementById('permissions-role-notice');
                    var checkboxes = document.querySelectorAll('.granular-perm-checkbox');

                    if (canSelect) {
                        if (container) {
                            container.classList.remove('opacity-30', 'pointer-events-none');
                        }
                        if (notice) {
                            notice.classList.add('hidden');
                        }
                        checkboxes.forEach(function(cb) {
                            cb.disabled = false;
                        });
                    } else {
                        if (container) {
                            container.classList.add('opacity-30', 'pointer-events-none');
                        }
                        if (notice) {
                            notice.classList.remove('hidden');
                        }
                        checkboxes.forEach(function(cb) {
                            cb.disabled = true;
                            cb.checked = false;
                        });
                    }
                }
                // Initialize on load
                if (typeof document !== 'undefined') {
                    document.addEventListener('DOMContentLoaded', updatePermissionState);
                }
                "#
            </script>
        </div>
    }
}
