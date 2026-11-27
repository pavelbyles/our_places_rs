# Implementation Plan: Optional Listing Slug Update When Listing Name Changes (#130)

**Spec Reference:** [`docs/specs/spec-130-feat-optional-listing-slug-update-when-listing-name-change.md`](../specs/spec-130-feat-optional-listing-slug-update-when-listing-name-change.md)
**Intent Reference:** [`docs/intent/intent-130-feat-optional-listing-slug-update-when-listing-name-change.md`](../intent/intent-130-feat-optional-listing-slug-update-when-listing-name-change.md)
**Baseline Commit:** `5cf75371`
**Branch:** `feat/130-optional-listing-slug-update-when-listing-name-change`

## Cross-Cutting Rules (Apply to Every Task)
- **Task Gates (`CONSTRAINTS.md`):** After each task run `cargo fmt --check`, `SQLX_OFFLINE=true cargo clippy -p <touched crates> --all-targets --all-features -- -D warnings`, and `SQLX_OFFLINE=true cargo clippy -p <touched crates> --all-features -- -D missing_docs`. Every new public struct, enum, variant, field, and function requires a `///` doc comment.
- **Panic Hygiene:** Zero `.unwrap()` or `.expect()` in production crates (`app_api/`, `common/`, `db_core/`, `web_client/src`, `web_ui_*/src`).

## Tasks

- [x] **Task 1: Strict Listing Slug Validation & Suggestion (`common`)**
  - **Scope:** S
  - **Description:** Add documented `validate_listing_slug(input: &str) -> Result<(), &'static str>` (strict 3–60 chars, lowercase ASCII `a-z`/`0-9` + single `-`, no whitespace/uppercase/punctuation, no leading/trailing/consecutive hyphens, rejects 36-char hyphenated and 32-char hex UUIDs) and `suggest_listing_slug(name: &str) -> Option<String>` (caps input at 255 chars, delegates to `slugify`, prefixes `v-` on UUID match within 60 chars, returns `None` if `< 3` chars) to `common/src/slug.rs`. Add `#[serde(default)]` `#[validate(custom(function = "validate_optional_listing_slug"))]` `pub slug: Option<String>` to `UpdatedListingRequest` in `common/src/models.rs` and update existing `UpdatedListingRequest` struct literals (`slug: None`, including `ValidListing::into_update` in `web_ui_admin/src/pages/listings/form.rs`) so the workspace compiles green.
  - **Files Touched:**
    - `common/src/slug.rs`
    - `common/src/models.rs`
    - `web_ui_admin/src/pages/listings/form.rs` (struct literal `slug: None` only)
  - **Acceptance Criteria:**
    - [x] Unit tests in `common/src/slug.rs` verify all valid/invalid slug cases (boundaries 2/3/60/61, whitespace, uppercase, punctuation, double/leading/trailing hyphens, 36-char and 32-char UUIDs) and `suggest_listing_slug`.
    - [x] `UpdatedListingRequest::validate()` rejects malformed `Some(slug)` with field-ready message and accepts `None`.
  - **Verification Command:** `cargo test -p common && SQLX_OFFLINE=true cargo check --workspace --all-features --tests && SQLX_OFFLINE=true cargo clippy -p common --all-targets --all-features -- -D warnings -D missing_docs`
  - **Dependencies:** None

- [x] **Task 2: Partial Unique Index Migration, Atomic DB Update & 409 API Contract (`db_core`, `api_core`, `listing_api`)**
  - **Scope:** M
  - **Description:** Create immutable migration via `sqlx migrate add listing_slug_partial_unique_index` dropping `CONSTRAINT listing_slug_unique` and creating `CREATE UNIQUE INDEX listing_slug_unique ON listing (slug) WHERE deleted_at IS NULL;`. Add `DbError::ListingSlugExists` (`db_core/src/error.rs`) and `UpdatedListing.slug` (`db_core/src/models.rs`). In `db_core/src/listing/write.rs`, add `AND deleted_at IS NULL` to `update_listing_with_conn`'s `SELECT ... FOR UPDATE` and `UPDATE`, skip unchanged slugs (`None`), validate changed slugs, add `slug = COALESCE($30, slug)`, map `23505` on `listing_slug_unique` to `DbError::ListingSlugExists` in both `update_listing_with_conn` and `create_listing`, and emit `tracing::info!` on slug change. Refresh `.sqlx/` via `cargo sqlx prepare --workspace`. Map `DbError::ListingSlugExists` to `409 Conflict` (`LISTING_SLUG_EXISTS`) in both `status_code()` and `error_response()` in `app_api/api_core/src/error.rs` (plus table test). Thread `req_data.slug` in `app_api/listing_api/src/apis.rs` and document `409` in `#[utoipa::path]`. Add DB and API integration tests.
  - **Files Touched:**
    - `db_core/migrations/<timestamp>_listing_slug_partial_unique_index.sql`
    - `db_core/src/error.rs`
    - `db_core/src/models.rs`
    - `db_core/src/listing/write.rs`
    - `db_core/src/listing/tests.rs`
    - `.sqlx/`
    - `app_api/api_core/src/error.rs`
    - `app_api/listing_api/src/apis.rs`
    - `app_api/listing_api/src/apis_test.rs`
  - **Acceptance Criteria:**
    - [x] Soft-deleted listings release their slugs and cannot be mutated via `update_listing_with_conn`.
    - [x] Duplicate active slug returns `DbError::ListingSlugExists` (`409 LISTING_SLUG_EXISTS`) and rolls back `listing_history` and `listing` atomically.
    - [x] Replaced slugs are immediately reusable; legacy short slugs succeed on unrelated edits when unchanged.
    - [x] `cargo sqlx prepare --workspace --check` passes.
  - **Verification Command:** `sqlx migrate run && cargo sqlx prepare --workspace --check && cargo test -p db_core -p api_core -p listing_api && SQLX_OFFLINE=true cargo clippy -p db_core -p api_core -p listing_api --all-targets --all-features -- -D warnings -D missing_docs`
  - **Dependencies:** Task 1

