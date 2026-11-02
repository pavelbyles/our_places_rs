# Implementation Plan: Promo Codes (#106)

**Spec Reference:** [`docs/specs/spec-106-feat-promo-codes.md`](../specs/spec-106-feat-promo-codes.md)
**Intent:** [`docs/intent/intent-106-feat-promo-codes.md`](../intent/intent-106-feat-promo-codes.md)
**Baseline Commit:** `e2f6e2d0`
**Branch:** `feat/106-promo-codes`

## Cross-Cutting Rules (apply to every task)
- **Local database**: Postgres runs in the devenv shell, not Docker. Run `sqlx` and `psql` commands through `devenv shell -- <command>`. Database tests need it running.
- **SQLx metadata**: after changing a query, run `devenv shell -- cargo sqlx prepare --workspace -- --all-targets` and commit the `.sqlx/` changes with that task.
- **Task gates**: `cargo fmt --check` and `SQLX_OFFLINE=true cargo clippy -p <touched crates> --all-targets --all-features -- -D warnings`. Every new public item has a `///` comment.
- **Tests come first**: write the failing test, then the code. New tests live in named modules, such as `mod promo_tests`. A filter that matches no tests reports "0 passed", which doesn't count as green, so check the count.
- **Concurrency tests** use real connections and `sqlx_db_tester::TestPg`, with no sleeps.
- **Error mapping**: every new `DbError` variant has explicit arms in both `status_code` and `error_response`.
- **Money**: `Decimal` only, `round_dp(2)` only, and no fallback amounts or rates.
- **Lock order**: listing → currency → promo code. Never take an earlier lock after a later one.
- **Promo-free behaviour**: outside the changes the spec calls for, bookings without a code must price as they do today.
- **Existing tests**: update them only for the changes the spec calls for, without weakening an assertion.

## Dependency Graph
Each line is `task ← dependencies`, matching every task's **Dependencies** field.

