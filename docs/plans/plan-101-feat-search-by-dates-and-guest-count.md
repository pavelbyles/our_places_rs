# Implementation Plan: Search by Dates and Guest Count (#101)

**Spec Reference:** [`docs/specs/spec-101-feat-search-by-dates-and-guest-count.md`](../specs/spec-101-feat-search-by-dates-and-guest-count.md)
**Intent:** [`docs/intent/intent-101-feat-search-by-dates-and-guest-count.md`](../intent/intent-101-feat-search-by-dates-and-guest-count.md)
**Baseline Commit:** `fca80dea`
**Branch:** `feat/101-search-by-dates-and-guest-count`
**Merges Into:** `dev`

> **Engineering Review Status**: LOCKED (rev 3), plus the confirm-race fix the originator added to Task 5 afterwards, which the panel hasn't re-audited
> **Review Date**: 2026-11-03
> **Review Panel**: Lead Architect, Edge Case Analyst, Security Reviewer, DB Guardian, QA Strategist

## Cross-Cutting Rules (apply to every task)
- **Local database**: Postgres runs in the devenv shell, not Docker. Run `sqlx`, `psql` and database tests through `devenv shell --`. Commands without it use `SQLX_OFFLINE=true`.
- **SQLx metadata**: after changing a macro query, run `devenv shell -- cargo sqlx prepare --workspace -- --all-targets` and commit the root `.sqlx/` changes with that task. Tests that add macro queries count too.
- **Task gates**: `cargo fmt --check`, `SQLX_OFFLINE=true cargo clippy -p <touched crates> --all-targets --all-features -- -D warnings`, and `SQLX_OFFLINE=true cargo check --workspace --exclude protoproj --all-features --tests`, so a change in one crate can't leave another uncompiled. Every new public item has a `///` comment.
- **Tests come first**: write the failing test, then the code. Each task creates the new, uniquely named test module its command filters on. A filter that matches no tests reports "0 passed", and a filter that also matches older tests can pass before the work exists. Neither counts as green.
- **Targeted tests while iterating**: run only the task's command. The full suites run once, in Task 22.
- **Database test isolation**:
  - `db_core` listing tests run in a rolled-back transaction on the real `DATABASE_URL`, so they see seeded rows. They scope assertions by `host` or `user_id`, and insert bookings with SQL inside the transaction.
  - `create_booking` opens its own transaction from a pool, so anything calling it, including concurrency tests, uses `sqlx_db_tester::TestPg` like `db_core/src/promo_tests.rs`, with real connections, `#[tokio::test(flavor = "multi_thread")]` and no sleeps.
- **E2E runs**: restart the APIs and portals from this worktree first (`apis-stop`, `frontends-stop`, then `apis-start`). Playwright reuses any healthy server on its ports, which could be from another checkout or an old build.
- **Error mapping**: a new `DbError` variant has explicit arms in both `status_code` and `error_response`. Expected refusals never log at ERROR.
- **Money**: `Decimal` only, no fallback amounts or rates. Prices with a gap of 0 and no past dates must not change.
- **Gap expression**: every SQL copy of the availability rule uses `LEAST(GREATEST(<listing>.days_between_bookings, 0), 365)`, so a stored out-of-range value can't overflow a date or shrink the rule.
- **Lock order** doesn't change. Holds take listing → currency → the guest's other pending bookings → promo code.
- **Existing tests**: update them only for changes the spec calls for, without weakening an assertion.
- **Service boundary**: `listing_api` and `booking_api` never depend on each other, including in tests.
- **Logs**: search text and the raw query string stay out of span fields and log lines, because a guest may type personal data into them. Refusals log only their code.
- **Blocking work and logs**: `capture_logs` uses a thread-local subscriber, so anything a test must capture is logged on the async side, after `spawn_blocking` returns.
- **E2E applies to `ui-js` too**: `globalSetup` and `webServer` run for that project as well, so the restart rule applies to Task 6's command.

## Dependency Graph
Each line is `task ← dependencies`, matching every task's **Dependencies** field. Riskiest work comes first: the gap changes which bookings the platform accepts, and the pricing move touches every quote and hold.

```text
Phase A: stay rules and errors
T1  stay input validation      ← —
T2  stay input error mapping   ← T1
T3  gap and discount limits    ← —

Phase B: gap enforcement
T4  gap in availability checks ← T3
T5  gap in holds, 409 refusal  ← T4
T6  gap on the calendar        ← T4

Phase C: shared pricing
T7  move pricing to common     ← —
T8  past dates at quote, hold  ← T1, T2, T7
T9  checkout error messages    ← T2, T5

Phase D: search API
T10 stay filters               ← T4
T11 text search                ← —
T12 availability parity test   ← T5, T6, T10
T13 search params, validation  ← T1, T2, T10, T11
T14 batched override load      ← —
T15 stay totals in search      ← T7, T13, T14

Phase E: portal
T16 search client refusals     ← T13
T17 listing card totals, links ← T15
T18 /listings page             ← T16, T17
T19 currency menu keeps search ← T9
T20 home form, 1 guest default ← —

Phase F: finish
T21 Playwright coverage, docs  ← T6, T8, T18, T19, T20
T22 full gates                 ← T12, T21
```

