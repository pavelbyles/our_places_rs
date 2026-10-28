# Spec 96: Redesigned Topcoat UI Portals (daisyUI → Topcoat UI) with Full Feature Parity

- **Source issue**: [#96](https://github.com/pavelbyles/our_places_rs/issues/96)
- **Intent**: [`docs/intent/intent-96-feat-redesigned-topcoat-ui-portals.md`](../intent/intent-96-feat-redesigned-topcoat-ui-portals.md) (Approved)
- **Design reference**: [`docs/design/guest-theme.md`](../design/guest-theme.md) ("Shoreline Mist")
- **Branch**: `feat/96-feat-redesigned-topcoat-ui-portals-daisyui-topcoat-ui`
- **Date**: 2026-10-28

## Overview

Build two new server-rendered portals on **Topcoat UI** (typed, source-owned `#[component]`s over `styles.css` tokens). They run side by side with the existing daisyUI portals and must reach full functional parity with them before a follow-up PR cuts the domain over and deletes the old crates.

| Crate | Kind | Port | Role |
|---|---|---|---|
| `web_client` | NEW lib, no UI | — | API client, `AppError`, session/auth extraction, service JWT minting |
| `web_ui_common` | NEW lib | — | Topcoat UI components, guest + admin themes, `ui.js`, shells, security headers |
| `web_ui_guest` | NEW bin | 3010 | Redesigned guest portal ("Shoreline Mist") |
| `web_ui_admin` | NEW bin | 3012 | Redesigned admin portal (neutral + teal `#245A6D`) |
| `web_app_common_tc` | existing | — | Becomes a re-export facade over `web_client`; keeps daisyUI layouts and inline-JS helpers |
| `web_app_tc` / `web_app_admin_tc` | existing | 3000 / 3002 | **Zero source changes** |

**Personas**: guests (luxury-villa booking experience, server-side auth), hosts/admins (dense data tool), developers/agents (one typed component layer, one token schema).

### Decisions resolved during design interrogation (2026-10-28)

| # | Question | Decision |
|---|---|---|
| D1 | Parity for **stubbed** old flows (checkout with hardcoded dates/guest ID calling `localhost:8081` from the browser; static `/profile`; `/reviews/submit` posting to a static page; `/bookings` listing every booking) | **Wire to the real backend APIs.** Each is logged as a *fixed-bug deviation* in the parity inventory (§Parity Inventory). E2E for those rows asserts real behavior and runs against the new app only. |
| D2 | Anonymous / shadow-user checkout | **Login required.** No shadow-user mechanism exists in the backend. `/checkout/*` redirects to `/login?redirect=…`; the booking's `guest_id` always comes from the server session. Closes the intent's "shadow-user" open question as N/A. |
| D3 | Date-range picker | **Custom calendar component**, server-rendered, month navigation over HTMX, range selection and keyboard support in `ui.js`. |
| D4 | Blocked nights in the calendar | **Add one read-only `booking_api` endpoint**: `GET /api/v1/bookings/listing/{id}/unavailable-ranges` (the only backend change in this spec). |
| D5 | CSP fallback if the Topcoat runtime fails strict CSP | **`ui.js` only.** Strict `script-src 'self'` wins; runtime signals are dropped. **Triggered**: the Phase 0 spike failed (see Spike results), so neither bin calls `.runtime()` or renders `topcoat::runtime::script()`. |
| D6 | Theme cookie vs HTML caching | No CDN/edge HTML caching exists. HTML responses send `Cache-Control: private, no-cache` and `Vary: Cookie` defensively. |
| D7 | Visitor time zone for the theme default | `ui.js` stores the **UTC offset in minutes** (`op_tz_offset`), not an IANA name. No `chrono-tz` dependency (keeps the binary and cold start lean). First visit uses `America/Jamaica` = fixed UTC−05:00 (no DST). |
| D8 | Who may see a booking's cancellation quote | **Only the booking's guest or a system administrator** (`admin` role). Hosts are not included. Enforced in the frontend for both portals (FR-4.13, FR-5.6). |
| D9 | `/bookings` scope | **Only the logged-in guest's bookings**, never anyone else's (FR-4.7). |
| D10 | Payout status changes | **Admins only.** Hosts can view their payout ledger but cannot change a status. Admins may override the status the system set (FR-5.7). |

## Requirements

### Functional Requirements

**FR-1 `web_client` extraction (Phase 1)**
1. Move `client.rs`, `api_client.rs`, `error.rs` and the Rust-only parts of `auth.rs` (`AuthUser`, `AdminAuthError`, `token_hash_to_hex`, `get_admin_session`, `get_guest_session`, `require_admin_auth`, `require_admin_role_auth`, `get_authenticated_admin`, `get_authenticated_guest`) from `web_app_common_tc` into `web_client`.
2. `web_app_common_tc` re-exports them at **identical paths**: `web_app_common_tc::{client, api_client, error, app_error, AppError, get_api_client, TopcoatApiClient}` and `web_app_common_tc::auth::*`. `auth_init_script`, `theme_*` scripts, layouts and components stay in `web_app_common_tc`.
3. `get_authenticated_guest` (always `None`) and `get_authenticated_admin` (hardcoded) keep their current behavior so the old crates are unchanged. They are documented as legacy and **must not** be used by new crates (enforced by a `rg` gate, see Acceptance Criteria). No `#[deprecated]` attribute: it would raise warnings in the old crates and fail `clippy -D warnings`.
4. New in `web_client` (used only by new crates):
   - `GuestSession { user_id: Uuid, name, email }` and `AdminSession { user_id: Uuid, name, email, role: AdminRole }`, with **non-optional** `user_id` (removes today's `user.id.unwrap_or_default()` → nil-UUID path).
   - `require_guest(cx) -> Result<GuestSession, GuestAuthError>`, `require_admin(cx)`, `require_admin_role(cx)`.
   - `ServiceJwt::from_env() -> Result<ServiceJwt, ConfigError>` and `ServiceJwt::mint(user_id) -> Result<String, AppError>`. Reads `JWT_SECRET` via a `static JWT_SECRET_ENV: &str` constant and **fails startup** if it is missing. The old crates' `"secret"` fallback is not carried over.
   - `safe_redirect(raw: &str) -> &str`: accepts only same-origin relative paths (starts with `/`, not `//` or `/\`, no scheme, ≤ 512 bytes); anything else becomes `/`.

**FR-2 Parity inventory and test baseline (Phase 2)**
1. The inventory in §Parity Inventory is the parity checklist. Every row has: old route, new route, auth, covering spec file, status (`same` / `fixed-bug deviation` / `retired`).
2. Missing Playwright specs are written against **:3000/:3002 first** and must pass there, except `fixed-bug deviation` rows (tagged `@new-only`).
3. Replace daisyUI-class selectors (`.card`, `.btn`, `.badge`, `.alert`, `.modal`, `.chat`) with roles, labels, IDs or `data-testid`.
4. Replace `localStorage.getItem('op_auth_user')` assertions with UI assertions (nav shows the user menu). Tag the legacy assertion `@old-only`.
5. Add projects `guest-portal-new-{chromium,firefox}` (`baseURL :3010`) and `admin-portal-new-{chromium,firefox}` (`:3012`) reusing the same `testDir`s, with `grepInvert: /@old-only/`. Old projects get `grepInvert: /@new-only/`. Add `webServer` entries for `web_ui_guest` / `web_ui_admin`.
6. Add a CSP fixture that subscribes to `securitypolicyviolation` (via `page.addInitScript` from a file, not inline in the app) plus console `Refused to` messages, and fails the test on any violation. Enabled for the `-new-` projects.

**FR-3 `web_ui_common` (Phase 3)**
1. `topcoat ui init`; commit `components.toml` with pinned registry versions. Install: accordion, alert, alert_dialog, avatar, badge, breadcrumb, button, card, checkbox, dialog, dropdown_menu, field, input, label, pagination, progress, radio_group, select, separator, sheet, sidebar, skeleton, spinner, switch, table, tabs, textarea, toggle, tooltip.
2. Two theme files with the same token names: `style/guest.css` (exact light/`.dim`/`.dark` values from `guest-theme.md` §1, Newsreader + Inter via `topcoat-font`) and `style/admin.css` (Topcoat `neutral`, overriding `--primary`/`--ring`/`--sidebar-primary` per `guest-theme.md` §6). Tailwind `dark:` variant: `@custom-variant dark (&:where(.dark, .dark *));`.
3. Custom components (Topcoat conventions: `#[component]`, `attrs` passthrough, token-only colors): `toast`, `steps`, `star_rating` (display) + `star_rating_input` (radio group, keyboard-operable), `chat_thread` + `chat_bubble`, `stat_tile`, `framed_image`, `hero`, `navbar`, `date_range_calendar` (§Frontend Views).
4. Domain components ported: `currency_selector`, `listing_card`, `price_breakdown`, `responsive_image` (keeps `<picture>`/`srcset`), `tax_document_view`. Tri-currency output must be **byte-identical in numbers and currency codes** to the old components. Unit tests compare formatted strings.
5. `ui.js` (one hashed asset per app, ≤ 15 KB minified, no dependencies), all behavior wired through `data-*` attributes:
   - `data-dialog-open="<id>"` / `data-dialog-close` → `showModal()` / `close()`; `dialog` light-dismiss on backdrop click.
   - `data-dismiss-outside` → closes open `<details>` dropdowns and sheets on outside click / Escape.
   - `data-theme-cycle` → light → dim → dark; writes `op_theme`; updates `<html>` class and the button `aria-label`.
   - On load: writes `op_tz_offset` (from `new Date().getTimezoneOffset()`, negated) if it is missing or changed.
   - `data-print` → `window.print()`.
   - Calendar range selection and arrow-key navigation (§Frontend Views).
   - HTMX glue: `htmx.config.includeIndicatorStyles = false` is set via `<meta name="htmx-config">`; toasts from `HX-Trigger: {"toast": …}` events. No `hx-on`.
6. Theme engine (server): `ThemeChoice::{Light, Dim, Dark}` resolved by `resolve_theme(cookie_theme, cookie_tz_offset, now_utc)`. Precedence: valid `op_theme` → else local hour from `op_tz_offset` (validated −840…840) → else UTC−05:00. Dark if local hour ≥ 18, else light (matches today's rule; `< 6` is **not** dark, per the intent wording "dark at or after 18:00, otherwise light"). Dim is only ever an explicit choice. The shell renders `<html class="dim|dark|">`.
7. Shells: `guest_shell` (navbar with server-rendered auth state, footer, mobile `sheet`) and `admin_shell` (`sidebar` + top bar; user-management links rendered only when `AdminRole::Admin`).
8. `security_headers` middleware (§Security Mitigations), shared by both apps.

**FR-4 `web_ui_guest` (Phase 4)**
1. A redesigned page for every guest row in the inventory, on the **same paths** as the old app (tests navigate by path).
2. **Server sessions** (cookie `op_guest_session`, namespace `guest`, 7-day rolling TTL as today):
   - `POST /login` (HTMX-enhanced form; works without JS) → `user_api` login → `session::stop` (fixation defence) → `session::start` → `create_session(namespace="guest", role="guest")` → `HX-Redirect`/303 to `safe_redirect(redirect)` or `/bookings`.
   - `POST /register` → `user_api` `POST /api/v1/users` **server-side** (replaces the browser call to `localhost:8083`) → 303 `/verify?email=…`.
   - `POST /verify` and `POST /verify/resend` → `user_api` verify / resend-verification server-side → on success 303 `/login?verified=1`.
   - `POST /logout` (nav form) and `GET /logout` (kept for parity) → `delete_session` + `session::stop` → 303 `/login`.
   - The navbar user menu is rendered **only** when `get_guest_session` returns a session. No `localStorage`/`sessionStorage` writes anywhere.
3. Protected pages (`/bookings`, `/bookings/{id}/*`, `/bookings/{code}/messages`, `/profile`, `/checkout/*`) call `require_guest`; unauthenticated → 303 `/login?redirect=<current path+query>` (HTMX requests get `HX-Redirect`).
4. Ownership: lists come from `GET /api/v1/bookings/user/{session.user_id}`. Every booking-scoped route loads the booking and requires `booking.guest_id == session.user_id`; otherwise **404** (not 403, to avoid ID enumeration). Backend calls carry a `ServiceJwt` for `session.user_id` so `booking_api` enforces ownership again (defence in depth).
5. **Checkout (D1, D2)**: `GET /checkout/{slug}?check_in&check_out&adults&children&infants&pets&currency` renders the server quote. `POST /checkout/{slug}` validates the form server-side, builds `NewBookingRequest` with `guest_id = session.user_id` and the submitted dates, calls `booking_api POST /api/v1/bookings` with the service JWT, then 303 → `/bookings?new_booking=true`. A conflict (dates taken, max guests) re-renders the form with an inline `alert` and the calendar refreshed. Legacy `/checkout-{jmd,eur,gbp,cad}/{id}` and the `{slug}-{ccy}` suffix form → 308 to `/checkout/{slug}?currency=XXX`.
6. **Session expiry mid-checkout** (intent §5): `POST /checkout` with no session → `HX-Redirect` to `/login?redirect=/checkout/{slug}?<all form fields>`. After re-login the form is pre-filled and availability is **re-checked**; no hold is created, extended or re-created by the frontend. If a pending hold already exists for this guest, it keeps its original expiry; if the dates are gone, the guest sees the conflict state and picks new dates.
7. **Bookings** (D1, D9): `/bookings` shows **only the logged-in guest's bookings**. The page calls `GET /api/v1/bookings/user/{session.user_id}` with a service JWT for that user. The user ID always comes from the session, never from a query string, form field or path. The old `get_all_bookings` call (every booking on the platform) is not used anywhere in `web_ui_guest`. Cancel, change-dates quote/request, approve/decline/cancel change request become HTMX form posts returning fragments (§API Endpoints).
8. **Profile** (D1): `GET /profile` shows real `user_api` data (`GET /api/v1/users/user/{id}`); `POST /profile` updates name/phone via `PUT /api/v1/users/user/{id}`; email change and password reset use the existing `profile/email` and `profile/password/{request,confirm}` endpoints.
9. **Reviews** (D1): `GET /reviews/submit?token=…` loads `GET /api/v1/reviews/token/{token}`; `POST /reviews/submit` posts to `POST /api/v1/reviews/token/{token}` → 303 `/reviews/success`. Missing/invalid/used token → friendly 404 page. The Bookings page links to review via the existing `review_eligibility` and `GET /api/v1/reviews/booking/{booking_id}/token`.
10. **Messaging**: `POST /bookings/{code}/messages` appends a `chat_bubble` (`hx-swap="beforeend"` into `#messages-list`, as today). `{code}` is resolved within the session user's bookings only (not "all bookings").
11. **Tax documents**: `/bookings/{id}/documents/{number}` with ownership check; the print button uses `data-print`.
12. `/htmx/welcome` (demo fragment) is **retired** (inventory status `retired`).
13. **Cancellation quote** (D8): `GET /bookings/{id}/cancellation-quote` calls `require_guest`, loads the booking, and requires `booking.guest_id == session.user_id`. Otherwise it returns **404**. Only then does it call `booking_api` with a service JWT for the guest. The guest portal has only `guest`-namespace sessions, so administrators use the admin portal route instead (FR-5.6). `web_client::TopcoatApiClient::get_cancellation_quote(id, jwt)` takes the JWT as a required argument. The old `common::app_client::get_cancellation_quote(id)` sends no user JWT, so `booking_api` answers 404 and the old route appears to be broken today. The new route's e2e test asserts that a real quote renders.

**FR-5 `web_ui_admin` (Phase 5)**
1. A redesigned page for every admin row in the inventory. Canonical paths are `/admin/...`. Legacy unprefixed aliases (`/listings`, `/listings/{id}`, `/listings/{id}/edit`, `/listings/{id}/pricing[/add]`, `/bookings`, `/payouts`, `/users`) → **308** to the `/admin/...` path (preserves method for `/pricing/add` POSTs).
2. Auth reuses the `admin` namespace (`op_admin_session`). `require_admin` on every page and fragment; `require_admin_role` on `/admin/users*` and `/api/admin/users/*`. Hosts are additionally scoped by `booking_api` / `listing_api` JWT checks, and the frontend never shows listings/bookings the API did not return for that user.
3. Every `fetch(...)` site in the old admin (bookings detail dialog, door codes, status, messages, change requests, payouts status, user create/update/delete/restore, listing create, host register/verify/resend) becomes an HTMX form post returning a fragment, or a full-page form post. JSON `/api/*` routes are **not** carried over (the inventory maps each to its replacement).
4. Image uploads keep the GCP V4 signed-URL flow (browser → GCS directly); `ui.js` performs the `PUT` to the presigned URL. `connect-src` allows only `https://storage.googleapis.com` besides `'self'`.
5. The admin login page keeps both tabs (sign in, host sign-up + verification).
6. **Cancellation quote, admin side** (D8): `GET /admin/bookings/{id}/cancellation-quote` (fragment inside the booking detail dialog) requires `require_admin_role`. Hosts get 403, and the dialog does not render the quote control for them. The call to `booking_api` carries the admin's service JWT.
7. **Payout status** (D10): `POST /admin/payouts/{id}/status` requires `require_admin_role`. The old route used `require_admin_auth`, which also lets hosts in; `booking_api` then rejected hosts with "Admin role required". Hosts see the payout ledger read-only, and the status controls are **not rendered** for them. Admins can override any status the system set. The override form requires a reason (`failure_reason` / `gateway_reference` as the status needs), which is logged with the admin's user ID.

**FR-6 Deployment (Phase 6)**
1. `web_ui_guest/Dockerfile.web_ui_guest`, `web_ui_admin/Dockerfile.web_ui_admin` (multi-stage, distroless/`debian-slim`, matching existing Topcoat Dockerfiles), compose services `web_ui_guest` (:3010) and `web_ui_admin` (:3012).
2. Terraform Cloud Run services `web-ui-guest` / `web-ui-admin` (256 MiB, min instances 0, `JWT_SECRET` from Secret Manager), **not** mapped to the production domain.

**FR-7 `booking_api` addition (D4, only backend change)**
- `GET /api/v1/bookings/listing/{id}/unavailable-ranges?from=YYYY-MM-DD&to=YYYY-MM-DD` → `200 {"ranges":[{"date_from":"…","date_to":"…"}]}`. Public (no auth), returns **only** date ranges (no booking IDs, guest data or status). `to - from ≤ 366` days, else 400. Merges overlapping ranges before returning.

### Exclusions / Non-Goals
- Domain cutover and deleting `web_app_tc`, `web_app_admin_tc`, `web_app_common_tc`.
- Any backend change other than FR-7 (and a `user_api` tweak only if guest sessions prove to need one; none is expected).
- New product features. D1 wiring uses only existing endpoints.
- Transactional email templates; the old guest portal's auth; Firefox `::picker(select)` styling.
- The `POST /api/v1/bookings/bookings/{id}/transfer` placeholder-transfer endpoint is not used (D2).

## Parity Inventory

Legend — **Auth**: `P` public, `G` guest session, `G*` guest session + ownership, `A` admin/host session, `A!` admin role only. **Status**: `same` (behavior unchanged, test runs on old + new), `fixed` (fixed-bug deviation, `@new-only`), `retired`.

### Guest (32 routes, `web_app_tc` → `web_ui_guest`)

| # | Old route | New route | Auth | Spec file | Status |
|---|---|---|---|---|---|
| G1 | `GET /` | `GET /` | P | `home.spec.ts` | same |
| G2 | `GET /htmx/welcome` | — | P | — | retired (demo fragment) |
| G3 | `GET /about` | `GET /about` | P | `static-pages.spec.ts` (new) | same |
| G4 | `GET /listings` | `GET /listings` | P | `listings.spec.ts` | same |
| G5 | `GET /listings/{slug}` | `GET /listings/{slug}` | P | `listing-detail.spec.ts` | same |
| G6 | `GET /listings/{slug}/quote` | `GET /listings/{slug}/quote` (fragment) | P | `listing-detail.spec.ts` | same |
| G7 | — | `GET /listings/{slug}/calendar?month=` (fragment) | P | `calendar.spec.ts` (new) | new (D3) |
| G8 | `GET /checkout/{slug}` | `GET /checkout/{slug}` | G | `checkout-flow.spec.ts` | fixed (D2: login required) |
| G9–G12 | `GET /checkout-{jmd,eur,gbp,cad}/{id}` | 308 → `/checkout/{id}?currency=` | G | `checkout-flow.spec.ts` | same (redirect) |
| G13 | browser `POST localhost:8081/api/v1/bookings` | `POST /checkout/{slug}` | G | `checkout-flow.spec.ts` | fixed (D1: real dates, session guest_id) |
| G14 | `GET /bookings` | `GET /bookings` | G* | `bookings.spec.ts` (new) | fixed (IDOR: own bookings only) |
| G15 | `PATCH /api/bookings/{id}` (cancel/update) | `POST /bookings/{id}/cancel` (fragment) | G* | `bookings.spec.ts` | same |
| G16 | `GET /api/bookings/{id}/cancellation-quote` | `GET /bookings/{id}/cancellation-quote` (fragment) | G* | `bookings.spec.ts` | fixed (D8: ownership check + forwards guest JWT; old route sent no JWT) |
| G17 | `GET /bookings/{id}/change-dates` | `GET /bookings/{id}/change-dates` | G* | `tax-documents.spec.ts`, `reservation-changes.spec.ts` (new) | same |
| G18 | `POST /api/bookings/{id}/change-quote` | `POST /bookings/{id}/change-quote` (fragment) | G* | `reservation-changes.spec.ts` | same |
| G19 | `POST /api/bookings/{id}/change-requests` | `POST /bookings/{id}/change-requests` | G* | `reservation-changes.spec.ts` | same |
| G20 | `GET /api/bookings/{id}/change-requests` | rendered server-side into G17 | G* | `reservation-changes.spec.ts` | same |
| G21–G23 | `POST /api/bookings/{id}/change-requests/{rid}/{approve,decline,cancel}` | `POST /bookings/{id}/change-requests/{rid}/{approve,decline,cancel}` (fragment) | G* | `reservation-changes.spec.ts` | same |
| G24 | `GET /bookings/{code}/messages` | `GET /bookings/{code}/messages` | G* | `messaging.spec.ts` (new) | fixed (code resolved within own bookings) |
| G25 | `POST /bookings/{code}/messages` (hx-post) | same (hx-post, `beforeend` `#messages-list`) | G* | `messaging.spec.ts` | same |
| G26 | `GET /bookings/{id}/documents/{number}` | same | G* | `tax-documents.spec.ts` | same |
| G27 | `GET /login` | `GET /login` | P | `auth.spec.ts` | same |
| G28 | `POST /api/auth/login` (JSON + localStorage) | `POST /login` (form, session) | P | `auth.spec.ts` | fixed (server session) |
| G29 | `GET /logout` | `GET /logout` + `POST /logout` | G | `auth.spec.ts` | same |
| G30 | `GET /register` (+ browser `POST localhost:8083/api/v1/users`) | `GET /register` + `POST /register` | P | `email-verification-flow.spec.ts` | fixed (server-side proxy) |
| G31 | `GET /verify` (+ browser verify / resend to :8083) | `GET /verify` + `POST /verify` + `POST /verify/resend` | P | `email-verification-flow.spec.ts` | fixed (server-side proxy) |
| G32 | `GET /profile` (static) | `GET /profile` + `POST /profile` | G | `profile.spec.ts` (new) | fixed (D1: real data) |
| G33 | `GET /reviews/submit` (static form) | `GET/POST /reviews/submit?token=` | token | `reviews.spec.ts` (new) | fixed (D1: token flow) |
| G34 | `GET /reviews/success` | `GET /reviews/success` | P | `reviews.spec.ts` | same |

### Admin (51 routes, `web_app_admin_tc` → `web_ui_admin`)

| # | Old route(s) | New route | Auth | Spec file | Status |
|---|---|---|---|---|---|
| A1 | `GET /`, `GET /admin` | `GET /admin` (`/` → 308) | A | `dashboard.spec.ts` | same |
| A2 | `GET /login` | `GET /login` | P | `auth.spec.ts` | same |
| A3 | `POST /api/auth/login` | `POST /login` | P | `auth.spec.ts` | same (form instead of fetch) |
| A4 | `POST /api/auth/register` | `POST /register` | P | `host-signup.spec.ts` (new) | same |
| A5 | `GET /register` | `GET /register` | P | `host-signup.spec.ts` | same |
| A6 | `POST /api/auth/verify`, `POST /api/auth/resend-code` | `POST /verify`, `POST /verify/resend` | P | `host-signup.spec.ts` | same |
| A7 | `GET /verify` | `GET /verify` | P | `host-signup.spec.ts` | same |
| A8 | `GET /logout` | `GET /logout` + `POST /logout` | A | `auth.spec.ts` | same |
| A9 | `GET /admin/listings`, `GET /listings` | `GET /admin/listings` (alias 308) | A | `listings-admin.spec.ts` | same |
| A10 | `GET /admin/listings/{id}`, `GET /listings/{id}` | `GET /admin/listings/{id}` (alias 308) | A | `listings-admin.spec.ts` | same |
| A11 | `GET /admin/listings/new` | same | A | `listings-admin.spec.ts` | same |
| A12 | `GET /admin/listings/clone/{slug}` | same | A | `listings-admin.spec.ts` | same |
| A13 | `GET /admin/listings/{id}/edit`, `GET /listings/{id}/edit` | `GET /admin/listings/{id}/edit` (alias 308) | A | `listings-admin.spec.ts`, `listing-fees-and-documents.spec.ts` | same |
| A14 | `POST /admin/listings`, `POST /api/admin/listings/create` | `POST /admin/listings` | A | `listings-admin.spec.ts` | same |
| A15 | — (presign via listing_api) | `POST /admin/listings/{id}/images/presign` (fragment) | A | `listing-images.spec.ts` (new) | same |
| A16 | `GET /admin/listings/{id}/pricing`, `GET /listings/{id}/pricing` | `GET /admin/listings/{id}/pricing` (alias 308) | A | `pricing-admin.spec.ts` (new) | same |
| A17 | `…/pricing/add` (×2, hx-post) | `POST /admin/listings/{id}/pricing/add` (hx-post → `#price-overrides-container` outerHTML) | A | `pricing-admin.spec.ts` | same |
| A18 | `POST /admin/listings/{id}/pricing/remove` (hx-post ×4) | same | A | `pricing-admin.spec.ts` | same |
| A19 | `GET /admin/bookings`, `GET /bookings` | `GET /admin/bookings` (alias 308) | A | `bookings-admin.spec.ts` | same |
| A20 | — (detail dialog built in JS) | `GET /admin/bookings/{id}` (dialog fragment) | A | `bookings-admin.spec.ts` | same |
| A21 | `PATCH /api/bookings/{id}` (status, door code) | `POST /admin/bookings/{id}/status`, `POST /admin/bookings/{id}/door-code` | A | `bookings-admin.spec.ts` | same |
| A22 | `GET/POST /api/bookings/{id}/messages`, `PATCH …/messages/read` | `GET/POST /admin/bookings/{code}/messages` (read marked on view) | A | `booking-messages-admin.spec.ts` (new) | same |
| A23 | `GET/POST /admin/bookings/{code}/messages` (hx-post) | same (`beforeend` `#admin-messages-list`) | A | `booking-messages-admin.spec.ts` | same |
| A24 | `GET /api/bookings/{id}/documents` | rendered into A20 | A | `bookings-admin.spec.ts` | same |
| A24b | — | `GET /admin/bookings/{id}/cancellation-quote` (fragment in A20) | A! | `bookings-admin.spec.ts` | new (D8) |
| A25 | `POST /api/bookings/{id}/change-quote`, `…/change-requests` (GET/POST), `…/{rid}/{approve,decline,cancel}` | `/admin/bookings/{id}/change-*` (fragments) | A | `reservation-changes-admin.spec.ts` (new) | same |
| A26 | `GET /admin/payouts`, `GET /payouts` | `GET /admin/payouts` (alias 308) | A | `payouts-admin.spec.ts` | same |
| A27 | `PATCH /api/payouts/{id}/status` | `POST /admin/payouts/{id}/status` (fragment) | A! | `payouts-admin.spec.ts` | fixed (D10: frontend gate tightened from admin-or-host to admin; hosts read-only) |
| A28 | `GET /admin/exchange-rates` | same | A | `exchange-rates.spec.ts` (new) | same |
| A29 | `GET /admin/users`, `GET /users` | `GET /admin/users` (alias 308) | A! | `users-admin.spec.ts` (new) | same |
| A30 | `GET /admin/users/new` | same | A! | `users-admin.spec.ts` | same |
| A31 | `POST /api/admin/users/{create,update,delete,restore}` | `POST /admin/users`, `POST /admin/users/{id}`, `POST /admin/users/{id}/delete`, `POST /admin/users/{id}/restore` | A! | `users-admin.spec.ts` | same |

Coverage note: the 51 old admin routes collapse into 31 rows, because aliases and sibling change-request endpoints share a row. A24b is new.

### Inline-JS sites to eliminate
~90 `<script>`/`onclick` sites; 23 `fetch(...)` calls (`bookings_admin.rs` ×10, `users_admin.rs` ×4, `login.rs` ×5, `listings_admin.rs`, `payouts_admin.rs`, guest `auth.rs` ×4, `bookings.rs` ×2, `booking_date_change.rs` ×2, `checkout.rs`); 7 `hx-post` sites (pricing ×5, messages ×2). All map to rows above.

## Edge Cases & Failure States

| Case | Behavior |
|---|---|
| Logged-out guest loads any page | HTML contains no user menu, name or email (asserted by e2e on raw response body). |
| Session expires mid-checkout | `POST /checkout` → `HX-Redirect` to `/login?redirect=<checkout + form state>`; after login, prefilled form, availability re-checked. Frontend never extends or re-creates holds. |
| Pending hold expires | Expiry is enforced by the **`pg_cron` job `cleanup_stale_pending_holds`**. It runs every 15 minutes (`*/15 * * * *`), sets `status='cancelled'` on `pending` bookings with `created_at < NOW() - INTERVAL '2 hours'`, and writes a `booking_history` row with reason "Hold expired after 2 hours". It was created in `20260826051703_create_cleanup_cron_job.sql` with a 15-minute window and changed to 2 hours in `20260927000000_add_door_code_and_notification_log.sql`. `booking_api`'s `payment_hold_expiry_reminder` also uses `created_at + 2h`. The effective lifetime is 2 h to 2 h 15 min. The frontend shows the hold's cancelled status after expiry and offers "choose dates again". |
| Session expiry and hold expiry together | Re-login after the cron has cancelled the hold → checkout shows "your hold expired" and restarts from availability. Re-login before that → checkout resumes. |
| Dates taken between quote and submit | `booking_api` rejects (row-locked availability check); page re-renders with conflict alert and refreshed calendar. |
| `redirect=//evil.com`, `redirect=https://…`, `redirect=/\evil` | `safe_redirect` → `/`. |
| Guest requests another guest's booking/document/messages | 404 (frontend check) and 401/403 from `booking_api` (JWT check). |
| Non-UUID path IDs | 404 (never 500). |
| `user_api` / `booking_api` down | Page renders a token-styled error `alert` with retry link; login shows "Service unavailable" without revealing which credential failed. 502-class errors are logged with trace ID. |
| `JWT_SECRET` missing | New apps refuse to start (fail fast), logged once. |
| `op_theme` = garbage | Ignored, default rule applies. `op_tz_offset` outside −840…840 or non-integer → ignored, UTC−05:00 used. |
| Visitor crosses 18:00 while browsing | Next navigation renders dark (no explicit choice saved). No client-side live switch. |
| JS disabled | Login, register, verify, checkout, profile, messages, pricing forms work as plain POST + 303. Calendar falls back to two `<input type="date">` inside `<noscript>`-free progressive markup (calendar grid is enhancement; hidden inputs become visible date inputs when `ui.js` has not set `data-enhanced`). |
| Calendar range spans a blocked night | Selection end is rejected client-side; server validates again on quote/checkout. |
| `unavailable-ranges` fails | Calendar renders all days selectable with a muted "availability will be confirmed at checkout" note; server-side check at quote/checkout still blocks conflicts. |
| HTMX fragment request after session expiry | `HX-Redirect` to login (no partial login page swapped into a fragment target). |
| Two tabs, logout in one | Other tab's next request gets the logged-out shell / redirect. |
| Theme cookie + cached HTML | `Vary: Cookie`, `Cache-Control: private, no-cache`. |
| Topcoat UI registry update | Components are vendored; `topcoat ui list` diff reviewed manually, never auto-updated. |

## Technical Implementation

### Data Models & Schema
- **No migrations.** `sessions.namespace` (default `'guest'`, migration `20260831143500`) is reused.
- FR-7 query (`db_core/src/booking.rs`, new fn `get_unavailable_ranges`), parameterized and using the existing `idx_booking_listing_id`:
  ```sql
  SELECT date_from, date_to FROM booking
  WHERE listing_id = $1 AND status IN ('pending', 'confirmed')
    AND date_from < $3 AND date_to > $2
  ORDER BY date_from
  ```
  Same status set and overlap rule as `check_availability`, so the calendar and the hold logic agree. Read-only, no lock (display only; the hold path keeps its `FOR UPDATE`). Run `cargo sqlx prepare`.
- `common`: add `UnavailableRange { date_from: NaiveDate, date_to: NaiveDate }` and `UnavailableRangesResponse { ranges: Vec<UnavailableRange> }` (documented), plus `merge_ranges(Vec<UnavailableRange>) -> Vec<UnavailableRange>` (pure, unit-tested).

### Crate layout

```
web_client/src/{lib,client,api_client,error,auth,session,jwt,redirect}.rs
web_ui_common/
  components.toml
  src/components/ui/…            # vendored Topcoat UI registry components
  src/components/{toast,steps,star_rating,chat,stat_tile,framed_image,hero,navbar,date_range_calendar}.rs
  src/components/domain/{currency_selector,listing_card,price_breakdown,responsive_image,tax_document_view}.rs
  src/{theme,shell_guest,shell_admin,security_headers,lib}.rs
  src/style/{guest.css,admin.css}
  src/assets/{ui.js,htmx.min.js}
web_ui_guest/src/{main,lib,layout}.rs, src/pages/*.rs, Dockerfile.web_ui_guest
web_ui_admin/src/{main,lib,layout}.rs, src/pages/*.rs, Dockerfile.web_ui_admin
```
- `#[layout("/")]` lives only in the binary crates (linkme constraint from spec 80); `web_ui_common` exports shell functions.
- Neither bin wires `.runtime()` (Phase 0 spike failed, D5). Both still load the `AssetBundle` (served as hashed `/_topcoat/assets/*` files) for `ui.js`, `htmx.min.js`, fonts and CSS. The bundle is only produced when the app runs under `topcoat dev` or the bundler build, as with the old apps; a plain `cargo run` logs "no asset bundle".
- Workspace: add members and `web_client`, `web_ui_common` to `[workspace.dependencies]`.

### API Endpoints (new apps; all HTML unless noted)

| Method & path | Request | Success | Errors |
|---|---|---|---|
| `POST /login` | form `email, password, redirect?` | 303 / `HX-Redirect` → `safe_redirect` | 200 re-render with `alert` (generic message; "not verified" links to `/verify`) |
| `POST /register` | form per `NewUserRequest` | 303 `/verify?email=` | 200 re-render with field errors (`validator`) |
| `POST /verify`, `/verify/resend` | form `email, code` / `email` | 303 `/login?verified=1` / toast | 200 re-render |
| `POST /logout`, `GET /logout` | — | 303 `/login` | — |
| `GET /listings/{slug}/calendar?month=YYYY-MM&start=&end=` | query | fragment `#calendar` | 400 on bad month |
| `GET /listings/{slug}/quote?check_in&check_out&…&currency` | query | fragment `#quote` | inline error |
| `POST /checkout/{slug}` | form (dates, party, arrival, message, currency) | 303 `/bookings?new_booking=true` | 200 re-render; 303 login if no session |
| `POST /bookings/{id}/cancel` | form `reason?` | fragment (booking card) + `HX-Trigger: toast` | 404 not owner |
| `POST /bookings/{id}/change-*` | form | fragment | 404 / inline error |
| `POST /profile` | form | 303 `/profile` + toast | re-render |
| `POST /reviews/submit` | form `token, ratings…, comment` | 303 `/reviews/success` | 404 bad token |
| Admin `POST /admin/...` (A14–A31) | form | fragment or 303 | 403 role, 404, inline error |
| `booking_api GET /api/v1/bookings/listing/{id}/unavailable-ranges` (JSON) | `from`, `to` | 200 `{ranges}` | 400 span > 366 / from ≥ to; 404 listing |

All state-changing routes are `POST` and pass the CSRF check (§Security).

### Frontend Views
- **Guest**: landscape `hero` with overlaid search (location, `date_range_calendar`, guests); portrait `listing_card` grid; listing detail with gallery (`dialog` lightbox, arrow keys via `ui.js`), amenities, map image link, reviews with `star_rating`; checkout with `steps` (dates → details → confirm), sticky `price_breakdown`; bookings as cards with status `badge`, actions in `dropdown_menu`, change/cancel in `alert_dialog`; messages via `chat_thread`; profile via `field`/`input`/`tabs`.
- **Admin**: `sidebar` shell; dashboard `stat_tile`s; dense `table`s with `pagination`; booking detail in `dialog` loaded by `hx-get`; listing studio in `tabs`; pricing overrides as an HTMX-swapped table.
- **`date_range_calendar`**: server renders a two-month `role="grid"` table (one month below `sm`), each day a `<button>` with `aria-label="Mon 12 Jan 2027, available"`, `aria-disabled` on past/blocked nights. Prev/next month buttons `hx-get` the fragment. `ui.js` handles click and arrow/Home/End/PageUp/PageDown, writes hidden `check_in`/`check_out` inputs, and fires `change` so the quote fragment refreshes (`hx-trigger="change from:#booking-dates"`). Blocked nights come from FR-7 for the visible window.
- **Required IDs / `data-testid`s preserved**: `#messages-list`, `#admin-messages-list`, `#price-overrides-container`, `#checkout-form`, `#checkout-error-banner`, `#booking-success-toast`, `#upcoming-bookings-container`, `#no-bookings-card`, `#guest-login-error`, plus every ID/test-id the Phase 2 selector sweep records. `nav-user-dropdown` / `nav-guest-actions` keep their IDs but are rendered conditionally instead of toggled.

### Background Workers
None. Emails triggered by booking/verification flows are unchanged (same backend calls).

### Phase 0: CSP spike (gates D5)
Render a page using one runtime signal under `Content-Security-Policy: script-src 'self'`; run Chromium + Firefox; record any violation. Also check that Topcoat/HTMX emit no inline `style=` that `style-src 'self'` would block. Record the result in this spec under "Spike results" before Phase 3 starts.

#### Spike results (2026-10-28, Topcoat 0.10.0, `web_ui_guest` under `topcoat dev`)

**Outcome: FAIL. The Topcoat runtime cannot run under `script-src 'self'`. D5 applies: no `.runtime()`, no signals, no `@event` handlers, no `$(…)` runtime expressions, and no shards in `web_ui_*`.**

The spike page had one `signal(cx, || 0.0)`, a button with `@click=$(|_e| count.set(count.get() + 1.0))`, and a `$(count.get())` text node. The CSP was `default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'`, delivered via `<meta http-equiv>`.

| Check | Chromium | Firefox |
|---|---|---|
| Runtime script loads (`<script type="module" src="/_topcoat/assets/topcoat-<hash>.js">`) | ✅ allowed by `'self'` | ✅ |
| `securitypolicyviolation` events | `script-src blocked=eval` | `script-src blocked=eval` ("Missing 'unsafe-eval'") |
| Page error | `Failed to compile event @click` | same |
| Click updates the signal | ❌ stays `Count: 0` | ❌ |

**Why:** the server ships every handler and expression as **JavaScript source text**, in `data-topcoat-on:<event>="(…) => …"` attributes and `<!--::topcoat::expr::start("…")-->` comments. The browser runtime compiles them with `new Function("cx", "return " + source)` (`topcoat-runtime/browser/src/expression/compile.ts`). Making them work would need `'unsafe-eval'`, which defeats the point of the CSP. A nonce or hash does not help, because `eval` is not covered by nonces.

**Other findings:**
- The vendored `htmx.min.js` has no `Function(`/`eval` path except `hx-on`, which is already banned. It is compatible with the strict CSP.
- The spike page itself emitted no inline `style=`. Topcoat UI registry components are fetched by `topcoat ui add`, so Task 3 must check them for inline `style=` attributes **and for `@event`/`$(…)` runtime usage**. Any component that relies on the runtime gets rewritten to native elements plus `data-*` hooks in `ui.js`.
- **Consequence for the plan:** every interactive behavior is native HTML (`<dialog>`, `<details>`, forms), HTMX, or `ui.js`. Topcoat UI components that rely on the runtime are patched in-repo (they are source-owned).

#### Topcoat UI registry audit (Task 3, 29 components, theme `neutral`)

| Component | Finding | Effect without the runtime | Action when first used |
|---|---|---|---|
| `dialog`, `alert_dialog`, `sheet` | `open: Expr<bool>` bound with `:attr=$(…)` | Initial state is server-rendered; it never changes on the client. The leftover `data-topcoat-*` attributes are inert, because no runtime script is loaded. | Open/close via native `<dialog>` + `ui.js` (`data-dialog-open` / `data-dialog-close`); change the prop to a plain `bool`. |
| `sidebar` | `open`, `mobile_open`, `active` as `Expr<bool>` (14 bindings) | Same as above | Collapse/mobile state via `ui.js` + a cookie; `active` as a plain `bool`. |
| `tabs` | `active: Expr<bool>` for `aria-current` | Same as above | Plain `bool`; tab switching via links/HTMX or `ui.js`. |
| `select` | `style=(checkmark_style(cx))` inline style | Blocked by `style-src 'self'` | Replace with a class or token-driven stylesheet rule. |
| other 23 | No runtime usage, no inline styles | Work as-is | — |

**Changes needed to vendor the registry under this repo's rules:**
- Enabled the `ui` and `icon-iconify` features on `topcoat` and added `web_ui_common/build.rs`, which stages the `lucide` icon set.
- Added a `//!` doc to each file. `#[component]` turns each parameter into a props-struct field, and a `///` on the parameter is forwarded to that field, so every parameter got a `///` (crate-level `deny(missing_docs)`).
- Removed 16 `#[allow(dead_code)]`, which aren't needed on public items (CONSTRAINTS: zero suppression pragmas).
- The registry's 62 usage examples are ` ```ignore ` doctests (they need a page context to compile). They are kept as documentation, not counted as silenced tests.
- These edits change the files' hashes from `components.toml`, so `topcoat ui add --overwrite` would discard them. Review registry updates by diff instead.

## Performance Considerations
- **Cold start (< 300 ms p50, < 1 s p95)**: no DB pool in the frontends (HTTP only), `reqwest` client built once and stored in `app_context`, asset bundle loaded once. No `chrono-tz` (D7). Release profile already `opt-level="z"`, LTO.
- **Memory (256 MiB)**: SSR views stream; no per-request caches. Calendar fragment renders ≤ 62 day cells.
- **Request fan-out**: listing detail = listing + reviews + unavailable-ranges for 2 months; issue them concurrently with `tokio::try_join!` (old app calls sequentially). `/bookings` = one `bookings/user/{id}` call; listing names come from the booking payload or one batched search, never per-booking calls (N+1).
- **Session lookup**: one `user_api` call per protected request (as today). Pages that render the guest nav but are public call `get_guest_session` once per request and pass the result down; no repeated lookups in components.
- **FR-7 query**: O(k) in overlapping bookings for one listing over ≤ 366 days, served by `idx_booking_listing_id`; `merge_ranges` O(k) on sorted input.
- **Assets**: `ui.js` ≤ 15 KB, hashed, `Cache-Control: public, max-age=31536000, immutable`. One CSS file per app. Fonts self-hosted via `topcoat-font`, `font-display: swap`, preload the two used weights.
- **No blocking**: no CPU-heavy work in the frontends (password hashing stays in `user_api` behind `spawn_blocking`).

## Security Mitigations

**Trust boundaries**: browser → new apps (forms, query strings, path params, cookies `op_*`); new apps → backend APIs (service JWT); browser → GCS (presigned PUT only).

| Threat (OWASP) | Mitigation |
|---|---|
| A01 Broken access control / IDOR | `require_guest` / `require_admin` / `require_admin_role` on every page, fragment and POST; booking-scoped routes check `guest_id == session.user_id` (404 otherwise); lists only from `bookings/user/{session.user_id}`; backend enforces again via JWT `sub`. Fixes the old `/bookings` all-bookings listing and the unchecked cancellation quote. |
| A01 Open redirect | `safe_redirect` on every `redirect` param. |
| A01 CSRF | Session cookies `SameSite=Lax; HttpOnly; Secure` (prod). Middleware rejects non-GET requests whose `Origin` (or, if absent, `Referer`) is not the app's own origin (403). `GET /logout` is the only state-changing GET (parity); it is low impact. |
| A02 Crypto / secrets | `JWT_SECRET` required, no default; Secret Manager in Cloud Run. Service JWTs expire in 5 min (old: 1 h). Session tokens stored hashed (`token_hash_to_hex`), as today. |
| A03 Injection / XSS | Topcoat `view!` escapes by default; no `raw` HTML from user content (messages, reviews, names). Strict CSP below. Backend SQLx queries are parameterized (FR-7 included). |
| A04 Insecure design | No auth state in browser storage; server renders auth-dependent nav. Session fixation: `session::stop` before `session::start` on login. |
| A05 Misconfiguration | Headers on every response: `Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' https://storage.googleapis.com data:; font-src 'self'; connect-src 'self'` (admin: `+ https://storage.googleapis.com`)`; frame-ancestors 'none'; base-uri 'self'; form-action 'self'; object-src 'none'`; `X-Content-Type-Options: nosniff`; `Referrer-Policy: strict-origin-when-cross-origin`; `Permissions-Policy: camera=(), microphone=(), geolocation=()`; `Strict-Transport-Security: max-age=31536000; includeSubDomains` (prod only); `Cross-Origin-Opener-Policy: same-origin`. |
| A07 Auth failures | Generic login errors (no user enumeration beyond the existing "not verified" hint); rate limiting stays in `user_api` (unchanged); logout deletes the server session. |
| A08 Integrity | Vendored Topcoat UI components; pinned `components.toml`; `htmx.min.js` vendored, not from a CDN. |
| A09 Logging | Log auth failures and 403/404 ownership denials with trace ID and user ID, **never** passwords, codes, tokens or full emails (mask local part). |
| A10 SSRF | Backend base URLs come from config only; no user-supplied URLs are fetched server-side. |
| Cookie tampering | `op_theme` / `op_tz_offset` are allowlisted/range-checked; they never affect auth. |

## Test Plan

### Unit Tests
- `cargo test -p web_client`: `safe_redirect` table (relative ok; `//`, `/\`, `http:`, `javascript:`, overlong → `/`); `GuestSession` from session DTO (missing `user_id` → error, never nil UUID); `ServiceJwt` (missing secret → `Err`; minted token decodes with `sub`, `exp ≤ now+5min`); `AuthUser::initials` (moved tests).
- `cargo test -p web_ui_common`: `resolve_theme` matrix (each cookie; offset −840/0/840/invalid; hours 17:59/18:00/05:59/06:00 local; Jamaica default); currency/price formatting parity vs. old components for USD/JMD/EUR/GBP/CAD with fixed `Decimal` inputs; calendar month grid (leap Feb 2028, month starting Sunday, blocked/past flags, `aria-label` text); `security_headers` emits exact CSP string per app.
- `cargo test -p common`: `merge_ranges` (empty, disjoint, touching `date_to == date_from`, nested, unsorted input).
- `cargo test -p web_ui_guest -p web_ui_admin`: view render tests — logged-out shell contains no `nav-user-dropdown`; admin shell hides user-management links for `host`.

### Integration Tests
- `cargo test -p booking_api`: `unavailable-ranges` returns only pending/confirmed overlapping ranges, no guest fields in JSON, 400 on span > 366 and `from ≥ to`, 404 unknown listing.
- `cargo test -p web_ui_guest` (with a mocked API client trait or `wiremock`): login sets session and calls `create_session` with `namespace="guest"`; protected page without session → 303 with encoded `redirect`; booking of another user → 404; checkout POST builds `guest_id` from session (ignores any posted `guest_id`); cross-origin POST → 403.
- `cargo test -p web_app_tc -p web_app_admin_tc -p web_app_common_tc` unchanged and green after Phase 1.

### E2E Tests (Playwright, Chromium + Firefox)
- **Existing** (selectors migrated): `home`, `listings`, `listing-detail`, `auth`, `checkout-flow`, `email-verification-flow`, `tax-documents` (guest); `auth`, `dashboard`, `listings-admin`, `listing-fees-and-documents`, `bookings-admin`, `payouts-admin` (admin).
- **New**: `static-pages`, `calendar`, `bookings`, `reservation-changes`, `messaging`, `profile`, `reviews` (guest); `host-signup`, `listing-images`, `pricing-admin`, `booking-messages-admin`, `reservation-changes-admin`, `exchange-rates`, `users-admin` (admin).
- **Security / auth scenarios** (`@new-only`): logged-out raw HTML has no user nav; `localStorage`/`sessionStorage` contain no auth keys after login; guest A cannot open guest B's booking, messages or document (404); `redirect=//evil.com` lands on `/`; session cookie deleted mid-checkout → login → prefilled checkout → booking succeeds; same with dates taken during re-login → conflict state; CSP fixture reports zero violations across the whole suite.
- **Theme**: toggle cycles light → dim → dark and persists across reload with no class change after first paint (assert `<html>` class in the initial response); clock mocked to 18:00 and 17:59 in Jamaica for first visit; `op_tz_offset` set to +540 changes the default.
- **Accessibility**: `@axe-core/playwright` on each page × 3 themes × both apps, zero serious/critical violations; keyboard-only calendar selection; visible focus ring.
- **Parity gate command**: `npx playwright test --project=guest-portal-chromium --project=guest-portal-firefox --project=admin-portal-chromium --project=admin-portal-firefox --project=guest-portal-new-chromium --project=guest-portal-new-firefox --project=admin-portal-new-chromium --project=admin-portal-new-firefox`.

## Acceptance Criteria
- [x] 1. Phase 0 spike result recorded in this spec; `.runtime()` usage matches it (D5): runtime dropped.
- [ ] 2. `git diff main -- web_app_tc web_app_admin_tc` is empty; `cargo test -p web_app_tc -p web_app_admin_tc -p web_app_common_tc` passes.
- [ ] 3. Every row in §Parity Inventory has a passing spec on its required projects (`same` → old + new; `fixed` → new; `retired` → none), Chromium and Firefox.
- [ ] 4. `rg -i 'daisyui' web_ui_common web_ui_guest web_ui_admin web_client` returns nothing.
- [ ] 5. `rg '<script>|onclick=|hx-on|localStorage|sessionStorage' web_ui_*/src --glob '!**/assets/*.js'` returns nothing; `rg 'get_authenticated_(guest|admin)' web_ui_*` returns nothing.
- [ ] 6. The CSP header (exact string in §Security) is present on every response of both new apps; the e2e suite reports zero CSP violations.
- [ ] 7. Logged-out guest responses never contain signed-in navigation; no auth keys exist in browser storage after any flow.
- [ ] 8. Guests can only read or act on their own bookings, messages and documents (e2e cross-account tests pass). `/bookings` for guest A never lists guest B's bookings, and `rg 'get_all_bookings' web_ui_guest` returns nothing.
- [ ] 8a. A cancellation quote is visible only to the booking's guest (guest portal) or an admin-role user (admin portal). Another guest gets 404, and a host gets 403 with no quote control rendered.
- [ ] 8b. Payout status can be changed only by admin-role users. Hosts see the ledger read-only (no status controls in the HTML), and a forged host POST returns 403.
- [ ] 9. Checkout creates a real booking with the selected dates and the session user's ID; session-expiry-mid-checkout e2e (both branches) passes.
- [ ] 10. Theme toggle on every page; persists via `op_theme`; time-of-day default with Jamaica first-visit and `op_tz_offset` thereafter; no flash (class present in the initial HTML).
- [ ] 11. `booking_api` `unavailable-ranges` endpoint shipped with integration tests, OpenAPI annotations and `cargo sqlx prepare` metadata; calendar greys out booked nights.
- [ ] 12. `CONSTRAINTS.md` updated (tightened only): panic gate → `! rg -q "\.(unwrap|expect)\(" app_api/ common/ db_core/ web_client/ web_app_tc/src web_app_admin_tc/src web_app_common_tc/src web_ui_common/src web_ui_guest/src web_ui_admin/src` (production code; `#[cfg(test)]` modules moved to `tests/` or excluded via an `rg` multiline guard recorded with the gate); `missing_docs` gate covers `web_client`, `web_ui_*`; new test rows for `web_client`, `web_ui_common`, `web_ui_guest`, `web_ui_admin`; Stage 2 confirms `app_api/`, `common/`, `db_core/` pass the gate.
- [ ] 13. `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test` for all new crates pass.
- [ ] 14. axe: zero serious/critical violations in all 6 theme combinations.
- [ ] 15. Cold start < 300 ms p50 and RSS < 256 MiB for both new Cloud Run services (`/cloudrun-perf-audit`).
- [ ] 16. Dockerfiles, compose services (:3010/:3012) and Terraform services exist; services are not attached to the production domain.

## Hold-Expiry E2E Strategy
- **Where expiry is enforced**: only by the `pg_cron` job `cleanup_stale_pending_holds` (see Edge Cases). There is no `expires_at` column and no check at request time.
- **Why tests can't wait for it**: both migrations skip `pg_cron` setup when `current_database() LIKE '%test%'`, and the window is 2 hours anyway.
- **Test approach**: add `expirePendingHold(bookingId)` to `playwright/e2e/setup/db-seed.ts`. It runs the **same** `UPDATE … SET status='cancelled'` + `booking_history` insert that the cron runs, scoped to one booking. The SQL is kept as a constant that mirrors the latest migration, with a comment linking it there.
- **Scenario**: create a hold → delete the session cookie → `expirePendingHold` → log in → expect the "hold expired" state and a restart from availability. The sibling scenario skips `expirePendingHold` and expects checkout to resume.

## Open Questions / Verification Items
1. **Hold TTL (resolved 2026-10-28)**: **2 hours** is correct. That is the rule already in production (migration `20260927000000`, reminder email). This PR changes the hold time from 15 minutes to 2 hours in `CONSTRAINTS.md` §3, the intent, `docs/ARCHITECTURE.md` and the repo skills, in a separate wording-only commit (plan Task 57). No backend change. The spec doesn't depend on the value: tests use `expirePendingHold`, and the UI never shows a countdown derived from a hardcoded TTL.
2. **`booking_api` cancellation-quote authorization vs D8**: the backend's `authorize_booking_party` lets the **guest, the listing's host, or an admin** read the quote. D8 says guest or admin only. The new frontends enforce D8. Removing hosts at the API layer is a backend change and should be a follow-up issue unless you want it in this PR.
3. **Unauthenticated per-user booking lists (follow-up issue, backend)**: `booking_api GET /api/v1/bookings/user/{id}` and `user_api GET /api/v1/users/user/{id}/bookings` take no `Claims`. Proposed fix:
   - Add a `claims: api_core::auth::Claims` extractor to both handlers. Missing or invalid JWT → 401.
   - Allow the request only if `claims.sub == id` or the caller has the `Admin` role.
   - Otherwise return **404**, matching `authorize_booking_party`'s anti-enumeration convention.
   - Callers must send a user JWT: `web_client` already will (FR-4.7). `common::app_client::get_user_bookings` needs an extra `token: Option<&str>` argument (no current caller passes one).
   - Add tests: own ID 200, other ID 404, admin 200, no token 401.
   - Cloud Run IAM (`get_client` audience tokens) is not a substitute: it authenticates the *calling service*, not the end user.
