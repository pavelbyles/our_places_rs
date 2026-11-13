# Intent: Luxury Villa Guest UI & Detail Page Improvements (Katablanca Design Inspiration)

## Metadata
- **Author**: Originator (via GitHub Issue #122, @pavelbyles)
- **Date**: 2026-11-11 (aligned with spec 2026-11-12)
- **Status**: Review
- **Target Area**: `web_ui_guest` (listing detail), `web_ui_common` (guest shell grid, hero, highlights ribbon, suite cards, amenity groups, footer), `web_ui_admin` (listing form: ribbon and amenity group editors), `common` (typed `listing_details` DTOs, currency formatting), `db_core` (`listing_details` JSONB seeds)
- **Source Issue**: https://github.com/pavelbyles/our_places_rs/issues/122
- **Spec**: [spec-122](../specs/spec-122-feat-ui-improvements.md)
- **Design Inspiration**: [Villa KATABLANCA](http://katablanca.com/the-villa), `/suites`, `/experiences`
- **Design Mockup**: [Seagrape House listing mockup](https://claude.ai/artifact/6rTt8v5z8QJRjpwqtaWcL9)

---

## 1. Problem Statement & Motivation
Our Places is a luxury short-term rental platform representing high-end Caribbean properties ($2,000–$4,000+/night in Jamaica). The current guest portal listing page falls short of that positioning:

1. **Brand & Content Gap**: The listing page shows a generic 2-line description, a plain inline string ("12 Guests · 5 Beds · 5 Baths"), a flat amenity list, and no mention of staff services, even though Jamaican luxury villas are distinguished mainly by full staffing (private chef, butler, housekeeping).
2. **No Host-Controlled Highlights**: Each villa sells on different points ("Direct beach access", "24/7 security", "5.5 bathrooms"), but hosts have no way to put them front and centre.
3. **Missing Suite-Level Breakdown**: Groups booking a 5–6 bedroom villa need to see individual bedroom configurations (beds, en-suite, views) before reserving.
4. **Empty Review State Friction**: *"No reviews yet. Reviews come only from guests who stayed"* on a marquee villa dampens conversion.
5. **Lack of Surrounding Context**: International guests need drive times to attractions and airports, and to know what the concierge can arrange.
6. **Formatting Bugs**: Double currency symbols (`USD $2,400/night`) and missing bed counts (`- beds`) erode trust.

Villa Katablanca (`katablanca.com/the-villa`) shows a proven aesthetic for Jamaican villas: editorial full-bleed imagery, a bold ribbon of key items, distinct suite breakdowns, grouped amenities, and staff/experience transparency.

---

## 2. Proposed Outcome (The Vision)
Transform the listing detail page (`/listings/{slug}`) into an editorial luxury experience inspired by Katablanca, **using the existing Shoreline Mist theme unchanged**, backed by typed Topcoat SSR components and host-editable structured content.

| Katablanca Design Element | Current Our Places State | Target State |
|---|---|---|
| **Highlights Ribbon** | Tiny inline text (`text-sm text-muted-foreground`) | Full-width ribbon of **3–6 host-entered items** (icon, value, label — e.g. "6 / Bedrooms", "Direct / Beach access"), in `--primary` / `--primary-foreground`. Falls back to bedrooms, bathrooms, max guests when the host hasn't set it. |
| **Editorial Hero & Narrative** | Title above a photo grid | Full-bleed hero photo with location pill and rating, sticky in-page section nav, intro with italic serif pull-quote and multi-paragraph story. |
| **Room-by-Room Suites** | Generic bedroom count | Suite cards with photo, bed configuration, capacity, en-suite and view chips. |
| **Amenity Groups** | Flat 2-column list | **Host-configured groups per listing** (title, icon, items). New listings start with 4 suggested groups: Villa, Health & Wellness, Family, Common Areas. |
| **Staff & Services** | Unmentioned | "Included with your stay" vs. priced "Concierge add-ons", with "Ask us to arrange" opening the enquiry dialog. |
| **Policies & Location** | Not shown | House-policy summary (check-in/out, minimum stay, cancellation, deposit, rules) and a drive-time list to attractions and airports. |
| **Reviews Fallback** | "No reviews yet" | Labelled guestbook quotes ("From the villa guestbook") shown only when there are no verified reviews. |
| **Curated Experiences** | None | "Discover the area" cards with drive times (Phase 2: host curation and guest requests). |
| **Footer** | Single line | 4-column footer: brand, explore, policies, concierge contact. |
| **Currency & Spec Formatting** | `USD $2,400`, `- beds` | `$2,400 / night`; beds omitted when unknown. |

---

## 3. Scope Boundaries

### Phase 1 (In Scope)
- **Listing page redesign** (`web_ui_guest/src/pages/listing_detail.rs`): hero, section nav, intro, highlights ribbon, suites, amenity groups, staff & services, policies, reviews/guestbook, location, photo mosaic, closing CTA, mobile booking bar.
- **Guest shell** (`web_ui_common/src/shell_guest.rs`): `<main>` becomes a content grid so sections can run full width without changing other pages; 4-column footer.
- **Admin listing form** (`web_ui_admin`): editors for the ribbon (3–6 items) and amenity groups (1–6 groups), plus suites, staff services, policies and nearby places. Works without JS; `ui.js` adds reordering and preview.
- **Data model** (`common`): typed `StructuredListingDetails` over the existing `listing_details` JSONB, with validation shared by the admin form and `listing_api`.
- **Seeds** (`db_core`): structured content for the 5 Jamaican villas.
- **Bug fixes**: double currency prefix; missing bed count.
- **Tests**: unit tests for validation/fallbacks, component tests, Playwright for layout, themes, responsiveness and host round-trips.

### Phase 2 (In Scope, separate delivery)
- Experiences Flow: admin catalog and per-listing toggles/pricing, `/experiences` page, checkout "Enhance your stay" step, experience requests from `/bookings/{id}`.

### Out of Scope / Non-Goals
- New colours or theme changes: the Shoreline Mist light / `.dim` / `.dark` palette stays as is.
- Booking state machine, payment processing, or tax engine changes.
- iCal sync (handled in #58).
- External CMS: content stays in `listing.listing_details` JSONB.

---

## 4. Affected Systems & Stakeholders

| Component | Responsibility / Changes |
|---|---|
| `common` | `StructuredListingDetails` DTOs (`RibbonItem`, `AmenityGroup`, `SuiteItem`, `StaffServices`, `HousePolicies`, `NearbyPlace`, …), validation, currency display fix. |
| `db_core` | Seed `listing_details` for the 5 Jamaican properties. |
| `web_ui_common` | `.page-grid` / `.full-bleed` utilities, `listing_hero`, `section_nav`, `highlights_ribbon`, `suite_card`, `amenity_group`, `photo_mosaic`, footer; `ui.js` enhancements. |
| `web_ui_guest` | `listing_detail.rs` layout; Phase 2 experiences pages. |
| `web_ui_admin` | Listing form editors for ribbon, amenity groups and other structured content; Phase 2 experiences page. |
| `listing_api` | Re-validates structured details (`INVALID_RIBBON`, `INVALID_AMENITY_GROUPS`). |
| **Guests** | Clear, high-trust view of rooms, amenities, staff, policies and location. |
| **Hosts** | Control over what each listing highlights and how amenities are grouped. |

---

## 5. Constraints & Invariants
1. **Financial Precision**: all prices (add-ons, experiences) use `rust_decimal::Decimal`.
2. **Zero Panic Policy**: no `.unwrap()` / `.expect()` in production or view code. Invalid stored content logs a warning and falls back; it never breaks the page.
3. **CSP**: no inline scripts; dialogs and enhancements via `ui.js` data attributes; no third-party map embeds. Host-picked icons come from closed enums, never uploaded SVG.
4. **Performance**: responsive `<picture>` variants, `fetchpriority="high"` hero, lazy-loaded imagery below it, fixed aspect ratios (no layout shift); Cloud Run p50 < 300ms.
5. **Theme**: Shoreline Mist tokens only (light, `.dim`, `.dark`). No hard-coded hex colours. In `.dark` the ribbon renders as a light band; this is accepted.

---

## 6. Success Metrics & Acceptance Signals
- [ ] Listing page renders the hero, section nav, ribbon and full-width bands edge to edge; other guest pages are visually unchanged.
- [ ] Hosts can set 3–6 ribbon items per listing; listings without them show the facts-based fallback.
- [ ] Hosts can configure amenity groups per listing; legacy flat lists still render.
- [ ] Suites, staff & services, policies and location sections render, and are omitted when empty.
- [ ] Guestbook quotes appear only without verified reviews and are labelled.
- [ ] `USD $2,400` → `$2,400 / night`; no `- beds`.
- [ ] No horizontal scroll at 390px; legible in light, `.dim` and `.dark`.
- [ ] All Playwright guest/admin tests pass; `cargo clippy --workspace --all-targets` is clean.

---

## 7. Resolved Decisions
| Question | Decision |
|---|---|
| Storage for structured content | `listing.listing_details` JSONB with typed Serde DTOs in `common`; no new tables. |
| Ribbon content | Host-entered per listing (3–6 items), facts-based fallback. Separate highlight badges dropped. |
| Amenity groups | Host-configured per listing (1–6 groups); 4 suggested groups prefilled on new listings. |
| Theme | Keep Shoreline Mist; map new elements to existing tokens. Dark-mode light ribbon accepted. |
| Full-width sections | Shell `<main>` becomes a content grid with an opt-in `.full-bleed` track. |
| Concierge / experience action | Opens the existing enquiry dialog pre-filled with the topic. |
| Testimonials when there are no reviews | Per-listing `curated_testimonials`, always labelled "From the villa guestbook". |
| Delivery | Phase 1 listing page; Phase 2 Experiences Flow. |

---

## 8. Addendum: Photo Names (agreed 2026-11-12, same branch and PR)

- **Outcome**: Listing photos get an optional host-set name (≤ 80 chars, trimmed, duplicates allowed) in a nullable `listing_image.name` column (one new migration). On the admin edit page, each processed photo has a textbox below it (placeholder "Photo N") that saves via HTMX on blur or Save, with a toast, and works without JS. Saving an empty box clears the name. A new `listing_api` endpoint updates it behind `require_listing_manager` and refuses invalid names with a 400 error. Image responses carry the name.
- **Where names appear**: suite photo picker (name or "Photo N"); guest alt text on the photo's images (name, or "{listing name}, photo N"); a visible caption **only in the full-size viewer**, only when a name is set.
- **User**: hosts picking suite photos and describing pictures; guests viewing photos full size, including screen-reader users.
- **Why now**: the #122 suite photo picker only offers "Photo 1…12".
- **Success criteria**: a name survives reload and shows in the textbox and picker; the full-size viewer shows the caption and alt text matches it; the gallery dialog, mosaic and hero show no caption; clearing restores "Photo N" and the fallback alt; names over 80 chars are refused; non-owners get 403/404; unit, API and Playwright tests cover these.
- **Binding constraints**: one new migration only; compile-time SQLx with refreshed offline data; no `unwrap`/`expect` in production code; CSP-safe, with behaviour wired through `ui.js` data attributes.
- **Explicitly out of scope**: visible captions anywhere but the full-size viewer; reordering or choosing the main photo; bulk rename; naming during upload; translated or auto-generated names; separate name and description fields.
