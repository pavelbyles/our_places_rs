# Implementation Plan: Admin-Managed Currencies, Pairs & Effective-Dated Rates (#100)

**Spec Reference:** [`docs/specs/spec-100-feat-update-currency-pairs.md`](../specs/spec-100-feat-update-currency-pairs.md)
**Intent:** [`docs/intent/intent-100-feat-update-currency-pairs.md`](../intent/intent-100-feat-update-currency-pairs.md)
**Baseline Commit:** `eb05d61a`
**Branch:** `feat/100-update-currency-pairs`

> **Engineering Review Status**: NOT YET REVIEWED (run `/plan-eng-review` before `/build`)

## Ordering Rationale
Risk first. The migration (FKs across 10 columns, `CHAR → VARCHAR`) and the unified rate lookup are the two riskiest pieces and everything else depends on them. After that comes a vertical slice for **currency management** (DB → API → client → UI), then a vertical slice for **rate management**, then E2E and the release gates.

## Dependency Graph
```
T1 migration ─┬─ T2 common currency ─┬─ T8 db currency ─ T9 API currency ─ T11 client ─┬─ T12a admin selects
              │                      │                                   │             ├─ T12b guest selects
              │                      │                                   T10 enforce   └─ T13 admin currencies UI
              ├─ T3 common rates ────┤
              ├─ T4 errors ──────────┼─ T5 get_rate+RateUse ─ T6 callers ─ T7 search/detail ─ T20 villa card
              │                      │                    └─ T7b lock regression tests
              │                      └─ T14 publish ─ T15 delete/retire/history ─ T16 API rates ─ T17 client
              │                                                                    └─ T18 admin rates page ─ T19 history/retire UI
              └─────────────────────────────────────────────── T21 E2E ─ T22 docs + gates
```

---

## Phase 1 — Foundation

- [ ] **T1: Currency reference migration**
  - **Scope:** M
  - **Description:** `sqlx migrate add currency_reference_and_rate_management`. Steps from spec §3.1, in this order:
    1. Create `currencies`.
    2. Normalise all 10 columns with `upper(trim())`, and `RAISE EXCEPTION` on any code not matching `^[A-Z]{3}$`.
    3. Seed USD/JMD/EUR/GBP/CAD plus the distinct codes found.
    4. Convert the `CHAR(3)` columns to `VARCHAR(3)`.
    5. Add the RESTRICT FKs.
    6. Add `created_by`/`retired_at`/`retired_by` and the `CHECK`s to the rate table.
    7. Create `currency_audit_log` with the append-only trigger.
    8. Create `idx_listing_base_currency_live`.

    Regenerate `.sqlx/`.
  - **Files Touched:**
    - `db_core/migrations/<ts>_currency_reference_and_rate_management.sql` (new)
    - `.sqlx/*.json` (regenerated)
    - `db_core/src/currency.rs` (`#[cfg(test)]` migration tests only)
  - **Acceptance Criteria:**
    - [ ] `sqlx migrate run` succeeds on a fresh DB and on a DB seeded by `playwright/e2e/setup/db-seed.ts`.
    - [ ] Inserting `booking.currency = 'XXX'` fails with an FK violation, and so does `user_history.default_currency = 'XXX'`.
    - [ ] `DELETE FROM currencies WHERE code='USD'` fails (RESTRICT). `UPDATE`/`DELETE` on `currency_audit_log` raises.
    - [ ] `rate <= 0` and `base = target` inserts are rejected by `CHECK`.
    - [ ] Existing tests still compile with `SQLX_OFFLINE=true`, and all of them pass.
  - **Verification Command:** `sqlx migrate run && cargo sqlx prepare --workspace && cargo test -p db_core currency::migration_tests`
  - **Dependencies:** None

