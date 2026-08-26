# Spec 77: Verified Guest Review System Audit Fixes & Security Enhancements

## Overview

Following an architectural and security audit of the **Verified Guest Review System** (GH Issue 74 / Spec 63), eight critical vulnerabilities, concurrency race conditions, status code mismatches, and UI/UX performance bottlenecks were identified across `db_core`, `app_api`, `common`, and `web_app`.

This specification details the technical architecture, security mitigations, database concurrency fixes, offline SQLx query caching, edge case resilience matrix, performance optimizations, and comprehensive test plan to remediate GH Issue 77.

---

## 1. Requirements & Technical Specification

### 1.1 Security & Authorization Enhancements (`app_api/listing_api`)

#### 1.1.1 Authenticated Review Token Issuance (`GET /api/v1/reviews/booking/{booking_id}/token`)
- **Current Behavior**: If the request lacks an `x-user-id` header, the handler executes `SELECT guest_id FROM booking WHERE id = $1` and uses that ID to issue a review token without authorization.
- **Remediation**:
  - Remove unauthenticated DB fallback.
  - Require authenticated JWT claims context (`Claims` extracted via Actix middleware or authenticated session).
  - Verify `authenticated_user_id == booking.guest_id`.
  - Return `401 Unauthorized` if unauthenticated, or `403 Forbidden` if caller ID does not match `booking.guest_id`.

#### 1.1.2 Verified Host Reply Authorization (`POST /api/v1/reviews/{id}/reply`)
- **Current Behavior**: Relies on an unauthenticated client header `x-user-id` to identify the host.
- **Remediation**:
  - Remove reliance on unauthenticated `x-user-id` header.
  - Extract `auth_user_id` strictly from verified JWT token claims (`claims.sub`).
  - Validate that `auth_user_id == listing.user_id` in `db_review::add_host_reply`.
  - Return `401 Unauthorized` if unauthenticated, or `403 Forbidden` if caller is not the listing host owner.

---

### 1.2 Concurrency, Database Logic & CI Query Caching (`db_core`)

#### 1.2.1 Listing Rating Aggregation Row Locking
- **Current Behavior**: Review insertion computes `SELECT AVG(overall_rating), COUNT(*) FROM review WHERE listing_id = $1` and applies `UPDATE listing` without locking the parent `listing` row. Concurrent review submissions cause lost updates and invalid aggregate ratings.
- **Remediation**:
  - Execute a row-level lock within the database transaction prior to aggregate recalculation:
    ```sql
    SELECT id FROM listing WHERE id = $1 FOR UPDATE;
    ```
  - Enforce lock order: `review_token` (UPDATE) $\rightarrow$ `listing` (SELECT FOR UPDATE) $\rightarrow$ `review` (INSERT) $\rightarrow$ `listing` (UPDATE).

#### 1.2.2 Strict 15-Day Token Expiration Binding
- **Current Behavior**: `get_or_create_booking_review_token` sets `expires_at` to `Utc::now() + Duration::days(days_remaining + 1)`, extending token validity beyond the 15-day post-checkout window.
- **Remediation**:
  - Bind token expiration deterministically to the booking checkout timestamp:
    ```rust
    let cutoff_datetime = (booking.date_to + Duration::days(15))
        .and_hms_opt(23, 59, 59)
        .unwrap_or_default();
    ```
  - Ensure all tokens for a booking expire at 23:59:59 UTC on day 15 following `date_to`.

#### 1.2.3 Explicit Domain Error to HTTP Status Code Mapping
- **Current Behavior**: Map domain errors to generic `400 Bad Request` or `500 Internal Server Error`.
- **Remediation**:
  - Map review domain errors to explicit HTTP status codes:
    - `401 Unauthorized`: Missing or invalid JWT credentials.
    - `403 Forbidden`: User attempts token issuance or host reply for unowned booking/listing.
    - `400 Bad Request` (`NOT_YET_VALID`): Attempted review submission before stay checkout date (`today < booking.date_to`).
    - `410 Gone` (`EXPIRED`): Attempted review submission after 15-day window (`today > cutoff_date`).
    - `409 Conflict` (`TOKEN_ALREADY_USED`): Duplicate submission with an already consumed token.
    - `409 Conflict` (`HOST_REPLY_ALREADY_EXISTS`): Duplicate host reply submission.

#### 1.2.4 Offline SQLx Query Cache Maintenance (`.sqlx`)
- **Current Behavior**: CI running `SQLX_OFFLINE=true cargo check --workspace --all-targets` fails if new or modified queries in test modules (`#[cfg(test)]`) are missing from `.sqlx/`.
- **Remediation**:
  - Execute `DATABASE_URL="..." cargo sqlx prepare --workspace -- --all-targets` to populate compile-time query data in `.sqlx/` for lib, bin, and test targets.
  - Commit all generated `.sqlx/query-*.json` files to source control to guarantee clean offline CI pipeline execution.

