# Spec 122: Luxury Villa Guest UI Upgrades & Experiences Flow

> Driven by GitHub Issue #122. Intent: [intent-122](../intent/intent-122-feat-luxury-villa-ui-improvements.md). Design Inspiration: [Villa KATABLANCA](http://katablanca.com/the-villa), `/suites`, `/experiences`. Design mockup: [Seagrape House listing mockup](https://claude.ai/artifact/6rTt8v5z8QJRjpwqtaWcL9) (clickable; includes the host ribbon editor).

---

## Overview

Our Places is an isomorphic Rust short-term rental platform for luxury villas in Jamaica. This specification modernizes the public guest listing experience (`/listings/{slug}`) with editorial layout patterns from Villa Katablanca, introduces structured luxury content (a **host-entered highlights ribbon**, grouped amenities, room-by-room suites, staff transparency, house policies, location drive times), enriches the site footer with legal/contact links, and establishes an **Experiences Flow** enabling hosts to curate local excursions and guests to request them.

The work ships in two phases (D8): **Phase 1** is the listing page, ribbon, footer and bug fixes; **Phase 2** is the Experiences Flow.

**Personas**:
- **Guest**: Scans the villa's ribbon of key items, browses suites, amenities, staff and policies, reviews drive times, and (Phase 2) requests curated local experiences for their stay.
- **Host**: Sets the ribbon items for each listing, manages suites, grouped amenities and staff services, and (Phase 2) activates/prices local experiences in the admin studio.
- **Admin**: Curates the global experiences catalog, oversees listing quality, and reviews experience requests across all bookings.

---

## 1. Key Design Decisions

| # | Decision | Choice | Rationale |
|---|---|---|---|
| **D1** | **Attributes Storage** | Structured `listing_details` JSONB in PostgreSQL `listing` table. | Avoids heavy table migrations; integrates with existing `ListingResponse.listing_details: Option<serde_json::Value>` and compile-time verified SQLx queries while adding typed Serde schemas in `common`. |
| **D2** | **Amenities Architecture** | **Host-configured groups per listing**: 1–6 groups, each with a title, an icon from a closed set, and a list of items. New listings are prefilled with 4 suggested groups (**Villa**, **Health & Wellness**, **Family**, **Common Areas**) that hosts can rename, reorder, remove or add to. Legacy flat arrays render as one "Amenities" group. | Villas differ (a city apartment has no "Common Areas"; a beach villa may want "Beach & Water"). Same host-controlled pattern as the ribbon. Staffed services (butler, chef, housekeeping) belong in Staff & Services (§2.5), and the suggested groups don't include them so they aren't listed twice. |
| **D3** | **Highlights Ribbon (host-entered)** | The ribbon shows 3–6 items the host enters per listing (`icon`, `value`, `label`). When unset, it is derived from listing facts (bedrooms, bathrooms, max guests). The separate "key highlights" pill badges are **dropped**. | Each villa sells on different points ("Direct / Beach access", "24/7 / Security", "5.5 / Bathrooms"). One host-controlled ribbon covers both stats and qualitative highlights without a second, duplicate component. |
| **D4** | **Experiences Flow** | Host curates in `web_ui_admin`; guest discovers on listing/portal and requests at checkout or in `/bookings/{id}`. | Hosts choose which partner experiences apply to their villa; guests add them with strict row-level ownership checks. |
| **D5** | **Financial Precision** | Experience and add-on pricing stored as `rust_decimal::Decimal`. | Enforces the `CONSTRAINTS.md` rule prohibiting floating-point types for money. |
| **D6** | **Footer Architecture** | Multi-column footer in `web_ui_common::shell_guest`. | Replaces single-line footer with 4 columns: Brand/Address, Explore, Policies (`/terms`, `/cancellation-policy`, `/privacy-policy`), Concierge contact. |
| **D7** | **Theme** | Keep the existing **Shoreline Mist** theme (`web_ui_common/src/style/guest.css`) unchanged. New components use existing tokens only; **no hard-coded hex colours and no new colour tokens**. | Preserves the brand and the light / `.dim` / `.dark` contract. See §2.10 for the token mapping. |
| **D8** | **Phasing** | Phase 1: §2.1–§2.9 and bug fixes. Phase 2: §2.11 Experiences Flow (admin catalog, checkout step, booking requests). | Phase 1 is presentation-only over `listing_details`; Phase 2 adds routes, ownership checks and checkout changes, and deserves its own review. |
| **D10** | **Full-width sections** | The guest shell's `<main>` becomes a named-line CSS grid: a centred content column (same 1540px max and 24px gutters as today) plus a full-width track. Children sit in the content column by default; elements with `.full-bleed` span edge to edge. | Existing pages render unchanged with no per-page edits. Avoids the `width: 100vw` breakout, which adds a horizontal scrollbar on systems with classic scrollbars. |
| **D9** | **Guestbook quotes** | Curated quotes render only when there are no verified reviews, always under a visible "From the villa guestbook" label, never mixed with verified reviews. | Presenting host-supplied quotes as verified reviews is a trust and consumer-protection risk. |

