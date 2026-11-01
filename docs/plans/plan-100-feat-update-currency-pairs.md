# Implementation Plan: Admin-Managed Currencies, Pairs & Effective-Dated Rates (#100)

**Spec Reference:** [`docs/specs/spec-100-feat-update-currency-pairs.md`](../specs/spec-100-feat-update-currency-pairs.md)
**Intent:** [`docs/intent/intent-100-feat-update-currency-pairs.md`](../intent/intent-100-feat-update-currency-pairs.md)
**Baseline Commit:** `eb05d61a`
**Branch:** `feat/100-update-currency-pairs`

> **Engineering Review Status**: LOCKED (rev 3)
> **Review Date**: 2026-11-01
> **Review Panel**: Lead Architect, Edge Case Analyst, Security Reviewer, DB Guardian, QA Strategist

## Cross-Cutting Rules (apply to every task)
- **One clock**: every db_core rate/currency function takes `now: DateTime<Utc>`, with no SQL `now()` in WHERE clauses. Handlers read the time **after** taking any advisory lock: `publish_rate` reads `clock_timestamp()` inside the transaction after the lock and uses it as `now`.
- **Lock order** (no cycles): `listing FOR UPDATE` → `currencies FOR KEY SHARE | FOR UPDATE` → pair advisory lock `pg_advisory_xact_lock(hashtext('fx:'||base||':'||target))`. **Never take a `currencies` row lock after the pair lock**, and that includes the **implicit** `FOR KEY SHARE` taken by FK checks on inserts into `currency_exchange_rates` and `currency_audit_log`. So every rate transaction (publish, delete, retire) first locks **every** currency row it will reference, **in ascending code order**, using one helper `lock_pair_currencies(tx, base, target, base_mode)`: `FOR UPDATE` on the base for retire, `FOR KEY SHARE` otherwise. Only then does it take the pair lock. The later FK checks then hit rows the tx already holds and never wait. Deactivate/retire transactions stay short, with no I/O inside.
- **Advisory lock in sqlx**: `SELECT pg_advisory_xact_lock(hashtext($1)) IS NULL AS "_locked!"` (a `void` return doesn't decode).
- **AuthZ**: every admin API route calls `super::require_admin` (403 `Forbidden` + `warn!`). Staff-readable routes call a new `require_staff` helper (403, not 401). `actor_id` always comes from the user loaded by the helper, never from the request body.
- **Portal mutations** are `POST` `#[page]`s whose first statement is `admin_only(cx)`. No `hx-delete`/`PATCH` in the portal. Mutating forms use `hx-disabled-elt="find button"` to stop double submits.
- **Error mapping**: every new `DbError` variant has explicit arms in **both** `status_code` and `error_response`. SQLSTATE 23505 (unique) and 23503 (FK) on currency writes map to `CURRENCY_EXISTS` / `RATE_VERSION_EXISTS` / `CURRENCY_NOT_FOUND`, never a bare 500. SQLSTATE **40P01** (deadlock, e.g. the rare admin "change own default currency to C" vs "deactivate C" race) and **55P03** (lock timeout) map to the existing retryable **503 `SERVICE_UNAVAILABLE`** with `Retry-After`, plus a `warn!` naming the operation.
- **Tests**: new tests live in **named modules** (`mod <name>_tests`). Each task's verification filter must match ≥ the stated test count, because a filter that matches nothing shows "0 passed" and is not green. Concurrency tests use two TestPg connections with `SET LOCAL lock_timeout='200ms'` and assert SQLSTATE `55P03`, never sleeps. Test-only DB setup uses runtime `sqlx::query().bind()`.
- **SQLx metadata**: `cargo sqlx prepare --workspace -- --all-targets`.
- **Checkpoint gates** (A–D): `cargo fmt --check`, clippy `-D warnings`, clippy `-D missing_docs`, the CONSTRAINTS `rg` gates (unwrap/expect, floats incl. `common/src/exchange_rate.rs` + `db_core/src/exchange_rate.rs`, env::var), and backend + web UI tests.

## Dependency Graph
Each line is `task ← dependencies`, matching every task's **Dependencies** field.
```
T1  migration            ← —
T2  common currency      ← —
T3  common rate rules    ← —
T4  errors               ← T2, T3
T5  get_rate + RateUse   ← T1, T4
T6  callers              ← T5
T7  search/detail        ← T6, T8
T7b rate-lock tests      ← T6
T8  db currency          ← T1, T2, T4
T9  API currency         ← T8
T10 enforce active       ← T8
T11 client + cache       ← T9
T12a admin selects       ← T11
T12b guest selects       ← T11
T13 currencies card      ← T11
T14 publish_rate         ← T3, T5, T8
T15 delete/retire/hist   ← T14
T16 API rates            ← T15
T17 client rates         ← T16
T18 admin rates page     ← T13, T17
T19 history/retire UI    ← T18
T20 villa card           ← T7
T21 E2E                  ← T12b, T19, T20   (transitively T10, T12a via checkpoints C/D)
T22 docs + gates         ← T21
```
Critical path: T2/T3 → T4 → T8 → T14 → T15 → T16 → T17 → T18 → T19 → T21 → T22.

---

## Phase 1 — Foundation

- [ ] **T1: Currency reference migration**
  - **Scope:** M
  - **Description:** `sqlx migrate add --source db_core/migrations currency_reference_and_rate_management`. The order inside the migration:
    0. `SET LOCAL lock_timeout = '5s'`, then **one** `LOCK TABLE listing, "user", user_history, currency_exchange_rates, booking, booking_history, reservation_change_request, host_payout_ledger, tax_document IN ACCESS EXCLUSIVE MODE`. This matches the order the app touches the tables (hold: `listing FOR UPDATE` → `booking` insert → history; confirmation → payout ledger → tax document). A `lock_timeout` (55P03) or deadlock (40P01) aborts the single migration transaction cleanly; retry it in a quiet window.
    1. **Pre-checks** that `RAISE EXCEPTION` naming the table, column and value for: any code not matching `^[A-Z]{3}$` after `upper(trim())`; any `currency_exchange_rates` row with `rate <= 0` or `base = target`.
    2. Normalise each of the 10 columns with `UPDATE … SET col = upper(trim(col)) WHERE col IS DISTINCT FROM upper(trim(col))`. The `WHERE` matters: without it, the `host_payout_ledger.updated_at` trigger would rewrite every row.
    3. Create `currencies` and seed it (USD/JMD/EUR/GBP/CAD named, ∪ distinct codes found).
    4. **Column types: no change.** It was verified on 2026-11-01 against the local devenv PostgreSQL 18.6 that a `CHAR(3)` FK → `VARCHAR(3)` PK is created and enforced. The 5 `CHAR(3)` columns stay as they are, so there is no table rewrite and no `.sqlx` `Bpchar` churn.
    5. RESTRICT FKs on all 10 columns.
    6. Rate table: add `created_by`, `retired_at`, `retired_by`, `CHECK (rate > 0)`, `CHECK (base_currency <> target_currency)`.
    7. `currency_audit_log` + a `BEFORE UPDATE OR DELETE` row trigger **and** a `BEFORE TRUNCATE` statement trigger, both of which raise.
    8. `idx_listing_base_currency_live`.
  - **Files Touched:**
    - `db_core/migrations/<ts>_currency_reference_and_rate_management.sql` (new)
    - `db_core/src/currency.rs` (`mod migration_tests`)
    - `db_core/src/exchange_rate.rs` (tests: the existing `'FOO'/'BAR'` fixtures must insert those currencies first or switch to seeded codes)
    - `.sqlx/*.json`
  - **Acceptance Criteria:**
    - [ ] `migration_tests` (≥5 tests) uses `sqlx::migrate::Migrator` to apply migrations up to the **previous** version and inserts dirty data (`'usd '`, `'jmd'`, plus one invalid code in a separate test). It then applies the new migration and asserts the codes are normalised and the invalid code raises an error naming the table and column.
    - [ ] FK violations on `booking.currency` and `user_history.default_currency`. `DELETE FROM currencies` fails. `UPDATE`/`DELETE`/`TRUNCATE` on `currency_audit_log` raise. `CHECK`s reject `rate <= 0` and `base = target`.
    - [ ] A normalise run that has nothing to change leaves `host_payout_ledger.updated_at` untouched.
    - [ ] `rg "INSERT INTO currency_exchange_rates" --type rust` shows only seeded or pre-inserted codes. All existing tests pass.
  - **Verification Command:** `sqlx migrate run --source db_core/migrations && cargo sqlx prepare --workspace -- --all-targets && cargo test -p db_core migration_tests && cargo test -p db_core exchange_rate`
  - **Dependencies:** None

- [ ] **T2: `common` currency domain types**
  - **Scope:** S
  - **Description:** New `common/src/currency.rs`: the `CurrencyCode` newtype (byte check `^[A-Z]{3}$`, `TryFrom<&str>`, serde transparent + validating), `Currency`, `CurrenciesResponse`, `CreateCurrencyRequest` (name trimmed, 1–64), `UpdateCurrencyRequest` (at least one field, otherwise a validation error), `ListingRef`, `CurrencyInUse`. Everything is `///` documented and has `ToSchema`.
  - **Files Touched:** `common/src/currency.rs` (new), `common/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] `mod currency_tests` (≥8 tests): `USD` is accepted; `usd`, `US`, `USDD`, `U$D`, empty, `" USD"` are rejected. Serde rejects invalid codes. Name empty or 65 characters → error. Empty update → error.
  - **Verification Command:** `cargo test -p common currency_tests`
  - **Dependencies:** None

- [ ] **T3: `common` rate rules & DTOs**
  - **Scope:** S
  - **Description:** Extend `common/src/exchange_rate.rs`:
    - `parse_rate(&str)`: strict decimal syntax (no exponent, sign, comma or surrounding whitespace). Apply `normalize()` before the ≤6 decimal-places check, so `"160.5000000"` is accepted. Require >0 and ≤ `999_999_999.999999`.
    - `requires_confirmation(baseline, proposed)`: computed as `(proposed − baseline).abs() * 100 > RATE_GUARD_THRESHOLD_PCT * baseline`, with no division. `deviation_pct` is only used for display (`round_dp(2)`).
    - `normalize_effective_at(requested, now, grace, horizon)`: `InPast` for anything before now − grace, and `TooFarAhead` beyond `now + 366 days`.
    - DTOs: `PublishRateRequest` (rate is a string), `RateVersion`, `RateState`, `PairHistoryResponse`, `RateCoverageGap`.
  - **Files Touched:** `common/src/exchange_rate.rs`
  - **Acceptance Criteria:**
    - [ ] `mod rate_rules_tests` (≥14 tests):
      - Exactly 5% → no confirmation. 5.000001% → confirmation. Rises and drops are treated the same.
      - Grace: now−59s → now. now−61s → `InPast`. Horizon: +366d ok, +367d → `TooFarAhead`.
      - Rate parsing: `0`, `-1`, `1e3`, `+160`, `" 160 "`, `160,5` and 7 significant decimal places are rejected. `160.5000000` and `0.000001` are accepted.
  - **Verification Command:** `cargo test -p common rate_rules_tests`
  - **Dependencies:** None

- [ ] **T4: Error variants & HTTP mapping**
  - **Scope:** S
  - **Description:** Add `DbError` variants: `CurrencyNotFound` (404, admin `PATCH /currencies/{code}` only), `CurrencyUnknown` (422 `CURRENCY_NOT_FOUND`, used by `ensure_active` and for 23503 on writes), `CurrencyInactive`, `CurrencyInUse(Vec<ListingRef>)`, `EffectiveAtInPast`, `EffectiveAtTooFar`, `RateAlreadyEffective`, `RateVersionExists`, `RateChangeConfirmationRequired{baseline,proposed,deviation_pct}`, `CurrencyExists`. Each gets explicit arms in `status_code` **and** `error_response`, plus a `warn!` on business 4xx. Map SQLSTATE 23505/23503 to these variants at the db_core boundary.
  - **Files Touched:** `db_core/src/error.rs`, `app_api/api_core/src/error.rs`
  - **Acceptance Criteria:** [ ] `mod currency_error_tests` (≥12, incl. 40P01 → 503 and 55P03 → 503 with `Retry-After`): status + JSON `code` for each variant. `CURRENCY_IN_USE` includes `listings`. Confirmation-required includes string decimals.
  - **Verification Command:** `cargo test -p api_core currency_error_tests`
  - **Dependencies:** T2, T3

### ✅ Checkpoint A — full gate set (see Cross-Cutting Rules)

---

## Phase 2 — Unified Rate Resolution (high risk)

- [ ] **T5: `get_rate` with `RateUse`, explicit-first single query**
  - **Scope:** S
  - **Description:** Add the `RateUse { NewQuote, Settlement }` parameter. A single `UNION ALL` query returns `(rate, derived)` with `ORDER BY derived, effective_at DESC, updated_at DESC LIMIT 1`. The inverse is computed **in Rust** (`(Decimal::ONE / r).round_dp(6)`), never with SQL `round()`. `NewQuote` filters `retired_at IS NULL`, which also covers derivation from a retired pair, and requires both currencies active. `Settlement` filters on neither.
  - **Files Touched:** `db_core/src/exchange_rate.rs`
  - **Acceptance Criteria:**
    - [ ] `mod get_rate_tests` (≥9, all with a fixed `at`):
      - explicit beats derived
      - derived when only the reverse exists
      - a scheduled-only explicit pair falls through to derived
      - a future row is ignored
      - a single row stays effective indefinitely
      - missing → `FxRateUnavailable`
      - a retired pair (direct **and** as a derived source) is excluded for `NewQuote` and used for `Settlement`
      - an inactive currency → `CurrencyInactive` for `NewQuote` only
      - inverse rounding matches `round_dp(6)` at a midpoint
  - **Verification Command:** `cargo test -p db_core get_rate_tests`
  - **Dependencies:** T1, T4

- [ ] **T6: Move existing rate callers to explicit `RateUse`**
  - **Scope:** S (mechanical)
  - **Description:** Hold creation uses `NewQuote`. Display, payout at confirmation, reservation-change payout and tax-document JMD use `Settlement`.
  - **Files Touched:** `app_api/booking_api/src/apis/bookings.rs`, `app_api/booking_api/src/apis/mod.rs`, `db_core/src/booking/update.rs`, `db_core/src/reservation_change.rs`, `db_core/src/tax_document.rs`
  - **Acceptance Criteria:**
    - [ ] Existing suites pass unchanged.
    - [ ] New `mod hold_rate_use_tests` (≥2 in booking_api): a hold on a retired pair → 422 `FX_RATE_UNAVAILABLE`; a hold in an inactive currency → 422 `CURRENCY_INACTIVE`.
  - **Verification Command:** `cargo test -p db_core -p booking_api`
  - **Dependencies:** T5

- [ ] **T7: Search/detail on the unified lookup, and fallbacks removed**
  - **Scope:** M
  - **Description:**
    - Parse `?currency=` with `CurrencyCode` (400 on failure). Require it to be active (422 `CURRENCY_INACTIVE`) **before** any rate lookup.
    - Add `get_rates_for_bases(pool, &[String], target, now, NewQuote)`. Bases equal to the target are put in the map as `1` in Rust. Other missing bases are absent from the map.
    - Search sets null prices + `price_unavailable_in` and logs one aggregated `warn!` per request (missing bases + count), not one per base.
    - Detail uses `get_rate_pool(.., NewQuote)`.
    - Add `ListingResponse.price_unavailable_in`.
    - **Delete** `currency::get_exchange_rate_and_currency` / `get_exchange_rates_cache`.
  - **Files Touched:** `db_core/src/exchange_rate.rs`, `db_core/src/currency.rs`, `app_api/listing_api/src/apis.rs`, `common/src/models.rs`, `app_api/listing_api/src/apis_test.rs`
  - **Acceptance Criteria:**
    - [ ] `mod currency_conversion_tests` (≥7):
      - one unresolvable listing → 200 with null prices + flag, and the others converted
      - a base equal to the target is never flagged
      - `?currency=zz1` → 400
      - an inactive target → 422
      - detail for an unresolvable pair → 422
      - a future-dated rate doesn't change prices
      - detail `?currency=JMD` == checkout `base_nightly_rate` at the same instant
    - [ ] `rg "get_exchange_rate_and_currency|get_exchange_rates_cache"` → none.
  - **Verification Command:** `cargo test -p listing_api currency_conversion_tests`
  - **Dependencies:** T6, T8 (`ensure_active` for the target-currency check)

- [ ] **T7b: Rate-lock regression tests**
  - **Scope:** S
  - **Description:** In `mod rate_lock_tests` (booking_api):
    - Hold at R1 → R2 effective → confirm: `booking.exchange_rate == R1`.
    - The payout uses the host rate at confirmation time.
    - The tax document uses the JMD rate at issue time.
    - A booking in a currency deactivated since the hold, or a pair retired since, still confirms and issues documents.
  - **Files Touched:** `app_api/booking_api/src/apis/tests.rs`
  - **Acceptance Criteria:** [ ] ≥4 tests asserting the stored values.
  - **Verification Command:** `cargo test -p booking_api rate_lock_tests`
  - **Dependencies:** T6

### ✅ Checkpoint B — full gate set

---

## Phase 3 — Currency Management (vertical slice)

- [ ] **T8: `db_core` currency CRUD, `ensure_active`, in-use check**
  - **Scope:** M
  - **Description:**
    - `list_currencies(active_only)`, `create_currency`, `update_currency`.
    - `update_currency` with all fields unchanged is a no-op with no audit row. Deactivation runs `FOR UPDATE` on the currency row → live listings → `CurrencyInUse`.
    - `ensure_active(conn, code)`: selects `FOR KEY SHARE`, then checks `active` in Rust, so `CurrencyUnknown` and `CurrencyInactive` stay distinct.
    - `rate_coverage_gaps(now)`.
    - Audit rows are written in the same tx. `actor_id` is a parameter supplied by the handler from `require_admin`.
  - **Files Touched:** `db_core/src/currency.rs`, `db_core/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] `mod currency_admin_tests` (≥10):
      - duplicate → `CurrencyExists`
      - deactivating with a live listing → `CurrencyInUse`, and a soft-deleted listing doesn't block
      - reactivation works
      - a no-op update writes no audit row
      - unknown vs inactive are distinct
      - coverage gaps listed
      - **concurrency, both orders**: (a) tx A holds `FOR KEY SHARE` while B's deactivation hits `lock_timeout` 55P03; after A commits, B retried → `CurrencyInUse`. (b) Deactivation commits first, then the listing write → `CurrencyInactive`.
  - **Verification Command:** `cargo test -p db_core currency_admin_tests`
  - **Dependencies:** T1, T2, T4

