//! The admin portal page shell: sidebar navigation, top bar with the
//! signed-in user, and the main area. Signed-out pages (login, host
//! sign-up) get a centred layout without navigation.

use topcoat::{
    Result,
    asset::Asset,
    context::Cx,
    view::{Child, View, component, view},
};
use web_client::session::AdminRole;

use crate::assets::{HTMX_CONFIG, HTMX_JS, UI_JS};
use crate::components::navbar::theme_toggle;
use crate::components::toast::toast_region;
use crate::fonts::INTER;
use crate::theme::{ThemeChoice, theme_for_request};

/// The signed-in admin-portal user shown in the shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminNavUser {
    /// Display name.
    pub name: String,
    /// Email address.
    pub email: String,
    /// Role; user management links are shown to admins only.
    pub role: AdminRole,
}

/// Per-request data the admin shell needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminShell {
    /// Page `<title>` (the console name is appended).
    pub title: String,
    /// The signed-in user, or `None` on signed-out pages.
    pub user: Option<AdminNavUser>,
    /// Theme rendered on `<html>`.
    pub theme: ThemeChoice,
}

const NAV_LINK: &str = "flex h-10 items-center rounded-lg px-3 text-sm text-sidebar-foreground hover:bg-sidebar-accent";

/// Sidebar, top bar, main landmark and toast region (or a centred main for
/// signed-out pages). Rendered without the asset bundle, so it is testable.
#[component]
pub async fn admin_chrome(
    /// Shell data.
    shell: AdminShell,
    /// Page content.
    #[default]
    child: Child<'_>,
) -> Result<impl View> {
    let AdminShell { user, theme, .. } = shell;
    Ok(view! {
        if let Some(user) = user {
            let is_admin = user.role == AdminRole::Admin;
            let role_label = if is_admin { "Administrator" } else { "Host" };
            <div class="flex min-h-screen">
                <aside class="hidden w-60 shrink-0 border-r border-sidebar-border bg-sidebar p-4 md:block">
                    <a href="/admin" class="mb-6 block font-serif text-xl text-sidebar-foreground no-underline">"Our Places"</a>
                    <nav aria-label="Console" class="flex flex-col gap-1">
                        <a href="/admin" class=(NAV_LINK)>"Dashboard"</a>
                        <a href="/admin/listings" class=(NAV_LINK)>"Listings"</a>
                        <a href="/admin/bookings" class=(NAV_LINK)>"Bookings"</a>
                        <a href="/admin/payouts" class=(NAV_LINK)>"Payouts"</a>
                        <a href="/admin/exchange-rates" class=(NAV_LINK)>"Exchange rates"</a>
                        if is_admin {
                            <p id="admin-sidebar-users-title" class="mt-4 px-3 text-xs font-semibold tracking-wide text-muted-foreground uppercase">"Users"</p>
                            <a id="admin-sidebar-users-link" href="/admin/users" class=(NAV_LINK)>"All users"</a>
                            <a id="admin-sidebar-users-new-link" href="/admin/users/new" class=(NAV_LINK)>"Invite user"</a>
                        }
                    </nav>
                </aside>
                <div class="flex min-w-0 flex-1 flex-col">
                    <header class="flex items-center justify-between gap-4 border-b border-border px-6 py-3">
                        <details id="admin-mobile-nav" data-dismiss-outside="" class="relative md:hidden">
                            <summary aria-label="Menu" class="flex size-11 cursor-pointer list-none items-center justify-center rounded-full border border-border">"☰"</summary>
                            <nav aria-label="Console (mobile)" class="absolute left-0 z-50 mt-2 flex w-56 flex-col rounded-xl border border-border bg-popover p-2 shadow-sm">
                                <a href="/admin" class=(NAV_LINK)>"Dashboard"</a>
                                <a href="/admin/listings" class=(NAV_LINK)>"Listings"</a>
                                <a href="/admin/bookings" class=(NAV_LINK)>"Bookings"</a>
                                <a href="/admin/payouts" class=(NAV_LINK)>"Payouts"</a>
                                if is_admin {
                                    <a href="/admin/users" class=(NAV_LINK)>"Users"</a>
                                }
                            </nav>
                        </details>
                        <div class="ml-auto flex items-center gap-3">
                            theme_toggle(theme: theme)
                            <div class="text-right text-sm leading-tight">
                                <p id="admin-top-user-name" class="font-medium">(user.name)</p>
                                <p class="text-xs text-muted-foreground"><span id="admin-top-user-role">(role_label)</span> " · " (user.email)</p>
                            </div>
                            <form method="post" action="/logout">
                                <button type="submit" class="h-10 rounded-full border border-border px-4 text-sm">"Sign out"</button>
                            </form>
                        </div>
                    </header>
                    <main id="main" class="flex-1 px-6 py-8">(child)</main>
                </div>
            </div>
        } else {
            <div class="flex min-h-screen flex-col">
                <header class="flex items-center justify-between px-6 py-4">
                    <span class="font-serif text-xl">"Our Places"</span>
                    theme_toggle(theme: theme)
                </header>
                <main id="main" class="flex flex-1 items-start justify-center px-6 py-12">(child)</main>
            </div>
        }
        toast_region()
    })
}

/// The full admin document: `<html class="dim|dark">`, head with the bin's
/// stylesheet, Inter, htmx and `ui.js` (no inline scripts), then the chrome.
#[component]
pub async fn admin_document(
    /// Shell data.
    shell: AdminShell,
    /// The bin's compiled Tailwind stylesheet (`topcoat::tailwind::stylesheet!()`).
    stylesheet: Asset,
    /// Page content.
    #[default]
    child: Child<'_>,
) -> Result<impl View> {
    let title = format!("{} · Our Places Admin", shell.title);
    let html_class = shell.theme.html_class();
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en" class=(html_class)>
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <title>(title)</title>
                <meta name="htmx-config" content=(HTMX_CONFIG) />
                topcoat::font::link(font: INTER)
                <link rel="stylesheet" href=(stylesheet) />
                <script src=(HTMX_JS) defer=(true)></script>
                <script src=(UI_JS) defer=(true)></script>
            </head>
            <body>
                admin_chrome(shell: shell, (child))
            </body>
        </html>
    })
}

/// Renders an admin page: just `slot` for an HTMX partial request,
/// otherwise the full document. `user` must come from the server session.
pub async fn admin_shell<'a>(
    __cx: &'a Cx,
    stylesheet: Asset,
    title: impl Into<String>,
    user: Option<AdminNavUser>,
    slot: Child<'a>,
) -> Result<impl View + 'a> {
    let partial = topcoat::htmx::hx_request(__cx) && !topcoat::htmx::hx_boosted(__cx);
    let shell = AdminShell {
        title: title.into(),
        user,
        theme: theme_for_request(__cx),
    };
    Ok(view! {
        if partial {
            (slot)
        } else {
            admin_document(shell: shell, stylesheet: stylesheet, (slot))
        }
    })
}