## Spec Coverage

| Spec section | Tasks |
|---|---|
| §1.1 Search parameters | T13, T16 |
| §1.2 Validation | T1, T2, T13 |
| §1.3 Search filters | T10, T11, T13 |
| §1.4 Stay total | T7, T14, T15 |
| §1.5 Availability rule, gap limits | T3, T4, T5, T6, T10, T12 |
| §1.6 Past dates, hold refusals | T5, T8 |
| §1.7 Portal | T9, T16, T17, T18, T19, T20 |
| §3.7 Documentation | T21 |
| §6 Test plan | Each task's tests, T12, T21 |
| §7 Acceptance criteria | T22 checks each one |

## Test Priorities

| Priority | Tasks |
|---|---|
| P0 happy path | T10, T11, T15, T21 |
| P0 empty or invalid input | T1, T13, T18 |
| P1 error handling | T5, T9, T15, T16, T18 |
| P1 concurrency | T5 |

## Risks

| Risk | Guard |
|---|---|
| Moving pricing changes a price | T7 pins current figures before moving any code, including the rounding asymmetry at a rate of 1 |
| The rule's SQL copies drift apart | T12's parity test covers every path at gaps of 0, 1 and 3, and a stored out-of-range gap |
| A stored gap breaks search or shrinks the rule | The shared clamped expression, T10's test, and T3's release check |
| A gap set long ago takes effect on release | T22 records a production check of existing values and of future bookings inside them |
| Search converts prices twice | T15 prices from database rows before conversion, and tests with a non-base currency |
| Card total differs from the hold | T7's golden file is checked by both services in T15, and T21 compares real pages |
| Pricing blocks an async worker | T15 runs the pricing loop in `spawn_blocking` |

---

## Tasks

### Phase A: Stay Rules and Errors

- [x] **Task 1: Add stay input validation to `common`**
  - **Scope:** S
  - **Description:** Add `jamaica_today`, `StayInputError`, `validate_stay_dates`, `parse_stay_search` and `StaySearch` (spec §3.2), including `q` handling. Replace every copy of the Jamaica date formula in the portal and `booking_api`, including those in `reservation_changes.rs`. Move `urlencode` from `web_ui_guest/src/pages/register.rs` to a new `common::query` module, because `web_ui_common` needs it in Task 17, and update its importers.
  - **Files Touched:**
    - `common/src/availability.rs`
    - `common/src/query.rs`
    - `common/src/lib.rs`
    - `web_ui_guest/src/support.rs`
    - `web_ui_guest/src/pages/register.rs`
    - `web_ui_guest/src/pages/auth.rs`
    - `web_ui_guest/src/pages/checkout.rs`
    - `app_api/booking_api/src/apis/booking_updates.rs`
    - `app_api/booking_api/src/apis/reservation_changes.rs`
    - `app_api/booking_api/src/apis/cron.rs`
  - **Acceptance Criteria:**
    - [ ] Each row of spec §1.2 gives its code and exact message, in the stated order.
    - [ ] Empty values count as absent, and a half-filled date range can't be represented in `StaySearch`.
    - [ ] Check-in today passes, yesterday fails. Exactly `max_nights` passes, one more fails.
    - [ ] `guests` of 0, -1, "abc" and above `i32` are refused.
    - [ ] `q` is trimmed, blank counts as absent, and a NUL or control character or more than 200 characters is refused.
    - [ ] `jamaica_today` at 03:00 UTC returns the previous date.
    - [ ] Booking update and cron tests still pass with the shared date.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p common stay_input_tests && devenv shell -- cargo test -p booking_api test_booking_date_change_api && devenv shell -- cargo test -p booking_api test_complete_stays_cron`
  - **Dependencies:** None

- [x] **Task 2: Map stay input errors to a typed 400**
  - **Scope:** XS
  - **Description:** Add `DbError::StayInput(StayInputError)` and return it as a 400 with `{code, message}`, alongside `PromoRefused`.
  - **Files Touched:**
    - `db_core/src/error.rs`
    - `app_api/api_core/src/error.rs`
  - **Acceptance Criteria:**
    - [ ] Each `StayInputError` variant returns a 400 with its code and message.
    - [ ] It isn't treated as a server failure by `error_response`.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p api_core stay_input_error_tests`
  - **Dependencies:** Task 1

- [x] **Task 3: Limit the gap and discount settings**
  - **Scope:** S
  - **Description:** `days_between_bookings` must be 0–365, on create, update and the admin form. `weekly_discount_percentage` and `monthly_discount_percentage` must be 0–100 on create and update, because an unbounded value could overflow `Decimal` and panic in pricing. The admin form already caps them. Run a read-only check on the local database for out-of-range rows, and flag any to the originator rather than changing them. Task 22 records the production check.
  - **Files Touched:**
    - `common/src/models.rs`
    - `web_ui_admin/src/pages/listings/form.rs`
    - `web_ui_admin/tests/listing_gap_limit.rs`
    - `app_api/listing_api/src/apis_test.rs`
  - **Acceptance Criteria:**
    - [ ] Create and update accept a gap of 0 and 365, and refuse -1 and 366 with a 400.
    - [ ] Create and update refuse discounts below 0 and above 100.
    - [ ] `ListingFields::validate` reports an error for a gap of 366, rather than sending it.
  - **Verification Command:** `devenv shell -- cargo test -p listing_api gap_limit_tests && SQLX_OFFLINE=true cargo test -p web_ui_admin --test listing_gap_limit`
  - **Dependencies:** None

