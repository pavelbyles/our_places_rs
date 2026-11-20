# Implementation Plan: Admin Preview Listing (#128)

**Spec Reference:** [`docs/specs/spec-128-feat-admin-preview-listing.md`](../specs/spec-128-feat-admin-preview-listing.md)
**Baseline Commit:** `4a1250ad`
**Branch:** `feat/128-admin-preview-listing`

## Cross-Cutting Rules (Apply to Every Task)
- **Task Gates (`CONSTRAINTS.md`):** After each task run `cargo fmt --check`, `SQLX_OFFLINE=true cargo clippy -p <touched crates> --all-targets --all-features -- -D warnings`, and `SQLX_OFFLINE=true cargo clippy -p <touched crates> --all-features -- -D missing_docs`. Every new public struct, enum, variant, field, and function requires a `///` doc comment.
- **Panic Hygiene:** Zero `.unwrap()` or `.expect()` in production crates (`app_api/`, `common/`, `db_core/`, `web_client/src`, `web_ui_*/src`).

## Tasks

- [x] **Task 1: Authenticated Draft Quote Endpoint (`booking_api`)**
  - **Scope:** S
  - **Description:** Reuse existing `pub(super) struct PublicQuoteQuery` (`stay_pricing.rs:411`). Add `preview_listing_quote` accepting `Claims` and `PublicQuoteQuery`: enforce `require_host_or_admin(pool.get_ref(), &claims, "preview listing quote").await?` (`apis/mod.rs:260`, `403` for `Staff` / `Guest`), load the listing via `db_listing::get_listing_by_id` without checking `is_active` (mapping `RowNotFound` and non-owning `Host` via `not_found(...)` in `apis/mod.rs:199`), and delegate to `check_guest_count` + `price_stay`. Register `GET /api/v1/bookings/listing/{id}/preview-quote` inside the `require_user`-wrapped `/api/v1/bookings` scope in `routes.rs` and add it to `ApiDoc`. Add integration/unit tests covering `Admin`, owning `Host`, non-owning `Host` (`404`), `Staff` (`403`), and `is_active = false`.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/stay_pricing.rs`
    - `app_api/booking_api/src/apis/routes.rs`
  - **Acceptance Criteria:**
    - [ ] `GET /api/v1/bookings/listing/{id}/preview-quote` returns `200` with `StayQuoteResponse` for `Admin` and owning `Host` on both active and inactive (`is_active = false`) listings.
    - [ ] `Staff` and `Guest` receive `403 Forbidden`; non-owning `Host` and missing listing IDs receive `404 Not Found`.
    - [ ] `public_listing_quote` continues rejecting `is_active = false` listings.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p booking_api && SQLX_OFFLINE=true cargo clippy -p booking_api --all-targets -- -D warnings`
  - **Dependencies:** None

- [x] **Task 2: Preview Quote Client Method (`web_client`)**
  - **Scope:** XS
  - **Description:** Add `preview_listing_quote(listing_id: Uuid, check_in: NaiveDate, check_out: NaiveDate, guests: u32, jwt: &str)` to `web_client/src/client.rs` calling `GET /api/v1/bookings/listing/{listing_id}/preview-quote` with bearer auth (currency fixed to `listing.base_currency`) and preserving `ApiCallError` for `NoQuote::from_error`.
  - **Files Touched:**
    - `web_client/src/client.rs`
  - **Acceptance Criteria:**
    - [ ] `preview_listing_quote` compiles cleanly with doc comments and propagates structured API errors.
  - **Verification Command:** `cargo test -p web_client && cargo clippy -p web_client --all-targets -- -D warnings`
  - **Dependencies:** Task 1

- [x] **Task 3: Guest Preview Stylesheet & `AdminChrome::StandalonePreview` (`web_ui_common` + `web_ui_admin`)**
  - **Scope:** S
  - **Description:** Add `web_ui_admin/src/style/preview.css` importing `../../../web_ui_common/src/style/guest.css` so preview loads the guest palette (`--background: #EEF3F4`, `--border: #D3DEE2`, `--radius-*` tokens, `.page-grid`, `.page-columns`, `.page-flow`, `.full-bleed`, `.bleed-inner`, `body:has([data-mobile-book-bar])`) rather than `admin.css`'s neutral oklch colours. Add `AdminChrome` (`Standard` vs `StandalonePreview { stylesheet: Asset, include_newsreader: bool }`) to `shell_admin.rs` so `StandalonePreview` reuses `<!DOCTYPE html>`, `<head>`, `HTMX_CONFIG`, `INTER`, `HTMX_JS`, `UI_JS`, `theme_for_request`, `toast_region`, and `hx_request && !hx_boosted` partial unwrapping while omitting the admin sidebar/topbar and loading `NEWSREADER` plus the guest preview stylesheet.
  - **Files Touched:**
    - `web_ui_admin/src/style/preview.css`
    - `web_ui_common/src/shell_admin.rs`
  - **Acceptance Criteria:**
    - [ ] Preview stylesheet compiles with guest colours, radius tokens, and `.page-grid` / `.full-bleed` / `.page-columns` / `.bleed-inner` classes.
    - [ ] `AdminChrome::StandalonePreview` renders a full-width document without admin sidebar/topbar and unwraps HTMX partials.
  - **Verification Command:** `cargo test -p web_ui_common && cargo check -p web_ui_admin && cargo clippy -p web_ui_common -p web_ui_admin --all-targets -- -D warnings`
  - **Dependencies:** None