---

## 2. Requirements

### 2.1. Page Layout (`/listings/{slug}`)

Top to bottom (see mockup):

| # | Block | Width | Notes |
|---|---|---|---|
| 1 | **Hero** | Full-bleed | First listing photo, `clamp(380px, 62vh, 600px)` tall, bottom gradient overlay. Location pill (tracked uppercase), serif title, rating + review count, "View all N photos" button opening the existing `gallery-dialog`. Image uses `responsive_image` with `fetchpriority="high"`. |
| 2 | **Section nav** | Full-bleed, sticky | Anchor links: Overview · Suites · Amenities · Services · Policies · Reviews · Location. Links for empty sections are omitted. Desktop also shows nightly price + "Check availability" (anchor to `#book`). Active link highlighting is a progressive enhancement in `ui.js`; the bar works without JS. |
| 3 | **Intro (Overview)** | Contained, 2 columns | Eyebrow "The villa", italic serif pull-quote (`listing_details.pull_quote`), description split into paragraphs on blank lines, "Explore suites" / "Check availability" pill buttons (anchors), host line. Second column: framed photo. Stacks on < 860px. |
| 4 | **Highlights ribbon** | Full-bleed | §2.2. |
| 5 | **Body** | Contained, `minmax(0,1fr) 380px` | Left: Suites (§2.3), Amenities (§2.4), Staff & Services (§2.5), Policies (§2.6), Reviews / Guestbook (§2.7), Location (§2.8). Right: existing booking card, sticky, starting **below** the ribbon. |
| 6 | **Photo mosaic** | Contained | 5-tile grid (1 large + 4), hover zoom, last tile shows "All N photos". Opens the existing lightbox. Replaces the current top-of-page photo grid. |
| 7 | **Discover the area** | Full-bleed band | Phase 1: hidden unless `listing_details.experiences` is set. Phase 2: §2.11. |
| 8 | **Closing CTA** | Full-bleed | "Plan your stay" band with "Check availability" anchor and concierge contact as selectable text. |

- **Full-bleed blocks** (D10): `guest_chrome`'s `<main>` ([shell_guest.rs](../../web_ui_common/src/shell_guest.rs), currently `mx-auto max-w-[1540px] px-6 py-12`) changes to:
  ```css
  /* web_ui_common/src/style/guest.css */
  .page-grid {
    display: grid;
    grid-template-columns:
      [full-start] minmax(1.5rem, 1fr)
      [content-start] min(1540px - 3rem, 100% - 3rem) [content-end]
      minmax(1.5rem, 1fr) [full-end];
    padding-block: 3rem;
  }
  .page-grid > * { grid-column: content; min-width: 0; }
  .page-grid > .full-bleed { grid-column: full; }
  .page-grid > .full-bleed:first-child { margin-top: -3rem; } /* hero meets the header */
  ```
  The listing page's root becomes a fragment (or `display: contents` wrapper) so its blocks are direct grid children; hero, section nav, ribbon, bands and closing CTA carry `.full-bleed` and centre their own inner content with the same 1540px max. The footer is outside `<main>` and is unaffected. A Playwright check confirms no horizontal scroll at 390px and 1280px, and a visual check confirms other guest pages are unchanged.
- **Mobile (< 1024px)**: the booking card is replaced by a fixed bottom bar (price + "Check dates" anchor to the booking form) that respects `env(safe-area-inset-bottom)`. The full booking form renders inline after the body sections.
- **Section headers**: tracked uppercase eyebrow + serif `h2`, consistent across sections.
- **Buttons**: rounded-full outline pills (`--primary` border/text, fills on hover); "Reserve" is the solid primary pill.

### 2.2. Highlights Ribbon (host-entered)

1. **Content**: 3–6 `RibbonItem`s from `listing_details["ribbon"]`, in host order. Each item is:
   - `icon`: one value from the closed `RibbonIcon` set (bed, bath, guests, pool, beach, ocean, chef, butler, security, wifi, solar, car, gym, star). Icons are our own inline SVGs; hosts never upload SVG.
   - `value`: ≤ 8 characters, e.g. `6`, `5.5`, `Direct`, `24/7`.
   - `label`: ≤ 24 characters, e.g. `Bedrooms`, `Beach access`.
