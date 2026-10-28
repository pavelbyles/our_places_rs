# Implementation Plan: Redesigned Topcoat UI Portals (#96)

**Spec Reference:** `docs/specs/spec-96-feat-redesigned-topcoat-ui-portals-daisyui-topcoat-ui.md`
**Baseline Commit:** `b77cfb7a`

## Conventions for every task
- Gates run on each task: `cargo fmt --check` and `SQLX_OFFLINE=true cargo clippy -p <touched crates> --all-targets --all-features -- -D warnings`. New crates enforce docs with a crate-level `#![deny(missing_docs)]`. Passing `-D missing_docs` on the command line also lints `common`, which already fails it, so that is left to Task 57.
- **Old-app guard** (any task touching `web_app_common_tc`, `common`, `playwright/`): `git diff b77cfb7a -- web_app_tc web_app_admin_tc` must be empty except `web_app_*/tests` selector updates.
- Shorthands: `NEW_GUEST` = `npx playwright test --project=guest-portal-new-chromium --project=guest-portal-new-firefox`; `NEW_ADMIN` = same with `admin-portal-new-*`; `OLD_GUEST` / `OLD_ADMIN` = the existing `guest-portal-*` / `admin-portal-*` projects.
- No `.unwrap()`/`.expect()`, inline `<script>`, `onclick`, `hx-on`, `localStorage` or daisyUI in `web_client` / `web_ui_*`.

## Dependency graph (phases)

```
P0 Foundations ──┬─► P1 web_client ──────────────┐
  (T1–T3, risk)  ├─► P2 Parity baseline (old) ───┤
                 └─► P3 web_ui_common ───────────┼─► P4 web_ui_guest ─┐
                       ▲                         │                    ├─► P6 Deploy & gates
   P3b availability (backend, T29–T31) ──────────┘─► P5 web_ui_admin ─┘
```
P1, P2 and P3 can run in parallel after P0. P4/P5 page tasks are independent of each other once their shell task is done.

---

## Phase 0: Foundations (risk-first)

- [x] **Task 1: Scaffold the four new crates**
  - **Scope:** M
  - **Description:** Add `web_client`, `web_ui_common` (libs) and `web_ui_guest`, `web_ui_admin` (bins, ports 3010/3012 defaults) to the workspace with minimal documented `lib.rs`/`main.rs` and workspace deps. Bins serve a placeholder `/` page.
  - **Files Touched:** `Cargo.toml`, `web_client/{Cargo.toml,src/lib.rs}`, `web_ui_common/{Cargo.toml,src/lib.rs}`, `web_ui_guest/{Cargo.toml,build.rs,src/main.rs,src/lib.rs}`, `web_ui_admin/{…same}`
  - **Acceptance Criteria:**
    - [ ] `cargo check --workspace` passes; both bins start and answer `GET /` on 3010/3012.
    - [ ] No `daisyui` string in new crates.
  - **Verification Command:** `SQLX_OFFLINE=true cargo check -p web_client -p web_ui_common -p web_ui_guest -p web_ui_admin && cargo test -p web_ui_guest -p web_ui_admin`
  - **Dependencies:** None

- [x] **Task 2: CSP spike — Topcoat runtime under strict `script-src 'self'`**
  - **Scope:** M
  - **Description:** In `web_ui_guest`, add a spike page using one runtime signal and a temporary hand-set CSP header (`default-src 'self'; script-src 'self'; style-src 'self'`). Write a throwaway Playwright spec that collects `securitypolicyviolation` events in Chromium + Firefox. Also check HTMX/Topcoat for inline `style=` output. Record result + decision (keep `.runtime()` or drop per D5) in the spec under "Spike results", then delete the spike page.
  - **Files Touched:** `web_ui_guest/src/pages/spike.rs` (temporary), `playwright/e2e/spike/csp-spike.spec.ts` (temporary), `docs/specs/spec-96-…md`
  - **Acceptance Criteria:**
    - [ ] Spec has a "Spike results" section with violations found (or none) per browser and the D5 outcome.
    - [ ] Spike files removed in the same task.
  - **Verification Command:** `npx playwright test playwright/e2e/spike --project=guest-portal-new-chromium --project=guest-portal-new-firefox` (run before deletion); `rg -n "Spike results" docs/specs/spec-96-*.md`
  - **Dependencies:** Task 1 (needs Task 12's projects; if not yet done, run with an inline `--config` baseURL override)

- [x] **Task 3: Initialise Topcoat UI registry in `web_ui_common`**
  - **Scope:** M
  - **Description:** `topcoat ui init`, then `topcoat ui add` the 29 components listed in spec FR-3.1. Commit `components.toml` with pinned versions; components compile under workspace clippy/doc rules (add `///` docs where the registry lacks them). **Because the Task 2 spike failed:** check every component for runtime usage (`@event`, `$(…)`, `signal`, `#[shard]`) and inline `style=`, and record the list. Fixing them happens when each component is first used.
  - **Files Touched:** `web_ui_common/components.toml`, `web_ui_common/src/components/ui/*.rs`, `web_ui_common/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] `topcoat ui list` shows all 29 installed.
    - [ ] `cargo clippy -p web_ui_common -- -D warnings` clean (crate-level `deny(missing_docs)`).
    - [ ] Audit of runtime usage and inline styles recorded in the spec's Spike results.
  - **Verification Command:** `(cd web_ui_common && topcoat ui list) && SQLX_OFFLINE=true cargo clippy -p web_ui_common --all-features -- -D warnings -D missing_docs`
  - **Dependencies:** Task 1

---

## Phase 1: `web_client` extraction

- [x] **Task 4: Move `error`, `client`, `api_client` into `web_client` with re-exports**
  - **Scope:** M
  - **Description:** Move the three modules verbatim; `web_app_common_tc` re-exports them at identical paths (`web_app_common_tc::{client, api_client, error, AppError, app_error, TopcoatApiClient, get_api_client}`).
  - **Files Touched:** `web_client/src/{client,api_client,error,lib}.rs`, `web_client/Cargo.toml`, `web_app_common_tc/src/{lib,client,api_client,error}.rs`, `web_app_common_tc/Cargo.toml`
  - **Acceptance Criteria:**
    - [ ] Old-app guard passes (zero diff in `web_app_tc`, `web_app_admin_tc`).
    - [ ] Old crates' tests pass unchanged.
  - **Verification Command:** `cargo test -p web_client -p web_app_common_tc -p web_app_tc -p web_app_admin_tc && git diff --quiet b77cfb7a -- web_app_tc web_app_admin_tc`
  - **Dependencies:** Task 1

- [x] **Task 5: Move Rust auth/session logic into `web_client::auth`**
  - **Scope:** S
  - **Description:** Move `AuthUser`, `AdminAuthError`, `token_hash_to_hex`, `get_admin_session`, `get_guest_session`, `require_admin_auth`, `require_admin_role_auth`, `get_authenticated_*` (legacy, unchanged behavior, doc-commented as legacy). `web_app_common_tc::auth` becomes `pub use web_client::auth::*;` + `auth_init_script`.
  - **Files Touched:** `web_client/src/auth.rs`, `web_app_common_tc/src/auth.rs`, `web_app_common_tc/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] Every existing `web_app_common_tc::auth::…` path still resolves.
    - [ ] `AuthUser::initials` tests moved to `web_client`.
  - **Verification Command:** `cargo test -p web_client -p web_app_common_tc -p web_app_tc -p web_app_admin_tc && git diff --quiet b77cfb7a -- web_app_tc web_app_admin_tc`
  - **Dependencies:** Task 4

