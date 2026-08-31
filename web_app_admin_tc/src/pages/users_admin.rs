use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::view,
};
use web_app_common_tc::get_api_client;

#[page("/admin/users")]
pub async fn admin_users_page(cx: &Cx) -> Result {
    render_users_directory(cx).await
}

#[page("/users")]
pub async fn users_alias_page(cx: &Cx) -> Result {
    render_users_directory(cx).await
}

async fn render_users_directory(cx: &Cx) -> Result {
    let __cx = cx;
    let api = get_api_client(cx);
    let users = api.get_all_users(Some(1), Some(50), None).await.unwrap_or_default();
    let total_count = users.len();

    view! {
        <div class="space-y-8 py-6 max-w-7xl mx-auto px-4 md:px-6">
            // Header
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 border-b border-base-200 pb-4">
                <div class="space-y-1">
                    <span class="text-primary font-bold tracking-widest uppercase text-xs">"Access Control & Accounts"</span>
                    <h1 class="text-3xl font-serif font-bold tracking-tight text-base-content">
                        "User Directory & Roles"
                    </h1>
                    <p class="text-xs text-base-content/60">
                        "Manage host profiles, administrator permissions, and shadow user account promotions."
                    </p>
                </div>
                <a href="/admin/users/new" class="btn btn-primary btn-sm rounded-full px-5 font-bold tracking-wide shadow-md">
                    "+ Invite / Create User"
                </a>
            </div>

            // Filter Bar
            <div class="bg-base-100 dark:bg-base-200/80 p-4 rounded-2xl border border-base-200 dark:border-base-100/20 shadow-sm flex flex-col md:flex-row items-center justify-between gap-4">
                <div class="flex-1 w-full flex items-center gap-3">
                    <input
                        type="text"
                        placeholder="Search by name, email, or user UUID..."
                        class="input input-bordered input-sm w-full max-w-md rounded-xl font-medium"
                    />
                    <select class="select select-bordered select-sm rounded-xl font-medium">
                        <option value="">"All Roles"</option>
                        <option value="admin">"Administrator"</option>
                        <option value="host">"Host / Property Owner"</option>
                        <option value="booker">"Guest / Booker"</option>
                    </select>
                </div>
                <div class="text-xs text-base-content/60 font-semibold">
                    (format!("{} Registered Accounts", total_count))
                </div>
            </div>

            // Users Table
            <div class="bg-base-100 dark:bg-base-200 rounded-2xl border border-base-200 dark:border-base-100/20 shadow-md overflow-hidden">
                <div class="overflow-x-auto">
                    <table class="table table-zebra w-full">
                        <thead>
                            <tr class="text-xs text-base-content/60 uppercase tracking-wider">
                                <th>"User"</th>
                                <th>"Roles & Permissions"</th>
                                <th>"Verification"</th>
                                <th>"Properties & Bookings"</th>
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

                                    <tr>
                                        <td class="font-bold flex items-center gap-3">
                                            <div class="avatar placeholder">
                                                <div class="w-10 h-10 rounded-full bg-primary/20 text-primary border-2 border-primary/40 font-bold text-sm flex items-center justify-center">
                                                    <span>(initials)</span>
                                                </div>
                                            </div>
                                            <div>
                                                <div class="font-bold text-sm text-base-content">(display_name)</div>
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
                                        <td class="text-right space-x-1">
                                            <button class="btn btn-ghost btn-xs text-primary font-bold">"View Details"</button>
                                        </td>
                                    </tr>
                                }
                            }
                        </tbody>
                    </table>
                </div>
            </div>
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
pub async fn admin_new_user_page(_cx: &Cx) -> Result {
    view! {
        <div class="max-w-4xl mx-auto py-8 px-4 space-y-8">
            <div class="border-b border-base-200 pb-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                <div>
                    <span class="text-primary font-bold tracking-widest uppercase text-xs">"User Access Control"</span>
                    <h1 class="text-3xl font-serif font-bold tracking-tight text-base-content">
                        "Invite & Configure User Account"
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
                                "1. Identity & Contact Information"
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
                                "2. Roles & Privilege Hierarchy"
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
                                <div class="text-[11px] text-base-content/60">"Can list & manage properties"</div>
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
                                "3. Granular Capabilities & Permissions"
                            </h2>
                        </div>
                        <span class="badge badge-accent badge-sm font-bold">"Host & Admin Only"</span>
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
