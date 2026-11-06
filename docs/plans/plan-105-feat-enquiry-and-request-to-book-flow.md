# Implementation Plan: Enquiry and Request-to-Book Flow (#105)

**Spec Reference:** [`docs/specs/spec-105-feat-enquiry-and-request-to-book-flow.md`](../specs/spec-105-feat-enquiry-and-request-to-book-flow.md)
**Intent:** [`docs/intent/intent-105-feat-enquiry-and-request-to-book-flow.md`](../intent/intent-105-feat-enquiry-and-request-to-book-flow.md)
**Baseline Commit:** `e0f5c6bb`
**Branch:** `feat/105-enquiry-and-request-to-book-flow`

## Cross-Cutting Rules (apply to every task)
- **Local database**: Postgres runs in the devenv shell, not Docker. Run `sqlx`, `psql` and DB tests through `devenv shell -- <command>`.
- **SQLx metadata**: after changing a query, run `devenv shell -- cargo sqlx prepare --workspace -- --all-targets` and commit the root `.sqlx/` changes with that task.
- **Task gates**: `cargo fmt --check`, `SQLX_OFFLINE=true cargo clippy -p <touched crates> --all-targets --all-features -- -D warnings`, and `SQLX_OFFLINE=true cargo check --workspace --exclude protoproj --all-features --tests`. Every new public item has a `///` comment.
- **Tests come first**: write the failing test, then the code. Each task creates the named test module its command filters on (for example `mod request_create_tests`). A filter that matches 0 tests, or only older tests, is not green.
- **Targeted tests while iterating**: run only the task's command. The full suite runs once, in T42.
- **Concurrency tests** use real connections (`sqlx_db_tester::TestPg`) and `#[tokio::test(flavor = "multi_thread")]`, with no sleeps. Expiry is simulated by writing `expires_at` in the past, not by waiting.
- **Lock order** (spec §3.3): listing → booking_request → currency → the guest's pending bookings → promo code. Never take an earlier lock after a later one.
- **Live-request predicate**: copied verbatim into each SQL site (`status IN ('open','accepted','quoted') AND expires_at > NOW()` + gap widening). Every copy is covered by `availability_parity_tests`.
- **Clock**: state-changing actions check expiry with `clock_timestamp()`; read queries use `NOW()`.
- **Money**: `Decimal` only, including form parsing (`Decimal::from_str`). `round_dp(2)` only. No fallback rates or amounts.
- **Logs**: message text, quote terms, decline notes and promo code strings never enter a log or span. Instrumented functions `skip` them and log lengths or ids.
- **Error mapping**: every new `RequestRefusal` variant has a `code()`, an HTTP status, and arms in both `status_code` and `error_response`.
- **Instant-only behaviour**: listings left on `instant` (the default) must behave exactly as they do today, apart from live requests blocking dates.
- **Existing tests**: update them only for changes the spec calls for, without weakening an assertion.
- **E2E runs**: restart the APIs and portals from this worktree first (`apis-stop`, `frontends-stop`, `apis-start`, `frontends`), then run `devenv shell -- test-e2e-guest` or `test-e2e-admin`.

## Dependency Graph
Each line is `task ← dependencies`, matching every task's **Dependencies** field.

```text
Phase A: foundations
T1  common request domain types      ← —
T2  calculator custom stay price     ← —
T3  StayPriceSnapshot                ← T2
T4  migration                        ← —
T5  db_core models + DbError         ← T1, T4
T6  API error mapping                ← T5
T7  request config settings          ← —

Phase B: booking mode (vertical slice)
T8  listing booking_mode read/write  ← T1, T4
T9  instant hold mode enforcement    ← T6, T8

Phase C: requests block dates (highest risk)
T10 db create_request                ← T3, T5
T11 availability includes requests   ← T10
T12 instant hold vs live requests    ← T9, T10
T13 promo usage counts requests      ← T10
T14 price_stay rate source + override← T2
T15 API create/read requests         ← T6, T7, T11, T12, T13, T14

Phase D: responses and conversion
T16 db accept/decline/withdraw       ← T10
T17 API accept/decline/withdraw      ← T15, T16
T18 db convert offer to hold         ← T13, T16
T19 API checkout endpoint            ← T17, T18

Phase E: threads, enquiries, quotes
T20 db conversations                 ← T5
T21 API conversations + messages     ← T6, T20
T22 db enquiries                     ← T20
T23 API enquiries                    ← T15, T21, T22
T24 db quotes                        ← T14, T16, T22
T25 API quotes + preview             ← T17, T23, T24

Phase F: expiry and email
T26 expiry sweeps                    ← T16, T22
T27 email templates                  ← T1
T28 request notifications wiring     ← T19, T23, T25, T26, T27
T29 host near-expiry reminder        ← T26, T27

Phase G: inbox
T30 inbox + response metrics         ← T20, T25

Phase H: clients and portals
T31 app_client methods               ← T19, T21, T25, T30
T32 admin listing booking mode       ← T8, T31
T33 guest listing CTAs + checkout    ← T31
T34 guest enquiry pages              ← T31, T33
T35 guest My requests + detail       ← T31, T34
T36 admin inbox page                 ← T31
T37 admin request detail + actions   ← T36
T38 admin enquiry detail             ← T37
T39 Playwright guest flows           ← T32, T33, T34, T35, T37
T40 Playwright admin flows           ← T32, T36, T37, T38

Phase I: finish
T41 docs + API contracts             ← T29, T30
T42 final gates                      ← all
```

## Spec Coverage
| Spec area | Tasks |
|---|---|
| §1.1 booking mode | T1, T8, T9, T32, T33 |
| §1.2 enquiry | T22, T23, T34, T38 |
| §1.3 request to book | T10, T11, T13, T15, T33 |
| §1.4 host response | T16, T17, T24, T25, T37 |
| §1.5 custom quote pricing | T2, T14, T24, T25 |
| §1.6 conversion | T3, T18, T19, T35 |
| §1.7 withdrawal, supersession, expiry | T12, T16, T26, T29 |
| §1.8 conversation | T4, T20, T21 |
| §1.9 notifications | T27, T28, T29 |
| §1.10 inbox | T30, T36 |
| §2 edge cases E1–E24 | T10–T12, T16, T18, T24, T26 (named in acceptance criteria) |
| §3.4 config | T7 |
| AC 19 docs | T41 |