- [ ] **T2: `common` currency domain types**
  - **Scope:** S
  - **Description:** New `common/src/currency.rs` with the `CurrencyCode` newtype (byte-level `^[A-Z]{3}$`, `TryFrom<&str>`, serde transparent), `Currency`, `CurrenciesResponse`, `CreateCurrencyRequest`, `UpdateCurrencyRequest`, `ListingRef`, `CurrencyInUse`. Everything is `///` documented and has `ToSchema`.
  - **Files Touched:** `common/src/currency.rs` (new), `common/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] `USD` is accepted. `usd`, `US`, `USDD`, `U$D` and empty are rejected.
    - [ ] Serde round-trip rejects invalid codes at deserialisation.
  - **Verification Command:** `cargo test -p common currency`
  - **Dependencies:** None (can run in parallel with T1)

- [ ] **T3: `common` rate rules & DTOs**
  - **Scope:** S
  - **Description:** Extend `common/src/exchange_rate.rs`:
    - Pure functions: `validate_rate` (>0, ≤6 decimal places, ≤ `999_999_999.999999`, reject rather than round), `deviation_pct`, `RATE_GUARD_THRESHOLD_PCT = dec!(5)`, `requires_confirmation`, `normalize_effective_at(requested, now, grace)` (returns `EffectiveAtError::InPast`).
    - DTOs: `PublishRateRequest` (rate is a string on the wire), `RateVersion`, `RateState`, `PairHistoryResponse`, `RateCoverageGap`.
  - **Files Touched:** `common/src/exchange_rate.rs`
  - **Acceptance Criteria:**
    - [ ] Exactly 5% → no confirmation. 5.000001% → confirmation. Rises and drops are treated symmetrically.
    - [ ] Grace: `None` → now. now−59s → now. now−61s → `InPast`. now+1h unchanged.
    - [ ] `0`, negative, 7 decimal places and over max → `Err`. No `f32`/`f64` (`rg` gate).
  - **Verification Command:** `cargo test -p common exchange_rate`
  - **Dependencies:** None

- [ ] **T4: Error variants & HTTP mapping**
  - **Scope:** S
  - **Description:** Add these `DbError` variants: `CurrencyInactive`, `CurrencyInUse(Vec<ListingRef>)`, `EffectiveAtInPast`, `RateAlreadyEffective`, `RateVersionExists`, `RateChangeConfirmationRequired{baseline,proposed,deviation_pct}`, `CurrencyExists`. Map each to its status and `{code, message, ...}` body in `ApiError`, and log a `warn!` on 4xx business rejections. The unique-violation → `RateVersionExists`/`CurrencyExists` mapping lives in db_core.
  - **Files Touched:** `db_core/src/error.rs`, `app_api/api_core/src/error.rs`
  - **Acceptance Criteria:**
    - [ ] One unit test per variant asserts the status code and JSON `code` (`CURRENCY_INACTIVE` 422, `CURRENCY_IN_USE` 409 with `listings`, `EFFECTIVE_AT_IN_PAST` 422, `RATE_ALREADY_EFFECTIVE` 409, `RATE_VERSION_EXISTS` 409, `RATE_CHANGE_CONFIRMATION_REQUIRED` 422 with string decimals, `CURRENCY_EXISTS` 409).
  - **Verification Command:** `cargo test -p api_core error`
  - **Dependencies:** T2, T3

### ✅ Checkpoint A
`SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features -- -D warnings` and backend tests green.

---

## Phase 2 — Unified Rate Resolution (high risk)

- [ ] **T5: `get_rate` with `RateUse`, explicit-first single query**
  - **Scope:** S
  - **Description:** Add a `RateUse { NewQuote, Settlement }` parameter to `get_rate`/`get_rate_pool`. Replace the two sequential queries with one `UNION ALL ... ORDER BY derived, effective_at DESC, updated_at DESC LIMIT 1`. `NewQuote` filters `retired_at IS NULL` and joins `currencies` to require both active (`CurrencyInactive` otherwise). `Settlement` ignores both filters. Keep `#[tracing::instrument]`.
  - **Files Touched:** `db_core/src/exchange_rate.rs`
  - **Acceptance Criteria:**
    - [ ] Explicit beats derived. Derived when only the reverse exists. A scheduled-only explicit pair falls through to derived. A future row is ignored until `at`. A single row stays effective indefinitely. Missing → `FxRateUnavailable`.
    - [ ] A retired pair is excluded for `NewQuote` and used for `Settlement`. An inactive currency → `CurrencyInactive` for `NewQuote` only.
  - **Verification Command:** `cargo test -p db_core exchange_rate`
  - **Dependencies:** T1, T4

