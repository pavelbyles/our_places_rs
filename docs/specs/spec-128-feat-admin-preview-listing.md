# Spec 128: Admin Preview Listing

## Overview

Hosts and administrators need to inspect how a property listing renders in the guest-facing layout both before publishing (`is_active = false`) and after publishing (`is_active = true`). Today, `/admin/listings/{id}` in `web_ui_admin` only displays a key-value summary table, while `web_ui_guest` blocks inactive drafts and exposes live booking and enquiry controls on active listings.

This specification adds a host- and admin-only preview route at `/admin/listings/{id}/preview` inside `web_ui_admin` (plus HTMX quote and calendar fragment routes). It shares a single pure listing-detail presentation in `web_ui_common` with `web_ui_guest`, renders in a standalone full-width guest-like layout with an in-flow `"Back to Admin"` bar and a fixed top-right `"Preview"` pill, keeps date selection and live price quote calculation functional, and disables all booking and enquiry actions.

---

## Requirements

### Functional Requirements
1. **Preview Route & Visibility:** `GET /admin/listings/{id}/preview` in `web_ui_admin` renders both active (`is_active = true`) and inactive/draft (`is_active = false`) listings.
2. **Role & Ownership Access Control:**
   - `Admin` users may preview any listing.
   - `Host` users may preview only listings they own (`listing.user_id == session.user_id`); non-owned listings return `404 Not Found`.
   - `Staff` users are denied access (`403 Forbidden`) on all preview pages, HTMX fragments, and backend preview-quote endpoints.
3. **Standalone Preview Chrome:**
   - Render without the `web_ui_admin` sidebar or topbar (`AdminChrome::StandalonePreview`) using guest layout grid classes and `NEWSREADER` + `INTER` fonts.
   - Display an in-flow `"Back to Admin"` bar linking to `/admin/listings/{id}` (`data-testid="preview-back-bar"`).
   - Display a fixed top-right `"Preview · Active"` or `"Preview · Draft"` pill (`data-testid="preview-pill"`).
4. **Guest-Parity Sections:** Render hero images, highlights ribbon, suites, amenities, house policies, guestbook, map, availability calendar, and reviews.
5. **Interactive Quote & Calendar:** Date selection, guest count changes, calendar month navigation (`/admin/listings/{id}/preview/calendar`), and live price quote calculation (`/admin/listings/{id}/preview/quote`) remain functional for both active and draft listings using `listing.base_currency`.
6. **Disabled Booking & Enquiry Controls:** Disable **Reserve** / **Request to book** (`#listing-book-cta`), `#listing-enquire-cta`, concierge add-on enquiry links, **Ask us to arrange**, and `#plan-your-stay` enquiry buttons by rendering `<button type="button" disabled aria-disabled="true">` with a notice (`data-testid="preview-disabled-notice"`).
7. **Entry Links:** Add `"Preview"` links (same-tab navigation) only on `/admin/listings`, `/admin/listings/{id}`, and `/admin/listings/{id}/edit`.

### Exclusions / Non-Goals
- Opening preview access to `Staff` users.
- Allowing hosts or admins to create real bookings, holds, or enquiries from preview mode.
- Public or unauthenticated shareable preview links.
- Currency switching (`?currency=...`) or dynamic `?from=` return targets in preview mode (locked to `listing.base_currency` and `/admin/listings/{id}`).
- Changing `web_ui_guest` visibility rules for `is_active = false` listings.

---

## Edge Cases & Failure States

1. **Draft Without Nightly Rate (`price_per_night: None`):**
   - `ListingSummary::from(&details.listing)` defaults `price_per_night: None` to `"0"` (`"US$0"`). `listing_detail` must check `listing.price_per_night.is_some()` directly and display `"Rate not set"` instead of `"US$0"`.
   - `nightly_price` (`booking_api/src/apis/mod.rs:76`) returns `DbError::ValidationError` when `price_per_night` is `None`, which `NoQuote::from_error` (`common/src/availability.rs:114`) does not classify as a refusal (falling through to `NoQuote::Failed` `"try again"` with a warning log). Therefore, skip the `preview-quote` API call whenever `listing.price_per_night.is_none()` and render `"Rate not set"` directly in `#quote`.