2. **Fallback**: when `ribbon` is absent or empty, derive 3 items from listing fields: bedrooms, bathrooms (full + ½ per half bath, e.g. `5.5`), max guests. A legacy listing always shows a ribbon.
3. **Invalid stored data** (e.g. 2 items, over-length text, unknown icon) never panics: the page logs `tracing::warn!` with the listing id and renders the fallback.
4. **Rendering** (`web_ui_common/src/components/highlights_ribbon.rs`, `data-testid="highlights-ribbon"`):
   - Background `--primary`, text and icons `--primary-foreground`, labels `color-mix(in srgb, var(--primary-foreground) 75%, var(--primary))`.
   - Value in serif display numerals (`clamp(40px, 4.6vw, 64px)`, weight 300); label in tracked uppercase (`0.32em`, 12px); 34px line icon above.
   - Grid: one row of N columns on desktop; max 3 columns < 860px; 2 columns < 600px.
   - Markup: `<section aria-label="Villa at a glance"><ul>` with one `<li>` per item, icons `aria-hidden`.
5. **Admin editor** (`web_ui_admin/src/pages/listings/form.rs`, "Highlights ribbon" fieldset):
   - 6 fixed row slots (icon `<select>`, value input, label input); empty rows are ignored on save. No JS required.
   - Reorder via up/down buttons and "Prefill from listing facts" are progressive enhancements in `ui.js`; without JS the host orders rows by slot.
   - A live preview of the ribbon is shown under the rows (JS enhancement).
   - Saved into `listing_details["ribbon"]` via the existing merge path (same as `amenities`, `form.rs` merge), so other keys are preserved.
   - Validation lives in `common` (shared by admin form and `listing_api`): 3–6 complete items; a half-filled row is an error naming the row ("Item 4 needs a value and a label."); length caps as above. `listing_api` rejects invalid payloads with `400 { "code": "INVALID_RIBBON", ... }`.

### 2.3. Room-by-Room Suites

1. Section `#suites` renders when `listing_details["suites"]` is non-empty; otherwise it and its nav link are omitted.
2. 2-column card grid (1 column < 600px). Each card: photo (3:2, hover zoom `scale(1.05)` over 700ms, `loading="lazy"`), suite name (serif), facts row with icons (bed configuration, guest capacity, en-suite description), and chips for view/location (e.g. "Direct ocean view", "Private balcony").
3. Shows the first 4 suites; "Show all N suites" reveals the rest (`<details>` or `ui.js` toggle, works without JS).

### 2.4. Amenity Groups (host-configured)

1. **Content**: `listing_details["amenity_groups"]`, 1–6 `AmenityGroup`s in host order. Each group:
   - `title`: ≤ 32 characters (e.g. "Villa", "Beach & Water").
   - `icon`: one value from the closed `AmenityIcon` set (home, waves, heart, family, sun, utensils, sparkles, shield, car, star). Our own SVGs; no host-uploaded icons.
   - `items`: 1–20 strings, each ≤ 60 characters (`MAX_AMENITY_CHARS`, moved from `form.rs` into `common`). Total items across groups stays ≤ 50 (`MAX_AMENITIES`).
2. **Rendering** (`web_ui_common/src/components/amenity_group.rs`): section `#amenities` with one card per group in an `auto-fit, minmax(220px, 1fr)` grid. Card: `--card` surface, `--border` hairline, `--radius-surface`, group icon in `--primary`, checklist with `--primary` check icons. Groups with no items are skipped.
3. **Backwards compatibility**: a listing with only the legacy flat `amenities` array renders one card titled "Amenities". On first save in the new editor, the flat list is moved into a group titled "Amenities" and the legacy key is removed.
4. **Admin editor** (`web_ui_admin/src/pages/listings/form.rs`, "Amenities" fieldset):
   - 6 fixed group slots, each with title input, icon `<select>`, and an items `<textarea>` (one per line, as the current amenities field works). Empty slots are ignored on save. No JS required.
   - New listings are prefilled with the 4 suggested groups and example placeholders (Villa / Health & Wellness / Family / Common Areas); hosts rename, clear or add groups.
   - Reorder via up/down buttons is a `ui.js` enhancement; without JS the host orders groups by slot.
   - Validation lives in `common` and is re-checked by `listing_api` (`400 { "code": "INVALID_AMENITY_GROUPS", ... }`). Errors name the group ("Group 2 needs a title.").
   - Saved via the existing `listing_details` merge path, preserving other keys.

### 2.5. Staff & Services

1. Section `#services`, two columns (stacked < 860px):
   - **Included with your stay**: `--card` surface, checklist (e.g. private chef, butler, daily housekeeping, pool attendant, 24/7 security), optional note line (e.g. groceries at cost, customary gratuity).
   - **Concierge add-ons**: dashed `--border`, rows of name + price text (e.g. "Airport transfer, MBJ (1h 15m) — $180 each way", "Pre-arrival grocery stocking — On request").
