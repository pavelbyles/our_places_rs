# Implementation Plan: iCal Calendar Synchronization (#58)

**Spec Reference:** [`docs/specs/spec-58-ical-calendar-synchronization-external-channel-manager.md`](../specs/spec-58-ical-calendar-synchronization-external-channel-manager.md) (rev 2)
**Intent:** [`docs/intent/intent-58-ical-calendar-synchronization-external-channel-manager.md`](../intent/intent-58-ical-calendar-synchronization-external-channel-manager.md)
**Baseline Commit:** `e84ffe3c`
**Branch:** `feat/58-ical-calendar-synchronization-external-channel-manager`

> **Engineering Review Status**: LOCKED (rev 2.1)
> **Review Date**: 2026-11-09
> **Review Panel**: Lead Architect, Edge Case Analyst, Security Reviewer, DB Guardian, QA Strategist

## Cross-Cutting Rules (apply to every task)
- **Local database**: Postgres runs in the devenv shell, not Docker. Run `sqlx`, `psql` and database tests through `devenv shell -- <command>`. If another agent's devenv build is running, wait for it to finish. Use `SQLX_OFFLINE=true` for compile-only checks.
- **SQLx metadata**: after changing a query, run `devenv shell -- cargo sqlx prepare --workspace -- --all-targets` and commit the `.sqlx/` changes with that task.
- **Task gates**: `cargo fmt --check`, `SQLX_OFFLINE=true cargo clippy -p <touched crates> --all-targets --all-features -- -D warnings` and `SQLX_OFFLINE=true cargo check --workspace --exclude protoproj --all-features --tests`. Every new public item gets a `///` comment.
- **Tests come first**: write the failing test, then the code. Test modules are **sibling files declared in the parent `mod.rs`** (`#[cfg(test)] mod x_tests;`), so the filter path is `<module>::x_tests`. Each task creates the module its command filters on. A filter that matches nothing reports "0 passed", and that doesn't count as green.
- **One filter per `cargo test`**: cargo accepts a single positional filter, so commands with several chain separate `cargo test` calls with `&&`.
- **Concurrency tests** use real connections, `sqlx_db_tester::TestPg` and `#[tokio::test(flavor = "multi_thread")]`, with no sleeps.
- **Fetch tests** use local **https** servers with `rcgen` certificates, an injected root CA, an injected `Resolve` and injected timeouts of about 200 ms. Never real DNS, never a real channel, and no 10 s waits.
- **Lock order**: listing → feed. Bookings and requests are read only. Every feed edit (create, update, toggle, delete) takes the listing lock first. Never hold a transaction across an HTTP fetch. Reconcile and manual-block transactions run `SET LOCAL lock_timeout = '5s'`. Liveness checks use `clock_timestamp()`.
- **Secrets in logs**: never log a feed URL. Use `feed_id` and `redact(url)` (host only). Use `#[instrument(skip_all, fields(…))]`. Call `.without_url()` on every `reqwest::Error`. Map `23505` before logging. Event text and guest PII never reach a log or span.
- **Settings**: `EA__CALENDAR_SYNC__*` comes from env only. Never write it into `config/*.toml`, because prod loads `Development.toml`.
- **Error mapping**: every new `DbError` or `ApiError` variant has explicit arms in `status_code` and `error_response`, with a stable `code`.
- **No panics or floats**: no `unwrap`/`expect` in production paths, and no `f32`/`f64`.
- **Existing tests**: update them only where the spec requires it (the §1.8 JWT changes), and never weaken an assertion.
- **Targeted tests while iterating**; the full suite runs once, in T27.

## Dependency Graph
Each line is `task ← dependencies`.

```text
Phase A: foundations (pure, no DB)
T1  migration + DbError variants          ← —
T2  listing_today + feed_url + blocklist  ← —
T3  ICS parser                            ← —
T4  ICS writer + ETag                     ← T3
T5  conflict rules + calendar DTOs        ← —

Phase B: the highest-risk pieces first
T6  GuardedFetcher + settings guard       ← T2
T7  blocks in availability.rs + parity    ← T1
T8  blocks in hold/request/accept/search  ← T7

Phase C: sync core
T9  feeds CRUD + claim (db)               ← T1, T5
T10 reconcile + failure recording         ← T3, T9
T11 conflicts upsert/list/ack             ← T5, T10
T12 manual blocks (db)                    ← T1, T2
T13 sync orchestration (api_core)         ← T6, T10, T11
T14 email templates + EmailTemplate       ← T11

Phase D: server-side listing authorization
T15 require_listing_manager + retrofit    ← —
T16 clients/portal/Playwright send JWT    ← T15

Phase E: API, export and UI
T17 feed + sync endpoints                 ← T13, T15
T18 block + conflict + overview API       ← T11, T12, T15
T19 export query + listing_api .ics       ← T1, T4
T20 calendar client fns + guest .ics      ← T16, T17, T18, T19
T21 admin page: export + feeds            ← T20
T22 admin page: blocks + conflicts        ← T21
T23 admin dashboard card                  ← T20

Phase F: job, deploy, verify
T24 calendar_sync_job crate + republish   ← T13, T14
T25 CI deploy + main-only scheduler       ← T24, T17, T21
T26 Playwright E2E                        ← T20, T22, T23
T27 docs + full gate                      ← T1–T26
```

---

## Phase A: Foundations

