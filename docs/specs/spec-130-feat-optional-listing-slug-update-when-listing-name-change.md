# Spec 130: Optional Listing Slug Update When Listing Name Changes

## Overview

Hosts and administrators need a safe way to update a listing's public URL slug (`listing.slug`) when renaming a property or cleaning up an existing slug. Today, `db_core` generates `listing.slug` once at creation and never modifies it on update. Automatically regenerating slugs on every name edit would silently break shared links and bookmarks, while keeping slugs immutable leaves renamed properties tied to stale URLs.

This specification adds an edit-only `"Listing URL"` slug control on `/admin/listings/{id}/edit` in `web_ui_admin` so `Admin` users and owning `Host` users can keep the current slug (default), edit it manually at any time, or adopt an optional slug suggestion derived from a changed listing name. Manually entered slugs are validated without silent normalization and surface inline field errors on format or uniqueness failures without partial saves. Replaced and soft-deleted slugs become immediately reusable (no redirect table, partial unique index `WHERE deleted_at IS NULL`), and the legacy currency-suffix redirect shim on `GET /checkout/{slug}` in `web_ui_guest` is removed so checkout URLs always resolve as listing slugs.

---

## Requirements

### Functional Requirements
1. **Edit-Only Slug Field (`web_ui_admin`):**
   - Display a `"Listing URL"` input (`id="listing-slug"`, `name="slug"`, prefixed with `/listings/`) inside the `"Basics"` fieldset on `/admin/listings/{id}/edit`.
   - Omit the slug input on listing creation (`/admin/listings/new`) and cloning (`/admin/listings/clone/{slug}`).
   - Display an inline warning beneath the slug input explaining that changing the URL immediately breaks existing links and bookmarks (404).
2. **Default Unchanged Behavior & Legacy Compatibility:**
   - Editing a listing without changing `#listing-slug` sends `slug: None` in `UpdatedListingRequest` and preserves `listing.slug` unchanged.
   - Older listings with legacy slugs (e.g., shorter than 3 characters) must still save unrelated edits without triggering slug validation errors.
3. **Manual Editing Anytime:**
   - Hosts and admins may edit `#listing-slug` on `/admin/listings/{id}/edit` even when the listing name has not changed.
4. **Optional Name-Change Slug Suggestion:**
   - Editing `#listing-name` triggers a debounced (`250ms`) HTMX request to `GET /admin/listings/slug-suggestion?name=...&slug=...`.
   - When the derived suggestion differs from the current `#listing-slug` input, render a suggestion chip with a `"Use suggested URL"` button (`type="button"`) that populates `#listing-slug` only when explicitly clicked (never auto-overwriting `#listing-slug`).
   - When JavaScript is disabled, `#listing-slug` remains manually editable.
5. **Strict URL-Safe Validation (No Silent Normalization):**
   - Manually entered slugs must satisfy all rules without rewriting:
     - Length between `MIN_SLUG_CHARS` (3) and `MAX_SLUG_CHARS` (60) characters inclusive.
     - Only lowercase ASCII letters (`a-z`), digits (`0-9`), and hyphens (`-`) — uppercase letters, spaces (including leading/trailing whitespace), and punctuation are rejected.
     - No leading hyphen, trailing hyphen, or consecutive hyphens (`--`).
     - Must not parse as a UUID (`uuid::Uuid::parse_str`, covering both 36-char hyphenated and 32-char hex forms).
   - Format violations render inline next to `#listing-slug` (`#listing-slug-error`) alongside any other form validation errors, preserving all submitted inputs and persisting nothing.
6. **Atomic Uniqueness & Immediate Reuse:**
   - Replace the full-table `listing_slug_unique` constraint with a partial unique index `WHERE deleted_at IS NULL` so soft-deleted listings release their slugs.
   - Uniqueness collisions inside `update_listing_with_conn` roll back the transaction atomically (including the `listing_history` snapshot) and return `409 Conflict` with `{ "code": "LISTING_SLUG_EXISTS", "message": "That listing URL is already in use by another listing." }`, rendered inline at `#listing-slug-error`.
   - Replaced slugs immediately return `404` on guest routes and become immediately available for other listings.
