# Intent: Redesigned Topcoat UI Portals (daisyUI → Topcoat UI) with Full Feature Parity

## Metadata
- **Author**: Originator (via GitHub Issue #96, @pavelbyles)
- **Date**: 2026-10-28
- **Status**: Approved
- **Target Area**: Frontend + shared web plumbing (`web_client` NEW, `web_ui_common` NEW, `web_ui_guest` NEW, `web_ui_admin` NEW, `web_app_common_tc`, Playwright suite, Dockerfiles, docker-compose, `infra/`); minimal `user_api` touch for guest sessions only.
- **Source Issue**: https://github.com/pavelbyles/our_places_rs/issues/96
- **Design Reference**: [`docs/design/guest-theme.md`](../design/guest-theme.md) ("Shoreline Mist")

---

## 1. Problem Statement & Motivation
The current Topcoat portals (`web_app_tc` :3000, `web_app_admin_tc` :3002, shared `web_app_common_tc`) have five structural problems:

1. **Untyped, repeated styling**: daisyUI class strings appear 1,000+ times (~350 `btn`, ~310 `input`, ~186 `badge`). Styling decisions are hand-copied, and the compiler can't check them.
2. **Theme drift**: The guest and admin crates use `emerald`/`sunset`, while `web_app_common_tc` uses `emerald`/`night` with a different `dark:` variant. There are three separate `tailwind.css` entry points.
3. **Guest auth exists only in the browser**: `get_authenticated_guest` ([auth.rs:180](../../web_app_common_tc/src/auth.rs#L180)) always returns `None`. Guest login is a JS `fetch` that stores the user in `localStorage['op_auth_user']` ([auth.rs:356](../../web_app_common_tc/src/auth.rs#L356)), and a script then toggles the nav. The server never knows who the guest is, so logged-out guests can briefly see signed-in navigation, and protected guest pages can't be enforced on the server.
4. **Inline JS everywhere**: ~90 inline `<script>` / `onclick` sites prevent a strict Content-Security-Policy and make behavior hard to test.
5. **Brand gap**: The guest portal should look like a premium luxury-villa brand. The admin portal should be a clean, dense, data-focused tool.

**Why now**: These problems compound with every new feature (#81 messaging, #92 reservation changes, #60 tax documents). Each new screen adds more daisyUI strings and inline JS, and adds another guest page that depends on browser-only auth.

---

## 2. Proposed Outcome (The Vision)
Two **new, redesigned portals** are built from scratch on **Topcoat UI**, a set of source-owned, typed Rust components with `styles.css` design tokens. They run **side by side** with the old portals until a Playwright parity gate proves functional equivalence. Production cutover and retiring the old crates happen in a follow-up PR.

| Capability | Old portals | New portals |
|---|---|---|
| Component layer | daisyUI class strings | Typed Topcoat UI `#[component]`s, owned in-repo |
| Themes | emerald/sunset/night, drifting | One token schema; guest "Shoreline Mist" + admin neutral/teal; **light → dim → dark** |
| Theme persistence | Inline script, `data-theme` | Server reads a cookie and renders the `<html>` class; no flash, no inline JS |
| Guest auth | `localStorage['op_auth_user']` | Topcoat server session, `guest` namespace |
| Client JS | ~90 inline sites | Topcoat runtime signals + **one** hashed `ui.js` |
| CSP | None possible | `script-src 'self'`, `frame-ancestors 'none'` |
| Ports | :3000 / :3002 | :3010 / :3012 |

### Target crate graph
```
web_client          (NEW, no UI)  API client, HTTP client, AppError, auth/session logic
├── web_app_common_tc  (existing)  re-exports web_client at the SAME paths; keeps daisyUI layouts/scripts
│   ├── web_app_tc        (existing, :3000) zero source changes
│   └── web_app_admin_tc  (existing, :3002) zero source changes
└── web_ui_common      (NEW)       Topcoat UI components, both themes, ui.js, shared layouts
    ├── web_ui_guest      (NEW, :3010)
    └── web_ui_admin      (NEW, :3012)
```
All new crates depend on `common` for domain models, `Decimal` pricing, and DTOs. Frontends never depend on API crates.

---

## 3. Scope Boundaries

### In Scope (MVP)
- **Phase 0, Foundations**: Add the four crates to the workspace. Run a **CSP spike** to confirm the Topcoat runtime (documented as experimental) emits no inline script or `eval` under `script-src 'self'`. If it does, fall back to `ui.js` only. Record the result in the spec.
- **Phase 1, extract `web_client`**: Move `api_client.rs`, `client.rs`, `error.rs`, and the Rust-only parts of `auth.rs` (`AuthUser`, `AdminAuthError`, `require_admin_auth`, `get_authenticated_*`, `token_hash_to_hex`, session lookup). `web_app_common_tc` re-exports them at identical paths. The inline JS helpers stay where they are.
- **Phase 2, parity inventory and test baseline against the OLD apps**:
  - Inventory every route and HTMX endpoint: 32 guest routes, 51 admin routes, 17 `hx-*` calls. For each, record its purpose, auth requirement, and covering e2e test.
  - Write the missing Playwright tests and get them passing on :3000 / :3002 first.
  - Replace daisyUI-class selectors with roles or `data-testid`.
  - Add Playwright projects for :3010 / :3012 that run the same spec files.
- **Phase 3, `web_ui_common`**:
  - Two theme CSS files (`guest.css`, `admin.css`) with identical token names and light/`.dim`/`.dark` variants.
  - Install about 29 Topcoat UI components.
  - Build the ones Topcoat UI lacks: toast, steps, star rating, chat thread, stat tile, framed image, hero, navbar, date-range picker.
  - Port the domain components: `currency_selector`, `listing_card`, `price_breakdown`, `responsive_image` (keeping `<picture>`/`srcset`), `star_rating`, `tax_document_view`.
  - `ui.js`: native `<dialog>` handling, outside-click dismiss, theme cycle and cookie, HTMX glue.
  - Guest and admin shell layouts.
- **Phase 4, `web_ui_guest`**:
  - A redesigned page for every inventoried guest route.
  - **Guest server sessions**: login and registration via HTMX-enhanced form posts through `user_api`, with a `guest`-namespace session.
  - Server-rendered auth nav; logout deletes the session.
  - Protected pages are checked on the server and limited to the guest's own resources.
- **Phase 5, `web_ui_admin`**: A redesigned page for every inventoried admin route. Admin auth reuses the existing `admin`-namespace session logic, now in `web_client`.
- **Phase 6, deployment wiring**:
  - `Dockerfile.web_ui_guest` / `Dockerfile.web_ui_admin` and compose services.
  - CSP and hardening headers.
  - Terraform Cloud Run services, **not** attached to the production domain.

### Out of Scope / Non-Goals
- Domain cutover and deleting `web_app_tc` / `web_app_admin_tc` / `web_app_common_tc` (follow-up PR).
- Backend or API changes, except a minimal `user_api` adjustment if guest sessions need one.
- New product features. Parity only.
- Transactional email templates.
- Moving the **old** guest portal to server sessions.
- Firefox styling of customizable `select` (`::picker(select)`). Firefox gets the native select.

---

## 4. Affected Systems & Stakeholders
- **User Personas**:
  - **Guests**: full visual redesign; auth moves to the server; behavior otherwise unchanged.
  - **Hosts/Admins**: denser, data-focused admin UI.
  - **Developers/agents**: typed components; one theme schema.
- **System Components**:

| Component | Change |
|---|---|
| `web_client` (NEW) | UI-free API client, `AppError`, session/auth extraction |
| `web_app_common_tc` | Becomes a re-export facade over `web_client`; daisyUI UI retained |
| `web_app_tc`, `web_app_admin_tc` | **No source changes** (must still compile and pass tests) |
| `web_ui_common` / `web_ui_guest` / `web_ui_admin` (NEW) | Full new UI layer |
| `db_core` `sessions` table | Reused. `namespace` already defaults to `'guest'` (migration `20260831143500`). No migration expected |
| `app_api/user_api` | No change expected. Originator confirmed the existing login response is enough to issue a `guest` session |
| `playwright/` | New specs, selector refactor, 4 new projects (guest/admin × chromium/firefox) |
| `Cargo.toml`, Dockerfiles, `docker-compose.yml`, `infra/` | New members, images, services |
| `CONSTRAINTS.md` gates | Extended (tightened, not weakened). The panic-prevention gate covers **every Rust crate**: `app_api/ common/ db_core/ web_client/ web_app_tc/ web_app_admin_tc/ web_app_common_tc/ web_ui_common/ web_ui_guest/ web_ui_admin/`. This replaces the stale `web_app/` path and is scoped to production code (`src/`, excluding `tests/` and `#[cfg(test)]`). The `missing_docs` gate covers the new crates. New Frontend/Admin test rows are added for `web_client` / `web_ui_common` / `web_ui_guest` / `web_ui_admin` |

---

## 5. Constraints & Non-Negotiable Invariants
- **Stack**: Topcoat **0.10** SSR + HTMX + Tailwind v4. State lives on the server. **No** WASM, SPA framework, or Alpine.
- **No daisyUI** in any new crate. Components use only theme tokens (no raw hex).
- **Domain**: `Decimal` for all money; tri-currency display must render identically to the old portals. The 15-minute checkout hold behavior is unchanged.
- **Session expiry mid-checkout** (decided): If the guest session expires during checkout, the guest is asked to log in again. The `pending_payment` hold is **not** extended or re-created; it keeps running on its original `expires_at`. If they log in again before it expires, checkout resumes. If the hold has also expired, checkout must restart from availability.
- **Panic-free**: no `.unwrap()` / `.expect()` in production paths. Monadic error handling. `///` docs on all public items (`RS-D1001`). No `todo!()` stubs.
- **Backwards compatibility**: The old portals must keep working, unchanged at the source level, throughout.
- **Test contract**: Keep the element IDs and `data-testid`s the shared Playwright suite depends on. Visual structure can change freely.
- **Security**:
  - No auth state in `localStorage` / `sessionStorage`.
  - CSP `script-src 'self'` with no `unsafe-inline` / `unsafe-eval`. Set `htmx.config.includeIndicatorStyles = false`; no `hx-on`.
  - `frame-ancestors 'none'` plus standard hardening headers.
  - Guests can access only their own resources, enforced on the server.
- **Performance**: Cloud Run 256MB RAM, cold start < 300ms p50 / < 1s p95.
- **Accessibility**: Keyboard reachable, visible `--ring` focus, labelled fields, WCAG AA contrast in all 6 theme combinations (guest/admin × light/dim/dark).

---

## 6. Success Metrics & Signals
- **Leading Indicators** (the parity gate):
  - [ ] `web_app_tc` / `web_app_admin_tc` are unchanged at the source level and green after the `web_client` extraction.
  - [ ] The parity inventory maps 100% of old routes and HTMX endpoints to new routes.
  - [ ] Every inventoried flow has a Playwright test that passes on old **and** new apps, in Chromium and Firefox. Exception: existing bugs are fixed in the new app only, and each deviation is recorded in the parity inventory.
  - [ ] Session expiry mid-checkout has e2e coverage: re-login within the hold window resumes checkout, and re-login after hold expiry forces a restart.
  - [ ] `rg 'daisyui' web_ui_* web_client` and `rg '<script>|onclick=' web_ui_*/src` both return nothing.
  - [ ] The e2e suite runs with **zero CSP violations** on both new apps.
  - [ ] A logged-out guest never receives signed-in nav in the HTML. No auth keys in browser storage.
  - [ ] The light/dim/dark toggle appears on every page and persists. Time-of-day default (dark at or after 18:00, otherwise light): on the **first visit** the server uses `America/Jamaica` time. `ui.js` then stores the visitor's timezone in a cookie, and later defaults use the **visitor's local time**. An explicit toggle choice always overrides the default. No flash of the wrong theme, and no inline script.
  - [ ] `cargo test` passes for all new crates. `CONSTRAINTS.md` has test rows for them. Its panic-prevention gate covers production code in every Rust crate, old and new, and passes. Its `missing_docs` gate covers `web_client/` and `web_ui_*/`.
  - [ ] Cold start and memory are within the Cloud Run budget.
- **Lagging Indicators**:
  - Domain cutover PR merged and old crates deleted.
  - daisyUI class-string count in the repo reaches 0 after cutover.
  - Fewer styling and theme regressions in later feature PRs.

---

## 7. Open Questions & Policy Concerns
### Resolved (originator, 2026-10-28)
- [x] **Guest session issuance**: `user_api` already returns everything needed. No backend change is planned.
- [x] **Session expiry mid-checkout**: Ask the guest to log in again. The hold is not extended; it lives or dies on its original `expires_at` (see §5).
- [x] **Theme default timezone**: First visit uses `America/Jamaica` time, rendered on the server with no flash. Later visits use the visitor's local time, from a timezone cookie set by `ui.js`.
- [x] **Parity test baseline**: Fix existing bugs in the new app only, and record each deviation in the parity inventory.
- [x] **PR sizing**: One PR for all phases is fine (single developer).
- [x] **CONSTRAINTS.md coverage**: Add `cargo test` rows for the new crates. Extend the `missing_docs` gate to `web_client/` and `web_ui_*/`.
- [x] **Panic gate scope**: The `.unwrap()`/`.expect()` gate applies to **all paths containing Rust code**, replacing the stale `web_app/` path. Verified 2026-10-28: `web_app_tc/src`, `web_app_admin_tc/src`, and `web_app_common_tc/src` already have zero hits, so the old crates stay unchanged. Their hits are all in `tests/`, so the gate must exclude test code (`tests/`, `#[cfg(test)]`). Stage 2 must confirm the same holds for `app_api/`, `common/`, and `db_core/`.

### Still Open
- [ ] **Theme cookie and SSR caching**: Responses vary by cookie. Confirm there is no CDN/edge HTML caching, or add `Vary: Cookie`.
- [ ] **Shadow-user / guest-checkout flow**: Confirm unauthenticated checkout (shadow user) works unchanged with server sessions, and that promotion creates a `guest` session without dropping the hold.
- [ ] **CSP spike fallback**: If the Topcoat runtime fails under strict CSP, is "`ui.js` only" acceptable for every interactive component (date-range picker, gallery, chat)?
- [ ] **Date-range picker**: Native `<input type="date">` pair, or a custom component? This affects accessibility and Firefox parity.
- [ ] **Known risk, Topcoat UI maturity (0.x)**: Pin the registry versions and review `topcoat ui list` diffs. Patched components are owned in-repo.
