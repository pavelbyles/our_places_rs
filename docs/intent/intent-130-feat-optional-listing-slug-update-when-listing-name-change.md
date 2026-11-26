# Optional Listing Slug Update When Listing Name Changes (Issue #130)

*   **Author:** Pavel Byles
*   **Status:** Approved
*   **Date:** 2026-11-26

--------------------------------------------------------------------------------

## Overview

Hosts and administrators need a safe way to update a listing's public URL slug (`listing.slug`) when a property is renamed or when an existing slug needs cleanup. Today a listing's slug is generated once at creation and cannot be edited afterward. This feature adds an edit-only URL slug control on `/admin/listings/{id}/edit` in `web_ui_admin` so hosts and admins can keep the current slug (default), edit it manually at any time, or adopt an optional slug suggestion derived from a changed listing name.

Manually entered slugs are validated without silent normalization and show inline field errors when malformed. Uniqueness conflicts roll back atomically without partial saves, and replaced or soft-deleted slugs immediately return 404 and become reusable (no redirect table). The legacy currency-suffix redirect shim on guest checkout is also removed so `/checkout/{slug}` always resolves as a listing slug.

## The Problem

When a listing is created, `db_core` derives its `slug` from the initial listing name and never changes it on subsequent updates. This creates two conflicting user-facing failure modes:

*   **Stale or mismatched URLs:** If a host renames a villa (for example from a draft placeholder to its final marketing name) or wants to fix a typo in an existing slug, the public URL stays permanently tied to the old name across guest listing pages, admin clone links, and calendar export filenames.
*   **Risk of silent link breakage:** Automatically regenerating `listing.slug` on every name edit would silently break shared links, guest bookmarks, and search engine results whenever a host makes a minor title tweak.

Three existing implementation defects also block safe slug updates today:

*   **Unhandled conflict errors:** Listing updates do not map PostgreSQL unique-violation errors on `listing.slug`, causing duplicate slugs to surface as `500 Internal Server Error`.
*   **Soft-deleted listings hold slugs:** The `listing.slug` unique constraint applies to the full table, so soft-deleted listings permanently occupy slugs that public lookups ignore.
*   **Checkout currency-suffix redirect collision:** The guest checkout page strips `-<active_currency>` suffixes before looking up a listing, which permanently redirects valid custom slugs ending in a currency code (such as `villa-usd` or `house-jmd`) to a 404.

## Goals

*   Allow `Admin` users and the owning `Host` to edit a listing's `slug` on `/admin/listings/{id}/edit` either manually at any time (without requiring a name change) or by adopting an optional slug suggestion generated when the listing name changes.
*   Keep the existing `slug` unchanged by default unless the host or admin explicitly modifies it or adopts a suggested slug.
*   Ensure unrelated edits on existing listings with legacy non-conforming slugs continue to succeed without being blocked by new slug rules (unchanged slugs are treated as a no-op).
*   Enforce URL-safe listing slug validation (3–60 characters, lowercase ASCII letters and digits separated by single hyphens, no whitespace, and not shaped like a UUID) with no silent normalization and inline field-level error reporting.
*   Enforce slug uniqueness atomically in `listing_api` / `db_core` across active (non-deleted) listings, returning an inline error (`409 Conflict`, `LISTING_SLUG_EXISTS`) without saving partial listing changes or creating an orphan `listing_history` row.
*   Release slugs immediately when a listing is updated to a new slug or soft-deleted (`deleted_at IS NOT NULL`) so those slugs become available for other listings.
*   Remove the legacy currency-suffix redirect shim from guest checkout so `/checkout/{slug}` always resolves as a listing slug.
*   Display an inline warning on the edit form explaining that changing the slug immediately breaks existing links and bookmarks (404).

## Non-Goals

*   301/302 redirects from old slugs to new slugs, or maintaining a historical slug redirect table.
*   Reserving replaced slugs or enforcing cooldown periods before reuse.
*   Allowing custom slug entry during listing creation (`/admin/listings/new`) or cloning (`/admin/listings/clone/{slug}`).
*   Silently normalizing or rewriting manually typed slugs before saving.
*   Changing how bookings, enquiries, reviews, tax documents, or UUID-based admin routes identify listings.
*   Refactoring create-time slug generation (`generate_listing_slug_native`) beyond mapping duplicate-slug collisions to `409 Conflict`.

## Critical User Journeys (CUJs)

