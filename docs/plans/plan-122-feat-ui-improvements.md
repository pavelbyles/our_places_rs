# Engineering Plan #122: Luxury Listing Page (Phase 1)

## Metadata
- **Intent**: [intent-122](../intent/intent-122-feat-luxury-villa-ui-improvements.md)
- **Spec**: [spec-122](../specs/spec-122-feat-ui-improvements.md)
- **Mockup**: [Seagrape House listing mockup](https://claude.ai/artifact/6rTt8v5z8QJRjpwqtaWcL9)
- **Branch**: `feat/122-ui-improvements`
- **Baseline Commit**: `8ab9689b`
- **Status**: Approved 2026-11-12 — executing via `/build auto`
- **Scope**: Phase 1 only (spec D8). Phase 2 (Experiences Flow, spec §2.11) gets its own plan once Phase 1 merges.
- **Blast Radius**: ~30 files across 6 crates (`common`, `listing_api`, `web_ui_common`, `web_ui_guest`, `web_ui_admin`) + Playwright. No migrations: all content lives in the existing `listing.listing_details` JSONB.

---

## 1. Dependency Graph

```
T1 bug fixes ─────────────────────────────────────────────────────────────┐
T2 shell grid (risk) ──┬───────────────────────────────┐                  │
T3 details DTO ──┬─ T4 ribbon domain ─┬─ T5 guest ribbon ─┤               │
                 │                    ├─ T6 API ribbon ─ T7 admin ribbon ─┤
                 │                    │                                   │
                 ├─ T8 amenity domain ┬─ T9 guest amenities               │
                 │                    └─ T10 API groups ─ T11 admin groups┤
                 │                                                        │
                 ├─ T12 page layout (needs T2, T5) ─ T13 mosaic + CTA + mobile bar
                 ├─ T14 suites guest ─ T15 suites admin                   │
                 ├─ T16 staff & services guest ─ T17 staff admin          │
                 ├─ T18 policies + location guest ─ T19 policies/nearby admin
                 └─ T20 guestbook fallback guest ─ T21 guestbook admin    │
T22 footer ───────────────────────────────────────────────────────────────┤
T23 ui.js editor enhancements (needs T7, T11) ────────────────────────────┤
T24 E2E seed data ─ T25 Playwright suite ─ T26 full gate ◄────────────────┘
```

Risk-first ordering: **T2** (shell grid touches every guest page) and **T3/T4** (the shared data contract) go first. Guest rendering for each section lands before its admin editor, so every task is visible in the browser via seeded JSON.

---

## 2. Tasks

### Foundation

- [x] **Task 1: Fix double currency prefix and missing bed count**
  - **Scope:** S
  - **Description:** `ListingSummary::price_text` renders `"{symbol}{price}"` when the display currency has an unambiguous symbol, otherwise a disambiguated form (`US$` style for shared `$`, ISO code for none). `listing_detail.rs` omits the beds fact when `beds <= 0`; `ListingSummary::specs()` likewise.
  - **Files Touched:**
    - `web_ui_common/src/components/domain/listing_card.rs`
    - `web_ui_guest/src/pages/listing_detail.rs`
    - `web_ui_common/tests/components.rs`
  - **Acceptance Criteria:**
    - [ ] USD listing → `"$2,400"`; no `"USD $"` anywhere on cards or detail page.
    - [ ] `beds == 0` renders no beds fact; never `"- beds"` / `"0 beds"`.
    - [ ] Existing `display-currency` E2E expectations updated if they assert the old string.
  - **Verification Command:** `cargo test -p web_ui_common --test components && cargo test -p web_ui_guest`
  - **Dependencies:** None

- [x] **Task 2: Guest shell content grid with `.full-bleed` track (D10)**
  - **Scope:** S
  - **Description:** Add `.page-grid` / `.full-bleed` utilities to `guest.css`; change `guest_chrome`'s `<main>` from `mx-auto max-w-[1540px] px-6 py-12` to `page-grid`. No page opts in yet, so every page must look identical.
  - **Files Touched:**
    - `web_ui_common/src/style/guest.css`
    - `web_ui_common/src/shell_guest.rs`
    - `web_ui_common/tests/shells.rs`
  - **Acceptance Criteria:**
    - [ ] `<main>` carries `page-grid`; content column max width 1540px with 24px gutters (unchanged from today).
    - [ ] `theme_tokens` test still passes (no token changes).
    - [ ] Home, listings, listing detail, about, bookings: no visual change and no horizontal scroll at 390px and 1280px.
  - **Verification Command:** `cargo test -p web_ui_common --test shells --test theme_tokens && pnpm test:e2e:guest -- home.spec.ts listings.spec.ts static-pages.spec.ts theme.spec.ts`
  - **Dependencies:** None

- [x] **Task 3: `StructuredListingDetails` with per-section lenient parsing**
  - **Scope:** S
  - **Description:** New `common::listing_details` module with the §3.1 DTOs (excluding Phase 2 experiences behaviour). `StructuredListingDetails::from_json(Option<&Value>)` parses **each section independently**, so one malformed section is dropped with a `tracing::warn!` (listing id, section key) and the rest still render.
  - **Files Touched:**
    - `common/src/listing_details/mod.rs` (new)
    - `common/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] `None` / `{}` → `default()`.
    - [ ] Malformed `suites` with valid `amenity_groups` → suites `None`, groups parsed.
    - [ ] Unknown keys (e.g. legacy `amenities`, future keys) are ignored on read.
    - [ ] Every public item has a `///` doc comment (RS-D1001).
  - **Verification Command:** `cargo test -p common listing_details`
  - **Dependencies:** None

- [x] **Task 4: Ribbon domain — `RibbonIcon`, `RibbonItem`, `Ribbon`**
  - **Scope:** S
  - **Description:** `Ribbon::try_from(Vec<RibbonItem>) -> Result<Ribbon, RibbonError>` (3–6 items, value ≤ 8, label ≤ 24, non-blank); `Ribbon::fallback(bedrooms, full_baths, half_baths, max_guests)`; `Ribbon::for_listing(&StructuredListingDetails, &ListingResponse)`. Bathrooms value = `full + 0.5 × half` computed with `Decimal` and trimmed (`5 + 1 half → "5.5"`, `5 + 2 half → "6"`).
  - **Files Touched:**
    - `common/src/listing_details/ribbon.rs` (new)
    - `common/src/listing_details/mod.rs`
  - **Acceptance Criteria:**
    - [ ] Unit tests for every `RibbonError` variant and boundaries (2/3/6/7 items, 8/9-char value, 24/25-char label).
    - [ ] Fallback yields 3 items; invalid stored ribbon → fallback with a warning.
    - [ ] Unknown icon string fails that section's parse (handled by T3) → fallback.
  - **Verification Command:** `cargo test -p common ribbon`
  - **Dependencies:** Task 3

### Highlights Ribbon (vertical slice)

- [x] **Task 5: Guest highlights ribbon component on the listing page**
  - **Scope:** M
  - **Description:** `highlights_ribbon` component (`data-testid="highlights-ribbon"`, `<section aria-label="Villa at a glance"><ul>`), inline SVG per `RibbonIcon`, tokens per spec §2.10. Placed on `listing_detail.rs` as a `.full-bleed` block; replaces `<p>(summary.specs())</p>`.
  - **Files Touched:**
    - `web_ui_common/src/components/highlights_ribbon.rs` (new)
    - `web_ui_common/src/components/mod.rs`
    - `web_ui_guest/src/pages/listing_detail.rs`
    - `web_ui_common/tests/components.rs`
  - **Acceptance Criteria:**
    - [ ] Renders N `<li>` for 3–6 items with escaped text; icons `aria-hidden`.
    - [ ] Grid: N columns desktop, ≤ 3 under 860px, 2 under 600px.
    - [ ] Legacy listing (no `ribbon`) shows bedrooms / bathrooms / max guests.
    - [ ] Legible in light, `.dim`, `.dark`.
  - **Verification Command:** `cargo test -p web_ui_common --test components ribbon && cargo test -p web_ui_guest`
  - **Dependencies:** Tasks 2, 4

- [x] **Task 6: `listing_api` rejects invalid ribbons**
  - **Scope:** S
  - **Description:** Validate `listing_details.ribbon` (when present) in `NewListingRequest` / update validation via `Ribbon::try_from`; map to `400 { "code": "INVALID_RIBBON", "message": <RibbonError> }` with `tracing::warn!`.
  - **Files Touched:**
    - `common/src/models.rs` (validation hook on listing requests)
    - `app_api/listing_api/src/apis.rs`
    - `app_api/listing_api/src/apis_test.rs`
  - **Acceptance Criteria:**
    - [ ] Create and update with 2 items → 400 `INVALID_RIBBON`; with 3 valid → 2xx and stored.
    - [ ] Payload without `ribbon` unaffected.
  - **Verification Command:** `cargo test -p listing_api ribbon` (needs local Postgres via devenv)
  - **Dependencies:** Task 4

- [x] **Task 7: Admin "Highlights ribbon" fieldset (no-JS)**
  - **Scope:** M
  - **Description:** 6 fixed row slots (icon `<select>`, value, label) in the listing form; empty rows ignored; errors name the row; saved via the existing `listing_details` merge path (other keys preserved).
  - **Files Touched:**
    - `web_ui_admin/src/pages/listings/form.rs`
    - `web_ui_admin/tests/listing_ribbon.rs` (new)
  - **Acceptance Criteria:**
    - [ ] Edit form pre-fills saved items; legacy listing shows empty slots and a hint that the fallback is in use.
    - [ ] Half-filled row → form re-renders with "Item N needs a value and a label." and keeps input.
    - [ ] Saving preserves `amenities` and other keys.
  - **Verification Command:** `cargo test -p web_ui_admin --test listing_ribbon`
  - **Dependencies:** Task 6

### Amenity Groups (vertical slice)

- [x] **Task 8: Amenity group domain**
  - **Scope:** S
  - **Description:** `AmenityIcon`, `AmenityGroup`, `AmenityGroupError`, `validate_amenity_groups`, `amenity_groups_for` (legacy flat list → one "Amenities" group), `SUGGESTED_AMENITY_GROUPS`. Move `MAX_AMENITIES` / `MAX_AMENITY_CHARS` from `form.rs` into `common` and re-export.
  - **Files Touched:**
    - `common/src/listing_details/amenities.rs` (new)
    - `common/src/listing_details/mod.rs`
    - `web_ui_admin/src/pages/listings/form.rs` (use moved constants)
  - **Acceptance Criteria:**
    - [ ] Unit tests for every `AmenityGroupError` variant and boundaries (0/1/6/7 groups, 20/21 items, 60/61 chars, total 50/51).
    - [ ] Legacy flat list maps to a single group; empty → empty vec.
    - [ ] Existing `listing_amenities` admin test still passes.
  - **Verification Command:** `cargo test -p common amenit && cargo test -p web_ui_admin --test listing_amenities`
  - **Dependencies:** Task 3

- [x] **Task 9: Guest amenity group cards**
  - **Scope:** S
  - **Description:** `amenity_group` component; listing page `#amenities` section replaces "What this place offers" facts list. Empty groups skipped; section omitted when none.
  - **Files Touched:**
    - `web_ui_common/src/components/amenity_group.rs` (new)
    - `web_ui_common/src/components/mod.rs`
    - `web_ui_guest/src/pages/listing_detail.rs`
    - `web_ui_common/tests/components.rs`
  - **Acceptance Criteria:**
    - [ ] Groups render in stored order with title, icon, check list.
    - [ ] Legacy listing renders one "Amenities" card.
  - **Verification Command:** `cargo test -p web_ui_common --test components amenit`
  - **Dependencies:** Task 8

- [x] **Task 10: `listing_api` rejects invalid amenity groups**
  - **Scope:** XS
  - **Description:** Same pattern as Task 6 for `amenity_groups` → `400 INVALID_AMENITY_GROUPS`.
  - **Files Touched:**
    - `common/src/models.rs`
    - `app_api/listing_api/src/apis_test.rs`
  - **Acceptance Criteria:**
    - [ ] Group without title → 400; valid groups → stored.
  - **Verification Command:** `cargo test -p listing_api amenity_groups`
  - **Dependencies:** Tasks 6, 8

- [x] **Task 11: Admin "Amenities" groups fieldset**
  - **Scope:** M
  - **Description:** Replace the flat amenities textarea with 6 group slots (title, icon, items textarea). New listings prefilled with the 4 suggested titles. Legacy flat list loads as one "Amenities" group; on save, `amenity_groups` is written and the `amenities` key removed.
  - **Files Touched:**
    - `web_ui_admin/src/pages/listings/form.rs`
    - `web_ui_admin/tests/listing_amenities.rs`
  - **Acceptance Criteria:**
    - [ ] Create form shows 4 suggested group titles.
    - [ ] Legacy listing round-trips into one group and loses the `amenities` key after save.
    - [ ] Errors name the group ("Group 2 needs a title.").
  - **Verification Command:** `cargo test -p web_ui_admin --test listing_amenities`
  - **Dependencies:** Task 10

### Page Layout

- [x] **Task 12: Hero, section nav and intro**
  - **Scope:** M
  - **Description:** Restructure `listing_detail.rs` as direct `.page-grid` children: full-bleed hero (first photo, `fetchpriority="high"`, location pill, rating, "View all N photos" → existing `gallery-dialog`), sticky section nav (links only for non-empty sections; price + "Check availability" anchor on desktop), intro (pull-quote, description split on blank lines, CTAs, host line, framed photo). Booking card becomes sticky beside the body, starting below the ribbon.
  - **Files Touched:**
    - `web_ui_guest/src/pages/listing_detail.rs`
    - `web_ui_common/src/components/listing_hero.rs` (new)
    - `web_ui_common/src/components/section_nav.rs` (new)
    - `web_ui_common/src/components/mod.rs`
  - **Acceptance Criteria:**
    - [ ] Hero meets the header with no gap; no horizontal scroll at 390px.
    - [ ] Listing without photos renders a title band instead of an empty hero.
    - [ ] Existing `listing-detail.spec.ts` (reserve link, quote panel, calendar) still passes.
  - **Verification Command:** `cargo test -p web_ui_guest && pnpm test:e2e:guest -- listing-detail.spec.ts`
  - **Dependencies:** Tasks 2, 5

- [x] **Task 13: Photo mosaic, closing CTA and mobile booking bar**
  - **Scope:** M
  - **Description:** 5-tile `photo_mosaic` replacing the top photo grid (reuses `gallery_layout` tests or replaces them), closing "Plan your stay" band, fixed mobile booking bar (< 1024px) with safe-area padding; full booking form renders inline after the body on mobile.
  - **Files Touched:**
    - `web_ui_common/src/components/photo_mosaic.rs` (new)
    - `web_ui_common/src/components/mod.rs`
    - `web_ui_guest/src/pages/listing_detail.rs`
  - **Acceptance Criteria:**
    - [ ] Mosaic handles 1–5+ photos without empty cells; last tile shows "All N photos".
    - [ ] Lightbox still opens from mosaic tiles.
    - [ ] Mobile bar visible < 1024px, hidden ≥ 1024px.
  - **Verification Command:** `cargo test -p web_ui_guest gallery && pnpm test:e2e:guest -- listing-detail.spec.ts`
  - **Dependencies:** Task 12

### Content Sections

- [x] **Task 14: Guest suites section**
  - **Scope:** S
  - **Description:** `suite_card` component; `#suites` section, first 4 shown, rest behind `<details>` "Show all N suites". Suite image resolved from the listing's own images by `image_id` (Decision 1); unknown id → no image.
  - **Files Touched:**
    - `web_ui_common/src/components/suite_card.rs` (new)
    - `web_ui_common/src/components/mod.rs`
    - `web_ui_guest/src/pages/listing_detail.rs`
    - `web_ui_common/tests/components.rs`
  - **Acceptance Criteria:**
    - [ ] Card shows bed configuration, capacity, en-suite, feature chips, lazy image with hover zoom.
    - [ ] No suites → section and nav link omitted.
  - **Verification Command:** `cargo test -p web_ui_common --test components suite`
  - **Dependencies:** Tasks 3, 12

- [ ] **Task 15: Admin suites fieldset**
  - **Scope:** M
  - **Description:** Fixed suite slots (name, bed configuration, max guests, en-suite text, features one per line, image picker from listing images). Validation in `common` (lengths, max 12 suites).
  - **Files Touched:**
    - `common/src/listing_details/mod.rs`
    - `web_ui_admin/src/pages/listings/form.rs`
    - `web_ui_admin/tests/listing_suites.rs` (new)
  - **Acceptance Criteria:**
    - [ ] Saved suites round-trip and render on the guest page.
    - [ ] Image picker lists only this listing's images.
  - **Verification Command:** `cargo test -p web_ui_admin --test listing_suites`
  - **Dependencies:** Task 14

- [ ] **Task 16: Guest staff & services section**
  - **Scope:** S
  - **Description:** `#services` two columns: included checklist + note; add-ons with `Decimal` price formatted in the display currency (or "On request"). "Ask us to arrange" links to the existing enquiry flow with the topic pre-filled.
  - **Files Touched:**
    - `web_ui_guest/src/pages/listing_detail.rs`
    - `web_ui_guest/src/pages/enquiries.rs` (accept `topic` query param)
    - `web_ui_guest/tests/smoke.rs`
  - **Acceptance Criteria:**
    - [ ] Add-on prices use `Decimal` and the existing money formatter; no floats.
    - [ ] Enquiry form opens with the add-on name in the message.
  - **Verification Command:** `cargo test -p web_ui_guest`
  - **Dependencies:** Tasks 3, 12

- [ ] **Task 17: Admin staff & services fieldset**
  - **Scope:** S
  - **Description:** Included items textarea, note input, add-on slots (name, price, unit).
  - **Files Touched:**
    - `web_ui_admin/src/pages/listings/form.rs`
    - `web_ui_admin/tests/listing_services.rs` (new)
  - **Acceptance Criteria:**
    - [ ] Price parsed as `Decimal` (≤ 2 dp, reuses `validate_fee`); invalid price names the row.
  - **Verification Command:** `cargo test -p web_ui_admin --test listing_services`
  - **Dependencies:** Task 16

- [ ] **Task 18: Guest policies and location sections**
  - **Scope:** S
  - **Description:** `#policies` `<dl>` from `HousePolicies`; `#location` area text + approximate-area illustration + `NearbyPlace` drive-time list (minutes → "10 min" / "1h 15m").
  - **Files Touched:**
    - `web_ui_guest/src/pages/listing_detail.rs`
    - `common/src/listing_details/mod.rs` (drive-time formatter + tests)
  - **Acceptance Criteria:**
    - [ ] Empty policies / nearby → sections and nav links omitted.
    - [ ] Drive-time formatter unit-tested (0, 59, 60, 75, 120).
  - **Verification Command:** `cargo test -p common drive_time && cargo test -p web_ui_guest`
  - **Dependencies:** Tasks 3, 12

- [ ] **Task 19: Admin policies and nearby fieldsets**
  - **Scope:** S
  - **Description:** Policy text inputs; nearby slots (name, drive minutes).
  - **Files Touched:**
    - `web_ui_admin/src/pages/listings/form.rs`
    - `web_ui_admin/tests/listing_policies.rs` (new)
  - **Acceptance Criteria:**
    - [ ] Round-trip preserves other `listing_details` keys.
  - **Verification Command:** `cargo test -p web_ui_admin --test listing_policies`
  - **Dependencies:** Task 18

- [ ] **Task 20: Guest reviews / guestbook fallback**
  - **Scope:** S
  - **Description:** Verified reviews unchanged. With no reviews: up to 4 `curated_testimonials` with the "From the villa guestbook…" notice; neither → section omitted. "No reviews yet" removed.
  - **Files Touched:**
    - `web_ui_guest/src/pages/listing_detail.rs`
    - `web_ui_guest/tests/smoke.rs`
  - **Acceptance Criteria:**
    - [ ] Guestbook quotes never render alongside verified reviews.
    - [ ] Notice text always present with quotes.
  - **Verification Command:** `cargo test -p web_ui_guest && pnpm test:e2e:guest -- reviews.spec.ts`
  - **Dependencies:** Tasks 3, 12

- [ ] **Task 21: Admin guestbook fieldset**
  - **Scope:** S
  - **Description:** Up to 4 quote slots (guest name, home city, stay month, rating 1–5, quote ≤ 400 chars).
  - **Files Touched:**
    - `web_ui_admin/src/pages/listings/form.rs`
    - `web_ui_admin/tests/listing_guestbook.rs` (new)
  - **Acceptance Criteria:**
    - [ ] Rating outside 1–5 rejected with a row-specific error.
  - **Verification Command:** `cargo test -p web_ui_admin --test listing_guestbook`
  - **Dependencies:** Task 20

### Shell, Enhancements, Tests

- [ ] **Task 22: Four-column guest footer and placeholder policy pages**
  - **Scope:** M
  - **Description:** Brand, Explore, Policies, Concierge columns; concierge contact from typed config (no raw `env::var` literals, RS-W1015), column hidden when unset. Add `/terms`, `/cancellation-policy`, `/privacy-policy`, `/house-rules` as static pages with a visible "Draft — placeholder text" notice. `/experiences` link hidden until Phase 2.
  - **Files Touched:**
    - `web_ui_common/src/shell_guest.rs`
    - `web_ui_common/tests/shells.rs`
    - `web_ui_guest/src/pages/policies.rs` (new)
    - `web_ui_guest/src/pages/mod.rs`
    - `playwright/e2e/guest-portal/static-pages.spec.ts`
  - **Acceptance Criteria:**
    - [ ] 4 columns ≥ 860px, 2 below; no dead links.
    - [ ] Each policy page returns 200 and shows the draft notice.
  - **Verification Command:** `cargo test -p web_ui_common --test shells && pnpm test:e2e:guest -- static-pages.spec.ts`
  - **Dependencies:** Task 2

- [ ] **Task 23: `ui.js` editor and nav enhancements**
  - **Scope:** M
  - **Description:** Data-attribute driven: section-nav active link (IntersectionObserver), up/down reorder for ribbon and amenity group slots, ribbon "Prefill from listing facts", live ribbon preview. All CSP-safe; forms keep working with JS off.
  - **Files Touched:**
    - `web_ui_common/src/assets/ui.js`
    - `playwright/e2e/ui-js/ui-js.spec.ts`
  - **Acceptance Criteria:**
    - [ ] Reorder changes the submitted slot order.
    - [ ] No inline scripts; CSP tests (`security.spec.ts`) pass.
  - **Verification Command:** `pnpm test:e2e -- ui-js.spec.ts security.spec.ts`
  - **Dependencies:** Tasks 7, 11, 12

- [ ] **Task 24: E2E seed data for structured details**
  - **Scope:** S
  - **Description:** Extend `playwright/e2e/setup/db-seed.ts` so one known listing has full structured content (ribbon, groups, suites, services, policies, nearby, guestbook) and one stays legacy. Add both to `fixtures/test-data.ts`.
  - **Files Touched:**
    - `playwright/e2e/setup/db-seed.ts`
    - `playwright/e2e/fixtures/test-data.ts`
  - **Acceptance Criteria:**
    - [ ] Seed is idempotent; existing E2E suites unaffected.
  - **Verification Command:** `pnpm test:e2e:guest -- listing-detail.spec.ts`
  - **Dependencies:** Task 3

- [ ] **Task 25: Playwright coverage for the new page**
  - **Scope:** M
  - **Description:** Spec §6.3 items 1–6: section presence, responsive (390 / 1280), themes, host ribbon round-trip, host amenity-groups round-trip, other guest pages unchanged.
  - **Files Touched:**
    - `playwright/e2e/guest-portal/listing-detail.spec.ts`
    - `playwright/e2e/admin-portal/listings-admin.spec.ts`
    - `playwright/e2e/guest-portal/theme.spec.ts`
  - **Acceptance Criteria:**
    - [ ] All new tests pass in chromium and firefox projects.
  - **Verification Command:** `pnpm test:e2e:guest && pnpm test:e2e:admin`
  - **Dependencies:** Tasks 13–24

- [ ] **Task 26: Full quality gate**
  - **Scope:** XS
  - **Description:** Full workspace run once at the end (targeted tests during iteration).
  - **Acceptance Criteria:**
    - [ ] `cargo clippy --workspace --all-targets` clean; no new `#[allow]`.
    - [ ] No `.unwrap()` / `.expect()` in non-test code added by this branch.
    - [ ] Full Rust and Playwright suites green.
  - **Verification Command:** `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && pnpm test:e2e`
  - **Dependencies:** All

---

## 3. Spec Deviations Found While Planning

| Spec says | Reality | Plan |
|---|---|---|
| Seeds in `db_core/seeds/` for the 5 villas | `db_core/seeds/` holds only `001_admin_user.sql`; E2E data comes from `playwright/e2e/setup/db-seed.ts` | Task 24 seeds E2E data there. Real villa content is entered through the new admin editors (no SQL seed for production content). |
| E2E target `/listings/the-great-house` | No such listing; fixtures use e.g. `the-courtyard-studio-new-kingston` | Task 24 adds a structured-content fixture listing. |
| `SuiteItem.image_url: Option<String>` | Free URLs bypass the GCS variant pipeline and may be blocked by CSP `img-src` | Store an image **id** from the listing's own images (Open Question 1). |
| Footer links to `/terms`, `/cancellation-policy`, `/privacy-policy`, `/house-rules` | Only `/about` exists in `web_ui_guest` | Footer links only to existing pages until content exists (Open Question 2). |

---

## 4. Resolved Decisions (2026-11-12)

| # | Question | Decision |
|---|---|---|
| 1 | Suite images | `SuiteItem.image_id: Option<Uuid>` referencing one of the listing's uploaded images. |
| 2 | Policy pages | Add `/terms`, `/cancellation-policy`, `/privacy-policy`, `/house-rules` with clearly marked **draft placeholder** text (Task 22). Owner replaces copy before launch. |
| 3 | Concierge contact | Typed config with no defaults; the footer's Concierge column hides when unset. |
| 4 | Admin editors | All new fieldsets go in the **existing listing create/edit form**. |
| 5 | Execution | `/build auto` approved for T1–T26. |
