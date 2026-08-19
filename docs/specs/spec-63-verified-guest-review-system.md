# Spec 63: Verified Guest Review & Rating System

## Overview

Authentic guest reviews and property ratings are critical for establishing guest trust and driving bookings on short-term rental platforms. However, open review systems without booking verification are vulnerable to spam, unverified claims, and fraudulent ratings.

This specification details the end-to-end design and technical implementation of a **Verified Guest Review & Rating System** for the Our Places monorepo. Under this system:
1. Reviews are strictly gated by single-use, cryptographically signed review tokens issued when a booking is `completed`.
2. Review tokens become active (`valid_from`) 24 hours post-checkout to ensure guests have departed and reflected on their stay, expiring after 30 days.
3. Ratings encompass 4 explicit sub-dimensions (**Cleanliness**, **Accuracy**, **Location**, and **Value**) which automatically calculate the **Overall Rating** for the stay.
4. Review submissions atomically recalculate and update denormalized property aggregate ratings (`overall_rating` and `review_count`) on the `listing` table.
5. Listing hosts can post a single immutable response to guest reviews on their properties.

The implementation spans all layers of the Isomorphic Rust Monorepo (`db_core`, `common`, `app_api`, `web_app_common`, `web_app`, and `web_app_admin`).

---

## Requirements

### 1. Database Schema (`db_core`)
- **New Tables**:
  - `review`: Stores completed guest reviews, sub-ratings, calculated overall rating, feedback text, and optional host response.
  - `review_token`: Manages single-use verification tokens linked to completed bookings.
- **`review_token` Fields**:
  - `id`: `UUID PRIMARY KEY DEFAULT uuidv7()`
  - `token`: `VARCHAR(64) NOT NULL UNIQUE` (Secure random token string)
  - `booking_id`: `UUID NOT NULL UNIQUE REFERENCES booking(id) ON DELETE CASCADE`
  - `guest_id`: `UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE`
  - `listing_id`: `UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE`
  - `valid_from`: `TIMESTAMPTZ NOT NULL` (Set to `completed_at + 24 hours`)
  - `expires_at`: `TIMESTAMPTZ NOT NULL` (Set to `completed_at + 30 days`)
  - `used_at`: `TIMESTAMPTZ` (NULL when unused; populated with timestamp upon submission)
  - `created_at`: `TIMESTAMPTZ NOT NULL DEFAULT NOW()`
- **`review` Fields**:
  - `id`: `UUID PRIMARY KEY DEFAULT uuidv7()`
  - `booking_id`: `UUID NOT NULL UNIQUE REFERENCES booking(id) ON DELETE CASCADE`
  - `listing_id`: `UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE`
  - `guest_id`: `UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE`
  - `cleanliness_rating`: `INTEGER NOT NULL CHECK (cleanliness_rating BETWEEN 1 AND 5)`
  - `accuracy_rating`: `INTEGER NOT NULL CHECK (accuracy_rating BETWEEN 1 AND 5)`
  - `location_rating`: `INTEGER NOT NULL CHECK (location_rating BETWEEN 1 AND 5)`
  - `value_rating`: `INTEGER NOT NULL CHECK (value_rating BETWEEN 1 AND 5)`
  - `overall_rating`: `NUMERIC(3, 2) NOT NULL CHECK (overall_rating BETWEEN 1.00 AND 5.00)`
  - `public_review_text`: `TEXT`
  - `private_host_feedback`: `TEXT`
  - `host_reply_text`: `TEXT`
  - `host_replied_at`: `TIMESTAMPTZ`
  - `created_at`: `TIMESTAMPTZ NOT NULL DEFAULT NOW()`
  - `updated_at`: `TIMESTAMPTZ NOT NULL DEFAULT NOW()`
- **Indexes & Database Constraints**:
  - `idx_review_listing_id` on `review(listing_id)`
  - `idx_review_guest_id` on `review(guest_id)`
  - `idx_review_token_hash` on `review_token(token)`

### 2. Isomorphic Domain Models & Calculation Logic (`common`)
- **Models (`common::models`)**:
  - `Review`: Full domain model for a property review.
  - `NewReviewRequest`: Payload submitted by guest containing token, sub-ratings (1-5), public text, and private feedback.
  - `HostReplyRequest`: Payload submitted by host containing reply text.
  - `ReviewTokenInfo`: DTO containing booking, listing name, and token status returned when guest accesses `/review?token=XYZ`.
  - `ListingRatingSummary`: Structure containing overall average rating, sub-dimension averages (Cleanliness, Accuracy, Location, Value), total review count, and rating distribution breakdown (counts for 5, 4, 3, 2, 1 stars).
- **Sub-Rating Calculation**:
  - `overall_rating` for a review is computed as the arithmetic mean of `cleanliness_rating`, `accuracy_rating`, `location_rating`, and `value_rating`, rounded to 2 decimal places using `rust_decimal::Decimal`.

### 3. Verification & Token Lifecycle
- **Token Generation**:
  - Issued immediately when a booking transitions to `completed` in `db_core::booking` / `app_api::booking_api`.
  - `valid_from` is set to `completed_at + 24 hours` (or `NOW() + 24 hours` if transitioning manually).
  - `expires_at` is set to `completed_at + 30 days`.