- [ ] **T6: Move existing rate callers to explicit `RateUse`**
  - **Scope:** S (mechanical, 5 files)
  - **Description:** Hold creation uses `NewQuote`. Booking display rate, payout at confirmation, reservation-change payout and tax-document JMD use `Settlement`. Behaviour stays the same apart from the new filters.
  - **Files Touched:** `app_api/booking_api/src/apis/bookings.rs`, `app_api/booking_api/src/apis/mod.rs`, `db_core/src/booking/update.rs`, `db_core/src/reservation_change.rs`, `db_core/src/tax_document.rs`
  - **Acceptance Criteria:**
    - [ ] Existing booking, reservation-change and tax-document test suites pass unchanged.
  - **Verification Command:** `cargo test -p db_core -p booking_api`
  - **Dependencies:** T5

- [ ] **T7: Search/detail on the unified lookup, and fallbacks removed**
  - **Scope:** M
  - **Description:**
    - Add `get_rates_for_bases(pool, &[String], target, at, NewQuote)` (one `= ANY($1)` query; missing bases are absent from the map).
    - Search sets `price_per_night`/`extra_guest_fee = None` plus `price_unavailable_in = Some(target)` for missing bases, and logs `warn!` once per missing pair.
    - Detail uses `get_rate_pool(.., NewQuote)` and returns 422 when the rate is missing.
    - Add `ListingResponse.price_unavailable_in` (`skip_serializing_if`).
    - **Delete** `currency::get_exchange_rate_and_currency` and `get_exchange_rates_cache`.
  - **Files Touched:** `db_core/src/exchange_rate.rs`, `db_core/src/currency.rs`, `app_api/listing_api/src/apis.rs`, `common/src/models.rs`, `app_api/listing_api/src/apis_test.rs`
  - **Acceptance Criteria:**
    - [ ] Search with one unresolvable listing → 200. That listing has null prices and `price_unavailable_in`. The others are converted. No USD relabel and no rate `1`.
    - [ ] Detail for the same pair → 422 `FX_RATE_UNAVAILABLE`.
    - [ ] A future-dated rate doesn't change search or detail prices.
    - [ ] Detail `?currency=JMD` nightly price == checkout `base_nightly_rate` at the same instant.
    - [ ] `rg "get_exchange_rate_and_currency|get_exchange_rates_cache"` → no matches.
  - **Verification Command:** `cargo test -p listing_api && cargo test -p db_core exchange_rate`
  - **Dependencies:** T6

- [ ] **T7b: Rate-lock regression tests**
  - **Scope:** S
  - **Description:** Integration tests only. Hold at R1, then publish R2 effective now via direct insert, then confirm: `booking.exchange_rate == R1`. The payout ledger uses the confirmation-time host rate. The tax document uses the issue-date JMD rate. A booking in a since-deactivated currency or retired pair still confirms and issues documents.
  - **Files Touched:** `app_api/booking_api/src/apis/tests.rs`
  - **Acceptance Criteria:** [ ] All four scenarios assert the stored values.
  - **Verification Command:** `cargo test -p booking_api rate_lock`
  - **Dependencies:** T6

### ✅ Checkpoint B
Full backend test suite green. `rg "f(32|64)" common/src/exchange_rate.rs db_core/src/exchange_rate.rs` returns nothing.

---

## Phase 3 — Currency Management (vertical slice)

