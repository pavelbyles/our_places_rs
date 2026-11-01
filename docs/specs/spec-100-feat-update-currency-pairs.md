# Spec 100: Admin-Managed Currencies, Currency Pairs & Effective-Dated Exchange Rates

> Driven by GitHub Issue #100. Intent: [intent-100](../intent/intent-100-feat-update-currency-pairs.md) (Approved 2026-11-01).

## Overview
Makes currencies and exchange rates fully database-driven and admin-managed. A new `currencies` reference table becomes the single source of truth for every currency code in the schema (10 columns across 9 tables, each enforced by a foreign key). Admins add and deactivate currencies, create currency pairs, publish effective-dated rates (no backdating, 60s grace), delete rates that haven't taken effect yet, and retire pairs, all from `/admin/exchange-rates`. A 5% typo guard requires explicit confirmation. Hosts keep a read-only current-rate table. All guest-facing pricing (search, listing detail, quote, checkout hold) resolves rates through one effective-dated, fallback-free lookup that prefers an explicit pair over a derived reverse. A booking's base → payment rate is locked when its hold is created.

**Personas**: Admin (full control) · Host (read-only current rates) · Guest (indirect: consistent prices from browse through checkout).

---

## 1. Requirements & Domain Rules

### 1.1. Currencies
1. `currencies(code, name, active)`. `code` matches `^[A-Z]{3}$`, is the primary key, and is immutable. `name` is 1–64 characters and editable.
2. Admins create currencies and toggle `active`. **Currencies are never deleted** (all foreign keys use `ON DELETE RESTRICT ON UPDATE RESTRICT`).
3. **Deactivation is blocked** while any non-deleted listing (`listing.deleted_at IS NULL`, active or inactive) has `base_currency = code`. The response is `409 CURRENCY_IN_USE` with the blocking listings. Reactivation is always allowed.
4. An inactive currency can't be chosen for a new pair or rate, a listing's `base_currency` (create or change), a user's `default_currency` (registration or change), or a checkout/hold currency. Existing rows that already hold it stay valid.
5. No hardcoded currency lists anywhere. These are all removed and replaced by the active-currency list from the API: `web_ui_admin/src/pages/listings/form.rs::CURRENCIES`, `web_ui_guest/src/pages/checkout.rs::CURRENCIES`, `web_ui_common/.../currency_selector.rs::DISPLAY_CURRENCIES`, `web_ui_admin/src/pages/exchange_rates.rs::USD_TARGETS`.

