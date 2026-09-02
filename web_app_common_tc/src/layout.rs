use topcoat::{
    Result,
    context::Cx,
    htmx::hx_request,
    view::view,
};
use crate::auth::auth_init_script;
use crate::components::currency_selector::currency_selector;
use crate::theme::{theme_init_script, theme_toggle, theme_toggle_script};

pub async fn guest_base_layout(__cx: &Cx, slot: Result) -> Result {
    // If request was initiated by HTMX, swap the inner page fragment only
    if hx_request(__cx) {
        return slot;
    }

    let init_script = theme_init_script();
    let toggle_script = theme_toggle_script();
    let auth_script = auth_init_script();

    view! {
        <!DOCTYPE html>
        <html lang="en" data-theme="emerald">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <title>"Our Places - Luxury Jamaica Stays"</title>
                <link href=(topcoat::tailwind::stylesheet!()) rel="stylesheet" type="text/css" />
                <style>
                    r#"
                    .hero-luxury {
                        position: relative;
                        width: 100%;
                        min-height: 520px;
                        overflow: hidden;
                        display: flex;
                        flex-direction: column;
                        justify-content: space-between;
                        align-items: center;
                        text-align: center;
                        padding: 3.5rem 1.5rem 2.5rem 1.5rem;
                        background: #0f172a;
                    }
                    .hero-luxury-overlay {
                        position: absolute;
                        inset: 0;
                        background: linear-gradient(180deg, rgba(0, 0, 0, 0.65) 0%, rgba(0, 0, 0, 0.35) 45%, rgba(0, 0, 0, 0.8) 100%);
                        pointer-events: none;
                        z-index: 10;
                    }
                    .search-capsule {
                        position: relative;
                        z-index: 20;
                        width: 100%;
                        max-width: 64rem;
                        background: rgba(255, 255, 255, 0.25);
                        backdrop-filter: blur(24px) saturate(180%);
                        -webkit-backdrop-filter: blur(24px) saturate(180%);
                        border: 1px solid rgba(255, 255, 255, 0.4);
                        border-radius: 9999px;
                        box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.4);
                        padding: 0.75rem 1.25rem;
                        color: #ffffff;
                        display: flex;
                        flex-direction: row;
                        align-items: center;
                        justify-content: space-between;
                        gap: 0.5rem;
                    }
                    @media (max-width: 768px) {
                        .search-capsule {
                            flex-direction: column;
                            border-radius: 1.5rem;
                            padding: 1rem;
                        }
                    }
                    [data-theme="night"] .search-capsule,
                    .dark .search-capsule {
                        background: rgba(15, 23, 42, 0.75);
                        border-color: rgba(255, 255, 255, 0.15);
                        color: #f8fafc;
                    }
                    .search-field {
                        flex: 1;
                        display: flex;
                        flex-direction: column;
                        text-align: left;
                        padding: 0 0.75rem;
                        border-right: 1px solid rgba(255, 255, 255, 0.25);
                    }
                    @media (max-width: 768px) {
                        .search-field {
                            width: 100%;
                            border-right: none;
                            border-bottom: 1px solid rgba(255, 255, 255, 0.15);
                            padding: 0.5rem 0;
                        }
                    }
                    .search-field-last {
                        flex: 1;
                        display: flex;
                        flex-direction: column;
                        text-align: left;
                        padding: 0 0.75rem;
                    }
                    @media (max-width: 768px) {
                        .search-field-last {
                            width: 100%;
                            padding: 0.5rem 0;
                        }
                    }
                    .search-label {
                        font-size: 0.625rem;
                        font-weight: 700;
                        text-transform: uppercase;
                        letter-spacing: 0.1em;
                        color: rgba(255, 255, 255, 0.85);
                        margin-bottom: 0.25rem;
                    }
                    .search-input, .search-date-input {
                        width: 100%;
                        background: transparent;
                        border: none;
                        font-size: 0.875rem;
                        font-weight: 600;
                        color: #ffffff;
                        outline: none;
                        padding: 0;
                    }
                    .search-input option {
                        background: #1e293b;
                        color: #ffffff;
                    }
                    .btn-discover {
                        background: #f59e0b;
                        color: #0f172a;
                        font-weight: 700;
                        font-size: 0.75rem;
                        text-transform: uppercase;
                        letter-spacing: 0.05em;
                        padding: 0.875rem 1.75rem;
                        border-radius: 9999px;
                        border: none;
                        display: flex;
                        align-items: center;
                        gap: 0.5rem;
                        cursor: pointer;
                        box-shadow: 0 10px 25px -5px rgba(245, 158, 11, 0.5);
                        transition: all 0.2s ease;
                    }
                    .btn-discover:hover {
                        background: #fbbf24;
                        transform: translateY(-1px);
                    }
                    "#
                </style>
                <script>(init_script)</script>
                <script>(toggle_script)</script>
                <script src="https://cdn.jsdelivr.net/npm/htmx.org@2.0.4/dist/htmx.min.js"></script>
                <script>(auth_script)</script>
            </head>

            <body class="min-h-screen bg-base-100 text-base-content antialiased">
                <div class="drawer">
                    <input id="my-drawer-2" type="checkbox" class="drawer-toggle" />
                    <div class="drawer-content flex flex-col min-h-screen">
                        // Header with Editorial Branding & Controls
                        <header class="navbar bg-base-100/95 w-full px-4 md:px-8 border-b border-base-200 sticky top-0 z-50 backdrop-blur-md shadow-xs flex justify-between items-center">
                            <div class="flex items-center gap-2">
                                <div class="lg:hidden">
                                    <label for="my-drawer-2" aria-label="open sidebar" class="btn btn-square btn-ghost btn-sm">
                                        <svg
                                            xmlns="http://www.w3.org/2000/svg"
                                            fill="none"
                                            viewBox="0 0 24 24"
                                            class="inline-block h-5 w-5 stroke-current"
                                        >
                                            <path
                                                stroke-linecap="round"
                                                stroke-linejoin="round"
                                                stroke-width="2"
                                                d="M4 6h16M4 12h16M4 18h16"
                                            ></path>
                                        </svg>
                                    </label>
                                </div>
                                <a href="/" class="flex items-center gap-2 text-lg md:text-xl font-serif font-bold tracking-wider text-base-content hover:text-primary transition-colors">
                                    <span class="text-amber-500 text-2xl">"🌴"</span>
                                    <span class="tracking-widest uppercase text-base md:text-lg">"Our Places"</span>
                                </a>
                            </div>

                            <div class="hidden lg:flex items-center">
                                <ul class="flex items-center gap-6 font-medium text-xs tracking-wider uppercase text-base-content/80">
                                    <li><a href="/listings" class="hover:text-primary transition-colors">"Destinations"</a></li>
                                    <li><a href="/listings" class="hover:text-primary transition-colors">"Villas"</a></li>
                                    <li><a href="/about" class="hover:text-primary transition-colors">"Experiences"</a></li>
                                    <li><a href="/listings" class="hover:text-primary transition-colors">"Special Offers"</a></li>
                                    <li><a href="/about" class="hover:text-primary transition-colors">"Contact"</a></li>
                                </ul>
                            </div>

                            <div class="flex items-center gap-3">
                                currency_selector(current_currency: None)
                                theme_toggle()

                                // Unauthenticated Guest State (Default)
                                <div id="nav-guest-actions" class="flex items-center gap-2">
                                    <a href="/login" class="btn btn-ghost btn-sm text-xs font-bold uppercase tracking-wider">"Log In"</a>
                                    <a href="/register" class="btn btn-primary btn-sm rounded-xl text-xs font-bold uppercase tracking-wider shadow-sm">"Sign Up"</a>
                                </div>

                                // Authenticated User Avatar Dropdown (Shown dynamically when user is logged in)
                                <div id="nav-user-dropdown" class="dropdown dropdown-end" style="display: none;">
                                    <div tabindex="0" role="button" class="btn btn-ghost btn-circle avatar border-2 border-primary/40 hover:border-primary">
                                        <div class="w-9 h-9 rounded-full bg-primary/20 text-primary flex items-center justify-center font-black text-sm" id="nav-user-avatar-initials">
                                            "JD"
                                        </div>
                                    </div>
                                    <ul
                                        tabindex="0"
                                        class="menu menu-sm dropdown-content bg-base-100 rounded-box z-50 mt-3 w-56 p-2 shadow-2xl border border-base-200"
                                    >
                                        <li class="px-4 py-2 border-b border-base-200 mb-2">
                                            <div class="flex flex-col gap-0.5">
                                                <span class="font-bold text-sm" id="nav-user-name">"Jane Doe"</span>
                                                <span class="badge badge-success badge-xs font-semibold">"Verified Guest"</span>
                                                <span class="text-xs opacity-60 truncate" id="nav-user-email">"jane.doe@example.com"</span>
                                            </div>
                                        </li>
                                        <li><a href="/bookings">"📅 My Bookings"</a></li>
                                        <li><a href="/profile">"👤 Profile & Preferences"</a></li>
                                        <li><a href="/listings">"🌴 Explore Villas"</a></li>
                                        <li class="border-t border-base-200 mt-2 pt-2">
                                            <a href="/logout" onclick="logoutUser(event)" class="text-error font-medium">"Logout"</a>
                                        </li>
                                    </ul>
                                </div>
                            </div>
                        </header>

                        // Main Slot Content
                        <main class="flex-grow w-full">
                            (slot?)
                        </main>

                        // Footer matching Leptos design
                        <footer class="footer sm:footer-horizontal bg-neutral text-neutral-content p-10 mt-16 border-t border-base-300">
                            <aside class="space-y-2">
                                <div class="flex items-center gap-2">
                                    <svg
                                        width="36"
                                        height="36"
                                        viewBox="0 0 24 24"
                                        xmlns="http://www.w3.org/2000/svg"
                                        fill-rule="evenodd"
                                        clip-rule="evenodd"
                                        class="fill-current text-primary"
                                    >
                                        <path d="M22.672 15.226l-2.432.811.841 2.515c.33 1.019-.209 2.127-1.23 2.456-1.15.325-2.148-.321-2.463-1.226l-.84-2.518-5.013 1.677.84 2.517c.391 1.203-.434 2.542-1.831 2.542-.88 0-1.601-.564-1.86-1.314l-.842-2.516-2.431.809c-1.135.328-2.145-.317-2.463-1.229-.329-1.018.211-2.127 1.231-2.456l2.432-.809-1.621-4.823-2.432.808c-1.355.384-2.558-.59-2.558-1.839 0-.817.509-1.582 1.327-1.846l2.433-.809-.842-2.515c-.33-1.02.211-2.129 1.232-2.458 1.02-.329 2.13.209 2.461 1.229l.842 2.515 5.011-1.677-.839-2.517c-.403-1.238.484-2.553 1.843-2.553.819 0 1.585.509 1.85 1.326l.841 2.517 2.431-.81c1.02-.33 2.131.211 2.461 1.229.332 1.018-.21 2.126-1.23 2.456l-2.433.809 1.622 4.823 2.433-.809c1.242-.401 2.557.484 2.557 1.838 0 .819-.51 1.583-1.328 1.847m-8.992-6.428l-5.01 1.675 1.619 4.828 5.011-1.674-1.62-4.829z"></path>
                                    </svg>
                                    <span class="font-extrabold text-xl tracking-tight">"Our Places"</span>
                                </div>
                                <p class="text-sm opacity-80">
                                    "Luxury villa & property rental platform in Jamaica."
                                    <br />
                                    "Providing premium Caribbean stays since 2024."
                                </p>
                            </aside>
                            <nav>
                                <h3 class="footer-title">"Navigation"</h3>
                                <a href="/" class="link link-hover">"Home"</a>
                                <a href="/listings" class="link link-hover">"Listings"</a>
                                <a href="/about" class="link link-hover">"About"</a>
                                <a href="/bookings" class="link link-hover">"Bookings"</a>
                            </nav>
                            <nav>
                                <h3 class="footer-title">"Account"</h3>
                                <a href="/login" class="link link-hover">"Login"</a>
                                <a href="/register" class="link link-hover">"Sign Up"</a>
                                <a href="/profile" class="link link-hover">"Profile"</a>
                            </nav>
                        </footer>
                    </div>

                    // Drawer Side for Mobile Screen Menu
                    <div class="drawer-side z-50">
                        <label for="my-drawer-2" aria-label="close sidebar" class="drawer-overlay"></label>
                        <ul class="menu bg-base-200 text-base-content min-h-full w-80 p-6 space-y-3">
                            <li class="mb-4">
                                <a href="/" class="text-2xl font-black text-primary px-0">"Our Places"</a>
                            </li>
                            <li><a href="/" class="font-semibold text-base">"Home"</a></li>
                            <li><a href="/listings" class="font-semibold text-base">"Listings"</a></li>
                            <li><a href="/about" class="font-semibold text-base">"About Us"</a></li>
                            <li><a href="/bookings" class="font-semibold text-base">"My Bookings"</a></li>
                            <li><a href="/profile" class="font-semibold text-base">"Profile & Settings"</a></li>
                            <div class="divider my-2"></div>
                            <div id="drawer-guest-actions" class="space-y-2">
                                <li><a href="/login" class="btn btn-outline btn-sm w-full">"Log In"</a></li>
                                <li><a href="/register" class="btn btn-primary btn-sm text-primary-content w-full">"Sign Up"</a></li>
                            </div>
                            <div id="drawer-user-actions" class="space-y-2" style="display: none;">
                                <li><a href="/logout" onclick="logoutUser(event)" class="btn btn-outline btn-error btn-sm w-full font-bold">"Log Out"</a></li>
                            </div>
                        </ul>
                    </div>
                </div>
            </body>
        </html>
    }
}