7. **Guest Checkout Slug Resolution (`web_ui_guest`):**
   - Remove `split_currency_suffix` and its unconditional 301 redirect in `GET /checkout/{slug}` so valid slugs ending in active currency codes (e.g., `villa-usd`, `house-jmd`) remain bookable.

### Exclusions / Non-Goals
- 301/302 redirects from old slugs to new slugs, or a historical slug redirect table.
- Reserving replaced slugs or enforcing cooldown windows before reuse.
- Custom slug entry on listing creation or cloning.
- Silent normalization of manually typed slugs.
- Changing UUID-keyed routes or entities (bookings, requests, enquiries, reviews, tax documents, iCal export URLs `/listings/{id}/calendar.ics`, admin detail/edit/pricing routes).
- Optimistic concurrency (`updated_at`) for concurrent edit tabs.

---

## Edge Cases & Failure States

1. **Legacy Non-Conforming Slugs on Unrelated Edits:**
   - `POST /admin/listings/{id}/edit` compares `fields.slug` against the authoritative DB `listing.slug` loaded via `managed_listing`. Equal values emit `slug: None` (skipping validation). Direct `PATCH /api/v1/listings/{id}` callers must similarly omit `slug` when unchanged; `Some(slug)` always runs validation.
2. **Soft-Deleted Listing Mutation:**
   - `update_listing_with_conn` adds `AND deleted_at IS NULL` to `SELECT ... FOR UPDATE` and `UPDATE` so soft-deleted rows cannot be mutated or write orphan `listing_history` rows.
3. **Create-Time Slug Collision:**
   - `create_listing` maps `23505` on `listing_slug_unique` to `DbError::ListingSlugExists` (`409 Conflict`) instead of `500` if a newly generated slug collides with an active manually claimed slug.
4. **Concurrent Slug Race:**
   - Two hosts simultaneously claiming the same slug serialize at PostgreSQL's `listing_slug_unique` partial unique index; the loser rolls back cleanly and receives `409 LISTING_SLUG_EXISTS`.
5. **Combined Validation Errors on POST:**
   - `update_listing` validates `fields.slug` (when changed) independently alongside `fields.validate(...)` so submitting both an invalid price and an invalid slug surfaces both errors on the same response without calling `listing_api`.
6. **Repeated HTMX Swaps After Clicking "Use suggested URL":**
   - `ui.js` clears only the children of `#listing-slug-suggestion` (`container.replaceChildren()`) rather than removing `#listing-slug-suggestion` from the DOM, keeping subsequent HTMX swaps functional.
7. **In-Flight Guest Checkout & Sitemap Cache:**
   - A guest submitting `POST /checkout/{old-slug}` concurrently with a slug rename receives `404`; `/sitemap.xml` caches for up to 1 hour (`max-age=3600`).
8. **Route Shadowing Audit:**
   - Verified `web_ui_guest/src/pages/` and `web_ui_admin/src/pages/listings/`: no static single-segment sibling routes exist under `/listings/{slug}`, `/checkout/{slug}`, or `/admin/listings/clone/{slug}`. UUID lookup precedence in `get_listing_by_id_or_slug` is guarded by rejecting UUID-shaped slugs.

---

## Technical Implementation

### Data Models & Schema
- **`common/src/slug.rs`**:
  - Keep `slugify` and `user_slug` unchanged.
  - Add `pub fn validate_listing_slug(input: &str) -> Result<(), &'static str>` (strict 3–60 chars, `[a-z0-9-]`, no leading/trailing/double `-`, no whitespace/uppercase, not `Uuid::parse_str`).
  - Add `pub fn suggest_listing_slug(name: &str) -> Option<String>` (caps input at 255 chars, delegates to `slugify`, prefixes `v-` if `Uuid::parse_str` matches while respecting `MAX_SLUG_CHARS`, returns `None` if `< MIN_SLUG_CHARS`).