- [ ] **T9: Currency endpoints in `booking_api`**
  - **Scope:** M
  - **Description:** New `apis/currencies.rs`.
    - `GET /api/v1/currencies` is registered **outside** every `require_user` scope.
    - `/api/v1/admin/currencies` gets its own `web::scope(..).wrap(from_fn(require_user))`, with `require_admin` in each handler.
    - The path `{code}` is parsed with `CurrencyCode` (400).
  - **Files Touched:** `app_api/booking_api/src/apis/currencies.rs` (new), `apis/mod.rs`, `apis/routes.rs`, `apis/authz_tests.rs`, `apis/tests.rs`
  - **Acceptance Criteria:** [ ] `mod currency_api_tests` (≥10):
    - the public list needs no auth and returns active currencies only
    - the admin list includes inactive currencies
    - host → 403 and anonymous → 401 on every admin route
    - invalid code → 400
    - name empty or too long → 400
    - PATCH unknown → 404
    - duplicate → 409
    - deactivating a currency in use → 409 with listings
  - **Verification Command:** `cargo test -p booking_api currency_api_tests`
  - **Dependencies:** T8

- [ ] **T10: Enforce active currency inside write transactions**
  - **Scope:** M
  - **Description:** Call `ensure_active(&mut tx, code)` in the **same transaction** as each write that sets a new or changed currency:
    - `create_listing` (wrap it in a tx; today it autocommits) and `update_listing_with_conn`
    - user registration (wrap in a tx) and `update_user` (already transactional, `db_core/src/user.rs:142`)
    - the hold in `db_core::booking::hold::create_booking`, right after `listing … FOR UPDATE`

    An unchanged inactive value is left alone.
  - **Files Touched:** `db_core/src/listing/write.rs`, `db_core/src/user.rs`, `db_core/src/booking/hold.rs`, `app_api/listing_api/src/apis.rs`, `app_api/user_api/src/apis/registration.rs`, `app_api/user_api/src/apis/profile.rs`
  - **Acceptance Criteria:** [ ] `mod currency_inactive_tests` in listing_api, user_api and booking_api (≥7 in total):
    - an inactive currency on listing create, listing update, registration, profile change or hold → 422
    - keeping an unchanged inactive value → 200
    - an unknown code → 422 `CURRENCY_NOT_FOUND` (no 23503 500)
  - **Verification Command:** `cargo test -p listing_api -p user_api -p booking_api currency_inactive_tests`
  - **Dependencies:** T8