---

## Phase A: Foundations

- [x] **Task 1: Request domain types in `common`**
  - **Scope:** S
  - **Description:** Add `common/src/booking_request.rs` with `BookingMode`, `RequestStatus` (`is_live`, `can_transition_to`), `EnquiryStatus`, `EnquiryClosedReason`, `DeclineReason` (`requires_note`), `RequestRefusal` (`code`, `message`, `http_status`), `response_window` (D4 cap at check-in 00:00 Jamaica = 05:00 UTC, 1h minimum) and `is_near_expiry`. Pure, with no I/O.
  - **Files Touched:**
    - `common/src/booking_request.rs`
    - `common/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] An exhaustive (from, to) transition table: terminal states have no outgoing edges, and `quoted → quoted` is allowed.
    - [ ] `BookingMode` allows the right combinations for all three modes.
    - [ ] `response_window`: the normal window, the cap at check-in, a capped window under 1h → `RequestWindowTooShort`, and a past check-in.
    - [ ] `is_near_expiry`: inclusive 25% boundary, 1% and 99%, and false after expiry.
    - [ ] `RequestRefusal` codes are unique, and each maps to 400, 403, 409 or 422 as in spec §3.4.
    - [ ] `DeclineReason::Other` requires a note.
  - **Verification Command:** `cargo test -p common booking_request_tests`
  - **Dependencies:** None

- [x] **Task 2: `BookingCalculator::with_custom_stay_price`**
  - **Scope:** S
  - **Description:** Add `impl BookingCalculator<Discounted> { pub fn with_custom_stay_price(self, stay_total) -> Self }` as in spec §3.2: it replaces the subtotal, sets `discount_value = None`, zeroes the extra-guest fee, and sets the display `actual_daily_rate = round(total / days, 2)`. Add `custom_stay_total` (base → payment currency) and golden cases in a separate `common/testdata/custom_quote_golden.json`, because `stay_pricing_golden.json` is also replayed by listing search, which can't apply a custom price.
  - **Files Touched:**
    - `common/src/pricing.rs`
    - `common/testdata/custom_quote_golden.json`
  - **Acceptance Criteria:**
    - [ ] Weekly or monthly discounts and extra-guest fees are discarded and replaced by the custom total.
    - [ ] Cleaning fee and deposit are untouched; GCT 15% and the 5% platform fee are recalculated from the custom total.
    - [ ] `host_gross` on the result equals `round((custom + cleaning) × rate)`.
    - [ ] The custom-quote golden cases pass in `common` (T14 replays them through `price_stay`).
  - **Verification Command:** `cargo test -p common custom_stay_price_tests`
  - **Dependencies:** None

- [x] **Task 3: `StayPriceSnapshot`**
  - **Scope:** S
  - **Description:** Add a serde struct holding every money field `NewBooking` needs, plus currency, exchange rate, cancellation policy and the optional promo `{id, terms, stay_discount, cleaning_discount}`. `from_calculator` is the only constructor. Add `to_new_booking` (in db_core models, T5) later.
  - **Files Touched:**
    - `common/src/booking_request.rs`
  - **Acceptance Criteria:**
    - [ ] A JSON round trip preserves every Decimal's value and scale exactly.
    - [ ] Built from a `Taxed` calculator, it matches the calculator's fields one for one.
    - [ ] Snapshotting a calculator with no promo gives `promo: None`.
  - **Verification Command:** `cargo test -p common price_snapshot_tests`
  - **Dependencies:** Task 2

- [ ] **Task 4: Migration**
  - **Scope:** M
  - **Description:** `devenv shell -- sqlx migrate add enquiries_and_booking_requests` with the full schema from spec §3.1: `booking_mode`, `conversation`, `booking_enquiry`, `booking_request`, `booking_request_quote`, `booking_request_history`, `request_notification_log`, the `booking_message` re-anchoring with backfill, and every index. No existing migration is edited.
  - **Files Touched:**
    - `db_core/migrations/<ts>_enquiries_and_booking_requests.sql`
    - `db_core/src/migration_tests.rs` (new module, or the existing migration test file if present)
  - **Acceptance Criteria:**
    - [ ] It applies cleanly on a fresh DB and on a DB seeded with bookings and messages (`devenv shell -- sqlx migrate run`).
    - [ ] Backfill: messages on two bookings of one guest × listing share one conversation; different guests get different conversations; `conversation_id` is `NOT NULL` afterwards.
    - [ ] Existing listings have `booking_mode = 'instant'`.
    - [ ] The partial unique index refuses a second live request for the same pair, but allows one after the first becomes `expired`.
    - [ ] `booking_request_history` refuses a row with neither or both subject ids.
  - **Verification Command:** `devenv shell -- cargo test -p db_core request_migration_tests`
  - **Dependencies:** None

- [ ] **Task 5: db_core models and `DbError::RequestRefused`**
  - **Scope:** S
  - **Description:** `sqlx::Type` enums mirroring T1, `DbBookingRequest`, `DbBookingEnquiry`, `DbRequestQuote`, `DbConversation`, `From` impls into the `common` DTOs, and `StayPriceSnapshot → NewBooking`. Add `DbError::RequestRefused(RequestRefusal)`. `DbBookingMessage` gains `conversation_id`, `enquiry_id`, `request_id`, and `booking_id: Option<Uuid>`; existing call sites compile.
  - **Files Touched:**
    - `db_core/src/models.rs`
    - `db_core/src/error.rs`
    - `db_core/src/booking_message.rs`
  - **Acceptance Criteria:**
    - [ ] Every enum round-trips through Postgres (insert and select a row per variant).
    - [ ] A snapshot → `NewBooking` conversion preserves every money field.
    - [ ] Existing booking-message tests still pass.
  - **Verification Command:** `devenv shell -- cargo test -p db_core request_model_tests`
  - **Dependencies:** Task 1, Task 4

- [ ] **Task 6: API error mapping for request refusals**
  - **Scope:** XS
  - **Description:** Add `status_code` and `error_response` arms for `DbError::RequestRefused(r)` → `{code, message}` with `r.http_status()`, plus a `warn!` with the code.
  - **Files Touched:**
    - `app_api/api_core/src/error.rs`
  - **Acceptance Criteria:**
    - [ ] Each variant gives its status code and body code (mirrors `every_refusal_is_a_422_with_its_code_and_message`).
    - [ ] `status_code` and `error_response` agree for every variant.
  - **Verification Command:** `cargo test -p api_core request_refusal_mapping`
  - **Dependencies:** Task 5

- [ ] **Task 7: Request window settings**
  - **Scope:** XS
  - **Description:** Add `request_response_hours` (24), `quote_valid_hours` (48) and `request_near_expiry_pct` (25) to `ApplicationSettings` with serde defaults and to `Default.toml` with comments. Validate the ranges in `main.rs` next to `pending_hold_minutes` (1–720, 1–720, 1–99).
  - **Files Touched:**
    - `app_api/api_core/src/settings.rs`
    - `app_api/booking_api/config/Default.toml`
    - `app_api/booking_api/src/main.rs`
    - `app_api/listing_api/src/apis_test.rs` (struct literal)
  - **Acceptance Criteria:**
    - [ ] Defaults load when the keys are absent.
    - [ ] `EA__APPLICATION__REQUEST_RESPONSE_HOURS=6` overrides the window.
    - [ ] 0, negative and out-of-range values fail validation with a message naming the key.
  - **Verification Command:** `cargo test -p api_core request_settings_tests && cargo test -p booking_api request_settings_tests`
  - **Dependencies:** None

## Phase B: Booking Mode

- [ ] **Task 8: Listing `booking_mode` read and write**
  - **Scope:** M
  - **Description:** `common::models::Listing` and its create and update DTOs gain `booking_mode` (`#[serde(default)]`). The listing write and read queries and the `listing_history` snapshot include it. `listing_api` handlers pass it through.
  - **Files Touched:**
    - `common/src/models.rs`
    - `db_core/src/listing/write.rs`
    - `db_core/src/listing/queries.rs`
    - `app_api/listing_api/src/` (handlers + tests)
  - **Acceptance Criteria:**
    - [ ] Create with no mode → `instant`; update to `both` → GET returns `both`; history row records it.
    - [ ] Public and admin listing DTOs expose the mode.
    - [ ] An unknown mode string → 400.
  - **Verification Command:** `devenv shell -- cargo test -p db_core listing_booking_mode_tests && devenv shell -- cargo test -p listing_api booking_mode`
  - **Dependencies:** Task 1, Task 4