- [x] **Task 1: Calendar sync migration and error variants**
  - **Scope:** S
  - **Description:** Run `sqlx migrate add calendar_sync` and fill it with spec §3.1: 3 enums and 3 tables, `sync_generation`, `reopened_at`, the composite FKs `(feed_id, listing_id)`, the tightened source CHECK, the partial unique index on manual blocks, and all indexes.
    - **PostgreSQL versions**: Cloud SQL runs 17 and devenv runs 18. Both support `ON DELETE SET NULL (feed_id)`. **Use no PG 18-only features**, such as `RETURNING old.*`. `migration_tests` logs `server_version`. Pinning devenv to `postgresql_17` is left to the user, because it affects every worktree.
    - Add the Rust enum types (`sqlx::Type`) and the `DbError` variants `CalendarFeedLimit`, `CalendarFeedDuplicate`, `CalendarBlockManagedByFeed`, `CalendarBlockDuplicate`, and `CalendarSyncRateLimited { retry_after_secs }`. Map them in `api_core::error` (409 ×4, and 429 with `Retry-After`).
  - **Files Touched:**
    - `db_core/migrations/<ts>_calendar_sync.sql`
    - `db_core/src/error.rs`
    - `db_core/src/models.rs`
    - `app_api/api_core/src/error.rs` (new `mod calendar_error_tests`)
    - `db_core/src/calendar/mod.rs`
    - `db_core/src/calendar/migration_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Migrations apply on a fresh database.
    - [ ] The CHECKs and FKs refuse: a manual block with a feed or UID, an imported block without a UID or with a note, `date_to ≤ date_from`, a duplicate `(feed_id, external_uid)`, a duplicate manual range, and a block whose `listing_id` doesn't match its feed's.
    - [ ] Deleting a feed cascades its blocks and sets `feed_id` NULL on conflicts, which keep their `listing_id`.
    - [ ] `calendar_error_tests` check each variant's status, code and `Retry-After`.
  - **Verification Command:** `devenv shell -- cargo test -p db_core calendar::migration_tests && SQLX_OFFLINE=true cargo test -p api_core calendar_error_tests`
  - **Dependencies:** None

- [x] **Task 2: Jamaica "today", feed URL validation and the IP blocklist**
  - **Scope:** S
  - **Description:** Add `common::reference::{LISTING_UTC_OFFSET_SECS, listing_today}`, and `common::calendar::feed_url::{validate_feed_url, ip_allowed, redact, FeedUrlError}` per spec §3.2. Check IP literals with `url::Host::Ipv4`/`Host::Ipv6`, never `host_str().parse()`. Use the full IPv4 and IPv6 blocklist, including mapped, compatible, NAT64 and 6to4 addresses, with the metadata IPs always blocked.
  - **Files Touched:**
    - `common/src/reference.rs` (tests added in its existing `mod tests`)
    - `common/src/calendar/mod.rs`
    - `common/src/calendar/feed_url.rs`
    - `common/src/calendar/feed_url_tests.rs`
    - `common/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] `listing_today` at 04:59Z gives the previous Jamaica date, and at 05:00Z the current one.
    - [ ] The following are refused: http, user info, the self URL, every blocklist range, and `[::ffff:7f00:1]`, `2130706433`, `0x7f.1`, `017700000001`, `64:ff9b::7f00:1` and `2002:7f00:1::`.
    - [ ] Another listing's export URL is allowed.
    - [ ] `redact` returns the host only.
  - **Verification Command:** `cargo test -p common calendar::feed_url_tests && cargo test -p common reference::tests::listing_today`
  - **Dependencies:** None

- [x] **Task 3: ICS feed parser**
  - **Scope:** M
  - **Description:** `common::calendar::ics_parse::parse_feed(&[u8], …)`, implementing every spec §1.3 rule:
    - UTF-8 required and the BOM stripped.
    - `END:VCALENDAR` required and `VEVENT`s balanced, so a **truncated body fails**.
    - Only top-level `VEVENT` properties are read; `VTIMEZONE`/`VALARM` are skipped.
    - `T000000Z` is treated as a DATE. Inverted or empty ranges become 1 night. `DURATION` is supported. A missing `DTSTART` gives a warning.
    - `CANCELLED` events, the `RRULE` warning, and `@ourplaces` UIDs skipped.
    - The window, with only `date_to` clipped and `checked_add_months(24)`.
    - **UID normalisation** that guarantees unique UIDs of ≤ 512 bytes: hashing, derivation, numbering in (date_from, date_to) order, and moving to the next suffix on a collision.
    - 5,000 events and 8 KB lines at most.

    No new dependency besides the existing `sha2`.
  - **Files Touched:**
    - `common/src/calendar/ics_parse.rs`
    - `common/src/calendar/ics_parse_tests.rs`
    - `common/tests/fixtures/ical/{airbnb,vrbo,google_rrule,timed,malformed,truncated,uids}.ics`
  - **Acceptance Criteria:**
    - [ ] The Airbnb and VRBO fixtures give the expected ranges.
    - [ ] Every rule row in spec §1.3, and every UID case in spec §6, has a test.
    - [ ] A property test, or exhaustive table, shows the output UIDs are always unique and ≤ 512 bytes.
    - [ ] A reordered feed gives the same UIDs.
    - [ ] Event text never appears in `ParsedFeed`.
  - **Verification Command:** `cargo test -p common calendar::ics_parse_tests`
  - **Dependencies:** None