- [ ] **T11: Client methods and active-currency cache**
  - **Scope:** S
  - **Description:** Add `common/src/app_client/currencies.rs`. The TTL comes from a new `common::app_client::currency_cache_ttl()` next to the existing `booking_api_url()` helpers. It reads `static CURRENCY_CACHE_TTL_ENV: &str = "CURRENCY_CACHE_TTL_SECONDS"` (a named constant, per the env-var gate). The default is 60 and 0 means no cache. An unparseable value logs a `warn!` and uses 60; it's a display cache, not money. The cache is an `Arc` field on `TopcoatApiClient` set up by its constructors. `active_currencies()` then provides:
    - injectable clock
    - **single-flight** refresh after expiry
    - on fetch failure, **serve the stale list with a `warn!`** (display only, never money); with no cache at all, return an error
    - lock poisoning maps to an error, no unwrap

    Also add `invalidate_currencies()` and the admin methods.
  - **Files Touched:** `common/src/app_client/currencies.rs` (new), `common/src/app_client/mod.rs`, `web_client/src/client.rs`
  - **Acceptance Criteria:** [ ] `mod currency_cache_tests` (≥5): no second call within TTL; refetch after TTL or invalidate; TTL 0 always fetches; stale served on error; concurrent misses → 1 fetch.
  - **Verification Command:** `cargo test -p web_client currency_cache_tests`
  - **Dependencies:** T9