### Phase B: Gap Enforcement

- [x] **Task 4: Apply the gap in availability checks**
  - **Scope:** S
  - **Description:** `check_availability` and `check_availability_excluding` use the rule in spec §1.5 with the shared gap expression, joining `listing`. Qualify every column. This also covers `GET /bookings/availability`, booking updates and all three reservation-change paths, with no change at their call sites.
  - **Files Touched:**
    - `db_core/src/booking/availability.rs`
    - `db_core/src/booking/gap_tests.rs`
    - `db_core/src/booking/mod.rs`
    - `app_api/booking_api/src/apis/tests.rs`
    - `.sqlx/`
  - **Acceptance Criteria:**
    - [ ] With a gap of N, a stay N−1 days from a booking is refused on either side, and N days away is accepted.
    - [ ] A gap of 0 behaves as today, including same-day turnover.
    - [ ] A booking changing its dates ignores itself and is checked against its neighbours.
    - [ ] A booking update, and a reservation change at quote, request and approval, each return 409 `DATE_UNAVAILABLE` inside the gap.
    - [ ] `GET /bookings/availability` returns `available: false` inside the gap.
    - [ ] Raising a gap doesn't affect a booking that keeps its dates, and confirming a live hold still works.
    - [ ] With A `[10,15)`, B `[15,20)` and a gap raised to 2, moving A to `[10,14)` is refused, because it's still inside the gap, and `[10,13)` is accepted.
  - **Verification Command:** `devenv shell -- cargo test -p db_core gap_tests && devenv shell -- cargo test -p booking_api gap_change_tests`
  - **Dependencies:** Task 3

- [x] **Task 5: Apply the gap in holds**
  - **Scope:** S
  - **Description:** The overlap query in `create_booking` uses the same rule, after the listing row lock. Add `LEAST(GREATEST(days_between_bookings, 0), 365) AS "gap!"` to the lock query at `hold.rs:34`, and bind it in the overlap query as `date_from - $4::int4 < $3 AND date_to + $4::int4 > $2`. The cast is required: without it, `date + $4` is ambiguous and `date - $4` resolves to `date - date`. A conflict returns `DbError::DateUnavailable`. In `bookings.rs`, add an arm for it below ERROR, like the `PromoRefused` arm, since losing a race after a search is now a normal flow. Check that the status-only 400 assertions at `authz_tests.rs:899` and `:909` don't expect the old conflict 400. Also close an existing double-booking window: `lock_booking` computes `hold_active` with `NOW()` (`update.rs:73`), which is fixed when the transaction begins, so a confirmation that waited on the listing lock can succeed after its hold expired and a rival took the dates. Use `clock_timestamp()` there, as `keep_promo_use_on_confirmation` already does. Test it in a `#[cfg(test)] mod confirm_clock_tests` in `update.rs`, because `lock_booking` is private: inside one transaction, set the hold's `expires_at = clock_timestamp()`, then call `lock_booking`. That value is later than the transaction's `NOW()` and earlier than the next `clock_timestamp()`, so the old code reports the hold as active and the fix doesn't, with no sleep.
  - **Files Touched:**
    - `db_core/src/booking/hold.rs`
    - `db_core/src/booking/update.rs`
    - `db_core/src/booking/hold_gap_tests.rs`
    - `db_core/src/booking/mod.rs`
    - `app_api/booking_api/src/apis/bookings.rs`
    - `app_api/booking_api/src/apis/tests.rs`
    - `.sqlx/`
  - **Acceptance Criteria:**
    - [ ] A hold inside the gap of a confirmed booking or live hold is refused with `DATE_UNAVAILABLE`. An expired hold doesn't count.
    - [ ] Concurrent holds inside each other's gap produce exactly one success, using `TestPg`.
    - [ ] A guest can replace their own live hold with one inside its gap, and the old hold is cancelled.
    - [ ] `POST /api/v1/bookings` returns 409 with `DATE_UNAVAILABLE` on a conflict, and emits no ERROR event.
    - [ ] `hold_active` is false for a hold whose `expires_at` falls between the transaction's start and the check, and confirming a hold that's still live works as before.
  - **Verification Command:** `devenv shell -- cargo test -p db_core hold_gap_tests && devenv shell -- cargo test -p db_core confirm_clock_tests && devenv shell -- cargo test -p booking_api hold_conflict_tests`
  - **Dependencies:** Task 4

