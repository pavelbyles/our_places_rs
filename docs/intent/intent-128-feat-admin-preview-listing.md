# Admin Preview Listing (Issue #128)

*   **Author:** Pavel Byles
*   **Status:** Draft
*   **Date:** 2026-11-19

--------------------------------------------------------------------------------

## Overview

Hosts and administrators need to verify how a property listing appears to guests before publishing it and after making updates. This feature adds a dedicated `/admin/listings/{id}/preview` route inside `web_ui_admin` that renders both active and inactive/draft listings in a standalone, full-width guest-like layout. It includes a `"Back to Admin"` bar and a top-right `"Preview"` pill, keeps date selection and live price quote calculation functional, and disables all booking and enquiry submission actions.

## The Problem

Today, `/admin/listings/{id}` in `web_ui_admin` only displays a plain key-value summary table of facts and amenities. Hosts and administrators cannot see how hero photos, highlights, suites, house policies, guestbook entries, the map, the availability calendar, or reviews actually look together in the guest-facing layout:

*   **Pre-publish blind spot:** Draft listings (`is_active = false`) cannot be viewed on `web_ui_guest` at all, so hosts must publish a listing first just to inspect its layout, seasonal pricing, and copy.
*   **Post-publish friction and risk:** Inspecting an active listing currently requires switching to the public guest portal, where live **Book**, **Request to book**, and **Send enquiry** controls are active and can trigger accidental bookings or test enquiries.
*   **Role boundary:** Cleaning and maintenance `Staff` users inside `web_ui_admin` should not gain access to listing previews or pricing verification.

## Goals

*   Allow `Admin` users and the owning `Host` to preview both active (`is_active = true`) and inactive/draft (`is_active = false`) listings at `/admin/listings/{id}/preview` inside `web_ui_admin`.
*   Render all guest-facing listing sections in a standalone full-width guest-like layout: hero images, highlights ribbon, suites, amenities, house policies, guestbook, map, availability calendar, and reviews.
*   Clearly distinguish preview mode from edit mode and the live guest site with a `"Back to Admin"` bar and a `"Preview"` pill in the top-right of the viewport.
*   Keep date selection and live price quote calculation working in preview mode so hosts can verify nightly and seasonal pricing before publishing.
*   Disable **Book**, **Request to book**, and all **Send enquiry** actions (including concierge add-on enquiry links) so no accidental bookings or enquiries can be submitted.
*   Restrict preview access strictly to `Admin` and the owning `Host` (returning `403` for `Staff` and `404` for non-owning hosts), and add `"Preview"` entry links only on `/admin/listings`, `/admin/listings/{id}`, and `/admin/listings/{id}/edit`.

## Non-Goals

*   Opening `/admin/listings/{id}/preview` to `Staff` users.
*   Allowing hosts or admins to create real bookings, holds, or enquiries from preview mode.
*   Generating public or unauthenticated shareable preview URLs for external reviewers.
*   Changing `web_ui_guest` visibility rules so that `is_active = false` listings become publicly accessible on the guest portal.

## Open Questions

*   **Question 1 (Currency Switching):** Should `/admin/listings/{id}/preview` support currency switching (`?currency=USD|JMD|...`) so hosts can verify converted quotes, or default strictly to the listing's `base_currency`?
*   **Question 2 (Return Target):** Should the `"Back to Admin"` link always return to `/admin/listings/{id}`, or preserve where the user opened preview from (for example `/admin/listings/{id}/edit`)?
*   **Question 3 (Tab Behavior):** Should `"Preview"` links open in the same tab (making the `"Back to Admin"` bar the primary return path) or in a new tab?

## Critical User Journeys (CUJs)

### CUJ 1: Host Previews an Inactive Draft Before Publishing

*   **Given**: A host owns a draft listing (`is_active = false`) and is signed into `web_ui_admin` on `/admin/listings/{id}` or `/admin/listings/{id}/edit`.
*   **When**: The host clicks `"Preview"`, selects check-in and check-out dates on `/admin/listings/{id}/preview`, and inspects the rendered sections.
*   **Then**: The page renders in a standalone full-width guest-like layout with a `"Back to Admin"` bar and top-right `"Preview"` pill, displays all populated sections and a live price breakdown for the selected dates, and keeps **Book**, **Request to book**, and **Send enquiry** disabled.