- [x] **Task 4: ICS feed writer and ETag**
  - **Scope:** S
  - **Description:** `common::calendar::ics_write::{write_feed, etag}` (spec §1.6.2–3):
    - The headers, then events with `VALUE=DATE`, `STATUS:CONFIRMED` and `TRANSP:OPAQUE`.
    - CRLF line endings.
    - Every value escaped, the UID included.
    - Control characters stripped from names.
    - 75-octet folding that is safe for UTF-8.
    - An ETag computed with the `DTSTAMP` lines left out.
  - **Files Touched:**
    - `common/src/calendar/ics_write.rs`
    - `common/src/calendar/ics_write_tests.rs`
  - **Acceptance Criteria:**
    - [ ] A strict line check passes: every line ≤ 75 octets and ends in CRLF.
    - [ ] A UID containing `,;\` and a newline round-trips.
    - [ ] The ETag is stable across `stamp` changes and changes when an event changes.
    - [ ] `write → parse_feed` round-trips the ranges.
  - **Verification Command:** `cargo test -p common calendar::ics_write_tests`
  - **Dependencies:** T3

- [x] **Task 5: Conflict rules and calendar DTOs**
  - **Scope:** S
  - **Description:** Add `common::calendar::conflict::{ConflictKind::classify, StaleReason, stale_reason}`, and the spec §3.2 DTOs, each with `ToSchema`: `SyncOutcome` (`ok`/`warning`/`failed`/`skipped`/`deferred`), `Page<T>`, and `NewBlockedDateRequest` with validator rules.
  - **Files Touched:**
    - `common/src/calendar/conflict.rs`
    - `common/src/calendar/conflict_tests.rs`
    - `common/src/calendar/dto_tests.rs`
    - `common/src/models.rs`
  - **Acceptance Criteria:**
    - [ ] `classify` gives overlap, turnover or none for gaps of 0, 1 and 3, including touching ranges.
    - [ ] `NewBlockedDateRequest` refuses the past, `to ≤ from`, more than 365 nights, beyond 24 months, and a note over 500 chars.
  - **Verification Command:** `cargo test -p common calendar::conflict_tests && cargo test -p common calendar::dto_tests`
  - **Dependencies:** None

## Phase B: Highest-Risk Pieces First

- [x] **Task 6: SSRF-guarded feed fetcher and the cloud settings guard**
  - **Scope:** M
  - **Description:** Spec §3.4.
    - **The fetcher**: `api_core::calendar::fetch::GuardedFetcher::new(FetcherConfig)`, with injectable timeouts, root CA and `Resolve`.
      - https always.
      - The IP-literal check on the first URL and on every redirect target, because hyper skips the resolver for literals.
      - The resolver filter, `no_proxy`, a custom redirect policy (≤ 3 hops), and the streamed 2 MB cap.
      - `.without_url()` on every error, mapped to `FetchError`.
      - The `Accept` and `User-Agent` headers.
      - Built lazily with `OnceLock`.
    - **The settings**: `CalendarSyncSettings`, read from env only, plus a **startup guard** that refuses to run when `allow_private_hosts` or `extra_root_ca_path` is set while `K_SERVICE` or `CLOUD_RUN_JOB` is present. The guard is a pure function, `check_cloud_guard(&settings, env: impl Fn(&str) -> Option<String>)`, so tests never call `set_var`, which is unsafe in Rust 2024 and races with other tests.
    - **The test resolver hook** is a constructor argument only, never read from settings or env, and `ip_allowed` always **wraps** it.
    - **Dev-deps**: `rcgen`, and `tokio-rustls` or `axum-server` for the test https server.
  - **Files Touched:**
    - `app_api/api_core/src/calendar/mod.rs`
    - `app_api/api_core/src/calendar/fetch.rs`
    - `app_api/api_core/src/calendar/fetch_tests.rs`
    - `app_api/api_core/src/calendar/settings_tests.rs`
    - `app_api/api_core/src/settings.rs`
    - `app_api/api_core/Cargo.toml`
  - **Acceptance Criteria:**
    - [ ] With default settings these are refused before connecting: `https://127.0.0.1`, `https://[::ffff:7f00:1]`, `https://localhost` (via the injected resolver), a name resolving to `10.0.0.5`, and `169.254.169.254`, the last one even with `allow_private_hosts`.
    - [ ] The happy path against the rcgen https server works with `allow_private_hosts` and the injected CA.
    - [ ] A redirect to `http://` fails, and so does a redirect to an internal https host (E15).
    - [ ] A body over 2 MB gives `TooLarge`, and a stalled server gives `Timeout` (about 200 ms injected).
    - [ ] Header matchers check `Accept` and `User-Agent`.
    - [ ] The `FetchError` `Display`/`Debug` text never contains the path or query.
    - [ ] `settings_tests`: the guard refuses each toggle with `K_SERVICE` set and with `CLOUD_RUN_JOB` set.
  - **Verification Command:** `cargo test -p api_core calendar::fetch_tests && cargo test -p api_core calendar::settings_tests`
  - **Dependencies:** T2

- [x] **Task 7: Blocks in `availability.rs`, the exclude-own-nights rule and parity**
  - **Scope:** M
  - **Description:** Add the `listing_blocked_dates` branch to `check_availability`, `check_availability_excluding` and `get_unavailable_ranges`, using the spec §1.2.4 rule with the gap for `ical_feed` only and **the bounding predicates**. In `check_availability_excluding`, test blocks only against nights the excluded booking doesn't already hold (spec §1.2.5). Update the module doc. Add `Kind::ImportedBlock` and `Kind::ManualBlock` rows, with gap cases, to `availability_parity_tests`.
  - **Files Touched:**
    - `db_core/src/booking/availability.rs`
    - `db_core/src/booking/availability_parity_tests.rs`
    - `db_core/src/booking/unavailable_ranges_tests.rs`
  - **Acceptance Criteria:**
    - [ ] With gap = 2, an imported block makes the 2 days on each side unavailable, and a manual block doesn't.
    - [ ] `get_unavailable_ranges` returns the widened imported ranges and the raw manual ones.
    - [ ] A booking that overlaps a block can be shortened, or moved off the block, through `check_availability_excluding`, but can't be extended into it.
    - [ ] Parity rows cover **shortening at the start** and **shortening at the end** while inside a block. Both are allowed, which proves the empty-range guards work.
    - [ ] A gap change applies straight away (E13).
    - [ ] Existing parity rows are unchanged.
    - [ ] `EXPLAIN` in a test comment shows that `idx_blocked_dates_listing_range` is used.
  - **Verification Command:** `devenv shell -- cargo test -p db_core booking::availability_parity_tests && devenv shell -- cargo test -p db_core booking::unavailable_ranges_tests`
  - **Dependencies:** T1

