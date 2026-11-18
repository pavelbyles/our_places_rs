> **Engineering Review Status**: LOCKED
> **Review Date**: 2026-11-18
> **Review Panel**: Lead Architect, Edge Case Analyst, Security Reviewer, DB Guardian, QA Strategist

# Implementation Plan: Turnover & Cleaning Crew Dispatch View (#66)

**Spec Reference:** [`docs/specs/spec-66-feat-turnover-cleaning-crew-dispatch-view.md`](../specs/spec-66-feat-turnover-cleaning-crew-dispatch-view.md)
**Concepts Brief:** [`concepts_brief.md`](../../concepts_brief.md)
**Baseline Commit:** `cce8b255`
**Branch:** `feat/66-turnover-cleaning-crew-dispatch-view`

## Cross-Cutting Rules (Apply to Every Task)
- **Local Database & SQLx Metadata:** Run PostgreSQL, `sqlx`, and DB tests via `devenv shell -- <command>`. Whenever SQL queries change, regenerate offline metadata (`devenv shell -- cargo sqlx prepare --workspace -- --all-targets`) and commit `.sqlx/` updates with that task. Test-only fixture setup inside `#[cfg(test)]` should use runtime `sqlx::query(...).bind(...)` instead of un-prepared `query!` macros.
- **Task Gates (`CONSTRAINTS.md`):** After each task run `cargo fmt --check`, `SQLX_OFFLINE=true cargo clippy -p <touched crates> --all-targets --all-features -- -D warnings`, and `SQLX_OFFLINE=true cargo clippy -p <touched crates> --all-features -- -D missing_docs`. Every new public struct, enum, variant, field, and function requires a `///` doc comment.
- **Panic & Secret Hygiene:** Zero `.unwrap()` or `.expect()` in all newly added or modified files across production crates (`app_api/`, `common/`, `db_core/`, `web_client/src`, `web_ui_*/src`), including new test modules in `app_api/` and `db_core/` (tests must return `Result<(), Box<dyn std::error::Error>>` and use `?` / `.ok_or(...)`). Zero raw string literals in `env::var("KEY")`; zero plaintext tokens, OTPs, emails, or comment bodies in `tracing` logs or spans (log IDs, roles, and character counts only).
- **Lock Order & Lock Timeout:** Always execute `SET LOCAL lock_timeout = '5s'` (mapping SQLSTATE `55P03` to `DbError::LockContention` -> `503 ServiceUnavailable`) and acquire PostgreSQL `FOR UPDATE` row locks strictly in `Listing -> Booking / Feed -> Turnover` order across every turnover write transaction.
- **Migration Immutability:** Never modify previously applied files in `db_core/migrations/`; create a single new timestamped migration in Task 2.
- **TDD Flow:** Write failing unit/integration tests first inside the named test module before implementing production logic.

## Dependency Graph (DAG)

```text
Phase A: Foundations & Guard Rename (Risk-First)
T1  common UserRole::Staff + common::turnover domain/FSM  <-- None
T2  DB migration (staff enum, tokens, roster, turnovers)  <-- None
T3  Guard rename (staff -> host_or_admin) + AdminRole     <-- T1

Phase B: Password Setup & Forgot-Password Vertical Slice
T4  db_core::action_token + user_api password reset APIs  <-- T1, T2
T5  EmailTemplate variants + email_worker templates       <-- T1
T6  web_ui_admin /forgot-password, /reset-password, shell <-- T3, T4, T5

Phase C: Host-Scoped Staff Roster (/admin/staff-users)
T7  db_core::staff + user_api /api/v1/staff-users (Claims)<-- T2, T4, T5
T8  web_ui_admin /admin/staff-users + /admin/users keep   <-- T6, T7

Phase D: Eager Turnover Lifecycle & API Vertical Slice
T9  db_core::turnover CRUD + idempotent reconciler        <-- T1, T2
T10 Hook reconciler into booking/change/iCal/cron sweeps  <-- T9
T11 booking_api /api/v1/turnovers + app_client methods    <-- T3, T9

Phase E: Comments, Outbox Recovery & Mobile Dispatch UI
T12 Turnover comments API + general stuck outbox sweeper  <-- T5, T10, T11
T13 web_ui_admin /admin/turnovers mobile dispatch board   <-- T6, T11
T14 web_ui_admin /admin/turnovers/{id} comment thread UI  <-- T12, T13

Phase F: CI/CD Wiring, E2E Suite & Final Gates
T15 CI ADMIN_BASE_URL on user_api + ENVIRONMENT.md + .sqlx<-- T7, T12
T16 Playwright E2E suite (staff-users, reset, turnovers)  <-- T8, T14, T15
T17 Full CONSTRAINTS.md workspace verification gate       <-- T16
```