- [ ] **T12a: Admin selects driven by the DB**
  - **Scope:** S
  - **Description:** Remove the hardcoded lists. Pure render fns take `&[Currency]` plus the current value. An inactive current value renders as `CODE (inactive)` and stays selected.
  - **Files Touched:** `web_ui_admin/src/pages/listings/form.rs`, `web_ui_admin/src/pages/users/form.rs`, `web_ui_admin/src/pages/users/mod.rs`
  - **Acceptance Criteria:** [ ] `mod currency_select_tests` (≥3 render tests).
  - **Verification Command:** `cargo test -p web_ui_admin currency_select_tests`
  - **Dependencies:** T11

- [ ] **T12b: Guest selects driven by the DB**
  - **Scope:** S
  - **Description:** Remove `DISPLAY_CURRENCIES` and `checkout.rs::CURRENCIES`. Render fns take `&[Currency]`. If the stored preference or default currency isn't in the active list, the selector preselects nothing and prompts "Choose a currency", and detail/quote fetches are made without `?currency` until one is chosen. If `active_currencies()` errors with no cache, the selector is hidden and prices stay in the listing's base currency (correctly labelled), with a `warn!`.
  - **Files Touched:** `web_ui_common/src/components/domain/currency_selector.rs`, `web_ui_guest/src/pages/checkout.rs`, `web_ui_guest/src/pages/profile.rs`
  - **Acceptance Criteria:** [ ] `mod currency_select_tests` (≥4 render tests incl. inactive preference + empty list). `rg "DISPLAY_CURRENCIES|const CURRENCIES" web_ui_*` → none.
  - **Verification Command:** `cargo test -p web_ui_common -p web_ui_guest currency_select_tests`
  - **Dependencies:** T11