- [x] **Task 6: Typed sessions and route guards**
  - **Scope:** S
  - **Description:** Add `GuestSession`, `AdminSession` (non-optional `user_id`), `AdminRole`, `GuestAuthError`, and `require_guest` / `require_admin` / `require_admin_role` returning redirect-friendly errors (303 for pages, `HX-Redirect` for HTMX requests).
  - **Files Touched:** `web_client/src/session.rs`, `web_client/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] Session DTO without `user_id` → error, never nil UUID.
    - [ ] Host session → `require_admin_role` returns Forbidden.
  - **Verification Command:** `cargo test -p web_client session`
  - **Dependencies:** Task 5

- [x] **Task 7: `ServiceJwt` with fail-fast secret**
  - **Scope:** S
  - **Description:** `ServiceJwt::from_env()` reads `JWT_SECRET` via `static JWT_SECRET_ENV`; errors if missing/empty. `mint(user_id)` issues 5-minute tokens.
  - **Files Touched:** `web_client/src/jwt.rs`, `web_client/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] Missing secret → `Err`; minted token decodes with correct `sub`, `exp ≤ now+300s`.
    - [ ] No `"secret"` fallback literal anywhere in `web_client`.
  - **Verification Command:** `cargo test -p web_client jwt && ! rg -q '"secret"' web_client/src`
  - **Dependencies:** Task 4

- [x] **Task 8: `safe_redirect` helper**
  - **Scope:** XS
  - **Description:** Same-origin relative path validator per spec FR-1.4.
  - **Files Touched:** `web_client/src/redirect.rs`
  - **Acceptance Criteria:**
    - [ ] Table test: `/bookings` ok; `//x`, `/\x`, `http://x`, `javascript:`, >512 bytes, empty → `/`.
  - **Verification Command:** `cargo test -p web_client redirect`
  - **Dependencies:** Task 1

- [x] **Task 9: JWT-forwarding client methods**
  - **Scope:** S
  - **Description:** Add `get_user_bookings_as(user_id, jwt)`, `get_booking_as(id, jwt)`, `get_cancellation_quote_as(id, jwt)` to `TopcoatApiClient` (new names; existing methods untouched for old crates). Calls set `Authorization: Bearer` and return a typed `UserCallError` (`NotFound` / `Unauthorized` / `Status` / `Transport`).
  - **Files Touched:** `web_client/src/client.rs`, `web_client/tests/client_auth.rs`
  - **Acceptance Criteria:**
    - [ ] `wiremock` tests assert the bearer header and path for each method.
  - **Verification Command:** `cargo test -p web_client --test client_auth`
  - **Dependencies:** Task 4, Task 7

---

## Phase 2: Parity baseline against the OLD apps

- [x] **Task 10: Migrate guest specs off daisyUI selectors and `localStorage`**
  - **Scope:** M
  - **Description:** Replace `.card/.btn/.badge/.alert/.modal/.chat` selectors with roles/labels/IDs/`data-testid` in all 7 guest specs, and update assertions that had drifted from the app ("Villa" → "Listing" copy). The `localStorage` assertion is gated with `isNewPortal()` (`fixtures/portal.ts`) and paired with a UI assertion.
  - **Files Touched:** `playwright/e2e/guest-portal/*.spec.ts`, `playwright/e2e/fixtures/auth.fixture.ts`
  - **Acceptance Criteria:**
    - [ ] `rg '\.(card|btn|badge|alert|modal|chat)\b' playwright/e2e/guest-portal` returns nothing.
    - [ ] OLD_GUEST green.
  - **Verification Command:** `npx playwright test --project=guest-portal-chromium --project=guest-portal-firefox`
  - **Dependencies:** None

- [x] **Task 11: Migrate admin specs off daisyUI selectors**
  - **Scope:** M
  - **Description:** Same as Task 10 for the 6 admin specs. Also fix drifted IDs and copy (`#admin-auth-error`, "Listings Management", "Active Listings"), and tag the 4 tests that need the broken legacy booking-details dialog `@new-only` (inventory A20). Verified with `--grep-invert @new-only` until Task 12 adds the per-project filters.
  - **Files Touched:** `playwright/e2e/admin-portal/*.spec.ts`
  - **Acceptance Criteria:**
    - [ ] No daisyUI class selectors in `playwright/e2e/admin-portal`.
    - [ ] OLD_ADMIN green.
  - **Verification Command:** `npx playwright test --project=admin-portal-chromium --project=admin-portal-firefox`
  - **Dependencies:** None