### CUJ 1: Host Renames a Listing and Keeps the Existing Slug (Default)

*   **Given:** A host owns a listing named `"Coral Cove Villa"` with slug `coral-cove-villa` and opens `/admin/listings/{id}/edit`.
*   **When:** The host changes the name to `"Coral Cove Estate"`, sees the optional suggestion `coral-cove-estate` without clicking to apply it, and clicks `"Save changes"`.
*   **Then:** The listing's name updates to `"Coral Cove Estate"`, its `slug` remains `coral-cove-villa`, and `/listings/coral-cove-villa` continues to work uninterrupted.

### CUJ 2: Host Renames a Listing and Accepts the Suggested Slug

*   **Given:** A host owns `"Coral Cove Villa"` (`coral-cove-villa`) and opens `/admin/listings/{id}/edit`.
*   **When:** The host changes the name to `"Coral Cove Estate"`, clicks the button to adopt the suggested slug `coral-cove-estate`, and clicks `"Save changes"`.
*   **Then:** The listing updates atomically to name `"Coral Cove Estate"` and slug `coral-cove-estate`; `/listings/coral-cove-estate` loads the listing, `/listings/coral-cove-villa` returns 404, and `coral-cove-villa` is immediately available for another listing.

### CUJ 3: Host Manually Sets a Custom Slug Without Changing the Name

*   **Given:** A host or admin opens `/admin/listings/{id}/edit` for an existing listing.
*   **When:** They leave the name unchanged, edit the slug field to a valid unused slug (e.g., `sea-grape-house`), and click `"Save changes"`.
*   **Then:** The listing's slug updates to `sea-grape-house` and becomes reachable at `/listings/sea-grape-house`.

### CUJ 4: Invalid Format Rejected Inline Without Partial Save

*   **Given:** A host opens `/admin/listings/{id}/edit`, changes the listing's nightly price, and enters a malformed slug (such as `"My Villa!"`, `"villa--one"`, `"ab"`, leading/trailing whitespace, or a UUID).
*   **When:** The host clicks `"Save changes"`.
*   **Then:** The form re-renders with an inline validation error next to the slug field, all submitted form inputs are preserved, and neither the slug nor the nightly price change is persisted.

### CUJ 5: Duplicate Active Slug Rejected Atomically Inline

*   **Given:** A host opens `/admin/listings/{id}/edit`, changes the listing's nightly price, and enters a slug already used by another active listing.
*   **When:** The host clicks `"Save changes"`.
*   **Then:** `listing_api` rolls back the database transaction and returns `409 Conflict` (`LISTING_SLUG_EXISTS`), the form re-renders with an inline error next to the slug field, no orphan `listing_history` row is written, and neither the slug nor the nightly price change is persisted.

### CUJ 6: Unrelated Edit on a Listing With a Legacy Slug

*   **Given:** An older listing in the database has a legacy slug shorter than 3 characters (e.g., `v1`) and the host opens `/admin/listings/{id}/edit`.
*   **When:** The host updates the description without touching the slug field and clicks `"Save changes"`.
*   **Then:** The edit succeeds without triggering a slug validation error, and the listing keeps `v1`.

## Technical Plan (Architecture)

The implementation spans five layers while preserving monorepo boundaries (shared domain logic in `common`, database schema and atomic transaction handling in `db_core`, HTTP error contracts in `api_core` / `listing_api`, and portal UI in `web_ui_admin` and `web_ui_guest`):

1.  **Shared Validation & Suggestion (`common`)**
    *   Extend `common/src/slug.rs` alongside the existing `slugify` helper (which already folds accented Latin characters, strips non-alphanumeric characters into single hyphens, trims leading/trailing hyphens, and caps length at 60 characters).
    *   Add a listing slug validator that rejects inputs without normalizing (length outside 3–60, any character outside lowercase ASCII `a-z`/`0-9`/`-` including whitespace, leading/trailing/consecutive hyphens, or any string parsing as a UUID since listing lookup tries UUID parsing before slug lookup).
    *   Add a listing slug suggester that derives a candidate slug from a name via `slugify`, prefixes `v-` (capped at 60 chars) if the result parses as a UUID, and returns `None` if fewer than 3 characters remain.
    *   Add optional `slug` to the listing update request DTO. Direct `PATCH /api/v1/listings/{id}` callers must omit `slug` (`None`) when not changing it; passing `Some(slug)` triggers validation at the API boundary.

