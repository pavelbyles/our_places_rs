# Implementation Plan: Promo Codes (#106)

**Spec Reference:** [`docs/specs/spec-106-feat-promo-codes.md`](../specs/spec-106-feat-promo-codes.md)
**Intent:** [`docs/intent/intent-106-feat-promo-codes.md`](../intent/intent-106-feat-promo-codes.md)
**Baseline Commit:** `e2f6e2d0`
**Branch:** `feat/106-promo-codes`

> **Engineering Review Status**: LOCKED (rev 6)
> **Review Date**: 2026-11-02
> **Review Panel**: Lead Architect, Edge Case Analyst, Security Reviewer, DB Guardian, QA Strategist

## Cross-Cutting Rules (apply to every task)
- **Local database**: Postgres runs in the devenv shell, not Docker. Run `sqlx` and `psql` commands through `devenv shell -- <command>`. Database tests need it running.
- **SQLx metadata**: after changing a query, run `devenv shell -- cargo sqlx prepare --workspace -- --all-targets` and commit the root `.sqlx/` changes with that task.
- **Task gates**: `cargo fmt --check`, `SQLX_OFFLINE=true cargo clippy -p <touched crates> --all-targets --all-features -- -D warnings` and `SQLX_OFFLINE=true cargo check --workspace --exclude protoproj --all-features --tests`, so a change in one crate can't leave another uncompiled. Every new public item has a `///` comment.
- **Tests come first**: write the failing test, then the code. New tests live in named modules, such as `mod promo_tests`. A filter that matches no tests reports "0 passed", and a filter that matches only older tests proves nothing, so neither counts as green. Each task creates the named module its command filters on.
- **Concurrency tests** use real connections, `sqlx_db_tester::TestPg` and `#[tokio::test(flavor = "multi_thread")]`, with no sleeps.
- **E2E runs**: first restart the APIs and portals from this worktree (`apis-stop`, `frontends-stop`, then `apis-start`), and run Playwright through `devenv shell --`. Playwright reuses any healthy server on its ports, which could be from another checkout or an old build.
- **Logs**: promo code strings and `PromoTerms` never enter a log or span field. Any instrumented function taking a code, or a struct containing one, must skip it. Use `promo_code_id` or the SHA-256 hash.
- **Error mapping**: every new `DbError` variant has explicit arms in both `status_code` and `error_response`.
- **Money**: `Decimal` only, `round_dp(2)` only, and no fallback amounts or rates.
- **Lock order**: holds take listing → currency → the guest's other pending bookings → promo code; confirmation takes listing → booking → promo code. Never take an earlier lock after a later one. The one exception is code creation, which takes currency and listing `FOR KEY SHARE` through foreign keys, which is safe because `KEY SHARE` doesn't wait behind a queued deactivation.
- **Usage counts** are a separate statement after the promo lock, never a single CTE with it. Under READ COMMITTED, a later statement gets a fresh snapshot, and a CTE would reuse the snapshot from before the lock wait.
- **Promo-free behaviour**: outside the changes the spec calls for, bookings without a code must price as they do today.
- **Existing tests**: update them only for the changes the spec calls for, without weakening an assertion.

## Dependency Graph
Each line is `task ← dependencies`, matching every task's **Dependencies** field.

```text
Phase A: payout fixes (no promo dependency, highest financial risk)
T1  stale discount            ← —
T2  retained payouts          ← —
T3  host gross + cleaning     ← T2

Phase B: promo foundations
T4  migration                 ← —
T5  common::promo             ← —
T6  calculator type-state     ← T5
T7  Booking promo columns     ← T3, T4, T5
T8  promo errors              ← T5

Phase C: redemption and pricing
T9  usage lock and count      ← T4, T7, T8
T10 redemption in hold        ← T9
T11 price_stay + hold code    ← T6, T10
T12 quote endpoints           ← T11

Phase D: reservation changes
T13 promo re-evaluation       ← T6, T7
T14 direct update path        ← T1, T10, T13
T15 change quote + approval   ← T3, T10, T13

Phase E: records and communications
T16 tax documents             ← T7, T12
T17 ledger columns + CSV      ← T2, T3, T7
T18 booking response          ← T7
T19 guest emails              ← T7, T11
T20 host emails               ← T17, T19
T20a cross-surface agreement  ← T11, T16, T17, T19, T20

Phase F: management and portals
T21 promo management db       ← T8, T9
T22 promo management API      ← T21
T23 client methods            ← T12, T22
T24 one stay total            ← T23
T25 checkout promo entry      ← T24
T26 change warnings           ← T15, T23, T25
T27 admin promo page          ← T23, T25
T28 ledger + booking detail   ← T17, T18, T25

Phase G: finish
T29 docs + API contracts      ← T12, T22
T30 final gates               ← all
```

## Spec Coverage
| Spec area | Tasks |
|---|---|
| §1.1, §1.2 code rules and authorization | T5, T21, T22, T27 |
| §1.3 pricing order | T6, T11, T29 |
| §1.4 validation | T5, T9, T11, T12 |
| §1.5 usage | T9, T10 |
| §1.6 reservation changes | T13, T14, T15, T26 |
| §1.7 host payout | T3, T7, T15 |
| §1.7.1 retained payouts | T2, T17 |
| §1.7.2 one stay total | T12, T24 |
| §1.7.3 maximum stay | T11, T12, T24 |
| §1.8 display | T16–T20, T20a, T24–T28 |
| Stale discount | T1, T14 |

---

## Phase A: Payout Fixes

- [x] **Task 1: Clear stale discounts on direct updates**
  - **Scope:** S
  - **Description:** Add `UpdatedBooking.replace_discounts`, set it in `reprice`, and write `discount_value = CASE WHEN $21 THEN $9 ELSE COALESCE($9, discount_value) END`.
  - **Files Touched:**
    - `db_core/src/models.rs`
    - `db_core/src/booking/update.rs`
    - `app_api/booking_api/src/apis/booking_updates.rs`
    - `db_core/src/booking/tests.rs`
    - `app_api/booking_api/src/apis/cron.rs`
    - `app_api/booking_api/src/apis/tests.rs`
  - **Acceptance Criteria:**
    - [ ] A failing test first reproduces the stale discount on a stay shortened below 7 nights.
    - [ ] Shortening below the threshold clears `discount_value`, and `accommodation_net()` matches the new total.
    - [ ] A status-only update keeps the discount.
    - [ ] A `PATCH` that shortens a stay through `reprice` clears the discount, so the flag is proven to be set.
  - **Verification Command:** `cargo test -p db_core stale_discount && cargo test -p booking_api stale_discount`
  - **Dependencies:** None