- [ ] **T13: Admin currencies card**
  - **Scope:** S
  - **Description:** New `pages/currencies.rs` with POST `#[page]` actions (create, rename, toggle). The first statement of each is `admin_only`. On `CURRENCY_IN_USE`, a modal lists the listings (escaped via `view!`) with links to edit them. `invalidate_currencies()` runs after any mutation. Pure render fns.
  - **Files Touched:** `web_ui_admin/src/pages/currencies.rs` (new), `web_ui_admin/src/pages/mod.rs`
  - **Acceptance Criteria:** [ ] `mod currencies_card_tests` (≥3 render tests: admin card, host omitted, blocked modal). Host 403 enforcement is covered by T9 + T21.
  - **Verification Command:** `cargo test -p web_ui_admin currencies_card_tests`
  - **Dependencies:** T11

### ✅ Checkpoint C — full gate set

---

## Phase 4 — Rate Management (vertical slice)

- [ ] **T14: `db_core::publish_rate`**
  - **Scope:** M
  - **Description:** One tx (the order follows the lock-order rule):
    1. `lock_pair_currencies(tx, base, target, KeyShare)` (ascending code order), then the active check for both (`ensure_active` semantics).
    2. Take the pair advisory lock.
    3. `now = clock_timestamp()`, read **after** the lock.
    4. `normalize_effective_at(req, now, grace, horizon)`.
    5. Baseline = the latest explicit row with `effective_at < new`.
    6. Guard.
    7. Reinstate the pair if it's retired (audit `pair_reinstated`).
    8. Insert with `created_by = actor_id`.
    9. Audit `rate_published`.
  - **Files Touched:** `db_core/src/exchange_rate.rs`
  - **Acceptance Criteria:** [ ] `mod publish_rate_tests` (≥9):
    - past → `EffectiveAtInPast`
    - too far ahead → `EffectiveAtTooFar`
    - >5% unconfirmed → error with details
    - confirmed → saved with the audit flag
    - new pair → no guard
    - retired → reinstated with an audit row
    - duplicate → `RateVersionExists`
    - **concurrency**: B blocks on A's advisory lock (55P03 under `lock_timeout`). After A commits, B's stored `effective_at` > A's, and B's guard baseline is A's row.
  - **Verification Command:** `cargo test -p db_core publish_rate_tests`
  - **Dependencies:** T3, T5, T8