- **`common/src/models.rs`**:
  - Add `#[serde(default)]` `#[validate(custom(function = "validate_optional_listing_slug"))]` `pub slug: Option<String>` to `UpdatedListingRequest`, mapping `validate_listing_slug` errors to `ValidationError::new("invalid_listing_slug").with_message(msg.into())`.
- **`db_core/migrations/<timestamp>_listing_slug_partial_unique_index.sql` (New)**:
  - `ALTER TABLE listing DROP CONSTRAINT listing_slug_unique;`
  - `CREATE UNIQUE INDEX listing_slug_unique ON listing (slug) WHERE deleted_at IS NULL;`
- **`db_core/src/error.rs` & `db_core/src/models.rs`**:
  - Add `DbError::ListingSlugExists` (`"That listing URL is already in use by another listing."`).
  - Add `pub slug: Option<String>` to `UpdatedListing`.
- **`db_core/src/listing/write.rs`**:
  - In `create_listing`: map `SQLSTATE_UNIQUE_VIOLATION` (`23505`) on `listing_slug_unique` to `DbError::ListingSlugExists`.
  - In `update_listing_with_conn`:
    1. Lock `SELECT ... FROM listing WHERE id = $1 AND deleted_at IS NULL FOR UPDATE`.
    2. Compute `effective_slug`: `None` if `updated_listing_data.slug` is `None` or equals `current.slug`; otherwise validate via `common::slug::validate_listing_slug` (mapping failure to `DbError::ValidationError`).
    3. Archive pre-update state to `listing_history`, then `UPDATE listing SET ..., slug = COALESCE($30, slug) WHERE id = $1 AND deleted_at IS NULL`.
    4. Map `23505` on `listing_slug_unique` to `DbError::ListingSlugExists` (atomic rollback of `tx`).
    5. Log `tracing::info!(listing_id = %id, old_slug = %current.slug, new_slug = %new_slug, "listing slug updated")` when `effective_slug.is_some()`.
  - Refresh `.sqlx/` via `cargo sqlx prepare --workspace`.

### API Endpoints
- **`app_api/api_core/src/error.rs`**:
  - Map `ApiError::Database(DbError::ListingSlugExists)` in **both** `ResponseError::status_code()` (`StatusCode::CONFLICT`) and `ResponseError::error_response()` (`{ "code": "LISTING_SLUG_EXISTS", "message": "That listing URL is already in use by another listing." }`).
- **`app_api/listing_api/src/apis.rs`**:
  - Thread `slug: req_data.slug` into `UpdatedListing` in `update_listing`.
  - Add `(status = 409, description = "Listing slug already in use")` to `#[utoipa::path]`.

### Frontend Views
- **`web_ui_common/src/assets/ui.js`**:
  - Inside the existing delegated `document.addEventListener("click", ...)` handler (line 157), handle `[data-apply-slug-target][data-apply-slug-value]`: assign value to the target input, focus it, and call `container.replaceChildren()` on `#listing-slug-suggestion`.
