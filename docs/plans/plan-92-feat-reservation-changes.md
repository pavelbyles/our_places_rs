# Engineering Plan #92: Bilateral Reservation Changes (Dates & Guest Count)

## Metadata
- **Intent**: [intent-92](../intent/intent-92-feat-reservation-changes.md)
- **Spec**: [spec-92](../specs/spec-92-feat-reservation-changes.md)
- **Branch**: `feat/92-feat-reservation-changes` (Base: `dev`)
- **Status**: Ready for Execution (Pending Lock Approval)
- **Blast Radius**: ~24 files across 8 crates (`common`, `db_core`, `api_core`, `listing_api`, `booking_api`, `email_worker`, `web_app_common_tc`, `web_app_tc`, `web_app_admin_tc`). Structured into 6 ordered, verifiable vertical slices.

---

## 1. Architecture Review: Data Flows, State Machines & Boundaries

### 1.1. End-to-End Data Flow (ASCII)
```
+---------------------------------------------------------------------------------------------------+
| 1. QUOTE & PROPOSAL CREATION (Optimistic Availability Check, Zero Calendar Lock)                  |
+---------------------------------------------------------------------------------------------------+
Guest / Host / Admin
   │
   ├─► POST /api/v1/bookings/{id}/change-quote  {date_from?, date_to?, num_adults, num_children, ...}
   │     └─► booking_api:
   │           1. Validate pre-stay / mid-stay date rules against today_jm (America/Jamaica)
   │           2. Validate 1 <= proposed_persons <= listing.max_guests & num_adults >= 1
   │           3. Check availability excluding booking.id (if dates changed)
   │           4. Re-price full stay at booking.exchange_rate & booking.tax_rate:
   │                seasonal_subtotal + extra_guest_fee_total -> apply_discounts (overall stay)
   │                -> with_fees(cleaning, deposit) -> apply_taxes(tax_rate) -> finalize()
   │           5. Compute guest balance_due (new_total - old_total) & host_payout_delta
   │         ◄── 200 OK ReservationChangeQuoteResponse
   │
   └─► POST /api/v1/bookings/{id}/change-requests
         └─► db_core::reservation_change::create_change_request (TX):
               1. Auto-expire any past-due pending request for booking_id (expires_at_date <= today_jm)
               2. Verify no active 'pending' request exists (uq_reservation_change_pending)
               3. Verify host_payout_ledger status == 'pending'
               4. INSERT reservation_change_request (status = 'pending', pricing_snapshot, expires_at_date)
             COMMIT
             ──► Enqueue EmailTemplate::ReservationChangeRequested (to counterparty via email_outbox)
         ◄── 201 Created ReservationChangeRequestResponse

+---------------------------------------------------------------------------------------------------+
| 2. COUNTERPARTY APPROVAL (Serializable Locking, Payment Settlement, Receipt & Payout Adjustment)  |
+---------------------------------------------------------------------------------------------------+
Counterparty (Host if Guest-initiated; Guest if Host-initiated; or Admin)
   │
   └─► POST /api/v1/bookings/{id}/change-requests/{req_id}/approve  {response_note?}
         └─► booking_api:
               1. Verify caller is authorized counterparty (or Admin)
               2. PaymentSettlementGateway::settle_change_balance(booking_id, req_id, currency, balance_due)
                    ├─ balance_due == 0  ──► ChangeSettlementStatus::NotRequired
                    ├─ balance_due > 0   ──► charge_difference ──► Charged (SIM-CHG-...)  [or 422 Failed]
                    └─ balance_due < 0   ──► issue_refund      ──► Refunded (SIM-REF-...) [or 422 Failed]
               3. db_core::reservation_change::approve_change_request (Single Atomic TX):
                    a. SELECT id FROM listing WHERE id = $1 FOR UPDATE          (Global Lock Order #1)
                    b. SELECT *  FROM booking WHERE id = $2 FOR UPDATE          (Global Lock Order #2)
                    c. SELECT *  FROM reservation_change_request WHERE id = $3 FOR UPDATE (Lock #3)
                    d. Re-verify status == 'pending', expires_at_date > today_jm, date rules, payout 'pending'
                    e. Re-verify check_availability_excluding (if dates changed) ─✗─► 409 DATE_UNAVAILABLE
                    f. UPDATE booking (dates, number_of_persons, metadata, financial snapshot columns)
                    g. INSERT booking_history (immutable audit trail with responder_id)
                    h. Supersede active REC -> INSERT tax_document REC-YYYY-{code}-R{n}
                       (with amount_previously_paid = old_total, balance_due = new_total - old_total)
                    i. UPDATE host_payout_ledger (gross_amount, platform_fee_amount, net_payout_amount)
                    j. UPDATE reservation_change_request SET status = 'approved', settlement_status, ...
                  COMMIT
               4. Enqueue EmailTemplate::ReservationChangeApproved (with revised receipt URL)
         ◄── 200 OK ReservationChangeRequestResponse
```