- [x] **Task 4: Configurable `CalendarProps` URL Prefix & Shared `NoQuote` / `breakdown` (`web_ui_common` + `web_ui_guest`)**
  - **Scope:** S
  - **Description:** Replace `listing_slug: String` with `pub calendar_url_prefix: String` in `CalendarProps` (`date_range_calendar.rs:149-150`) and update `calendar_url` (plus existing callers in `web_ui_guest/src/pages/listing_detail.rs` and unit tests). Move `breakdown(&StayQuoteResponse) -> PriceBreakdown` and `NoQuote` from `web_ui_guest/src/quote.rs` into the existing `web_ui_common/src/components/domain/price_breakdown.rs` (re-exporting in `web_ui_guest/src/quote.rs`). Note: do not use `NoQuote::from_error` for `price_per_night: None` (since `nightly_price` returns `DbError::ValidationError`, which falls through to `NoQuote::Failed`); handlers will skip the quote call and show `"Rate not set"` directly when `price_per_night` is `None`.
  - **Files Touched:**
    - `web_ui_common/src/components/date_range_calendar.rs`
    - `web_ui_common/src/components/domain/price_breakdown.rs`
    - `web_ui_guest/src/quote.rs`
    - `web_ui_guest/src/pages/listing_detail.rs`
  - **Acceptance Criteria:**
    - [ ] `calendar_url` formats month links using `calendar_url_prefix` instead of hardcoding `/listings/{slug}/calendar`.
    - [ ] `NoQuote` and `breakdown` live in `web_ui_common` with unit tests for stay-rule refusals.
  - **Verification Command:** `cargo test -p web_ui_common -p web_ui_guest && cargo clippy -p web_ui_common -p web_ui_guest --all-targets -- -D warnings`
  - **Dependencies:** None

- [x] **Task 5a: Move Listing Section Helpers & Map Tests to `web_ui_common`**
  - **Scope:** S
  - **Description:** Extract the pure section rendering helpers (hero gallery, highlights ribbon, suites, amenities, house policies, guestbook, map, availability calendar, reviews, sticky section nav) and location-map unit tests from `web_ui_guest/src/pages/listing_detail.rs` into `web_ui_common/src/components/domain/listing_detail.rs` (exporting from `web_ui_common/src/components/domain/mod.rs`), parameterized by `CtaMode` (`Live { enquire_href }` vs `Preview`) for `#listing-enquire-cta`, concierge add-on enquiry buttons, **Ask us to arrange**, and `#plan-your-stay` enquiry CTA, plus safe draft fallbacks (0 photos, missing coordinates, empty suites/reviews, and `"Rate not set"` when `price_per_night` is `None`).
  - **Files Touched:**
    - `web_ui_common/src/components/domain/listing_detail.rs`
    - `web_ui_common/src/components/domain/mod.rs`
  - **Acceptance Criteria:**
    - [ ] Section helpers and location-map tests compile and pass in `web_ui_common`.
    - [ ] Draft fallbacks handle 0 photos, unmapped coordinates, `price_per_night: None` (`"Rate not set"`), and empty suites/reviews without panicking.
  - **Verification Command:** `cargo test -p web_ui_common && cargo clippy -p web_ui_common --all-targets -- -D warnings`
  - **Dependencies:** Task 3, Task 4

- [x] **Task 5b: Move `quote_panel` to `web_ui_common` with Live/Preview CTA Switch**
  - **Scope:** S
  - **Description:** Move `quote_panel` (rendering `#quote`) and `booking_cta` into `web_ui_common/src/components/domain/listing_detail.rs` parameterized by `quote_url`, `CtaMode` (`Live { checkout_href, enquire_href }` vs `Preview`), and `rate_unset: bool` (`"Rate not set"` when `price_per_night` is `None`). In `CtaMode::Preview`, render `#listing-book-cta` (**Reserve** / **Request to book**) and `#listing-enquire-cta` as `<button type="button" disabled aria-disabled="true">` with `data-testid="preview-disabled-notice"`.
  - **Files Touched:**
    - `web_ui_common/src/components/domain/listing_detail.rs`
  - **Acceptance Criteria:**
    - [ ] `#quote` renders live price breakdowns or `"Rate not set"` cleanly.
    - [ ] `CtaMode::Preview` renders `#listing-book-cta` and all enquiry CTAs as disabled `<button>` elements.
  - **Verification Command:** `cargo test -p web_ui_common && cargo clippy -p web_ui_common --all-targets -- -D warnings`
  - **Dependencies:** Task 5a