2. "Ask us to arrange" opens the existing enquiry dialog with the topic pre-filled (intent open question 2, option A).

### 2.6. House Policies

Section `#policies`: a 2-column definition list (`<dl>`, 1 column < 600px) on a `--card` surface:
- **Check-in / Check-out** (e.g. from 3:00 PM / by 11:00 AM).
- **Minimum stay** (e.g. 3 nights; 7 nights Dec 18 – Jan 4).
- **Cancellation** and **Security deposit**.
- **House rules** (e.g. no smoking indoors · quiet from 11 PM · events by approval).

### 2.7. Reviews & Guestbook Fallback

1. Verified reviews render as today when present.
2. When `reviews.is_empty()`, the cold "No reviews yet" text is suppressed. If `listing_details["curated_testimonials"]` is non-empty, render up to 4 quotes (italic serif, `--primary` left rule, name · home city · stay month · rating), followed by the notice "From the villa guestbook. Verified reviews from bookings on Our Places will appear here as they come in." (D9)
3. If neither exists, the section and its nav link are omitted.

### 2.8. Location & Drive Times

Section `#location`: area name, one-line description, "The exact address is shared after booking." Two columns (stacked < 860px): an approximate-area illustration (no third-party map embed; CSP) and a list of `NearbyPlace`s with drive time (`tabular-nums`), e.g. Green Grotto Caves 10 min, Sangster Intl. (MBJ) 1h 15m.

### 2.9. Enhanced Footer (`web_ui_common/src/shell_guest.rs`)

1. Four columns (2 columns < 860px) on `--card` with a `--border` top rule:
   - **Brand & Destination**: "Our Places" wordmark, tagline, location (Runaway Bay & Kingston, Jamaica).
   - **Explore**: Villas (`/listings`), Experiences (`/experiences`, Phase 2), About (`/about`), Become a Host (`/register`).
   - **Policies**: Terms (`/terms`), Cancellation Policy (`/cancellation-policy`), Privacy Policy (`/privacy-policy`), House Rules (`/house-rules`). These four pages are added in this branch with clearly marked draft placeholder text, replaced by the owner before launch.
   - **Concierge**: WhatsApp link, phone, email, Instagram. Values come from configuration, not hard-coded.
2. Bottom hairline (`--divider`): copyright, GCT note, currency indicator.

### 2.10. Theme Token Mapping (D7)

No new colours. Components use existing Shoreline Mist tokens in all three themes (light, `.dim`, `.dark`):

| Element | Tokens |
|---|---|
| Ribbon, closing CTA band | bg `--primary`; text/icons `--primary-foreground`; labels `color-mix(--primary-foreground 75%, --primary)` |
| Pill buttons, eyebrows, active section-nav link | `--primary` |
| Editorial bands (intro, experiences) | `--card` on the `--background` page |
| Cards (suites, amenities, services, policies, booking) | `--card`, `--border`, `--radius-surface` |
| Chips | `--chip` |
| Dividers | `--divider` |
| Photo placeholders | `--photo-placeholder` |

> [!NOTE]
> In `.dark`, `--primary` is a light blue (`#AECBEB`), so the ribbon and closing CTA render as light bands with dark text. Confirmed as intended.

### 2.11. Experiences Flow (Phase 2)

1. **Catalog & Data Model**: pre-seeded catalog of Jamaican excursions and in-villa experiences:
   - *Dunn's River Falls Guided Climb* (Ocho Rios)
   - *Martha Brae Bamboo River Rafting* (Falmouth)
   - *Private Catamaran Sunset Cruise* (Runaway Bay / Ocho Rios)
   - *Green Grotto Caves Exploration* (Discovery Bay)
   - *Luminous Lagoon Night Swim* (Falmouth)
   - *In-Villa 5-Course Chef Tasting Dinner*
   - *Private Airport VIP Fast-Track & Luxury Transfer* (MBJ / KIN)
2. **Host Management (`web_ui_admin`)**: `/listings/{id}/experiences` lists the catalog; hosts toggle which apply, set a price (or "on request") and host notes, saved into `listing_details["experiences"]` with audit logging.
3. **Guest Discovery & Request (`web_ui_guest`)**:
   - **Listing page**: "Discover the area" band — horizontally scrolling cards (scroll-snap, min 260px) with photo, drive-time chip, category eyebrow, title, one-line description, price text, and "Ask us" pill.
   - **`/experiences`**: portal-wide directory.
   - **`/checkout/{slug}`**: optional "Enhance your stay" step to request experiences with the reservation.
   - **`/bookings/{id}`**: guests with pending/confirmed upcoming bookings can request an experience.

### 2.12. Quick-Win Bug Fixes (Phase 1)