### CUJ 2: Unauthorized Staff or Non-Owning Host Attempts Preview

*   **Given**: A `Staff` user or a `Host` who does not own the target listing is signed into `web_ui_admin`.
*   **When**: They request `/admin/listings/{id}/preview`, `/admin/listings/{id}/preview/quote`, or `/admin/listings/{id}/preview/calendar`.
*   **Then**: `Staff` receives `403 Forbidden`, and a non-owning `Host` receives `404 Not Found`.

## Technical Plan (Architecture)

The implementation splits into three layers so that `web_ui_admin` and `web_ui_guest` remain decoupled sibling crates while sharing one listing presentation:

1.  **Authenticated Draft Quote Endpoint (`booking_api` + `common` / `web_client`)**
    *   `app_api/booking_api/src/apis/stay_pricing.rs` currently blocks `is_active = false` listings in `load_listing` for the public quote endpoint. We keep the public route restricted to active listings and add an authenticated `Claims`-gated preview quote endpoint (`GET /api/v1/bookings/listing/{listing_id}/preview-quote`).
    *   The handler validates that the caller is `Admin` or the owning `Host` (returning `403` for `Staff` and `404` for non-owning hosts), loads the listing without the `is_active` check, and delegates to the existing `price_stay` pipeline.
    *   `common` / `web_client` adds a corresponding `preview_listing_quote` client method passing the caller's bearer session token.
    *   Calendar availability (`get_unavailable_ranges` in `app_api/booking_api/src/apis/availability.rs`) already works for inactive listings and needs no backend change.

2.  **Shared Listing Detail & Parameterized Calendar (`web_ui_common`)**
    *   Extract the pure presentation view, section helpers, and `quote_panel` markup from `web_ui_guest/src/pages/listing_detail.rs` into `web_ui_common/src/components/domain/listing_detail.rs` (and `quote_panel.rs` if kept separate).
    *   Parameterize the shared view with:
        *   `quote_url`: `/listings/{slug}/quote` in guest mode vs. `/admin/listings/{id}/preview/quote` in admin preview.
        *   `calendar_url`: `/listings/{slug}/calendar` in guest mode vs. `/admin/listings/{id}/preview/calendar` in admin preview.
        *   `CtaMode`: `Live { checkout_href, enquire_href }` vs. `Preview` (renders **Reserve** / **Request to book**, **Ask a question**, concierge add-on enquiry links, and **Ask us to arrange** as disabled controls with `aria-disabled="true"` and a short preview notice, while keeping date inputs, guest count, and `#book` anchor links active).
    *   Update `CalendarProps` and `calendar_url` in `web_ui_common/src/components/date_range_calendar.rs` to accept a configurable `calendar_base_url` instead of hardcoding `/listings/{slug}/calendar`.
    *   Add a standalone `preview_shell` helper in `web_ui_common` (loading `INTER` and `NEWSREADER` fonts, `HTMX_JS`, `UI_JS`, `toast_region`, the sticky `"Back to Admin"` bar, and the top-right `"Preview"` pill) so `web_ui_admin` can render full-width guest proportions without the admin sidebar.

3.  **Standalone Preview Route & Entry Links (`web_ui_admin`)**
    *   Add `web_ui_admin/src/pages/listings/preview.rs` with three `host_or_admin`-gated routes:
        *   `GET /admin/listings/{id}/preview` (full preview page)
        *   `GET /admin/listings/{id}/preview/quote` (HTMX quote fragment calling `preview_listing_quote`)
        *   `GET /admin/listings/{id}/preview/calendar` (HTMX calendar month fragment)
    *   Update `web_ui_admin/src/layout.rs` (`#[layout("/")]`) to detect `/admin/listings/*/preview` routes and skip `admin_shell` (rendering `preview_shell` instead) while preserving HTMX partial responses.
    *   Add `"Preview"` links only in `/admin/listings` (`web_ui_admin/src/pages/listings/mod.rs` table actions), `/admin/listings/{id}` (`listing_detail_page` header actions), and `/admin/listings/{id}/edit` (`web_ui_admin/src/pages/listings/edit.rs` header actions).
    *   Keep `Metadata` / `json_ld` / canonical guest links exclusively in `web_ui_guest/src/pages/listing_detail.rs` so `web_ui_admin` remains `noindex, nofollow`.