- [x] **Task 8: Blocks in the hold, request, accept and search paths**
  - **Scope:** M
  - **Description:** Add the block rule to:
    - `booking/hold.rs`: a new statement after the `requested` check, under the listing lock
    - `booking_request/request.rs::check_dates_free`, which covers create, quote and convert
    - **`booking_request/respond.rs::accept_request`**: a new block check under its existing listing lock (spec §1.2.4)
    - `listing/queries.rs`: the search `NOT EXISTS`, with the bounding predicates

    Add the hold, request, accept and search columns to the parity table.
  - **Files Touched:**
    - `db_core/src/booking/hold.rs`
    - `db_core/src/booking_request/request.rs`
    - `db_core/src/booking_request/respond.rs`
    - `db_core/src/listing/queries.rs`
    - `db_core/src/booking/availability_parity_tests.rs`
    - `db_core/src/booking_request/{respond_tests,quote_tests,convert_tests}.rs`
  - **Acceptance Criteria:**
    - [ ] All 7 paths agree for every block row.
    - [ ] Creating, quoting, converting and **accepting** a request over a new block gives `DateUnavailable` (E11).
    - [ ] Instant-booking blocked nights is refused (E10).
    - [ ] The search query count is unchanged (#116 assertion).
  - **Verification Command:**
    ```bash
    devenv shell -- cargo test -p db_core booking::availability_parity_tests && \
    devenv shell -- cargo test -p db_core booking_request::respond_tests && \
    devenv shell -- cargo test -p db_core booking_request::quote_tests && \
    devenv shell -- cargo test -p db_core booking_request::convert_tests && \
    devenv shell -- cargo test -p db_core booking_request::hold_request_tests && \
    devenv shell -- cargo test -p listing_api stay_total_tests::query_count
    ```
  - **Dependencies:** T7

## Phase C: Sync Core

- [x] **Task 9: Feed storage and claiming**
  - **Scope:** S
  - **Description:** `db_core::calendar::feeds`:
    - `create`, under the listing lock: checks the limit and duplicates, and maps `23505` to `CalendarFeedDuplicate` before anything can log.
    - `list` and `get`, filtered by `listing_id`.
    - `update`: locks listing → feed; a URL change increments `sync_generation` and resets the status.
    - `delete`: locks listing → feed.
    - `claim_due` and `claim_for_listing`, using the spec §1.4.4 CTE (`ORDER BY … LIMIT … FOR UPDATE OF f SKIP LOCKED`) and returning `FeedClaim { id, listing_id, url, sync_generation, previous_attempt_at }`.
    - `release_claim`.

    The feed row type's `Debug` redacts the URL.
  - **Files Touched:**
    - `db_core/src/calendar/feeds.rs`
    - `db_core/src/calendar/feeds_tests.rs`
    - `db_core/src/calendar/mod.rs`
  - **Acceptance Criteria:**
    - [ ] A 6th feed gives `CalendarFeedLimit`. A duplicate URL gives `CalendarFeedDuplicate`, including through a forced `23505` race.
    - [ ] Another listing's feed id gives `None`.
    - [ ] A URL change increments the generation.
    - [ ] Two concurrent `claim_due` calls never claim the same feed (`multi_thread`).
    - [ ] A claim inside the interval returns nothing, and `release_claim` makes the feed claimable again.
    - [ ] `release_claim` doesn't touch a feed that was claimed again after this run's claim.
    - [ ] A URL change sets `last_attempt_at = NULL`, so the new URL is claimable immediately.
    - [ ] Feeds of deleted listings are skipped.
    - [ ] The redacted `Debug` output contains no URL path.
  - **Verification Command:** `devenv shell -- cargo test -p db_core calendar::feeds_tests`
  - **Dependencies:** T1, T5

- [x] **Task 10: Reconcile and failure recording**
  - **Scope:** M
  - **Description:** `db_core::calendar::reconcile::{reconcile_feed, record_failure}`, per spec §1.4.2–3:
    - `SET LOCAL lock_timeout`, with `55P03` becoming `Deferred`.
    - Locks listing → feed, then the **generation check**.
    - The `UNNEST` upsert, with `IS DISTINCT FROM`.
    - `DELETE … NOT (external_uid = ANY($uids::text[]))`, with the array never NULL.
    - The E2 warning and the status update.
    - Failure recording: a generation check, the count raised for scheduled runs only, one `calendar_feed_failing_host` outbox row at 4 failures, and `failure_alerted_at`.
  - **Files Touched:**
    - `db_core/src/calendar/reconcile.rs`
    - `db_core/src/calendar/reconcile_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Inserts, date changes and removals are reflected, and a second run changes nothing.
    - [ ] An empty `$uids` deletes every block on the feed (E2) and records the warning.
    - [ ] A failure keeps the blocks.
    - [ ] Exactly one outbox row at 4 scheduled failures, none for "Sync now" failures, and the alert is re-armed after a success.
    - [ ] A generation mismatch, a deleted feed or a disabled feed means nothing happens, for both functions (E5).
    - [ ] A lock held elsewhere for more than 5 s gives `Deferred` (E26).
    - [ ] Two reconciles of the same feed run one after the other (`multi_thread`).
  - **Verification Command:** `devenv shell -- cargo test -p db_core calendar::reconcile_tests`
  - **Dependencies:** T3, T9

- [x] **Task 11: Conflict upsert, listing and acknowledgement**
  - **Scope:** M
  - **Description:** `db_core::calendar::conflicts`:
    - `detect_for_feed(tx, feed_id, gap)`, using the spec §1.5.2 `ON CONFLICT (feed_id, external_uid, booking_id) DO UPDATE … WHERE IS DISTINCT FROM … RETURNING (xmax = 0), reopened`, with the reopen rules and `clock_timestamp()`. It's called from `reconcile_feed` and queues a `calendar_conflict_host` email only when a conflict opens or reopens.
    - `list_conflicts`, with `stale_reason` worked out in SQL and the history paginated 20 per page.
    - `acknowledge` and `overview_for_user`.
  - **Files Touched:**
    - `db_core/src/calendar/conflicts.rs`
    - `db_core/src/calendar/conflict_tests.rs`
    - `db_core/src/calendar/reconcile.rs`
  - **Acceptance Criteria:**
    - [ ] Overlap and turnover are detected, and an expired hold gives none.
    - [ ] A re-sync gives no new row or email.
    - [ ] **Remove then re-add** reopens the conflict with exactly one email.
    - [ ] **The same UID on two feeds** gives two rows.
    - [ ] **Turnover becoming overlap** reopens an acknowledged conflict with one email (E8).
    - [ ] `reopened` comes from `reopened_at = now()`, not from RETURNING on the updated row. A conflict acknowledged while no longer overlapping, whose event then moves back onto the booking, reopens with one email.
    - [ ] Each stale reason, `acknowledge` (repeating it is harmless, and listing B's conflict can't be acknowledged through A), and pagination.
    - [ ] **Race test**: a hold and a reconcile on the same nights give either a refused hold or a recorded conflict, never neither (`multi_thread`).
    - [ ] A conflicted pending hold still confirms.
  - **Verification Command:** `devenv shell -- cargo test -p db_core calendar::conflict_tests`
  - **Dependencies:** T5, T10

- [x] **Task 12: Manual blocks (db)**
  - **Scope:** S
  - **Description:** `db_core::calendar::blocks::{create_manual_block, delete_manual_block, list_blocks}`. Create takes the listing lock with `lock_timeout`, checks the exact nights against confirmed stays, live holds and live requests with `clock_timestamp()`, and maps the partial-unique violation to `CalendarBlockDuplicate`. `list_blocks` returns `feed_name`, `feed_active` and `note`.
  - **Files Touched:**
    - `db_core/src/calendar/blocks.rs`
    - `db_core/src/calendar/block_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Refused over a stay, hold or request. Allowed over gap days (E22) and over other blocks.
    - [ ] An exact duplicate gives `CalendarBlockDuplicate` (E27).
    - [ ] Deleting an imported block gives `ManagedByFeed`, and deleting through another listing gives not found.
  - **Verification Command:** `devenv shell -- cargo test -p db_core calendar::block_tests`
  - **Dependencies:** T1, T2

- [x] **Task 13: Sync orchestration**
  - **Scope:** M
  - **Description:** `api_core::calendar::sync::sync_feeds(pool, publisher, fetcher, claims, trigger, concurrency, deadline)`, per spec §1.4.5–7:
    - Fetch in parallel.
    - **Reconcile one feed at a time per listing.**
    - Use `spawn_blocking` for bodies over 64 KB.
    - Return `deferred` for anything unfinished at the deadline.
    - Publish after commit from a detached task; a failure is logged at `error`.
    - `#[instrument(skip_all, …)]` with per-feed child spans.
  - **Files Touched:**
    - `app_api/api_core/src/calendar/sync.rs`
    - `app_api/api_core/src/calendar/sync_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Ok, failing and malformed feeds in one call give 3 correct results, and only the ok feed writes blocks.
    - [ ] 5 feeds of one listing never use more than 1 pool connection at a time (pool high-water mark through `pool.size() - pool.num_idle()` sampling).
    - [ ] The injected deadline gives `deferred`, and the failure count is unchanged.
    - [ ] Publish runs only after commit (the test publisher counts calls), and a publish failure is logged at `error` without failing the result.
    - [ ] Captured logs (`tracing-test`) never contain the URL's path or query.
  - **Verification Command:** `devenv shell -- cargo test -p api_core calendar::sync_tests`
  - **Dependencies:** T6, T10, T11

- [ ] **Task 14: Email templates and `EmailTemplate` variants**
  - **Scope:** S
  - **Description:** Add `EmailTemplate::{CalendarConflictHost, CalendarFeedFailingHost}` and their payload structs in `common/src/email.rs`. Add the templates `calendar_conflict_host.html` and `calendar_feed_failing_host.html` (spec §3.7 keys, including `reopened`), registered in `processor.rs`, with every value going through `escape_html`.
  - **Files Touched:**
    - `common/src/email.rs`
    - `app_api/email_worker/templates/calendar_conflict_host.html`
    - `app_api/email_worker/templates/calendar_feed_failing_host.html`
    - `app_api/email_worker/src/processor.rs`
    - `app_api/email_worker/src/tests.rs`
  - **Acceptance Criteria:**
    - [ ] Both templates render every key.
    - [ ] A `<script>` in `listing_name` or `feed_name` is escaped.
    - [ ] No URL keys exist.
  - **Verification Command:** `devenv shell -- cargo test -p email_worker calendar`
  - **Dependencies:** T11

## Phase D: Server-Side Listing Authorization

- [ ] **Task 15: `require_listing_manager` and the existing write endpoints**
  - **Scope:** M
  - **Description:** Add `listing_api::authz::require_listing_manager` (spec §1.8.1: 401 before the lookup, then 404, then 403). Apply it to `PATCH`/`DELETE /listings/{id}`, image presign and delete, and price-override `POST`/`PUT`/`DELETE`. `create_listing` requires the Admin or Host role. A host is always the owner. An admin-set `user_id` must belong to an active Host or Admin, otherwise 422. Public `GET`s are unchanged.
  - **Files Touched:**
    - `app_api/listing_api/src/authz.rs`
    - `app_api/listing_api/src/authz_tests.rs`
    - `app_api/listing_api/src/apis.rs`
    - `app_api/listing_api/src/apis_test.rs` (mounted as `apis::tests`)
    - `app_api/listing_api/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] Every endpoint listed returns 401 with no JWT, 403 for another host, and 404 for a deleted listing, and succeeds for the owner and the admin.
    - [ ] A host creating a listing for someone else ends up as the owner.
    - [ ] An admin setting a guest as the owner gets 422.
    - [ ] Existing `apis::tests` pass once given JWTs.
  - **Verification Command:** `devenv shell -- cargo test -p listing_api authz_tests && devenv shell -- cargo test -p listing_api apis::tests`
  - **Dependencies:** None