#### 1.2.5 Scale-to-Zero Pending Booking Hold Cleanup (`db_core` & `app_api`)
- **Current Behavior**: Stale 15-minute pending booking holds are cleaned up via a `tokio::spawn` loop running every 10 minutes in `booking_api/src/main.rs`. This background loop prevents Cloud Run instances from scaling completely down to zero.
- **Remediation**:
  - Remove the continuous background cleanup loop from the Rust application code.
  - Migrate the 15-minute expiration logic into the PostgreSQL database using the `pg_cron` extension via a new database migration.
  - The cron job is configured to run every 15 minutes, safely canceling stale holds and logging immutable history transitions without waking or billing Cloud Run resources.

---

### 1.3 UI/UX Performance & Interactive Improvements (`web_app`)

#### 1.3.1 Live Overall Rating Calculation Display (`web_app/src/components/review_submit.rs`)
- **Current Behavior**: Sub-rating radio buttons do not update or show the dynamic overall rating score prior to submission.
- **Remediation**:
  - Bind `cleanliness`, `accuracy`, `location`, and `value` rating inputs to Leptos `RwSignal<i32>` signals.
  - Implement a derived memo signal for overall rating:
    ```rust
    let overall_score = Signal::derive(move || {
        (cleanliness.get() + accuracy.get() + location.get() + value.get()) as f64 / 4.0
    });
    ```
  - Render an interactive rating header displaying `★ {format!("{:.2}", overall_score.get())} / 5.0` in real-time.

#### 1.3.2 Enforced Radio Button Selection Defaults (`web_app/src/components/review_submit.rs`)
- **Current Behavior**: `RatingInput` hardcodes `checked` on 1-star radio inputs (`value="1"`).
- **Remediation**:
  - Update default selection to 5 stars (`value="5"`) or require explicit user selection with validation feedback.

#### 1.3.3 N+1 Review Eligibility API Request Elimination (`web_app/src/components/bookings.rs` & `common`)
- **Current Behavior**: Each `BookingItemCard` on `/bookings` initiates an independent HTTP request to `GET /api/v1/reviews/booking/{booking_id}/token`, creating $N$ network requests for $N$ bookings.
- **Remediation**:
  - Embed review eligibility DTO `review_eligibility: Option<BookingReviewEligibility>` directly inside `BookingResponse` in `common::models`.
  - Populate `review_eligibility` in a single SQL query (`LEFT JOIN review_token` & `LEFT JOIN review`) within `db_core::booking::get_user_bookings`.
  - Reduce frontend network calls on `/bookings` from $O(N)$ to $O(1)$.

#### 1.3.4 Listing Reviews Pagination / Load More Controls (`web_app/src/components/listing_detail.rs`)
- **Current Behavior**: Reviews section hardcodes page 1 (`per_page = 10`) without controls to fetch remaining reviews.
- **Remediation**:
  - Add reactive `page` signal (`RwSignal::new(1)`) and review accumulation state.
  - Implement a "Load More Reviews" DaisyUI button (`btn btn-outline`) that increments `page` and appends reviews to the listing page view until all reviews are rendered.

---

### 1.4 Edge Case Analysis & Resilience Matrix

| Edge Case Scenario | Vulnerability / Failure Mode | Mitigating Strategy | Architectural Guardrail |
| :--- | :--- | :--- | :--- |
| **Concurrent Review Submissions** | Two guests submit reviews for the same listing at the exact same millisecond, risking lost update on `listing.overall_rating`. | Acquire `SELECT id FROM listing WHERE id = $1 FOR UPDATE` lock in DB transaction before calculating `AVG()`. | Serializable DB row locking |
| **Token Expiration Drift** | Token generated on day 14 post-checkout receives relative `+ 15 days` lifespan, exceeding stay checkout window. | Bind `expires_at` strictly to `(booking.date_to + 15 days) 23:59:59 UTC`. | Fixed timestamp calculation |
| **Unauthenticated Token Request** | User calls `GET /api/v1/reviews/booking/{id}/token` without JWT token, guessing booking UUID. | Validate JWT claims (`claims.sub`). Verify caller matches `booking.guest_id`. Return `401`/`403`. | OWASP Access Control Check |
| **Host Reply Spoofing** | Attacker injects `x-user-id: <HOST_ID>` header in `POST /reviews/{id}/reply`. | Ignore `x-user-id` HTTP header. Extract host ID strictly from authenticated JWT token claims. | JWT Claims Context |
| **N+1 API Storm on `/bookings`** | Loading 20 bookings triggers 20 parallel HTTP calls to `/reviews/booking/{id}/token`. | Join eligibility into single `/api/v1/bookings/user/{id}` payload using `LEFT JOIN`. | $O(1)$ Batch Querying |
| **Unselected Rating Inputs** | Radio form default checked on 1 star submits 1-star ratings for untouched dimensions. | Set radio defaults to 5 stars or require explicit validation for all 4 sub-rating fields. | Leptos Signal Validation |
| **Missing SQLx Query Cache in CI** | `sqlx::query!` in test targets (`#[cfg(test)]`) fails when `SQLX_OFFLINE=true` in CI pipeline. | Run `cargo sqlx prepare --workspace -- --all-targets` to populate `.sqlx` cache for all targets. | CI/CD Offline Safety |