```text
Host / Admin Browser
  │
  ├── GET /admin/listings/{id}/preview ──► web_ui_admin (preview.rs)
  │     │                                    ├── host_or_admin + ownership check
  │     │                                    ├── listing_api: get_listing_by_id (active or draft)
  │     │                                    ├── booking_api: get_unavailable_ranges
  │     │                                    └── web_ui_common: preview_shell + listing_detail (CtaMode::Preview)
  │     │
  │     ├── HTMX change on #booking-dates ─► GET /admin/listings/{id}/preview/quote
  │     │                                      └── booking_api: GET /api/v1/bookings/listing/{id}/preview-quote (Claims)
  │     │
  │     └── HTMX month click ──────────────► GET /admin/listings/{id}/preview/calendar
  │                                            └── web_ui_common: date_range_calendar (calendar_base_url)
```

## Alternatives Considered

### Alternative A: Duplicate `listing_detail.rs` inside `web_ui_admin`

*   **Pros**: Leaves `web_ui_guest` and `web_ui_common` untouched during initial delivery.
*   **Cons**: Duplicates ~1,000+ lines of Topcoat markup, section helpers, and `quote_panel` logic across sibling crates; future guest UI updates would silently drift from admin preview.
*   **Reason for Rejection**: Rejected due to maintenance cost and layout drift; extracting the pure presentation into `web_ui_common` keeps both portals aligned without violating sibling crate boundaries.

### Alternative B: Link or `<iframe>` `web_ui_guest` from `web_ui_admin`

*   **Pros**: Avoids building any listing detail view in `web_ui_admin`.
*   **Cons**: `web_ui_admin` and `web_ui_guest` run as separate portals/origins, complicating session/cookie sharing; `web_ui_guest` would need special preview-mode auth and draft-bypass logic to show `is_active = false` listings without leaking them publicly.
*   **Reason for Rejection**: Rejected by author: preview must live at a dedicated route inside `web_ui_admin` with clean admin authentication and zero changes to `web_ui_guest` public visibility rules.

### Alternative C: Relax Public `booking_api` Quote Endpoint for `is_active = false`

*   **Pros**: Avoids adding a new `preview-quote` endpoint in `booking_api`.
*   **Cons**: Allows unauthenticated callers to probe nightly rates, seasonal pricing, and tax breakdowns for unpublished draft listings.
*   **Reason for Rejection**: Rejected on security grounds; keeping the public quote endpoint restricted to `is_active = true` and adding a `Claims`-authenticated preview quote endpoint protects unpublished pricing.

### Alternative D: Render Preview Inside Standard `admin_shell` (Sidebar + Topbar)

*   **Pros**: Requires no layout bypass in `web_ui_admin/src/layout.rs`.
*   **Cons**: The persistent admin sidebar narrows the content column, breaking full-bleed hero proportions, sticky section navigation, and responsive desktop breakpoints that hosts need to verify.
*   **Reason for Rejection**: Rejected by author: preview requires a standalone full-width guest-like layout with a `"Back to Admin"` bar and a top-right `"Preview"` pill.

## Non-Functional Requirements

*   **Security & Access Control:** `Staff` sessions must receive `403 Forbidden` on `/admin/listings/{id}/preview`, its HTMX fragments, and `booking_api`'s `preview-quote` endpoint. Non-owning `Host` sessions must receive `404 Not Found`. Public `booking_api` quote endpoints must continue rejecting `is_active = false`. `web_ui_admin` must not emit `json_ld` or canonical guest URLs on preview pages.
*   **Performance & Cold Start:** No new microservices, background workers, or database migrations are introduced; preview reuses existing `listing_api`, `booking_api`, and `web_ui_common` pipelines.