- [ ] **Task 16: Clients and portal send the user JWT on listing writes**
  - **Scope:** M
  - **Description:** Add `user_jwt: &str` to the `common::app_client::listings` write functions and to `web_client` presign, sending it through `HttpClient::user_request`. Update the admin call sites to pass `service_token(cx, user)`. Update the Playwright API spec to send JWTs, with 401 and 403 cases.
  - **Files Touched:**
    - `common/src/app_client/listings.rs`
    - `web_client/src/client.rs`
    - `web_ui_admin/src/pages/listings/{edit,create,images}.rs`
    - `web_ui_admin/src/pages/pricing.rs`
    - `playwright/e2e/api/listing-api.spec.ts`
  - **Acceptance Criteria:**
    - [ ] The workspace compiles.
    - [ ] The portal tests pass.
    - [ ] The Playwright API spec passes.
    - [ ] Run locally, editing a listing, its pricing and its images works for the owner and the admin.
  - **Verification Command:** `cargo test -p web_client && cargo test -p web_ui_admin && devenv shell -- npx playwright test --config=playwright/playwright.config.ts --project=backend-api listing-api`
  - **Dependencies:** T15

## Phase E: API, Export and UI

- [ ] **Task 17: Feed management and "Sync now" endpoints**
  - **Scope:** M
  - **Description:** `listing_api::calendar` routes, per spec §3.5:
    - `GET`/`POST /feeds`. `POST` validates, saves, then runs a first sync through `claim_for_listing(60 s)`, and returns 201 even when that sync fails.
    - `PATCH /feeds/{feed_id}`: validates again, increments the generation, and syncs through the claim.
    - `DELETE /feeds/{feed_id}`.
    - `POST /sync`: claims, then runs `sync_feeds` with a 20 s deadline, returning `skipped` and `deferred` results, or 429 with `Retry-After`.

    Also add the OpenAPI entries, an `EmailPublisher` in app data, the settings guard at startup, and the `GuardedFetcher` `OnceLock`.
  - **Files Touched:**
    - `app_api/listing_api/src/calendar.rs`
    - `app_api/listing_api/src/calendar_tests.rs`
    - `app_api/listing_api/src/apis.rs`
    - `app_api/listing_api/src/main.rs`
    - `app_api/listing_api/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] 201 with a sync result against the rcgen fixture server.
    - [ ] 201 with `failed` against a refused URL.
    - [ ] 409 for the limit and for a duplicate. 422 for an invalid URL and for a self URL.
    - [ ] 401 without a JWT, and 403 for host A on listing B.
    - [ ] PATCH URL: validated (422 for a self URL), generation incremented, re-synced, and the old blocks kept when the new URL fails.
    - [ ] Toggling off keeps the blocks.
    - [ ] A second `POST /sync` or `POST /feeds` within 60 s is rate-limited, with `Retry-After`.
    - [ ] A 5-feed sync holds at most 1 connection and finishes within 20 s.
    - [ ] The 20 s deadline also applies to `POST /feeds` and to PATCH with a new URL. A reconcile already running when the deadline passes still commits, and its result is reported as completed.
  - **Verification Command:** `devenv shell -- cargo test -p listing_api calendar_tests::feeds`
  - **Dependencies:** T13, T15

- [ ] **Task 18: Block, conflict and overview endpoints**
  - **Scope:** M
  - **Description:** `GET`/`POST /blocks`, `DELETE /blocks/{block_id}`, `GET /conflicts?page`, `POST /conflicts/{id}/acknowledge`, and `GET /api/v1/calendar/overview` (scoped to the caller).
  - **Files Touched:**
    - `app_api/listing_api/src/calendar.rs`
    - `app_api/listing_api/src/calendar_tests.rs`
    - `app_api/listing_api/src/apis.rs`
  - **Acceptance Criteria:**
    - [ ] **Every** endpoint returns 401 without a JWT and 403 for host A on listing B. A child id from another listing gives 404.
    - [ ] Blocks: 201, 409 `DATE_UNAVAILABLE`, 409 `CALENDAR_BLOCK_DUPLICATE`, and 422.
    - [ ] Deleting an imported block gives 409 `CALENDAR_BLOCK_MANAGED_BY_FEED`.
    - [ ] Acknowledging is idempotent, and history is paginated.
    - [ ] Overview counts the caller's listings only, and all listings for the admin.
  - **Verification Command:** `devenv shell -- cargo test -p listing_api calendar_tests::blocks && devenv shell -- cargo test -p listing_api calendar_tests::conflicts && devenv shell -- cargo test -p listing_api calendar_tests::overview`
  - **Dependencies:** T11, T12, T15

- [ ] **Task 19: Export query and the `listing_api` `.ics` endpoint**
  - **Scope:** S
  - **Description:** `db_core::calendar::export::export_events`, with the spec §1.6.2 contents and exclusions and the raw dates. Add `GET /api/v1/listings/{id}/calendar.ics` with the §1.6.3 headers, the `ETag` and 304 handling, and no user JWT.
  - **Files Touched:**
    - `db_core/src/calendar/export.rs`
    - `db_core/src/calendar/export_tests.rs`
    - `app_api/listing_api/src/calendar.rs`
    - `app_api/listing_api/src/calendar_tests.rs`
  - **Acceptance Criteria:**
    - [ ] The export includes confirmed stays, live holds, manual blocks and imported blocks (original UID). It excludes expired or cancelled holds, past items and live requests.
    - [ ] Dates don't include the gap.
    - [ ] A booking with a known guest name, email and note exports none of them, but does include `Reservation #`.
    - [ ] A matching `If-None-Match` gives 304.
    - [ ] A deleted listing gives 404, and an inactive listing gives 200.
  - **Verification Command:** `devenv shell -- cargo test -p db_core calendar::export_tests && devenv shell -- cargo test -p listing_api calendar_tests::export`
  - **Dependencies:** T1, T4