- [ ] **T8: `db_core` currency CRUD, `ensure_active`, in-use check**
  - **Scope:** M
  - **Description:** Rewrite `currency.rs` with:
    - `list_currencies(active_only)`, `create_currency`, `update_currency`.
    - Deactivation runs in a tx: `SELECT … FOR UPDATE` on the currency row, then live listings with that base (`deleted_at IS NULL`), then `CurrencyInUse`.
    - `ensure_active(conn, code)` (`FOR KEY SHARE` + active).
    - `rate_coverage_gaps()`.
    - Every mutation writes to `currency_audit_log` in the same tx.
  - **Files Touched:** `db_core/src/currency.rs`, `db_core/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] Create a duplicate → `CurrencyExists`. Deactivate with a live listing → `CurrencyInUse` with that listing. A soft-deleted listing doesn't block. Reactivate always works.
    - [ ] Concurrency test: a listing insert holding `FOR KEY SHARE` blocks deactivation, and after commit the deactivation returns `CurrencyInUse`.
    - [ ] One audit row per mutation, with `actor_id`.
  - **Verification Command:** `cargo test -p db_core currency`
  - **Dependencies:** T1, T2, T4

- [ ] **T9: Currency endpoints in `booking_api`**
  - **Scope:** M
  - **Description:** New `apis/currencies.rs`:
    - public `GET /api/v1/currencies` (active only)
    - admin `GET|POST /api/v1/admin/currencies`
    - admin `PATCH /api/v1/admin/currencies/{code}`

    Role checks use DB roles from `claims.sub` (`Admin` only → else 403). Add `#[utoipa::path]` and `#[instrument]`, and register them in `routes.rs`.
  - **Files Touched:** `app_api/booking_api/src/apis/currencies.rs` (new), `app_api/booking_api/src/apis/mod.rs`, `app_api/booking_api/src/apis/routes.rs`, `app_api/booking_api/src/apis/authz_tests.rs`, `app_api/booking_api/src/apis/tests.rs`
  - **Acceptance Criteria:**
    - [ ] Public list has no auth and excludes inactive currencies. Admin list includes inactive ones.
    - [ ] Host → 403 on every admin currency route. Anonymous → 401.
    - [ ] Invalid code → 400. Duplicate → 409. Deactivate in use → 409 with listings.
  - **Verification Command:** `cargo test -p booking_api currencies`
  - **Dependencies:** T8

- [ ] **T10: Enforce active currency on writes**
  - **Scope:** M
  - **Description:** Call `ensure_active` inside the write tx whenever the currency is new or changed: listing create/update (`base_currency`), user registration and profile (`default_currency`), and checkout hold creation. Leave an unchanged inactive value alone.
  - **Files Touched:** `app_api/listing_api/src/apis.rs`, `db_core/src/listing/write.rs`, `app_api/user_api/src/apis/registration.rs`, `app_api/user_api/src/apis/profile.rs`, `app_api/booking_api/src/apis/bookings.rs`
  - **Acceptance Criteria:**
    - [ ] An inactive currency on listing create, user default change or hold → 422 `CURRENCY_INACTIVE`.
    - [ ] Saving a profile or listing that keeps an existing inactive currency unchanged → 200.
    - [ ] An unknown code → 422 (no FK 500 leaks).
  - **Verification Command:** `cargo test -p listing_api -p user_api -p booking_api currency_inactive`
  - **Dependencies:** T8

- [ ] **T11: Client methods and active-currency cache**
  - **Scope:** S
  - **Description:** Add `common/src/app_client/currencies.rs` with the typed calls. In `web_client`, add `active_currencies()` with a 60s TTL in-process cache (`RwLock<Option<(Instant, Arc<Vec<Currency>>)>>`, no `unwrap` on lock poisoning: map it to an error), plus `invalidate_currencies()` and admin methods.
  - **Files Touched:** `common/src/app_client/currencies.rs` (new), `common/src/app_client/mod.rs`, `web_client/src/client.rs`
  - **Acceptance Criteria:**
    - [ ] Within TTL → no second HTTP call. After TTL or invalidate → refetch (test with an injectable clock).
  - **Verification Command:** `cargo test -p web_client currencies`
  - **Dependencies:** T9