- [x] **Task 2: Write retained payouts on partial refunds**
  - **Scope:** S
  - **Description:** Add `reopen_payout_as_retained`, and have `settle_cancellation` use the ledger ID from `cancel_pending_payout_for_booking`. `create_payout_ledger_entry` keeps `DO NOTHING`. Read `exchange_rate`, `platform_fee_pct` and `currency` from the ledger row under the booking lock, and calculate the retained gross from those, not from `host_payout_terms`. A host changing `default_currency` would otherwise put a converted amount under the old currency label. When no row exists, insert one through `create_payout_ledger_entry`, which now returns rows affected, so bookings confirmed before the ledger existed still get paid.
  - **Files Touched:**
    - `db_core/src/payout_ledger.rs`
    - `db_core/src/booking/update.rs`
    - `db_core/src/booking/tests.rs`
    - `db_core/Cargo.toml (`tracing-test` dev-dependency)`
  - **Acceptance Criteria:**
    - [ ] A failing test first shows that no retained row exists today, confirming the bug.
    - [ ] A partial refund leaves one `pending` row with the retained gross, commission and net.
    - [ ] A full refund leaves the row `cancelled`.
    - [ ] Repeating confirmation neither duplicates nor overwrites a row.
    - [ ] A `processing`, `paid` or `refunded` row is unchanged, with a warning asserted through captured logs.
    - [ ] A partial refund with no ledger row inserts one retained `pending` row.
    - [ ] A host currency change between confirmation and cancellation leaves the row's currency and rate consistent with its amounts.
    - [ ] A retired FX pair doesn't block the cancellation.
  - **Verification Command:** `cargo test -p db_core retained_payout`
  - **Dependencies:** None

- [x] **Task 3: Include the cleaning fee in host gross**
  - **Scope:** M
  - **Description:** Add `common::pricing::host_gross` and use it in every gross calculation. `approve_change_request` recalculates from the updated booking, using the ledger row's `exchange_rate` and `platform_fee_pct`, instead of writing the snapshot's amounts. Uses `booking.cleaning_fee` until T7 adds `cleaning_net()`. `revise_confirmed_terms` also uses the row's `exchange_rate`, `platform_fee_pct` and `currency` instead of `host_payout_terms`, fixing the same currency bug as T2, and the change quote uses the row's percentage when a row exists, so the quote and approval agree.
  - **Files Touched:**
    - `common/src/pricing.rs`
    - `db_core/src/booking/update.rs`
    - `db_core/src/reservation_change.rs`
    - `app_api/booking_api/src/apis/reservation_changes.rs`
    - `db_core/src/booking/hold_expiry_tests.rs`
    - `app_api/booking_api/src/apis/tests.rs`
  - **Acceptance Criteria:**
    - [ ] Gross includes the cleaning fee at confirmation, on revision, in the change quote (previous and proposed) and on approval.
    - [ ] Commission applies to the whole gross.
    - [ ] Revising an older row uses the new formula.
    - [ ] Approving a request whose snapshot holds an old-formula amount writes the new-formula amount.
    - [ ] A retained payout stays accommodation only.
    - [ ] A host currency change before a revision leaves the row consistent.
    - [ ] With a changed listing commission, the quote's proposed net equals what approval writes.
    - [ ] With no ledger row, a revision writes nothing and doesn't fail.
  - **Verification Command:** `cargo test -p common host_gross && cargo test -p db_core host_gross && cargo test -p booking_api reservation_change_payout && ! rg '(accommodation_net\(\)|discounted_subtotal) \* .*exchange_rate' db_core app_api && npm run test:e2e:admin -- payouts-admin reservation-changes-admin`
  - **Dependencies:** T2

---

## Phase B: Promo Foundations

- [x] **Task 4: Add the promo migration**
  - **Scope:** S
  - **Description:** Run `devenv shell -- sqlx migrate add --source db_core/migrations promo_codes`, then write spec §3.1: tables, enums, booking, history, ledger and tax document columns, and the rescheduled `cleanup_stale_pending_holds` job. Take the table locks first, following `20261101231550`: `SET LOCAL lock_timeout = '5s'` then `LOCK TABLE ... IN ACCESS EXCLUSIVE MODE`. Otherwise the foreign key creation followed by `ALTER` can deadlock with a running `update_booking`.
  - **Files Touched:**
    - `db_core/migrations/<timestamp>_promo_codes.sql`
    - `db_core/src/lib.rs`
    - `db_core/src/promo_tests.rs`
  - **Acceptance Criteria:**
    - [ ] The migration applies to a fresh database and the existing database.
    - [ ] `CHECK` constraints reject a percentage above 100, a fixed discount with no currency, and a lowercase code.
    - [ ] `promo_code.currency` has its foreign key to `currencies`.
    - [ ] No `UPDATE` on `host_payout_ledger`. The only `UPDATE booking` is inside the `cleanup_stale_pending_holds` job body.
    - [ ] A `promo_schema_rows` test using the `previous_migrations_dir` pattern (`db_core/src/currency.rs`) seeds ledger and booking rows, applies the migration, and asserts they're unchanged and the new columns default to 0.
    - [ ] A `promo_schema_cron_job` test: the job's SQL, extracted from the migration, runs in `TestPg` against an expired promo hold, and `booking_history` receives all four promo columns. Test databases skip the `DO` block and `cron.schedule` doesn't validate SQL, so this is the only check that the job body works.
    - [ ] A `promo_schema_checks` test covers the `CHECK` constraints, including rejecting a leading hyphen.
  - **Verification Command:** `devenv shell -- sqlx migrate run --source db_core/migrations && ! rg -qi 'UPDATE\s+host_payout_ledger\b' db_core/migrations/*_promo_codes.sql && test "$(rg -ci 'UPDATE\s+booking\b' db_core/migrations/*_promo_codes.sql)" = 1 && cargo test -p db_core promo_tests::promo_schema`
  - **Dependencies:** None

