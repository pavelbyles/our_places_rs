# Intent: Admin-Managed Currency Pairs & Effective-Dated Exchange Rates

## Metadata
- **Author**: Originator (via GitHub Issue #100)
- **Date**: 2026-11-01
- **Status**: Approved (2026-11-01)
- **Target Area**: Fullstack (`common`, `db_core`, `app_api/booking_api`, `app_api/listing_api`, `web_client`, `web_ui_admin`)

---

## 1. Problem Statement & Motivation
Exchange rates drive every non-base-currency checkout (Tri-Currency flow: listing base currency → guest payment currency → GCT/totals), yet today they can only be changed by hand-editing the `currency_exchange_rates` table.

1. **No write path**: `GET /api/v1/admin/exchange-rates` and the `/admin/exchange-rates` page are read-only for both admins and hosts. Admins cannot add a pair, retire a pair, or publish a new rate without database access.
2. **No effective-period model in the UI/API**: The table already carries `effective_at`, but nothing lets an operator schedule a rate, see rate history, or reason about "which rate applied on date X".
3. **Two divergent lookup paths**:
   - `db_core::exchange_rate::get_rate` (checkout) honours `effective_at <= at`, supports inverse pairs, and fails with `FxRateUnavailable` when no rate exists. Correct.
   - `db_core::currency::get_exchange_rate_and_currency` (listing search/detail in `listing_api`) ignores `effective_at` (a **future-dated** rate would apply immediately) and **silently falls back** to USD or to rate `1.0` when a pair is missing. This breaks the "no money fallbacks" standard, and once admins can schedule future rates, guests would see browse prices that don't match checkout.
4. **Hardcoded pair list**: `web_ui_admin/src/pages/exchange_rates.rs` hardcodes `USD_TARGETS = [JMD, EUR, GBP, CAD]` for "Not set" rows. Which pairs exist is decided in code, not by the admin.

Why now: rates move. Without an admin write path, prices go stale and every rate change needs an engineer.

---

## 2. Proposed Outcome (The Vision)
1. **Admin-only rate management** in the admin portal:
   - **Add pair**: create a new `base → target` pair between **any currencies stored in the database** (no hardcoded lists), with an initial rate and effective start. Pairs are rows in the existing `currency_exchange_rates` table.
   - **Update rate**: publish a new rate for an existing pair with an `effective_at` of **now or later**. The new row **supersedes** the previous one from that instant onward.
   - **Delete a scheduled rate**: a rate that hasn't taken effect yet (`effective_at > now`) can be deleted. The admin then enters a new rate to replace it. Rates that are already effective or in the past can't be edited or deleted.
   - **Remove pair**: retire a pair so it stops being used for new quotes/checkouts. **Its full rate history is kept.** Blocked while any listing still uses it, and the admin sees which listings.
   - **History view (admin only)**: per-pair timeline of past, current, and scheduled rates.
2. **Hosts stay read-only and see only the current rate**: a current-rate table with no history, no scheduled rates, and no mutation controls. The API rejects writes from hosts with 403.
3. **Effective-period semantics** (from the issue):
   - A rate is effective from its `effective_at` until the next rate for the same pair takes effect.
   - If a pair has only one rate, it is effective **indefinitely** until a newer one is entered.
   - Rate resolution at time `t` = the latest row with `effective_at <= t` (open-ended period, no explicit `effective_to` needed).
   - **No backdating**: nobody can enter a rate with a past `effective_at`. A short server-side grace period absorbs form-submit latency for "effective now" rates.
4. **Reverse-pair preference**: to convert `A → B`, use the explicit `A → B` pair when it exists. Otherwise derive it from the explicit `B → A` pair as `(1 / rate).round_dp(6)`. Once someone adds an explicit reverse pair, it always takes precedence over the derived value.
5. **Typo guard (5%)**: if a new rate differs from the pair's current effective rate by more than **5%**, the admin must explicitly confirm before it's saved. The override is recorded in the audit trail. A brand-new pair has no baseline, so the guard doesn't apply.
6. **Rate locked when the hold is created**: a booking uses the rate that was effective when its `pending` hold was created, and keeps it for its whole lifecycle (hold window, confirmation, reservation changes, tax documents, payout ledger). `booking.exchange_rate` already snapshots this. The intent is to **guarantee and test** it, so later rate changes, including ones that take effect during the hold window, never re-price existing bookings.
7. **Single source of truth for rate lookup**: all browse/search/checkout pricing resolves rates through one effective-dated, fallback-free function.

---

## 3. Scope Boundaries
### In Scope (MVP)
- Admin-only write endpoints in `booking_api` (alongside the existing `/api/v1/admin/exchange-rates` scope): create pair, add rate version, delete a not-yet-effective rate, retire pair. Role check `Admin` → otherwise 403.
- Validation: both currency codes must exist in the database currency reference (no hardcoded lists), `base != target`, `rate > 0`, `NUMERIC(15,6)` precision, `effective_at >= now − grace period` (server clock, rejected otherwise), no duplicate `(base, target, effective_at)` (already a unique constraint), and the 5% typo guard with an explicit confirm flag.
- Admin-only currency management: add a currency (code, name) and deactivate it from the admin portal. Never hard-delete.
- New migration: create `currencies`, seed it from `SELECT DISTINCT` across every currency column, then add foreign keys to all of them (§7 table).
- Admin UI: forms/HTMX actions on `/admin/exchange-rates` for add, update, delete-scheduled, and retire, a confirmation step when the 5% guard trips, and a per-pair history view. Hosts see the current-rate table only.
- Remove the hardcoded `USD_TARGETS` list. The page renders the pairs that exist in the DB.
- Merge or retire `db_core::currency::get_exchange_rate_and_currency` / `get_exchange_rates_cache` onto the effective-dated `db_core::exchange_rate::get_rate` semantics. No silent USD/`1.0` fallback.
- Audit trail for rate mutations: who changed what and when.
- Tests: effective-period resolution (past/current/future/single-row), explicit-reverse-over-derived preference, booking rate immutability after a rate edit, host 403 on writes, retired pair rejected at checkout, past `effective_at` rejected, delete allowed only for future rows, 5% guard.

### Out of Scope / Non-Goals
- Automated rate sync from an external provider (`EXCHANGE_RATE_API_KEY` exists but is not wired in). Deferred.
- Payment gateway integration and reconciling **booked vs. actually-settled** amounts (the issue notes they may differ). Only flagged here as a future concern.
- Re-pricing or back-filling existing bookings when a rate changes.
- Host-specific or listing-specific rate overrides.
- Cross-rate triangulation (e.g. GBP→JMD via USD) beyond the existing direct/inverse resolution.

---

## 4. Affected Systems & Stakeholders
- **User Personas**:
  - **Admin**: full CRUD on pairs and rates.
  - **Host**: read-only view of the current rate per pair (no history, no scheduled rates).
  - **Guest**: indirect. Sees browse and checkout prices derived from the current effective rate.
- **System Components**:
  - `db_core`: `currency_exchange_rates` table (new retirement marker and `created_by`), a new admin-managed `currencies` reference table with foreign keys from every currency column (see §7), `exchange_rate.rs` + `currency.rs` consolidation, new migration.
  - `common`: `exchange_rate.rs` DTOs (create/update requests, history response). No static currency list.
  - `booking_api`: `apis/ledger.rs` admin exchange-rate handlers + `apis/routes.rs` scope; `bookings.rs` checkout lookup (already effective-dated); `reservation_changes.rs` (must keep using `booking.exchange_rate`).
  - `listing_api`: `apis.rs` search/detail pricing (currently uses the non-effective-dated, fallback lookup).
  - `web_client`: client methods for new endpoints.
  - `web_ui_admin`: `pages/exchange_rates.rs` (role-gated controls, history view).
  - `openapi.yaml` / `openapi-prod.yaml`, `.sqlx/` offline metadata, Playwright `admin-portal/exchange-rates.spec.ts`.

---

## 5. Constraints & Non-Negotiable Invariants
- **Domain Invariants**:
  - `rust_decimal::Decimal` only. No `f32`/`f64` anywhere in the rate path, including form parsing.
  - A booking's `exchange_rate` is immutable once written. Rate edits must never change totals, GCT, tax documents (`exchange_rate`, `jmd_exchange_rate`), or `host_payout_ledger` rows of existing bookings.
  - Missing or retired rate → typed 4xx (`FX_RATE_UNAVAILABLE`), never a silent fallback rate or currency relabel.
  - Rate resolution is deterministic: `effective_at <= t ORDER BY effective_at DESC, updated_at DESC LIMIT 1`. An explicit pair always beats a derived reverse.
  - Retired pairs aren't used for new quotes, either directly or as the source for a derived reverse.
  - Rate history is append-only. "Update" inserts a new version. The only allowed removal is deleting a row whose `effective_at` is still in the future. Effective and past rows are immutable.
  - No row may be created with `effective_at` in the past (beyond the 60s grace period).
  - Every currency column in the schema, including history tables, references `currencies` (foreign key enforced, RESTRICT). No hardcoded currency lists in code.
  - Currencies are deactivated, never deleted.
- **Performance Budgets**: rate lookups stay single indexed queries (`idx_exchange_rates_pair`). Listing search must not add N+1 rate queries per result (dedupe per base currency, as the existing cache does). Cold start budget unchanged.
- **Security & Compliance**:
  - Writes are admin-only and enforced server-side in the API, not just hidden in the UI.
  - Every mutation is logged with actor ID (`tracing` + persisted audit record).
  - Migrations: new timestamped migration only. Never edit `20260408051402_…currency-exchange-rates.sql`.

---

## 6. Success Metrics & Signals
- **Leading Indicators**:
  - Admin can add a pair, publish a new rate, schedule a future rate, and retire a pair from the portal without DB access.
  - Host `POST`/`PUT`/`DELETE` on rate endpoints → 403. Host page renders with no mutation controls.
  - Test: book at rate R1 → admin publishes R2 → booking detail, reservation-change quote, tax document, and payout ledger all still use R1.
  - Test: a future-dated rate does not affect browse or checkout prices until its `effective_at`.
  - Test: hold created at R1 → R2 takes effect during the hold window → confirmation still uses R1.
  - Test: with only `USD → JMD` stored, `JMD → USD` is derived. After an explicit `JMD → USD` is added, the explicit value is used.
  - Test: a rate change of more than 5% without the confirm flag → 422. With the flag → saved and audited.
  - Test: deactivating a currency or retiring a pair that a listing uses → 409 `CURRENCY_IN_USE` listing those listings. After the listings change → succeeds.
  - Browse price and checkout price for the same listing/currency/instant are identical (one lookup path).
  - Zero code paths return a fallback `1.0` rate or relabel base-currency amounts as another currency.
- **Lagging Indicators**:
  - No manual SQL edits to `currency_exchange_rates` in prod after release.
  - No guest/host disputes caused by price drift between browse and checkout.

---

## 7. Open Questions & Policy Concerns
### Resolved (originator, 2026-11-01)
- [x] **Remove pair**: retire it and keep its history.
- [x] **Reverse pairs**: derive the reverse when no explicit reverse pair exists. Once an explicit reverse exists, use it in preference.
- [x] **Editing a rate**: a rate that hasn't taken effect can be deleted, and a new rate entered to replace it.
- [x] **Backdating**: not allowed. Nobody can enter a rate for a past date.
- [x] **Rate lock point**: when the hold is created.
- [x] **Allowed currencies**: admins can create new currency pairs between currencies that exist in the database.
- [x] **Typo guard**: yes, at a 5% deviation from the current effective rate.
- [x] **Host visibility**: current rate only.

- [x] **Pair registry**: no new pairs table. Pairs live in the existing `currency_exchange_rates` table, and retirement is a marker on that table (exact column shape is a Stage 2 detail).
- [x] **Currency code source**: database-driven. Every currency used in a pair must exist in the database. No hardcoded code lists in Rust or UI (this also removes `USD_TARGETS`).
- [x] **Explicit-reverse drift**: no guard across reverse directions. The 5% guard only compares a pair with its own current rate.
- [x] **Clock boundary for "now"**: add a short grace period. An `effective_at` up to the grace window in the past is accepted and stored as server `now()`. Anything older is rejected as backdated.

- [x] **Currency reference table**: add a new `currencies` table (code PK, name, active), seeded from every code already in use. `currency_exchange_rates.base_currency` and `target_currency` become foreign keys to it.
- [x] **Listings and users**: `listing.base_currency` and `user.default_currency` also become foreign keys to `currencies` in this iteration.
- [x] **Grace period length**: 60s, held in `application` settings (env-overridable like `pending_hold_minutes`).

- [x] **Who adds currencies**: admins add currencies through the admin portal (admin-only, same 403 rule for hosts).
- [x] **Every currency column links to `currencies`**, with no exceptions, including history/audit tables. The complete list today:

  | Table | Column(s) |
  |---|---|
  | `currency_exchange_rates` | `base_currency`, `target_currency` |
  | `listing` | `base_currency` |
  | `user` / `user_history` | `default_currency` |
  | `booking` / `booking_history` | `currency` |
  | `host_payout_ledger` | `currency` |
  | `tax_document` | `currency` |
  | `reservation_change_request` | `currency` |

  Any currency column added in future must reference `currencies` too.
- [x] **Consequence: currencies are never hard-deleted.** Foreign keys use `ON DELETE RESTRICT` and `ON UPDATE RESTRICT`. To stop using a currency, an admin **deactivates** it (`active = false`): it can't be picked for new pairs, listings, user defaults or checkouts, but every historical row that uses it stays valid.

- [x] **Deactivation blocked while listings use it**: deactivating a currency, or retiring a currency pair, is **blocked** if any listing still uses it. The admin sees the blocking listings (name + link to edit) so they can act, then retries. Typed 409 `CURRENCY_IN_USE` with the listing IDs in the payload.
  - Currency: blocked while any `listing.base_currency` = that code.
  - Pair `A → B`: blocked while any listing has `base_currency = A` (retiring it would stop those listings from quoting in `B`).

### Still open (Stage 2 design)
- [ ] **Migration safety**: the foreign key migration fails if any existing row holds a code that isn't seeded. Seed from `SELECT DISTINCT` across all affected columns, and normalise case and padding (`CHAR(3)` vs `VARCHAR(3)`) before adding constraints.
- [ ] **Gateway variance (future)**: when the payment gateway lands, where do booked vs. settled amount differences get recorded (payout ledger adjustment line vs. separate reconciliation table)? Out of scope here, but the schema shouldn't block it.