- [ ] **T15: Delete scheduled, retire, history, current, coverage**
  - **Scope:** M
  - **Description:**
    - `delete_scheduled_rate(id, actor, now)`: resolve the pair from `id`, run `lock_pair_currencies(.., KeyShare)`, take the **pair advisory lock**, then `DELETE … WHERE id=$1 AND effective_at > $now RETURNING *`. 0 rows is split into 404 vs `RateAlreadyEffective`. The audit `details` come from the `RETURNING` row.
    - `retire_pair(base, target, actor, now)`: run `lock_pair_currencies(.., base_mode = Update)` (base `FOR UPDATE`, target `FOR KEY SHARE`, ascending code order), then the **pair advisory lock**. It is blocked by live listings with `base_currency = base`, **and** by live listings with `base_currency = target` when there is no explicit non-retired `target → base` row, because those listings rely on the derived reverse. Every blocking listing is returned.
    - `pair_history(now)`, `list_current_rates(now)`.
  - **Files Touched:** `db_core/src/exchange_rate.rs`
  - **Acceptance Criteria:** [ ] `mod rate_admin_tests` (≥12):
    - delete future → ok; delete effective → `RateAlreadyEffective`
    - retire blocked by a direct listing
    - retire blocked by a reverse-dependent listing
    - retire not blocked when an explicit reverse exists
    - retire keeps history
    - history states at a fixed `now`
    - **concurrency (no 40P01 in any order)**: retire vs publish on the same pair; delete vs retire on the same pair; `retire(A→B)` vs `retire(B→A)`. Each pairing blocks (55P03 under `lock_timeout`) and then completes after the other commits.
    - `lock_pair_currencies` locks in ascending code order whichever way the pair is given (unit-level assertion on the generated order)
  - **Verification Command:** `cargo test -p db_core rate_admin_tests`
  - **Dependencies:** T14