- **Token Authorization & Frictionless Access**:
  - Single-use token authorizes access to submit a review for the associated booking without requiring guest login.
  - If a user session exists, verify it matches `booking.guest_id` (or log a warning if session differs while honoring valid token).
- **Single-Use Invalidation**:
  - Validated and consumed atomically within a database transaction during review creation:
    `UPDATE review_token SET used_at = NOW() WHERE token = $1 AND used_at IS NULL AND valid_from <= NOW() AND expires_at > NOW() RETURNING booking_id, guest_id, listing_id`.

### 4. Listing Rating Aggregation
- When a review is created inside `db_core::review::create_review_with_token`:
  - Recalculate average `overall_rating` and `review_count` for the property:
    `SELECT AVG(overall_rating), COUNT(*) FROM review WHERE listing_id = $1`.
  - Update `listing` table in the same transaction:
    `UPDATE listing SET overall_rating = $1, review_count = $2, updated_at = NOW() WHERE id = $3`.

### 5. Host Response & Moderation Rules
- Hosts (`listing.user_id`) can post a response to guest reviews via `POST /api/v1/reviews/{id}/reply`.
- Response validation: Verifies `host_reply_text IS NULL` before update.
- Immutability: Guest reviews and host responses are immutable once posted.

---

## Edge Cases

1. **Premature Review Attempt (`valid_from > NOW()`)**:
   - If guest clicks the link before the 24-hour post-checkout delay has elapsed, API returns `400 Bad Request` (`"TOKEN_NOT_YET_ACTIVE"`) with a clear user message stating when the review form will open.
2. **Expired Review Token (`expires_at <= NOW()`)**:
   - If guest clicks token after 30 days, API returns `410 Gone` (`"TOKEN_EXPIRED"`).
3. **Double Submission / Token Reuse**:
   - Attempting to submit with a token where `used_at IS NOT NULL` returns `409 Conflict` (`"TOKEN_ALREADY_USED"`).
4. **Duplicate Host Reply**:
   - Attempting to reply to a review that already has `host_reply_text` returns `409 Conflict` (`"HOST_REPLY_ALREADY_EXISTS"`).
5. **Non-Host Reply Attempt**:
   - Attempting to post a host reply by a user who is not the listing owner (`listing.user_id != claims.sub`) returns `403 Forbidden`.
6. **Concurrent Submissions**:
   - Database row-level locking on `review_token` and `booking` prevents duplicate review insertions or race conditions during aggregate updates.

---

## Technical Implementation

### System Architecture Flow

```mermaid
flowchart TD
    subgraph Booking Completion
        B[Booking Completed] -->|Trigger Token Creation| RT[Insert review_token: valid_from = completed_at + 24h]
    end

    subgraph Guest Review Flow
        G[Guest Clicks Email Link ?token=XYZ] -->|GET /api/v1/reviews/token/XYZ| TI[Validate Token & Fetch Details]
        TI -->|Submit Form| PR[POST /api/v1/reviews/token/XYZ]
        PR -->|Tx: Atomic Token Consume| DB_T[UPDATE review_token SET used_at = NOW()]
        PR -->|Tx: Insert Review| DB_R[INSERT INTO review]
        PR -->|Tx: Aggregate Ratings| DB_L[UPDATE listing SET overall_rating, review_count]
    end

    subgraph Host Response Flow
        H[Host Management UI] -->|POST /api/v1/reviews/id/reply| HR[Validate Ownership & Update host_reply_text]
    end
```

### Affected Files & Components

---

#### [NEW] [20260809213027_create_reviews_and_tokens.sql](file:///home/pav/code/our_places_rs-feat-63-verified-guest-review-system/db_core/migrations/20260809213027_create_reviews_and_tokens.sql)
- SQL migration script creating `review` and `review_token` tables and indexes.

#### [NEW] [db_core/src/review.rs](file:///home/pav/code/our_places_rs-feat-63-verified-guest-review-system/db_core/src/review.rs)
- Database entity module containing compile-time SQLx query routines:
  - `create_review_token()`
  - `get_token_info()`
  - `create_review_with_token()`
  - `add_host_reply()`
  - `get_listing_reviews()`
  - `get_listing_rating_summary()`

#### [MODIFY] [db_core/src/lib.rs](file:///home/pav/code/our_places_rs-feat-63-verified-guest-review-system/db_core/src/lib.rs)
- Export `pub mod review;`.

#### [MODIFY] [db_core/src/models.rs](file:///home/pav/code/our_places_rs-feat-63-verified-guest-review-system/db_core/src/models.rs)
- Define `Review` and `ReviewToken` database structs.

#### [MODIFY] [db_core/src/booking.rs](file:///home/pav/code/our_places_rs-feat-63-verified-guest-review-system/db_core/src/booking.rs)
- Update booking status transition logic (`update_booking_status`) to invoke `db_core::review::create_review_token` when booking status becomes `BookingStatus::Completed`.