### 1.2. State Machines

#### A. `ReservationChangeStatus` Lifecycle
```
                           +-------------------+
                           |      pending      |
                           +-------------------+
                             /    |     |    \
          counterparty      /     |     |     \   expires_at_date <= today_jm
            approves       /      |     |      \  OR booking cancelled/completed
                          v       |     |       v
               +----------+       |     |       +----------+
               | approved |       |     |       | expired  |
               +----------+       |     |       +----------+
                 [terminal]       |     |         [terminal]
                                  |     |
             counterparty declines|     |initiator withdraws (or booking cancelled)
                                  v     v
                        +----------+   +-----------+
                        | declined |   | cancelled |
                        +----------+   +-----------+
                         [terminal]     [terminal]

Invariant: Partial unique index `uq_reservation_change_pending` enforces <= 1 `pending` row per `booking_id`.
```

#### B. `ChangeSettlementStatus` Lifecycle
```
Request Created:
  ├─ balance_due == 0  ──► [ not_required ] (terminal)
  ├─ balance_due > 0   ──► [ pending_charge ] ──approve──► [ charged ]  (or [ failed ] on gateway error)
  └─ balance_due < 0   ──► [ pending_refund ] ──approve──► [ refunded ] (or [ failed ] on gateway error)
```

### 1.3. Component Boundaries & Error Paths
| Layer / Crate | Responsibility | Error Paths & HTTP Mapping |
| :--- | :--- | :--- |
| `common` | Pure `Decimal` extra-guest & overall-stay discount math; pre-stay vs. mid-stay date validator; DTOs & email payloads. | `PricingError::InvalidDateRange` (400), `PricingError::MinNightsNotMet` (400), `ReservationChangeValidationError` (400 / 409). |
| `db_core` | SQLx queries, global lock hierarchy (`listing` $\rightarrow$ `booking` $\rightarrow$ `reservation_change_request`), atomic receipt + payout update. | `DbError::DateUnavailable` (409 `DATE_UNAVAILABLE`), `DbError::BookingNotModifiable` (409 `BOOKING_NOT_MODIFIABLE`), `DbError::UniqueConstraintViolation` (409). |
| `listing_api` | CRUD validation for `guests_included` (`1..=max_guests`) and `extra_guest_fee` (`0..=1_000_000`). | `400 Bad Request` (`VALIDATION_ERROR`). |
| `booking_api` | Auth matrix enforcement, `PaymentSettlementGateway` trait orchestration, change request endpoints, cron auto-expiry. | `403 Forbidden` (unauthorized initiator/responder or non-admin direct PATCH), `422 Unprocessable Entity` (`PAYMENT_SETTLEMENT_FAILED`), `404 Not Found`. |
| `email_worker` | Renders `reservation_change_requested`, `reservation_change_approved`, `reservation_change_declined` HTML templates. | Outbox retry backoff (up to 3 attempts). |
| `web_app_tc` & `web_app_admin_tc` | Topcoat SSR forms for proposing changes, live quote delta preview, and counterparty approve/decline/withdraw actions. | Displays structured API error messages inline without page crash. |