## Detailed Implementation Plan

### Target Files to Create/Modify

#### 1. `app_api/booking_api/src/apis/stay_pricing.rs` (Modify)

*   **Type / Scope**: Rust Actix-web API handler.
*   **Proposed Changes**: Add an authenticated `preview_listing_quote` handler accepting `Claims` and `StayQuoteQuery`. Verify the caller is `Admin` or the listing's owning `Host` (`403` for `Staff`, `404` for non-owning `Host`), load the listing without rejecting `is_active = false`, and delegate to the existing `price_stay` calculation. Keep `public_listing_quote` and `load_listing` unchanged so public quotes still reject inactive listings.
*   **Rationale**: Required so live date selection and price breakdown calculation work on unpublished draft listings without exposing draft pricing to unauthenticated callers.

#### 2. `app_api/booking_api/src/apis/routes.rs` (Modify)

*   **Type / Scope**: Actix-web route registration and OpenAPI spec.
*   **Proposed Changes**: Register `GET /api/v1/bookings/listing/{id}/preview-quote` pointing to `stay_pricing::preview_listing_quote`.
*   **Rationale**: Exposes the authenticated draft-capable quote endpoint to `web_client`.

#### 3. `web_client/src/client.rs` (Modify)

*   **Type / Scope**: Shared Rust API client.
*   **Proposed Changes**: Add `preview_listing_quote(listing_id, query, bearer_token)` calling `GET /api/v1/bookings/listing/{id}/preview-quote` with the signed-in admin/host token.
*   **Rationale**: Allows `web_ui_admin` to request quotes for both active and inactive listings on behalf of the authenticated host or admin.

#### 4. `web_ui_common/src/components/date_range_calendar.rs` (Modify)

*   **Type / Scope**: Shared Topcoat UI component.
*   **Proposed Changes**: Add `calendar_url_prefix: String` (or `base_url`) to `CalendarProps` and update `calendar_url` so previous/next month HTMX links target the caller-supplied base path instead of hardcoding `/listings/{slug}/calendar`.
*   **Rationale**: Prevents month navigation in `/admin/listings/{id}/preview` from hitting non-existent `/listings/{slug}/calendar` routes inside `web_ui_admin`.

#### 5. `web_ui_common/src/components/domain/listing_detail.rs` (Create) & `web_ui_common/src/components/domain/mod.rs` (Modify)

*   **Type / Scope**: Shared Topcoat UI domain module.
*   **Proposed Changes**: Extract the pure listing detail presentation (hero gallery, highlights ribbon, suites, amenities, house policies, guestbook, map, availability calendar, reviews, sticky section nav) and `quote_panel` rendering from `web_ui_guest`. Parameterize with `quote_url`, `calendar_url`, and `CtaMode` (`Live { checkout_href, enquire_href }` vs `Preview`). In `CtaMode::Preview`, render **Reserve** / **Request to book**, `#listing-enquire-cta`, concierge add-on enquiry links, **Ask us to arrange**, and `#plan-your-stay` enquiry buttons as disabled elements (`disabled`, `aria-disabled="true"`) with a short preview notice, while keeping date inputs, guest count, and `#book` anchor links active. Handle missing draft fields (0 photos, `price_per_night: None`, unmapped coordinates, empty suites/reviews) without panicking.
*   **Rationale**: Eliminates ~1,000+ lines of duplicate markup across sibling crates `web_ui_guest` and `web_ui_admin` and guarantees visual parity.

#### 6. `web_ui_common/src/shell_admin.rs` (Modify)

*   **Type / Scope**: Shared Topcoat shell helper.
*   **Proposed Changes**: Add `admin_preview_shell` rendering a standalone full-width HTML document (no admin sidebar or topbar) that loads both `INTER` and `NEWSREADER` fonts, `HTMX_JS`, `UI_JS`, and `toast_region`, plus a sticky `"Back to Admin"` bar and a top-right `"Preview"` pill (e.g., `Preview · Active` / `Preview · Draft`) coordinated with sticky offsets.
*   **Rationale**: Gives `web_ui_admin` guest-page viewport proportions and font styling while clearly signaling preview mode.