#### [MODIFY] [common/src/models.rs](file:///home/pav/code/our_places_rs-feat-63-verified-guest-review-system/common/src/models.rs)
- Add domain DTOs: `NewReviewRequest`, `HostReplyRequest`, `ReviewTokenInfo`, `ListingRatingSummary`, `ReviewResponse`.

#### [NEW] [app_api/listing_api/src/reviews.rs](file:///home/pav/code/our_places_rs-feat-63-verified-guest-review-system/app_api/listing_api/src/reviews.rs)
- Actix-web handlers:
  - `GET /api/v1/listings/{id}/reviews`: Fetch paginated public reviews and rating summary.
  - `GET /api/v1/reviews/token/{token}`: Validate token and return review pre-fill information.
  - `POST /api/v1/reviews/token/{token}`: Submit guest review using single-use token.
  - `POST /api/v1/reviews/{id}/reply`: Post host response (JWT authenticated).

#### [MODIFY] [app_api/listing_api/src/main.rs](file:///home/pav/code/our_places_rs-feat-63-verified-guest-review-system/app_api/listing_api/src/main.rs)
- Register review routes in HTTP server configuration.

#### [NEW] [web_app_common/src/reviews.rs](file:///home/pav/code/our_places_rs-feat-63-verified-guest-review-system/web_app_common/src/reviews.rs)
- Leptos server functions and API client helper methods for interacting with review endpoints.

#### [MODIFY] [web_app_common/src/lib.rs](file:///home/pav/code/our_places_rs-feat-63-verified-guest-review-system/web_app_common/src/lib.rs)
- Export `pub mod reviews;`.

#### [NEW] [web_app/src/components/review_submit.rs](file:///home/pav/code/our_places_rs-feat-63-verified-guest-review-system/web_app/src/components/review_submit.rs)
- Interactive review submission page (`/review?token=...`) with DaisyUI star ratings for Cleanliness, Accuracy, Location, and Value, calculated overall rating display, review text area, and success state.

#### [MODIFY] [web_app/src/components/listing_detail.rs](file:///home/pav/code/our_places_rs-feat-63-verified-guest-review-system/web_app/src/components/listing_detail.rs)
- Integrate Rating Summary breakdown (Overall + 4 sub-dimensions) and Guest Reviews list with host replies.

#### [MODIFY] [web_app/src/app.rs](file:///home/pav/code/our_places_rs-feat-63-verified-guest-review-system/web_app/src/app.rs)
- Add `/review` route mapping to `ReviewSubmitPage`.

---

## Unit Test Cases

### 1. Domain & Rating Math Tests (`common/src/models.rs`)
- `test_sub_rating_overall_calculation`: Verify that sub-ratings `[5, 4, 5, 4]` compute to an overall rating of `4.50`.
- `test_sub_rating_rounding`: Verify that sub-ratings `[5, 5, 4, 5]` (`19 / 4 = 4.75`) round accurately to `4.75`.

### 2. Database Integration Tests (`db_core/src/review.rs`)
- `test_review_token_lifecycle`:
  1. Create completed booking.
  2. Generate token with `valid_from` in past.
  3. Validate token redemption sets `used_at`.
  4. Attempt second redemption; verify `Err(TokenAlreadyUsed)`.
- `test_listing_rating_aggregation`:
  1. Submit first review with rating `5.00`. Verify `listing.overall_rating = 5.00` and `listing.review_count = 1`.
  2. Submit second review with rating `4.00`. Verify `listing.overall_rating = 4.50` and `listing.review_count = 2`.
- `test_host_reply_permissions`:
  1. Host submits reply to guest review; verify success.
  2. Non-host attempts reply; verify `Err(Forbidden)`.
  3. Host attempts second reply; verify `Err(HostReplyAlreadyExists)`.

### 3. Backend API Handler Tests (`app_api/listing_api/src/reviews.rs`)
- `test_post_review_token_validation`: Verify `400` for premature token, `410` for expired token, `409` for used token.
- `test_get_listing_reviews_pagination`: Verify paginated retrieval of public reviews.

---

## Acceptance Criteria

- [ ] Migration script `20260810000000_create_reviews_and_tokens.sql` applies cleanly and defines `review` and `review_token` tables with constraints.
- [ ] Transitioning booking to `completed` automatically creates a `review_token` record with `valid_from = completed_at + 24 hours` and `expires_at = completed_at + 30 days`.
- [ ] Guest accessing `/review?token=XYZ` can view booking info and submit ratings for Cleanliness, Accuracy, Location, and Value.
- [ ] Overall rating is calculated as the rounded 2-decimal average of the 4 sub-ratings.
- [ ] Review submission atomically consumes the token (`used_at = NOW()`), records the review, and updates `listing.overall_rating` and `listing.review_count`.
- [ ] Review tokens cannot be reused or submitted prior to `valid_from`.
- [ ] Public reviews and rating breakdown are displayed on the listing detail page (`ListingDetailPage`).
- [ ] Listing hosts can submit a single reply to reviews on their property.
- [ ] All unit and integration tests pass across `common`, `db_core`, and `app_api`.
- [ ] Workspace compiles cleanly with `cargo check --workspace` and `cargo test --workspace`.