2.  **Partial Unique Index Migration & Atomic DB Update (`db_core`)**
    *   Add a PostgreSQL migration replacing the full-table `listing_slug_unique` constraint with a partial unique index `WHERE deleted_at IS NULL` so soft-deleted listings immediately release their slugs.
    *   Guard listing updates so soft-deleted rows cannot be mutated (`AND deleted_at IS NULL` in the row-locked `SELECT ... FOR UPDATE` and `UPDATE`), skip unchanged slugs (protecting legacy slugs on unrelated edits), validate changed slugs as defense-in-depth, and map PostgreSQL `23505` violations on `listing_slug_unique` to a dedicated `ListingSlugExists` database error on both update and create paths.
    *   Because history archival and the listing update share one transaction, a slug conflict rolls back both with no partial save and no orphan history row. Log `listing_id`, `old_slug`, and `new_slug` at `tracing::info!` whenever a slug changes.

3.  **API Wiring & 409 Conflict Contract (`app_api/api_core`, `app_api/listing_api`)**
    *   Map `ListingSlugExists` to `409 Conflict` with JSON body `{ "code": "LISTING_SLUG_EXISTS", "message": "That listing URL is already in use by another listing." }`.
    *   Thread optional `slug` through the `PATCH /api/v1/listings/{id}` handler into `db_core` and document `409` in OpenAPI.