- [x] **Task 12: New-port projects, tag filters and CSP fixture**
  - **Scope:** S
  - **Description:** Add `guest-portal-new-*` (:3010) and `admin-portal-new-*` (:3012) projects reusing the same `testDir`s; `grepInvert` `@old-only` / `@new-only` per side; `webServer` entries for the new bins; CSP-violation fixture (file-based init script) auto-used by `-new-` projects.
  - **Files Touched:** `playwright/playwright.config.ts`, `playwright/e2e/fixtures/base.fixture.ts` (auto `cspGuard` fixture; all portal specs now import `test` from it or from `auth.fixture`), `playwright/e2e/fixtures/csp-listener.js`
  - **Acceptance Criteria:**
    - [ ] `npx playwright test --list` shows 8 portal projects.
    - [ ] A deliberate inline-script test page fails the fixture (verified once, not committed).
  - **Verification Command:** `npx playwright test --list | grep -c "portal-new"`
  - **Dependencies:** Task 1

- [x] **Task 13: Hold-expiry seed helper**
  - **Scope:** XS
  - **Description:** `expirePendingHold(bookingId)` in db-seed, running the cron's UPDATE + `booking_history` insert for one booking; SQL constant commented with the source migration (`20260927000000`).
  - **Files Touched:** `playwright/e2e/setup/db-seed.ts`, `playwright/e2e/api/hold-expiry.spec.ts`
  - **Acceptance Criteria:**
    - [ ] Helper flips a seeded `pending` booking to `cancelled` with history reason "Hold expired after 2 hours".
  - **Verification Command:** `npx playwright test --project=backend-api hold-expiry`
  - **Dependencies:** None

- [x] **Task 14: Old-app specs — bookings & reservation changes**
  - **Scope:** M
  - **Description:** New `bookings.spec.ts` and `reservation-changes.spec.ts` covering G14–G23 against :3000. Rows marked `fixed` in the inventory (G14 scope, G16) are `@new-only`.
  - **Files Touched:** `playwright/e2e/guest-portal/{bookings,reservation-changes}.spec.ts`, `playwright/e2e/fixtures/bookings-data.ts` (per-test guests and bookings, test JWTs)
  - **Acceptance Criteria:**
    - [ ] OLD_GUEST green for non-`@new-only` tests.
  - **Verification Command:** `npx playwright test guest-portal/bookings guest-portal/reservation-changes --project=guest-portal-chromium --project=guest-portal-firefox`
  - **Dependencies:** Task 10, Task 12

- [x] **Task 15: Old-app specs — messaging, static pages, profile, reviews**
  - **Scope:** M
  - **Description:** `messaging.spec.ts` (G24–G25), `static-pages.spec.ts` (G3), and `@new-only` skeletons for `profile.spec.ts` (G32) and `reviews.spec.ts` (G33–G34; G34 runs on old too).
  - **Files Touched:** `playwright/e2e/guest-portal/{messaging,static-pages,profile,reviews}.spec.ts`
  - **Acceptance Criteria:**
    - [ ] OLD_GUEST green for non-`@new-only` tests.
  - **Verification Command:** `npx playwright test guest-portal/messaging guest-portal/static-pages guest-portal/reviews --project=guest-portal-chromium`
  - **Dependencies:** Task 10, Task 12

- [x] **Task 16: Old-app specs — host signup, users admin**
  - **Scope:** M
  - **Description:** `host-signup.spec.ts` (A4–A7), `users-admin.spec.ts` (A29–A31, admin role vs host 403).
  - **Files Touched:** `playwright/e2e/admin-portal/{host-signup,users-admin}.spec.ts`, `playwright/e2e/fixtures/auth.fixture.ts`
  - **Acceptance Criteria:**
    - [ ] OLD_ADMIN green.
  - **Verification Command:** `npx playwright test admin-portal/host-signup admin-portal/users-admin --project=admin-portal-chromium --project=admin-portal-firefox`
  - **Dependencies:** Task 11, Task 12

- [x] **Task 17: Old-app specs — pricing, exchange rates, admin messages, admin change requests, listing images**
  - **Scope:** M
  - **Description:** `pricing-admin`, `exchange-rates`, `booking-messages-admin`, `reservation-changes-admin` specs (A16–A18, A22–A25, A28). `payouts-admin.spec.ts` gains `@new-only` host-read-only tests (D10). (`listing-images` dropped: inventory A15 is out of scope, since no legacy UI uploads images.)
  - **Files Touched:** `playwright/e2e/admin-portal/{pricing-admin,exchange-rates,booking-messages-admin,reservation-changes-admin,payouts-admin}.spec.ts`
  - **Acceptance Criteria:**
    - [ ] OLD_ADMIN green for non-`@new-only` tests.
    - [x] Every inventory row's "Spec file" exists (except `calendar.spec.ts`, which Task 31 adds).
  - **Verification Command:** `npx playwright test --project=admin-portal-chromium --project=admin-portal-firefox`
  - **Dependencies:** Task 11, Task 12

---

## Phase 3: `web_ui_common`

- [x] **Task 18: Theme stylesheets**
  - **Scope:** S
  - **Description:** `guest.css` / `admin.css` with exact tokens from `guest-theme.md` §1/§6 (light, `.dim`, `.dark`), `@custom-variant dark`, Newsreader + Inter via `topcoat-font`; `build.rs` in each bin compiles its theme.
  - **Files Touched:** `web_ui_common/src/style/{base,guest,admin}.css`, `web_ui_{guest,admin}/src/style/tailwind.css` (entry points), `web_ui_common/tests/theme_tokens.rs` (diffs guest tokens against the design doc). Font files are wired in Task 26 with the shells.
  - **Acceptance Criteria:**
    - [ ] Token values match the design doc (scripted diff of `--*` declarations).
    - [ ] No raw hex outside the two theme files: `rg '#[0-9A-Fa-f]{6}' web_ui_*/src --glob '!**/style/*.css'` empty.
  - **Verification Command:** `SQLX_OFFLINE=true cargo build -p web_ui_guest -p web_ui_admin && ! rg -q '#[0-9A-Fa-f]{6}' web_ui_common/src/components web_ui_guest/src web_ui_admin/src`
  - **Dependencies:** Task 3