- [ ] **Task 9: Enforce booking mode on instant holds**
  - **Scope:** S
  - **Description:** `create_booking` reads `booking_mode` with `is_active` under the listing lock and refuses `request` listings with `RequestRefusal::BookingModeNotAllowed`.
  - **Files Touched:**
    - `db_core/src/booking/hold.rs`
    - `db_core/src/booking/tests.rs`
    - `app_api/booking_api/src/apis/tests.rs`
  - **Acceptance Criteria:**
    - [ ] `POST /api/v1/bookings` on a `request` listing → 409 `BOOKING_MODE_NOT_ALLOWED`, with no booking row.
    - [ ] `instant` and `both` listings book as before.
    - [ ] A mode change committed while a hold waits on the lock is seen by that hold.
  - **Verification Command:** `devenv shell -- cargo test -p db_core booking_mode_hold_tests && devenv shell -- cargo test -p booking_api booking_mode_hold`
  - **Dependencies:** Task 6, Task 8

## Phase C: Requests Block Dates (highest risk)

- [ ] **Task 10: `db_core::booking_request::create_request`**
  - **Scope:** M
  - **Description:** One transaction (spec §1.3.3): listing lock and mode check; inline expiry of this guest's lapsed rows on the listing; `REQUEST_ALREADY_LIVE`; availability against confirmed bookings, other guests' live holds and live requests (with the gap); supersession of the guest's own live hold on the listing; insert; history; close the source enquiry; optional message (stored once T20 lands; until then the column is left out). Promo usage arrives in T13. Unique-violation mapping goes to `RequestAlreadyLive`.
  - **Files Touched:**
    - `db_core/src/booking_request/mod.rs`
    - `db_core/src/booking_request/request.rs`
    - `db_core/src/booking_request/history.rs`
    - `db_core/src/booking_request/request_tests.rs`
    - `db_core/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] E1 (request part): two guests requesting the same dates concurrently → exactly one success; the other gets `DateUnavailable`.
    - [ ] E2: the same guest twice concurrently → one success, one `RequestAlreadyLive`.
    - [ ] A lapsed-but-unswept request by the same guest is expired inline, and a new request succeeds.
    - [ ] E16: the guest's own pending hold on the listing is cancelled, with a `booking_history` row.
    - [ ] An `instant` listing → `BookingModeNotAllowed`; an inactive listing → `ListingUnavailable`.
    - [ ] One `booking_request_history` row (`NULL → open`, with the actor).
  - **Verification Command:** `devenv shell -- cargo test -p db_core request_create_tests`
  - **Dependencies:** Task 3, Task 5

- [ ] **Task 11: Availability queries include live requests**
  - **Scope:** M
  - **Description:** Add the live-request predicate to `check_availability`, `check_availability_excluding`, `get_unavailable_ranges` (`UNION ALL`) and `push_stay_conditions`. Add `check_availability_excluding_request`. Extend `availability_parity_tests` with request fixtures.
  - **Files Touched:**
    - `db_core/src/booking/availability.rs`
    - `db_core/src/listing/queries.rs`
    - `db_core/src/booking/availability_parity_tests.rs`
    - `db_core/src/booking/unavailable_ranges_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Live `open`, `accepted` and `quoted` requests block in all four queries, including gap nights (E23).
    - [ ] `declined`, `expired`, `withdrawn`, `cancelled` and `converted` don't block.
    - [ ] Expiry on read: `expires_at` in the past with no sweep → free in all four at once.
    - [ ] E24: a reservation change into request-held dates → `DateUnavailable`.
    - [ ] The parity tests agree across every copy.
  - **Verification Command:** `devenv shell -- cargo test -p db_core availability_parity && devenv shell -- cargo test -p db_core request_availability_tests`
  - **Dependencies:** Task 10