- [ ] **T12a: Admin selects driven by the DB**
  - **Scope:** S
  - **Description:** Remove `listings/form.rs::CURRENCIES` and the user form currency list. Options come from `active_currencies()`. A row's own inactive value renders as `CODE (inactive)` and stays selected.
  - **Files Touched:** `web_ui_admin/src/pages/listings/form.rs`, `web_ui_admin/src/pages/users/form.rs`, `web_ui_admin/src/pages/users/mod.rs`
  - **Acceptance Criteria:** [ ] Render tests: options match the API list, and the inactive current value is shown and selected.
  - **Verification Command:** `cargo test -p web_ui_admin`
  - **Dependencies:** T11

- [ ] **T12b: Guest selects driven by the DB**
  - **Scope:** S
  - **Description:** Remove `DISPLAY_CURRENCIES` and `checkout.rs::CURRENCIES`. The selector, profile and checkout use `active_currencies()`. Legacy `/checkout-xxx/{id}` routes stay. A `CURRENCY_INACTIVE` or `FX_RATE_UNAVAILABLE` response renders the existing alert.
  - **Files Touched:** `web_ui_common/src/components/domain/currency_selector.rs`, `web_ui_guest/src/pages/checkout.rs`, `web_ui_guest/src/pages/profile.rs`
  - **Acceptance Criteria:** [ ] Render tests pass. `rg "DISPLAY_CURRENCIES|const CURRENCIES|USD_TARGETS" web_ui_*` → no matches (the `USD_TARGETS` part completes after T18).
  - **Verification Command:** `cargo test -p web_ui_common -p web_ui_guest`
  - **Dependencies:** T11

- [ ] **T13: Admin currencies card**
  - **Scope:** S
  - **Description:** On `/admin/exchange-rates`, add a currencies card for admins only: a list that includes inactive currencies, an add form (code, name), rename, and an activate/deactivate toggle. On `CURRENCY_IN_USE`, a modal lists the listings with links to `/admin/listings/{id}/edit`. Actions are gated by `admin_only`, with HTMX swaps and `invalidate_currencies()` after a mutation.
  - **Files Touched:** `web_ui_admin/src/pages/currencies.rs` (new), `web_ui_admin/src/pages/mod.rs`
  - **Acceptance Criteria:** [ ] Hosts never see the card, and a host POST to the action → 403. Blocked modal render test.
  - **Verification Command:** `cargo test -p web_ui_admin currencies`
  - **Dependencies:** T11

### ✅ Checkpoint C
Admin can add/deactivate currencies end to end, and every selector is DB-driven. All web UI tests and backend tests green.

---

## Phase 4 — Rate Management (vertical slice)

- [ ] **T14: `db_core::publish_rate`**
  - **Scope:** M
  - **Description:** One tx:
    1. `pg_advisory_xact_lock(hashtext('fx:'||$1||':'||$2))`.
    2. `ensure_active` for both currencies.
    3. `normalize_effective_at` (grace passed in).
    4. Baseline = the latest explicit row with `effective_at < new`.
    5. Guard → `RateChangeConfirmationRequired` unless confirmed.
    6. If the pair is retired, clear `retired_*` and write a `pair_reinstated` audit row.
    7. Insert with `created_by`.
    8. Write a `rate_published` audit row (baseline, deviation, `guard_overridden`).
  - **Files Touched:** `db_core/src/exchange_rate.rs`
  - **Acceptance Criteria:**
    - [ ] Past → `EffectiveAtInPast`. >5% unconfirmed → error with details. Confirmed → saved with the audit flag. New pair → no guard. Retired pair → reinstated.
    - [ ] Two concurrent publishes are serialised (test with two connections). Duplicate `effective_at` → `RateVersionExists`.
  - **Verification Command:** `cargo test -p db_core publish_rate`
  - **Dependencies:** T3, T5, T8