- [ ] **Task 20: Calendar client functions and the public guest-portal `.ics` route**
  - **Scope:** M
  - **Description:** Add `common/src/app_client/calendar.rs` and the `web_client` wrappers: the user JWT on every management call, and `get_calendar_ics` through `get_request` with the service identity only. In `web_ui_guest`, add `pages/calendar_export.rs` with `#[route(GET "/listings/{id}/calendar.ics")]` and the spec §1.6.4 cache (60 s TTL, 1,000 entries, 404s cached, errors not).
    - A bad UUID or an upstream 404 gives 404.
    - An upstream 304 gives 304.
    - Any other upstream failure gives 503 without `Cache-Control`.
    - Inbound headers are never forwarded.

    The admin portal's "Sync now" call uses a per-request `.timeout(25 s)`. Add `wiremock` to the `web_ui_guest` dev-deps.
  - **Files Touched:**
    - `common/src/app_client/calendar.rs`
    - `common/src/app_client/mod.rs`
    - `web_client/src/client.rs`
    - `web_ui_guest/src/pages/calendar_export.rs`
    - `web_ui_guest/src/pages/calendar_export_tests.rs`
    - `web_ui_guest/src/pages/mod.rs`
    - `web_ui_guest/Cargo.toml`
    - `web_client/tests/calendar_client.rs`
  - **Acceptance Criteria:**
    - [ ] `web_client/tests/calendar_client.rs` (wiremock): management calls send `X-Forwarded-Authorization`, and `get_calendar_ics` sends none.
    - [ ] Tests against wiremock: 200 with the right headers and body, 304, 404, and 503.
    - [ ] A second request inside the TTL makes no upstream call, and that includes a 404.
    - [ ] An inbound `X-Forwarded-Authorization` header doesn't reach the upstream.
    - [ ] `/listings/{slug}` pages are unaffected.
  - **Verification Command:** `cargo test -p web_ui_guest calendar_export && cargo test -p web_client --test calendar_client`
  - **Dependencies:** T16, T17, T18, T19