- [ ] **Task 5c: Point `web_ui_guest` at Shared `listing_detail` & Verify Unchanged Guest HTML**
  - **Scope:** S
  - **Description:** Update `web_ui_guest/src/pages/listing_detail.rs` to delegate section and `#quote` rendering to `web_ui_common::components::domain::listing_detail` using `CtaMode::Live`, while retaining `Metadata`, `json_ld`, canonical URLs, context loading (`display_currency`, `map_config`, `tokio::join!` reviews), and `is_active` gating in `web_ui_guest`. Confirm guest listing tests pass with unchanged HTML structure and IDs (`#quote`, `#listing-book-cta`, `#listing-enquire-cta`).
  - **Files Touched:**
    - `web_ui_guest/src/pages/listing_detail.rs`
  - **Acceptance Criteria:**
    - [ ] `web_ui_guest` listing detail pages render identically using `CtaMode::Live` with SEO `Metadata` and `json_ld` intact.
    - [ ] All `web_ui_guest` tests (including `listing_detail_seo_tests`) pass.
  - **Verification Command:** `cargo test -p web_ui_guest && cargo clippy -p web_ui_guest --all-targets -- -D warnings`
  - **Dependencies:** Task 5b

- [ ] **Task 6: Admin Preview Route, Fragments, Layout Bypass, & Entry Links (`web_ui_admin`)**
  - **Scope:** M
  - **Description:** Extract `managed_listing_details` in `web_ui_admin/src/pages/listings/mod.rs` (have `managed_listing` delegate to it). Create `web_ui_admin/src/pages/listings/preview.rs` with `GET /admin/listings/{id}/preview`, `GET /admin/listings/{id}/preview/quote`, and `GET /admin/listings/{id}/preview/calendar` gated by `host_or_admin` + ownership check (`403` for `Staff`, `404` for non-owners). Obtain the bearer JWT via `service_token(cx, &user)` (matching `listings/calendar.rs:83`) for `preview_listing_quote`, and skip calling `preview_listing_quote` (rendering `"Rate not set"` in `#quote`) when `listing.price_per_night.is_none()`. Render the in-flow `"Back to Admin"` bar (`data-testid="preview-back-bar"` linking to `/admin/listings/{id}`), fixed top-right `"Preview · Active|Draft"` pill (`data-testid="preview-pill"`), and shared `listing_detail` with `CtaMode::Preview`. Update `web_ui_admin/src/layout.rs` to detect normalized `/admin/listings/{id}/preview` and `/preview/{quote,calendar}` paths via `topcoat::router::uri(cx)`, skip `nav_user(cx)`, and pass `AdminChrome::StandalonePreview` with the guest preview stylesheet. Add same-tab `"Preview"` links on `/admin/listings`, `/admin/listings/{id}`, and `/admin/listings/{id}/edit`.
  - **Files Touched:**
    - `web_ui_admin/src/pages/listings/preview.rs`
    - `web_ui_admin/src/pages/listings/mod.rs`
    - `web_ui_admin/src/pages/listings/edit.rs`
    - `web_ui_admin/src/layout.rs`
  - **Acceptance Criteria:**
    - [ ] `/admin/listings/{id}/preview` renders active and draft listings in standalone full-width guest-styled layout with `"Back to Admin"` and `"Preview · Active|Draft"` pill, without admin sidebar/topbar.
    - [ ] `/preview/quote` and `/preview/calendar` return bare HTMX fragments and update `#quote` and `#booking-dates` cleanly (showing `"Rate not set"` when `price_per_night` is `None`).
    - [ ] `"Preview"` links appear only on `/admin/listings`, `/admin/listings/{id}`, and `/admin/listings/{id}/edit`.
  - **Verification Command:** `cargo test -p web_ui_admin && cargo clippy -p web_ui_admin --all-targets -- -D warnings && cargo clippy -p web_ui_admin --all-features -- -D missing_docs`
  - **Dependencies:** Task 2, Task 5c

- [ ] **Task 7: Playwright E2E Coverage & Full Workspace Gate**
  - **Scope:** S
  - **Description:** Add Playwright E2E tests in `playwright/e2e/admin-portal/listings-admin.spec.ts` verifying `"Preview"` links on `/admin/listings`, `/admin/listings/{id}`, and `/admin/listings/{id}/edit`; standalone full-width guest-styled layout (`preview-back-bar`, `preview-pill`) for both active and draft listings; live date selection, calendar navigation, and quote calculation updating `#quote` (plus `"Rate not set"` when `price_per_night` is `None`); disabled `<button>` controls (`#listing-book-cta`, `#listing-enquire-cta`, `preview-disabled-notice`); and `Staff` `403` denial. Run full workspace formatting, clippy, and test gates.
  - **Files Touched:**
    - `playwright/e2e/admin-portal/listings-admin.spec.ts`
  - **Acceptance Criteria:**
    - [ ] All new Playwright admin preview tests pass.
    - [ ] `cargo fmt --check`, `cargo clippy --workspace`, and `cargo test --workspace` pass with zero warnings.
  - **Verification Command:** `cargo fmt --check && SQLX_OFFLINE=true cargo clippy --workspace --all-targets -- -D warnings && npx playwright test playwright/e2e/admin-portal/listings-admin.spec.ts`
  - **Dependencies:** Task 6