- [x] **Task 6: Show the gap on the calendar**
  - **Scope:** M
  - **Description:** `get_unavailable_ranges` returns ranges widened by the gap, and finds bookings just outside the window whose gap reaches into it. Select `b.date_from - <gap> AS "date_from!"` and `b.date_to + <gap> AS "date_to!"`, because sqlx types computed columns as optional. Qualify `b.date_from` in `WHERE`. Doc comments and screen-reader text change from "booked" to "unavailable", and tests assert the new wording. Regenerate the rendered fixture with `UPDATE_FIXTURES=1 SQLX_OFFLINE=true cargo test -p web_ui_common --test date_range_calendar`. `data-state` and `ui.js`'s selection logic don't change.
  - **Files Touched:**
    - `db_core/src/booking/availability.rs`
    - `db_core/src/booking/gap_calendar_tests.rs`
    - `db_core/src/booking/mod.rs`
    - `common/src/availability.rs`
    - `web_ui_common/src/components/date_range_calendar.rs`
    - `web_ui_common/src/assets/ui.js`
    - `web_ui_common/tests/date_range_calendar.rs`
    - `playwright/e2e/ui-js/calendar.spec.ts`
    - `playwright/e2e/ui-js/fixtures/calendar.html`
    - `.sqlx/`
  - **Acceptance Criteria:**
    - [ ] Ranges widen by the gap, and widened ranges merge when they touch.
    - [ ] A booking just outside the window greys out the nights its gap reaches.
    - [ ] A gap of 0 returns the same ranges as today.
    - [ ] The response still contains dates only.
    - [ ] Calendar labels and `ui.js` announcements say "unavailable".
  - **Verification Command:** `devenv shell -- cargo test -p db_core gap_calendar_tests && SQLX_OFFLINE=true cargo test -p web_ui_common --test date_range_calendar && devenv shell -- npx playwright test --config=playwright/playwright.config.ts --project=ui-js-chromium calendar`
  - **Dependencies:** Task 4

### Phase C: Shared Pricing

- [x] **Task 7: Move promo-free pricing into `common`**
  - **Scope:** M
  - **Description:** First add `common/testdata/stay_pricing_golden.json`, holding inputs and expected totals, and tests that pin `price_stay` to it. Then move the conversion and pricing steps into `price_before_promo` and `fits_booking_columns` (spec §3.2), with `price_stay` calling them. No price changes. Move the rounding asymmetry unchanged: the nightly rate always rounds, and `convert` skips rounding at a rate of 1. The file is read with `include_str!` by both services' tests in Task 15, so it adds no production code or shared test crate. It holds only 2-decimal inputs, because Task 15 stores them in `NUMERIC(_,2)` columns. Pin the tests through the HTTP endpoints, so Task 8's new `StayRequest` field doesn't affect them.
  - **Files Touched:**
    - `common/testdata/stay_pricing_golden.json`
    - `common/src/pricing.rs`
    - `app_api/booking_api/src/apis/stay_pricing.rs`
    - `app_api/booking_api/src/apis/stay_pricing_pin_tests.rs`
    - `app_api/booking_api/src/apis/mod.rs`
  - **Acceptance Criteria:**
    - [ ] Pinned figures pass before and after the move, with extra guests, an override, weekly and monthly discounts, a non-base currency and a rate of 1.
    - [ ] A rate-of-1 case with 3-decimal inputs pins the rounding asymmetry, in `pricing_core_tests` in `common`, not in the golden file.
    - [ ] Existing promo, quote and hold tests pass unchanged.
    - [ ] `common` pricing has no database access.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p common pricing_core_tests && devenv shell -- cargo test -p booking_api stay_pricing_pin_tests && devenv shell -- cargo test -p booking_api promo_hold_tests`
  - **Dependencies:** None

- [x] **Task 8: Refuse past dates at quote and hold**
  - **Scope:** S
  - **Description:** `StayRequest` gains `today: NaiveDate`, set by one helper at its three call sites, so tests can choose the date. `price_stay` uses `validate_stay_dates`, replacing its existing date and length checks. Keep the `i32::try_from` guard, because `max_stay_nights` is an `i64` with no upper bound.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/stay_pricing.rs`
    - `app_api/booking_api/src/apis/bookings.rs`
    - `app_api/booking_api/src/apis/past_date_tests.rs`
    - `app_api/booking_api/src/apis/mod.rs`
  - **Acceptance Criteria:**
    - [ ] A past check-in returns 400 `INVALID_STAY_DATES` from `create_booking`, the public quote and the signed-in quote, and creates no hold.
    - [ ] With `today = X`, where X differs from the real date, check-in X is accepted and X−1 refused.
    - [ ] Over-limit stays return `INVALID_STAY_DATES`.
    - [ ] Booking updates and reservation changes behave as before.
  - **Verification Command:** `devenv shell -- cargo test -p booking_api past_date_tests`
  - **Dependencies:** Tasks 1, 2, 7