#### 7. `web_ui_guest/src/pages/listing_detail.rs` & `web_ui_guest/src/quote.rs` (Modify)

*   **Type / Scope**: Guest portal SSR pages.
*   **Proposed Changes**: Replace inline listing detail markup and `quote_panel` rendering with calls to `web_ui_common::components::domain::listing_detail` using `CtaMode::Live`, retaining `Metadata`, `json_ld`, canonical URLs, and `is_active` gating in `web_ui_guest`.
*   **Rationale**: Keeps a single source of truth for listing presentation while isolating guest SEO and public visibility rules in `web_ui_guest`.

#### 8. `web_ui_admin/src/pages/listings/preview.rs` (Create) & `web_ui_admin/src/pages/listings/mod.rs` (Modify)

*   **Type / Scope**: Admin portal SSR routes and listing index/detail links.
*   **Proposed Changes**: Create `preview.rs` with three `host_or_admin`-gated routes: `GET /admin/listings/{id}/preview`, `GET /admin/listings/{id}/preview/quote`, and `GET /admin/listings/{id}/preview/calendar` (each validating UUID, `Admin` or owning `Host` via `managed_listing`, and returning `403` for `Staff` / `404` for non-owners). Update `mod.rs` to export `preview` and add `"Preview"` links to the `/admin/listings` table actions and `/admin/listings/{id}` header actions.
*   **Rationale**: Implements the admin preview routes and two of the three requested entry points.

#### 9. `web_ui_admin/src/pages/listings/edit.rs` (Modify)

*   **Type / Scope**: Admin listing edit page.
*   **Proposed Changes**: Add a `"Preview"` link in the header actions of `/admin/listings/{id}/edit` pointing to `/admin/listings/{id}/preview`.
*   **Rationale**: Implements the third requested entry point without adding preview links to unrelated admin pages.

#### 10. `web_ui_admin/src/layout.rs` (Modify)

*   **Type / Scope**: Admin root layout wrapper.
*   **Proposed Changes**: Inspect the request path in `admin_layout` and bypass `admin_shell` (rendering `admin_preview_shell` instead for full-document requests, or bare slot for HTMX fragments) when serving `/admin/listings/*/preview`.
*   **Rationale**: Prevents `#[layout("/")]` from wrapping the standalone preview in the admin sidebar and topbar.

#### 11. `playwright/e2e/admin-portal/listings-admin.spec.ts` (Modify) & Crate Unit/Integration Tests

*   **Type / Scope**: E2E and Rust test coverage.
*   **Proposed Changes**: Add Playwright tests verifying `"Preview"` links on `/admin/listings`, `/admin/listings/{id}`, and `/admin/listings/{id}/edit`; standalone full-width layout with `"Back to Admin"` bar and top-right `"Preview"` pill for both active and draft listings; live date selection and quote calculation; disabled **Book** / **Request to book** / **Send enquiry** controls; and `Staff` `403` denial. Add Rust tests in `booking_api` for `preview_listing_quote` (`Admin`, owning `Host`, non-owning `Host` `404`, `Staff` `403`, and `is_active = false` quote success).
*   **Rationale**: Verifies all acceptance criteria and authorization boundaries end-to-end.

### Verification Plan

*   **Compilation/Build**: `cargo check --workspace` and `cargo build --workspace`
*   **Unit & Integration Tests**: `cargo test --workspace` (including `booking_api` `preview_listing_quote` auth and inactive-listing tests, plus `web_ui_common` calendar URL tests)
*   **Static Analysis & Linting**: `cargo clippy --workspace -- -D warnings` and `cargo sqlx prepare --check` (if any SQLx queries are added)
*   **E2E/Integration**: `npx playwright test playwright/e2e/admin-portal/listings-admin.spec.ts`

--------------------------------------------------------------------------------

*Drafted with EGM*
