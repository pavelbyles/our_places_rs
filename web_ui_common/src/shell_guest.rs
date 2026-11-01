//! The guest portal page shell: document head, header (wordmark, nav,
//! currency, theme toggle, server-rendered account menu), main and footer.

use topcoat::{
    Result,
    asset::Asset,
    context::Cx,
    view::{Child, View, component, view},
};

use crate::assets::{HTMX_CONFIG, HTMX_JS, UI_JS};
use crate::components::domain::currency_selector::currency_selector;
use crate::components::navbar::theme_toggle;
use crate::components::toast::toast_region;
use crate::fonts::{INTER, NEWSREADER};
use crate::theme::{ThemeChoice, theme_for_request};

/// The signed-in guest shown in the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavUser {
    /// Display name.
    pub name: String,
    /// Email address.
    pub email: String,
}

/// Per-request data the guest shell needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestShell {
    /// Page `<title>` (the site name is appended).
    pub title: String,
    /// The signed-in guest, or `None`. Signed-in navigation is rendered only
    /// when this is `Some`; nothing is toggled on the client.
    pub user: Option<NavUser>,
    /// Theme rendered on `<html>`.
    pub theme: ThemeChoice,
    /// Display currency for the selector.
    pub currency: Option<String>,
}

/// Header, main landmark, footer and toast region. Separate from the
/// document head so it can be rendered without the asset bundle.
#[component]
pub async fn guest_chrome(
    /// Shell data.
    shell: GuestShell,
    /// Page content.
    #[default]
    child: Child<'_>,
) -> Result<impl View> {
    let GuestShell {
        user,
        theme,
        currency,
        ..
    } = shell;
    let pill = "inline-flex h-11 items-center whitespace-nowrap rounded-full px-3 text-sm font-medium sm:px-5";
    Ok(view! {
        <a href="#main" class="sr-only focus:not-sr-only focus:absolute focus:top-2 focus:left-2 focus:rounded-full focus:bg-card focus:px-4 focus:py-2">"Skip to content"</a>
        <header class="border-b border-border">
            <div class="mx-auto flex max-w-[1540px] items-center justify-between gap-2 px-3 py-4 sm:gap-4 sm:px-6">
                <a href="/" class="shrink-0 whitespace-nowrap font-serif text-[20px] text-foreground no-underline sm:text-[26px]">"Our Places"</a>
                <nav aria-label="Main" class="hidden items-center gap-6 text-[15px] font-medium md:flex">
                    <a href="/listings" class="text-foreground">"Listings"</a>
                    <a href="/about" class="text-foreground">"About"</a>
                </nav>
                <div class="flex items-center gap-1 sm:gap-2 [&>*]:shrink-0">
                    currency_selector(current_currency: currency)
                    // Below 380px the toggle moves into the menu so the bar fits.
                    <span class="hidden min-[380px]:contents">
                        theme_toggle(theme: theme)
                    </span>
                    if let Some(user) = user.clone() {
                        <details id="nav-user-dropdown" data-dismiss-outside="" class="relative">
                            <summary aria-label="Account menu" class="flex h-11 cursor-pointer list-none items-center gap-2 rounded-full border border-border px-4 text-sm">
                                <span id="nav-user-name" class="max-w-[7rem] truncate sm:max-w-none">(user.name)</span>
                            </summary>
                            <div class="absolute right-0 z-50 mt-2 w-56 rounded-xl border border-border bg-popover p-2 text-popover-foreground shadow-sm">
                                <p id="nav-user-email" class="px-3 py-2 text-xs text-muted-foreground">(user.email)</p>
                                <a href="/bookings" class="block rounded-lg px-3 py-2 text-sm">"My bookings"</a>
                                <a href="/profile" class="block rounded-lg px-3 py-2 text-sm">"Profile"</a>
                                <form method="post" action="/logout">
                                    <button type="submit" class="w-full rounded-lg px-3 py-2 text-left text-sm">"Sign out"</button>
                                </form>
                            </div>
                        </details>
                    } else {
                        <div id="nav-guest-actions" class="flex items-center gap-2">
                            <a href="/login" class=(format!("{pill} bg-primary text-primary-foreground"))>"Sign in"</a>
                        </div>
                    }
                    <details id="mobile-nav" data-dismiss-outside="" class="relative md:hidden">
                        <summary aria-label="Menu" class="flex size-11 cursor-pointer list-none items-center justify-center rounded-full border border-border">"☰"</summary>
                        <nav aria-label="Mobile" class="absolute right-0 z-50 mt-2 flex w-56 flex-col rounded-xl border border-border bg-popover p-2 shadow-sm">
                            <a href="/listings" class="rounded-lg px-3 py-2 text-sm">"Listings"</a>
                            <a href="/about" class="rounded-lg px-3 py-2 text-sm">"About"</a>
                            <div class="flex items-center justify-between px-3 py-1 text-sm min-[380px]:hidden">
                                <span>"Theme"</span>
                                theme_toggle(theme: theme)
                            </div>
                            if user.is_some() {
                                <a href="/bookings" class="rounded-lg px-3 py-2 text-sm">"My bookings"</a>
                            } else {
                                <a href="/register" class="rounded-lg px-3 py-2 text-sm">"Create account"</a>
                            }
                        </nav>
                    </details>
                </div>
            </div>
        </header>
        <main id="main" class="mx-auto w-full max-w-[1540px] px-6 py-12">(child)</main>
        <footer class="mt-32 border-t border-border">
            <div class="mx-auto flex max-w-[1540px] flex-col gap-4 px-6 py-10 text-sm text-muted-foreground md:flex-row md:justify-between">
                <p class="font-serif text-xl text-foreground">"Our Places"</p>
                <nav aria-label="Footer" class="flex flex-wrap gap-6">
                    <a href="/register">"List your property"</a>
                    <a href="/about">"About"</a>
                </nav>
            </div>
        </footer>
        toast_region()
    })
}

/// The full guest document: `<html class="dim|dark">`, head with the bin's
/// stylesheet, fonts, htmx and `ui.js` (no inline scripts), then the chrome.
#[component]
pub async fn guest_document(
    /// Shell data.
    shell: GuestShell,
    /// The bin's compiled Tailwind stylesheet (`topcoat::tailwind::stylesheet!()`).
    stylesheet: Asset,
    /// Page content.
    #[default]
    child: Child<'_>,
) -> Result<impl View> {
    let title = format!("{} · Our Places", shell.title);
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
                topcoat::font::link(font: NEWSREADER, preload: false)
                <link rel="stylesheet" href=(stylesheet) />
                <script src=(HTMX_JS) defer=(true)></script>
                <script src=(UI_JS) defer=(true)></script>
            </head>
            <body>
                guest_chrome(shell: shell, (child))
            </body>
        </html>
    })
}

/// Renders a guest page: just `slot` for an HTMX partial request, otherwise
/// the full document. `user` must come from the server session.
pub async fn guest_shell<'a>(
    __cx: &'a Cx,
    stylesheet: Asset,
    title: impl Into<String>,
    user: Option<NavUser>,
    currency: Option<String>,
    slot: Child<'a>,
) -> Result<impl View + 'a> {
    let partial = topcoat::htmx::hx_request(__cx) && !topcoat::htmx::hx_boosted(__cx);
    let shell = GuestShell {
        title: title.into(),
        user,
        theme: theme_for_request(__cx),
        currency,
    };
    Ok(view! {
        if partial {
            (slot)
        } else {
            guest_document(shell: shell, stylesheet: stylesheet, (slot))
        }
    })
}