- [ ] **Task 12: Instant holds respect and supersede requests**
  - **Scope:** S
  - **Description:** The `create_booking` overlap check adds other guests' live requests. The same guest's live request on the listing is set to `cancelled` (`superseded_by_instant_booking`), and their open enquiry to `closed/booked`, both with history in the same transaction. The function returns the superseded ids for T28's emails.
  - **Files Touched:**
    - `db_core/src/booking/hold.rs`
    - `db_core/src/booking/hold_request_tests.rs`
    - `app_api/booking_api/src/apis/bookings.rs`
  - **Acceptance Criteria:**
    - [ ] E1: an instant hold and a request racing for the same dates → exactly one success.
    - [ ] Another guest's live request → `DateUnavailable`.
    - [ ] The same guest's request is cancelled and the hold created; E17: their request on another listing is untouched.
  - **Verification Command:** `devenv shell -- cargo test -p db_core hold_request_tests`
  - **Dependencies:** Task 9, Task 10

- [ ] **Task 13: Promo usage counts live requests (D2)**
  - **Scope:** S
  - **Description:** In `lock_and_count`, add live `booking_request` rows (`open`/`accepted`, `expires_at > NOW()`) to the total and per-guest counts, as a separate statement after the lock. `create_request` takes `lock_and_count` → `check_status` → `check_usage` when a code is present.
  - **Files Touched:**
    - `db_core/src/promo.rs`
    - `db_core/src/booking_request/request.rs`
    - `db_core/src/promo_tests.rs`
  - **Acceptance Criteria:**
    - [ ] With a limit-1 code, a live request makes another guest's hold refuse with `PROMO_USAGE_EXHAUSTED` (or the matching #106 code).
    - [ ] Withdrawing or expiring the request (without a sweep) frees the use.
    - [ ] The per-guest limit counts the guest's requests.
    - [ ] A `quoted` request no longer counts.
  - **Verification Command:** `devenv shell -- cargo test -p db_core promo_request_usage_tests`
  - **Dependencies:** Task 10

- [ ] **Task 14: `price_stay` rate source and stay override**
  - **Scope:** S
  - **Description:** `StayRequest` gains `rate: RateSource { Current, Locked(Decimal) }` and `stay_override: Option<Decimal>` (base currency). A locked rate skips the FX lookup. An override converts at the rate, calls `with_custom_stay_price`, forces `promo = None`, and checks the quote floor (`> 0` after conversion and rounding, `check_amounts_fit`) → `InvalidQuotePrice`.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/stay_pricing.rs`
    - `app_api/booking_api/src/apis/stay_pricing_pin_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Existing callers (`Current`, no override) price identically; the golden tests are unchanged.
    - [ ] A locked rate is used even after a newer rate version is published.
    - [ ] Override values of 0, negative, 3 dp and too large are each refused with 400 `INVALID_QUOTE_PRICE`.
    - [ ] An override with a promo code in the request ignores the code.
  - **Verification Command:** `devenv shell -- cargo test -p booking_api stay_override_tests`
  - **Dependencies:** Task 2

- [ ] **Task 15: API create and read requests**
  - **Scope:** M
  - **Description:** `apis/booking_requests.rs`: `POST /api/v1/booking-requests` (price → snapshot → `create_request` with `response_window`), `GET /{id}`, and `GET /mine`. Add `authorize_request_party` (guest, host or admin, otherwise 403) and the owner-on-own-listing refusal (E19). Wire `routes.rs` and the utoipa annotations.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/booking_requests.rs`
    - `app_api/booking_api/src/apis/mod.rs`
    - `app_api/booking_api/src/apis/routes.rs`
    - `app_api/booking_api/src/apis/request_api_tests.rs`
  - **Acceptance Criteria:**
    - [ ] 201 with the snapshot breakdown; dates are then unavailable through `/bookings/listing/{id}/unavailable-ranges`.
    - [ ] Each error in spec §3.4 for this row is returned with its code, including 422 `REQUEST_WINDOW_TOO_SHORT` for a check-in tomorrow at 00:30 Jamaica.
    - [ ] A host GETting another host's request → 403; a guest GETting another guest's → 403.
    - [ ] `/mine` returns only the caller's rows.
    - [ ] Message and promo strings are absent from captured logs.
  - **Verification Command:** `devenv shell -- cargo test -p booking_api request_api_tests`
  - **Dependencies:** Task 6, Task 7, Task 11, Task 12, Task 13, Task 14

## Phase D: Responses and Conversion

- [ ] **Task 16: db accept, decline and withdraw**
  - **Scope:** M
  - **Description:** Each takes listing → request locks, checks `can_transition_to`, checks `expires_at > clock_timestamp()` for accept, and caps the new `expires_at` (`quote_valid_hours`) at check-in. They set `first_response_at` (host actions), write history with the actor, and return the notification facts in one round trip.
  - **Files Touched:**
    - `db_core/src/booking_request/respond.rs`
    - `db_core/src/booking_request/respond_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Accepting leaves the dates blocked and creates **no** booking row.
    - [ ] E3: accept vs withdraw concurrently → one wins; the other gets `RequestNotOpen`.
    - [ ] E4: accepting after `expires_at` (no sweep) → `RequestExpired`.
    - [ ] D6: decline works from `open`, `accepted` and `quoted`, and frees the dates; `Other` without a note → `DeclineNoteRequired`.
    - [ ] Each transition writes exactly one history row.
  - **Verification Command:** `devenv shell -- cargo test -p db_core request_respond_tests`
  - **Dependencies:** Task 10