2. **Incomplete Draft Sections:**
   - **0 photos:** Render the hero empty/placeholder state without panicking.
   - **Missing coordinates:** Omit the interactive map and display `"Location not mapped"`.
   - **Empty suites, guestbook, or reviews:** Render each section's existing empty-state or omit optional sections cleanly.
3. **Sticky Element Collision:**
   - `section_nav` uses `sticky top-0 z-20` and `#book` uses `lg:sticky lg:top-20`. Rendering `"Back to Admin"` as a normal in-flow bar (not `sticky top-0`) prevents overlap with `section_nav`, `#book`, and `#book` anchor scroll margins. The `"Preview"` pill uses `position: fixed` top-right (`z-40`) with compact sizing so it does not cover hero gallery controls.
4. **HTMX Partial Requests vs. Full Page Loads:**
   - If `/admin/listings/{id}/preview/quote` or `/admin/listings/{id}/preview/calendar` is requested via HTMX (`hx_request(cx) && !hx_boosted(cx)`), `admin_layout` must return the bare fragment without `<html>` or `"Back to Admin"` chrome.
5. **Invalid UUID or Missing Listing:**
   - Invalid UUID path segments return `404 Not Found` before calling backend APIs.

---

## Technical Implementation

### Data Models & Schema
- No database migrations or new tables are required.
- **`web_ui_common/src/components/date_range_calendar.rs`**:
  - Replace `listing_slug: String` with `pub calendar_url_prefix: String` in `CalendarProps` (since `listing_slug` is only used to build `calendar_url`).
  - Update `web_ui_guest/src/pages/listing_detail.rs` to pass `format!("/listings/{slug}/calendar")`, and `web_ui_admin/src/pages/listings/preview.rs` to pass `format!("/admin/listings/{id}/preview/calendar")`.
- **`web_ui_common/src/components/domain/listing_detail.rs` (New)**:
  - Pure presentation structs with zero API or session calls:
    - `CtaMode::Live { checkout_href: String, enquire_href: String }`
    - `CtaMode::Preview`
    - `ListingDetailProps` (listing details, availability calendar props, initial quote state, `quote_url`, `cta_mode`, `map_config`)
    - `QuotePanelState` (moved/shared with `NoQuote` and `breakdown(&StayQuoteResponse) -> PriceBreakdown` extracted from `web_ui_guest/src/quote.rs`).
- **`web_ui_common/src/shell_admin.rs`**:
  - Add `AdminChrome { Standard, StandalonePreview { include_newsreader: bool } }` (or `admin_document_with_chrome`) so `StandalonePreview` reuses `<!DOCTYPE html>`, `<head>`, `HTMX_CONFIG`, `INTER`, `HTMX_JS`, `UI_JS`, `theme_for_request`, `toast_region`, and `hx_request && !hx_boosted` partial unwrapping while omitting the admin sidebar/topbar and loading `NEWSREADER`.

### API Endpoints
- **`app_api/booking_api/src/apis/stay_pricing.rs` & `routes.rs`**:
  - Reuse existing `pub(super) struct PublicQuoteQuery` (`stay_pricing.rs:411`) for preview quotes.
  - Add `GET /api/v1/bookings/listing/{id}/preview-quote` inside the `require_user`-wrapped `/api/v1/bookings` scope in `routes.rs` and add it to `ApiDoc` `#[openapi(paths(...))]`.
  - Handler `preview_listing_quote`:
    1. Call `let user = require_host_or_admin(pool.get_ref(), &claims, "preview listing quote").await?;` (`booking_api/src/apis/mod.rs:260`, `403 Forbidden` for `Staff` or `Guest`).
    2. Load listing via `db_listing::get_listing_by_id` without checking `is_active`; map `RowNotFound` to `not_found("Listing not found")` (`booking_api/src/apis/mod.rs:199`).
    3. Verify `is_admin(&user) || listing.user_id == user.id`; otherwise return `not_found("Listing not found")`.
    4. Delegate to `check_guest_count` and `price_stay` (using `listing.base_currency`).