---

## 2. Performance & Scalability Considerations

1. **Elimination of N+1 Query & Network Bottleneck**:
   - Embedding review eligibility within `BookingResponse` reduces API HTTP round-trips from $O(N)$ to $O(1)$.
   - Database queries on `/bookings` execute via a single indexed `LEFT JOIN`, preserving Cloud Run latency budgets ($<300\text{ms}$ p50).

2. **Database Row Lock Concurrency & Serialization**:
   - `SELECT FOR UPDATE` on `listing` table adds minor transactional lock overhead (~2-5ms) during review submission.
   - Lock duration is minimal as the transaction only performs `AVG()` recalculation and single-row update before committing.
   - Lock acquisition order is strictly fixed (`review_token` $\rightarrow$ `listing` $\rightarrow$ `review`), preventing database deadlocks.

3. **Cloud Run Scale-to-Zero Budget Compliance**:
   - Zero additional runtime dependencies introduced.
   - Leptos WASM bundle size impact is negligible (< 2KB gzip).

---

## 3. Threat Modeling & Security Mitigations

| Threat ID | Threat Category (OWASP) | Vulnerability Description | Required Mitigation |
| :--- | :--- | :--- | :--- |
| **SEC-01** | **A01:2021 - Broken Access Control** | Unauthenticated users guessing `booking_id` can fetch review tokens and post fake reviews. | Enforce JWT claims context on `GET /api/v1/reviews/booking/{booking_id}/token` and verify `claims.sub == booking.guest_id`. |
| **SEC-02** | **A01:2021 - Broken Access Control** | Untrusted `x-user-id` HTTP header allows unauthorized users to spoof host replies on any review. | Extract caller ID strictly from verified JWT claims (`claims.sub`) and verify `auth_user_id == listing.user_id`. |
| **SEC-03** | **A04:2021 - Insecure Design / Concurrency** | Race condition during aggregate calculation allows stale rating data overwrites. | Enforce `SELECT id FROM listing WHERE id = $1 FOR UPDATE` within the DB transaction prior to aggregate rating recalculation. |
| **SEC-04** | **A07:2021 - Identification & Auth Failures** | Token expiration drift permits review submissions beyond the intended 15-day window. | Bind `expires_at` strictly to `booking.date_to + 15 days` at 23:59:59 UTC. |

---

## 4. Comprehensive Test Plan

### 4.1 Unit Tests (`common` & `db_core`)
- **`test_token_expiration_calculation`**: Verify that `expires_at` is set to `date_to + 15 days` (23:59:59 UTC) regardless of when the token generation function is executed.
- **`test_sub_rating_overall_calculation`**: Test that `NewReviewRequest::calculate_overall_rating` accurately averages sub-ratings (e.g. 5, 4, 5, 4 $\rightarrow$ 4.50) using `rust_decimal::Decimal`.

### 4.2 Integration Tests (`app_api/listing_api` & `db_core`)
- **`test_unauthenticated_token_issuance_forbidden`**: Attempt fetching a review token without JWT auth header; verify `401 Unauthorized` response.
- **`test_unauthorized_guest_token_issuance_forbidden`**: Attempt fetching a review token for a booking belonging to another user; verify `403 Forbidden` response.
- **`test_unverified_host_reply_rejected`**: Attempt posting host reply using forged `x-user-id` header without valid JWT token; verify request is rejected.
- **`test_concurrent_review_submission_locking`**: Spawn 10 concurrent tokio tasks submitting reviews for the same listing. Verify row lock serializes operations and final `listing.overall_rating` and `review_count` match expected averages.
- **`test_http_status_code_mappings`**: Verify explicit HTTP status codes returned for `EXPIRED` (410), `TOKEN_ALREADY_USED` (409), and `NOT_YET_VALID` (400).
- **`test_offline_sqlx_query_cache`**: Verify `SQLX_OFFLINE=true cargo check --workspace --all-targets` compiles cleanly against cached `.sqlx` metadata.

### 4.3 E2E & WASM Component Tests (`web_app`)
- **`test_live_rating_display_updates`**: Simulate star selection changes in `ReviewSubmitPage` and verify live overall rating display updates dynamically.
- **`test_bookings_dashboard_single_network_request`**: Inspect network tab on `/bookings` with 20 past bookings; verify 0 secondary token calls occur ($O(1)$ API calls).
- **`test_listing_detail_pagination_load_more`**: Render listing detail page with 15 reviews; verify page 1 displays 10 reviews and clicking "Load More Reviews" appends the remaining 5 reviews.