1. **Double Currency Prefix**: `ListingSummary::price_text` in `web_ui_common/src/components/domain/listing_card.rs` formats `"{currency} {symbol}{price}"` → `"USD $2,400"`. Render `"{symbol}{price}"` (e.g. `"$2,400"`) when the symbol is unambiguous for the selected display currency, otherwise a disambiguated form (e.g. `"US$2,400"`).
2. **Missing Bed Count**: in `listing_detail.rs`, when `listing.beds == 0` or missing, omit the beds fact instead of rendering `"- beds"` / `"0 beds"`.

### 2.13. Photo Names (addendum, intent §8)

1. **Storage**: new migration adds `name VARCHAR(80) NULL` to `listing_image`. Only host-typed names are stored; there is no stored default.
2. **Display fallback**: an unnamed photo is "Photo N", where N is its 1-based position among the listing's processed photos (`display_order`). The label is computed when shown, so removing a photo renumbers the rest.
3. **Rules** (in `common`, shared by the admin form and `listing_api`): trimmed; at most 80 characters; an empty name clears it (`NULL`); duplicates allowed.
4. **API**: `PATCH /api/v1/listings/{id}/images/{image_id}` with body `{ "name": string | null }`.
   - Checked by `authz::require_listing_manager`; an image that doesn't belong to the listing gives 404.
   - Invalid names give `400 { "code": "INVALID_PHOTO_NAME", "message": ... }` with `tracing::warn!`.
   - Returns the updated `ListingImageResponse`.
   - `ListingImageResponse` gains `name: Option<String>` (`#[serde(default)]`).
5. **Admin edit page ("Photos" section)**:
   - Below each processed photo: a small form with a textbox (`maxlength=80`, placeholder "Photo N", current name as its value) and a "Save" button.
   - It posts to `POST /admin/listings/{id}/images/name`.
   - With JS, HTMX submits the form when the textbox loses focus, swaps only that photo's form, and shows a "Name saved" toast. Without JS, the Save button posts and the edit page reloads.
   - A refused name re-renders the form with the message and the typed value kept.
   - Photos still uploading or processing show no textbox.
6. **Suite photo picker**: the options read the photo's name, or "Photo N".
7. **Guest page**:
   - Every `<img>` for a photo (hero, intro, mosaic, gallery dialog, full-size viewer, suite cards) uses the name as alt text, or `"{listing name}, photo N"` when unnamed.
   - A visible caption appears only in the full-size viewer, only when the photo has a name. `ui.js` reads `data-caption` on the active item and writes it into a `[data-lightbox-caption]` element (`aria-live="polite"`), which is left empty when there's no name.
   - No captions in the gallery dialog, mosaic, hero or suite cards.

---

## 3. Data Models & Schemas

### 3.1. Structured Listing Details DTOs (`common/src/models.rs`)