- [ ] **Task 21: Admin calendar page, export and feeds sections**
  - **Scope:** M
  - **Description:** `web_ui_admin/src/pages/listings/calendar.rs`:
    - the `/admin/listings/{id}/calendar` page, guarded by `managed_listing` and linked next to "Pricing"
    - the export link with a copy button, from `GUEST_PORTAL_URL`
    - the imported calendars table: status badges and the error tooltip
    - add, edit, toggle and remove fragments
    - the "Sync now" button with `hx-indicator` and `hx-disabled-elt`, showing per-feed results including `skipped`/`deferred`, and the 429 message

    Errors show in a DaisyUI `alert`.
  - **Files Touched:**
    - `web_ui_admin/src/pages/listings/calendar.rs`
    - `web_ui_admin/src/pages/listings/calendar_tests.rs`
    - `web_ui_admin/src/pages/listings/mod.rs`
    - `web_ui_admin/src/pages/listings/edit.rs` (nav link)
    - `web_ui_admin/src/main.rs` (env)
  - **Acceptance Criteria:**
    - [ ] Fragment render tests for each feed status and each sync outcome.
    - [ ] Another host's listing gives 404.
    - [ ] Every form posts with the service token.
  - **Verification Command:** `cargo test -p web_ui_admin listings::calendar_tests`
  - **Dependencies:** T20

- [ ] **Task 22: Admin calendar page, blocks and conflicts sections**
  - **Scope:** M
  - **Description:**
    - **Blocks**: the manual block list and add/remove forms (`hx-disabled-elt`). Imported blocks listed read-only and grouped by feed, labelled "from a disabled calendar" when their feed is off.
    - **Conflicts**: open first, then stale greyed out with the reason, then acknowledged history collapsed and paginated. A "Reopened" badge, and an acknowledge button. A stale `event_removed` conflict from a deleted feed is labelled "calendar removed", so a re-added feed's new conflict isn't mistaken for a duplicate.
  - **Files Touched:**
    - `web_ui_admin/src/pages/listings/calendar.rs`
    - `web_ui_admin/src/pages/listings/calendar_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Render tests for the order, stale labels, reopened badge and disabled-feed label (E28).
    - [ ] A 409 shows inline.
    - [ ] Acknowledging swaps the row into history.
  - **Verification Command:** `cargo test -p web_ui_admin listings::calendar_tests`
  - **Dependencies:** T21

- [ ] **Task 23: Admin dashboard calendar card**
  - **Scope:** S
  - **Description:** A `/admin` card showing open, non-stale conflicts and feeds with `consecutive_failures ≥ 4`. It's hidden when both are 0, and hidden with a `warn` log when the overview fails.
  - **Files Touched:**
    - `web_ui_admin/src/pages/dashboard.rs`
    - `web_ui_admin/src/pages/dashboard_tests.rs` (declared in `pages/mod.rs`)
  - **Acceptance Criteria:**
    - [ ] Render tests for the card with counts, with zero counts (hidden), and with an error (hidden).
  - **Verification Command:** `cargo test -p web_ui_admin pages::dashboard_tests`
  - **Dependencies:** T20

## Phase F: Job, Deploy and Verify

- [ ] **Task 24: `calendar_sync_job` crate, stuck-email republish and Dockerfile**
  - **Scope:** M
  - **Description:** A new workspace member `app_api/calendar_sync_job`, per spec §1.4.8 and §3.7.
    - `run(config, deps) -> anyhow::Result<RunSummary>` does: the settings guard, `create_connection_pool_sized(concurrency + 2)` (new in `db_core/src/connection.rs`; no migrations), the publisher, and a loop of `claim_due(10 min, 32)` → `sync_feeds`.
    - The deadline is checked **before each feed**, and claims that never started go back through `release_claim`.
    - It republishes stuck `calendar_*` outbox rows (`db_core::calendar::outbox`), then logs the summary and flushes telemetry.
    - `main` maps `Err` to exit 1.

    Add `Dockerfile.calendar_sync_job`, modelled on the email worker's.
  - **Files Touched:**
    - `Cargo.toml` (members)
    - `db_core/src/connection.rs`
    - `db_core/src/calendar/outbox.rs`
    - `db_core/src/calendar/outbox_tests.rs`
    - `app_api/calendar_sync_job/{Cargo.toml,src/main.rs,src/run.rs,src/run_tests.rs}`
    - `Dockerfile.calendar_sync_job`
  - **Acceptance Criteria:**
    - [ ] `run_tests`: 2 due feeds (one ok, one failing) give the right summary.
    - [ ] An injected deadline releases claims that never started.
    - [ ] An unreachable DB makes `run` return `Err`.
    - [ ] A stuck calendar email is republished once, and non-calendar templates are left alone (`outbox_tests`).
    - [ ] `docker build -f Dockerfile.calendar_sync_job .` succeeds.
    - [ ] `devenv shell -- cargo run -p calendar_sync_job` exits 0 locally.
  - **Verification Command:** `devenv shell -- cargo test -p calendar_sync_job run_tests && devenv shell -- cargo test -p db_core calendar::outbox_tests`. Run `docker build -f Dockerfile.calendar_sync_job -t calendar-sync-job:local .` once, at the end of the task, not in the iteration loop.
  - **Dependencies:** T13, T14

- [ ] **Task 25: CI deploy with a dedicated service account and a Scheduler on `main` only**
  - **Scope:** M
  - **Description:** Spec §3.7.
    - **Bootstrap, done once by hand by the user**: create `calendar-sync-job-{dev,prod}` with its IAM bindings, and set the repo vars `INFRA_SA_CALENDAR_JOB_{DEV,PROD}`. The task writes this up in `docs/ENVIRONMENT.md`.
    - **`cicd_full_stack.yml`**:
      - `SA_CALENDAR_JOB`, plus a guard step that fails when it's empty.
      - Build and push the image.
      - `gcloud run jobs deploy calendar-sync-$SUFFIX` on every branch. The env has no JWT, GCS or SMTP variables, and sets `ALLOW_PRIVATE_HOSTS=false`.
      - On **main only**: `run.invoker` on the job, and the Scheduler `create|update` with `--oauth-service-account-email=$SA_CALENDAR_JOB`. Update the free-tier comment.
      - listing_api: the email topic env, `ALLOW_PRIVATE_HOSTS=false`, and `roles/pubsub.publisher`.
      - The admin portal: `GUEST_PORTAL_URL`.
      - An `actionlint` step.
    - **`cleanup_feature.yml`**: `gcloud run jobs delete`.
  - **Files Touched:**
    - `.github/workflows/cicd_full_stack.yml`
    - `.github/workflows/cleanup_feature.yml`
    - `docs/ENVIRONMENT.md` (bootstrap section)
  - **Acceptance Criteria:**
    - [ ] **Before merge, statically**:
      - `actionlint` passes.
      - `rg -U "Schedule Calendar Sync[\s\S]{0,200}if: github.ref == 'refs/heads/main'" .github/workflows/cicd_full_stack.yml` matches.
      - `rg -n "calendar-sync" .github/workflows/cicd_full_stack.yml | rg -v "refs/heads/main"` shows no scheduler command outside that step.
      - The job deploy step contains neither `JWT_SECRET` nor `GCS_`.
    - [ ] **Before merge, on the feature branch**: the push deploys the job, `gcloud run jobs execute calendar-sync-<suffix> --wait` succeeds, and `test "$(gcloud scheduler jobs list --location <region> --format='value(name)' | wc -l)" -le 2` passes (the dev and main sweeps only).
    - [ ] **After merge to `main`** (a checklist item for the PR): the `calendar-sync-main` Scheduler exists, `test "$(… | wc -l)" -eq 3` passes, and one scheduled execution succeeds. The CI actionlint step pins `rhysd/actionlint:1.7.7`. The bootstrap steps grant the Cloud Scheduler service agent `roles/iam.serviceAccountTokenCreator` on `SA_CALENDAR_JOB`, so it can mint OAuth tokens.
    - [ ] Running cleanup on the branch deletes the job.
  - **Verification Command:** `docker run --rm -v "$PWD":/repo -w /repo rhysd/actionlint:1.7.7 && rg -U "Schedule Calendar Sync[\s\S]{0,200}if: github.ref == 'refs/heads/main'" .github/workflows/cicd_full_stack.yml`, then after the push: `gcloud run jobs execute calendar-sync-<suffix> --region <region> --wait && test "$(gcloud scheduler jobs list --location <region> --format='value(name)' | wc -l)" -le 2`
  - **Dependencies:** T24, T17, T21

- [ ] **Task 26: Playwright E2E**
  - **Scope:** M
  - **Description:** Set up the HTTPS fixture:
    - **CA at a fixed path**: a devenv script `ical-fixture-ca` creates `$DEVENV_STATE/ical-fixture/{ca.pem,cert.pem,key.pem}` with openssl. It runs before `apis-start` and does nothing if the files already exist. devenv sets `EA__CALENDAR_SYNC__EXTRA_ROOT_CA_PATH=$DEVENV_STATE/ical-fixture/ca.pem` and `EA__CALENDAR_SYNC__ALLOW_PRIVATE_HOSTS=true` (local env only, never config files). The fetcher's `OnceLock` reads the CA on its first fetch.
    - **Fixture server**: global setup (`playwright/e2e/setup/ical-fixture-server.ts`) starts a Node `https` server with that cert and key, and **returns a teardown function**, so the server stays up for the whole run and is closed afterwards.
    - **`admin-portal/calendar-sync.spec.ts`**: add a feed and see its status `ok` and its blocks; "Sync now" twice to get the rate-limit message; add and remove a manual block; acknowledge a seeded conflict.
    - **`guest-portal/calendar-export.spec.ts`**: `.ics` content type, `Reservation #` present and the guest email absent; a blocked night greyed out and refused at checkout.

    First restart the APIs and portals from this worktree (`apis-stop`, `frontends-stop`, then `apis-start`).
  - **Files Touched:**
    - `playwright/e2e/admin-portal/calendar-sync.spec.ts`
    - `playwright/e2e/guest-portal/calendar-export.spec.ts`
    - `playwright/e2e/fixtures/ical/*.ics`
    - `playwright/e2e/setup/ical-fixture-server.ts`
    - `devenv.nix` (the `ical-fixture-ca` script and env)
    - seed helpers
  - **Acceptance Criteria:**
    - [ ] Both specs pass against freshly restarted services.
  - **Verification Command:** `devenv shell -- npx playwright test --config=playwright/playwright.config.ts --project=admin-portal-chromium calendar-sync && devenv shell -- npx playwright test --config=playwright/playwright.config.ts --project=guest-portal-chromium calendar-export`
  - **Dependencies:** T20, T22, T23

