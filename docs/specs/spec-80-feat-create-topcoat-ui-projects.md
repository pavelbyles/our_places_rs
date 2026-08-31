# Spec 80: Create Topcoat UI Projects & Feature Parity Architecture

## Overview

The **Our Places** short-term luxury villa booking platform originally operated using WebAssembly frontend applications built on Leptos (`web_app` for guests, `web_app_admin` for administrators, and `web_app_common` for shared components). 

To optimize cold-start performance, eliminate heavy client-side WebAssembly bundles, and leverage modern server-driven UI paradigms on GCP Cloud Run ($0.25\text{ vCPU}$, $256\text{MB RAM}$), this specification details the architectural migration and implementation of the **Topcoat** UI projects:
- **`web_app_common_tc`**: Shared Topcoat UI library, Tailwind CSS/DaisyUI design tokens, HTMX integration, automated time-of-day theme engine, shared sample datasets, and backend API clients.
- **`web_app_tc`**: High-performance, server-rendered public guest portal running on **Port 3000** for villa search, dynamic seasonal pricing calculation, booking checkout with 15-minute atomic holds, verified review submission, and guest profile management.
- **`web_app_admin_tc`**: Secure, server-rendered internal administration dashboard running on **Port 3002** for 23-field listing studio editing & cloning, dynamic seasonal pricing rules, booking audit logs, and type-enforced user capability management.

---

## 1. Technical Specification & Phased Architecture

```mermaid
flowchart TB
    subgraph s1["Topcoat UI Layer (Server-Rendered SSR + HTMX)"]
        WTC["web_app_tc (Guest Portal :3000)"]
        WATC["web_app_admin_tc (Admin Dashboard :3002)"]
        WCTC["web_app_common_tc (Layouts, Themes, Components, HTMX)"]
    end
    subgraph s2["Shared Isomorphic Crates"]
        COM["common (Tri-Currency Pricing, DTOs, Static Reference)"]
        DBC["db_core (PostgreSQL Schema & SQLx Migrations)"]
    end
    subgraph s3["Backend Microservices (Actix-web)"]
        LA["listing_api (:8082)"]
        BA["booking_api (:8081)"]
        UA["user_api (:8083)"]
        IW["image_worker (Pub/Sub)"]
    end
    subgraph s4["Infrastructure & Data Store"]
        PG[("PostgreSQL 16")]
        GCS[("Google Cloud Storage")]
    end

    WTC --> WCTC
    WATC --> WCTC
    WCTC --> COM
    WTC -.-> LA
    WTC -.-> BA
    WTC -.-> UA
    WATC -.-> LA
    WATC -.-> BA
    WATC -.-> UA
    LA --> DBC
    BA --> DBC
    UA --> DBC
    DBC --> PG
    LA -- V4 Signed URL --> GCS
```

---

### Phase 1: Foundation & Shared UI Library (`web_app_common_tc`)

The foundation crate encapsulates shared layouts, HTMX assets, Tailwind/DaisyUI styling, time-of-day theme automation, and HTTP client infrastructure.

#### 1.1. Crate Configuration & Build Pipeline
- **Crate**: `web_app_common_tc` (Rust 2024 edition, target library `rlib`).
- **Dependencies**: `topcoat` (features: `htmx`, `tailwind`, `router`, `view`), `common`, `serde`, `serde_json`, `reqwest`, `tracing`, `chrono`, `rust_decimal`.
- **Topcoat Linkme Layout Architecture**:
  - `#[layout("/")]` macros must reside strictly in binary crates (`web_app_tc` and `web_app_admin_tc`) to prevent linker collisions during static route table aggregation.
  - `web_app_common_tc` exports reusable layout functions (`guest_base_layout`, `admin_base_layout`) which are invoked by the binary crate layouts.

#### 1.2. Tailwind CSS & DaisyUI Theme Configuration
- **Design Tokens**:
  - Primary Theme: `emerald` (Light luxury theme).
  - Dark Theme: `night` / `sunset` (Dark luxury theme).
  - Class-based theme selector `.dark` and `data-theme` attribute toggling.
- **Embedded Luxury Tokens**:
  - Dedicated glassmorphic classes (`.search-capsule`, `.hero-luxury`, `.frosted-dropdown-menu`) embedded directly in layout `<style>` to ensure zero-FOUC and eliminate external bundle dependency failures.