---

## Tasks

### Phase A: Foundations & Guard Rename (Risk-First)

- [x] **Task 1: Add `UserRole::Staff` and `common::turnover` Domain Types & Transition Rules**
  - **Scope:** S
  - **Description:** Add `UserRole::Staff` with `Display`/`FromStr` support in `common/src/models.rs`. Create `common/src/turnover.rs` defining `TurnoverStatus`, `TurnoverSourceKind`, `TurnoverActorRole`, `TurnoverResponse`, `TurnoverDetailResponse`, `TurnoverStatusHistoryEntry`, `TurnoverCommentResponse` (including `author_user_id: Option<Uuid>` and `author_name: String`), `TransitionTurnoverRequest` (including `expected_status: Option<TurnoverStatus>`), `StaffMemberSummary`, `InviteStaffRequest`, `UpdateStaffAssignmentsRequest`, `PasswordResetRequest`, `PasswordTokenRedeemRequest`, and pure `can_transition(actor_role, source_kind, from, to) -> Result<(), TurnoverTransitionError>` enforcing that `Staff` cannot enter `InspectedAndReady`, `Staff` can leave `InspectedAndReady`, and `Staff` can mark only `Manual` turnovers `NotRequired`.
  - **Files Touched:**
    - `common/src/models.rs`
    - `common/src/turnover.rs` (create)
    - `common/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] `UserRole::from_str("staff")` and `Display` round-trip cleanly.
    - [ ] Unit tests verify every allowed and forbidden transition across `Admin`, `Host`, and `Staff` for `booking`, `ical_feed`, and `manual` turnovers.
    - [ ] Every public item has `///` doc comments (`#![deny(missing_docs)]`).
  - **Verification Command:** `cargo test -p common turnover && SQLX_OFFLINE=true cargo clippy -p common --all-targets --all-features -- -D warnings && SQLX_OFFLINE=true cargo clippy -p common --all-features -- -D missing_docs`
  - **Dependencies:** None

- [x] **Task 2: Add Database Migration for `staff` Role, Tokens, Roster, and Turnovers**
  - **Scope:** M
  - **Description:** Create a new timestamped migration via `sqlx migrate add` extending `user_role` with `'staff'` (`ALTER TYPE user_role ADD VALUE IF NOT EXISTS 'staff'`) and creating `account_action_token`, `host_staff` (`invited_by UUID REFERENCES "user"(id) ON DELETE SET NULL`), `listing_staff` (`assigned_by UUID REFERENCES "user"(id) ON DELETE SET NULL`), `turnover` (`source_feed_id UUID REFERENCES listing_calendar_feed(id) ON DELETE SET NULL`, partial unique indexes on `booking_id` and `(listing_id, source_feed_id, source_external_uid)` plus `ON DELETE SET NULL` FKs), `turnover_status_history`, and `turnover_comment` (`author_user_id UUID REFERENCES "user"(id) ON DELETE SET NULL`, `author_name TEXT NOT NULL`). Update `db_core/src/models.rs` with `UserRole::Staff`.
  - **Files Touched:**
    - `db_core/migrations/<timestamp>_turnover_dispatch_and_staff.sql` (create)
    - `db_core/src/models.rs`
    - `db_core/src/turnover_migration_tests.rs` (create)
    - `db_core/src/lib.rs`
  - **Acceptance Criteria:**
    - [ ] `sqlx migrate run` succeeds on a clean database and on an existing migrated database.
    - [ ] Partial unique indexes reject duplicate `booking_id` or `(listing_id, source_feed_id, source_external_uid)` turnovers while allowing multiple `manual` turnovers.
    - [ ] Deleting a `listing_blocked_dates` row sets `turnover.blocked_date_id` to `NULL` without deleting the turnover or its comments, and hard-deleting an author/inviter user sets nullable FK references to `NULL` while preserving `author_name`.
  - **Verification Command:** `devenv shell -- cargo test -p db_core turnover_migration_tests`
  - **Dependencies:** None