- [ ] **Task 17: API accept, decline and withdraw**
  - **Scope:** S
  - **Description:** `POST /{id}/accept`, `/decline` and `/withdraw`, with role checks (host or admin for accept and decline, guest for withdraw) and an `info!` log with `actor_id`, `request_id` and the action.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/booking_requests.rs`
    - `app_api/booking_api/src/apis/routes.rs`
    - `app_api/booking_api/src/apis/request_api_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Another host → 403 on accept and decline; a guest can't accept; a host can't withdraw; an admin can act on any listing.
    - [ ] E10: accept on a deactivated listing → 409 `LISTING_UNAVAILABLE`; decline and withdraw still work.
  - **Verification Command:** `devenv shell -- cargo test -p booking_api request_respond_api`
  - **Dependencies:** Task 15, Task 16

- [ ] **Task 18: `convert_offer_to_hold`**
  - **Scope:** M
  - **Description:** One transaction (spec §1.6.2). Locks listing → request (status, owner, `clock_timestamp()` expiry, current `quote_id`) → currency. Re-checks availability excluding this request, cancels the guest's other pending holds, then for the promo: `lock_code` + redemption insert **without** `check_usage`. Inserts the booking from the snapshot (with `pending_hold_minutes`), writes both histories, and sets the request to `converted` with `booking_id`.
  - **Files Touched:**
    - `db_core/src/booking_request/convert.rs`
    - `db_core/src/booking/hold.rs` (shared insert helper)
    - `db_core/src/booking_request/convert_tests.rs`
  - **Acceptance Criteria:**
    - [ ] The booking's money fields equal the snapshot exactly, even after a rate, price or override change (E13).
    - [ ] No release gap: a rival hold racing the conversion waits on the lock, then gets `DateUnavailable`.
    - [ ] E4/E6: an expired offer → `RequestExpired` / `QuoteExpired`; a stale `quote_id` → `QuoteSuperseded`; no booking row in either case.
    - [ ] E14: the code is deactivated and its limit reached after submission, and conversion still redeems at snapshot terms.
    - [ ] E12: an inactive currency → `CurrencyInactive`.
    - [ ] E15: the converted hold expires normally and the request stays `converted`.
  - **Verification Command:** `devenv shell -- cargo test -p db_core request_convert_tests`
  - **Dependencies:** Task 13, Task 16

- [ ] **Task 19: API checkout endpoint**
  - **Scope:** S
  - **Description:** `POST /api/v1/booking-requests/{id}/checkout` `{quote_id}` → 201 `BookingResponse`. Only the guest may call it. A confirmation code is generated with the same retry loop as `create_booking`.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/booking_requests.rs`
    - `app_api/booking_api/src/apis/routes.rs`
    - `app_api/booking_api/src/apis/request_api_tests.rs`
  - **Acceptance Criteria:**
    - [ ] The happy path returns a `pending` booking whose `expires_at` ≈ now + `pending_hold_minutes`.
    - [ ] A host calling it → 403; each refusal returns its typed code.
    - [ ] The resulting booking confirms through the existing status endpoint.
  - **Verification Command:** `devenv shell -- cargo test -p booking_api request_checkout_api`
  - **Dependencies:** Task 17, Task 18

## Phase E: Threads, Enquiries and Quotes

- [ ] **Task 20: db conversations**
  - **Scope:** M
  - **Description:** `db_core/src/conversation.rs`: `upsert`, `get_with_parties`, `list_messages`, `insert_message` (sets `last_message_at`, and `first_response_at` on the pair's open enquiry or live request when a host or admin posts) and `mark_read`. `booking_message.rs` reads and writes by the booking's conversation. `transfer_booking_guest` re-anchors the messages (E18). Wire `create_request`'s optional message.
  - **Files Touched:**
    - `db_core/src/conversation.rs`
    - `db_core/src/booking_message.rs`
    - `db_core/src/booking/hold.rs`
    - `db_core/src/conversation_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Messages posted on an enquiry, a request and the booking all appear in the booking's list, in order.
    - [ ] Only the first host or admin post sets `first_response_at`; a guest post never does.
    - [ ] A shadow-user transfer moves the booking's messages into the new guest's conversation.
  - **Verification Command:** `devenv shell -- cargo test -p db_core conversation_tests`
  - **Dependencies:** Task 5

- [ ] **Task 21: API conversations and booking messages**
  - **Scope:** S
  - **Description:** `GET/POST /api/v1/conversations/{id}/messages` and `PATCH …/read`, limited to the parties and admins. `messages.rs` goes through the conversation with an unchanged contract; the cancelled-booking rule stays on the booking endpoint.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/conversations.rs`
    - `app_api/booking_api/src/apis/messages.rs`
    - `app_api/booking_api/src/apis/routes.rs`
    - `app_api/booking_api/src/apis/conversation_api_tests.rs`
  - **Acceptance Criteria:**
    - [ ] The existing booking-message API tests pass unchanged.
    - [ ] Another host → 403; E20: control characters or an empty message → 400.
    - [ ] A thread started on an enquiry is visible at `/bookings/{id}/messages` after conversion.
  - **Verification Command:** `devenv shell -- cargo test -p booking_api conversation_api_tests`
  - **Dependencies:** Task 6, Task 20

- [ ] **Task 22: db enquiries**
  - **Scope:** S
  - **Description:** `create_enquiry` (upserts the conversation, posts the message, supersedes the previous open enquiry, history) and `close_enquiry` (`closed_by_host`). No availability check.
  - **Files Touched:**
    - `db_core/src/booking_request/enquiry.rs`
    - `db_core/src/booking_request/enquiry_tests.rs`
  - **Acceptance Criteria:**
    - [ ] An enquiry on fully booked dates succeeds and blocks nothing.
    - [ ] A second enquiry closes the first with `superseded`.
    - [ ] History rows for create, supersede and close.
  - **Verification Command:** `devenv shell -- cargo test -p db_core enquiry_tests`
  - **Dependencies:** Task 20

- [ ] **Task 23: API enquiries**
  - **Scope:** S
  - **Description:** `POST /api/v1/enquiries` (stay-input and guest-count validation, the inactive-listing check, E19), `GET /{id}`, `POST /{id}/close`. `/booking-requests/mine` includes enquiries. `POST /booking-requests` with `enquiry_id` validates ownership and the listing match.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/enquiries.rs`
    - `app_api/booking_api/src/apis/booking_requests.rs`
    - `app_api/booking_api/src/apis/routes.rs`
    - `app_api/booking_api/src/apis/enquiry_api_tests.rs`
  - **Acceptance Criteria:**
    - [ ] A host gets 403 on another host's enquiry.
    - [ ] A request with someone else's `enquiry_id` → 403; on success the enquiry closes with `request_submitted`.
  - **Verification Command:** `devenv shell -- cargo test -p booking_api enquiry_api_tests`
  - **Dependencies:** Task 15, Task 21, Task 22