- [ ] **T16: Rate endpoints and settings**
  - **Scope:** M
  - **Description:** Move the handlers to a new `apis/exchange_rates.rs`. The staff GET uses `require_staff` (403, no longer 401). Admin routes use `require_admin`. `/coverage` is registered before `/{id}`. `{id}` is parsed as `web::Path<Uuid>` and `{base}`/`{target}` with `CurrencyCode`. Add the settings `rate_effective_grace_seconds` (default 60; startup rejects ≤0 or **>300**).
  - **Files Touched:** `app_api/booking_api/src/apis/exchange_rates.rs` (new), `apis/ledger.rs`, `apis/routes.rs`, `apis/authz_tests.rs`, `app_api/api_core/src/settings.rs`, `app_api/booking_api/config/Default.toml`, `app_api/booking_api/src/main.rs`
  - **Acceptance Criteria:** [ ] `mod exchange_rate_api_tests` (≥12):
    - a host gets 403 on every mutation, history and coverage route, and 200 on the current table
    - a guest-only user gets 403 on the current table
    - a lowercase or invalid path → 400
    - a rate string `1e5`/`1,000` → 400
    - every §3.2 error code is reached
    - grace 0 and 301 are rejected by the settings validator
    - the env override works
  - **Verification Command:** `cargo test -p booking_api exchange_rate_api_tests`
  - **Dependencies:** T15

- [ ] **T17: Client rate methods**
  - **Scope:** XS
  - **Files Touched:** `common/src/app_client/currencies.rs`, `web_client/src/client.rs`
  - **Acceptance Criteria:** [ ] `mod currency_client_tests` (≥2): each API error `code` maps to its typed `UserCallError` variant, and the success bodies deserialise.
  - **Verification Command:** `cargo test -p web_client currency_client_tests`
  - **Dependencies:** T16

- [ ] **T18: Admin exchange-rate page: table, publish form, coverage**
  - **Scope:** M
  - **Description:** Remove `USD_TARGETS` and the "Not set" rows. **Replace** the test `lists_missing_usd_pairs_as_not_set` with `lists_db_rows_only` (a behaviour change, recorded in the commit message, not a silenced test). The publish form uses `hx-post` with `hx-disabled-elt`. Add the confirm `alert-warning` fragment and the coverage alert for admins only. Pure render fns.
  - **Files Touched:** `web_ui_admin/src/pages/exchange_rates.rs`
  - **Acceptance Criteria:** [ ] `mod exchange_rates_page_tests` (≥4: admin, host, confirm fragment, coverage).
  - **Verification Command:** `cargo test -p web_ui_admin exchange_rates_page_tests`
  - **Dependencies:** T13, T17

- [ ] **T19: Pair history page with delete and retire**
  - **Scope:** S
  - **Description:** `/admin/exchange-rates/{base}/{target}` (`admin_only`). Delete is `hx-post` → `POST /admin/exchange-rates/rates/{id}/delete` (no `hx-delete`). Retire is a POST that reuses the blocked modal from T13.
  - **Files Touched:** `web_ui_admin/src/pages/exchange_rate_history.rs` (new), `web_ui_admin/src/pages/mod.rs`
  - **Acceptance Criteria:** [ ] `mod history_page_tests` (≥3 render tests: badges, delete only on scheduled rows, blocked modal).
  - **Verification Command:** `cargo test -p web_ui_admin history_page_tests`
  - **Dependencies:** T18