- [x] **Task 5: Add `common::promo` rules**
  - **Scope:** S
  - **Description:** Add `PromoTerms`, `PromoRefusal`, `normalize_code(&str) -> Result<Option<String>, PromoRefusal>`, `check_status` (checks 2–4), `check_stay_rules` (7–9) and `check_usage` (10–11). Blank is `Ok(None)` and malformed is `Err(NotFound)`, so a malformed code can't turn into a full-price hold.
  - **Files Touched:**
    - `common/src/promo.rs`
    - `common/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] Every §1.4 check has a test, including boundaries: the `valid_to` instant, the last eligible night and the exact minimum value.
    - [ ] `PromoTerms` round-trips through JSON with no loss of precision.
    - [ ] Within each function, the first failing check in §1.4 order is returned. The order across functions is tested in T12.
    - [ ] `" Summer "` normalizes to `SUMMER`; `SUM_MER`, `abc`, `-ABC` and a 33-character code are `Err(NotFound)`; blank is `Ok(None)`.
  - **Verification Command:** `cargo test -p common promo::`
  - **Dependencies:** None

- [x] **Task 6: Add the calculator's promo step**
  - **Scope:** M
  - **Description:** Add the `PromoApplied` type-state and `apply_promo(Option<&PromoTerms>)`, and move `apply_taxes` to it. Add `promo_stay_discount`, `promo_cleaning_discount` and `cleaning_net`. All call sites pass `None` for now. New tests live in `mod promo_tests`.
  - **Files Touched:**
    - `common/src/pricing.rs`
    - `app_api/booking_api/src/apis/bookings.rs`
    - `app_api/booking_api/src/apis/booking_updates.rs`
    - `app_api/booking_api/src/apis/reservation_changes.rs`
    - `app_api/booking_api/src/apis/tests.rs`
  - **Acceptance Criteria:**
    - [ ] The spec §1.3 worked example matches exactly, down to a 1,275.50 total.
    - [ ] Tests cover percentage, fixed, cap and 100% discounts, for each scope.
    - [ ] A fixed discount goes to the stay first, then the cleaning fee.
    - [ ] The deposit never changes, and the platform fee follows `A'`.
    - [ ] `apply_taxes` can't be called without passing through `apply_promo`.
    - [ ] Existing no-promo pricing tests pass with only the added `.apply_promo(None)`.
    - [ ] Scope `stay_and_cleaning` with no cleaning fee gives `P_C = 0` with no error.
  - **Verification Command:** `cargo test -p common pricing::promo_tests && cargo test -p booking_api --lib apis::tests`
  - **Dependencies:** T5