#### 1.3. Time-of-Day Automated Theme Engine & Manual Switcher (`src/theme.rs`)
- **Automated Default Selection**:
  - If no manual theme preference is stored in `localStorage`, the client time of day is evaluated.
  - If current local hour $\ge 18$ (6:00 PM) or $< 6$ (6:00 AM), dark mode (`night`/`sunset`) is applied.
  - Otherwise, the light theme (`emerald`) is applied.
- **Interactive Switcher (`theme_toggle`)**:
  - Toggles `data-theme` and `.dark` class dynamically and saves to `localStorage.setItem('theme', ...)`.

#### 1.4. Shared UI Components
- **`VillaCard`**: Visual property card featuring image thumbnail, title, location (parish, Jamaica), base/converted nightly rate, overall rating, and review count badge.
- **`ResponsiveImage`**: Generates HTML `<picture>` and `srcset` tags for 640px (mobile), 1024px (tablet), and 1920px (desktop) WebP assets.
- **`StarRating`**: Renders SVG star icons (with fractional support) and review count link.
- **`PriceBreakdown`**: Itemized stay summary calculating nights, effective daily rate, statutory GCT 15% (`common::reference::JAMAICAN_GCT_RATE`), and tri-currency conversions (`rust_decimal::Decimal`).

---

### Phase 2: Public Guest Portal (`web_app_tc` : Port 3000)

The guest application provides full feature parity with the Leptos `web_app`, running on port `3000`.

#### 2.1. Routing Map & Page Structure
| Route | Method | Layout | Description |
| :--- | :--- | :--- | :--- |
| `/` | `GET` | `guest_layout` | Full-bleed Caribbean hero, featured villas, floating search capsule. |
| `/listings` | `GET` | `guest_layout` | Search directory with structure, price, and parish filtering. |
| `/listings/{id}` | `GET` | `guest_layout` | Villa studio details, amenities, reviews, interactive booking widget. |
| `/listings/{id}/quote` | `POST` | Fragment | Dynamic HTMX stay price quote calculating seasonal overrides and 15% GCT. |
| `/checkout/{id}` | `GET`, `POST` | `guest_layout` | 15-minute hold checkout flow, guest details, tri-currency payment summary. |
| `/login` | `GET`, `POST` | `guest_layout` | Guest login (password or 6-digit email code). |
| `/register` | `GET`, `POST` | `guest_layout` | Account registration with shadow user promotion. |
| `/verify` | `GET`, `POST` | `guest_layout` | 6-digit email verification code input. |
| `/bookings` | `GET` | `guest_layout` | Guest reservation dashboard (upcoming, active, past). |
| `/bookings/{id}` | `GET` | `guest_layout` | Itemized booking details, receipt, and cancellation modal. |
| `/reviews/submit` | `GET`, `POST` | `guest_layout` | Verified guest review submission gated by 15-day token. |
| `/profile` | `GET`, `POST` | `guest_layout` | User profile management, password update, currency preferences. |
| `/about` | `GET` | `guest_layout` | About Our Places and Jamaican villa portfolio story. |

---

### Phase 3: Administrative Dashboard (`web_app_admin_tc` : Port 3002)

The administrative portal runs on port `3002` with executive controls, 23-field listing editing, dynamic pricing overrides, and type-safe access control.

#### 3.1. Routing Map & Page Structure
| Route | Method | Layout | Description |
| :--- | :--- | :--- | :--- |
| `/login` | `GET`, `POST` | `admin_layout` | Admin/Host authentication and session initialization. |
| `/` or `/admin` | `GET` | `admin_layout` | Executive KPI dashboard (gross revenue, occupancy, bookings schedule). |
| `/admin/listings` | `GET` | `admin_layout` | Listing directory with status badges and quick action controls. |
| `/admin/listings/new` | `GET`, `POST` | `admin_layout` | 23-field villa studio wizard with GPS geocoding and GCS presigned URLs. |
| `/admin/listings/{id}/edit` | `GET`, `POST` | `admin_layout` | Executive studio editor with Caribbean country selector and metric stat tiles. |
| `/admin/listings/clone/{id}` | `GET`, `POST` | `admin_layout` | 1-click property cloning and template generation with `(Copy)` naming. |
| `/admin/listings/{id}/pricing` | `GET` | `admin_layout` | Seasonal dynamic price overrides and minimum night stay manager. |
| `/admin/listings/{id}/pricing/add` | `POST` | Fragment | Live HTMX handler for adding seasonal overrides and returning updated table. |
| `/admin/listings/{id}/pricing/remove` | `POST` | Fragment | Live HTMX handler for removing overrides with instant table refresh. |
| `/admin/bookings` | `GET` | `admin_layout` | Master booking schedule, hold status, and date lock monitor. |
| `/admin/users` | `GET` | `admin_layout` | User directory with search and role badges (`admin`, `host`, `booker`). |
| `/admin/users/new` | `GET`, `POST` | `admin_layout` | User invitation with **algebraic type-enforced capability constraints**. |
| `/admin/exchange-rates` | `GET`, `POST` | `admin_layout` | Live currency exchange rates sync and statutory tax rate panel. |