- [ ] **T15: Delete scheduled, retire, history, current, coverage**
  - **Scope:** M
  - **Description:**
    - `delete_scheduled_rate` (`DELETE … WHERE id=$1 AND effective_at > now() RETURNING`, 0 rows → distinguish 404 from `RateAlreadyEffective`).
    - `retire_pair` (`FOR UPDATE` on the base currency row → live listings with that base → `CurrencyInUse`).
    - `pair_history` (with `RateState`).
    - `list_current_rates` (explicit, non-retired, effective now).
    - Each mutation is audited.
  - **Files Touched:** `db_core/src/exchange_rate.rs`
  - **Acceptance Criteria:**
    - [ ] Delete future → ok. Delete effective → `RateAlreadyEffective`.
    - [ ] Retire with a live listing → `CurrencyInUse`. Retire otherwise sets `retired_at` on all rows and keeps history.
    - [ ] History states are correct across past, current and scheduled.
  - **Verification Command:** `cargo test -p db_core exchange_rate`
  - **Dependencies:** T14

- [ ] **T16: Rate endpoints and grace setting**
  - **Scope:** M
  - **Description:**
    - Move the exchange-rate handlers from `ledger.rs` into a new `apis/exchange_rates.rs`.
    - Add these admin-only routes (spec §3.2): `POST /admin/exchange-rates`, `DELETE /{id}`, `POST /{base}/{target}/retire`, `GET /{base}/{target}/history`, `GET /coverage`. The current-table GET stays staff-only.
    - Add `application.rate_effective_grace_seconds` (default 60) and reject a value ≤ 0 at startup.
  - **Files Touched:** `app_api/booking_api/src/apis/exchange_rates.rs` (new), `app_api/booking_api/src/apis/ledger.rs`, `app_api/booking_api/src/apis/routes.rs`, `app_api/api_core/src/settings.rs`, `app_api/booking_api/config/Default.toml`, `app_api/booking_api/src/main.rs`
  - **Acceptance Criteria:**
    - [ ] Host → 403 on every mutation, history and coverage route. Host → 200 on the current table.
    - [ ] Every spec §3.2 error code is reachable and asserted.
    - [ ] `EA__APPLICATION__RATE_EFFECTIVE_GRACE_SECONDS` override works.
  - **Verification Command:** `cargo test -p booking_api exchange_rates`
  - **Dependencies:** T15

- [ ] **T17: Client rate methods**
  - **Scope:** XS
  - **Description:** Typed `app_client`/`web_client` methods for publish, delete, retire, history and coverage.
  - **Files Touched:** `common/src/app_client/currencies.rs`, `web_client/src/client.rs`
  - **Acceptance Criteria:** [ ] Compiles. Error codes surface as typed `UserCallError`.
  - **Verification Command:** `cargo test -p web_client`
  - **Dependencies:** T16

- [ ] **T18: Admin exchange-rate page: table, publish form, coverage**
  - **Scope:** M
  - **Description:**
    - Remove `USD_TARGETS` and the "Not set" rows.
    - Admins get the publish form (active currency selects, rate text input, optional UTC `datetime-local`) and the coverage gaps alert.
    - On `RATE_CHANGE_CONFIRMATION_REQUIRED`, swap in an `alert-warning` that shows baseline → proposed (±%) with a Confirm resubmit.
    - Hosts see the table only.
  - **Files Touched:** `web_ui_admin/src/pages/exchange_rates.rs`
  - **Acceptance Criteria:** [ ] Render tests cover the admin and host variants and the confirm-alert fragment.
  - **Verification Command:** `cargo test -p web_ui_admin exchange_rates`
  - **Dependencies:** T13, T17

