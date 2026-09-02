use topcoat::{
    Result,
    context::Cx,
    htmx::hx_request,
    router::layout,
    view::view,
};
use web_app_common_tc::auth::auth_init_script;
use web_app_common_tc::theme::{theme_init_script, theme_toggle, theme_toggle_script};

#[layout("/")]
pub async fn admin_layout(cx: &Cx, slot: Result) -> Result {
    // If request was initiated by HTMX, swap the inner page fragment only
    if hx_request(cx) {
        return slot;
    }

    let init_script = theme_init_script();
    let toggle_script = theme_toggle_script();
    let auth_script = auth_init_script();

    let admin_session = web_app_common_tc::auth::get_admin_session(cx).await;
    let is_admin_ssr = match &admin_session {
        Some(user) => user.is_admin(),
        None => true,
    };

    view! {
        <!DOCTYPE html>
        <html lang="en" data-theme="emerald">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <title>"Our Places - Executive Admin Console"</title>
                <link href=(topcoat::tailwind::stylesheet!()) rel="stylesheet" type="text/css" />
                // Immediate Route Guard: If not logged in as Admin / Host / Staff, immediately hide document and redirect to /login?redirect=...
                <script>
                    r#"
                    (function() {
                        try {
                            var path = window.location.pathname;
                            if (path !== '/login') {
                                if (path !== '/login/') {
                                    var isAuthedAdmin = false;
                                    var isStrictAdmin = false;
                                    var userJson = localStorage.getItem('op_auth_user');
                                    if (!userJson) {
                                        userJson = sessionStorage.getItem('op_auth_user');
                                    }
                                    if (userJson) {
                                        try {
                                            var u = JSON.parse(userJson);
                                            if (u) {
                                                if (u.email) {
                                                    var r = (u.role || '').toLowerCase();
                                                    if (r === 'admin') { isAuthedAdmin = true; isStrictAdmin = true; }
                                                    if (r === 'superadmin') { isAuthedAdmin = true; isStrictAdmin = true; }
                                                    if (r === 'host') { isAuthedAdmin = true; }
                                                }
                                            }
                                        } catch(e) {}
                                    }

                                    if (!isAuthedAdmin) {
                                        document.documentElement.style.display = 'none';
                                        var curPath = window.location.pathname + window.location.search + window.location.hash;
                                        var target = (curPath === '/' || curPath === '') ? '/' : curPath;
                                        window.location.replace('/login?redirect=' + encodeURIComponent(target));
                                        return;
                                    }

                                    // Non-admin accounts (e.g. host) must not access user management
                                    if (path.indexOf('/admin/users') === 0 || path === '/users' || path === '/users/') {
                                        if (!isStrictAdmin) {
                                            document.documentElement.style.display = 'none';
                                            window.location.replace('/admin');
                                            return;
                                        }
                                    }
                                }
                            }
                        } catch(e) {}
                    })();
                    "#
                </script>
                <script>(init_script)</script>
                <script>(toggle_script)</script>
                <script>(auth_script)</script>
                <script src="https://cdn.jsdelivr.net/npm/htmx.org@2.0.4/dist/htmx.min.js"></script>
            </head>

            <body class="min-h-screen bg-base-200/50 text-base-content antialiased">
                <div class="drawer lg:drawer-open min-h-screen">
                    <input id="admin-drawer" type="checkbox" class="drawer-toggle" />

                    // Main App Content Container
                    <div class="drawer-content flex flex-col min-h-screen">
                        // Top Admin Navbar
                        <header class="navbar bg-base-100 w-full px-4 md:px-6 border-b border-base-300 sticky top-0 z-40 backdrop-blur-md shadow-xs flex justify-between items-center">
                            <div class="flex items-center gap-3">
                                <label for="admin-drawer" aria-label="toggle sidebar" class="btn btn-square btn-ghost btn-sm lg:hidden">
                                    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" class="inline-block h-5 w-5 stroke-current">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16" />
                                    </svg>
                                </label>
                                <div class="flex items-center gap-2">
                                    <span class="text-xs font-bold uppercase tracking-wider text-primary">"Executive Portal"</span>
                                    <span class="text-base-content/50">"|"</span>
                                    <span class="text-xs font-mono text-base-content/70 hidden sm:inline">"GCP Cloud Run (0.25 vCPU)"</span>
                                </div>
                            </div>

                            <div class="flex items-center gap-3">
                                <span class="badge badge-success badge-xs gap-1 hidden md:flex font-mono text-[10px]">
                                    <span class="inline-block w-1.5 h-1.5 rounded-full bg-success-content animate-pulse"></span>
                                    "System Healthy"
                                </span>

                                theme_toggle()

                                // Quick Actions Dropdown
                                <div class="dropdown dropdown-end">
                                    <div tabindex="0" role="button" class="btn btn-primary btn-sm rounded-full px-4 font-bold tracking-wide shadow-sm">
                                        "+ Quick Create ▾"
                                    </div>
                                    <ul tabindex="0" class="dropdown-content menu bg-base-100 rounded-box z-50 w-52 p-2 shadow-2xl border border-base-200">
                                        <li><a href="/admin/listings/new" class="font-semibold">"🌴 New Villa Listing"</a></li>
                                        if is_admin_ssr {
                                            <li id="admin-quick-add-user"><a href="/admin/users/new" class="font-semibold">"👤 Invite / Add User"</a></li>
                                        }
                                        <li><a href="/admin/exchange-rates" class="font-semibold">"💱 Sync Exchange Rates"</a></li>
                                    </ul>
                                </div>

                                // Unauthenticated Admin Top Button (Default when logged out)
                                <div id="admin-auth-guest" class="flex items-center gap-2">
                                    <a href="/login" class="btn btn-outline btn-primary btn-sm rounded-full font-bold">
                                        "Sign In"
                                    </a>
                                </div>

                                // Authenticated User Avatar Dropdown (Shown ONLY when logged in)
                                <div id="admin-auth-user" class="dropdown dropdown-end" style="display: none;">
                                    <div tabindex="0" role="button" class="btn btn-ghost btn-circle avatar border-2 border-primary/40 hover:border-primary">
                                        <div class="w-8 h-8 rounded-full bg-primary/20 text-primary flex items-center justify-center font-bold text-xs" id="admin-top-avatar-initials">
                                            "AD"
                                        </div>
                                    </div>
                                    <ul tabindex="0" class="menu menu-sm dropdown-content bg-base-100 rounded-box z-50 mt-3 w-56 p-2 shadow-2xl border border-base-200">
                                        <li class="px-4 py-2 border-b border-base-200 mb-2">
                                            <div class="flex flex-col gap-0.5">
                                                <span class="font-bold text-sm" id="admin-top-user-name">"Administrator"</span>
                                                <span class="badge badge-primary badge-xs font-semibold" id="admin-top-user-role">"Super Administrator"</span>
                                                <span class="text-xs opacity-60 truncate" id="admin-top-user-email">"admin@ourplaces.io"</span>
                                            </div>
                                        </li>
                                        <li><a href="/admin">"Dashboard"</a></li>
                                        if is_admin_ssr {
                                            <li id="admin-top-manage-users"><a href="/admin/users">"Manage Users"</a></li>
                                        }
                                        <li class="border-t border-base-200 mt-2 pt-2">
                                            <a href="/logout" class="text-error font-medium" onclick="logoutUser(event)">"Logout"</a>
                                        </li>

                                    </ul>
                                </div>
                            </div>
                        </header>

                        // Main Content Body
                        <main class="flex-grow w-full p-4 md:p-8">
                            (slot?)
                        </main>

                        // Admin Footer
                        <footer class="footer footer-center p-4 bg-base-100 text-base-content/60 text-xs border-t border-base-300">
                            <div>
                                <p>"Our Places Admin Console • Version 0.1.0 • GCP Cloud Run Scale-to-Zero Architecture"</p>
                            </div>
                        </footer>
                    </div>

                    // Left Side Navigation Sidebar (Rendered 100% in Pure Rust SSR)
                    <div class="drawer-side z-50 border-r border-base-300">
                        <label for="admin-drawer" aria-label="close sidebar" class="drawer-overlay"></label>
                        <aside class="bg-base-100 min-h-full w-64 p-4 flex flex-col justify-between">
                            <div class="space-y-6">
                                // Brand Header
                                <div class="flex items-center gap-3 px-2 py-3 border-b border-base-200">
                                    <span class="text-2xl">"🌴"</span>
                                    <div>
                                        <div class="font-serif font-black text-lg tracking-wider text-base-content">"Our Places"</div>
                                        <div class="text-[10px] font-bold tracking-widest text-primary uppercase">"Admin Console"</div>
                                    </div>
                                </div>

                                // Navigation Groups
                                <ul class="menu menu-md w-full p-0 space-y-1">
                                    // Section: Overview
                                    <li class="menu-title text-[11px] uppercase tracking-wider font-bold text-base-content/70">
                                        "Overview"
                                    </li>
                                    <li>
                                        <a href="/admin" class="flex items-center gap-3 font-semibold rounded-xl active:bg-primary">
                                            <span>"📊"</span>
                                            <span>"Dashboard & KPIs"</span>
                                        </a>
                                    </li>

                                    // Section: Inventory & Properties
                                    <li class="menu-title text-[11px] uppercase tracking-wider font-bold text-base-content/50 pt-3">
                                        "Inventory & Properties"
                                    </li>
                                    <li>
                                        <a href="/admin/listings" class="flex items-center gap-3 font-semibold rounded-xl active:bg-primary">
                                            <span>"🌴"</span>
                                            <span>"Villa Portfolio"</span>
                                        </a>
                                    </li>
                                    <li>
                                        <a href="/admin/listings/new" class="flex items-center gap-3 text-xs font-medium rounded-xl text-base-content/80">
                                            <span>"➕"</span>
                                            <span>"Add New Villa"</span>
                                        </a>
                                    </li>

                                    // Section: Bookings & Holds
                                    <li class="menu-title text-[11px] uppercase tracking-wider font-bold text-base-content/50 pt-3">
                                        "Bookings & Holds"
                                    </li>
                                    <li>
                                        <a href="/admin/bookings" class="flex items-center gap-3 font-semibold rounded-xl active:bg-primary">
                                            <span>"📅"</span>
                                            <span>"Master Schedule"</span>
                                        </a>
                                    </li>

                                    // Section: Users & Governance
                                    if is_admin_ssr {
                                        <li id="admin-sidebar-users-title" class="menu-title text-[11px] uppercase tracking-wider font-bold text-base-content/50 pt-3">
                                            "Users & Governance"
                                        </li>
                                        <li id="admin-sidebar-users-link">
                                            <a href="/admin/users" class="flex items-center gap-3 font-semibold rounded-xl active:bg-primary">
                                                <span>"👥"</span>
                                                <span>"User Management"</span>
                                            </a>
                                        </li>
                                        <li id="admin-sidebar-users-new-link">
                                            <a href="/admin/users/new" class="flex items-center gap-3 text-xs font-medium rounded-xl text-base-content/80">
                                                <span>"✉️"</span>
                                                <span>"Invite User"</span>
                                            </a>
                                        </li>
                                    }

                                    // Section: Configuration & Finance
                                    <li class="menu-title text-[11px] uppercase tracking-wider font-bold text-base-content/50 pt-3">
                                        "Configuration & Finance"
                                    </li>
                                    <li>
                                        <a href="/admin/exchange-rates" class="flex items-center gap-3 font-semibold rounded-xl active:bg-primary">
                                            <span>"💱"</span>
                                            <span>"Exchange Rates & FX"</span>
                                        </a>
                                    </li>
                                </ul>
                            </div>

                            // Unauthenticated Bottom Sidebar (Default when logged out)
                            <div id="admin-sidebar-guest" class="pt-4 border-t border-base-200 space-y-2">
                                <a href="/login" class="btn btn-primary btn-sm w-full rounded-xl font-bold">
                                    "Sign In to Console"
                                </a>
                            </div>

                            // Authenticated Bottom Sidebar (Shown ONLY when logged in)
                            <div id="admin-sidebar-user" class="pt-4 border-t border-base-200 space-y-3" style="display: none;">
                                <div class="flex items-center gap-3 px-2">
                                    <div class="avatar">
                                        <div class="w-9 h-9 rounded-full bg-primary/20 text-primary flex items-center justify-center font-bold text-xs" id="admin-sidebar-avatar-initials">
                                            "AD"
                                        </div>
                                    </div>
                                    <div class="overflow-hidden">
                                        <div class="font-bold text-xs truncate" id="admin-sidebar-user-name">"Administrator"</div>
                                        <div class="text-[10px] text-base-content/70 truncate" id="admin-sidebar-user-role">"Super Administrator"</div>
                                    </div>
                                </div>
                                <a href="/logout" class="btn btn-outline btn-error btn-xs w-full rounded-xl font-bold" onclick="logoutUser(event)">
                                    "Sign Out"
                                </a>

                            </div>
                        </aside>
                    </div>
                </div>
            </body>
        </html>
    }
}