---

## 2. New Architectural & Functional Enhancements

Beyond standard feature parity, Topcoat introduces substantial enhancements to developer velocity, runtime safety, and user experience:

### 2.1. 1-Click Listing Template Cloning (`/admin/listings/clone/{id}`)
* **Workflow**: Deep-clones all 23 database fields from an existing villa into a new pre-filled studio form.
* **Naming Semantics**: Automatically appends `(Copy)` to the title (e.g. `"The Reef House"` $\rightarrow$ `"The Reef House (Copy)"`).
* **Identity Isolation**: Generates a fresh `Uuid::now_v7()` upon submission, preventing accidental overwrites while preserving complex geocodes, amenities, and pricing configurations.

### 2.2. Executive Studio Card Design System with 2px High-Contrast Borders
* **Visual Structure**: Form controls are grouped into high-contrast 2px bordered studio cards (`border-2 border-base-300 dark:border-base-content/20 bg-base-100 rounded-xl px-4 py-2.5`) with domain icon headers:
  - `🏷️ Property Identity & Branding`
  - `📍 Caribbean Location & GPS Coordinates`
  - `🛏️ Accommodations & Spatial Architecture`
  - `💰 Financial Yield & Length-of-Stay Incentives`
  - `🖼️ High-Resolution Media Management`
  - `✨ Bespoke Amenities & House Rules`
* **Studio Header Banner**: Displays the property thumbnail avatar, publication status badge (`Active` / `Draft`), location metadata, and current nightly rate.

### 2.3. Interactive Caribbean Country Selector
* Upgraded from a static/readonly field to a dynamic dropdown supporting 7 primary Caribbean luxury rental jurisdictions:
  - 🇯🇲 **Jamaica** (`Jamaica`)
  - 🇧🇧 **Barbados** (`Barbados`)
  - 🇧🇸 **Bahamas** (`Bahamas`)
  - 🇱🇨 **Saint Lucia** (`Saint Lucia`)
  - 🇰🇾 **Cayman Islands** (`Cayman Islands`)
  - 🇹🇨 **Turks and Caicos** (`Turks and Caicos`)
  - 🇩🇴 **Dominican Republic** (`Dominican Republic`)

### 2.4. Rust Algebraic Type-Enforced Role Capabilities (`RoleCapabilityProfile`)
* Uses Rust's algebraic type system to prevent unprivileged `Booker` accounts from possessing administrative permissions:
  ```rust
  pub enum PrivilegedScope {
      Host,
      Admin,
  }

  pub struct RoleCapabilityProfile {
      pub scope: Option<PrivilegedScope>,
      pub is_booker: bool,
      pub permissions: GranularPermissions,
  }
  ```
* **Constraint Invariant**: Calling `RoleCapabilityProfile::build()` with permissions but without `is_host` or `is_admin` returns `Err(PermissionTypeConstraintError::BookerCannotHoldPrivileges)`.
* **Client-Side Reactive Enforcement**: Synchronized with JavaScript on `/admin/users/new` that automatically disables and clears permission checkboxes when pure booker accounts are selected.

### 2.5. Real-Time HTMX Dynamic Pricing Manager
* Live HTMX endpoints (`/admin/listings/{id}/pricing/add`, `/admin/listings/{id}/pricing/remove`) swap the `#price-overrides-container` table partial on the fly with live status badges and toast feedback banners, eliminating full-page refreshes.