- [x] **Task 3: Rename `staff` Guards to `host_or_admin` and Add `AdminRole::Staff`**
  - **Scope:** M
  - **Description:** Eliminate the `staff` naming collision before adding crew routes. Rename `web_ui_admin::support::staff` to `host_or_admin` across all existing privileged page handlers, and rename `booking_api`'s `require_staff` to `require_host_or_admin` in `mod.rs`, `promo_codes.rs`, and `exchange_rates.rs`. Add `AdminRole::Staff` in `web_client/src/session.rs` and update `portal_role` in `web_ui_admin/src/support.rs` (`Admin > Host > Staff`). Because `web_client::session::require_admin(cx)` accepts any `AdminRole`, ensure `web_ui_admin::support::host_or_admin(cx)` explicitly rejects `AdminRole::Staff` with `GuardError::Forbidden`, and `admin_only` rejects `AdminRole::Staff`.
  - **Files Touched:**
    - `web_client/src/session.rs`
    - `web_client/tests/session.rs`
    - `web_ui_admin/src/support.rs`
    - `web_ui_admin/src/pages/{dashboard.rs, pricing.rs, payouts.rs, promo_codes.rs, exchange_rates.rs, requests.rs}`
    - `web_ui_admin/src/pages/{bookings/*, listings/*}`
    - `app_api/booking_api/src/apis/{mod.rs, promo_codes.rs, exchange_rates.rs, authz_tests.rs}`
  - **Acceptance Criteria:**
    - [ ] Zero callers of the old `support::staff` or `require_staff` guard names remain.
    - [ ] Tests in `web_client/tests/session.rs` and `authz_tests.rs` confirm `AdminRole::Staff` is rejected by `host_or_admin` and `admin_only`.
    - [ ] Existing admin/host authorization tests continue passing unchanged.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_client -p web_ui_admin && devenv shell -- cargo test -p booking_api authz_tests`
  - **Dependencies:** Task 1

---

### Phase B: Password Setup & Forgot-Password Vertical Slice

- [x] **Task 4: Implement `db_core::action_token` and `user_api` Password Setup / Reset APIs**
  - **Scope:** M
  - **Description:** Create `db_core/src/action_token.rs` storing SHA-256 hashes of CSPRNG 32-byte tokens (`staff_invite` 72h, `password_reset` 30m), invalidating prior unused tokens for `(user_id, purpose)` on re-issue, and atomically consuming tokens via `UPDATE account_action_token SET used_at = NOW() WHERE token_hash = $1 AND used_at IS NULL AND expires_at > clock_timestamp() RETURNING user_id` while verifying `user.is_active = TRUE AND deleted_at IS NULL`. Add an unusable-password helper. Add `POST /api/v1/users/password-reset/request` (normalizing email via `.trim().to_lowercase()`, building reset URL strictly from `ADMIN_BASE_URL`, returning non-enumerating `202 Accepted`) and `POST /api/v1/users/password-reset/confirm` (hashing via `tokio::task::spawn_blocking` *before* opening the DB transaction, setting `is_verified = true`, revoking sessions via `SessionsDb::delete_user_sessions`) in `app_api/user_api/src/apis/account.rs` and `mod.rs`. Never log raw tokens or emails. Add a multi-threaded concurrency test proving two simultaneous token redemptions produce one success and one rejection.
  - **Files Touched:**
    - `db_core/Cargo.toml`
    - `db_core/src/action_token.rs` (create)
    - `db_core/src/lib.rs`
    - `app_api/user_api/Cargo.toml`
    - `app_api/user_api/src/apis/{account.rs, mod.rs, tests.rs}`
    - `.sqlx/`
  - **Acceptance Criteria:**
    - [ ] Expired, unknown, inactive-user, or already-used tokens return `400`/`401` and cannot be reused.
    - [ ] `#[tokio::test(flavor = "multi_thread")]` concurrency test proves 2 racing confirmations on the same token yield `1x 200 OK` and `1x 400/401`.
    - [ ] Consuming a valid token updates `password_hash`, marks `is_verified = true`, and deletes existing `sessions` rows for `user_id`.
    - [ ] Requesting a reset for a nonexistent, uppercase/whitespace, or soft-deleted email still returns `202 Accepted` cleanly.
  - **Verification Command:** `devenv shell -- cargo sqlx prepare --workspace -- --all-targets && devenv shell -- cargo test -p db_core action_token && devenv shell -- cargo test -p user_api password_reset`
  - **Dependencies:** Task 1, Task 2

- [x] **Task 5: Add `EmailTemplate` Variants and `email_worker` HTML Templates**
  - **Scope:** S
  - **Description:** Add `StaffInvite`, `StaffListingAssigned`, `PasswordResetLink`, and `TurnoverCommentNotification` to `common::email::EmailTemplate`. Create HTML templates in `app_api/email_worker/templates/` and register `app_api/email_worker/src/turnover_emails.rs` inside `processor.rs` (`RENDERERS` + `load_template`), escaping all dynamic values via `escape_html` and validating CTA links via `url::Url::parse` (`http`/`https` schemes only).
  - **Files Touched:**
    - `common/src/email.rs`
    - `app_api/email_worker/src/turnover_emails.rs` (create)
    - `app_api/email_worker/src/{processor.rs, tests.rs}`
    - `app_api/email_worker/templates/{staff_invite.html, staff_listing_assigned.html, password_reset_link.html, turnover_comment_notification.html}` (create)
  - **Acceptance Criteria:**
    - [ ] Every new template renders without falling back to `default_notification.html`.
    - [ ] Unit tests in `app_api/email_worker/src/tests.rs` verify XSS payloads in comment bodies/names are HTML-escaped and `javascript:` / non-`http(s)` URLs are stripped.
  - **Verification Command:** `cargo test -p common email && cargo test -p email_worker`
  - **Dependencies:** Task 1