### 1.2. Pairs & Rate Versions (`currency_exchange_rates`)
1. A **pair** `A → B` exists when at least one row has `(base_currency, target_currency) = (A, B)`. There is no separate pairs table.
2. **Publish rate** (creates the pair when it's new): `A != B`, both currencies active, `rate > 0` with at most 6 decimal places and `rate <= 999_999_999.999999` (`NUMERIC(15,6)`). A rate with more than 6 decimal places is **rejected, not rounded**.
3. **Effective time** (`application.rate_effective_grace_seconds`, default `60`, env `EA__APPLICATION__RATE_EFFECTIVE_GRACE_SECONDS`, must be positive at startup):
   - Omitted → stored as server `now()`.
   - `now − grace <= effective_at <= now` → stored as server `now()`.
   - `effective_at > now` → stored as given (scheduled).
   - `effective_at < now − grace` → `422 EFFECTIVE_AT_IN_PAST`.
4. **Typo guard**: the baseline is the explicit rate for `A → B` that will be in effect immediately before the new `effective_at` (the latest row with `effective_at < new.effective_at`). If `|new − baseline| / baseline × 100 > 5` and `confirm_large_change != true`, the response is `422 RATE_CHANGE_CONFIRMATION_REQUIRED` with `{ baseline_rate, proposed_rate, deviation_pct }`. When confirmed, the rate is saved and the audit log records `guard_overridden: true`. The guard doesn't apply when there is no explicit baseline (new pair). Derived reverses are never used as a baseline.
5. **Delete scheduled rate**: allowed only while `effective_at > now()`, checked atomically in the `DELETE ... WHERE` clause. Otherwise the response is `409 RATE_ALREADY_EFFECTIVE`. Effective and past rows are immutable. There is no update endpoint: a correction is a new version.
6. **Retire pair** `A → B`: sets `retired_at`/`retired_by` on all rows of the pair. History is kept. **Blocked** with `409 CURRENCY_IN_USE` while any non-deleted listing has `base_currency = A`.
7. **Reinstating a retired pair**: publishing a new rate for a retired pair clears `retired_at`/`retired_by` on its rows in the same transaction, and the audit log records `pair_reinstated`.

### 1.3. Rate Resolution
Single function `db_core::exchange_rate::get_rate(conn, base, target, at, RateUse)`:
1. `base == target` → `1`.
2. **Explicit first**: the latest `base → target` row with `effective_at <= at`. If there is one, use it.
3. **Derived reverse**: otherwise, the latest `target → base` row with `effective_at <= at` → `(1 / rate).round_dp(6)`.
4. Neither → `DbError::FxRateUnavailable` → `422 FX_RATE_UNAVAILABLE`. **There is never a fallback rate or currency.** The one exception is listing **search**, which marks the affected listings with "price unavailable" instead of failing the whole page (no price is shown, so no wrong amount is shown).
5. `RateUse` controls lifecycle filtering:

| `RateUse` | Callers | Retired pairs | Inactive currencies |
|---|---|---|---|
| `NewQuote` | listing search, listing detail (and therefore the guest quote), checkout hold creation | excluded | rejected (`422 CURRENCY_INACTIVE`) |
| `Settlement` | payout at confirmation (`booking/update.rs`), reservation-change payout (`reservation_change.rs`), tax-document JMD rate, booking display rate (`apis/mod.rs::display_rate`) | included | allowed |

> An explicit pair whose only rows are scheduled (no row effective yet) falls through to the derived reverse until its first rate takes effect.

### 1.4. Rate Locking (unchanged semantics, now guaranteed by tests)
| Conversion | Rate used | Change in this spec |
|---|---|---|
| Listing base → payment currency (`booking.exchange_rate`) | Effective at **hold creation**. Reused through confirmation, reservation changes and tax documents. | Tests only |
| Booking currency → host default currency (payout ledger) | Effective at **confirmation** (and at approval of a reservation change) | `RateUse::Settlement` only |
| Booking currency → JMD (tax documents) | Effective at **document issue date** | `RateUse::Settlement` only |

### 1.5. Visibility
| Capability | Admin | Host | Guest |
|---|---|---|---|
| Current rate table (explicit, non-retired pairs, rate effective now) | ✅ | ✅ | — |
| Pair history (past, current, scheduled, retired) | ✅ | ❌ 403 | — |
| Publish rate / delete scheduled rate / retire pair | ✅ | ❌ 403 | — |
| Currency list incl. inactive · create · edit name · toggle active | ✅ | ❌ 403 | — |
| Active currency list (selectors) | ✅ | ✅ | ✅ (public) |

### 1.6. Exclusions / Non-Goals
- Automated provider sync (`EXCHANGE_RATE_API_KEY`), triangulation via a third currency, and reconciling booked vs. settled gateway amounts.
- Per-currency minor units: all amounts stay `round_dp(2)`.
- Locking payout or JMD tax rates at hold time (decided: keep at confirmation and issue date).
- Legacy per-currency checkout routes (`/checkout-jmd/{id}` etc.) keep working. An inactive currency in one of them returns `422 CURRENCY_INACTIVE`.

---

## 2. Edge Cases & Failure States
| Case | Behaviour |
|---|---|
| Rate submitted "now" arrives 3s late | Within grace → stored as `now()`. |
| Rate with `effective_at` 2 minutes in the past | `422 EFFECTIVE_AT_IN_PAST`. |
| Admin deletes a scheduled rate just as it becomes effective | `DELETE ... WHERE effective_at > now()` affects 0 rows → `409 RATE_ALREADY_EFFECTIVE`. |
| Two admins publish rates for the same pair at once | `pg_advisory_xact_lock(hashtext('fx:' \|\| base \|\| ':' \|\| target))` serialises baseline read + insert. The second admin's guard compares against the first admin's rate if it's the new predecessor. |
| Same `(base, target, effective_at)` twice | Unique constraint → `409 RATE_VERSION_EXISTS`. |
| Explicit `USD→JMD` and `JMD→USD` disagree | Each direction uses its own explicit row. No cross-direction guard (decided). |
| Hold created at R1, R2 takes effect during the hold window | Confirmation uses `booking.exchange_rate = R1`. |
| Guest picks a currency with no resolvable pair to a listing's base | **Search**: the request succeeds. That listing comes back with `price_per_night = null`, `extra_guest_fee = null`, `base_currency` unchanged, and `price_unavailable_in = "<requested>"`. Its card shows "Price unavailable in <CUR>". Other listings are priced normally. **Detail, quote and checkout** → `422 FX_RATE_UNAVAILABLE`. The admin **coverage panel** (§3.3) lists every gap. |
| Currency deactivated while a guest has the checkout page open | Hold creation → `422 CURRENCY_INACTIVE`. The UI re-renders the selector. |
| Currency deactivated after holds or bookings exist in it | Unaffected: confirmation, payout and tax documents use `RateUse::Settlement`. |
| Pair retired while a hold in that pair is pending | Unaffected: rate already locked. Payout conversion uses `Settlement`. |
| Listing created with currency C while an admin deactivates C | Listing tx takes `FOR KEY SHARE` on the `currencies` row with an `active` check. Deactivation takes `FOR UPDATE` on the same row and then counts listings, so whichever commits second sees the other's result. |
| Host/user whose `default_currency` is deactivated | Existing value kept and still valid (FK). The profile form shows it as "(inactive)" and requires an active choice only when the field changes. |
| Seed/legacy data with lowercase or padded codes | The migration normalises (`upper(trim())`) before seeding and adding FKs. A non-`^[A-Z]{3}$` value aborts the migration with a clear `RAISE EXCEPTION`. |
| Admin purge of a user referenced by `created_by` / the audit log | `RESTRICT`. Admin accounts are already purge-protected. |

---

## 3. Technical Implementation

### 3.1. Data Model & Migration (`sqlx migrate add currency_reference_and_rate_management`)
New migration only. Never edit `20260408051402_…currency-exchange-rates.sql`.

```sql
-- 1. Reference table
CREATE TABLE currencies (
  code        VARCHAR(3)  PRIMARY KEY CHECK (code ~ '^[A-Z]{3}$'),
  name        VARCHAR(64) NOT NULL CHECK (length(trim(name)) > 0),
  active      BOOLEAN     NOT NULL DEFAULT true,
  created_by  UUID        NULL REFERENCES "user"(id),
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- 2. Normalise upper(trim()) on all 10 columns; RAISE EXCEPTION on any non-^[A-Z]{3}$ value
-- 3. Seed: USD, JMD, EUR, GBP, CAD (named) ∪ SELECT DISTINCT from all 10 columns (name = code if unknown)
-- 4. No column type changes: CHAR(3) columns (booking, booking_history, host_payout_ledger,
--    tax_document, reservation_change_request) can reference VARCHAR(3) directly
--    (verified on PostgreSQL 18.6, 2026-11-01)
-- 5. FKs (ON DELETE RESTRICT ON UPDATE RESTRICT) on:
--    currency_exchange_rates.base_currency, .target_currency, listing.base_currency,
--    "user".default_currency, user_history.default_currency, booking.currency,
--    booking_history.currency, host_payout_ledger.currency, tax_document.currency,
--    reservation_change_request.currency
-- 6. currency_exchange_rates: ADD created_by UUID NULL REFERENCES "user"(id),
--    retired_at TIMESTAMPTZ NULL, retired_by UUID NULL REFERENCES "user"(id),
--    CHECK (rate > 0), CHECK (base_currency <> target_currency)
-- 7. Audit log (append-only; BEFORE UPDATE OR DELETE trigger raises)
CREATE TABLE currency_audit_log (
  id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  actor_id        UUID NOT NULL REFERENCES "user"(id),
  action          VARCHAR(32) NOT NULL CHECK (action IN ('currency_created','currency_updated',
                    'currency_activated','currency_deactivated','rate_published',
                    'rate_deleted','pair_retired','pair_reinstated')),
  currency_code   VARCHAR(3) NULL REFERENCES currencies(code),
  base_currency   VARCHAR(3) NULL REFERENCES currencies(code),
  target_currency VARCHAR(3) NULL REFERENCES currencies(code),
  rate_id         UUID NULL,                 -- no FK: deleted scheduled rates stay auditable
  details         JSONB NOT NULL,            -- rate, effective_at, baseline, deviation_pct, guard_overridden
  created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_listing_base_currency_live ON listing (base_currency) WHERE deleted_at IS NULL;
```
Each step is checked against the migrations it depends on (`20261031010000_booking_hold_expires_at.sql` copies `booking` → `booking_history`, and the column type change must stay compatible). Run `cargo sqlx prepare --workspace` afterwards.

**Domain types (`common`)**
- `common/src/currency.rs` (new): `CurrencyCode` newtype (`TryFrom<&str>`, uppercase + `^[A-Z]{3}$`, no regex crate: byte check), `Currency { code, name, active }`, `CurrenciesResponse`, `CreateCurrencyRequest { code, name }`, `UpdateCurrencyRequest { name: Option, active: Option }`, `CurrencyInUse { listings: Vec<ListingRef { id, title, slug }> }`.
- `ListingResponse` (`common/src/models.rs`): add `price_unavailable_in: Option<String>` (`#[serde(skip_serializing_if = "Option::is_none")]`, documented). Set only by search, and only when the requested currency can't be resolved for that listing.
- `common/src/exchange_rate.rs` (extend): `PublishRateRequest { base_currency, target_currency, rate: Decimal /* string on the wire */, effective_at: Option<DateTime<Utc>>, confirm_large_change: bool }`, `RateVersion { id, rate, effective_at, created_by, state: RateState }`, `RateState { Past, Current, Scheduled }`, `PairHistoryResponse { base_currency, target_currency, retired_at, versions }`, `RateCoverageGap { listing_base_currency, target_currency }`.
- Pure functions (unit-tested, no DB): `deviation_pct(baseline, proposed) -> Decimal`, `RATE_GUARD_THRESHOLD_PCT = dec!(5)`, `requires_confirmation(baseline, proposed) -> bool`, `normalize_effective_at(requested, now, grace) -> Result<DateTime<Utc>, EffectiveAtError>`, `validate_rate(Decimal) -> Result<Decimal, RateError>` (>0, ≤6 decimal places, ≤ max).

**`db_core`**
- `currency.rs` (rewrite): `list_currencies(active_only)`, `create_currency`, `update_currency` (deactivate path: `SELECT … FOR UPDATE` on the currency row → listings in use → `CurrencyInUse`), `ensure_active(conn, code)` (`FOR KEY SHARE` + active check, used by listing/user/hold writes), `rate_coverage_gaps()`. **Delete** `get_exchange_rate_and_currency` and `get_exchange_rates_cache` (the silent USD/`1.0` fallbacks).
- `exchange_rate.rs`: add the `RateUse` parameter to `get_rate`/`get_rate_pool` (explicit-then-inverse in **one** `UNION ALL … ORDER BY derived, effective_at DESC, updated_at DESC LIMIT 1` query). Add `get_rates_for_bases(pool, &[String], target, at, RateUse) -> HashMap<String, Decimal>` (a single `= ANY($1)` query with `DISTINCT ON`, for search). Base currencies with no resolvable rate are **absent** from the map (not an error). The search handler sets `price_unavailable_in` for those listings and logs one `tracing::warn!` per missing `(base, target)` with the listing count. Add `publish_rate`, `delete_scheduled_rate`, `retire_pair`, `pair_history`, `list_current_rates` (filters `retired_at IS NULL`, `effective_at <= now()`). Every mutation writes `currency_audit_log` in the same transaction.
- `error.rs`: new `DbError` variants: `CurrencyInactive(String)`, `CurrencyInUse(Vec<ListingRef>)`, `EffectiveAtInPast`, `RateAlreadyEffective`, `RateVersionExists`, `RateChangeConfirmationRequired { baseline, proposed, deviation_pct }`, `CurrencyExists(String)`.
- Existing callers move to the new signature with the `RateUse` from §1.3. `listing_api` search uses `get_rates_for_bases`, and detail uses `get_rate_pool(.., NewQuote)`.
- Listing create/update (`listing_api`), user registration/profile (`user_api`) and hold creation (`booking_api/bookings.rs`) call `ensure_active` in their write transaction when the currency is new or changed.

### 3.2. API Endpoints (`booking_api`, owner of currencies and rates)
| Method & Path | Auth | Request | Success | Errors |
|---|---|---|---|---|
| `GET /api/v1/currencies` | public | — | `200 CurrenciesResponse` (active only) | — |
| `GET /api/v1/admin/currencies` | admin | — | `200` (all) | 401/403 |
| `POST /api/v1/admin/currencies` | admin | `CreateCurrencyRequest` | `201 Currency` | 400 `VALIDATION_ERROR`, 409 `CURRENCY_EXISTS` |
| `PATCH /api/v1/admin/currencies/{code}` | admin | `UpdateCurrencyRequest` | `200 Currency` | 404, 409 `CURRENCY_IN_USE` |
| `GET /api/v1/admin/exchange-rates` | admin, host | — | `200 ExchangeRatesResponse` (current, non-retired, explicit) | 401/403 |
| `GET /api/v1/admin/exchange-rates/coverage` | admin | — | `200 Vec<RateCoverageGap>` | 403 |
| `GET /api/v1/admin/exchange-rates/{base}/{target}/history` | admin | — | `200 PairHistoryResponse` | 403, 404 |
| `POST /api/v1/admin/exchange-rates` | admin | `PublishRateRequest` | `201 RateVersion` | 400, 409 `RATE_VERSION_EXISTS`, 422 `EFFECTIVE_AT_IN_PAST` / `CURRENCY_INACTIVE` / `RATE_CHANGE_CONFIRMATION_REQUIRED` |
| `DELETE /api/v1/admin/exchange-rates/{id}` | admin | — | `204` | 404, 409 `RATE_ALREADY_EFFECTIVE` |
| `POST /api/v1/admin/exchange-rates/{base}/{target}/retire` | admin | — | `200 PairHistoryResponse` | 404, 409 `CURRENCY_IN_USE` |

- Admin checks use `UserRole::Admin` server-side (`403 Forbidden` for hosts), not just hidden UI. Host access to the current table is unchanged.
- Error bodies: `{ "code", "message", ...details }`. `CURRENCY_IN_USE` includes `listings: [{id,title,slug}]`, and the confirmation error includes `baseline_rate`, `proposed_rate`, `deviation_pct` as strings.
- `#[utoipa::path]` on every handler. Regenerate `openapi.yaml` / `openapi-prod.yaml`. `#[tracing::instrument(skip(pool))]` with `actor_id`, `base`, `target` fields.
- `common/src/app_client` + `web_client`: typed methods for every endpoint. `web_client::active_currencies()` keeps an in-process cache with a **60s TTL** (single `RwLock<Option<(Instant, Arc<Vec<Currency>>)>>`), and admin mutations invalidate it in the admin process.

### 3.3. Frontend Views
**`web_ui_admin` `/admin/exchange-rates`** (`staff` for the page, `admin_only` for actions)
- **Current rates table** (`#exchange-rates-table`): both roles. Admins also get "History", "New rate" and "Retire" row actions.
- **Publish rate form** (admin): base/target `<select>` built from active currencies, rate text input (decimal string), optional `datetime-local` in UTC (blank = now). `hx-post`. On `RATE_CHANGE_CONFIRMATION_REQUIRED` it swaps in a DaisyUI `alert-warning` showing baseline → proposed (±x.xx%) with a "Confirm" button that resubmits with `confirm_large_change=true`.
- **Pair history page** `/admin/exchange-rates/{base}/{target}`: versions with `badge` Past/Current/Scheduled. Scheduled rows have a "Delete" button (`hx-delete`, confirm dialog).
- **Retire / deactivate blocked**: a modal lists the blocking listings with links to `/admin/listings/{id}/edit`.
- **Currencies card**: list (incl. inactive), add form (code, name), rename, activate/deactivate toggle (same blocked-modal flow).
- **Coverage gaps alert**: every `(listing base currency, active currency)` combination with no resolvable rate.
- Host view: the current rates table only. No history links, forms or coverage panel.
- Remove `USD_TARGETS` and the "Not set" rows. Listing form currency `<select>` and user form currency `<select>` come from active currencies. A row's own inactive value is shown as "CODE (inactive)" and kept selected.

**`web_ui_guest`**: `currency_selector` and checkout currency options come from `web_client::active_currencies()`. `villa_card` (`web_ui_common`): when `price_unavailable_in` is set, render "Price unavailable in <CUR>" (muted text, no amount) in place of the nightly price. The card still links to the listing, where the detail page shows the `FX_RATE_UNAVAILABLE` alert. A `422 CURRENCY_INACTIVE` / `FX_RATE_UNAVAILABLE` on quote or checkout renders the existing error alert with the message. No silent re-pricing.

### 3.4. Background Workers
None. `email_worker` test fixtures with `"USD"` strings are unaffected.

---

## 4. Performance Considerations
- Rate lookup: one indexed query (`idx_exchange_rates_pair`) instead of up to two. Search: one batched query per request instead of one or two per distinct base currency.
- Deactivate/retire in-use check: `idx_listing_base_currency_live` partial index. O(listings using the code).
- Active currency list: about 5–20 rows, cached 60s per UI instance. No added DB round-trip on guest page renders inside the TTL. No new dependencies, so cold start is unchanged.
- Migration: tables are small (5 villas). No table rewrites (column types unchanged). Normalisation and FK validation finish in well under a second. Run in a deploy window anyway because it takes `ACCESS EXCLUSIVE` locks.
- Advisory lock is scoped per pair and per transaction. There's no contention with booking holds (different keys, no row locks on `booking`).

## 5. Security Mitigations
- **AuthZ**: every mutation and history/coverage endpoint requires `UserRole::Admin`, checked in the handler from `claims.sub` → DB roles (not JWT claims alone). Hosts get 403. The public endpoint exposes only active `{code, name, active}`.
- **Input validation**: `CurrencyCode` newtype at the boundary, rates parsed as `Decimal` from strings (never floats), `name` trimmed and length-bounded, `effective_at` normalised server-side. Path `{base}/{target}` go through the same `CurrencyCode` parse before any query.
- **SQL**: `sqlx::query!` only. Advisory lock key is computed in SQL via `hashtext`, never string-concatenated into SQL.
- **Integrity**: DB `CHECK`s (`rate > 0`, `base <> target`, code format) and FKs back up app validation. The audit log is append-only via trigger.
- **CSRF / XSS**: admin forms use the existing session + HTMX conventions. Listing titles in the blocked modal are escaped by Topcoat `view!`.
- **Logging**: `tracing::warn!` on every 4xx business rejection (actor, pair, reason) and `tracing::error!` with the cause chain on 5xx. No PII beyond actor UUID.

---

## 6. Test Plan

### 6.1. Unit (`cargo test -p common`)
- `CurrencyCode`: accepts `USD`; rejects `usd` (before normalisation), `US`, `USDD`, `U$D`, empty.
- `validate_rate`: `0`, negative, 7 decimal places, over max → error. `0.000001` and `160.5` → ok.
- `deviation_pct` / `requires_confirmation`: exactly 5% → no confirmation. 5.000001% → confirmation. Rate drop and rate rise are symmetric.
- `normalize_effective_at`: `None` → now. now−59s → now. now−61s → `InPast`. now+1h → unchanged.

### 6.2. Integration (`cargo test -p db_core -p booking_api -p listing_api -p user_api`, local Postgres)
- Migration: every seeded/legacy code exists in `currencies`. Inserting `booking.currency = 'XXX'` fails with an FK violation. `DELETE FROM currencies` fails (RESTRICT).
- Resolution: explicit beats derived. Derived when only the reverse exists. A scheduled-only explicit pair falls through to derived. Future row ignored until `at`. Single row effective indefinitely. Missing → `FxRateUnavailable`. Retired pair excluded for `NewQuote` and included for `Settlement`.
- Publish: host → 403. Past `effective_at` → 422. >5% without flag → 422 with details. With flag → 201 + audit row `guard_overridden=true`. New pair → no guard. Publishing to a retired pair reinstates it.
- Delete: scheduled → 204 + audit. Effective → 409. Host → 403.
- Retire / deactivate: with a live USD listing → 409 listing the listing. A soft-deleted listing doesn't block. After the listing changes → success.
- Rate locking: hold at R1 → publish R2 effective now → confirm → `booking.exchange_rate = R1`. Payout uses the confirmation-time rate. Tax document uses the issue-date JMD rate.
- Parity: listing detail price (`?currency=JMD`) == checkout nightly rate at the same instant. Search with one unresolvable listing → 200. That listing has null prices + `price_unavailable_in`, and the others are converted (no USD relabel, no rate `1`). Detail with the same pair → 422.
- Inactive currency: hold creation / listing create / user default change → 422 `CURRENCY_INACTIVE`. An existing booking in that currency still confirms and issues tax documents.
- Concurrency: concurrent listing insert with currency C vs. deactivating C → exactly one succeeds. Never ends with a live listing on an inactive currency.

### 6.3. Web UI (`cargo test -p web_client -p web_ui_admin -p web_ui_guest`)
- No `CURRENCIES` / `DISPLAY_CURRENCIES` / `USD_TARGETS` constants remain (`rg` check in the sanity script).
- The admin page renders publish, history and retire controls for admins only. The host render has none.
- The active-currency cache returns the cached list within TTL and refetches after it.
- `villa_card` with `price_unavailable_in = Some("CAD")` renders "Price unavailable in CAD" and no amount.

### 6.4. E2E (Playwright, `playwright/e2e/admin-portal/exchange-rates.spec.ts` + new `currencies.spec.ts`)
Serial per project. Each browser project uses its own scratch currency and dedicated seeded pair, keyed by `testInfo.project.name` (e.g. chromium `XCD` / `USD→XCD`, firefox `TTD` / `USD→TTD`), and never `USD→JMD`. Specs must pass when rerun against the same DB. The webServer env sets `CURRENCY_CACHE_TTL_SECONDS=0` with `reuseExistingServer: false`.
- Admin adds the project's scratch currency (tolerating "already exists") → adds the project's pair → the guest currency selector shows it.
- Admin publishes a rate 10% off on the project's pair → confirmation alert → confirm → table updated.
- Admin schedules a rate for tomorrow → history shows Scheduled → delete → gone.
- Admin retires the project's pair while a seeded listing uses its base → blocked modal lists the listing.
- Host sees the current table only. A direct POST to the publish endpoint as host → 403.
- Update the existing `USD / EUR` assertion to reflect DB-driven rows (no "Not set" placeholder).

---

## 7. Acceptance Criteria
- [ ] 1. A new migration creates `currencies` and `currency_audit_log`, normalises codes, and adds RESTRICT FKs to all 10 currency columns. `cargo sqlx prepare --workspace --check` passes.
- [ ] 2. No hardcoded currency lists remain in Rust UI or API code. Selectors come from `GET /api/v1/currencies`.
- [ ] 3. Admins can create, rename, activate and deactivate currencies. Deactivation and pair retirement are blocked with `409 CURRENCY_IN_USE` listing the live listings.
- [ ] 4. Admins can publish rates (now or scheduled, 60s grace, no backdating), delete scheduled rates, retire and reinstate pairs, and view pair history. Every action is in `currency_audit_log`.
- [ ] 5. The 5% guard requires explicit confirmation and records overrides.
- [ ] 6. Hosts see only the current rate table. Every other endpoint returns 403 for hosts.
- [ ] 7. `db_core::currency::get_exchange_rate_and_currency` / `get_exchange_rates_cache` are deleted. All lookups go through `exchange_rate::get_rate*` with an explicit `RateUse`. No code path yields a fallback rate or relabels currency.
- [ ] 8. Explicit pair preferred over derived reverse. Derived used only when no effective explicit row exists.
- [ ] 9. `booking.exchange_rate` is locked at hold creation. Payout (confirmation-time) and tax-document JMD (issue-date) behaviour is unchanged and covered by tests.
- [ ] 10. Browse, detail, quote and checkout produce identical converted prices for the same listing, currency and instant. A search listing that can't be converted shows "Price unavailable in <CUR>" while the rest of the results render.
- [ ] 11. CONSTRAINTS.md gates pass: fmt, clippy `-D warnings`, `missing_docs`, no `unwrap/expect`, no floats in the rate path, all backend + web UI tests green, OpenAPI regenerated.

---

## 8. Decisions Log
- [x] **Search with an unresolvable pair** (originator, 2026-11-01): show that listing without a price, marked "Price unavailable in <CUR>". The rest of the search renders normally.
- [x] **Payout rate**: rate at confirmation (unchanged). **JMD tax-document rate**: issue date (unchanged).
- [x] **Column types**: no `CHAR → VARCHAR` conversion. A `CHAR(3)` FK → `VARCHAR(3)` PK was verified on PostgreSQL 18.6.

---

## 9. Engineering Review Amendments (panel rev 1 → plan rev 2, 2026-11-01)
These amendments **supersede** any conflicting text above. Task-level detail is in the plan's Cross-Cutting Rules.
1. **Clock**: db_core rate/currency functions take an explicit `now`. `publish_rate` reads `clock_timestamp()` **after** the pair advisory lock, so concurrent "now" publishes store in commit order and the second guard compares against the first.
2. **Pair lock everywhere**: `publish_rate`, `delete_scheduled_rate` and `retire_pair` all take `pg_advisory_xact_lock(hashtext('fx:'||base||':'||target))`. Lock order: `listing` → `currencies` → pair lock, and never a `currencies` row lock after the pair lock (publish, delete and retire first lock **every** currency row they will reference, including rows only touched implicitly by FK checks on the rate and audit-log inserts, in ascending code order: `FOR UPDATE` on the base for retire, `FOR KEY SHARE` otherwise. Only then do they take the pair lock). The migration's upfront `LOCK TABLE` follows the app's table order (`listing`, `"user"`, `user_history`, `currency_exchange_rates`, `booking`, `booking_history`, `reservation_change_request`, `host_payout_ledger`, `tax_document`).
3. **Retire block also covers the reverse**: retiring `A → B` is also blocked by live listings with `base_currency = B` when no explicit non-retired `B → A` row exists, because those listings price through the derived reverse.
4. **`ensure_active` runs in the write transaction**: listing create (now wrapped in a tx), listing update, user registration (wrapped in a tx) and update (already transactional), and hold creation inside `booking/hold.rs` after `listing FOR UPDATE`. Unknown code on a write → 422 `CURRENCY_NOT_FOUND` (`DbError::CurrencyUnknown`, also for SQLSTATE 23503). Inactive → 422 `CURRENCY_INACTIVE`. Admin `PATCH` on an unknown code → 404 (`DbError::CurrencyNotFound`).
5. **Search input**: `?currency=` is parsed as `CurrencyCode` (400) and must be active (422) before any lookup. A base equal to the target always converts at `1`. Missing pairs log one aggregated `warn!` per request.
6. **Effective-time bounds**: grace is `0 < grace ≤ 300s` (validated at startup). `effective_at` must be ≤ now + 366 days (`422 EFFECTIVE_AT_TOO_FAR`). Rate strings use strict decimal syntax and are normalised before the 6-decimal-place check.
7. **Guard math**: `|new − base| × 100 > 5 × base`, with no division. `deviation_pct` is for display only.
8. **AuthZ**: admin routes use `require_admin` (403). The staff current-rate GET uses `require_staff` and returns **403** for non-staff (was 401). `actor_id` always comes from the server-loaded user.
9. **Portal**: no `hx-delete`. Deleting a scheduled rate is `POST /admin/exchange-rates/rates/{id}/delete`. All mutating forms use `hx-disabled-elt`.
10. **Audit log**: also protected against `TRUNCATE`. `rate_deleted` details come from the `DELETE … RETURNING` row.
11. **Migration safety**: `SET LOCAL lock_timeout='5s'` + one ordered `LOCK TABLE` up front. Pre-checks `RAISE` on invalid codes or rate rows. Normalise `UPDATE`s use `WHERE col IS DISTINCT FROM upper(trim(col))` so they don't rewrite `host_payout_ledger.updated_at`.
12. **Currency cache**: TTL from `common::app_client::currency_cache_ttl()` (env `CURRENCY_CACHE_TTL_SECONDS` via a named constant) (default 60; 0 disables, used by E2E). Refresh is single-flight. On a fetch error the stale list is served with a `warn!` (display only). A guest whose stored currency is inactive is prompted to choose one.
13. **Tests**: named test modules with minimum counts, `Migrator`-based legacy-data migration tests, `lock_timeout` + SQLSTATE `55P03` concurrency tests, serial, rerun-safe Playwright specs with a separate scratch currency and seeded pair per browser project.
14. **Lock contention**: SQLSTATE 40P01 (deadlock) and 55P03 (lock timeout) map to retryable `503 SERVICE_UNAVAILABLE` with `Retry-After`, never a bare 500.