4.  **Edit-Only Admin Form UX & Stateless HTMX Suggestion (`web_ui_admin`, `web_ui_common`)**
    *   Add an edit-only `"Listing URL"` input inside the `"Basics"` fieldset of the shared listing form (omitted on create and clone), prefixed with `/listings/`, paired with an inline error alert slot and a 404 link-breakage warning.
    *   Add a stateless, listing-agnostic HTMX fragment endpoint `GET /admin/listings/slug-suggestion?name=...&slug=...`, gated by `host_or_admin` (the admin portal's session role guard that allows `Host` and `Admin` and returns `403` for `Staff`). Because it performs a pure in-memory string transform on bounded query input (max 255 chars) with zero database or `listing_api` calls, it has no per-listing IDOR surface and avoids hitting `listing_api` on every debounced keystroke.
    *   When the derived suggestion differs from the current slug input (`slug` query param), return a suggestion chip with a `"Use suggested URL"` button wired via `data-*` attributes to a CSP-compliant delegated click handler in `web_ui_common`'s `ui.js` (clearing only the chip inside the suggestion container so subsequent HTMX swaps keep working). When JavaScript is disabled, the slug input remains manually editable.
    *   On `POST /admin/listings/{id}/edit`, load the authoritative listing via `managed_listing` (the admin portal helper that fetches the listing from `listing_api` and verifies `Admin` or owning `Host` access, returning `404` for non-owners). Compare the submitted slug against the DB `listing.slug`: if equal, send `slug: None`; if different, validate locally alongside other form fields and route both local format errors and API `LISTING_SLUG_EXISTS` / `slug` rejections to the inline slug error slot.

5.  **Checkout Legacy Currency-Suffix Removal (`web_ui_guest`)**
    *   Remove `split_currency_suffix` and its unconditional 301 redirect in `web_ui_guest` checkout so `GET /checkout/{slug}` always resolves as a listing slug.

### End-to-End Edit & Save Flow

```text
Host / Admin Browser (/admin/listings/{id}/edit)
  │
  ├── Typing in listing name (HTMX, delay:250ms, includes current slug input)
  │     └──► GET /admin/listings/slug-suggestion?name=...&slug=...  [host_or_admin, in-memory, 0 DB/API calls]
  │            └── common::slug::suggest_listing_slug(name)
  │            └── returns optional "Use suggested URL" chip (never auto-applies)
  │
  └── Submit POST /admin/listings/{id}/edit
        ├── Load authoritative listing via managed_listing (ownership check -> 404 for non-owners)
        ├── Compare submitted slug vs DB listing.slug
        │     ├── Equal  ──► UpdatedListingRequest.slug = None (no-op; legacy slugs safe)
        │     └── Diff   ──► Validate slug alongside other fields
        │                      ├── Err ──► re-render form with inline slug error (and any other field errors)
        │                      └── Ok  ──► PATCH /api/v1/listings/{id} (slug: Some(...))
        │                                    └── db_core::update_listing_with_conn (tx, FOR UPDATE, deleted_at IS NULL)
        │                                          ├── INSERT INTO listing_history
        │                                          ├── UPDATE listing SET ..., slug = COALESCE(..., slug)
        │                                          └── 23505 on listing_slug_unique (WHERE deleted_at IS NULL)
        │                                                ──► rollback tx ──► 409 LISTING_SLUG_EXISTS
        │                                                ──► re-render form with inline slug error
        └── Success ──► 303 See Other /admin/listings/{id}
```

### Downstream Impact & Accepted Trade-offs

*   **Route collisions:** Audited `web_ui_guest/src/pages/` and `web_ui_admin/src/pages/listings/` — no static single-segment sibling routes exist under `/listings/{slug}`, `/checkout/{slug}`, or `/admin/listings/clone/{slug}` that could shadow a listing slug; UUID lookup precedence is handled by rejecting UUID-shaped slugs.
*   **Calendar export URLs:** iCal feed URLs (`/listings/{id}/calendar.ics`) are keyed by UUID, so external calendar subscriptions do not break when a slug changes (only the download filename updates).
*   **In-flight guest checkout & sitemap cache:** A guest submitting `POST /checkout/{old-slug}` while a host simultaneously changes the slug will receive 404; `/sitemap.xml` caches for up to 1 hour (`max-age=3600`); confirmed bookings, requests, enquiries, reviews, and tax documents are keyed by `listing_id` (UUID) and are unaffected.
*   **Concurrent edit tabs:** Consistent with the rest of the listing edit form, concurrent edits follow last-write-wins semantics (optimistic concurrency via `updated_at` is out of scope).

## Alternatives Considered

### Alternative A: 301 Redirects via `listing_history` or a Dedicated Redirect Table

*   **Pros:** Preserves old shared links, guest bookmarks, and search engine rankings after a listing slug changes.
*   **Cons:** Requires reserving historical slugs indefinitely (preventing another listing from claiming them), adds an extra redirect lookup path on public 404s, and introduces conflict rules when a listing is renamed back to an earlier slug.
*   **Reason for Rejection:** Rejected by author: replaced slugs must become immediately available for reuse by other listings rather than staying reserved for redirects.

### Alternative B: Gate Slug Edits Strictly on a Name Change

*   **Pros:** Hides the slug field during routine edits, lowering the chance of accidental URL changes.
*   **Cons:** Forces hosts or admins to perform a dummy rename just to fix a typo, clean up an auto-generated slug, or update a listing that was renamed before this feature existed.
*   **Reason for Rejection:** Rejected by author: hosts and admins must be able to manually set a listing's slug at any time with URL-safe validation, while name changes separately offer an optional slug suggestion.

### Alternative C: Silently Normalize Manual Slug Input via `common::slug::user_slug`

*   **Pros:** Reuses `user_slug` directly and forgives uppercase letters, spaces, or punctuation in typed input.
*   **Cons:** Silently mutates what the host typed (for example stripping characters or truncating at 60 characters), which can publish a different URL than the host expected.
*   **Reason for Rejection:** Rejected by author: manually entered slugs must be validated without silent normalization and rejected with an inline error when malformed.

### Alternative D: Keep `split_currency_suffix` in `web_ui_guest/src/pages/checkout.rs` as a 404 Fallback

*   **Pros:** Preserves legacy `/checkout/{slug}-jmd` bookmarks while avoiding false redirects when a real listing slug ends in a currency code.
*   **Cons:** Retains dead legacy URL-parsing logic and extra active-currency lookups on checkout paths where `{slug}` identifies a listing directly.
*   **Reason for Rejection:** Rejected by author: `/checkout/{slug}` should refer only to a listing's slug, so the backwards-compatibility currency shim should be removed.

### Alternative E: Keep the Full-Table `listing_slug_unique` Constraint (No Database Migration)

*   **Pros:** Avoids adding a PostgreSQL migration and updating index definitions.
*   **Cons:** Soft-deleted listings (`deleted_at IS NOT NULL`) continue blocking active listings from using their slugs even though public queries ignore deleted rows.
*   **Reason for Rejection:** Rejected by author: replacing `listing_slug_unique` with a partial unique index `WHERE deleted_at IS NULL` ensures soft-deleted listings release their slugs for reuse.

### Alternative F: Listing-Scoped Suggestion Route (`GET /admin/listings/{id}/slug-suggestion`) or Client-Side JS Generator

*   **Pros:** A listing-scoped route could compare against the DB slug directly; client-side JS avoids HTTP round-trips.
*   **Cons:** A listing-scoped route triggers an authenticated `listing_api` + PostgreSQL lookup on every 250ms keystroke; client-side JS duplicates `common::slug::slugify` (accent folding, 60-char trim, UUID prefixing) and drifts over time.
*   **Reason for Rejection:** Rejected in favor of a stateless, `host_or_admin`-gated HTMX endpoint (`GET /admin/listings/slug-suggestion?name=...&slug=...`) that calls `common::slug::suggest_listing_slug` in-memory with zero DB/API calls and falls back to manual input when JavaScript is unavailable.

--------------------------------------------------------------------------------

## Non-Functional Requirements

*   **Security & Access Control:** Only `Admin` users and the owning `Host` may update a listing's slug (`403 Forbidden` for `Staff`, `404 Not Found` for non-owning hosts). `GET /admin/listings/slug-suggestion` is gated by `host_or_admin` (`403` for `Staff`), accesses no listing records, and bounds `name` input length (max 255 chars) to prevent unnecessary allocation; query strings are excluded from tracing spans. The original `listing.slug` used on `POST` to decide whether a slug changed is always read from the authoritative database listing loaded via `managed_listing`, never trusted from a hidden form input.
*   **Data Integrity & Concurrency:** Slug uniqueness is enforced authoritatively by the PostgreSQL partial unique index `listing_slug_unique` (`WHERE deleted_at IS NULL`) inside `update_listing_with_conn`'s row-locked transaction (`SELECT ... FOR UPDATE`). A `23505` violation rolls back both `listing_history` and `listing` atomically.
*   **Observability:** Emit a structured `tracing::info!` event recording `listing_id`, `old_slug`, and `new_slug` whenever a listing's slug changes.
*   **Performance & Cold Start:** No new services, background workers, or external dependencies are introduced. The stateless suggestion handler runs in-memory with zero DB or `listing_api` calls, and unchanged slug submissions send `slug: None` to skip unnecessary index writes.
*   **Documentation Hygiene:** Per `CONSTRAINTS.md`, all new public structs, enums, variants, fields, and functions must carry `///` doc comments (`-D missing_docs`).

--------------------------------------------------------------------------------

## Detailed Implementation Plan

### Target Files to Create/Modify

#### 1. `common/src/slug.rs` (Modify)

*   **Type / Scope:** Shared isomorphic Rust domain utility + unit tests.
*   **Proposed Changes:** Keep existing `slugify` and `user_slug` unchanged. Add two documented public functions (`///` doc comments):
    *   `pub fn validate_listing_slug(input: &str) -> Result<(), &'static str>`: validates without normalizing; rejects inputs shorter than `MIN_SLUG_CHARS` (3) (`"Listing URL must be at least 3 characters."`), longer than `MAX_SLUG_CHARS` (60) (`"Listing URL must be at most 60 characters."`), any character outside lowercase ASCII `a-z`, `0-9`, and `-` including uppercase or whitespace (`"Listing URL may only contain lowercase letters, numbers, and single hyphens."`), leading/trailing hyphens or consecutive hyphens `--` (`"Listing URL cannot start or end with a hyphen or contain consecutive hyphens."`), and any string parsing via `uuid::Uuid::parse_str` (`"Listing URL cannot be formatted like a UUID."`).
    *   `pub fn suggest_listing_slug(name: &str) -> Option<String>`: truncates input to at most 255 chars before calling `slugify`, prefixes `v-` (capped to `MAX_SLUG_CHARS` and trimmed of trailing hyphens) if the result parses as a `Uuid`, and returns `None` if fewer than `MIN_SLUG_CHARS` remain.
    *   Add unit tests covering valid slugs, uppercase/whitespace/punctuation rejection, boundary lengths (2, 3, 60, 61), leading/trailing/double hyphens, both 36-char hyphenated and 32-char hex UUID rejection, and `suggest_listing_slug` accent folding and UUID prefixing.
*   **Rationale:** Provides a single shared validator and suggester across `common`, `db_core`, `listing_api`, and `web_ui_admin` with zero drift and no unused error-enum boilerplate.

#### 2. `common/src/models.rs` (Modify)

*   **Type / Scope:** Shared API request DTO.
*   **Proposed Changes:** Add `#[serde(default)]` `#[validate(custom(function = "validate_optional_listing_slug"))]` `pub slug: Option<String>` (with `///` doc comment) to `UpdatedListingRequest`. Implement `fn validate_optional_listing_slug(slug: &str) -> Result<(), validator::ValidationError>` calling `crate::slug::validate_listing_slug(slug)` and mapping errors via `ValidationError::new("invalid_listing_slug").with_message(msg.into())`. Update any existing `UpdatedListingRequest` struct literals in `common` tests (`slug: None`).
*   **Rationale:** Exposes optional slug updates on `PATCH /api/v1/listings/{id}` and returns field-level validation messages matching existing custom validators (`validate_discount_percentage`, `validate_fee`).

#### 3. `db_core/migrations/<timestamp>_listing_slug_partial_unique_index.sql` (Create)

*   **Type / Scope:** Immutable PostgreSQL SQLx migration (created via `sqlx migrate add listing_slug_partial_unique_index`).
*   **Proposed Changes:** Drop the existing table constraint `ALTER TABLE listing DROP CONSTRAINT listing_slug_unique;` (confirmed created as a constraint in `20260406052700_20260406000000-add-listing-slug.sql:18`) and create `CREATE UNIQUE INDEX listing_slug_unique ON listing (slug) WHERE deleted_at IS NULL;`.
*   **Rationale:** Releases slugs when listings are soft-deleted while preserving the `listing_slug_unique` identifier for `23505` conflict matching.

#### 4. `db_core/src/error.rs` (Modify)

*   **Type / Scope:** Database error enum.
*   **Proposed Changes:** Add documented `///` variant `DbError::ListingSlugExists` with `#[error("That listing URL is already in use by another listing.")]` and a small helper (or match arm) for recognizing `SQLSTATE_UNIQUE_VIOLATION` (`23505`) on constraint `"listing_slug_unique"`.
*   **Rationale:** Distinguishes listing slug uniqueness conflicts from generic SQLx failures so `api_core` returns `409 Conflict` instead of `500 Internal Server Error`.

#### 5. `db_core/src/models.rs` (Modify)

*   **Type / Scope:** Database update model.
*   **Proposed Changes:** Add documented `pub slug: Option<String>` to `UpdatedListing` and update existing `UpdatedListing` struct literals in tests (`slug: None`).
*   **Rationale:** Passes optional slug updates from `listing_api` into `db_core::listing::update_listing_with_conn`.

#### 6. `db_core/src/listing/write.rs` (Modify)

*   **Type / Scope:** Listing create and update transaction logic.
*   **Proposed Changes:**
    *   In `create_listing`, map `23505` on `listing_slug_unique` to `DbError::ListingSlugExists`.
    *   In `update_listing_with_conn`, add `AND deleted_at IS NULL` to the `SELECT ... FROM listing WHERE id = $1 AND deleted_at IS NULL FOR UPDATE` query and the `UPDATE listing ... WHERE id = $1 AND deleted_at IS NULL` query.
    *   Compute `effective_slug: Option<&str>`: `None` when `updated_listing_data.slug` is `None` or equals `current.slug`; otherwise call `common::slug::validate_listing_slug(s)` (mapping errors to `DbError::ValidationError`) and pass `Some(s)`.
    *   Extend `UPDATE listing SET ...` with `slug = COALESCE($30, slug)` (after existing 29 bind parameters).
    *   Map `SQLSTATE_UNIQUE_VIOLATION` (`23505`) where `db_error.constraint() == Some("listing_slug_unique")` to `DbError::ListingSlugExists` (rolling back the transaction, including the `listing_history` insert).
    *   When `effective_slug` is `Some(new_slug)`, log `tracing::info!(listing_id = %id, old_slug = %current.slug, new_slug = %new_slug, "listing slug updated")`.
*   **Rationale:** Enforces atomic, defense-in-depth slug updates, prevents mutating soft-deleted rows, protects legacy slugs on unrelated edits, and handles create/update slug collisions uniformly.

#### 7. `.sqlx/` (Modify)

*   **Type / Scope:** SQLx offline query metadata.
*   **Proposed Changes:** Run `cargo sqlx prepare --workspace` after modifying `update_listing_with_conn`.
*   **Rationale:** Keeps offline compile-time query metadata synchronized for CI builds.

#### 8. `app_api/api_core/src/error.rs` (Modify)

*   **Type / Scope:** Shared Actix-web error mapper and error-table tests.
*   **Proposed Changes:** Add `ApiError::Database(DbError::ListingSlugExists)` to **both** `ResponseError::status_code()` (`StatusCode::CONFLICT`) and `ResponseError::error_response()` returning `{ "code": "LISTING_SLUG_EXISTS", "message": "That listing URL is already in use by another listing." }`. Add a test case to `error_mapping_tests::error_status_code_and_payload_table`.
*   **Rationale:** Ensures `409 Conflict` is returned without triggering the 5xx `tracing::error!` branch and verifies the mapping in unit tests.

#### 9. `app_api/listing_api/src/apis.rs` (Modify)

*   **Type / Scope:** Listing API PATCH handler and OpenAPI annotation.
*   **Proposed Changes:** Pass `slug: req_data.slug` when constructing `UpdatedListing` in `update_listing`, and add `(status = 409, description = "Listing slug already in use")` to `#[utoipa::path]`.
*   **Rationale:** Connects the HTTP PATCH payload to `db_core` and documents the 409 response in OpenAPI.

#### 10. `web_ui_common/src/assets/ui.js` (Modify)

*   **Type / Scope:** Shared CSP-compliant UI script.
*   **Proposed Changes:** Add a branch inside the existing top-level delegated `document.addEventListener("click", ...)` listener (line 157) matching `[data-apply-slug-target][data-apply-slug-value]`: set the target input's `.value` to `data-apply-slug-value`, focus it, and clear only the children of `#listing-slug-suggestion` (`container.replaceChildren()`) without removing `#listing-slug-suggestion` itself from the DOM.
*   **Rationale:** Enables one-click adoption of the HTMX-rendered slug suggestion under strict CSP (`script-src 'self'`) while keeping `#listing-slug-suggestion` intact for subsequent HTMX swaps.

#### 11. `web_ui_admin/src/pages/listings/form.rs` (Modify)

*   **Type / Scope:** Shared listing form component, raw fields struct, validation, and API error parser.
*   **Proposed Changes:**
    *   Add documented `pub slug: String` to `ListingFields`, populate it in `from_listing`, leave it empty in `blank`, and set `slug: None` in `ValidListing::into_update`.
    *   Add `#[default] slug_error: Option<String>` to `listing_form` (checking Clippy's `too_many_arguments` limit and grouping optional edit props if needed so `create.rs` and `pin_picker_tests::render` need zero changes).
    *   When `editing` is true (never on create or clone):
        *   Add `hx-get="/admin/listings/slug-suggestion"`, `hx-trigger="input changed delay:250ms"`, `hx-target="#listing-slug-suggestion"`, and `hx-include="#listing-name, #listing-slug"` on `#listing-name`.
        *   Render the `"Listing URL"` input (`id="listing-slug"`, `name="slug"`, `maxlength="60"`, prefixed with `/listings/`) inside the `"Basics"` fieldset, paired with an empty `#listing-slug-suggestion` container, an inline `#listing-slug-error` alert slot when `slug_error` is `Some`, and a helper warning stating that changing the URL immediately breaks existing links and bookmarks (404).
    *   Keep `api_rejection_messages` intact for `create.rs`, and add `pub fn split_api_rejection(err: &dyn std::fmt::Display) -> Option<(Option<String>, Vec<String>)>` returning `(slug_error, general_errors)` so `LISTING_SLUG_EXISTS` and validator `"slug"` field errors route exclusively to `#listing-slug-error` without duplicating into `#listing-form-errors`.
*   **Rationale:** Delivers edit-only slug editing, inline error routing, 404 warning, and opt-in suggestions without breaking `create.rs` or `pin_picker_tests`.

#### 12. `web_ui_admin/src/pages/listings/edit.rs` (Modify)

*   **Type / Scope:** Admin edit listing page, POST handler, and stateless HTMX suggestion route.
*   **Proposed Changes:**
    *   Add `#[derive(Deserialize, Default)] struct SlugSuggestionQuery { name: String, slug: Option<String> }` and `#[page("/admin/listings/slug-suggestion")]` gated by `host_or_admin(cx).await?` (zero DB/API calls). Bound `q.name` length, call `common::slug::suggest_listing_slug(&q.name)`, and when `Some(suggested)` differs from `q.slug.as_deref().unwrap_or("")`, return an HTML fragment containing the suggested slug and a `type="button"` `"Use suggested URL"` button with `data-apply-slug-target="listing-slug"` and `data-apply-slug-value=(suggested)`. Otherwise return an empty fragment.
    *   In `update_listing`: compare `fields.slug` against the authoritative DB `listing.slug`. Run `common::slug::validate_listing_slug(&fields.slug)` independently alongside `fields.validate(Some(listing.user_id), admin)` whenever `fields.slug != listing.slug` so both general errors and `slug_error` render on the same submit. Only call `listing_api` when both succeed, setting `request.slug = (fields.slug != listing.slug).then(|| fields.slug.clone())`. On API error, partition via `split_api_rejection` into `(slug_error, errors)` and pass `slug_error` to `listing_form` on re-render.
*   **Rationale:** Keeps the suggestion endpoint stateless with zero DB/API overhead, preserves legacy slugs on unrelated edits, and surfaces both format and conflict errors inline without partial saves.

#### 13. `web_ui_guest/src/pages/checkout.rs` (Modify)

*   **Type / Scope:** Guest checkout page.
*   **Proposed Changes:** Remove `split_currency_suffix` (lines 128–134) and its unconditional `redirect_permanent` check in `checkout_page` (lines 599–602). Retain `redirect_permanent` (still used by `legacy_currency_checkout!`) and `active_currency_codes` (still used by `checkout_submit`).
*   **Rationale:** Ensures `/checkout/{slug}` always resolves as a listing slug so valid slugs ending in `-usd`, `-jmd`, `-eur`, etc. remain bookable.

#### 14. Tests (`db_core/src/listing/tests.rs`, `app_api/listing_api/src/apis_test.rs`, `web_ui_admin/src/pages/listings/form.rs`, `playwright/e2e/admin-portal/listings-admin.spec.ts`) (Modify)

*   **Type / Scope:** Rust unit/integration tests and Playwright E2E tests.
*   **Proposed Changes:**
    *   `db_core/src/listing/tests.rs`: test updating slug, atomic rollback on duplicate active slug (`DbError::ListingSlugExists` with no orphan `listing_history` row), reusing a replaced slug on another listing, reusing a soft-deleted listing's slug, rejecting updates on soft-deleted rows, and unrelated edits succeeding on a legacy short slug when `slug: None` or unchanged.
    *   `app_api/listing_api/src/apis_test.rs`: test `PATCH /api/v1/listings/{id}` returning `200` on valid slug change, `400` on invalid/UUID/whitespace slug, and `409` (`LISTING_SLUG_EXISTS`) on duplicate active slug.
    *   `web_ui_admin/src/pages/listings/form.rs`: test `split_api_rejection` for `LISTING_SLUG_EXISTS` and validator `slug` field errors, and verify slug input is absent on create/clone forms.
    *   `playwright/e2e/admin-portal/listings-admin.spec.ts`: test renaming a listing while keeping the default slug, renaming and clicking `"Use suggested URL"` to update the slug (confirming new URL loads and old URL returns 404), manually editing a slug without a name change, and inline error display without partial save on invalid format and duplicate slug.
*   **Rationale:** Verifies slug validation, atomic rollback, API status codes, form rendering, and E2E edit flows.

### Verification Plan

*   **Formatting:** `cargo fmt --check` and `topcoat fmt --check`
*   **Compilation/Build & Offline SQLx:** `sqlx migrate run`, `cargo sqlx prepare --workspace --check`, `SQLX_OFFLINE=true cargo check --workspace --all-features --tests`, and `cargo build --workspace`
*   **Static Analysis & Linting:** `SQLX_OFFLINE=true cargo clippy --workspace --all-targets -- -D warnings`, `SQLX_OFFLINE=true cargo clippy --workspace --all-features -- -D missing_docs`, and panic check (`! rg -q "\.(unwrap|expect)\(" app_api/ common/ db_core/ web_client/src web_ui_common/src web_ui_guest/src web_ui_admin/src`)
*   **Unit & Integration Tests:** `cargo test -p common`, `cargo test -p db_core`, `cargo test -p api_core`, `cargo test -p listing_api`, `cargo test -p web_ui_admin`, `cargo test -p web_ui_guest` (and `cargo test --workspace`)
*   **E2E/Integration:** `npx playwright test playwright/e2e/admin-portal/listings-admin.spec.ts`

--------------------------------------------------------------------------------

*Drafted with EGM*