- [x] **Task 3: Remove Legacy Checkout Currency-Suffix Shim (`web_ui_guest`)**
  - **Scope:** XS
  - **Description:** Remove `split_currency_suffix` (lines 128–134) and its unconditional 301 redirect in `checkout_page` (lines 599–602) in `web_ui_guest/src/pages/checkout.rs`, retaining `redirect_permanent` and `active_currency_codes` for their remaining callers.
  - **Files Touched:**
    - `web_ui_guest/src/pages/checkout.rs`
  - **Acceptance Criteria:**
    - [x] `GET /checkout/{slug}` resolves directly as a listing slug without stripping `-usd`/`-jmd`/`-eur` suffixes.
    - [x] `web_ui_guest` tests and clippy pass with zero warnings.
  - **Verification Command:** `cargo test -p web_ui_guest && SQLX_OFFLINE=true cargo clippy -p web_ui_guest --all-targets --all-features -- -D warnings`
  - **Dependencies:** None

- [x] **Task 4: Edit-Only Admin Slug UX, Inline Errors & Stateless HTMX Suggestion (`web_ui_common`, `web_ui_admin`)**
  - **Scope:** M
  - **Description:** Add a delegated click branch in `web_ui_common/src/assets/ui.js` (inside the existing click listener at line 157) for `[data-apply-slug-target][data-apply-slug-value]` that populates the target input, focuses it, and clears children of `#listing-slug-suggestion` (`replaceChildren()`). In `web_ui_admin/src/pages/listings/form.rs`, add `pub slug: String` to `ListingFields`, add `#[default] slug_error: Option<String>` to `listing_form` (keeping parameter count within Clippy's limit), render the edit-only `/listings/` + `#listing-slug` input, `#listing-slug-suggestion`, `#listing-slug-error`, 404 warning, and HTMX attributes (`hx-get="/admin/listings/slug-suggestion"`, `hx-trigger="input changed delay:250ms"`, `hx-target="#listing-slug-suggestion"`, `hx-include="#listing-name, #listing-slug"`) on `#listing-name`, and add `split_api_rejection` (keeping `api_rejection_messages` intact for `create.rs`). In `web_ui_admin/src/pages/listings/edit.rs`, add stateless `GET /admin/listings/slug-suggestion` (`host_or_admin`-gated, zero DB/API calls) and update `POST /admin/listings/{id}/edit` to diff against authoritative DB `listing.slug`, validate changed slugs alongside `fields.validate(...)`, and route format/conflict errors to `#listing-slug-error`. Add unit tests in `form.rs`.
  - **Files Touched:**
    - `web_ui_common/src/assets/ui.js`
    - `web_ui_admin/src/pages/listings/form.rs`
    - `web_ui_admin/src/pages/listings/edit.rs`
  - **Acceptance Criteria:**
    - [x] Slug field and HTMX suggestion render only on `/admin/listings/{id}/edit`, never on create or clone.
    - [x] Clicking `"Use suggested URL"` populates `#listing-slug` without removing `#listing-slug-suggestion` from the DOM.
    - [x] Unchanged slugs send `slug: None`; invalid/duplicate slugs render inline at `#listing-slug-error` without partial saves.
  - **Verification Command:** `topcoat fmt --check && cargo test -p web_ui_common -p web_ui_admin && SQLX_OFFLINE=true cargo clippy -p web_ui_common -p web_ui_admin --all-targets --all-features -- -D warnings -D missing_docs`
  - **Dependencies:** Task 1, Task 2

- [x] **Task 5: Playwright E2E Coverage & Full Workspace Definition of Done (`playwright`)**
  - **Scope:** S
  - **Description:** Add Playwright E2E tests in `playwright/e2e/admin-portal/listings-admin.spec.ts` covering: (1) rename listing keeping default slug, (2) rename listing and click `"Use suggested URL"` confirming new guest URL loads and old URL returns 404, (3) manual slug edit without name change, and (4) invalid format and duplicate active slug rendering inline `#listing-slug-error` without persisting partial price changes. Run all `CONSTRAINTS.md` quality gates across the full workspace.
  - **Files Touched:**
    - `playwright/e2e/admin-portal/listings-admin.spec.ts`
  - **Acceptance Criteria:**
    - [x] All new and existing Playwright admin listing tests pass.
    - [x] Full workspace `cargo fmt --check`, `topcoat fmt --check`, `cargo sqlx prepare --workspace --check`, `cargo clippy` (`-D warnings`, `-D missing_docs`), panic check, and `cargo test --workspace` pass.
  - **Verification Command:** `cargo fmt --check && topcoat fmt --check && cargo sqlx prepare --workspace --check && SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings && SQLX_OFFLINE=true cargo clippy --workspace --all-features -- -D missing_docs && cargo test --workspace`
  - **Dependencies:** Task 1, Task 2, Task 3, Task 4