- [x] **Task 7: Add promo columns to `Booking`**
  - **Scope:** M (mechanical; `query_as!` requires every query to change together, so this can't be split further)
  - **Description:** Add the four columns to `Booking` and to every query that returns it. Update `record_booking_history`. `accommodation_net()` subtracts `promo_stay_discount`, and `cleaning_net()` is new. Switch `host_gross` callers to `cleaning_net()`.
  - **Files Touched:**
    - `db_core/src/models.rs`
    - `db_core/src/booking/hold.rs`, `queries.rs`, `update.rs`, `history.rs`
    - `db_core/src/reservation_change.rs`
    - `db_core/src/tax_document.rs`
    - root `.sqlx/`
    - `app_api/booking_api/src/apis/tests.rs`
    - `app_api/booking_api/src/apis/reservation_changes.rs`
    - `db_core/src/promo_tests.rs`
  - **Acceptance Criteria:**
    - [ ] A booking with promo amounts returns the correct `accommodation_net()` and `cleaning_net()`.
    - [ ] Host gross reflects both discounts.
    - [ ] `booking_history` receives the promo columns.
    - [ ] All existing `db_core` and `booking_api` tests pass.
  - **Verification Command:** `cargo test -p db_core promo_tests::booking_columns && cargo test -p db_core && cargo test -p booking_api`
  - **Dependencies:** T3, T4, T5

- [x] **Task 8: Add promo error mapping**
  - **Scope:** XS
  - **Description:** Add `DbError::PromoRefused(PromoRefusal)`, `PromoCodeExists` and `PromoAlreadyDeactivated`, with status codes and `{code, message}` bodies.
  - **Files Touched:**
    - `db_core/src/error.rs`
    - `app_api/api_core/src/error.rs`
  - **Acceptance Criteria:**
    - [ ] Every refusal returns 422 with its §1.4 code.
    - [ ] The two management errors return 409.
    - [ ] Refusals aren't logged as server errors.
  - **Verification Command:** `cargo test -p api_core promo_error_tests`
  - **Dependencies:** T5

---

## Phase C: Redemption & Pricing

- [x] **Task 9: Lock and count code usage**
  - **Scope:** S
  - **Description:** Add `db_core::promo::lock_and_count`: `SELECT ... FOR UPDATE` on the code, then total and per-guest counts using the spec §1.5 live-booking predicate. Also add `get_by_code` and listing eligibility, including the `listing.user_id = owner_host_id` rule. Add a separate non-locking `usage_counts(executor, code_id, guest_id)` for quotes. It excludes that guest's `pending` bookings from both counts, because the hold would cancel them, so a quote can't refuse a code the hold would accept. All of these use `skip(pool, code)` on instrumentation. Skip counting when both limits are `NULL`.
  - **Files Touched:**
    - `db_core/src/promo.rs`
    - `db_core/src/lib.rs`
    - `db_core/src/promo_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Counts include live `pending`, `confirmed` and `completed` bookings.
    - [ ] Counts exclude expired holds (swept or not), `cancelled` bookings and released redemptions.
    - [ ] The per-guest count follows `booking.guest_id` after `transfer_booking_guest`.
    - [ ] A second transaction waits on the lock (`lock_timeout` test, SQLSTATE `55P03`).
    - [ ] A host-owned code isn't eligible on a listing transferred to another host.
    - [ ] With a guest's own live hold on a one-use code, `usage_counts` reports the use as free, and `lock_and_count` still counts it until the hold is cancelled.
  - **Verification Command:** `cargo test -p db_core promo_tests::usage`
  - **Dependencies:** T4, T7, T8

- [x] **Task 10: Record redemptions in `create_booking`**
  - **Scope:** M (highest-risk task: test concurrency before anything else depends on it)
  - **Description:** `NewBooking` gains `promo: Option<NewBookingPromo>`. After cancelling the guest's other holds, lock and count, run `check_status` then `check_usage`, insert the booking and insert the redemption. Change `hold.rs:17` to `skip(pool, new_booking), fields(listing_id = %new_booking.listing_id, guest_id = %new_booking.guest_id)`. Use `SET LOCAL lock_timeout = '5s'` on the promo lock, mapped through `DbError::from_sqlx_contended`, because the pool has 5 connections and waiters would otherwise pin them. Confirmation of a booking with a `promo_code_id` takes the promo lock after `lock_booking`, then re-reads `expires_at > clock_timestamp()` and returns `HoldExpired` if false.
  - **Files Touched:**
    - `db_core/src/models.rs`
    - `db_core/src/booking/hold.rs`
    - `db_core/src/promo_tests.rs`
    - `db_core/src/booking/update.rs`
    - `db_core/src/booking/tests.rs (`promo: None`)`
    - `db_core/src/booking/unavailable_ranges_tests.rs (`promo: None`)`
    - `db_core/src/reservation_change.rs (`promo: None`)`
    - `app_api/booking_api/src/apis/bookings.rs (`promo: None`)`
    - `app_api/booking_api/src/apis/rate_tests.rs, tests.rs (`promo: None`)`
  - **Acceptance Criteria:**
    - [ ] A guest re-placing a hold can reuse a one-use code.
    - [ ] A refused hold rolls back fully, and the guest's earlier hold stays in place.
    - [ ] An expired hold, including through `expire_stale_holds`, frees its use.
    - [ ] Cancelling a confirmed booking frees its use.
    - [ ] A code deactivated during a live hold can still be confirmed.
    - [ ] N ≥ 8 concurrent holds from N distinct guests on N distinct listings, with one remaining use, give exactly one success and N−1 `PROMO_USAGE_LIMIT_REACHED`. Distinct listings mean only the promo lock can serialise them.
    - [ ] With one confirmed redemption and a per-guest limit of 2, two holds on different listings are released together, with a third transaction holding the promo row until `pg_locks` shows both waiting. They leave live uses ≤ 2, and any failure is `PROMO_GUEST_LIMIT_REACHED`. Don't assert exactly one success, since a later hold can legitimately cancel an earlier one.
    - [ ] A hold with no code completes while a second transaction holds the promo row.
    - [ ] A hold that lost its use at expiry can't be confirmed afterwards (real-connection test holding the promo lock across the confirm).
    - [ ] A code expiring during a live hold can still be confirmed.
  - **Verification Command:** `cargo test -p db_core promo_tests::redemption`
  - **Dependencies:** T9

- [x] **Task 11: Share stay pricing and accept a code at hold**
  - **Scope:** M
  - **Description:** Move pricing from `create_booking` into `price_stay`. Resolve the code, checks 1–9, the conversion rate and terms snapshot, then call the calculator. `NewBookingRequest` gains `promo_code`. Log failed attempts with the guest ID, reason and a SHA-256 hash of the code, never the plain code. Add `application.max_stay_nights: i64` (a `DEFAULT_MAX_STAY_NIGHTS` constant of 3650, override with `EA__APPLICATION__MAX_STAY_NIGHTS`, must be positive at startup, checked in a small testable function following the pending-hold check), compare it with `num_days()` before any `i32` conversion, and reject longer stays with a 400 before any rate lookup or calculator call. Stays over a year are valid bookings; the limit only stops absurd ranges. After the calculator, `price_stay` returns a 400 `ValidationError` if any amount stored in a `DECIMAL(10,2)` booking column exceeds 99,999,999.99: subtotal, discount, tax, cleaning fee, deposit, platform fee or total. This is checked before the promo discount is subtracted as well as after, because a 100% code can hide a gross amount that still has to be stored. Add `log_promo_refusal(guest_id, refusal, code)` at `warn`. It hashes the normalized code, or the trimmed raw input if malformed. The hold handler calls it for refusals from `price_stay` and for `DbError::PromoRefused`, through a match arm before the catch-all at `bookings.rs:324`. Validate the requested currency with `CurrencyCode` first, and log a failed rate lookup at `warn`, because this path becomes reachable anonymously in T12.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/stay_pricing.rs`
    - `app_api/booking_api/src/apis/bookings.rs`
    - `app_api/booking_api/src/apis/mod.rs`
    - `common/src/models.rs`
    - `app_api/booking_api/src/apis/promo_hold_tests.rs`
    - `app_api/api_core/src/settings.rs`
    - `app_api/booking_api/src/main.rs`
    - `app_api/booking_api/config/Default.toml`
    - `app_api/user_api/src/apis/tests.rs`
    - `app_api/listing_api/src/apis_test.rs`
    - `app_api/booking_api/src/apis/authz_tests.rs`
    - `docs/ENVIRONMENT.md`
    - `app_api/booking_api/src/apis/rate_tests.rs`
    - `app_api/booking_api/src/apis/tests.rs`
  - **Acceptance Criteria:**
    - [ ] A hold with a valid code stores the code, terms snapshot and both discount amounts.
    - [ ] A hold with an invalid code returns 422 with its reason and creates no hold.
    - [ ] A missing rate for a code's currency returns `FX_RATE_UNAVAILABLE`.
    - [ ] Publishing a new rate after the hold doesn't change the booking.
    - [ ] A hold with no code prices exactly as before, except stays over the limit, which return 400.
    - [ ] Blank `promo_code` prices as no code. `SUM_MER`, `abc` and a 33-character code each return 422 `PROMO_NOT_FOUND` and create no hold.
    - [ ] A refusal under lock logs at `warn` with the hash and emits no `error` event.
    - [ ] A captured-subscriber test (`tracing_subscriber` fmt layer with `FmtSpan::NEW | FmtSpan::CLOSE`) across a real hold and refusal finds the plain code nowhere, including span fields.
    - [ ] A code whose currency was deactivated returns `FX_RATE_UNAVAILABLE`, and existing bookings using it are unaffected.
    - [ ] A non-positive `max_stay_nights` is rejected at startup.
  - **Verification Command:** `cargo test -p booking_api promo_hold_tests`
  - **Dependencies:** T6, T10

- [x] **Task 12: Add the quote endpoints**
  - **Scope:** M
  - **Description:** Add `StayQuoteResponse`, a public `GET /api/v1/bookings/listing/{id}/quote` with no promo, and a signed-in `POST /api/v1/bookings/quote`. A refused code returns 200 with the full-price quote and `promo_refusal`. `cleaning_fee` is the gross `C`, so the rows add up. `POST /quote` calls `caller()`, which rejects deactivated accounts, uses `usage_counts`, and calls `log_promo_refusal` for every §1.4 refusal.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/stay_pricing.rs`
    - `app_api/booking_api/src/apis/routes.rs`
    - `common/src/models.rs`
    - `app_api/booking_api/src/apis/promo_hold_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Public quote, signed-in quote and hold totals are equal for the same inputs with no code, including extra guests and a weekly discount.
    - [ ] With a code, signed-in quote and hold totals are equal.
    - [ ] The public quote needs no auth and ignores any code sent to it.
    - [ ] `POST /quote` without auth returns 401.
    - [ ] Per-guest counts use `claims.sub`.
    - [ ] A stay error returns the same error as the hold.
    - [ ] A refused code returns 200 with the full price and `promo_refusal`; a missing rate returns 200 with `FX_RATE_UNAVAILABLE` in it.
    - [ ] `subtotal − stay_discount − promo.discount + cleaning_fee + tax + platform_fee + deposit == total_amount` for every scope and discount type.
    - [ ] After a guest's live hold on a one-use code, the quote returns the discount and the replacement hold has the same total.
    - [ ] A quote succeeds, the code is then deactivated, and the hold returns 422 with no new hold and the earlier hold intact.
    - [ ] A code failing an early and a late check returns the earlier reason, across 4 vs 7, 6 vs 7 and 9 vs 10.
    - [ ] `check_out=9999-12-31` and a `+262000` year return 400 on the public quote, and the signed-in quote and hold return the same error, without calling `calculate_dynamic_quote`.
    - [ ] A 400-night stay is quoted and can be held. A stay one night over the limit is refused.
    - [ ] A stay within the night limit whose amounts exceed the column maximum returns the same 400 on both quotes and the hold, and never a 500.
    - [ ] A refused quote emits the log event with `claims.sub`, the reason and the hash, and never the plain code.
    - [ ] A deactivated account gets the same refusal from `POST /quote` as from other routes.
  - **Verification Command:** `cargo test -p booking_api promo_hold_tests::quote`
  - **Dependencies:** T11

---

## Phase D: Reservation Changes

- [x] **Task 13: Re-evaluate a promo against a new stay**
  - **Scope:** S
  - **Description:** Add a pure `common::promo::reevaluate(terms, new_stay, eligible)` returning kept or lost with a reason. It runs checks 7–9 only, using the snapshot. Add `PromoOutcome`.
  - **Files Touched:**
    - `common/src/promo.rs`
    - `common/src/models.rs`
  - **Acceptance Criteria:**
    - [ ] A percentage is recalculated on the new amounts.
    - [ ] A fixed amount stays the same, subject to the cap, using the snapshotted rate.
    - [ ] Falling below a minimum, or outside the stay window, returns lost with its reason.
    - [ ] Validity dates, deactivation and usage limits aren't consulted.
  - **Verification Command:** `cargo test -p common promo::reevaluate_tests`
  - **Dependencies:** T6, T7

- [x] **Task 14: Apply promos on direct updates**
  - **Scope:** S
  - **Description:** `reprice` re-evaluates from the booking's snapshot. Promo columns follow `replace_discounts`. When lost, clear them and set `released_at` in the same transaction. Change `update.rs:17` to `skip(pool, updated_booking), fields(booking_id = %id)`.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/booking_updates.rs`
    - `db_core/src/models.rs`
    - `db_core/src/booking/update.rs`
    - `db_core/src/promo_tests.rs`
    - `app_api/booking_api/src/apis/cron.rs`
    - `app_api/booking_api/src/apis/tests.rs`
  - **Acceptance Criteria:**
    - [ ] A date change keeps the discount using the snapshot, even if the code is deactivated or expired.
    - [ ] A lost discount clears the booking's promo fields and frees the use.
    - [ ] The old snapshot stays in `booking_history`.
    - [ ] A captured-subscriber run of `update_booking` with promo terms finds the plain code nowhere.
  - **Verification Command:** `cargo test -p db_core promo_tests::update && cargo test -p booking_api promo_reprice`
  - **Dependencies:** T1, T10, T13

- [x] **Task 15: Apply promos on change quotes and approvals**
  - **Scope:** M
  - **Description:** The change quote re-evaluates and returns `promo_outcome`. `ReservationChangePricingSnapshot` gains `#[serde(default)]` fields. Approval writes promo amounts from the snapshot and releases the redemption if lost. Use the calculator's `discounted_subtotal` and `cleaning_net` for gross, not `reservation_changes.rs:224-225` and the gross fee at `:180`. The snapshot also stores `promo_lost_reason`, exposed on `ReservationChangeRequestResponse`. Approval is rejected when the snapshot keeps a promo but the booking no longer has one, because an admin edit can get in between. The handler also checks this before `settle_balance_delta` (`reservation_changes.rs:556`), so a refusal doesn't come after settlement, and the check under lock stays.
  - **Files Touched:**
    - `common/src/models.rs`
    - `app_api/booking_api/src/apis/reservation_changes.rs`
    - `db_core/src/reservation_change.rs`
  - **Acceptance Criteria:**
    - [ ] The quote reports none, kept or lost, with a reason.
    - [ ] Approving a kept promo writes the new amounts; approving a lost promo clears them and frees the use.
    - [ ] A snapshot stored before this release still deserializes and approves.
    - [ ] Host payout on approval reflects the discounts.
    - [ ] Approving a request that keeps a promo, after a direct edit removed it, is refused and writes no promo amounts.
  - **Verification Command:** `cargo test -p db_core reservation_change::promo_tests && cargo test -p booking_api promo_change_quote`
  - **Dependencies:** T3, T10, T13

---

## Phase E: Records & Communications

- [x] **Task 16: Show promos on tax documents**
  - **Scope:** M
  - **Description:** All three document builders use `cleaning_net()`, and `RefundableAmounts` takes the net cleaning fee. Fill `promo_code` and `promo_discount`, and show "Includes promo discount (CODE)" in the totals. `get_cancellation_quote` builds its own `RefundableAmounts` at `documents.rs:35`, and must also use `cleaning_net()`.
  - **Files Touched:**
    - `db_core/src/tax_document.rs`
    - `common/src/tax_document.rs`
    - `web_ui_common/src/components/domain/tax_document_view.rs`
    - root `.sqlx/`
    - `app_api/booking_api/src/apis/documents.rs`
    - `common/src/cancellation.rs`
  - **Acceptance Criteria:**
    - [ ] Taxable amount equals `A' + C'` and the tax equals the quote's.
    - [ ] Line items sum to the total with a promo.
    - [ ] `refund_total + retained_total == total_price` with a promo in both scopes.
    - [ ] The discount shows on the document, and a no-promo document is unchanged.
    - [ ] For a promo booking, `get_cancellation_quote`'s `refund_total` equals the refund receipt's total, and `retained_total >= 0`. The sum invariant alone can't catch this, since it still holds with a wrong refund.
  - **Verification Command:** `cargo test -p common cancellation::promo && cargo test -p db_core tax_document::promo_tests && cargo test -p booking_api cancellation_quote && npm run test:e2e:guest -- tax-documents`
  - **Dependencies:** T7, T12

- [x] **Task 17: Add ledger columns and CSV fields**
  - **Scope:** M
  - **Description:** Add `cleaning_fee_amount` and `promo_discount_amount` to `NewPayout`, the ledger DTOs and the writes. Retained rows hold 0 for both. Join the promo code for display. Append the CSV columns. Move `csv_cell` from `web_ui_admin/src/support.rs` to `common` and use it for all text cells, closing the formula injection that `listing_name` already has on this export.
  - **Files Touched:**
    - `common/src/payout.rs`
    - `common/src/csv.rs`
    - `db_core/src/payout_ledger.rs`
    - `db_core/src/booking/update.rs`
    - `db_core/src/reservation_change.rs`
    - `db_core/src/models.rs`
    - `web_ui_admin/src/support.rs`
  - **Acceptance Criteria:**
    - [ ] Confirmation, revision and approval write both amounts in the host currency.
    - [ ] Retained rows hold 0 for both, and old rows read as 0.
    - [ ] Ledger entries and CSV include the cleaning fee, code and discount.
    - [ ] New CSV columns are added at the end of the header.
    - [ ] A cell beginning with `=`, `+`, `-` or `@` is neutralised in the CSV.
  - **Verification Command:** `cargo test -p common csv::promo_columns && cargo test -p db_core payout_ledger::promo_tests`
  - **Dependencies:** T2, T3, T7

- [x] **Task 18: Return promos on booking responses**
  - **Scope:** XS
  - **Description:** `BookingResponse` gains `promo_code` and `promo_discount`, and `display_rate` conversion includes the discount.
  - **Files Touched:**
    - `common/src/models.rs`
    - `app_api/api_core/src/models.rs`
    - `app_api/booking_api/src/apis/mod.rs`
    - `app_api/booking_api/src/apis/tests.rs`
  - **Acceptance Criteria:**
    - [ ] Booking responses carry the code and discount.
    - [ ] A display-currency response converts the discount.
  - **Verification Command:** `cargo test -p booking_api promo_booking_response`
  - **Dependencies:** T7

- [x] **Task 19: Show promos in guest emails**
  - **Scope:** S
  - **Description:** Guest payloads gain `#[serde(default)]` promo fields. `{{promo_line}}` is empty when there's no promo. Both the hold-time and confirmation emails carry the discount, using after-promo figures. `send_completion_email` (`notifications.rs:359`) and the confirmation payload (`:175`) use `cleaning_net()`. Extract pure payload builders, because payloads are built inside `tokio::spawn`, and test those.
  - **Files Touched:**
    - `common/src/email.rs`
    - `app_api/booking_api/src/apis/notifications.rs`
    - `app_api/booking_api/src/apis/bookings.rs`
    - `app_api/email_worker/src/processor.rs`
    - `app_api/email_worker/templates/booking_confirmation_guest.html`
    - `app_api/email_worker/src/tests.rs`
  - **Acceptance Criteria:**
    - [ ] A promo booking's email shows the code and discount.
    - [ ] A no-promo email has no empty line or leftover placeholder.
    - [ ] Payloads queued before this release still render.
    - [ ] The completion email's total equals the tax invoice's `total_amount` for a `stay_and_cleaning` promo.
  - **Verification Command:** `cargo test -p booking_api notifications::tests && cargo test -p email_worker promo_line`
  - **Dependencies:** T7, T11

- [x] **Task 20: Correct host email payouts**
  - **Scope:** S
  - **Description:** The confirmation and update payloads take `total_payout` and `currency` from the ledger row. If it's missing, log an error and send no payout figure. Add the promo line. `total_payout` becomes `Option` with a conditional line, so a missing row shows no blank amount. Correct `payout_impact` on the host cancellation email (`notifications.rs:261`), which sends `sub_total_price` as "Net Payout Impact", to the retained net from the ledger. Prefix the builder tests with `host_`.
  - **Files Touched:**
    - `common/src/email.rs`
    - `app_api/booking_api/src/apis/notifications.rs`
    - `app_api/email_worker/src/processor.rs`
    - `app_api/email_worker/templates/booking_confirmation_host.html`
    - `app_api/email_worker/templates/booking_updated_host.html`
    - `app_api/email_worker/templates/booking_cancelled_host.html`
    - `app_api/email_worker/src/tests.rs`
  - **Acceptance Criteria:**
    - [ ] `total_payout` equals the ledger net, in the ledger currency, with and without a promo.
    - [ ] A missing ledger row logs an error and shows no invented figure.
    - [ ] A host cancellation email shows the ledger's retained net, or no figure when nothing is retained.
  - **Verification Command:** `cargo test -p booking_api notifications::tests::host_ && cargo test -p email_worker host_payout`
  - **Dependencies:** T17, T19


- [x] **Task 20a: Prove the figures agree across surfaces**
  - **Scope:** S
  - **Description:** One integration test takes a promo booking through quote, hold, confirmation and documents, for spec §6's agreement and immutability requirements.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/promo_hold_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Discount, net and tax agree across the quote, booking, email payload, tax document and ledger row.
    - [ ] Deactivating the code and publishing a rate leaves the booking, document and ledger values unchanged.
  - **Verification Command:** `cargo test -p booking_api promo_hold_tests::agreement`
  - **Dependencies:** T11, T16, T17, T19, T20
---

## Phase F: Management & Portals

- [x] **Task 21: Add promo management queries**
  - **Scope:** S
  - **Description:** Add create (with selected listings), list with stats from one grouped query per page, get and deactivate. Map unique violations to `PromoCodeExists`. Map `ON DELETE RESTRICT` failures to clear refusals in `delete_listing` and `hard_delete_user`, since the new foreign keys reach those paths.
  - **Files Touched:**
    - `db_core/src/promo.rs`
    - `db_core/src/promo_tests.rs`
    - root `.sqlx/`
    - `db_core/src/listing/write.rs`
    - `db_core/src/user.rs`
  - **Acceptance Criteria:**
    - [ ] A case-variant duplicate returns `PromoCodeExists`.
    - [ ] Listing filters by owner, and admin sees all.
    - [ ] Stats split active holds, confirmed uses and remaining uses correctly.
    - [ ] Deactivating twice returns `PromoAlreadyDeactivated`.
    - [ ] A used code can't be deleted, accepting SQLSTATE `23001` or `23503`.
    - [ ] Hard-deleting a booking with a redemption, or a listing or user tied to a code, returns a clear refusal instead of a 500.
    - [ ] Two concurrent deactivations give one success and one `PromoAlreadyDeactivated`.
  - **Verification Command:** `cargo test -p db_core promo_tests::management`
  - **Dependencies:** T8, T9

- [x] **Task 22: Add the promo management API**
  - **Scope:** M
  - **Description:** Add `apis/promo_codes.rs` with list, create, get and deactivate under `/api/v1/admin/promo-codes`, using `require_staff` plus the spec §1.2 ownership rules. Add request validation for §1.1 and the DTOs. Wrap the scope in `from_fn(require_user)`. `validate_new_code` in `common::promo` covers §1.1, including at most 2 decimal places and no empty selected-listings set. Create and deactivate log `info` with `promo_code_id` and `actor_id`, never `code`.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/promo_codes.rs`
    - `app_api/booking_api/src/apis/routes.rs`
    - `app_api/booking_api/src/apis/mod.rs`
    - `common/src/promo.rs`
    - `app_api/booking_api/src/apis/authz_tests.rs`
  - **Acceptance Criteria:**
    - [ ] A host gets 403 on creating for, viewing or deactivating another host's or an admin's code.
    - [ ] A guest gets 403 on all management routes.
    - [ ] A host can't create a code with a listing they don't own.
    - [ ] Admin-created codes have no owner, and host codes have the host as owner.
    - [ ] `created_by` and `deactivated_by` come from the authenticated user, never the request body.
    - [ ] An inactive currency returns 422 `CURRENCY_INACTIVE`.
    - [ ] Each §1.1 violation returns 400, through table-driven tests, including `0.001`, `33.335` and `applies_to_all=false` with no listings.
    - [ ] A case-variant duplicate and a double deactivation each return 409.
    - [ ] An anonymous caller gets 401.
  - **Verification Command:** `cargo test -p booking_api promo_codes && cargo test -p booking_api authz_tests::promo_codes && cargo test -p common promo::validation`
  - **Dependencies:** T21

- [x] **Task 23: Add client methods**
  - **Scope:** S
  - **Description:** Add `common/src/app_client/promo.rs` and `web_client` wrappers for both quotes and all management endpoints. Methods taking an ID take `Uuid`. Extract a pure error-parsing function so preservation of the code can be unit tested.
  - **Files Touched:**
    - `common/src/app_client/promo.rs`
    - `common/src/app_client/mod.rs`
    - `web_client/src/client.rs`
    - `web_client/src/api_client.rs`
  - **Acceptance Criteria:**
    - [ ] All new endpoints have typed client methods.
    - [ ] API error codes are preserved in the error text, so `PROMO_*` codes can be matched by the portals.
  - **Verification Command:** `cargo test -p common app_client::promo && SQLX_OFFLINE=true cargo check -p web_client --all-targets`
  - **Dependencies:** T12, T22

- [x] **Task 24: Use one stay total on the listing page and checkout**
  - **Scope:** M
  - **Description:** `PriceBreakdown` gains `platform_fee`, `promo_code` and `promo_discount`. The listing page and checkout both use the API quote. Delete `stay_price`, `GCT_RATE`, the old `quote_for`, `common::app_client::get_pricing_quote` and its wrappers. Update `checkout-flow.spec.ts:35`. "Confirm reservation" is disabled when no quote is available, so a guest can't place a hold without seeing a total. Cite #106 and spec §3.5 in the commit that deletes `quote.rs` tests, as `CONSTRAINTS.md` requires.
  - **Files Touched:**
    - `web_ui_common/src/components/domain/price_breakdown.rs`
    - `web_ui_guest/src/quote.rs`
    - `web_ui_guest/src/pages/listing_detail.rs`
    - `web_ui_guest/src/pages/checkout.rs`
    - `common/src/app_client/listings.rs`, `web_client/src/client.rs`, `web_client/src/api_client.rs`
    - `playwright/e2e/guest-portal/checkout-flow.spec.ts`
    - `common/src/app_client/mod.rs`
  - **Acceptance Criteria:**
    - [ ] The listing page and checkout show the same total as the hold, including extra guests and a weekly discount.
    - [ ] The listing page works signed out.
    - [ ] Dates that can't be quoted show no price, with no fallback figure.
    - [ ] `checkout-flow.spec.ts:35` expects 4136.25 and passes, and `calendar.spec.ts` still passes.
    - [ ] A new E2E assertion checks that the listing page and checkout totals match.
    - [ ] An over-limit stay shows no price.
    - [ ] When the quote fails, checkout shows no total and "Confirm reservation" is disabled.
  - **Verification Command:** `! rg 'stay_price|GCT_RATE|get_pricing_quote' web_ui_guest web_client common/src/app_client && cargo test -p web_ui_common -p web_ui_guest && npm run test:e2e:guest`
  - **Dependencies:** T23

- [x] **Task 25: Add promo entry to checkout**
  - **Scope:** M
  - **Description:** Add the code field and "Apply" button (`name="intent" value="apply_promo" formnovalidate`) inside the existing form, before "Confirm reservation", plus "Remove". Re-render without placing a booking. `place_booking` sends the code and maps `PROMO_*` errors to `PromoRefusal::message()`. `db-seed.ts` deletes redemptions, clears `booking.promo_code_id`, and deletes `promo_code_listing` and `promo_code` rows before its existing deletes, because the new `RESTRICT` keys would make cleanup fail. Promo fixtures use `psql`, so they don't depend on the admin page, and random code suffixes, because codes are permanently unique and two browser projects run in parallel.
  - **Files Touched:**
    - `web_ui_guest/src/pages/checkout.rs`
    - `web_ui_common/src/components/domain/price_breakdown.rs`
    - `playwright/e2e/guest-portal/checkout-flow.spec.ts`
    - `playwright/e2e/setup/db-seed.ts`
    - `playwright/e2e/fixtures/bookings-data.ts`
  - **Acceptance Criteria:**
    - [ ] Applying a valid code shows the discount and the new total, and confirming stores it.
    - [ ] An expired code shows its reason and the full price.
    - [ ] Details the guest entered are kept after applying.
    - [ ] Pressing Enter applies the code and doesn't place a booking.
    - [ ] A code invalidated before confirming shows the reason and keeps the code entered.
    - [ ] It works with JavaScript disabled.
    - [ ] "Remove" clears the code and restores the full price.
    - [ ] Input of `"><script>` is echoed escaped.
    - [ ] A second consecutive E2E run logs `✓ Removed data left by previous e2e runs`.
    - [ ] A `checkout::promo_tests` unit test maps each `PROMO_*` error to `PromoRefusal::message()`.
  - **Verification Command:** `cargo test -p web_ui_guest checkout::promo_tests && npm run test:e2e:guest -- checkout-flow`
  - **Dependencies:** T24

- [ ] **Task 26: Warn guests about a lost discount**
  - **Scope:** S
  - **Description:** `booking_change.rs` shows a warning, with the reason, when `promo_outcome` is lost. The pending-change banner in `bookings.rs` shows `promo_lost_reason`, because a host-proposed change is accepted there, without visiting the change page.
  - **Files Touched:**
    - `web_ui_guest/src/pages/booking_change.rs`
    - `web_ui_guest/src/pages/bookings.rs`
    - `playwright/e2e/guest-portal/reservation-changes.spec.ts`
  - **Acceptance Criteria:**
    - [ ] The warning appears before the guest accepts.
    - [ ] It doesn't appear for none or kept.
    - [ ] A host- or admin-proposed change that loses the discount shows the reason in the banner before "Accept change".
  - **Verification Command:** `npm run test:e2e:guest -- reservation-changes`
  - **Dependencies:** T15, T23, T25

- [ ] **Task 27: Add the admin promo page**
  - **Scope:** M
  - **Description:** Add `pages/promo_codes.rs`, following `currencies.rs`: list with stats, create form, deactivate, and nav links. Dates are Jamaica calendar days, sent as the start of `valid_from` and the end of `valid_to`. All three routes use `staff(cx)` from `web_ui_admin/src/support.rs`, not the `admin_only` that `currencies.rs` uses, and parse `{id}` as `Uuid` before calling the API. Log `promo_code_id`, not the code. Fixtures use random code suffixes.
  - **Files Touched:**
    - `web_ui_admin/src/pages/promo_codes.rs`
    - `web_ui_admin/src/pages/mod.rs`
    - `web_ui_common/src/shell_admin.rs`
    - `playwright/e2e/admin-portal/promo-codes.spec.ts`
  - **Acceptance Criteria:**
    - [ ] An admin can create, list and deactivate a code with every §1.1 option.
    - [ ] A host sees only their own codes, and only their listings in the picker.
    - [ ] API validation errors appear on the form, and entered values are kept.
    - [ ] Deactivate uses `hx-disabled-elt` to stop double submits.
    - [ ] Output is escaped.
    - [ ] A unit test following `currencies.rs` covers the list, the status and the deactivate form, and that titles are escaped.
  - **Verification Command:** `cargo test -p web_ui_admin promo_codes && npm run test:e2e:admin -- promo-codes`
  - **Dependencies:** T23, T25

- [ ] **Task 28: Show promos on the ledger and booking detail pages**
  - **Scope:** S
  - **Description:** Add the cleaning fee, code and discount to `payouts.rs`, showing accommodation as `gross − cleaning_fee_amount`. Add the promo line to the guest and admin booking detail pages. Also add the columns to the portal's own export (`payouts.rs:199`), which is built separately from the API's CSV and uses `csv_cell`. Extend `checkout-flow.spec.ts`: apply, confirm, and see the discount on the booking.
  - **Files Touched:**
    - `web_ui_admin/src/pages/payouts.rs`
    - `web_ui_admin/src/pages/bookings/detail.rs`
    - `web_ui_guest/src/pages/bookings.rs`
    - `playwright/e2e/guest-portal/checkout-flow.spec.ts`
    - `playwright/e2e/admin-portal/payouts-admin.spec.ts`
    - `playwright/e2e/admin-portal/bookings-admin.spec.ts`
  - **Acceptance Criteria:**
    - [ ] A promo booking shows the code and discount on all three pages.
    - [ ] A host can see the code and discount from an admin-created code on their own bookings and ledger.
    - [ ] Pages without a promo are unchanged.
    - [ ] The portal's export includes the new columns.
  - **Verification Command:** `npm run test:e2e:admin -- payouts-admin bookings-admin && npm run test:e2e:guest -- bookings checkout-flow`
  - **Dependencies:** T17, T18, T25

---

## Phase G: Finish

- [ ] **Task 29: Update docs and API contracts**
  - **Scope:** S
  - **Description:** Add the pricing order, lock order and payout formula to `docs/ARCHITECTURE.md`. Add the new paths to both OpenAPI files, and to the utoipa registration in `routes.rs`.
  - **Files Touched:**
    - `docs/ARCHITECTURE.md`
    - `openapi.yaml`
    - `openapi-prod.yaml`
    - `app_api/booking_api/src/apis/routes.rs`
  - **Acceptance Criteria:**
    - [ ] §4.2 states the promo position and is correct about tax and fee order.
    - [ ] The public quote, signed-in quote and all management paths are in both OpenAPI files.
    - [ ] A new test asserts that the generated OpenAPI document includes the new paths.
    - [ ] The promised lock order and payout formula are in `docs/ARCHITECTURE.md`, and `max_stay_nights` is in its settings table.
  - **Verification Command:** `for f in openapi.yaml openapi-prod.yaml; do for p in '/api/v1/bookings/listing/{id}/quote:' '/api/v1/bookings/quote:' '/api/v1/admin/promo-codes:' '/api/v1/admin/promo-codes/{id}:' '/api/v1/admin/promo-codes/{id}/deactivate:'; do rg -qF "$p" "$f" || exit 1; done; done && rg -q 'promo code' docs/ARCHITECTURE.md && cargo test -p booking_api openapi_includes_promo`
  - **Dependencies:** T12, T22

- [ ] **Task 30: Run the final gates**
  - **Scope:** S
  - **Description:** Run every `CONSTRAINTS.md` gate, `/sanity-check-workflow` and the full suites, then check each spec §7 acceptance criterion against a test. Run `/audit-booking-flow`.
  - **Files Touched:** None expected. Fixes go in the task that caused the failure.
  - **Acceptance Criteria:**
    - [ ] Every §7 acceptance criterion maps to a passing test.
    - [ ] All gates pass, with no new `#[allow]`, `#[ignore]` or `.skip`, except the three baseline failures in the note below, which pass on this branch's diff.
    - [ ] `/audit-booking-flow` reports no unresolved finding.
  - **Verification Command:** `cargo fmt --check && SQLX_OFFLINE=true cargo check --workspace --exclude protoproj --all-features --tests && SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features -- -D warnings && devenv shell -- cargo sqlx prepare --workspace --check -- --all-targets && cargo test --workspace --exclude web_client --exclude web_ui_common --exclude web_ui_guest --exclude web_ui_admin --exclude protoproj && cargo test -p web_client -p web_ui_common -p web_ui_guest -p web_ui_admin && ! rg -q "f(32|64)" common/src/pricing.rs common/src/promo.rs db_core/src/promo.rs app_api/booking_api/ && cargo audit && npm run test:e2e`
  - **Baseline note:** three `CONSTRAINTS.md` gates already fail at `e2f6e2d0`, so they can't pass as whole-repo checks here. Run each separately, outside the command above. The pass condition is no failure in an item added or changed since `e2f6e2d0`, and the rest is reported, not fixed here. `CONSTRAINTS.md` stays unchanged.
    - `.unwrap()`/`.expect()` grep: matches 40 files. Review `git diff e2f6e2d0` for new uses outside test modules.
    - `env::var` literal grep: review the diff for new raw `env::var("...")` literals.
    - `SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features --message-format=short -- -W missing_docs`: `common` and `db_core` have undocumented public items. Warnings don't stop dependent crates compiling, where `-D` would hide them. Confirm no `missing documentation` line points at an item added or changed since `e2f6e2d0`.
  - **Dependencies:** All