---

## 2. Comprehensive Edge Case Analysis

| Category | Scenario | Mitigation / Handling |
| :--- | :--- | :--- |
| **Empty / Null Inputs** | Request submitted with `date_from: None`, `date_to: None`, and unchanged `number_of_persons` | Rejected with `400 Bad Request` ("At least one reservation term (dates or guest count) must change"). |
| **Pre-Stay vs. Mid-Stay Dates** | Guest attempts to change `date_from` on or after check-in day (`current.date_from <= today_jm`) | Rejected with `409 BOOKING_NOT_MODIFIABLE`; UI disables check-in date picker once check-in has arrived. |
| **Mid-Stay Early Checkout / Extension** | Guest or Host modifies `date_to` while stay is in progress (`current.date_from <= today_jm < proposed_date_to`) | Allowed! Validates `proposed_date_to > today_jm`, `proposed_date_to > current.date_from`, min-stay rules, and date availability. |
| **Scale (1000x Load)** | High read/write volume on booking listings & change requests | Composite indexes `idx_reservation_change_booking (booking_id, created_at DESC)` and `idx_reservation_change_status_expiry (status, expires_at_date) WHERE status = 'pending'` ensure $O(\log N)$ lookups. |
| **Concurrent Access & Deadlocks** | Two concurrent requests try to create a `pending` change request for the same booking, or approval races with a new checkout on overlapping dates | Partial unique index `uq_reservation_change_pending` prevents duplicate `pending` rows at the DB engine level. Global lock ordering (`listing` $\rightarrow$ `booking` $\rightarrow$ `reservation_change_request`) prevents PostgreSQL `40P01` deadlocks. |
| **Network / Gateway Failures** | Payment gateway times out or rejects card charge/refund during approval | Gateway call executes before committing the DB mutation; failure marks `settlement_status = 'failed'` and returns `422`, leaving `booking`, `tax_document`, and `host_payout_ledger` untouched. |
| **Malformed / Out-of-Bounds Data** | `proposed_num_adults == 0`, negative `extra_guest_fee`, `guests_included > max_guests`, or `proposed_persons > listing.max_guests` | Rejected by both Rust validator / handler checks (`400`) and PostgreSQL `CHECK` constraints. |
| **Settled Host Payout** | Change requested or approved after admin already marked `host_payout_ledger` as `processing` or `paid` | Rejected with `409 BOOKING_NOT_MODIFIABLE` ("Reservation cannot be modified after host payout has been processed"). |

---

## 3. Ordered Vertical Slices & Task Breakdown

```
S0 Schema & Domain Contracts ─► S1 Listing Extra-Guest Fees ─► S2 Change Quote & Request Lifecycle (Core DB + API)
                                                                               │
                                                   ┌───────────────────────────┴───────────────────────────┐
                                                   ▼                                                       ▼
                                    S3 Email Templates & Notifications                      S4 Guest & Admin Topcoat UI
                                                   └───────────────────────────┬───────────────────────────┘
                                                                               ▼
                                                                S5 SQLx Offline Sync & Full Gate
```

- [x] **Slice 0: Database Migration & Isomorphic Pricing / Domain Contracts (`common`, `db_core`)** — **Scope: M**
  - Create `db_core/migrations/20261018000000_reservation_change_requests.sql`.
  - Update `common/src/pricing.rs` (`with_extra_guests`, overall-stay discount calculation, `validate_reservation_change_dates`).
  - Add DTOs and enums to `common/src/models.rs` and `db_core/src/models.rs`.
  - **Verification**: `sqlx migrate run && cargo test -p common`