- [ ] **Task 9: Fix checkout's error messages**
  - **Scope:** XS
  - **Description:** `booking_error_message` checks error codes before the word match, and shows `INVALID_STAY_DATES`'s own message.
  - **Files Touched:**
    - `web_ui_guest/src/pages/checkout.rs`
  - **Acceptance Criteria:**
    - [ ] A full `FX_RATE_UNAVAILABLE` body gives the currency message, not the dates message.
    - [ ] `DATE_UNAVAILABLE` gives the dates message, and `INVALID_STAY_DATES` shows its message.
    - [ ] Existing cases keep their messages.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_guest booking_error_codes`
  - **Dependencies:** Tasks 2, 5

### Phase D: Search API

- [ ] **Task 10: Add stay filters to `get_listings`**
  - **Scope:** S
  - **Description:** `ListingFilter` gains `check_in`, `check_out` and `guests`. `get_listings` adds the availability, guest, minimum-stay and bookable-only conditions from spec §3.3, with every value bound. Add `..Default::default()` to the field-by-field literal at `app_api/listing_api/src/apis.rs:144`, so the workspace still compiles, and Task 13 fills the fields and removes it.
  - **Files Touched:**
    - `common/src/models.rs`
    - `db_core/src/listing/queries.rs`
    - `db_core/src/listing/tests.rs`
    - `app_api/listing_api/src/apis.rs`
  - **Acceptance Criteria:**
    - [ ] `max_guests` boundary: equal matches, one more doesn't.
    - [ ] Minimum stay at and below the limit, including a partial override with a higher `min_nights`.
    - [ ] Dated searches leave out inactive listings and listings with no price, even with `include_inactive`.
    - [ ] Pagination returns full pages when earlier listings were filtered out.
    - [ ] A listing storing `days_between_bookings = 2147483647` with a confirmed booking doesn't fail the search, and other listings still return.
    - [ ] Searches with no new fields return what they return today.
  - **Verification Command:** `devenv shell -- cargo test -p db_core search_filter_tests`
  - **Dependencies:** Task 4

- [ ] **Task 11: Add text search on name, city or country**
  - **Scope:** XS
  - **Description:** `ListingFilter` gains `q`, an escaped case-insensitive partial match. The existing `name` filter doesn't change. The field is added through the `..Default::default()` from Task 10 if that's done first. If this task runs first, add it here.
  - **Files Touched:**
    - `common/src/models.rs`
    - `db_core/src/listing/queries.rs`
    - `db_core/src/listing/tests.rs`
    - `app_api/listing_api/src/apis.rs`
  - **Acceptance Criteria:**
    - [ ] Matches on name, city and country, ignoring case. A NULL `city` doesn't hide a name match.
    - [ ] `%`, `_` and `\` match literally.
    - [ ] `name` still matches only `listing.name`.
  - **Verification Command:** `devenv shell -- cargo test -p db_core text_search_tests`
  - **Dependencies:** None

- [ ] **Task 12: Prove every path agrees on availability**
  - **Scope:** S
  - **Description:** Add the table-driven parity test from spec §6, comparing `check_availability`, the hold, search and the calendar for the same bookings. Use `TestPg`, a fresh listing for each case, and a different guest for the hold probe, because a guest's own holds don't conflict and a successful probe would otherwise affect the next case.
  - **Files Touched:**
    - `db_core/src/booking/availability_parity_tests.rs`
    - `db_core/src/booking/mod.rs`
  - **Acceptance Criteria:**
    - [ ] Covers confirmed, live hold, expired hold, cancelled and completed bookings.
    - [ ] Covers overlap, same-day turnover either way, and N−1 and N days from the booking on each side.
    - [ ] Every path agrees at gaps of 0, 1 and 3, and at a stored gap of -1 and 2147483647, which behave as 0 and 365.
  - **Verification Command:** `devenv shell -- cargo test -p db_core availability_parity_tests`
  - **Dependencies:** Tasks 5, 6, 10

- [ ] **Task 13: Accept and validate search params in `listing_api`**
  - **Scope:** S
  - **Description:** `ListingQueryParams` gains `q`, `check_in`, `check_out` and `guests` as strings. A helper taking `now` calls `parse_stay_search` before any query, and passes the result to the filter. Fill the `ListingFilter` literal at `apis.rs:144` and remove Task 10's `..Default::default()`, which `clippy::needless_update` would otherwise flag. Replace both `#[tracing::instrument]` attributes on `get_listings` (`apis.rs:101` and `:114`) with one `#[tracing::instrument(skip_all)]`, so a refusal doesn't log at ERROR and `query` and `req` aren't recorded. Change `db_core/src/listing/queries.rs:9` to `skip(executor, filter)`. `CustomRootSpanBuilder` records a path-only `http.target`, by building its span without the query-bearing field, as `web_ui_common/src/request_tracing.rs:46-52` does. Calling `span.record` after `root_span!` won't work, because the macro has already stored the full target and the log layer appends rather than replaces. `main.rs` validates `max_stay_nights` at startup, and the key is added to `listing_api`'s `Default.toml`, so the two services can't disagree. Copy `capture_logs` from `promo_hold_tests.rs` into `apis_test.rs` as a `pub(super)` helper, for this task and Task 15.
  - **Files Touched:**
    - `common/src/models.rs`
    - `app_api/api_core/src/tracing_utils.rs`
    - `app_api/listing_api/src/apis.rs`
    - `app_api/listing_api/src/main.rs`
    - `app_api/listing_api/config/Default.toml`
    - `db_core/src/listing/queries.rs`
    - `app_api/listing_api/src/apis_test.rs`
  - **Acceptance Criteria:**
    - [ ] Each invalid input returns a 400 with its code and message. Using a lazy pool pointing at a closed port, the test gets a 400 rather than a 5xx, which proves no query ran.
    - [ ] `check_in=abc` gets the typed error, not Actix's untyped 400.
    - [ ] A refused search emits no ERROR event.
    - [ ] With the root span wrapper in place, a marker value in `q` appears in no log line, for a valid search or a refused one.
    - [ ] Valid params filter results through the API, and text combines with dates and guests.
    - [ ] With `now` set to a fixed time, the Jamaica date decides what counts as past.
    - [ ] Searches with no new params return what they return today.
  - **Verification Command:** `devenv shell -- cargo test -p listing_api search_param_tests`
  - **Dependencies:** Tasks 1, 2, 10, 11