```text
Phase A: payout fixes (no promo dependency, highest financial risk)
T1  stale discount            ← —
T2  retained payouts          ← —
T3  host gross + cleaning     ← —

Phase B: promo foundations
T4  migration                 ← —
T5  common::promo             ← —
T6  calculator type-state     ← T5
T7  Booking promo columns     ← T3, T4
T8  promo errors              ← T5

Phase C: redemption and pricing
T9  usage lock and count      ← T4, T7, T8
T10 redemption in hold        ← T9
T11 price_stay + hold code    ← T6, T10
T12 quote endpoints           ← T11

Phase D: reservation changes
T13 promo re-evaluation       ← T6, T7
T14 direct update path        ← T1, T13
T15 change quote + approval   ← T3, T13

Phase E: records and communications
T16 tax documents             ← T7
T17 ledger columns + CSV      ← T2, T3, T7
T18 booking response          ← T7
T19 guest emails              ← T7
T20 host emails               ← T17

Phase F: management and portals
T21 promo management db       ← T4, T8
T22 promo management API      ← T21
T23 client methods            ← T12, T22
T24 one stay total            ← T23
T25 checkout promo entry      ← T24
T26 change page warning       ← T15, T23
T27 admin promo page          ← T23
T28 ledger + booking detail   ← T17, T18

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
| §1.8 display | T16–T20, T24–T28 |
| Stale discount | T1, T14 |

---

## Phase A: Payout Fixes

- [ ] **Task 1: Clear stale discounts on direct updates**
  - **Scope:** S
  - **Description:** Add `UpdatedBooking.replace_discounts`, set it in `reprice`, and write `discount_value = CASE WHEN $21 THEN $9 ELSE COALESCE($9, discount_value) END`.
  - **Files Touched:**
    - `db_core/src/models.rs`
    - `db_core/src/booking/update.rs`
    - `app_api/booking_api/src/apis/booking_updates.rs`
    - `db_core/src/booking/tests.rs`
  - **Acceptance Criteria:**
    - [ ] A failing test first reproduces the stale discount on a stay shortened below 7 nights.
    - [ ] Shortening below the threshold clears `discount_value`, and `accommodation_net()` matches the new total.
    - [ ] A status-only update keeps the discount.
  - **Verification Command:** `cargo test -p db_core stale_discount`
  - **Dependencies:** None

- [ ] **Task 2: Write retained payouts on partial refunds**
  - **Scope:** S
  - **Description:** Add `reopen_payout_as_retained`, and have `settle_cancellation` use the ledger ID from `cancel_pending_payout_for_booking`. `create_payout_ledger_entry` keeps `DO NOTHING`.
  - **Files Touched:**
    - `db_core/src/payout_ledger.rs`
    - `db_core/src/booking/update.rs`
    - `db_core/src/booking/tests.rs`
  - **Acceptance Criteria:**
    - [ ] A failing test first shows that no retained row exists today, confirming the bug.
    - [ ] A partial refund leaves one `pending` row with the retained gross, commission and net.
    - [ ] A full refund leaves the row `cancelled`.
    - [ ] A `processing` or `paid` row is unchanged, and a warning is logged.
    - [ ] Repeating confirmation neither duplicates nor overwrites a row.
  - **Verification Command:** `cargo test -p db_core retained_payout`
  - **Dependencies:** None

- [ ] **Task 3: Include the cleaning fee in host gross**
  - **Scope:** M
  - **Description:** Add `common::pricing::host_gross` and use it in every gross calculation. `approve_change_request` recalculates from the updated booking, using the ledger row's `exchange_rate` and `platform_fee_pct`, instead of writing the snapshot's amounts. Uses `booking.cleaning_fee` until T7 adds `cleaning_net()`.
  - **Files Touched:**
    - `common/src/pricing.rs`
    - `db_core/src/booking/update.rs`
    - `db_core/src/reservation_change.rs`
    - `app_api/booking_api/src/apis/reservation_changes.rs`
    - `db_core/src/booking/hold_expiry_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Gross includes the cleaning fee at confirmation, on revision, in the change quote (previous and proposed) and on approval.
    - [ ] Commission applies to the whole gross.
    - [ ] Revising an older row uses the new formula.
    - [ ] Approving a request whose snapshot holds an old-formula amount writes the new-formula amount.
    - [ ] A retained payout stays accommodation only.
    - [ ] `rg 'accommodation_net\(\) \* ' db_core app_api` finds no remaining gross calculation outside `host_gross`.
  - **Verification Command:** `cargo test -p common host_gross && cargo test -p db_core host_gross && cargo test -p booking_api reservation_change`
  - **Dependencies:** None

---

## Phase B: Promo Foundations

- [ ] **Task 4: Add the promo migration**
  - **Scope:** S
  - **Description:** Run `devenv shell -- sqlx migrate add --source db_core/migrations promo_codes`, then write spec §3.1: tables, enums, booking, history, ledger and tax document columns, and the rescheduled `cleanup_stale_pending_holds` job.
  - **Files Touched:**
    - `db_core/migrations/<timestamp>_promo_codes.sql`
  - **Acceptance Criteria:**
    - [ ] The migration applies to a fresh database and the existing database.
    - [ ] `CHECK` constraints reject a percentage above 100, a fixed discount with no currency, and a lowercase code.
    - [ ] `promo_code.currency` has its foreign key to `currencies`.
    - [ ] The migration has no `UPDATE` on `host_payout_ledger` or `booking`.
    - [ ] The pg_cron history insert includes the four promo columns and its logic is otherwise unchanged.
  - **Verification Command:** `devenv shell -- sqlx migrate run --source db_core/migrations && ! rg -qi 'UPDATE\s+(host_payout_ledger|booking)\b' db_core/migrations/*_promo_codes.sql && cargo test -p db_core promo_schema`
  - **Dependencies:** None