- [x] **Task 19: Theme resolution engine**
  - **Scope:** S
  - **Description:** `ThemeChoice`, cookie parsing (`op_theme`, `op_tz_offset` validation), `resolve_theme(…, now_utc)` with Jamaica UTC−05:00 fallback; `html_class()`.
  - **Files Touched:** `web_ui_common/src/theme.rs`
  - **Acceptance Criteria:**
    - [ ] Matrix tests: each cookie value, offsets −840/0/840/invalid, local 17:59/18:00/05:59/06:00.
  - **Verification Command:** `cargo test -p web_ui_common theme`
  - **Dependencies:** Task 1

- [x] **Task 20: Security headers and CSRF origin check**
  - **Scope:** M
  - **Description:** `SecurityHeaders` router layer: CSP (one policy for both portals, since A15 was dropped), hardening headers (including on error pages), and `Cache-Control: private, no-cache` plus `Vary: Cookie` on HTML; HSTS is opt-in. CSRF needs no custom code: Topcoat's default `OriginPolicy` already rejects cross-origin state-changing requests, and a test pins that.
  - **Files Touched:** `web_ui_common/src/security_headers.rs`, `web_ui_common/src/lib.rs`, `web_ui_common/tests/headers.rs`
  - **Acceptance Criteria:**
    - [ ] Exact CSP strings asserted per app; cross-origin POST → 403; same-origin POST passes.
  - **Verification Command:** `cargo test -p web_ui_common --test headers`
  - **Dependencies:** Task 2