- **`web_client/src/client.rs`**:
  - Add `pub async fn preview_listing_quote(&self, listing_id: Uuid, check_in: NaiveDate, check_out: NaiveDate, guests: u32, jwt: &str) -> anyhow::Result<StayQuoteResponse>` calling `GET /api/v1/bookings/listing/{listing_id}/preview-quote` with bearer auth (currency fixed to `listing.base_currency`).

### Frontend Views & CSS
- **Guest Stylesheet (`web_ui_admin/src/style/preview.css` & `web_ui_common/src/style/guest.css`)**:
  - `admin.css` uses neutral oklch colours whereas `guest.css` defines the guest palette (`--background: #EEF3F4`, `--border: #D3DEE2`, `--radius-*` tokens, `.page-grid`, `.page-columns`, `.page-flow`, `.full-bleed`, `.bleed-inner`, and `body:has([data-mobile-book-bar])`).
  - Have the preview page load a guest-themed stylesheet compiled from `web_ui_common/src/style/guest.css` (e.g., `web_ui_admin/src/style/preview.css` passed via `AdminChrome::StandalonePreview`) so preview renders with guest colours and layout classes instead of `admin.css`.
- **`web_ui_admin/src/layout.rs`**:
  - Match normalized request paths `/admin/listings/{id}/preview`, `/admin/listings/{id}/preview/quote`, and `/admin/listings/{id}/preview/calendar` to select `AdminChrome::StandalonePreview` (and skip `nav_user(cx)` on preview/partial requests).
- **`web_ui_admin/src/pages/listings/preview.rs` (New)**:
  - `GET /admin/listings/{id}/preview`: calls `host_or_admin(cx).await?`, validates UUID, calls `managed_listing_details(cx, &user, raw).await?` (extracted in `mod.rs` so `managed_listing` delegates to it without duplicate API calls), fetches `get_unavailable_ranges`, computes initial quote only when `listing.price_per_night.is_some()` (otherwise shows `"Rate not set"`), and renders the `"Back to Admin"` bar, top-right `"Preview · Active|Draft"` pill, and `web_ui_common::components::domain::listing_detail` with `CtaMode::Preview`.
  - `GET /admin/listings/{id}/preview/quote`: calls `host_or_admin(cx).await?`, validates ownership, skips `preview_listing_quote` and renders `"Rate not set"` in `#quote` when `listing.price_per_night.is_none()`, otherwise obtains the bearer JWT via `service_token(cx, &user)` (matching `listings/calendar.rs:83`), calls `preview_listing_quote`, and renders `#quote` with `CtaMode::Preview`.
  - `GET /admin/listings/{id}/preview/calendar`: calls `host_or_admin(cx).await?`, validates ownership, and renders `date_range_calendar` with `calendar_url_prefix`.
- **`web_ui_admin/src/pages/listings/mod.rs` & `edit.rs`**:
  - Add `"Preview"` same-tab links on `/admin/listings` (table Actions), `/admin/listings/{id}` (header Actions), and `/admin/listings/{id}/edit` (header Actions).
- **`web_ui_guest/src/pages/listing_detail.rs` & `quote.rs`**:
  - Delegate presentation and `quote_panel` rendering to `web_ui_common` with `CtaMode::Live`, keeping `Metadata`, `json_ld`, canonical guest URLs, and `is_active` gating strictly in `web_ui_guest`.

### Background Workers
- None.

---

## Performance Considerations

- **Single Listing Fetch:** Extracting `managed_listing_details` avoids issuing two `get_listing_by_id` HTTP calls per preview page load.
- **No Layout User Fetch on Preview:** Skipping `nav_user(cx)` in `admin_layout` for preview routes avoids an extra session/user round-trip.
- **No New Migrations or Services:** Reuses existing `booking_api`, `listing_api`, and `web_ui_admin` Cloud Run services with zero cold-start penalty from new dependencies.

---

## Security Mitigations