```rust
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Icons a host may pick for a ribbon item. Closed set: rendered from our own SVGs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RibbonIcon {
    Bed, Bath, Guests, Pool, Beach, Ocean, Chef, Butler,
    Security, Wifi, Solar, Car, Gym, Star,
}

/// One host-entered item in the listing's highlights ribbon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RibbonItem {
    pub icon: RibbonIcon,
    pub value: String, // ≤ 8 chars, e.g. "6", "5.5", "Direct"
    pub label: String, // ≤ 24 chars, e.g. "Bedrooms", "Beach access"
}

/// A validated ribbon: 3–6 complete items. Only constructible via `Ribbon::try_from`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Ribbon(Vec<RibbonItem>);

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RibbonError {
    #[error("A ribbon needs between 3 and 6 items, got {0}.")]
    Count(usize),
    #[error("Item {0} needs a value and a label.")]
    Incomplete(usize),
    #[error("Item {0}'s value is longer than 8 characters.")]
    ValueTooLong(usize),
    #[error("Item {0}'s label is longer than 24 characters.")]
    LabelTooLong(usize),
}

/// A specific sleeping suite or bedroom.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SuiteItem {
    pub id: String,
    pub name: String,
    pub bed_configuration: String, // e.g. "1 king"
    pub max_guests: i32,
    #[serde(default)]
    pub features: Vec<String>,     // chips, e.g. "Direct ocean view", "Private balcony"
    pub en_suite: Option<String>,  // e.g. "En-suite, soaking tub"; None = shared bath
    pub description: Option<String>,
    pub image_id: Option<Uuid>,    // one of the listing's own uploaded images (not a free URL)
}

/// Icons a host may pick for an amenity group. Closed set: rendered from our own SVGs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AmenityIcon {
    Home, Waves, Heart, Family, Sun, Utensils, Sparkles, Shield, Car, Star,
}

/// One host-configured amenity group (D2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AmenityGroup {
    pub title: String,      // ≤ 32 chars
    pub icon: AmenityIcon,
    pub items: Vec<String>, // 1–20 items, each ≤ 60 chars
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AmenityGroupError {
    #[error("Add between 1 and 6 amenity groups, got {0}.")]
    Count(usize),
    #[error("Group {0} needs a title.")]
    MissingTitle(usize),
    #[error("Group {0}'s title is longer than 32 characters.")]
    TitleTooLong(usize),
    #[error("Group {0} needs between 1 and 20 items.")]
    ItemCount(usize),
    #[error("An item in group {0} is longer than 60 characters.")]
    ItemTooLong(usize),
}

/// The 4 groups prefilled on a new listing (titles only; hosts fill items).
pub const SUGGESTED_AMENITY_GROUPS: [(&str, AmenityIcon); 4] = [
    ("Villa", AmenityIcon::Home),
    ("Health & Wellness", AmenityIcon::Waves),
    ("Family", AmenityIcon::Family),
    ("Common Areas", AmenityIcon::Sun),
];

/// An optional paid service shown under "Concierge add-ons".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceAddon {
    pub name: String,
    pub price: Option<Decimal>,   // None = "On request"
    pub unit: Option<String>,     // e.g. "each way", "day", "hour"
}

/// Staff & services breakdown.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StaffServices {
    #[serde(default)]
    pub included: Vec<String>,
    pub included_note: Option<String>,
    #[serde(default)]
    pub addons: Vec<ServiceAddon>,
}

/// A nearby place with drive time, for the Location section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NearbyPlace {
    pub name: String,
    pub drive_minutes: i32,
}

/// Curated local excursion or in-villa experience (Phase 2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListingExperience {
    pub id: String,
    pub title: String,
    pub category: String,           // e.g. "Adventure", "Culinary"
    pub drive_time_minutes: Option<i32>,
    pub location_name: String,
    pub description: String,
    pub price: Option<Decimal>,     // None = included or on request
    pub currency: String,
    pub image_url: Option<String>,
    pub is_enabled: bool,
}

/// Guestbook quote shown only when there are no verified reviews (D9).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CuratedTestimonial {
    pub guest_name: String,
    pub location: Option<String>,
    pub stay_date: String,
    pub quote: String,
    pub rating: i32,
}

/// House policies (§2.6).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HousePolicies {
    pub check_in_time: Option<String>,
    pub check_out_time: Option<String>,
    pub minimum_stay: Option<String>,
    pub cancellation: Option<String>,
    pub security_deposit: Option<String>,
    pub house_rules: Option<String>,
}

/// Structured listing metadata stored in `listing.listing_details` JSONB.
/// Every field is optional; unknown keys (e.g. legacy `amenities`) are preserved by the admin merge.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StructuredListingDetails {
    pub pull_quote: Option<String>,
    pub ribbon: Option<Vec<RibbonItem>>,          // validated into `Ribbon` at render time
    pub suites: Option<Vec<SuiteItem>>,
    pub amenity_groups: Option<Vec<AmenityGroup>>,
    pub amenities: Option<Vec<String>>,           // legacy flat list, rendered as one group
    pub staff_services: Option<StaffServices>,
    pub policies: Option<HousePolicies>,
    pub nearby: Option<Vec<NearbyPlace>>,
    pub experiences: Option<Vec<ListingExperience>>,
    pub curated_testimonials: Option<Vec<CuratedTestimonial>>,
}
```

- `Ribbon::try_from(Vec<RibbonItem>) -> Result<Ribbon, RibbonError>` enforces §2.2 rules.
- `Ribbon::fallback(&ListingResponse) -> Ribbon` builds bedrooms / bathrooms / max guests.
- `Ribbon::for_listing(details, listing) -> Ribbon` = stored ribbon if valid, else fallback (logging a warning when stored data was invalid).
- `validate_amenity_groups(&[AmenityGroup]) -> Result<(), AmenityGroupError>` enforces §2.4 rules.
- `amenity_groups_for(details) -> Vec<AmenityGroup>` = stored groups if present, else the legacy flat list as one "Amenities" group, else empty (section omitted).
- Deserializing `listing_details` never fails the page: a malformed JSON blob logs `tracing::warn!` and renders with `StructuredListingDetails::default()`.

---

## 4. Technical Architecture & Endpoints

### 4.1. Crate Boundaries & Interactions