- [x] **Task 21: `ui.js` core behaviors**
  - **Scope:** M
  - **Description:** Hashed asset with dialog open/close, outside-click dismiss, theme cycle + `op_theme` cookie + `aria-label`, `op_tz_offset` writer, `data-print`, HTMX toast glue; vendored `htmx.min.js`; `<meta name="htmx-config">` helper. ≤15 KB minified.
  - **Files Touched:** `web_ui_common/src/assets/{ui.js,htmx-2.0.4.min.js}`, `web_ui_common/src/assets.rs`, `playwright/e2e/ui-js/ui-js.spec.ts` (+ `ui-js-*` projects). HTMX 2.0.4 matches the CDN version the legacy portals actually run (the repo's unused vendored file is 4.0.0); verified against the npm tarball. **Bins (Tasks 32/44) must fail startup if the asset bundle is missing:** rendering an `Asset` without one panics.
  - **Acceptance Criteria:**
    - [ ] Theme toggle cycles and persists (theme.spec on NEW_GUEST once Task 26 lands; unit-level check via a fixture page now).
    - [ ] No `eval`/`new Function`/inline handlers in `ui.js`.
  - **Verification Command:** `! rg -q 'eval\(|new Function' web_ui_common/src/assets/ui.js && wc -c web_ui_common/src/assets/ui.js`
  - **Dependencies:** Task 2, Task 19

- [x] **Task 22: Custom presentational components**
  - **Scope:** M
  - **Description:** `toast`, `steps`, `stat_tile`, `framed_image`, `hero` following Topcoat UI conventions (token-only colors, `attrs` passthrough).
  - **Files Touched:** `web_ui_common/src/components/{toast,steps,stat_tile,framed_image,hero}.rs`, `web_ui_common/src/components/mod.rs`
  - **Acceptance Criteria:**
    - [ ] Render tests snapshot ARIA attributes (`role="status"` for toast, `aria-current="step"` for steps).
  - **Verification Command:** `cargo test -p web_ui_common components`
  - **Dependencies:** Task 3

- [x] **Task 23: Star rating and chat components**
  - **Scope:** S
  - **Description:** `star_rating` (display), `star_rating_input` (radio group, labelled), `chat_thread` / `chat_bubble` (keeps `#messages-list` id hook via attrs).
  - **Files Touched:** `web_ui_common/src/components/{star_rating,chat}.rs`
  - **Acceptance Criteria:**
    - [ ] Input renders 5 labelled radios per category; bubble escapes message text.
  - **Verification Command:** `cargo test -p web_ui_common star_rating chat`
  - **Dependencies:** Task 3

- [x] **Task 24: Domain components — money**
  - **Scope:** M
  - **Description:** Port `currency_selector` and `price_breakdown` to Topcoat UI. Add parity tests comparing formatted numbers/currency codes with the old `web_app_common_tc` components for USD/JMD/EUR/GBP/CAD.
  - **Files Touched:** `web_ui_common/src/components/domain/{currency_selector,price_breakdown,mod}.rs`, `web_ui_common/tests/money_parity.rs`, `web_ui_common/Cargo.toml` (dev-dep on `web_app_common_tc`)
  - **Acceptance Criteria:**
    - [ ] Extracted amount strings identical to old output for fixed `Decimal` inputs.
  - **Verification Command:** `cargo test -p web_ui_common --test money_parity`
  - **Dependencies:** Task 3

- [x] **Task 25: Domain components — listing card, image, tax document**
  - **Scope:** M
  - **Description:** Port `listing_card` (portrait), `responsive_image` (`<picture>`/`srcset` kept), `tax_document_view`.
  - **Files Touched:** `web_ui_common/src/components/domain/{listing_card,responsive_image,tax_document_view}.rs`
  - **Acceptance Criteria:**
    - [ ] `srcset` output equals old component for the same input; tax document totals identical.
  - **Verification Command:** `cargo test -p web_ui_common domain`
  - **Dependencies:** Task 24

- [x] **Task 26: Navbar and shells**
  - **Scope:** M
  - **Description:** `navbar`, `guest_shell` (server-rendered auth state, mobile `sheet`, footer, theme button, `<html class>` from Task 19) and `admin_shell` (sidebar; user-management links only for `AdminRole::Admin`).
  - **Files Touched:** `web_ui_common/src/components/navbar.rs`, `web_ui_common/src/{shell_guest,shell_admin}.rs`, `web_ui_common/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] Logged-out render contains no `nav-user-dropdown`; host admin shell contains no user-management links.
  - **Verification Command:** `cargo test -p web_ui_common shell`
  - **Dependencies:** Task 6, Task 19, Task 21, Task 22

### Phase 3b: Calendar availability (the one backend change)

- [x] **Task 27: `UnavailableRange` model and `merge_ranges`**
  - **Scope:** S
  - **Description:** Documented DTOs in `common` and pure `merge_ranges`.
  - **Files Touched:** `common/src/availability.rs` (DTOs and `merge_ranges` live together), `common/src/lib.rs`, `common/tests/availability.rs`
  - **Acceptance Criteria:**
    - [ ] Tests: empty, disjoint, touching, nested, unsorted.
  - **Verification Command:** `cargo test -p common availability`
  - **Dependencies:** None

- [x] **Task 28: `db_core::booking::get_unavailable_ranges`**
  - **Scope:** S
  - **Description:** Parameterized query (spec §Data Models) and `.sqlx` metadata.
  - **Files Touched:** `db_core/src/booking.rs`, `.sqlx/*.json`
  - **Acceptance Criteria:**
    - [ ] Returns only `pending`/`confirmed` overlapping rows; `cargo sqlx prepare --workspace --check` passes.
  - **Verification Command:** `cargo test -p db_core unavailable_ranges && cargo sqlx prepare --workspace --check -- --all-targets`
  - **Dependencies:** Task 27

- [x] **Task 29: `booking_api` `GET /bookings/listing/{id}/unavailable-ranges`**
  - **Scope:** S
  - **Description:** Public handler with 366-day span limit, 404 unknown listing, utoipa annotations; add the route and schemas to the API Gateway specs by hand (there is no generator).
  - **Files Touched:** `app_api/booking_api/src/apis.rs`, `app_api/booking_api/src/apis_test.rs`, `openapi.yaml`, `openapi-prod.yaml`
  - **Acceptance Criteria:**
    - [ ] Tests: ranges returned, no guest fields in JSON, 400 span/order, 404 listing.
  - **Verification Command:** `cargo test -p booking_api unavailable_ranges`
  - **Dependencies:** Task 28

- [x] **Task 30: `date_range_calendar` server component**
  - **Scope:** M
  - **Description:** Two-month `role="grid"` render (one month on small screens), day buttons with `aria-label`/`aria-disabled`, HTMX prev/next, hidden `check_in`/`check_out`, progressive fallback to native date inputs; client method `get_unavailable_ranges`.
  - **Files Touched:** `web_ui_common/src/components/date_range_calendar.rs`, `web_client/src/client.rs`
  - **Acceptance Criteria:**
    - [ ] Tests: leap Feb 2028, Sunday-start month, blocked/past flags, label text.
  - **Verification Command:** `cargo test -p web_ui_common date_range_calendar`
  - **Dependencies:** Task 3, Task 9, Task 29

- [x] **Task 31: Calendar interaction in `ui.js`**
  - **Scope:** S
  - **Description:** Click + Arrow/Home/End/PageUp/PageDown, range highlight, reject ranges crossing blocked nights, fire `change` on `#booking-dates`.
  - **Files Touched:** `web_ui_common/src/assets/ui.js`, `playwright/e2e/ui-js/calendar.spec.ts` (behaviour, against `fixtures/calendar.html`, which a Rust test keeps identical to the component's output), `playwright/e2e/guest-portal/calendar.spec.ts` (`@new-only` portal contract; it runs once Task 35 adds the listing page)
  - **Acceptance Criteria:**
    - [x] Keyboard-only range selection works in Chromium + Firefox (ui-js projects).
    - [ ] Portal calendar spec green on NEW_GUEST (verified in Task 35).
  - **Verification Command:** `npx playwright test --project=ui-js-chromium --project=ui-js-firefox`
  - **Dependencies:** Task 30, Task 21

---

## Phase 4: `web_ui_guest` (:3010)

- [ ] **Task 32: Guest app bootstrap and home page**
  - **Scope:** M
  - **Description:** `main.rs` (session store `op_guest_session`, `ServiceJwt::from_env` fail-fast, security middleware, `.runtime()` per Task 2), `#[layout("/")]` using `guest_shell`, `/` with hero + search.
  - **Files Touched:** `web_ui_guest/src/{main,lib,layout}.rs`, `web_ui_guest/src/pages/{mod,home}.rs`
  - **Acceptance Criteria:**
    - [ ] `home.spec.ts` green on NEW_GUEST with zero CSP violations.
    - [ ] App exits non-zero without `JWT_SECRET`.
  - **Verification Command:** `npx playwright test guest-portal/home --project=guest-portal-new-chromium --project=guest-portal-new-firefox`
  - **Dependencies:** Task 7, Task 12, Task 20, Task 26

- [ ] **Task 33: Guest login and logout (server sessions)**
  - **Scope:** M
  - **Description:** `GET/POST /login` (fixation-safe), `GET/POST /logout`, `safe_redirect`, generic error messages, server-rendered nav. Also add the shared mapping from `web_client::session::GuardError` to responses: `Unauthenticated` → 303 to `login_redirect(current path)` (or `HX-Redirect` for HTMX requests), `Forbidden` → 403, `Service` → 503 error page. Task 6 only provides the guards and `login_redirect`.
  - **Files Touched:** `web_ui_guest/src/pages/auth.rs`, `web_ui_guest/tests/auth.rs`
  - **Acceptance Criteria:**
    - [ ] `auth.spec.ts` green on NEW_GUEST; no auth keys in browser storage; `redirect=//evil.com` → `/`.
    - [ ] Integration test: `create_session` called with `namespace="guest"`.
  - **Verification Command:** `cargo test -p web_ui_guest --test auth && npx playwright test guest-portal/auth --project=guest-portal-new-chromium --project=guest-portal-new-firefox`
  - **Dependencies:** Task 8, Task 32

- [ ] **Task 34: Guest registration and email verification**
  - **Scope:** M
  - **Description:** `GET/POST /register`, `GET/POST /verify`, `POST /verify/resend`, all proxied server-side to `user_api`.
  - **Files Touched:** `web_ui_guest/src/pages/register.rs`, `web_ui_guest/src/pages/verify.rs`
  - **Acceptance Criteria:**
    - [ ] `email-verification-flow.spec.ts` green on NEW_GUEST; no browser request to `:8083` (asserted via `page.on('request')`).
  - **Verification Command:** `npx playwright test guest-portal/email-verification-flow --project=guest-portal-new-chromium --project=guest-portal-new-firefox`
  - **Dependencies:** Task 33

- [ ] **Task 35: Listings search and listing detail with calendar**
  - **Scope:** M
  - **Description:** `/listings`, `/listings/{slug}` (gallery dialog, amenities, reviews, concurrent fetches via `try_join!`), `/listings/{slug}/quote`, `/listings/{slug}/calendar` fragments.
  - **Files Touched:** `web_ui_guest/src/pages/{listings,listing_detail}.rs`
  - **Acceptance Criteria:**
    - [ ] `listings`, `listing-detail`, `calendar` specs green on NEW_GUEST.
  - **Verification Command:** `npx playwright test guest-portal/listings guest-portal/listing-detail guest-portal/calendar --project=guest-portal-new-chromium --project=guest-portal-new-firefox`
  - **Dependencies:** Task 25, Task 31, Task 32

- [ ] **Task 36: Checkout (login required, real booking)**
  - **Scope:** M
  - **Description:** `GET/POST /checkout/{slug}` with server validation, `guest_id` from session, conflict re-render, session-expiry `HX-Redirect` preserving form state, legacy `/checkout-{ccy}/{id}` and `-ccy` suffix 308s.
  - **Files Touched:** `web_ui_guest/src/pages/checkout.rs`, `web_ui_guest/tests/checkout.rs`, `playwright/e2e/guest-portal/checkout-flow.spec.ts`
  - **Acceptance Criteria:**
    - [ ] Integration test: posted `guest_id` ignored; session user used.
    - [ ] E2E: logged-out → login → back to checkout; booking created with selected dates; session-expiry resume; `expirePendingHold` → "hold expired" restart.
  - **Verification Command:** `cargo test -p web_ui_guest --test checkout && npx playwright test guest-portal/checkout-flow --project=guest-portal-new-chromium --project=guest-portal-new-firefox`
  - **Dependencies:** Task 13, Task 33, Task 35

- [ ] **Task 37: My bookings, cancel and cancellation quote**
  - **Scope:** M
  - **Description:** `/bookings` (own bookings only, via `get_user_bookings_as(session.user_id, jwt)`), `POST /bookings/{id}/cancel`, `GET /bookings/{id}/cancellation-quote` with ownership 404 (D8, D9).
  - **Files Touched:** `web_ui_guest/src/pages/bookings.rs`, `web_ui_guest/src/ownership.rs`, `web_ui_guest/tests/bookings.rs`
  - **Acceptance Criteria:**
    - [ ] `bookings.spec.ts` green on NEW_GUEST incl. guest A cannot see/quote guest B's booking.
    - [ ] `! rg -q get_all_bookings web_ui_guest`.
  - **Verification Command:** `cargo test -p web_ui_guest --test bookings && npx playwright test guest-portal/bookings --project=guest-portal-new-chromium --project=guest-portal-new-firefox`
  - **Dependencies:** Task 9, Task 33

- [ ] **Task 38: Change dates and change requests**
  - **Scope:** M
  - **Description:** `/bookings/{id}/change-dates`, change-quote, create, approve/decline/cancel fragments, all ownership-checked.
  - **Files Touched:** `web_ui_guest/src/pages/booking_change.rs`
  - **Acceptance Criteria:**
    - [ ] `reservation-changes.spec.ts` green on NEW_GUEST.
  - **Verification Command:** `npx playwright test guest-portal/reservation-changes --project=guest-portal-new-chromium --project=guest-portal-new-firefox`
  - **Dependencies:** Task 30, Task 37

- [ ] **Task 39: Guest messaging**
  - **Scope:** S
  - **Description:** `GET/POST /bookings/{code}/messages`, code resolved within own bookings, `chat_thread`, `beforeend` swap into `#messages-list`.
  - **Files Touched:** `web_ui_guest/src/pages/messages.rs`
  - **Acceptance Criteria:**
    - [ ] `messaging.spec.ts` green on NEW_GUEST incl. cross-account 404.
  - **Verification Command:** `npx playwright test guest-portal/messaging --project=guest-portal-new-chromium --project=guest-portal-new-firefox`
  - **Dependencies:** Task 23, Task 37

- [ ] **Task 40: Tax documents**
  - **Scope:** S
  - **Description:** `/bookings/{id}/documents/{number}` with ownership check and `data-print`.
  - **Files Touched:** `web_ui_guest/src/pages/documents.rs`
  - **Acceptance Criteria:**
    - [ ] `tax-documents.spec.ts` green on NEW_GUEST.
  - **Verification Command:** `npx playwright test guest-portal/tax-documents --project=guest-portal-new-chromium --project=guest-portal-new-firefox`
  - **Dependencies:** Task 25, Task 37

- [ ] **Task 41: Profile**
  - **Scope:** M
  - **Description:** `GET/POST /profile` with real `user_api` data; email change and password reset flows using existing endpoints.
  - **Files Touched:** `web_ui_guest/src/pages/profile.rs`, `web_client/src/client.rs`, `playwright/e2e/guest-portal/profile.spec.ts`
  - **Acceptance Criteria:**
    - [ ] `profile.spec.ts` green on NEW_GUEST (name update persists after reload).
  - **Verification Command:** `npx playwright test guest-portal/profile --project=guest-portal-new-chromium --project=guest-portal-new-firefox`
  - **Dependencies:** Task 33

- [ ] **Task 42: Reviews (token flow) and About page**
  - **Scope:** S
  - **Description:** `GET/POST /reviews/submit?token=`, `/reviews/success`, `/about`.
  - **Files Touched:** `web_ui_guest/src/pages/{reviews,about}.rs`, `playwright/e2e/guest-portal/reviews.spec.ts`
  - **Acceptance Criteria:**
    - [ ] `reviews` and `static-pages` specs green on NEW_GUEST; invalid token → 404 page.
  - **Verification Command:** `npx playwright test guest-portal/reviews guest-portal/static-pages --project=guest-portal-new-chromium --project=guest-portal-new-firefox`
  - **Dependencies:** Task 23, Task 32

- [ ] **Task 43: Guest security and theme e2e sweep**
  - **Scope:** S
  - **Description:** `@new-only` specs: raw-HTML no-user-nav for logged-out, storage has no auth keys, cross-account matrix, theme first-visit (mocked clock) / `op_tz_offset` / persistence / no class change after first paint.
  - **Files Touched:** `playwright/e2e/guest-portal/{security,theme}.spec.ts`
  - **Acceptance Criteria:**
    - [ ] All green on NEW_GUEST with zero CSP violations.
  - **Verification Command:** `npx playwright test guest-portal/security guest-portal/theme --project=guest-portal-new-chromium --project=guest-portal-new-firefox`
  - **Dependencies:** Tasks 33–42

---

## Phase 5: `web_ui_admin` (:3012)

- [ ] **Task 44: Admin app bootstrap, login/logout, dashboard, alias redirects**
  - **Scope:** M
  - **Description:** `main.rs` (`op_admin_session`), layout with `admin_shell`, `GET/POST /login`, `GET/POST /logout`, `/admin` dashboard with `stat_tile`s, `/` and unprefixed aliases → 308.
  - **Files Touched:** `web_ui_admin/src/{main,lib,layout}.rs`, `web_ui_admin/src/pages/{mod,auth,dashboard,aliases}.rs`
  - **Acceptance Criteria:**
    - [ ] `auth` and `dashboard` specs green on NEW_ADMIN; aliases preserve method (POST stays POST).
  - **Verification Command:** `npx playwright test admin-portal/auth admin-portal/dashboard --project=admin-portal-new-chromium --project=admin-portal-new-firefox`
  - **Dependencies:** Task 6, Task 7, Task 12, Task 20, Task 26

- [ ] **Task 45: Host sign-up and verification**
  - **Scope:** S
  - **Description:** `GET/POST /register`, `GET/POST /verify`, `POST /verify/resend` as form posts.
  - **Files Touched:** `web_ui_admin/src/pages/register.rs`
  - **Acceptance Criteria:**
    - [ ] `host-signup.spec.ts` green on NEW_ADMIN.
  - **Verification Command:** `npx playwright test admin-portal/host-signup --project=admin-portal-new-chromium --project=admin-portal-new-firefox`
  - **Dependencies:** Task 44

- [ ] **Task 46: Listings list, detail, new, clone, create**
  - **Scope:** M
  - **Description:** A9–A12, A14 with dense `table` + `pagination`.
  - **Files Touched:** `web_ui_admin/src/pages/listings/{mod,list,detail,create}.rs`
  - **Acceptance Criteria:**
    - [ ] `listings-admin.spec.ts` (non-edit tests) green on NEW_ADMIN.
  - **Verification Command:** `npx playwright test admin-portal/listings-admin --project=admin-portal-new-chromium --project=admin-portal-new-firefox`
  - **Dependencies:** Task 44

- [ ] **Task 47: Listing edit studio (fees, documents)**
  - **Scope:** M
  - **Description:** A13 in `tabs`, fees and documents sections.
  - **Files Touched:** `web_ui_admin/src/pages/listings/edit.rs`
  - **Acceptance Criteria:**
    - [ ] `listings-admin` + `listing-fees-and-documents` specs green on NEW_ADMIN.
  - **Verification Command:** `npx playwright test admin-portal/listings-admin admin-portal/listing-fees-and-documents --project=admin-portal-new-chromium --project=admin-portal-new-firefox`
  - **Dependencies:** Task 46

- [x] **Task 48: ~~Listing image upload via signed URL~~** (dropped: inventory A15 is out of scope, since no legacy admin page uploads images.)

- [ ] **Task 49: Seasonal pricing overrides**
  - **Scope:** S
  - **Description:** A16–A18 with `hx-post` → `#price-overrides-container` `outerHTML`.
  - **Files Touched:** `web_ui_admin/src/pages/pricing.rs`
  - **Acceptance Criteria:**
    - [ ] `pricing-admin.spec.ts` green on NEW_ADMIN.
  - **Verification Command:** `npx playwright test admin-portal/pricing-admin --project=admin-portal-new-chromium --project=admin-portal-new-firefox`
  - **Dependencies:** Task 46

- [ ] **Task 50: Admin bookings list and detail dialog**
  - **Scope:** M
  - **Description:** A19–A21, A24, A24b: list, `hx-get` detail dialog, status + door-code forms, documents, cancellation quote (admin role only; hosts 403, control not rendered).
  - **Files Touched:** `web_ui_admin/src/pages/bookings/{mod,list,detail,actions}.rs`
  - **Acceptance Criteria:**
    - [ ] `bookings-admin.spec.ts` green on NEW_ADMIN incl. host-403 quote test.
  - **Verification Command:** `npx playwright test admin-portal/bookings-admin --project=admin-portal-new-chromium --project=admin-portal-new-firefox`
  - **Dependencies:** Task 44

- [ ] **Task 51: Admin booking messages and change requests**
  - **Scope:** M
  - **Description:** A22, A23, A25.
  - **Files Touched:** `web_ui_admin/src/pages/bookings/{messages,changes}.rs`
  - **Acceptance Criteria:**
    - [ ] `booking-messages-admin` and `reservation-changes-admin` specs green on NEW_ADMIN.
  - **Verification Command:** `npx playwright test admin-portal/booking-messages-admin admin-portal/reservation-changes-admin --project=admin-portal-new-chromium --project=admin-portal-new-firefox`
  - **Dependencies:** Task 23, Task 50

- [ ] **Task 52: Payouts (admin-only status override)**
  - **Scope:** S
  - **Description:** A26–A27: ledger for all; status form only for admin role with required reason; host POST → 403 (D10).
  - **Files Touched:** `web_ui_admin/src/pages/payouts.rs`
  - **Acceptance Criteria:**
    - [ ] `payouts-admin.spec.ts` green on NEW_ADMIN incl. host read-only + forged POST 403.
  - **Verification Command:** `npx playwright test admin-portal/payouts-admin --project=admin-portal-new-chromium --project=admin-portal-new-firefox`
  - **Dependencies:** Task 44

- [ ] **Task 53: Exchange rates**
  - **Scope:** XS
  - **Description:** A28 read-only table.
  - **Files Touched:** `web_ui_admin/src/pages/exchange_rates.rs`
  - **Acceptance Criteria:**
    - [ ] `exchange-rates.spec.ts` green on NEW_ADMIN.
  - **Verification Command:** `npx playwright test admin-portal/exchange-rates --project=admin-portal-new-chromium --project=admin-portal-new-firefox`
  - **Dependencies:** Task 44

- [ ] **Task 54: User administration**
  - **Scope:** M
  - **Description:** A29–A31: list, new, create, update, delete, restore; `require_admin_role` everywhere; destructive actions behind `alert_dialog`.
  - **Files Touched:** `web_ui_admin/src/pages/users/{mod,list,form,actions}.rs`
  - **Acceptance Criteria:**
    - [ ] `users-admin.spec.ts` green on NEW_ADMIN incl. host 403.
  - **Verification Command:** `npx playwright test admin-portal/users-admin --project=admin-portal-new-chromium --project=admin-portal-new-firefox`
  - **Dependencies:** Task 44

---

## Phase 6: Deployment, gates and parity sign-off

- [ ] **Task 55: Dockerfiles and compose services**
  - **Scope:** S
  - **Description:** `Dockerfile.web_ui_guest`, `Dockerfile.web_ui_admin`, compose services on 3010/3012 with `JWT_SECRET` env.
  - **Files Touched:** `web_ui_guest/Dockerfile.web_ui_guest`, `web_ui_admin/Dockerfile.web_ui_admin`, `docker-compose.yml`
  - **Acceptance Criteria:**
    - [ ] Both images build; `docker compose up web_ui_guest web_ui_admin` serves `/` with CSP header.
  - **Verification Command:** `docker compose build web_ui_guest web_ui_admin && docker compose up -d web_ui_guest web_ui_admin && curl -sI localhost:3010 | grep -i content-security-policy`
  - **Dependencies:** Task 32, Task 44

- [ ] **Task 56: Terraform Cloud Run services**
  - **Scope:** S
  - **Description:** `web-ui-guest` / `web-ui-admin` services (256 MiB, min 0, secret-mounted `JWT_SECRET`), no domain mapping.
  - **Files Touched:** `infra/modules/app/*.tf`, `infra/environments.tf`, `infra/variables.tf`
  - **Acceptance Criteria:**
    - [ ] `terraform validate` passes; plan shows no domain-mapping changes.
  - **Verification Command:** `cd infra && terraform fmt -check -recursive && terraform validate`
  - **Dependencies:** Task 55

- [ ] **Task 57: Tighten `CONSTRAINTS.md` gates**
  - **Scope:** S
  - **Description:** Panic gate over all Rust crates' production code (replacing `web_app/`), `missing_docs` covering `web_client` / `web_ui_*`, new test rows, `rg` gates for daisyUI/inline JS/legacy auth helpers. **Separate commit (decided 2026-10-28: the hold lasts 2 hours):** change the hold time from 15 minutes to 2 hours in `CONSTRAINTS.md` §3, the #96 intent, `docs/ARCHITECTURE.md`, and the skills that state it (`audit-booking-flow`, `build`, `plan-eng-review`, `security-and-hardening`, `security-review`, `sonarqube-guard`, `resilience-exploration`). Wording only.
  - **Files Touched:** `CONSTRAINTS.md`, `docs/ARCHITECTURE.md`, `docs/intent/intent-96-…md`, `.claude/skills/{audit-booking-flow,build,plan-eng-review,security-and-hardening}/SKILL.md`, `.claude/skills/{security-review,sonarqube-guard,resilience-exploration}/REFERENCE.md`
  - **Acceptance Criteria:**
    - [ ] Every new gate command passes on the branch, including over `app_api/ common/ db_core/`.
    - [ ] `rg -i '15[- ]minute' CONSTRAINTS.md docs/ARCHITECTURE.md .claude/skills` returns no matches about the booking hold.
  - **Verification Command:** `! rg -q "\.(unwrap|expect)\(" web_client/src web_ui_common/src web_ui_guest/src web_ui_admin/src web_app_tc/src web_app_admin_tc/src web_app_common_tc/src && SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features -- -D warnings -D missing_docs`
  - **Dependencies:** Tasks 43, 54

- [ ] **Task 58: Accessibility sweep**
  - **Scope:** S
  - **Description:** `@axe-core/playwright` spec visiting every page in both new apps × light/dim/dark; fix contrast/label issues found.
  - **Files Touched:** `playwright/e2e/a11y/a11y.spec.ts`, `playwright/playwright.config.ts`, `package.json`
  - **Acceptance Criteria:**
    - [ ] Zero serious/critical violations across 6 theme combinations.
  - **Verification Command:** `npx playwright test playwright/e2e/a11y`
  - **Dependencies:** Tasks 43, 54

- [ ] **Task 59: Parity gate, inventory sign-off and perf audit**
  - **Scope:** S
  - **Description:** Run the full parity command from the spec; update every inventory row's status; run `/cloudrun-perf-audit` on both new services; tick spec acceptance criteria.
  - **Files Touched:** `docs/specs/spec-96-feat-redesigned-topcoat-ui-portals-daisyui-topcoat-ui.md`
  - **Acceptance Criteria:**
    - [ ] All 8 portal projects green; zero CSP violations; cold start < 300 ms p50, RSS < 256 MiB.
    - [ ] `git diff --quiet b77cfb7a -- web_app_tc/src web_app_admin_tc/src`.
  - **Verification Command:** `npx playwright test --project=guest-portal-chromium --project=guest-portal-firefox --project=admin-portal-chromium --project=admin-portal-firefox --project=guest-portal-new-chromium --project=guest-portal-new-firefox --project=admin-portal-new-chromium --project=admin-portal-new-firefox`
  - **Dependencies:** Tasks 55–58

---

## Risks scheduled early
| Risk | Mitigation task |
|---|---|
| Topcoat runtime violates strict CSP | Task 2 (first after scaffold); D5 fallback pre-approved |
| Topcoat UI registry immaturity | Task 3 early; components vendored and pinned |
| Extraction breaks old crates | Tasks 4–5 gated by zero-diff + old test suites |
| Calendar needs backend change | Tasks 27–29 independent of UI, can start day one |
| Old-app parity tests reveal broken flows | Phase 2 runs before any new page; broken rows re-tagged `@new-only` and noted in the inventory |

## Owner decisions
- Hold TTL: the **2 hours** rule now in production is correct (2026-10-28). Docs are aligned in Task 57. No backend change.