- **Role Isolation (`403 Forbidden`):** `host_or_admin(cx)` (`AdminSession::require_host_or_admin`) in `web_ui_admin` and `require_host_or_admin(pool, &claims, ...)` in `booking_api` reject `Staff` and `Guest` sessions before any listing lookup.
- **IDOR Protection (`404 Not Found`):** Both `managed_listing_details` and `preview_listing_quote` verify `is_admin || listing.user_id == caller.user_id`, returning `404` for non-owned listings so existence is not leaked.
- **Public Quote Protection:** `public_listing_quote` (`stay_pricing.rs`) remains locked to `is_active = true`; draft pricing is only exposed via authenticated `preview-quote`.
- **Accepted Risk — `availability::get_unavailable_ranges`:** `get_unavailable_ranges` does not filter on `is_active` and returns only booked/blocked date ranges (no pricing, no guest PII) for unguessable UUIDs; accepted without adding a second availability endpoint.
- **No Open Redirect:** `"Back to Admin"` always links to `/admin/listings/{id}` (no user-controlled `?from=` query parameter).
- **SEO & Indexing Isolation:** `web_ui_admin` enforces `X-Robots-Tag: noindex, nofollow` and omits `json_ld` / canonical links on preview pages.

---

## Test Plan

- **Unit Tests:**
  - `web_ui_common`: `date_range_calendar::calendar_url` tests covering custom `calendar_url_prefix` values (`/listings/{slug}/calendar` and `/admin/listings/{id}/preview/calendar`).
  - `web_ui_common`: `breakdown` and `NoQuote` mapping tests (including `price_per_night: None` and stay-rule refusals).
- **Integration Tests (`app_api/booking_api`):**
  - `preview_listing_quote` returns `200` with accurate `StayQuoteResponse` for `Admin` and owning `Host` on both `is_active = true` and `is_active = false` listings.
  - `preview_listing_quote` returns `403` for `Staff` and `Guest`, and `404` for non-owning `Host` or unknown `listing_id`.
  - `public_listing_quote` continues returning `ListingUnavailable` for `is_active = false`.
- **E2E Tests (`playwright/e2e/admin-portal/listings-admin.spec.ts`):**
  - Verify `"Preview"` links appear on `/admin/listings`, `/admin/listings/{id}`, and `/admin/listings/{id}/edit` and navigate in the same tab.
  - Verify active and draft (`is_active = false`) listings render in standalone full-width layout (no admin sidebar) with `data-testid="preview-back-bar"` (`/admin/listings/{id}`) and `data-testid="preview-pill"` (`Preview · Active` / `Preview · Draft`).
  - Verify date selection, calendar month navigation, and live quote calculation update `#quote` (and drafts with `price_per_night: None` show `"Rate not set"`).
  - Verify `#listing-book-cta` (**Reserve** / **Request to book**), `#listing-enquire-cta`, concierge add-on enquiry buttons, **Ask us to arrange**, and `#plan-your-stay` enquiry buttons are `<button disabled>` (non-navigating) and `data-testid="preview-disabled-notice"` is visible.
  - Verify `Staff` receives `403` on `/admin/listings/{id}/preview`.

---

## Acceptance Criteria

- [ ] `Admin` and owning `Host` can open `/admin/listings/{id}/preview` from `/admin/listings`, `/admin/listings/{id}`, and `/admin/listings/{id}/edit` for both active and draft (`is_active = false`) listings.
- [ ] `Staff` receives `403 Forbidden` and non-owning `Host` receives `404 Not Found` on `/admin/listings/{id}/preview`, `/preview/quote`, `/preview/calendar`, and `booking_api` `preview-quote`.
- [ ] Preview renders in a standalone full-width guest-like layout (no admin sidebar/topbar) with `"Back to Admin"` linking to `/admin/listings/{id}` and a fixed top-right `"Preview · Active"` / `"Preview · Draft"` pill that does not collide with sticky `section_nav` or `#book`.
- [ ] All guest listing sections (hero images, highlights ribbon, suites, amenities, house policies, guestbook, map, availability calendar, and reviews) render accurately, with safe fallbacks for incomplete drafts.
- [ ] Date selection, calendar month navigation, and live price quote calculation work in preview mode for both active and draft listings.
- [ ] **Reserve**, **Request to book**, and all **Send enquiry** actions are rendered as disabled `<button>` elements and cannot trigger bookings or enquiries.
- [ ] `cargo check --workspace`, `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, and `npx playwright test playwright/e2e/admin-portal/listings-admin.spec.ts` pass.