- [ ] **T20: Villa card "Price unavailable in CUR"**
  - **Scope:** XS
  - **Files Touched:** `web_ui_common/src/components/domain/villa_card.rs`
  - **Acceptance Criteria:** [ ] `mod villa_card_price_tests` (≥2).
  - **Verification Command:** `cargo test -p web_ui_common villa_card_price_tests`
  - **Dependencies:** T7

### ✅ Checkpoint D — full gate set

---

## Phase 5 — E2E, Docs, Gates

- [ ] **T21: Playwright E2E**
  - **Scope:** M
  - **Description:** `test.describe.configure({ mode: 'serial' })` (per project), plus **per-project isolation** keyed by `testInfo.project.name`:
    - Each project uses its own scratch currency code (e.g. `XCD` for chromium, `TTD` for firefox) and tolerates an existing row on rerun.
    - Each project uses its own **dedicated seeded pair** (e.g. `USD→XCD` / `USD→TTD`, never `USD→JMD`, which the pricing specs use). The 10% guard baseline stays deterministic because only that project's tests touch its pair.
    - Schedule then delete. Retire blocked by a seeded listing.
    - The host is read-only, and a direct POST from a host → 403.
    - The Playwright webServer env sets `CURRENCY_CACHE_TTL_SECONDS=0`. T21 runs with `reuseExistingServer: false` (or against dev servers started with that env), so the TTL actually applies locally.
    - `db-seed.ts` cleanup must not delete users referenced by `currency_audit_log`.
    - Update `exchange-rates.spec.ts` for DB-driven rows.
  - **Files Touched:** `playwright/e2e/admin-portal/exchange-rates.spec.ts`, `playwright/e2e/admin-portal/currencies.spec.ts` (new), `playwright/e2e/setup/db-seed.ts`, `playwright/playwright.config.ts` (webServer env)
  - **Acceptance Criteria:** [ ] Passes twice in a row on the same DB, on both browser projects.
  - **Verification Command:** `/playwright-e2e` (admin-portal projects, run twice)
  - **Dependencies:** T12b, T19, T20

- [ ] **T22: OpenAPI, docs, release gates**
  - **Scope:** S
  - **Files Touched:** `openapi.yaml`, `openapi-prod.yaml`, `docs/ARCHITECTURE.md` (§4.2), `docs/ENVIRONMENT.md` (grace + cache TTL env vars)
  - **Acceptance Criteria:**
    - [ ] Full CONSTRAINTS gate set. `cargo sqlx prepare --workspace --check -- --all-targets` passes.
    - [ ] `rg "DISPLAY_CURRENCIES|const CURRENCIES|USD_TARGETS|get_exchange_rate_and_currency"` → none.
  - **Verification Command:** `/sanity-check-workflow`
  - **Dependencies:** T21

---

## Risks & Mitigations
| Risk | Mitigation |
|---|---|
| Legacy prod data violates the new constraints | T1 pre-checks `RAISE` and name the offending rows. Dry-run against a prod snapshot before deploy. |
| Migration lock contention or deadlock | One upfront `LOCK TABLE` in fixed order with `lock_timeout 5s`. A failure aborts cleanly (one tx). Deploy in a quiet window. |
| Lookup signature change misses a caller | T6 is compile-forced (a new required parameter). |
| The guest currency cache is stale for ≤ TTL | The server rejects inactive currencies with `CURRENCY_INACTIVE` and the UI shows an alert. The TTL is configurable. |
| Two clocks disagree at a rate switchover | One clock: `now` passed explicitly, and read after the lock. |

## Review Log
| Rev | Date | Result |
|---|---|---|
| 1 | 2026-11-01 | UNLOCKED: Edge 7, Security 8, DB 7, QA 6, Architect 8. 17 blockers + observations folded into rev 2. |
| 2 | 2026-11-01 | Re-audit: Edge 8, Security 9, DB 7, QA 8. 8 edits: publish lock order (currencies before pair lock), migration `LOCK TABLE` order matched to the app, `CurrencyUnknown` 422 vs `CurrencyNotFound` 404, T7 → T8 dependency, T11 config via `common::app_client` env constant, T17 named tests, per-project E2E pairs + `reuseExistingServer:false`, graph redrawn. Folded into rev 3. |
| 3 | 2026-11-01 | DB re-check: the implicit audit-log FK lock after the pair lock caused cycles (delete vs retire, reverse retires) → fixed with `lock_pair_currencies` (ascending code order, before the pair lock). Residual 40P01 race → 503 retryable. **LOCKED**: Architecture 8, Edge 8, Security 9, Data & Concurrency 8, QA 8 (avg 8.2). |