```
/our_places_rs
├── common/
│   ├── models.rs                 # StructuredListingDetails, RibbonItem/Ribbon, SuiteItem, … DTOs
│   └── money.rs                  # Clean currency formatting (no duplicate USD $)
├── db_core/
│   └── seeds/                    # listing_details for the 5 Jamaican villas (ribbon, suites, policies, nearby…)
├── web_ui_common/
│   ├── components/
│   │   ├── listing_hero.rs       # Full-bleed hero with location pill + photo count   (P1)
│   │   ├── section_nav.rs        # Sticky in-page nav with price + CTA                (P1)
│   │   ├── highlights_ribbon.rs  # Host-entered ribbon + RibbonIcon SVG set           (P1)
│   │   ├── suite_card.rs         # Suite cards with hover zoom                        (P1)
│   │   ├── amenity_group.rs      # Host-configured amenity group cards                (P1)
│   │   ├── photo_mosaic.rs       # 5-tile mosaic opening the lightbox                 (P1)
│   │   └── experience_card.rs    # Excursion cards with drive times & CTA             (P2)
│   ├── static/ui.js              # Section-nav highlight, ribbon/group editor reorder (P1)
│   ├── style/guest.css           # .page-grid / .full-bleed utilities (D10)            (P1)
│   └── shell_guest.rs            # <main class="page-grid">, 4-column footer          (P1)
├── web_ui_guest/
│   └── pages/
│       ├── listing_detail.rs     # Page layout §2.1                                   (P1)
│       ├── experiences.rs        # /experiences                                       (P2)
│       ├── checkout.rs           # "Enhance your stay" step                           (P2)
│       └── bookings.rs           # Post-booking experience requests                   (P2)
└── web_ui_admin/
    └── pages/listings/
        ├── form.rs               # Ribbon editor, amenity groups, suites, staff, policies, nearby (P1)
        └── experiences.rs        # Host experience configuration                      (P2)
```

### 4.2. Routes

1. **Guest Portal (`web_ui_guest`)**:
   - `GET /listings/{slug}`: Enriched listing detail view (P1).
   - `GET /terms`, `GET /privacy-policy`, `GET /cancellation-policy`, `GET /house-rules`: Static policy pages (P1).
   - `GET /experiences`: Experiences directory (P2).
   - `POST /listings/{slug}/experiences/{id}/enquire`: HTMX endpoint opening the enquiry dialog pre-filled with the experience (P2).
   - `POST /bookings/{id}/experiences`: Authenticated guest requests an experience for their stay (P2).
2. **Admin Portal (`web_ui_admin`)**:
   - Existing listing create/edit routes accept the ribbon and other `listing_details` fieldsets (P1).
   - `GET|POST /admin/listings/{id}/experiences`: Experience toggles and pricing (P2).

---

## 5. Performance, Scalability & Threat Modeling

### 5.1. Performance & Cloud Run Budget
- **Cold start (<300ms p50)**: all components are statically compiled Topcoat views; no template parsing or JS runtime on the server.
- **Images**: `<picture>`/`srcset` variants (`mobile 640`, `tablet 1024`, `desktop 1920`); hero `fetchpriority="high"`; everything below the hero `loading="lazy"`.
- **No N+1 queries**: all structured content comes from the single `listing_details` JSONB read.
- **No layout shift**: suite, mosaic and experience images use fixed aspect-ratio boxes.

### 5.2. Threat Modeling & Security
- **IDOR**: admin updates check `listing.user_id == claims.sub` (or admin role); guest experience requests check `booking.user_id == claims.sub`.
- **CSP**: no inline scripts or `eval`; dialogs, section-nav highlight and ribbon-editor enhancements run from `ui.js` via data attributes. No third-party map iframes.
- **XSS**: Topcoat `view!` escapes all host-entered text (ribbon values/labels, suite names, quotes). Ribbon icons come from a closed enum, so hosts cannot inject SVG.
- **Input bounds**: ribbon and amenity-group length caps and counts enforced in `common` and re-checked in `listing_api` (`400 INVALID_RIBBON`, `400 INVALID_AMENITY_GROUPS`).
- **Financial precision**: add-on and experience prices use `rust_decimal::Decimal`.

---

## 6. Multi-Tier Test Plan

### 6.1. Unit Tests (`cargo test -p common -p web_ui_common`)
1. **Ribbon validation**: 2 and 7 items → `Count`; blank value or label → `Incomplete(n)`; 9-char value → `ValueTooLong`; 25-char label → `LabelTooLong`; 3 and 6 valid items → `Ok`.
2. **Ribbon fallback**: bedrooms/bathrooms/max guests produce 3 items; 5 full + 1 half bath → `"5.5"`; invalid stored ribbon → fallback.
3. **Ribbon serde**: unknown icon string fails deserialization of that item and triggers fallback, not a page error.
4. **Currency formatting**: `price_text()` → `"$2,400"` for USD; non-symbol currencies render unambiguously.
5. **Bed count**: beds fact omitted when `beds == 0` / missing.
6. **Details deserialization**: legacy flat `amenities` → single "Amenities" group; malformed JSON → `default()` with warning.
7. **Amenity group validation**: 0 and 7 groups → `Count`; blank title → `MissingTitle(n)`; 33-char title → `TitleTooLong`; 0 or 21 items → `ItemCount`; 61-char item → `ItemTooLong`; 1 and 6 valid groups → `Ok`.