### 2.6. Embedded Luxury Styling & Zero-FOUC Guarantee
* Inlined critical luxury styling tokens (`.search-capsule`, `.hero-luxury`, `.frosted-dropdown-menu`, backdrop filters) directly into the `<head>` of `guest_base_layout`, guaranteeing consistent rendering across standalone servers and CDN caches.

---

## 3. Monadic Architecture & Safety Guardrails

1. **Zero `unwrap()` / Zero `expect()` Policy**:
   - Production code and page handlers strictly avoid panicking methods, utilizing pure monadic combinator chains (`.ok().or_else(...)`, `.as_ref().map(...).unwrap_or_else(...)`, `.map_err(...)`).
2. **Tri-Currency Decimal Precision**:
   - Money and statutory tax rates exclusively use `rust_decimal::Decimal` (e.g. `dec!(0.15)` for 15% Jamaican GCT). Floating-point (`f32`/`f64`) money math is strictly prohibited.
3. **15-Minute Atomic Holds**:
   - Database row-level locks (`SELECT ... FOR UPDATE`) protect date ranges against double-booking during checkout.

---

## 4. Comprehensive Automated Test Plan

The Topcoat projects are validated by **20 automated unit and integration tests**:

### 4.1. `web_app_common_tc` (`foundation_tests.rs`) — 7 Tests
- `test_tri_currency_conversion_precision`: Validates zero-floating-point conversions from USD to JMD (155.50) and EUR (0.92).
- `test_seasonal_dynamic_pricing_calculation_with_overrides`: Verifies that active `PriceOverride` intervals (e.g. Christmas / New Year peak at `$1,000/night`) correctly override base rates and enforce minimum-stay thresholds.
- `test_length_of_stay_weekly_discount_calculation`: Tests 10% weekly discount deductions followed by 15% statutory GCT compounding.
- `test_discount_and_stay_totals`: Validates stay subtotaling, custom discounts, and tax lines.
- `test_statutory_tax_calculation_precision`: Verifies Jamaican statutory 15% GCT against `SupportedCountry::LIST`.
- `test_theme_scripts_contain_expected_themes_and_classes`: Tests zero-FOUC theme hydration script tokens.
- `test_sample_listings_lookup_and_slug_normalization`: Verifies case-insensitive slug lookup and structure matching.

### 4.2. `web_app_admin_tc` (`admin_portal_tests.rs`) — 8 Tests
- `test_granular_permissions_type_constraint`: Validates algebraic data type constraints (`RoleCapabilityProfile`) ensuring unprivileged `Booker` roles cannot hold administrative permissions.
- `test_listing_23_fields_coordinate_and_price_boundaries`: Validates Caribbean GPS boundaries ($17.0 \le \text{lat} \le 19.0$, $-79.0 \le \text{lon} \le -76.0$), positive rates, and positive guest limits across all 23 listing fields.
- `test_seasonal_override_inverted_dates_rejection`: Enforces date range integrity by verifying that inverted intervals are rejected.
- `test_listing_clone_and_field_coverage`: Tests deep-cloning properties with `(Copy)` naming semantics and 5 database structures (`Apartment`, `House`, `Townhouse`, `Studio`, `Villa`).
- `test_admin_seasonal_override_interval_validation`: Validates rate bounds and minimum stay criteria for peak season overrides.
- `test_admin_role_authorization_and_shadow_user_audit`: Verifies role strings (`admin`, `host`, `booker`) and 15-minute shadow hold promotion windows.
- `test_admin_kpi_revenue_and_tax_estimation`: Tests executive KPI revenue calculations with statutory 15% GCT projections.
- `test_admin_layout_navigation_sections`: Tests navigation sections across the executive portal.

### 4.3. `web_app_tc` (`guest_portal_tests.rs`) — 5 Tests
- `test_fifteen_minute_booking_hold_expiry_calculation`: Tests timestamp math for 15-minute reservation holds and hold expiration detection.
- `test_guest_stay_with_length_of_stay_discount_and_gct`: Validates 10-night luxury booking with 10% weekly discount and 15% Jamaican statutory GCT.
- `test_guest_stay_subtotal_and_statutory_gct`: Tests 5-night stay gross and net totals.
- `test_sample_listings_validity_and_parity`: Validates parity of featured Jamaican villas.
- `test_get_listing_by_id_api`: Verifies live API fetch over `listing_api` with fallback.