- **`web_ui_admin/src/pages/listings/form.rs`**:
  - Add `pub slug: String` to `ListingFields` (populated in `from_listing`, empty in `blank`) and `slug: None` in `ValidListing::into_update`.
  - Add `#[default] slug_error: Option<String>` to `listing_form` (keeping parameter count within Clippy's `too_many_arguments` limit and avoiding edits in `create.rs` or `pin_picker_tests`).
  - When `editing` is true:
    - Wire `#listing-name` with `hx-get="/admin/listings/slug-suggestion"`, `hx-trigger="input changed delay:250ms"`, `hx-target="#listing-slug-suggestion"`, `hx-include="#listing-name, #listing-slug"`.
    - Render `/listings/` + `#listing-slug` input (`name="slug"`, `maxlength="60"`), `#listing-slug-suggestion` container, inline `#listing-slug-error` (`role="alert"`), and inline 404 breakage warning.
  - Add `pub fn split_api_rejection(err: &dyn std::fmt::Display) -> Option<(Option<String>, Vec<String>)>` to route `LISTING_SLUG_EXISTS` and validator `"slug"` errors to `#listing-slug-error` without duplicating them in `#listing-form-errors` (keeping `api_rejection_messages` intact for `create.rs`).
- **`web_ui_admin/src/pages/listings/edit.rs`**:
  - Add stateless `#[page("/admin/listings/slug-suggestion")]` gated by `host_or_admin(cx).await?`, accepting `SlugSuggestionQuery { name: String, slug: Option<String> }`, returning the `"Use suggested URL"` fragment when `suggest_listing_slug(&q.name)` differs from `q.slug`.
  - In `update_listing`: compare `fields.slug` against authoritative DB `listing.slug`; validate changed slugs alongside `fields.validate(...)`; pass `request.slug = (fields.slug != listing.slug).then(|| fields.slug.clone())`; partition API rejections via `split_api_rejection` and pass `slug_error` to `listing_form` on re-render.
- **`web_ui_guest/src/pages/checkout.rs`**:
  - Remove `split_currency_suffix` (lines 128–134) and its redirect check in `checkout_page` (lines 599–602), retaining `redirect_permanent` and `active_currency_codes` for their remaining callers.

### Background Workers
- None affected.

---

## Performance Considerations

- **Zero DB/API Keystroke Overhead:** `GET /admin/listings/slug-suggestion` is stateless and pure in-memory (`O(min(n, 255))`), performing zero database or `listing_api` calls on debounced typing and avoiding scale-to-zero cold starts on `listing_api`.
- **Bounded Allocations:** `suggest_listing_slug` caps input at 255 characters before calling `slugify`, protecting the 256 MB Cloud Run RAM budget.
- **No-Op Index Writes:** Unchanged slugs pass `slug: None` (`COALESCE($30, slug)`), avoiding unnecessary B-tree index churn on routine edits.
- **Partial Unique Index:** `listing_slug_unique` (`WHERE deleted_at IS NULL`) is smaller than the previous full-table index and matches the `deleted_at IS NULL` predicate in `get_listing_by_id_or_slug`.

---

## Security Mitigations

- **Role & Ownership Enforcement (IDOR):** `POST /admin/listings/{id}/edit` and `PATCH /api/v1/listings/{id}` enforce `Admin` or owning `Host` access (`403` for `Staff`, `404` for non-owners). `GET /admin/listings/slug-suggestion` is `host_or_admin`-gated and takes no listing ID, eliminating per-listing IDOR/enumeration.
- **Authoritative Change Detection:** Original `listing.slug` is always read from `managed_listing` (DB-backed), never trusted from a hidden form input.
- **CSP Compliance:** Suggestion adoption uses `data-*` attributes handled by `web_ui_common/src/assets/ui.js` (`script-src 'self'`, zero inline event handlers).
- **SQL Injection & Tracing Hygiene:** All queries use parameterized `sqlx::query_as!`; `slug-suggestion` query strings are excluded from tracing spans.
- **Panic-Free Rust & Docs:** Zero `.unwrap()`/`.expect()` in production paths; `///` doc comments on all new public items (`-D missing_docs`).

---

## Test Plan

### Unit Tests
- **`common/src/slug.rs`** (`cargo test -p common`):
  - `validate_listing_slug`: valid slugs, length boundaries (2, 3, 60, 61), uppercase, leading/trailing/interior whitespace, punctuation, leading/trailing/consecutive hyphens, 36-char hyphenated UUID, and 32-char hex UUID.
  - `suggest_listing_slug`: accent folding, punctuation collapsing, 60-char trim without trailing hyphen, UUID `v-` prefixing, `< 3` char `None`, and >255 char truncation.
- **`app_api/api_core/src/error.rs`** (`cargo test -p api_core`):
  - Add `DbError::ListingSlugExists` to `error_status_code_and_payload_table` asserting `409`, `"LISTING_SLUG_EXISTS"`, and message.
- **`web_ui_admin/src/pages/listings/form.rs`** (`cargo test -p web_ui_admin`):
  - `split_api_rejection` partitioning for `LISTING_SLUG_EXISTS`, validator `"slug"` field errors, and mixed field errors; verify create/clone forms omit `#listing-slug`.

### Integration Tests
- **`db_core/src/listing/tests.rs`** (`cargo test -p db_core`):
  - Update slug successfully; duplicate active slug returns `DbError::ListingSlugExists` with atomic rollback (no partial field updates, no orphan `listing_history` row); replaced slug immediately reusable by another listing; soft-deleted listing's slug immediately reusable; updating soft-deleted listing returns `RowNotFound`; legacy short slug succeeds on unrelated edit when `slug: None` or unchanged.
- **`app_api/listing_api/src/apis_test.rs`** (`cargo test -p listing_api`):
  - `PATCH /api/v1/listings/{id}` returns `200` on valid slug change, `400` on invalid/UUID/whitespace slug, `409` (`LISTING_SLUG_EXISTS`) on duplicate active slug.
- **`web_ui_guest`** (`cargo test -p web_ui_guest`):
  - Verify existing checkout tests pass without `split_currency_suffix`.

### E2E Tests
- **`playwright/e2e/admin-portal/listings-admin.spec.ts`**:
  - Rename listing without clicking suggestion -> name updates, slug stays unchanged, old guest URL still loads.
  - Rename listing and click `"Use suggested URL"` -> slug updates, new guest URL loads, old guest URL returns `404`.
  - Manually edit slug without changing name -> slug updates and loads at new URL.
  - Submit invalid format slug + price change -> inline `#listing-slug-error` renders, inputs preserved, price not persisted.
  - Submit duplicate active slug + price change -> inline `#listing-slug-error` renders (`409`), price not persisted.

---

## Acceptance Criteria

- [ ] Renaming a listing on `/admin/listings/{id}/edit` offers an optional suggested slug derived from the new name while keeping the existing slug unchanged by default.
- [ ] Hosts and admins can manually edit a listing's slug on `/admin/listings/{id}/edit` without changing the listing name.
- [ ] Manually entered slugs are validated without silent normalization (3–60 chars, lowercase ASCII `a-z`/`0-9`/single `-`, no whitespace, non-UUID) with inline `#listing-slug-error` feedback.
- [ ] Unrelated edits on listings with legacy non-conforming slugs succeed without slug validation errors.
- [ ] Duplicate active slugs fail atomically with `409 Conflict` (`LISTING_SLUG_EXISTS`), inline error display, no partial save, and no orphan `listing_history` row.
- [ ] Replaced slugs and soft-deleted listings' slugs immediately return `404` on guest routes and are immediately reusable by other listings (`listing_slug_unique` partial index `WHERE deleted_at IS NULL`).
- [ ] `GET /checkout/{slug}` resolves strictly as a listing slug (`split_currency_suffix` removed) so slugs ending in currency codes remain bookable.
- [ ] All quality gates pass: `cargo fmt --check`, `topcoat fmt --check`, `cargo sqlx prepare --workspace --check`, `SQLX_OFFLINE=true cargo clippy --workspace --all-targets -- -D warnings`, `SQLX_OFFLINE=true cargo clippy --workspace --all-features -- -D missing_docs`, `cargo test --workspace`, and `npx playwright test playwright/e2e/admin-portal/listings-admin.spec.ts`.