- [ ] **Task 14: Load price overrides for many listings at once**
  - **Scope:** XS
  - **Description:** Add `get_active_overrides_for_listings`, one query returning overrides grouped by listing, and re-export it from `db_core/src/listing/mod.rs`, because `price_overrides` is a private module.
  - **Files Touched:**
    - `db_core/src/listing/price_overrides.rs`
    - `db_core/src/listing/mod.rs`
    - `db_core/src/listing/tests.rs`
  - **Acceptance Criteria:**
    - [ ] Returns only overrides touching the stay, grouped by listing, using the same overlap as `get_active_overrides_for_dates`.
    - [ ] Listings with no overrides are simply absent from the map.
  - **Verification Command:** `devenv shell -- cargo test -p db_core batched_override_tests`
  - **Dependencies:** None

- [ ] **Task 15: Return stay totals from search**
  - **Scope:** M
  - **Description:** Add `StayTotal` and `ListingResponse::stay_total`. With dates, the handler prices each listing from its database row, before mapping and conversion. Rates load in a helper that returns the map, so no connection is held during pricing, and `convert_search_prices` takes that map. The pricing loop runs in `spawn_blocking` on owned rows, because a host controls the number of overrides and the stay can be years long. A listing that can't be priced gets no total, including one with a stored discount outside 0–100, which is skipped before pricing because it could overflow `Decimal`. The closure returns the failed listing IDs, and one aggregated `warn!` names them after the join. A `JoinError` is logged, and the listings return with no totals. A failed override query fails the request, because pricing without overrides would be a guess. Wire the test module with `#[cfg(test)] #[path = "stay_total_tests.rs"] mod stay_total_tests;`, as `apis.rs` does for its others. Tests use `capture_logs` from Task 13 to count queries and warnings.
  - **Files Touched:**
    - `common/src/models.rs`
    - `app_api/api_core/src/models.rs`
    - `app_api/listing_api/src/apis.rs`
    - `app_api/listing_api/src/apis_test.rs`
    - `app_api/listing_api/src/stay_total_tests.rs`
    - `app_api/booking_api/src/apis/stay_total_golden_tests.rs`
    - `app_api/booking_api/src/apis/mod.rs`
  - **Acceptance Criteria:**
    - [ ] `listing_api`'s `stay_total` matches each case in the golden file, with extra guests, an override, a weekly discount and a non-base currency.
    - [ ] `booking_api`'s public quote and the `total_price` of a created hold match the same cases.
    - [ ] Guests omitted prices for 1, and currency omitted uses the base currency.
    - [ ] A missing rate gives `price_unavailable_in` on that listing and normal totals on the others.
    - [ ] A total too large to store gives no total, a warning, and normal results for the others.
    - [ ] A failed override query returns an error, never a total without overrides.
    - [ ] A stored discount outside 0–100 gives no total, a warning, and normal results for the others.
    - [ ] A failed blocking task returns the listings with no totals and a logged error, never a failed search. A pure helper, `totals_or_none(Result<_, JoinError>)`, holds this logic, and is tested with a real `JoinError` from `tokio::task::spawn_blocking(|| panic!())`.
    - [ ] After one warm-up request, counted `sqlx::query` log lines are the same for 1 and 50 listings, with dates and a non-base currency.
    - [ ] No `stay_total` appears without dates, and the JSON is otherwise unchanged.
  - **Verification Command:** `devenv shell -- cargo test -p listing_api stay_total_tests && devenv shell -- cargo test -p booking_api stay_total_golden_tests`
  - **Dependencies:** Tasks 7, 13, 14

### Phase E: Portal