- [ ] **Task 27: Docs and full gate**
  - **Scope:** S
  - **Description:**
    - `ARCHITECTURE.md`: a new §4.x on calendar sync (flow, locks, generation and claim, SSRF guard); a known-limits note in §7 (about a 90-minute refresh interval at 1000×, with per-listing fan-out as a follow-up, and keeping an eye on `emails_republished`); the §2 and §3 crate tables (`calendar_sync_job`, and `api_core`'s calendar module), and §7 (the job, the main-only Scheduler, the manual trigger, `SA_CALENDAR_JOB`).
    - `ENVIRONMENT.md`: `GUEST_PORTAL_URL`, the `EA__CALENDAR_SYNC__*` variables (env only), and the `gcloud run jobs execute` how-to.
    - Regenerate `openapi.yaml`.
    - Update spec §6 test-module names if they drifted.
    - Run the full gate, then the full suite once, then the playwright-e2e skill. Record the sanity-check wall time.
  - **Files Touched:**
    - `docs/ARCHITECTURE.md`
    - `docs/ENVIRONMENT.md`
    - `openapi.yaml`
    - `docs/specs/spec-58-…md`
  - **Acceptance Criteria:**
    - [ ] Every `CONSTRAINTS.md` §2 gate passes, and so does `actionlint`.
    - [ ] Sanity-check wall time is recorded, with a target of ≤ 90 s.
    - [ ] `listing_api` cold start p50 is under 300 ms (`/cloudrun-perf-audit`).
  - **Verification Command:** `/sanity-check-workflow`, then `/playwright-e2e`
  - **Dependencies:** T1–T26