- [ ] **T19: Pair history page with delete and retire**
  - **Scope:** S
  - **Description:** `/admin/exchange-rates/{base}/{target}` (admin_only) shows versions with Past/Current/Scheduled badges. Scheduled rows have a Delete action (`hx-delete` + confirm). The pair has a Retire action that reuses the `CURRENCY_IN_USE` modal from T13.
  - **Files Touched:** `web_ui_admin/src/pages/exchange_rate_history.rs` (new), `web_ui_admin/src/pages/mod.rs`
  - **Acceptance Criteria:** [ ] Host GET → 403. Delete is shown only on scheduled rows.
  - **Verification Command:** `cargo test -p web_ui_admin exchange_rate_history`
  - **Dependencies:** T18

- [ ] **T20: Villa card "Price unavailable in CUR"**
  - **Scope:** XS
  - **Description:** When `price_unavailable_in` is set, `villa_card` renders muted "Price unavailable in <CUR>" with no amount.
  - **Files Touched:** `web_ui_common/src/components/domain/villa_card.rs`
  - **Acceptance Criteria:** [ ] Render test for `Some("CAD")`. The normal price path is unchanged.
  - **Verification Command:** `cargo test -p web_ui_common villa_card`
  - **Dependencies:** T7

### ✅ Checkpoint D
Full workspace test suite + web UI tests green.

---

## Phase 5 — E2E, Docs, Gates

- [ ] **T21: Playwright E2E**
  - **Scope:** M
  - **Description:**
    - Update `exchange-rates.spec.ts` (DB-driven rows, no "Not set").
    - Add `currencies.spec.ts`: add TTD → `USD→TTD` → guest selector shows it; a 10% change shows the confirm alert; schedule then delete; retire blocked with listing shown; host is read-only and a direct POST → 403.
    - Seed data in `db-seed.ts`. The test env sets a short currency cache TTL or invalidates it.
  - **Files Touched:** `playwright/e2e/admin-portal/exchange-rates.spec.ts`, `playwright/e2e/admin-portal/currencies.spec.ts` (new), `playwright/e2e/setup/db-seed.ts`
  - **Acceptance Criteria:** [ ] All scenarios pass locally.
  - **Verification Command:** `/playwright-e2e` (admin-portal project)
  - **Dependencies:** T12b, T19, T20

- [ ] **T22: OpenAPI, docs, release gates**
  - **Scope:** S
  - **Description:**
    - Regenerate `openapi.yaml` / `openapi-prod.yaml`.
    - Update `docs/ARCHITECTURE.md` §4.2 (currencies table, `RateUse`, explicit-first resolution, rate-lock table) and `docs/ENVIRONMENT.md` (grace env var).
    - Run every CONSTRAINTS.md gate.
  - **Files Touched:** `openapi.yaml`, `openapi-prod.yaml`, `docs/ARCHITECTURE.md`, `docs/ENVIRONMENT.md`
  - **Acceptance Criteria:**
    - [ ] `cargo fmt --check`, clippy `-D warnings`, `-D missing_docs`, unwrap/expect `rg`, float `rg`, env-var `rg`, `cargo sqlx prepare --workspace --check`, backend and web UI tests all pass.
    - [ ] `rg "DISPLAY_CURRENCIES|const CURRENCIES|USD_TARGETS|get_exchange_rate_and_currency"` → no matches.
  - **Verification Command:** `/sanity-check-workflow`
  - **Dependencies:** T21

---

## Risks & Mitigations
| Risk | Mitigation |
|---|---|
| Migration fails on unexpected legacy codes in prod | Normalise first, and `RAISE EXCEPTION` names the table, column and value. Dry-run against a prod snapshot before deploy. |
| `CHAR → VARCHAR` changes the SQLx offline types | Both map to `String`. Regenerate `.sqlx/` in T1, and Checkpoint A catches drift. |
| Lookup signature change misses a caller | T6 is a compile-forced change (new required param). |
| The guest currency cache shows a just-deactivated currency for ≤60s | The server rejects it with `CURRENCY_INACTIVE` and the UI shows the alert. Accepted per spec. |
| `ACCESS EXCLUSIVE` locks during the migration | Small tables. Deploy in a quiet window. |
