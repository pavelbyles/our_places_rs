# Admin Preview Listing (GitHub Issue #128)

* **Author:** Pavel Byles
* **Date:** 2026-11-19

## Overview

This feature adds a dedicated `/admin/listings/{id}/preview` route inside `web_ui_admin` so `Admin` users and owning `Host` users can inspect how both active (`is_active = true`) and inactive/draft (`is_active = false`) listings look to guests before and after publishing. The preview renders in a standalone full-width guest-like layout with a `"Back to Admin"` bar and a `"Preview"` pill in the top-right of the viewport. All guest listing sections—hero images, highlights ribbon, suites, amenities, house policies, guestbook, map, availability calendar, and reviews—are rendered alongside working date selection and live price quote calculation, while **Book**, **Request to book**, and **Send enquiry** actions are disabled. Entry links appear only on `/admin/listings`, `/admin/listings/{id}`, and `/admin/listings/{id}/edit`, and `Staff` users are denied access.

## Analysis

Codebase inspection across `web_ui_admin`, `web_ui_guest`, `web_ui_common`, and `app_api/booking_api` identified four structural constraints and edge cases that must be addressed in the design:

### 1. Backend Quote Blocker for Inactive Listings (`booking_api`)
* `app_api/booking_api/src/apis/stay_pricing.rs` (`load_listing`) explicitly rejects `is_active = false` listings with `DbError::ListingUnavailable`. Consequently, live price quote calculation for draft/inactive listings will fail unless `booking_api` exposes an authenticated host/admin preview-quote path (while keeping the public guest quote endpoint locked to `is_active = true`).
* By contrast, `app_api/booking_api/src/apis/availability.rs` (`get_unavailable_ranges`) does not filter on `is_active`, so calendar availability already works for drafts.
* Drafts with `price_per_night: None` return a 400 from `nightly_price`; the quote panel must render that state cleanly without panicking.

### 2. Sibling Crate Isolation & Shared Listing View (`web_ui_common`)
* Per `AGENTS.md`, `web_ui_admin` and `web_ui_guest` are sibling crates that cannot depend on each other.
* `web_ui_guest/src/pages/listing_detail.rs` is ~1,770 lines and already uses shared components in `web_ui_common/src/components/`. Extracting the pure listing detail presentation and `quote_panel` into `web_ui_common` (parameterized by `quote_url`, `calendar_url`, and a preview/live CTA mode) avoids duplicating ~1,000+ lines of markup and guarantees visual parity between the guest portal and admin preview.
* `web_ui_common/src/components/date_range_calendar.rs` (`calendar_url`) currently hardcodes `/listings/{slug}/calendar?month=...`. Month navigation in preview mode requires passing a configurable `calendar_base_url` (e.g., `/admin/listings/{id}/preview/calendar`).

### 3. Standalone Shell, Assets, & Sticky Coordination (`web_ui_admin`)
* `web_ui_admin/src/layout.rs` (`#[layout("/")]`) wraps all admin routes in `admin_shell` (sidebar + topbar, `INTER` font only). The preview route needs a standalone full-width document shell (no admin sidebar, no guest header/footer) that loads `NEWSREADER` font alongside `INTER`, `HTMX_JS`, `UI_JS`, and guest layout utilities (`page-grid`, `full-bleed`, `page-columns`, `page-flow`).
* `web_ui_admin/src/main.rs` already configures `MapConfig` and `SecurityHeaders::admin().with_map_origin(...)`, so MapLibre maps, photo gallery `<dialog>`, and lightbox assets will work without CSP changes.
* `listing_detail.rs` uses sticky `section_nav` and `lg:sticky lg:top-20` on `#book`. The `"Back to Admin"` bar and top-right `"Preview"` pill must coordinate with sticky offsets (or stay in normal document flow) so they never overlap `section_nav` or `#book`.
* `web_ui_admin` already enforces `X-Robots-Tag: noindex, nofollow` and `Disallow: /`; preview must not emit `json_ld` (`VacationRental`) or canonical guest URLs.