7. **Photo names**: validation (trimmed, 80/81 chars, blank → `None`); `ListingImageResponse` deserializes without `name`; "Photo N" fallback and alt-text helpers.

### 6.2. Component & Integration Tests (`cargo test -p web_ui_guest -p web_ui_admin`)
1. **Ribbon component**: renders N `<li>` for 3–6 items with escaped text; `data-testid="highlights-ribbon"` present.
2. **Empty sections**: no suites / policies / nearby / reviews+testimonials → section and its nav link omitted.
3. **Guestbook**: empty reviews + testimonials → quotes and the "From the villa guestbook" notice render; no "No reviews yet".
4. **Admin ribbon save**: valid rows save into `listing_details.ribbon` and preserve other keys; a half-filled row returns the form with the row-specific error.
5. **Admin amenity groups**: new listing form shows the 4 suggested groups; saving renamed/reordered groups writes `listing_details.amenity_groups`; a listing with only legacy `amenities` opens with them in one "Amenities" group and, once saved, has no `amenities` key.
6. **Shell grid**: a page with no `.full-bleed` children renders its content in the 1540px column, as before.

### 6.3. Playwright E2E (`pnpm test:guest`, `pnpm test:admin`)
1. **Listing page**: `/listings/the-great-house` shows hero, section nav, ribbon, suites, amenity groups, services, policies, location and footer links (Terms, Privacy, Cancellation, WhatsApp).
2. **Responsive**: at 390px the ribbon is 2 columns, the bottom booking bar is visible, and the page has no horizontal scroll; at 1280px the booking card is sticky beside the body.
3. **Themes**: ribbon and pills are legible in light, `.dim` and `.dark`.
4. **Host ribbon round-trip**: host edits the ribbon in admin (change a value, reorder, remove an item), saves, and the guest page shows the new ribbon in order.
5. **Host amenity groups round-trip**: host renames a group, adds a fifth group, removes one, saves; the guest page shows the groups in the new order and titles.
6. **Other guest pages**: home, listings, bookings and about render inside the content column with no horizontal scroll (D10 regression).
7. **Experiences (P2)**: "Ask us" opens the enquiry dialog with the experience pre-filled; host toggles an experience and it appears on the guest page.

---

## 7. Acceptance Criteria

### Phase 1
- [ ] **Layout**: hero, sticky section nav, intro, ribbon, body with sticky booking card, mosaic, closing CTA render as in §2.1; mobile bottom booking bar < 1024px.
- [ ] **Highlights ribbon**: host-entered 3–6 items render with icon, value, label; fallback from listing facts when unset or invalid.
- [ ] **Admin ribbon editor**: hosts add, edit, reorder and remove ribbon items; validation errors name the row.
- [ ] **Suites**: cards with bed configuration, capacity, en-suite, feature chips and photos.
- [ ] **Amenities**: hosts configure 1–6 titled groups per listing in admin (4 suggested groups prefilled on new listings); legacy flat list renders as one group and migrates on save.
- [ ] **Full-width sections**: shell `<main>` uses the `.page-grid`; listing page blocks go edge to edge; other guest pages unchanged.
- [ ] **Staff & services**: included vs. add-ons with prices; "Ask us to arrange" opens the enquiry dialog pre-filled.
- [ ] **Policies, Location**: definition list and drive-time list render; empty sections are omitted.
- [ ] **Guestbook fallback**: labelled, only when there are no verified reviews.
- [ ] **Footer**: 4 columns incl. Terms, Cancellation, Privacy, House Rules, concierge contact.
- [ ] **Theme**: no new colour tokens and no hard-coded hex in new components; works in light, `.dim`, `.dark`.
- [ ] **Bug fixes**: `"USD $2,400"` → `"$2,400"`; no `"- beds"`.
- [ ] **Quality**: zero `.unwrap()` / `.expect()` in non-test code, strict CSP, clippy clean, Playwright suite passing.

- [ ] **Photo names (§2.13)**: hosts name each processed photo, with "Photo N" as the placeholder and an empty box clearing the name; names show in the suite picker, as guest alt text, and as the full-size viewer's caption only; names over 80 characters are refused; non-owners get 403/404.

### Phase 2
- [ ] **Experiences (host)**: hosts view and toggle experiences per listing in `web_ui_admin`.
- [ ] **Experiences (guest)**: guests see experiences with drive times on the listing and `/experiences`, and can request them at checkout or from `/bookings/{id}`.