- [x] **Task 6: Add `web_ui_admin` Forgot/Reset Password Pages, `Staff` Login Redirect, and Shell Isolation**
  - **Scope:** M
  - **Description:** Add `/forgot-password` and `/reset-password` Topcoat pages (`web_ui_admin/src/pages/password_reset.rs`), add `"Forgot password?"` link on `/login`, and wire `web_client/src/client.rs` methods. Update `web_ui_common/src/shell_admin.rs` (`admin_chrome`), `web_ui_admin/src/support.rs` (`nav_user`, `dispatch_actor`), `auth.rs`, `register.rs` (`verify_submit`), and `dashboard.rs` (`root_page`) so `AdminRole::Staff` skips `unread_guest_messages`, hides all non-dispatch links in desktop and mobile navs, `/` uses `303 see_other` (replacing `308 redirect_permanent` so browser cache cannot break role switches), and `/`, `/verify`, and `/login` route `AdminRole::Staff` directly to `/admin/turnovers`.
  - **Files Touched:**
    - `common/src/app_client/users.rs`
    - `web_client/src/client.rs`
    - `web_ui_common/src/shell_admin.rs`
    - `web_ui_admin/src/{support.rs, pages/mod.rs, pages/auth.rs, pages/dashboard.rs, pages/dashboard_tests.rs, pages/register.rs, pages/password_reset.rs (create)}`
  - **Acceptance Criteria:**
    - [x] Shell tests verify `AdminRole::Staff` renders `"Staff"` badge and only `Turnover Dispatch` and `Sign out` links in sidebar and mobile menu.
    - [x] `/`, `/login`, and `/verify` return `303` to `/admin/turnovers` for `AdminRole::Staff` and honor valid `/admin/turnovers*` redirect params while rejecting `/admin/bookings` redirects for `Staff`.
    - [x] `/reset-password?token=...` renders an inline recovery prompt when a token is expired or invalid.
  - **Verification Command:** `cargo test -p web_ui_common && SQLX_OFFLINE=true cargo test -p web_client -p web_ui_admin`
  - **Dependencies:** Task 3, Task 4, Task 5

---

### Phase C: Host-Scoped Staff Roster & Assignments (`/admin/staff-users`)