- [x] **Slice 1: Listing Extra-Guest Fee Persistence & API (`db_core`, `listing_api`, `booking_api`)** — **Scope: M**
  - Update `db_core/src/listing.rs` and `app_api/listing_api/src/apis.rs` to persist and return `guests_included` and `extra_guest_fee`.
  - Update `booking_api` (`create_booking`) to include extra-guest pricing in initial booking calculations and validate `number_of_persons <= listing.max_guests`.
  - **Verification**: `cargo test -p db_core listing && cargo test -p listing_api`

- [x] **Slice 2: Bilateral Reservation Change Engine & Payment Settlement Gateway (`db_core`, `booking_api`)** — **Scope: M**
  - Implement `db_core/src/reservation_change.rs` (create, list, approve with `FOR UPDATE` locks, decline, cancel, auto-expire).
  - Implement `PaymentSettlementGateway` trait and `SimulatedPaymentGateway` in `app_api/booking_api/`.
  - Add `/change-quote` and `/change-requests` endpoints in `app_api/booking_api/src/apis.rs`, restrict direct `PATCH /api/v1/bookings/{id}` date/guest mutation to Admins, and hook auto-expiry into `complete_stays_cron`.
  - **Verification**: `cargo test -p db_core reservation_change && cargo test -p booking_api`

- [ ] **Slice 3: Transactional Email Templates & Outbox Dispatch (`common`, `email_worker`, `booking_api`)** — **Scope: S**
  - Add `ReservationChangeRequested`, `ReservationChangeApproved`, and `ReservationChangeDeclined` templates and payloads in `common/src/email.rs` and `app_api/email_worker/`.
  - Dispatch outbox notifications on change request creation, approval, and decline.
  - **Verification**: `cargo test -p email_worker`

- [ ] **Slice 4: Topcoat Guest & Admin Portal UI (`web_app_common_tc`, `web_app_tc`, `web_app_admin_tc`)** — **Scope: M**
  - Add API client methods in `web_app_common_tc/src/client.rs`.
  - Update `web_app_tc/src/pages/booking_date_change.rs` and `web_app_tc/src/pages/bookings.rs` for guest change requests, mid-stay date rules, guest count inputs, and approving/declining host-initiated requests.
  - Update `web_app_admin_tc/src/pages/listings_admin.rs` (extra-guest fee fields) and `web_app_admin_tc/src/pages/bookings_admin.rs` (review/approve/decline guest change requests and initiate host/admin changes).
  - **Verification**: `cargo test -p web_app_tc -p web_app_common_tc -p web_app_admin_tc`

- [ ] **Slice 5: SQLx Offline Metadata Sync & Full Quality Gate Verification** — **Scope: XS**
  - Run `cargo sqlx prepare --workspace -- --all-targets` and execute all `CONSTRAINTS.md` checks.
  - **Verification**: `cargo fmt --check && SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features -- -D warnings && cargo sqlx prepare --workspace --check`

---

## 4. Test Matrix