### 4. All Disabled Enquiry/Booking Paths & Incomplete Drafts
* Beyond the primary **Book** / **Request to book** button and `#listing-enquire-cta` (`Ask a question`), `listing_detail.rs` links to `/listings/{slug}/enquire` from concierge add-on items (`#services`), `"Ask us to arrange"` (`arrange_href`), and `"Ask a question"` in `#plan-your-stay`. All of those links must be disabled/non-navigating in preview mode so clicking them inside `web_ui_admin` does not 404.
* Incomplete drafts (`is_active = false`) may have zero photos, no suites, no coordinates (`Location not mapped`), or no reviews/guestbook; all sections must handle empty/missing data gracefully.

## Goals vs. Non-Goals

* **In-Scope Goals:**
  * Host- and admin-only `/admin/listings/{id}/preview` route (plus `/preview/quote` and `/preview/calendar` HTMX fragment routes) in `web_ui_admin`, returning `403` for `Staff` and `404` for non-owning hosts.
  * Support both `is_active = true` and `is_active = false` listings, including live date selection and price quote calculation via an authenticated `booking_api` preview-quote endpoint.
  * Standalone full-width guest-like layout with a `"Back to Admin"` bar and a `"Preview"` pill in the top-right of the viewport.
  * Full section fidelity (hero images, highlights ribbon, suites, amenities, house policies, guestbook, map, availability calendar, reviews) with **Book**, **Request to book**, and all **Send enquiry** CTAs disabled.
  * `"Preview"` entry links added only on `/admin/listings`, `/admin/listings/{id}`, and `/admin/listings/{id}/edit`.
* **Out-of-Scope Non-Goals:**
  * Allowing `Staff` users to preview listings.
  * Allowing hosts to create real bookings, holds, or enquiries from preview mode.
  * Public/unauthenticated shareable preview links (token-based external preview URLs).
  * Modifying `web_ui_guest` public visibility rules for `is_active = false` listings.

## Alternatives Weighed

* **Duplicate `listing_detail.rs` inside `web_ui_admin` vs. Extract shared view to `web_ui_common`:** *Rejected.* Duplicating ~1,000+ lines of Topcoat markup across sibling crates violates DRY and guarantees visual drift between the guest page and admin preview.
* **Link or iframe `web_ui_guest` from `web_ui_admin` vs. Dedicated route inside `web_ui_admin`:** *Rejected by author.* Cross-origin/cross-service auth and cookie boundaries complicate draft access, and the feature requires previewing directly inside `web_ui_admin`.
* **Relax public `booking_api` quote endpoint for `is_active = false` vs. Authenticated `Claims`-gated preview quote endpoint:** *Rejected.* Relaxing the public quote endpoint would expose draft pricing and availability to unauthenticated callers.
* **Render preview inside standard `admin_shell` (sidebar + topbar) vs. Standalone full-width guest-like layout:** *Rejected by author.* The admin sidebar compresses the viewport and distorts the guest layout hosts need to inspect.

## Key Decisions & Staging for Next Steps

1. **Shared Presentation in `web_ui_common`:** Extract the pure listing detail view and `quote_panel` from `web_ui_guest/src/pages/listing_detail.rs` into `web_ui_common`, parameterized by `quote_url`, `calendar_url`, and `CtaMode` (`Live` vs `Preview`). Parameterize `date_range_calendar.rs` with `calendar_url`.
2. **Authenticated Draft Quote Support in `booking_api`:** Add a `Claims`-authenticated preview quote endpoint in `booking_api` (accessible to `Admin` or owning `Host`) so `is_active = false` listings can compute live price breakdowns without opening the public quote endpoint.
3. **Standalone Preview Chrome & Sticky Layout:** Bypass `admin_shell` sidebar/topbar for `/admin/listings/{id}/preview`, render a standalone document with guest fonts/styles, a `"Back to Admin"` bar, and a top-right `"Preview"` pill (e.g., `Preview · Active` / `Preview · Draft`).
4. **Open Questions for `/egm-draft`:**
   * Should preview support currency switching (`?currency=...`) or default to the listing's `base_currency`?
   * Should `"Back to Admin"` always target `/admin/listings/{id}` or preserve the originating page (e.g., `?from=edit`)?
   * Should `"Preview"` links open in the same tab so `"Back to Admin"` is the primary return path?