- [x] **Task 7: Implement `db_core::staff` and `user_api` `/api/v1/staff-users` Endpoints with `Claims` Auth**
  - **Scope:** M
  - **Description:** Create `db_core/src/staff.rs` for host-scoped staff listing/search (`host_staff` + `listing_staff` filtered to caller-owned listings unless `Admin`, escaping SQL `LIKE`/`ILIKE` wildcards `%`, `_`, `\` in `q`), idempotent race-safe (`ON CONFLICT` / retry) invite-or-attach (normalizing `email` via `.trim().to_lowercase()`, requiring non-empty `listing_ids` where `listing.deleted_at IS NULL`, creating `'staff'` user with locked password sentinel when new, or appending `'staff'` without overwriting PII/password/`is_active` when existing), invalidating prior unused `staff_invite` tokens before queuing `StaffInvite` (via `ADMIN_BASE_URL`) or `StaffListingAssigned`, updating `listing_staff` for caller-owned listings, and safely unlinking a host's staff (removing `'staff'` from `"user".roles` and calling `SessionsDb::delete_user_sessions(staff_user_id, Some("admin"))` when no remaining `host_staff` or `listing_staff` rows exist). Mount `/api/v1/staff-users*` in `app_api/user_api/src/apis/staff.rs` protected by `api_core::auth::require_user` + `Claims`. Add multi-threaded race test for concurrent invites of the same new email.
  - **Files Touched:**
    - `db_core/src/staff.rs` (create)
    - `db_core/src/lib.rs`
    - `app_api/user_api/src/apis/staff.rs` (create)
    - `app_api/user_api/src/apis/{mod.rs, tests.rs}`
    - `.sqlx/`
  - **Acceptance Criteria:**
    - [x] Unauthenticated requests or `Booker`/`Staff` tokens to `/api/v1/staff-users*` receive `401`/`403`, and empty `listing_ids` or soft-deleted/unowned `listing_id` is rejected (`400`/`403`).
    - [x] Host B inviting Host A's cleaner email succeeds, adds Host B's listing only, never shows Host A's listings to Host B, and Host B removing the cleaner leaves Host A's assignment untouched while unlinking the last host/listing removes `'staff'` and revokes admin sessions.
    - [x] `#[tokio::test(flavor = "multi_thread")]` test confirms concurrent invites of the same new email converge cleanly without `500` errors.
  - **Verification Command:** `devenv shell -- cargo sqlx prepare --workspace -- --all-targets && devenv shell -- cargo test -p db_core staff && devenv shell -- cargo test -p user_api staff`
  - **Dependencies:** Task 2, Task 4, Task 5

- [ ] **Task 8: Build `/admin/staff-users` Portal UI and Preserve `Staff` Role on `/admin/users`**
  - **Scope:** M
  - **Description:** Add `common::app_client` and `web_client` methods for staff roster endpoints. Create `web_ui_admin/src/pages/staff_users.rs` (`host_or_admin` guard) with invite form, owned-listing multi-checkbox selector, resend-invite action, and remove-from-roster confirmation dialog. Add `"Staff & Crew"` link to `shell_admin.rs` for `Host | Admin`. Update `UserForm::validate` in `web_ui_admin/src/pages/users/mod.rs`, `users/form.rs`, and `user_api/src/apis/profile.rs` so editing a `Staff`-only or multi-role `Staff` user via `/admin/users` retains `'staff'` in `user.roles` without failing the 3-checkbox `"Choose at least one role."` validation.
  - **Files Touched:**
    - `common/src/app_client/users.rs`
    - `web_client/src/client.rs`
    - `web_ui_common/src/shell_admin.rs`
    - `web_ui_admin/src/pages/{mod.rs, staff_users.rs (create), users/mod.rs, users/form.rs}`
    - `app_api/user_api/src/apis/profile.rs`
  - **Acceptance Criteria:**
    - [ ] `Staff` sessions get `403` on `/admin/staff-users` and `/admin/users`.
    - [ ] Host sees only their own staff and owned listings on `/admin/staff-users`.
    - [ ] Editing a `Staff`-only, `Host + Staff`, or `Booker + Staff` user in `/admin/users` succeeds and retains `'staff'` in `user.roles`.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_common -p web_client -p web_ui_admin && devenv shell -- cargo test -p user_api`
  - **Dependencies:** Task 6, Task 7

---

### Phase D: Eager Turnover Lifecycle & API Vertical Slice

- [ ] **Task 9: Implement `db_core::turnover` Repository and Idempotent Stay Reconciler**
  - **Scope:** M
  - **Description:** Create `db_core/src/turnover.rs` implementing: listing-scoped dispatch queries (`today | upcoming | past | all`, status, listing, pagination) with dynamic `next_checkin_date` projection across active bookings and `listing_blocked_dates`, manual turnover creation, role-checked status transition + `expected_status: Option<TurnoverStatus>` optimistic concurrency (`DbError::Conflict` on mismatch, idempotent no-op if `from == to`) + `turnover_status_history` insert, `reconcile_for_booking`, and `reconcile_for_listing` (`ical_feed` blocks only, ignoring `manual` blocks, shifting `checkout_date` in place on extensions, and transitioning `needs_cleaning` turnovers for removed `ical_feed` events to `not_required` **only when `turnover.checkout_date > today_jm`** via `common::reference::listing_today(Utc::now())` and `NO_EVENTS_WARNING` is false). Execute `SET LOCAL lock_timeout = '5s'` (mapping `55P03` -> `DbError::LockContention`) and acquire `SELECT id FROM listing WHERE id = $1 FOR UPDATE` *before* locking `turnover` in every write transaction. Include multi-threaded concurrency tests (`#[tokio::test(flavor = "multi_thread")]`).
  - **Files Touched:**
    - `db_core/src/turnover.rs` (create)
    - `db_core/src/error.rs`
    - `db_core/src/lib.rs`
    - `db_core/src/turnover_tests.rs` (create)
    - `.sqlx/`
  - **Acceptance Criteria:**
    - [ ] `#[tokio::test(flavor = "multi_thread")]` concurrency tests confirm racing status transitions (`expected_status` conflict vs idempotent no-op) and concurrent `reconcile_for_booking` / `reconcile_for_listing` never create duplicate rows or `40P01` deadlocks.
    - [ ] Extending a booking or iCal block updates `checkout_date` in place while keeping existing `CleaningInProgress` / `MaintenanceRequired` status and comments intact.
    - [ ] `reconcile_for_listing` does not mark a removed `ical_feed` turnover `not_required` when `turnover.checkout_date <= today_jm`, and `manual` calendar blocks never auto-create turnovers.
  - **Verification Command:** `devenv shell -- cargo sqlx prepare --workspace -- --all-targets && devenv shell -- cargo test -p db_core turnover_tests`
  - **Dependencies:** Task 1, Task 2

- [ ] **Task 10: Wire Eager Turnover Reconciliation into Booking Mutations, iCal Sync, and Sweeps**
  - **Scope:** M
  - **Description:** Invoke `db_core::turnover::reconcile_for_booking` inside `db_core/src/booking/update.rs::apply_status_side_effects` and `db_core/src/reservation_change.rs::approve_change_request`. Invoke `db_core::turnover::reconcile_for_listing` inside `db_core/src/calendar/reconcile.rs::reconcile_feed` before transaction commit. Add an idempotent turnover backfill sweep (under parent `listing` `FOR UPDATE` lock + `5s` `lock_timeout`) to `app_api/booking_api/src/apis/cron.rs::run_sweeps` with per-listing error isolation. Add tests verifying checkout-day `ical_feed` pruning (`e.date_to == today_jm`) preserves today's `needs_cleaning` turnover.
  - **Files Touched:**
    - `db_core/src/booking/update.rs`
    - `db_core/src/reservation_change.rs`
    - `db_core/src/calendar/reconcile.rs`
    - `db_core/src/calendar/reconcile_tests.rs`
    - `app_api/booking_api/src/apis/{cron.rs, tests.rs}`
    - `.sqlx/`
  - **Acceptance Criteria:**
    - [ ] Confirming a booking, approving a date change, or syncing an `ical_feed` block creates/updates the corresponding turnover in the same transaction.
    - [ ] Cancelling a booking or removing a future (`checkout_date > today_jm`) event from a non-empty `ical_feed` transitions `needs_cleaning` turnovers to `not_required`, while checkout-day `e.date_to == today_jm` pruning leaves `needs_cleaning` intact.
    - [ ] A per-listing error in `run_sweeps` logs an error and continues sweeping remaining listings without aborting stay completion.
  - **Verification Command:** `devenv shell -- cargo sqlx prepare --workspace -- --all-targets && devenv shell -- cargo test -p db_core && devenv shell -- cargo test -p booking_api cron`
  - **Dependencies:** Task 9

- [ ] **Task 11: Add `booking_api` `/api/v1/turnovers` Endpoints and `app_client` Methods**
  - **Scope:** M
  - **Description:** Create `app_api/booking_api/src/apis/turnovers.rs` and register routes in `routes.rs` for `GET /api/v1/turnovers`, `POST /api/v1/turnovers`, `GET /api/v1/turnovers/{id}`, and `POST /api/v1/turnovers/{id}/status`. Enforce `Claims` + listing-scoped access (`Admin`, `Host` owner, or assigned `Staff`), map invalid transitions to `403 TURNOVER_TRANSITION_FORBIDDEN`, map `DbError::Conflict` (`expected_status`) to `409 Conflict`, map `DbError::LockContention` to `503 ServiceUnavailable`, and verify zero guest PII or financial fields appear in JSON responses. Add typed `common/src/app_client/turnovers.rs` and `web_client` methods, plus API integration tests in `app_api/booking_api/src/apis/turnover_api_tests.rs`.
  - **Files Touched:**
    - `app_api/booking_api/src/apis/turnovers.rs` (create)
    - `app_api/booking_api/src/apis/turnover_api_tests.rs` (create)
    - `app_api/booking_api/src/apis/{mod.rs, routes.rs}`
    - `common/src/app_client/{mod.rs, turnovers.rs (create)}`
    - `web_client/src/client.rs`
  - **Acceptance Criteria:**
    - [ ] `Staff` receives `404`/`403` for listings they are not assigned to, and `403` when attempting to set `InspectedAndReady` or mark a booking/`ical_feed` turnover `NotRequired`.
    - [ ] `Staff` succeeds in transitioning `InspectedAndReady` -> `MaintenanceRequired` or `NeedsCleaning`.
    - [ ] Response payloads omit guest identity, booking totals, confirmation codes, and door codes, and stale `expected_status` returns `409 Conflict`.
  - **Verification Command:** `devenv shell -- cargo test -p booking_api turnover_api_tests && SQLX_OFFLINE=true cargo check -p web_client`
  - **Dependencies:** Task 3, Task 9

---

### Phase E: Comments, Outbox Recovery & Mobile Dispatch UI

- [ ] **Task 12: Implement Turnover Comments API and General Stuck `email_outbox` Republisher**
  - **Scope:** M
  - **Description:** Add `POST /api/v1/turnovers/{id}/comments` in `app_api/booking_api/src/apis/turnovers.rs` backed by `db_core::turnover::add_comment_with_outbox` (acquiring parent `listing` `FOR UPDATE` lock with `5s` `lock_timeout` before locking `turnover`, and persisting `author_name` snapshot). Resolve recipients as active verified `listing_staff` assigned to `listing_id` plus `listing.user_id`, excluding the comment author, insert `EmailTemplate::TurnoverCommentNotification` rows transactionally, and publish with `.instrument(tracing::Span::current())`. Leave `db_core::calendar::outbox::stuck_calendar_emails` untouched (preserving existing `outbox_tests.rs` and `calendar_sync_job` tests) and add `db_core::email_outbox::stuck_pending_emails(pool, older_than, limit)` called by `booking_api::cron::run_sweeps` (`EmailPublisher`).
  - **Files Touched:**
    - `db_core/src/turnover.rs`
    - `db_core/src/turnover_tests.rs`
    - `db_core/src/email_outbox.rs`
    - `app_api/booking_api/src/apis/{turnovers.rs, turnover_api_tests.rs, cron.rs}`
    - `.sqlx/`
  - **Acceptance Criteria:**
    - [ ] Posting a comment inserts `turnover_comment` (with `author_name` snapshot) and `email_outbox` rows atomically for host + other assigned active staff (never the author, never unassigned/inactive staff).
    - [ ] No comment body or email address appears in `tracing` logs.
    - [ ] Existing `db_core::calendar::outbox` and `calendar_sync_job` tests pass unmodified while `db_core::email_outbox::stuck_pending_emails` picks up stuck `staff_invite`, `password_reset_link`, and `turnover_comment_notification` rows.
  - **Verification Command:** `devenv shell -- cargo sqlx prepare --workspace -- --all-targets && devenv shell -- cargo test -p db_core outbox && devenv shell -- cargo test -p db_core turnover_tests && devenv shell -- cargo test -p booking_api turnover`
  - **Dependencies:** Task 5, Task 10, Task 11

- [ ] **Task 13: Build `web_ui_admin` `/admin/turnovers` Mobile-First Dispatch Board**
  - **Scope:** M
  - **Description:** Create `web_ui_admin/src/pages/turnovers.rs` (`dispatch_actor` guard) rendering a responsive Topcoat/DaisyUI card board with window tabs (`Today`, `Upcoming`, `Past`, `All`), status & listing filters, pagination, manual turnover creation form, and HTMX status transition buttons tailored to caller role (omitting `Inspected & Ready` action for `Staff` while enabling `Maintenance Required` / `Needs Cleaning` from `Inspected & Ready`). Render explicit error banners on API failures rather than `.unwrap_or_default()`. Link `/admin/turnovers` in `shell_admin.rs`.
  - **Files Touched:**
    - `web_ui_admin/src/pages/turnovers.rs` (create)
    - `web_ui_admin/src/pages/mod.rs`
    - `web_ui_common/src/shell_admin.rs`
  - **Acceptance Criteria:**
    - [ ] Mobile viewport (`390px`) renders stacked dispatch cards with ≥ 44px touch targets and zero horizontal overflow.
    - [ ] `Staff` sessions do not see `Inspected & Ready` trigger buttons or any revenue/booking/guest PII links.
    - [ ] Upstream API failure renders a visible alert banner (`role="alert"`) and logs `tracing::error!`.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_common -p web_ui_admin`
  - **Dependencies:** Task 6, Task 11

- [ ] **Task 14: Build `web_ui_admin` `/admin/turnovers/{id}` Detail & Comment Thread UI**
  - **Scope:** S
  - **Description:** Add `/admin/turnovers/{id}` and `POST /admin/turnovers/{id}/comments` handlers in `web_ui_admin/src/pages/turnovers.rs`, displaying turnover details, full allowed status controls, `turnover_status_history` timeline, and chronological comment thread reusing `web_ui_common::components::chat::{chat_thread, chat_bubble}` with HTMX partial updates (`data-reset-on-success`).
  - **Files Touched:**
    - `web_ui_admin/src/pages/turnovers.rs`
    - `web_ui_admin/tests/turnover_dispatch.rs` (create)
  - **Acceptance Criteria:**
    - [ ] Posting a comment appends a `chat_bubble` fragment without page reload and triggers backend outbox queueing.
    - [ ] Unauthorized listing IDs return `404`/`403` without leaking listing title or comments.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_admin --test turnover_dispatch`
  - **Dependencies:** Task 12, Task 13

---

### Phase F: CI/CD Wiring, E2E Suite & Final Gates

- [ ] **Task 15: Wire `ADMIN_BASE_URL` on `user_api`, Update `docs/ENVIRONMENT.md`, and Sync `.sqlx/`**
  - **Scope:** S
  - **Description:** Update `.github/workflows/cicd_full_stack.yml` (`configure-email-links`) so `ADMIN_BASE_URL=$ADMIN_URL` is injected onto `user_api` alongside `booking_api`, `listing_api`, and `calendar_sync_job`. Document `ADMIN_BASE_URL` for `user_api` in `docs/ENVIRONMENT.md`. Run `cargo sqlx prepare --workspace -- --all-targets` to verify all offline queries sync cleanly.
  - **Files Touched:**
    - `.github/workflows/cicd_full_stack.yml`
    - `docs/ENVIRONMENT.md`
    - `.sqlx/`
  - **Acceptance Criteria:**
    - [ ] `cicd_full_stack.yml` sets `ADMIN_BASE_URL` on the `user-api` Cloud Run service.
    - [ ] `devenv shell -- cargo sqlx prepare --workspace --check` passes with exit code `0`.
  - **Verification Command:** `devenv shell -- cargo sqlx prepare --workspace --check`
  - **Dependencies:** Task 7, Task 12

- [ ] **Task 16: Add Playwright E2E Suites for Staff Invites, Password Flows, Isolation & Dispatch Board**
  - **Scope:** M
  - **Description:** Add `playwright/e2e/admin-portal/staff-users.spec.ts` and `playwright/e2e/admin-portal/turnovers.spec.ts` verifying: (1) host invites new and shared multi-host cleaner email on `/admin/staff-users`, (2) cleaner redeems set-password link via `email_outbox` token and uses `/forgot-password`, (3) `Staff` lands on `/admin/turnovers` and receives `403`/blocked access on `/admin`, `/admin/bookings`, `/admin/listings`, `/admin/payouts`, `/admin/users`, `/admin/staff-users`, (4) mobile (`390x844`) dispatch board, status transitions including staff unsetting `Inspected & Ready`, manual turnover creation, and comment email outbox insertion, while (5) existing `users-admin.spec.ts` passes unchanged.
  - **Files Touched:**
    - `playwright/e2e/admin-portal/staff-users.spec.ts` (create)
    - `playwright/e2e/admin-portal/turnovers.spec.ts` (create)
  - **Acceptance Criteria:**
    - [ ] `users-admin.spec.ts`, `staff-users.spec.ts`, and `turnovers.spec.ts` pass on Chromium and Firefox (including mobile viewport checks).
  - **Verification Command:** `devenv shell -- npx playwright test playwright/e2e/admin-portal/users-admin.spec.ts playwright/e2e/admin-portal/staff-users.spec.ts playwright/e2e/admin-portal/turnovers.spec.ts`
  - **Dependencies:** Task 8, Task 14, Task 15

- [ ] **Task 17: Run Full `CONSTRAINTS.md` Workspace Gate**
  - **Scope:** S
  - **Description:** Run every check required by `CONSTRAINTS.md` §2 and `sanity-check-workflow` (`cargo fmt --check`, offline `cargo check --workspace --exclude protoproj --all-features --tests`, `cargo clippy -- -D warnings`, `cargo clippy -- -D missing_docs`, panic/env-var/float `rg` gates on touched files and crate `src/`, `cargo sqlx prepare --workspace --check`, `cargo audit`, isolated `test-api` backend suite, and isolated `test-web-ui` suite).
  - **Files Touched:**
    - None (verification only; fix any surfaced lint or doc gaps in touched files)
  - **Acceptance Criteria:**
    - [ ] All `CONSTRAINTS.md` §2 checks pass with zero warnings, zero `.unwrap()`/`.expect()` in touched/production code, and zero undocumented public APIs.
  - **Verification Command:** `cargo fmt --check && SQLX_OFFLINE=true cargo check --workspace --exclude protoproj --all-features --tests && SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features -- -D warnings && SQLX_OFFLINE=true cargo clippy --workspace --all-features -- -D missing_docs && ! git diff --name-only cce8b255...HEAD -- '*.rs' | xargs -r rg -q '\.(unwrap|expect)\(' && ! rg -q "f(32|64)" common/src/pricing.rs app_api/booking_api/ && ! rg -q 'env::var\("[A-Z0-9_]+"\)' app_api/ common/ db_core/ && devenv shell -- cargo sqlx prepare --workspace --check && cargo audit && devenv shell -- cargo test --verbose --workspace --exclude web_client --exclude web_ui_common --exclude web_ui_guest --exclude web_ui_admin --exclude protoproj && cargo test -p web_client -p web_ui_common -p web_ui_guest -p web_ui_admin --verbose`
  - **Dependencies:** Task 16