- [ ] **Task 24: db quotes**
  - **Scope:** M
  - **Description:** `send_quote` on a request (`open|quoted → quoted`; supersedes the previous quote; sets `current_quote_id`, the capped `expires_at` and `first_response_at`; the promo is dropped by status). `quote_enquiry` takes the listing lock, checks availability and `REQUEST_ALREADY_LIVE`, inserts a `quoted` request (`origin = enquiry_quote`), and closes the enquiry with `quoted`. Both write history with `quote_id`.
  - **Files Touched:**
    - `db_core/src/booking_request/quote.rs`
    - `db_core/src/booking_request/quote_tests.rs`
  - **Acceptance Criteria:**
    - [ ] E7: quoting an enquiry whose dates were taken → `DateUnavailable`, and the enquiry stays open.
    - [ ] E8: the guest already has a live request → `RequestAlreadyLive`.
    - [ ] A replacement quote marks the old one `superseded_at`; only one current quote.
    - [ ] A quote frees the request's reserved promo use (usage count drops).
  - **Verification Command:** `devenv shell -- cargo test -p db_core request_quote_tests`
  - **Dependencies:** Task 14, Task 16, Task 22

- [ ] **Task 25: API quotes and preview**
  - **Scope:** M
  - **Description:** `POST /booking-requests/{id}/quote-preview` (prices only, saves nothing), `POST /booking-requests/{id}/quote` (a locked rate from the snapshot), and `POST /enquiries/{id}/quote` (a `Current` rate, then snapshotted). The body's `stay_price` is a Decimal string. The response includes the promo-dropped flag, never the code.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/booking_requests.rs`
    - `app_api/booking_api/src/apis/enquiries.rs`
    - `app_api/booking_api/src/apis/routes.rs`
    - `app_api/booking_api/src/apis/quote_api_tests.rs`
  - **Acceptance Criteria:**
    - [ ] The preview and the sent quote give identical totals for the same input.
    - [ ] A JSON number for `stay_price` is refused; only a string parses (no float path).
    - [ ] Another host → 403 on preview and on quote; an expired request → 409 `REQUEST_EXPIRED`.
    - [ ] The quote's GCT and platform fee match a hand-computed fixture.
  - **Verification Command:** `devenv shell -- cargo test -p booking_api quote_api_tests`
  - **Dependencies:** Task 17, Task 23, Task 24

## Phase F: Expiry and Email