| Scenario | Type | Priority | Target Crate | Covered in Plan? |
| :--- | :---: | :---: | :--- | :---: |
| Happy path: Extra-guest fee + weekly/monthly overall-stay discount math | Unit | P0 | `common` | ☑ |
| Happy path: Guest requests date/guest change $\rightarrow$ Host approves $\rightarrow$ `REC-R2` + payout updated | Integration | P0 | `booking_api` | ☑ |
| Happy path: Host requests mid-stay `date_to` / guest change $\rightarrow$ Guest approves | Integration | P0 | `booking_api` | ☑ |
| Pre-stay vs. mid-stay boundary: `date_from` change rejected once check-in arrives | Unit & Int | P0 | `common`, `booking_api` | ☑ |
| Mid-stay early checkout (`proposed_date_to < current.date_to`) & negative `balance_due` refund | Integration | P0 | `booking_api` | ☑ |
| Empty / no-op change request (dates and guest count identical to booking) | Integration | P0 | `booking_api` | ☑ |
| Error handling: Counterparty declines or initiator withdraws pending request | Integration | P1 | `booking_api` | ☑ |
| Error handling: Non-admin attempts direct `PATCH /bookings/{id}` date/guest mutation (`403`) | Integration | P0 | `booking_api` | ☑ |
| Error handling: Self-approval by initiator rejected (`403`) | Integration | P0 | `booking_api` | ☑ |
| Concurrent access: Duplicate pending request blocked by `uq_reservation_change_pending` (`409`) | Integration | P1 | `db_core`, `booking_api` | ☑ |
| Concurrent access: Requested dates booked by another guest before approval (`409 DATE_UNAVAILABLE`) | Integration | P1 | `booking_api` | ☑ |
| Auto-expiry: Pending request expires when `expires_at_date <= today_jm` | Integration | P1 | `db_core`, `booking_api` | ☑ |
| Email rendering: Requested, Approved, and Declined templates render without unreplaced tokens | Unit | P1 | `email_worker` | ☑ |
| Performance bounds: Single-transaction approval with indexed lookups and zero blocking I/O | Smoke | P2 | `booking_api` | ☑ |

---

## 5. Security Review

| Threat / Vector | Mitigation |
| :--- | :--- |
| **Unauthorized Direct Mutation / Bypassing Approval** | `PATCH /api/v1/bookings/{id}` rejects `date_from`, `date_to`, and `number_of_persons` mutations unless the authenticated user holds `UserRole::Admin`. Guests and Hosts must use `/change-requests`. |
| **Self-Approval / Counterparty Spoofing** | `approve` and `decline` handlers verify JWT `claims.sub` against `booking.guest_id` and `listing.user_id`: Guest-initiated requests can only be approved/declined by the Listing Host or an Admin; Host-initiated requests can only be approved/declined by the Booking Guest or an Admin. |
| **IDOR on Change Quotes & Requests** | All `/change-quote` and `/change-requests` endpoints verify that the caller is the booking guest, the listing host, or an admin before returning data (returning `404` or `403` otherwise). |
| **Client-Side Price Tampering** | Client payloads only supply dates and guest counts. All monetary figures (`proposed_total_price`, `balance_due`, `proposed_net_payout`) are computed server-side from database rates using `rust_decimal::Decimal`. |
| **XSS in Change Request Reasons / Notes** | `reason` and `response_note` are length-validated, control-character stripped, and escaped via Topcoat `view!` SSR and HTML email escaping. |

---

## 6. Review Verdict

| Dimension | Score | Rationale |
| :--- | :---: | :--- |
| **Architecture Clarity** | **9.5 / 10** | Clean separation between `common` pricing/validation, `db_core` transactional locking (`listing` $\rightarrow$ `booking` $\rightarrow$ `reservation_change_request`), and `booking_api` payment gateway orchestration. |
| **Error Handling Completeness** | **9.0 / 10** | Every failure path maps to deterministic `ApiError` / `DbError` variants (`BOOKING_NOT_MODIFIABLE`, `DATE_UNAVAILABLE`, `VALIDATION_ERROR`, `PAYMENT_SETTLEMENT_FAILED`) with zero `.unwrap()`/`.expect()`. |
| **Test Coverage Plan** | **9.5 / 10** | Covers pre-stay and mid-stay date rules, extra-guest overall-stay discount math, bilateral approval matrix, concurrent overlap locking, auto-expiry, and email rendering. |
| **Security Posture** | **9.5 / 10** | Strict bilateral role enforcement prevents self-approval and unauthorized direct PATCH mutations; all pricing is server-authoritative. |
| **Performance Considerations** | **9.0 / 10** | Partial unique index and targeted composite indexes keep queries sub-millisecond; zero new heavy dependencies preserve Cloud Run $<300\text{ms}$ cold-start budget. |