- [ ] **Task 5: Add `common::promo` rules**
  - **Scope:** S
  - **Description:** Add `PromoTerms`, `PromoRefusal`, `normalize_code`, `check_redeemable` and `check_stay_rules`. Refusals follow the order in spec §1.4.
  - **Files Touched:**
    - `common/src/promo.rs`
    - `common/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] Every §1.4 check has a test, including boundaries: the `valid_to` instant, the last eligible night and the exact minimum value.
    - [ ] When several checks fail, the first in §1.4 order is returned.
    - [ ] `" Summer "` normalizes to `SUMMER`; bad characters and lengths are rejected; blank is `None`.
    - [ ] `PromoTerms` round-trips through JSON with no loss of precision.
  - **Verification Command:** `cargo test -p common promo::`
  - **Dependencies:** None

- [ ] **Task 6: Add the calculator's promo step**
  - **Scope:** M
  - **Description:** Add the `PromoApplied` type-state and `apply_promo(Option<&PromoTerms>)`, and move `apply_taxes` to it. Add `promo_stay_discount`, `promo_cleaning_discount` and `cleaning_net`. All call sites pass `None` for now.
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
  - **Verification Command:** `cargo test -p common pricing:: && cargo test -p booking_api --lib apis::tests`
  - **Dependencies:** T5

- [ ] **Task 7: Add promo columns to `Booking`**
  - **Scope:** M (mechanical; `query_as!` requires every query to change together, so this can't be split further)
  - **Description:** Add the four columns to `Booking` and to every query that returns it. Update `record_booking_history`. `accommodation_net()` subtracts `promo_stay_discount`, and `cleaning_net()` is new. Switch `host_gross` callers to `cleaning_net()`.
  - **Files Touched:**
    - `db_core/src/models.rs`
    - `db_core/src/booking/hold.rs`, `queries.rs`, `update.rs`, `history.rs`
    - `db_core/src/reservation_change.rs`
    - `db_core/src/tax_document.rs`
    - `db_core/.sqlx/`
  - **Acceptance Criteria:**
    - [ ] A booking with promo amounts returns the correct `accommodation_net()` and `cleaning_net()`.
    - [ ] Host gross reflects both discounts.
    - [ ] `booking_history` receives the promo columns.
    - [ ] All existing `db_core` and `booking_api` tests pass.
  - **Verification Command:** `cargo test -p db_core && cargo test -p booking_api`
  - **Dependencies:** T3, T4

- [ ] **Task 8: Add promo error mapping**
  - **Scope:** XS
  - **Description:** Add `DbError::PromoRefused(PromoRefusal)`, `PromoCodeExists` and `PromoAlreadyDeactivated`, with status codes and `{code, message}` bodies.
  - **Files Touched:**
    - `db_core/src/error.rs`
    - `app_api/api_core/src/error.rs`
  - **Acceptance Criteria:**
    - [ ] Every refusal returns 422 with its §1.4 code.
    - [ ] The two management errors return 409.
    - [ ] Refusals aren't logged as server errors.
  - **Verification Command:** `cargo test -p api_core promo_error`
  - **Dependencies:** T5

---

## Phase C: Redemption & Pricing

- [ ] **Task 9: Lock and count code usage**
  - **Scope:** S
  - **Description:** Add `db_core::promo::lock_and_count`: `SELECT ... FOR UPDATE` on the code, then total and per-guest counts using the spec §1.5 live-booking predicate. Also add `get_by_code` and listing eligibility, including the `listing.user_id = owner_host_id` rule.
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
  - **Verification Command:** `cargo test -p db_core promo_tests::usage`
  - **Dependencies:** T4, T7, T8

- [ ] **Task 10: Record redemptions in `create_booking`**
  - **Scope:** M (highest-risk task: test concurrency before anything else depends on it)
  - **Description:** `NewBooking` gains `promo: Option<NewBookingPromo>`. After cancelling the guest's other holds, lock and count, run `check_redeemable`, insert the booking and insert the redemption.
  - **Files Touched:**
    - `db_core/src/models.rs`
    - `db_core/src/booking/hold.rs`
    - `db_core/src/promo_tests.rs`
  - **Acceptance Criteria:**
    - [ ] N concurrent holds on one remaining use give exactly one success and N−1 `PROMO_USAGE_LIMIT_REACHED`.
    - [ ] Concurrent holds can't exceed the per-guest limit.
    - [ ] A guest re-placing a hold can reuse a one-use code.
    - [ ] A refused hold rolls back fully, and the guest's earlier hold stays in place.
    - [ ] An expired hold, including through `expire_stale_holds`, frees its use.
    - [ ] Cancelling a confirmed booking frees its use.
    - [ ] A code deactivated during a live hold can still be confirmed.
    - [ ] Holds with no code take no promo lock.
  - **Verification Command:** `cargo test -p db_core promo_tests::redemption`
  - **Dependencies:** T9

- [ ] **Task 11: Share stay pricing and accept a code at hold**
  - **Scope:** M
  - **Description:** Move pricing from `create_booking` into `price_stay`. Resolve the code, checks 1–9, the conversion rate and terms snapshot, then call the calculator. `NewBookingRequest` gains `promo_code`. Log failed attempts with the guest ID, reason and a SHA-256 hash of the code, never the plain code.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/stay_pricing.rs`
    - `app_api/booking_api/src/apis/bookings.rs`
    - `app_api/booking_api/src/apis/mod.rs`
    - `common/src/models.rs`
    - `app_api/booking_api/src/apis/promo_hold_tests.rs`
  - **Acceptance Criteria:**
    - [ ] A hold with a valid code stores the code, terms snapshot and both discount amounts.
    - [ ] A hold with an invalid code returns 422 with its reason and creates no hold.
    - [ ] A missing rate for a code's currency returns `FX_RATE_UNAVAILABLE`.
    - [ ] A hold with no code prices exactly as before.
    - [ ] Publishing a new rate after the hold doesn't change the booking.
    - [ ] Logs contain the hash and never the plain code (test asserts on the log helper's output).
  - **Verification Command:** `cargo test -p booking_api promo_hold_tests`
  - **Dependencies:** T6, T10

- [ ] **Task 12: Add the quote endpoints**
  - **Scope:** M
  - **Description:** Add `StayQuoteResponse`, a public `GET /api/v1/bookings/listing/{id}/quote` with no promo, and a signed-in `POST /api/v1/bookings/quote`. A refused code returns 200 with the full-price quote and `promo_refusal`.
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
  - **Verification Command:** `cargo test -p booking_api promo_hold_tests::quote`
  - **Dependencies:** T11

---

## Phase D: Reservation Changes

- [ ] **Task 13: Re-evaluate a promo against a new stay**
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
  - **Verification Command:** `cargo test -p common promo::reevaluate`
  - **Dependencies:** T6, T7

- [ ] **Task 14: Apply promos on direct updates**
  - **Scope:** S
  - **Description:** `reprice` re-evaluates from the booking's snapshot. Promo columns follow `replace_discounts`. When lost, clear them and set `released_at` in the same transaction.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/booking_updates.rs`
    - `db_core/src/models.rs`
    - `db_core/src/booking/update.rs`
    - `db_core/src/promo_tests.rs`
  - **Acceptance Criteria:**
    - [ ] A date change keeps the discount using the snapshot, even if the code is deactivated or expired.
    - [ ] A lost discount clears the booking's promo fields and frees the use.
    - [ ] The old snapshot stays in `booking_history`.
  - **Verification Command:** `cargo test -p db_core promo_tests::update && cargo test -p booking_api booking_update`
  - **Dependencies:** T1, T13

- [ ] **Task 15: Apply promos on change quotes and approvals**
  - **Scope:** M
  - **Description:** The change quote re-evaluates and returns `promo_outcome`. `ReservationChangePricingSnapshot` gains `#[serde(default)]` fields. Approval writes promo amounts from the snapshot and releases the redemption if lost.
  - **Files Touched:**
    - `common/src/models.rs`
    - `app_api/booking_api/src/apis/reservation_changes.rs`
    - `db_core/src/reservation_change.rs`
  - **Acceptance Criteria:**
    - [ ] The quote reports none, kept or lost, with a reason.
    - [ ] Approving a kept promo writes the new amounts; approving a lost promo clears them and frees the use.
    - [ ] A snapshot stored before this release still deserializes and approves.
    - [ ] Host payout on approval reflects the discounts.
  - **Verification Command:** `cargo test -p db_core reservation_change && cargo test -p booking_api reservation_change`
  - **Dependencies:** T3, T13

---

## Phase E: Records & Communications

- [ ] **Task 16: Show promos on tax documents**
  - **Scope:** M
  - **Description:** All three document builders use `cleaning_net()`, and `RefundableAmounts` takes the net cleaning fee. Fill `promo_code` and `promo_discount`, and show "Includes promo discount (CODE)" in the totals.
  - **Files Touched:**
    - `db_core/src/tax_document.rs`
    - `common/src/tax_document.rs`
    - `web_ui_common/src/components/domain/tax_document_view.rs`
    - `db_core/.sqlx/`
  - **Acceptance Criteria:**
    - [ ] Taxable amount equals `A' + C'` and the tax equals the quote's.
    - [ ] Line items sum to the total with a promo.
    - [ ] `refund_total + retained_total == total_price` with a promo in both scopes.
    - [ ] The discount shows on the document, and a no-promo document is unchanged.
  - **Verification Command:** `cargo test -p db_core tax_document && cargo test -p web_ui_common tax_document`
  - **Dependencies:** T7

- [ ] **Task 17: Add ledger columns and CSV fields**
  - **Scope:** M
  - **Description:** Add `cleaning_fee_amount` and `promo_discount_amount` to `NewPayout`, the ledger DTOs and the writes. Retained rows hold 0 for both. Join the promo code for display. Append the CSV columns.
  - **Files Touched:**
    - `common/src/payout.rs`
    - `common/src/csv.rs`
    - `db_core/src/payout_ledger.rs`
    - `db_core/src/booking/update.rs`
    - `db_core/src/reservation_change.rs`
  - **Acceptance Criteria:**
    - [ ] Confirmation, revision and approval write both amounts in the host currency.
    - [ ] Retained rows hold 0 for both, and old rows read as 0.
    - [ ] Ledger entries and CSV include the cleaning fee, code and discount.
    - [ ] New CSV columns are added at the end of the header.
  - **Verification Command:** `cargo test -p common csv && cargo test -p db_core payout_ledger`
  - **Dependencies:** T2, T3, T7

- [ ] **Task 18: Return promos on booking responses**
  - **Scope:** XS
  - **Description:** `BookingResponse` gains `promo_code` and `promo_discount`, and `display_rate` conversion includes the discount.
  - **Files Touched:**
    - `common/src/models.rs`
    - `app_api/api_core/src/models.rs`
    - `app_api/booking_api/src/apis/mod.rs`
  - **Acceptance Criteria:**
    - [ ] Booking responses carry the code and discount.
    - [ ] A display-currency response converts the discount.
  - **Verification Command:** `cargo test -p booking_api booking_response`
  - **Dependencies:** T7

- [ ] **Task 19: Show promos in guest emails**
  - **Scope:** S
  - **Description:** Guest payloads gain `#[serde(default)]` promo fields. `{{promo_line}}` is empty when there's no promo. Both the hold-time and confirmation emails carry the discount, using after-promo figures.
  - **Files Touched:**
    - `common/src/email.rs`
    - `app_api/booking_api/src/apis/notifications.rs`
    - `app_api/booking_api/src/apis/bookings.rs`
    - `app_api/email_worker/src/processor.rs`
    - `app_api/email_worker/templates/booking_confirmation_guest.html`
  - **Acceptance Criteria:**
    - [ ] A promo booking's email shows the code and discount.
    - [ ] A no-promo email has no empty line or leftover placeholder.
    - [ ] Payloads queued before this release still render.
  - **Verification Command:** `cargo test -p email_worker promo_line && cargo test -p booking_api notifications`
  - **Dependencies:** T7

- [ ] **Task 20: Correct host email payouts**
  - **Scope:** S
  - **Description:** The confirmation and update payloads take `total_payout` and `currency` from the ledger row. If it's missing, log an error and send no payout figure. Add the promo line.
  - **Files Touched:**
    - `common/src/email.rs`
    - `app_api/booking_api/src/apis/notifications.rs`
    - `app_api/email_worker/src/processor.rs`
    - `app_api/email_worker/templates/booking_confirmation_host.html`
    - `app_api/email_worker/templates/booking_updated_host.html`
  - **Acceptance Criteria:**
    - [ ] `total_payout` equals the ledger net, in the ledger currency, with and without a promo.
    - [ ] A missing ledger row logs an error and shows no invented figure.
  - **Verification Command:** `cargo test -p booking_api notifications && cargo test -p email_worker host_payout`
  - **Dependencies:** T17

---

## Phase F: Management & Portals

- [ ] **Task 21: Add promo management queries**
  - **Scope:** S
  - **Description:** Add create (with selected listings), list with stats from one grouped query per page, get and deactivate. Map unique violations to `PromoCodeExists`.
  - **Files Touched:**
    - `db_core/src/promo.rs`
    - `db_core/src/promo_tests.rs`
    - `db_core/.sqlx/`
  - **Acceptance Criteria:**
    - [ ] A case-variant duplicate returns `PromoCodeExists`.
    - [ ] Listing filters by owner, and admin sees all.
    - [ ] Stats split active holds, confirmed uses and remaining uses correctly.
    - [ ] Deactivating twice returns `PromoAlreadyDeactivated`.
    - [ ] A used code can't be deleted (foreign key test).
  - **Verification Command:** `cargo test -p db_core promo_tests::management`
  - **Dependencies:** T4, T8

- [ ] **Task 22: Add the promo management API**
  - **Scope:** M
  - **Description:** Add `apis/promo_codes.rs` with list, create, get and deactivate under `/api/v1/admin/promo-codes`, using `require_staff` plus the spec §1.2 ownership rules. Add request validation for §1.1 and the DTOs.
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
  - **Verification Command:** `cargo test -p booking_api promo_codes && cargo test -p booking_api authz_tests`
  - **Dependencies:** T21

- [ ] **Task 23: Add client methods**
  - **Scope:** S
  - **Description:** Add `common/src/app_client/promo.rs` and `web_client` wrappers for both quotes and all management endpoints.
  - **Files Touched:**
    - `common/src/app_client/promo.rs`
    - `common/src/app_client/mod.rs`
    - `web_client/src/client.rs`
    - `web_client/src/api_client.rs`
  - **Acceptance Criteria:**
    - [ ] All new endpoints have typed client methods.
    - [ ] API error codes are preserved in the error text, so `PROMO_*` codes can be matched by the portals.
  - **Verification Command:** `cargo test -p web_client && SQLX_OFFLINE=true cargo check -p web_client --all-targets`
  - **Dependencies:** T12, T22

- [ ] **Task 24: Use one stay total on the listing page and checkout**
  - **Scope:** M
  - **Description:** `PriceBreakdown` gains `platform_fee`, `promo_code` and `promo_discount`. The listing page and checkout both use the API quote. Delete `stay_price`, `GCT_RATE`, the old `quote_for`, `common::app_client::get_pricing_quote` and its wrappers. Update `checkout-flow.spec.ts:35`.
  - **Files Touched:**
    - `web_ui_common/src/components/domain/price_breakdown.rs`
    - `web_ui_guest/src/quote.rs`
    - `web_ui_guest/src/pages/listing_detail.rs`
    - `web_ui_guest/src/pages/checkout.rs`
    - `common/src/app_client/listings.rs`, `web_client/src/client.rs`, `web_client/src/api_client.rs`
    - `playwright/e2e/guest-portal/checkout-flow.spec.ts`
  - **Acceptance Criteria:**
    - [ ] The listing page and checkout show the same total as the hold, including extra guests and a weekly discount.
    - [ ] The listing page works signed out.
    - [ ] Dates that can't be quoted show no price, with no fallback figure.
    - [ ] `rg 'stay_price|GCT_RATE|get_pricing_quote' web_ui_guest web_client common/src/app_client` finds nothing.
    - [ ] `checkout-flow.spec.ts:35` expects 4136.25 and passes, and `calendar.spec.ts` still passes.
  - **Verification Command:** `cargo test -p web_ui_common -p web_ui_guest && npm run test:e2e:guest`
  - **Dependencies:** T23

- [ ] **Task 25: Add promo entry to checkout**
  - **Scope:** M
  - **Description:** Add the code field and "Apply" button (`name="intent" value="apply_promo" formnovalidate`) inside the existing form, before "Confirm reservation", plus "Remove". Re-render without placing a booking. `place_booking` sends the code and maps `PROMO_*` errors to `PromoRefusal::message()`.
  - **Files Touched:**
    - `web_ui_guest/src/pages/checkout.rs`
    - `web_ui_common/src/components/domain/price_breakdown.rs`
    - `playwright/e2e/guest-portal/checkout-flow.spec.ts`
  - **Acceptance Criteria:**
    - [ ] Applying a valid code shows the discount and the new total, and confirming stores it.
    - [ ] An expired code shows its reason and the full price.
    - [ ] Details the guest entered are kept after applying.
    - [ ] Pressing Enter applies the code and doesn't place a booking.
    - [ ] A code invalidated before confirming shows the reason and keeps the code entered.
    - [ ] It works with JavaScript disabled.
  - **Verification Command:** `cargo test -p web_ui_guest checkout && npm run test:e2e:guest -- checkout-flow`
  - **Dependencies:** T24

- [ ] **Task 26: Warn about a lost discount on the change page**
  - **Scope:** XS
  - **Description:** `booking_change.rs` shows a warning, with the reason, when `promo_outcome` is lost.
  - **Files Touched:**
    - `web_ui_guest/src/pages/booking_change.rs`
  - **Acceptance Criteria:**
    - [ ] The warning appears before the guest accepts.
    - [ ] It doesn't appear for none or kept.
  - **Verification Command:** `cargo test -p web_ui_guest booking_change`
  - **Dependencies:** T15, T23

- [ ] **Task 27: Add the admin promo page**
  - **Scope:** M
  - **Description:** Add `pages/promo_codes.rs`, following `currencies.rs`: list with stats, create form, deactivate, and nav links. Dates are Jamaica calendar days, sent as the start of `valid_from` and the end of `valid_to`.
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
  - **Verification Command:** `cargo test -p web_ui_admin promo_codes && npm run test:e2e:admin -- promo-codes`
  - **Dependencies:** T23

- [ ] **Task 28: Show promos on the ledger and booking detail pages**
  - **Scope:** S
  - **Description:** Add the cleaning fee, code and discount to `payouts.rs`, showing accommodation as `gross − cleaning_fee_amount`. Add the promo line to the guest and admin booking detail pages.
  - **Files Touched:**
    - `web_ui_admin/src/pages/payouts.rs`
    - `web_ui_admin/src/pages/bookings/detail.rs`
    - `web_ui_guest/src/pages/bookings.rs`
  - **Acceptance Criteria:**
    - [ ] A promo booking shows the code and discount on all three pages.
    - [ ] A host can see the discount from an admin-created code.
    - [ ] Pages without a promo are unchanged.
  - **Verification Command:** `cargo test -p web_ui_admin payouts && cargo test -p web_ui_admin bookings && cargo test -p web_ui_guest bookings`
  - **Dependencies:** T17, T18

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
  - **Verification Command:** `rg -c 'promo-codes|bookings/quote|/quote' openapi.yaml openapi-prod.yaml && cargo test -p booking_api openapi_includes_promo`
  - **Dependencies:** T12, T22

- [ ] **Task 30: Run the final gates**
  - **Scope:** S
  - **Description:** Run every `CONSTRAINTS.md` gate and the full suites, then check each spec §7 acceptance criterion against a test. Run `/audit-booking-flow`.
  - **Files Touched:** None expected. Fixes go in the task that caused the failure.
  - **Acceptance Criteria:**
    - [ ] Every §7 acceptance criterion maps to a passing test.
    - [ ] All gates pass, with no new `#[allow]`, `#[ignore]` or `.skip`.
    - [ ] `/audit-booking-flow` reports no unresolved finding.
  - **Verification Command:** `cargo fmt --check && SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features -- -D warnings && SQLX_OFFLINE=true cargo clippy --workspace --all-features -- -D missing_docs && devenv shell -- cargo sqlx prepare --workspace --check && cargo test --workspace --exclude web_client --exclude web_ui_common --exclude web_ui_guest --exclude web_ui_admin --exclude protoproj && cargo test -p web_client -p web_ui_common -p web_ui_guest -p web_ui_admin && ! rg -q "f(32|64)" common/src/pricing.rs common/src/promo.rs db_core/src/promo.rs app_api/booking_api/ && cargo audit && npm run test:e2e`
  - **Baseline note:** the `CONSTRAINTS.md` `.unwrap()`/`.expect()` grep already matches 40 files at `e2f6e2d0`, including test modules, so it can't pass as a whole-repo check here. Instead, review `git diff e2f6e2d0` and confirm no new `.unwrap()` or `.expect()` outside test modules. If `-D missing_docs` also fails on code this branch didn't touch, confirm that none of the failures are in new items, and report the rest instead of fixing them here.
  - **Dependencies:** All