- [ ] **Task 26: Expiry sweeps**
  - **Scope:** S
  - **Description:** `expire_stale_requests` (`FOR UPDATE SKIP LOCKED`, history, returns facts) and `close_past_enquiries(today_jamaica)`. Call both from `run_sweeps`; the response gains `requests_expired` and `enquiries_closed`.
  - **Files Touched:**
    - `db_core/src/booking_request/expiry.rs`
    - `app_api/booking_api/src/apis/cron.rs`
    - `db_core/src/booking_request/expiry_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Only lapsed live rows expire; terminal rows are untouched.
    - [ ] E22: a second run expires nothing.
    - [ ] E5: a row locked by an in-flight tx is skipped, then expired next run.
    - [ ] Enquiries with `date_from <= today` close as `expired`.
  - **Verification Command:** `devenv shell -- cargo test -p db_core request_expiry_tests && devenv shell -- cargo test -p booking_api run_sweeps`
  - **Dependencies:** Task 16, Task 22

- [ ] **Task 27: Email templates**
  - **Scope:** M
  - **Description:** Add the `EmailTemplate` variants and HTML templates for every row of spec §1.9, rendered through `render_email_body`, with `escape_html` on every user-supplied value.
  - **Files Touched:**
    - `common/src/email.rs`
    - `app_api/email_worker/src/processor.rs`
    - `app_api/email_worker/templates/*.html` (19 new)
    - `app_api/email_worker/src/tests.rs`
  - **Acceptance Criteria:**
    - [ ] Every template renders with a sample payload, with no unreplaced `{{…}}`.
    - [ ] `<script>` in `message_text`, `terms`, `decline_note` and names comes out escaped.
    - [ ] `as_str()` names are unique and match the file names.
  - **Verification Command:** `cargo test -p email_worker request_template_tests && cargo test -p common email_template`
  - **Dependencies:** Task 1

- [ ] **Task 28: Request notification wiring**
  - **Scope:** M
  - **Description:** `notifications::dispatch_request_notifications(pool, event, facts)` enqueues the guest and host emails after commit. Call it from every handler (enquiry, request, accept, decline, quote, checkout, withdraw), from the `create_booking` supersession and from the sweep expiry (claimed in `request_notification_log` first). The message email links to the thread.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/notifications.rs`
    - `app_api/booking_api/src/apis/booking_requests.rs`
    - `app_api/booking_api/src/apis/enquiries.rs`
    - `app_api/booking_api/src/apis/cron.rs`
    - `app_api/booking_api/src/apis/request_notification_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Each event enqueues exactly the expected two `email_outbox` rows (template and recipient).
    - [ ] A sweep run twice sends expiry emails once.
    - [ ] Admin actions email the host and the guest.
    - [ ] A failed recipient lookup is logged with `error!` and doesn't fail the request.
  - **Verification Command:** `devenv shell -- cargo test -p booking_api request_notification_tests`
  - **Dependencies:** Task 19, Task 23, Task 25, Task 26, Task 27

- [ ] **Task 29: Host near-expiry reminder**
  - **Scope:** S
  - **Description:** `requests_due_for_reminder(pct)` + `claim_request_notification`; `run_notification_sweep` sends `request_host_reminder`, and its response gains `request_reminders_processed`.
  - **Files Touched:**
    - `db_core/src/booking_request/expiry.rs`
    - `app_api/booking_api/src/apis/cron.rs`
    - `app_api/booking_api/src/apis/request_notification_tests.rs`
  - **Acceptance Criteria:**
    - [ ] It fires exactly once inside the window, not before it, not after expiry, and not for `accepted` or `quoted`.
    - [ ] Changing `request_near_expiry_pct` moves the window.
  - **Verification Command:** `devenv shell -- cargo test -p booking_api request_reminder_tests`
  - **Dependencies:** Task 26, Task 27

## Phase G: Inbox

- [ ] **Task 30: Inbox and response metrics**
  - **Scope:** M
  - **Description:** `inbox_items(scope, filter, page)` is one `UNION ALL` query joined to listing and user, 50 per page, ordered per spec §1.10. `host_response_metrics` is one 90-day `percentile_cont(0.5)` aggregate plus the expired-unanswered share. `GET /api/v1/inbox` is staff only, and hosts are scoped to their own listings in SQL.
  - **Files Touched:**
    - `db_core/src/booking_request/inbox.rs`
    - `app_api/booking_api/src/apis/inbox.rs`
    - `app_api/booking_api/src/apis/routes.rs`
    - `db_core/src/booking_request/inbox_tests.rs`
  - **Acceptance Criteria:**
    - [ ] A host sees only their own items and metrics; an admin sees all; a guest → 403.
    - [ ] Age, time to first response, time left and the near-expiry flag are correct on a fixed-clock fixture.
    - [ ] The median is correct for odd and even counts, and null when there are no responses.
    - [ ] `EXPLAIN` shows the partial indexes used (asserted in the test, or documented in the PR).
  - **Verification Command:** `devenv shell -- cargo test -p db_core inbox_tests && devenv shell -- cargo test -p booking_api inbox_api`
  - **Dependencies:** Task 20, Task 25

## Phase H: Clients and Portals

- [ ] **Task 31: `app_client` request methods**
  - **Scope:** S
  - **Description:** `common/src/app_client/booking_requests.rs` (+ conversations and inbox), exposed through `web_client`, with typed error-code mapping for the portals' flash messages.
  - **Files Touched:**
    - `common/src/app_client/booking_requests.rs`
    - `common/src/app_client/mod.rs`
    - `web_client/src/api_client.rs`
  - **Acceptance Criteria:**
    - [ ] A method for every endpoint in spec §3.4; a mock-server test per method checks the path, verb and body.
    - [ ] Error bodies are parsed to `RequestRefusal`.
  - **Verification Command:** `cargo test -p common app_client_requests && cargo test -p web_client booking_requests`
  - **Dependencies:** Task 19, Task 21, Task 25, Task 30

- [ ] **Task 32: Admin listing form booking mode**
  - **Scope:** S
  - **Description:** A `select` on create and edit with help text, passed through to `listing_api`.
  - **Files Touched:**
    - `web_ui_admin/src/pages/listings/form.rs`
    - `web_ui_admin/src/pages/listings/edit.rs`
  - **Acceptance Criteria:**
    - [ ] Renders the current mode; saving `request` persists it (render test).
  - **Verification Command:** `cargo test -p web_ui_admin listing_booking_mode`
  - **Dependencies:** Task 8, Task 31

- [ ] **Task 33: Guest listing buttons and request at checkout**
  - **Scope:** M
  - **Description:** `/listings/{slug}` shows buttons by mode, plus *Ask a question* and the "Host approval required" badge. `/checkout/{slug}` gets `action=book|request`, a message textarea and the request copy; a `request`-only listing hides *Book now*. Request success redirects to `/requests/{id}`.
  - **Files Touched:**
    - `web_ui_guest/src/pages/listing_detail.rs`
    - `web_ui_guest/src/pages/checkout.rs`
    - `web_ui_guest/src/pages/mod.rs`
  - **Acceptance Criteria:**
    - [ ] Render tests for each mode show the right buttons.
    - [ ] Request errors (`DATE_UNAVAILABLE`, `REQUEST_ALREADY_LIVE`, `REQUEST_WINDOW_TOO_SHORT`, `PROMO_*`) show as flash messages, with no 500.
    - [ ] No inline JS, and the CSP is unchanged.
  - **Verification Command:** `cargo test -p web_ui_guest booking_mode_cta && cargo test -p web_ui_guest checkout_request`
  - **Dependencies:** Task 31

- [ ] **Task 34: Guest enquiry pages**
  - **Scope:** S
  - **Description:** `/listings/{slug}/enquire` (GET/POST, prefilled from the query) and `/requests/enquiries/{id}` (thread, reply form, and *Submit request to book* linking to checkout with `mode=request&enquiry_id`).
  - **Files Touched:**
    - `web_ui_guest/src/pages/enquiries.rs`
    - `web_ui_guest/src/pages/mod.rs`
  - **Acceptance Criteria:**
    - [ ] Signed-out users are redirected to login with a return URL.
    - [ ] Message text renders escaped (render test with `<script>`).
  - **Verification Command:** `cargo test -p web_ui_guest enquiry_pages`
  - **Dependencies:** Task 31, Task 33

- [ ] **Task 35: Guest My requests and request detail**
  - **Scope:** M
  - **Description:** `/requests` lists cards with status badges and "expires in …" (an HTMX `every 60s` fragment refresh). `/requests/{id}` shows the history timeline, the breakdown, the quote terms, the promo-dropped notice, the decline reason and note, *Proceed to checkout* / *Accept quote* (POST with `quote_id` → `/bookings?new_booking=true`), *Withdraw* (with a confirm modal) and the thread. Add a nav link.
  - **Files Touched:**
    - `web_ui_guest/src/pages/requests.rs`
    - `web_ui_guest/src/pages/mod.rs`
    - `web_ui_common/src/` (guest shell nav entry)
  - **Acceptance Criteria:**
    - [ ] Each status renders the right actions (render tests per status).
    - [ ] `QUOTE_SUPERSEDED` and `QUOTE_EXPIRED` show a refresh prompt, not a 500.
  - **Verification Command:** `cargo test -p web_ui_guest request_pages`
  - **Dependencies:** Task 31, Task 34

- [ ] **Task 36: Admin inbox page**
  - **Scope:** M
  - **Description:** `/admin/inbox`: a table, filters, pagination, the near-expiry badge, a metrics stats card, and a nav entry with an open-count indicator.
  - **Files Touched:**
    - `web_ui_admin/src/pages/inbox.rs`
    - `web_ui_admin/src/pages/mod.rs`
    - `web_ui_common/src/` (admin shell nav entry)
  - **Acceptance Criteria:**
    - [ ] Render test with fixture items shows age, first response, time left and the badge.
    - [ ] Filter query parameters are passed through and validated.
  - **Verification Command:** `cargo test -p web_ui_admin inbox_page`
  - **Dependencies:** Task 31

- [ ] **Task 37: Admin request detail and actions**
  - **Scope:** M
  - **Description:** `/admin/requests/{id}`: *Accept*, *Decline* (reason select + note), *Send quote* (base-currency text input, terms, and an `hx-post` preview fragment), plus the thread and history.
  - **Files Touched:**
    - `web_ui_admin/src/pages/requests.rs`
    - `web_ui_admin/src/pages/mod.rs`
  - **Acceptance Criteria:**
    - [ ] The preview fragment shows the guest total, GCT and platform fee.
    - [ ] A malformed price (`1,000`, `abc`, `1.234`) is refused server-side with a flash message.
    - [ ] Actions are hidden for terminal statuses.
  - **Verification Command:** `cargo test -p web_ui_admin request_detail_page`
  - **Dependencies:** Task 36

- [ ] **Task 38: Admin enquiry detail**
  - **Scope:** S
  - **Description:** `/admin/enquiries/{id}`: thread, reply, *Send quote* (reusing T37's form component) and *Close*.
  - **Files Touched:**
    - `web_ui_admin/src/pages/requests.rs`
  - **Acceptance Criteria:**
    - [ ] `DATE_UNAVAILABLE` on a quote shows a flash message and the enquiry stays open.
  - **Verification Command:** `cargo test -p web_ui_admin enquiry_detail_page`
  - **Dependencies:** Task 37

- [ ] **Task 39: Playwright guest flows**
  - **Scope:** M
  - **Description:** Guest specs from spec §6 E2E: request mode, accept → pay, quote accept, decline, enquiry → request, and both mode. Seed listings in each mode through fixtures.
  - **Files Touched:**
    - `playwright/e2e/guest-portal/booking-requests.spec.ts`
    - `playwright/e2e/fixtures/` (request fixtures)
  - **Acceptance Criteria:**
    - [ ] All flows pass on chromium and firefox.
  - **Verification Command:** `devenv shell -- test-e2e-guest booking-requests`
  - **Dependencies:** Task 32, Task 33, Task 34, Task 35, Task 37

- [ ] **Task 40: Playwright admin flows**
  - **Scope:** S
  - **Description:** Inbox badge, accept, decline, quote with preview, enquiry quote and close, and the listing booking-mode change.
  - **Files Touched:**
    - `playwright/e2e/admin-portal/inbox.spec.ts`
  - **Acceptance Criteria:**
    - [ ] All flows pass on chromium and firefox.
  - **Verification Command:** `devenv shell -- test-e2e-admin inbox`
  - **Dependencies:** Task 32, Task 36, Task 37, Task 38

## Phase I: Finish

- [ ] **Task 41: Docs and API contracts**
  - **Scope:** S
  - **Description:** `openapi.yaml` and `openapi-prod.yaml` gain every new path and schema. `docs/ARCHITECTURE.md` gets a request lifecycle section next to §Pending Hold Lifecycle (with a state diagram), the §3.1 module tables, the §4.2.2 usage rule (D2), and the §6 config rows. `docs/ENVIRONMENT.md` gets the three env vars.
  - **Files Touched:**
    - `openapi.yaml`
    - `openapi-prod.yaml`
    - `docs/ARCHITECTURE.md`
    - `docs/ENVIRONMENT.md`
  - **Acceptance Criteria:**
    - [ ] Every path in spec §3.4 is present in both YAML files.
    - [ ] The utoipa document contains the same paths (test).
  - **Verification Command:** `for f in openapi.yaml openapi-prod.yaml; do for p in '/api/v1/booking-requests:' '/api/v1/booking-requests/{id}/checkout:' '/api/v1/enquiries:' '/api/v1/inbox:' '/api/v1/conversations/{id}/messages:'; do rg -qF "$p" "$f" || exit 1; done; done && rg -q 'request_response_hours' docs/ARCHITECTURE.md docs/ENVIRONMENT.md && cargo test -p booking_api openapi_includes_requests`
  - **Dependencies:** Task 29, Task 30

- [ ] **Task 42: Final gates**
  - **Scope:** S
  - **Description:** Run the full CONSTRAINTS §2 suite once and fix anything it finds. Run `/audit-booking-flow` against the new lock and history paths.
  - **Files Touched:** as needed
  - **Acceptance Criteria:**
    - [ ] `cargo fmt --check`, clippy `-D warnings -D missing_docs`, `cargo sqlx prepare --workspace --check`, backend and web UI test commands, and the float, unwrap and env-var greps all pass.
    - [ ] The full Playwright suite passes (`devenv shell -- test-e2e`).
    - [ ] Every acceptance criterion in spec §7 is ticked.
  - **Verification Command:** `cargo fmt --check && SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features -- -D warnings && devenv shell -- cargo sqlx prepare --workspace --check && devenv shell -- cargo test --workspace --exclude protoproj && devenv shell -- test-e2e`
  - **Dependencies:** All