- [ ] **Task 16: Surface search refusals in the client**
  - **Scope:** S
  - **Description:** `ListingSearchParams` gains the new fields as strings. Add `search_listings_at(client, base_url, audience, params)` in `common`, and make the free function delegate to it. `TopcoatApiClient::search_listings` passes its own client, URL and `listing_api_audience()`, so tests can reach a stub server. The audience must not be the URL, and the test's `FixedIdToken` wouldn't catch that, so assert it. It returns a `ListingSearchRefusal` on a 4xx whose code is `INVALID_STAY_DATES`, `INVALID_GUESTS`, `INVALID_SEARCH_TEXT` or starts with `CURRENCY_`. Other failures stay ordinary errors.
  - **Files Touched:**
    - `common/src/app_client/listings.rs`
    - `web_client/src/client.rs`
    - `web_client/tests/search_refusal.rs`
  - **Acceptance Criteria:**
    - [ ] Empty values aren't sent.
    - [ ] An allow-listed code gives a `ListingSearchRefusal` with its message.
    - [ ] A 4xx with an unknown code, or `{"error": ...}` with no code, stays an ordinary error, so its text never reaches a page.
    - [ ] Existing callers compile unchanged.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_client --test search_refusal`
  - **Dependencies:** Task 13

- [ ] **Task 17: Show totals on listing cards**
  - **Scope:** M
  - **Description:** `ListingSummary` gains `stay_total`, `search_query` and `href()`. Both cards show the total lines from spec §1.7, using `price_breakdown::money`, and their links carry the search, encoded with `common`'s `urlencode`.
  - **Files Touched:**
    - `web_ui_common/src/components/domain/listing_card.rs`
    - `web_ui_common/src/components/domain/villa_card.rs`
    - `web_ui_common/tests/domain_parity.rs`
  - **Acceptance Criteria:**
    - [ ] Total line with and without a deposit, with cents.
    - [ ] "Total unavailable for these dates" with dates but no total, and "Price unavailable in CUR" when a rate is missing.
    - [ ] Links carry the search, and `guests=1` when dates have no guests.
    - [ ] Empty values are left out of links, tested with guests and no dates.
    - [ ] Values containing `&currency=XXX#`, `+`, `=`, `"` and `<` round-trip once, with no injected parameter.
    - [ ] With no search, card output doesn't change, including on the home page carousel.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_common stay_card_tests && SQLX_OFFLINE=true cargo test -p web_ui_common --test domain_parity`
  - **Dependencies:** Task 15

- [ ] **Task 18: Build the `/listings` search page**
  - **Scope:** M
  - **Description:** Page functions need a real request context, so split `listings_page` into parts that can be tested: query parsing from a string, a pure `classify(anyhow::Result<Vec<_>>) -> Outcome`, and a `#[component] listings_results`. Tests live inline in `mod listings_page_tests`, using the `html()` helper pattern from `web_ui_common/tests/shells.rs`. The form has `q`, dates and guests, and hidden `host` and `currency`. Query values are read as strings and forwarded. The page uses `display_currency`, and forwards only the params in spec §1.1, `host` and `currency`.
  - **Files Touched:**
    - `web_ui_guest/src/pages/listings.rs`
    - `web_ui_guest/Cargo.toml` (`anyhow`, and `serde_urlencoded = "0.7.1"`, because Topcoat's decoder is private)
    - `playwright/e2e/guest-portal/listings.spec.ts`
  - **Acceptance Criteria:**
    - [ ] A refusal shows its message in `role="alert"` with the form and no listings.
    - [ ] Any other failure shows "We couldn't load listings. Please try again.", and is logged, checked with a captured log. It never reads as "no listings".
    - [ ] The "N stays" line is hidden on a refusal or failure.
    - [ ] `check_in=abc` no longer drops the rest of the search.
    - [ ] The form is pre-filled from the URL, with hidden `host` and `currency` where set.
    - [ ] "Show all listings" removes only `host`.
    - [ ] The empty state mentions dates when they were given.
    - [ ] `include_inactive`, `owner` and `user_id` on the portal URL aren't forwarded.
    - [ ] `listings.spec.ts:14` asserts the new placeholder.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_guest listings_page_tests`
  - **Dependencies:** Tasks 16, 17

- [ ] **Task 19: Keep the search when the currency changes**
  - **Scope:** M
  - **Description:** `currency_menu` builds links that keep `q`, `host`, `check_in`, `check_out` and `guests`, with `currency` first. A pure `search_params_from_query(&str)` picks them out by key, and forwards the pairs still encoded, so no decoder is needed. `guest_shell` reads the URI and passes them through a new `GuestShell.search_params` field, because reading it in `guest_chrome` would panic in `Cx::default()` tests. The checkout form's `action` uses the existing `checkout_url(...)` when a stay is valid, because re-rendering after a refusal otherwise has no query string to keep, and that's when the page tells guests to choose another currency.
  - **Files Touched:**
    - `web_ui_common/src/components/domain/currency_selector.rs`
    - `web_ui_common/src/shell_guest.rs`
    - `web_ui_common/tests/shells.rs`
    - `web_ui_guest/src/pages/checkout.rs`
  - **Acceptance Criteria:**
    - [ ] Allow-listed params are kept and any others are dropped. Task 17's hostile values stay encoded and add no parameter.
    - [ ] With no search params, links are `?currency=X`, as today.
    - [ ] The checkout form's `action` carries the stay, and a POST to it still works.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_common currency_menu_search_tests && SQLX_OFFLINE=true cargo test -p web_ui_guest checkout_form_action`
  - **Dependencies:** Task 9

- [ ] **Task 20: Default to 1 guest and update the home form**
  - **Scope:** S
  - **Description:** Add `DEFAULT_GUESTS` and `guests_or_default(Option<..>)` to `web_ui_guest/src/support.rs`, used by the listing page, where `guests` is `i32`, and checkout, where it's `u32`. Extract `#[component] home_search_form(today, currency)` from `home.rs`, so it can be tested. Date inputs get `min` set to today, and it has a hidden `currency` when one is chosen.
  - **Files Touched:**
    - `web_ui_guest/src/support.rs`
    - `web_ui_guest/src/pages/home.rs`
    - `web_ui_guest/src/pages/listing_detail.rs`
    - `web_ui_guest/src/pages/checkout.rs`
  - **Acceptance Criteria:**
    - [ ] `guests_or_default(None)` is 1, used at `listing_detail.rs:164`, `:314` and `checkout.rs:330`.
    - [ ] The home form renders `value="1"`, `min` and a hidden `currency`.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_guest default_guests_tests`
  - **Dependencies:** None

### Phase F: Finish

- [ ] **Task 21: Add Playwright coverage and update the docs**
  - **Scope:** M
  - **Description:** Add the E2E flows in spec §6, and a fixture listing with a gap by extending `createListingCopy` in `bookings-data.ts`. Update `docs/ARCHITECTURE.md`, and the entries in `openapi.yaml` and `openapi-prod.yaml` for changed routes they already list. Anything missing belongs to #113. `sanity-check` doesn't run Playwright, so this task runs the whole guest suite.
  - **Files Touched:**
    - `playwright/e2e/guest-portal/search.spec.ts`
    - `playwright/e2e/fixtures/bookings-data.ts`
    - `docs/ARCHITECTURE.md`
    - `openapi.yaml`
    - `openapi-prod.yaml`
  - **Acceptance Criteria:**
    - [ ] A home search reaches `/listings` with filters applied, and reloading gives the same results.
    - [ ] A card link keeps dates and guests through to checkout, and the card total equals checkout's total.
    - [ ] Switching currency keeps the search on `/listings`, the listing page (for dates in its URL) and checkout, and changes the totals.
    - [ ] With a gap, search leaves out a stay inside it, the calendar greys out those nights, and the last allowed check-out can be selected.
    - [ ] An old bookmark shows the validation message and a way to pick new dates.
    - [ ] Searching a city and a country finds the expected listings.
    - [ ] With no `guests` param, the listing page and checkout show 1.
    - [ ] The search page works at phone width.
    - [ ] Existing guest portal specs still pass.
  - **Verification Command:** `devenv shell -- npm run test:e2e:guest`
  - **Dependencies:** Tasks 6, 8, 18, 19, 20

- [ ] **Task 22: Run the full gates and check acceptance criteria**
  - **Scope:** S
  - **Description:** Run each gate in `CONSTRAINTS.md` §2 as its own command, because `sanity-check` covers only format, clippy, compile and the test matrix. The panic and env-var checks already fail on the baseline, mostly in test code, so check only what this branch adds, and report the baseline failure to the originator. `-D missing_docs` stops at `common`, which has 487 baseline errors, so run it as a warning and compare the results with the lines this branch adds. Inline `#[cfg(test)]` modules in production files are also matched by the added-lines check, so write those tests without `.unwrap()` or review those hits by hand. `CONSTRAINTS.md` doesn't change. Check each item in spec §7 against evidence.
  - **Files Touched:**
    - None, unless a gate finds a problem
  - **Acceptance Criteria:**
    - [ ] `sanity-check` passes.
    - [ ] No float in `common/src/pricing.rs` or `app_api/booking_api/`.
    - [ ] No `.unwrap()`, `.expect()` or raw `env::var` literal in lines this branch adds to production code.
    - [ ] `missing_docs` reports nothing on items this branch adds. Any baseline failure is reported.
    - [ ] `cargo sqlx prepare --workspace --check` and `cargo audit` pass.
    - [ ] `get_public_stay_quote` isn't called from the listings page or card files.
    - [ ] The full Playwright suite passes, including admin and `ui-js`.
    - [ ] Spec §7 items are each confirmed, and no migration was added.
    - [ ] **Release precondition, recorded in the PR:** run these against production, read-only, with the originator. A row returned by the first or third needs a decision. The second shows listings whose gap takes effect at release.
      - `SELECT id, days_between_bookings FROM listing WHERE days_between_bookings NOT BETWEEN 0 AND 365`
      - `SELECT id, days_between_bookings FROM listing WHERE days_between_bookings > 0`
      - `SELECT id FROM listing WHERE weekly_discount_percentage NOT BETWEEN 0 AND 100 OR monthly_discount_percentage NOT BETWEEN 0 AND 100`
  - **Verification Command:**
    ```bash
    devenv shell -- sanity-check \
      && ! rg -q "f(32|64)" common/src/pricing.rs app_api/booking_api/ \
      && ! git diff fca80dea -U0 -- app_api common db_core web_client/src web_ui_common/src web_ui_guest/src web_ui_admin/src ':!*test*' | rg '^\+.*(\.(unwrap|expect)\(|env::var\("[A-Z0-9_]+"\))' \
      && devenv shell -- cargo sqlx prepare --workspace --check \
      && cargo audit \
      && ! rg -q get_public_stay_quote web_ui_guest/src/pages/listings.rs web_ui_common/src/components/domain/listing_card.rs web_ui_common/src/components/domain/villa_card.rs \
      && devenv shell -- npm run test:e2e
    # Compared by hand with the lines this branch adds, not a pass or fail:
    SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features --message-format=short -- -W missing_docs
    ```
  - **Dependencies:** Tasks 12, 21
