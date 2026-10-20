# Spec 92: Bilateral Reservation Changes (Dates & Guest Count)

> Driven by GitHub Issue #92. Intent: [intent-92](../intent/intent-92-feat-reservation-changes.md).

## Overview
Introduces host-configurable extra-guest pricing (`guests_included` and `extra_guest_fee`) on listings and a bilateral reservation modification workflow (`reservation_change_request`) for confirmed bookings. Guests and hosts can propose changes to reservation dates (`date_from`, `date_to`) and/or party size (`number_of_persons`) before or during an active stay. Counterparty approval re-verifies availability under PostgreSQL `SELECT ... FOR UPDATE` row-level locks, executes payment delta settlement via a pluggable payment gateway interface, updates the booking and `booking_history`, supersedes and re-issues the statutory Payment Receipt (`REC-YYYY-{code}-R{n}`), updates the `pending` entry in `host_payout_ledger`, and dispatches transactional email notifications.

---

## 1. Requirements & Domain Rules

### 1.1. Listing Extra-Guest Pricing & Overall-Stay Discounts
- Each `listing` defines:
  - `guests_included: i32` (default `1`, constrained to `1 <= guests_included <= max_guests`).
  - `extra_guest_fee: Decimal` (default `0.00`, in listing `base_currency` per extra guest per night above `guests_included`, constrained to `0 <= extra_guest_fee <= 1_000_000`).
- When `extra_guest_fee > 0` and `number_of_persons > guests_included`:
  ```
  extra_guests          = max(0, number_of_persons - guests_included)
  extra_guest_fee_rate  = round_bankers(listing.extra_guest_fee * exchange_rate, 2)
  extra_guest_fee_total = round_bankers(extra_guests * extra_guest_fee_rate * total_days, 2)
  sub_total_price       = seasonal_nights_subtotal + extra_guest_fee_total
  ```
- Weekly (`total_days >= 7`) and monthly (`total_days >= 28`) percentage discounts apply to the **overall stay subtotal** (`sub_total_price`, which includes `extra_guest_fee_total`):
  ```
  discount_value    = round_bankers(sub_total_price * (discount_pct / 100), 2)
  accommodation_net = sub_total_price - discount_value
  taxable_base      = accommodation_net + cleaning_fee
  tax_value         = round_bankers(taxable_base * tax_rate, 2)
  platform_fee      = round_bankers(accommodation_net * 0.05, 2)
  total_price       = taxable_base + tax_value + platform_fee + damage_deposit
  ```
- Host payout gross amount is computed from `accommodation_net` converted to the host's default currency (`round_bankers(accommodation_net * host_exchange_rate, 2)`), so both date changes and guest count changes deterministically update guest invoicing and host payout.

### 1.2. Pre-Stay & Mid-Stay Validation Rules (`America/Jamaica`)
Let `today_jm = (Utc::now() - Duration::hours(5)).date_naive()`. Both request creation and counterparty approval enforce:
1. **Booking Status & Active Stay**:
   - `booking.status == BookingStatus::Confirmed` and `booking.date_to > today_jm` (else `409 BOOKING_NOT_MODIFIABLE`).
   - Any associated `host_payout_ledger` entry for `booking.id` must be in `pending` status (if already `processing` or `paid`, reject with `409 BOOKING_NOT_MODIFIABLE`).
2. **Material Change Requirement**:
   - At least one of `proposed_date_from != booking.date_from`, `proposed_date_to != booking.date_to`, or `proposed_number_of_persons != booking.number_of_persons` must be true (else `400 VALIDATION_ERROR`).
3. **Check-In Date Change Rule (`proposed_date_from != booking.date_from`)**:
   - Allowed **only before the existing check-in date**: both `booking.date_from > today_jm` and `proposed_date_from > today_jm` must hold (else `409 BOOKING_NOT_MODIFIABLE`).
4. **Check-Out Date Change Rule (`proposed_date_to != booking.date_to`)**:
   - Allowed **before or during an active stay**: `proposed_date_to > today_jm` and `proposed_date_to > proposed_date_from` must hold. `proposed_date_to` may be earlier than `booking.date_to` (early departure / shortened stay) or later than `booking.date_to` (stay extension).
5. **Combined Date Change Rule**:
   - When both `date_from` and `date_to` change, both rules (3) and (4) must hold.
6. **Minimum Stay & Guest Capacity**:
   - `total_days = (proposed_date_to - proposed_date_from).num_days() >= required_min_nights` (evaluated across listing `minimum_stay` and any active `PriceOverride` rows in `[proposed_date_from, proposed_date_to)`).
   - `1 <= proposed_number_of_persons <= listing.max_guests`, `proposed_num_adults >= 1`, and `proposed_number_of_persons == (proposed_num_adults + proposed_num_children + proposed_num_infants) as i32`.

### 1.3. Bilateral Approval Matrix & Auto-Expiry
| Initiator | Who Can Approve / Decline | Who Can Withdraw (`Cancel`) | Direct `PATCH /bookings/{id}` Date/Guest Mutation |
| :--- | :--- | :--- | :--- |
| **Guest** (`booking.guest_id`) | Listing **Host** (`listing.user_id`) or **Admin** | Initiating **Guest** or **Admin** | Forbidden (`403 Forbidden`) |
| **Host** (`listing.user_id`) | Booking **Guest** (`booking.guest_id`) or **Admin** | Initiating **Host** or **Admin** | Forbidden (`403 Forbidden`) |
| **Admin** (`UserRole::Admin`) | Counterparty (**Guest** or **Host**) or **Admin** | **Admin** | Allowed (force-apply administrative override) |

- **Single Active Request Invariant**: At most one `pending` `reservation_change_request` may exist per `booking_id` (`UNIQUE (booking_id) WHERE status = 'pending'`). Submitting a new request while one is `pending` returns `409 Conflict` (`CHANGE_REQUEST_ALREADY_PENDING`) unless the existing request is withdrawn or declined first (or auto-expired).
- **Auto-Expiry**:
  - `expires_at_date` is set on creation:
    - If `proposed_date_from != previous_date_from`: `min(previous_date_from, proposed_date_from)`.
    - Otherwise (only `date_to` and/or guest count changing): `min(previous_date_to, proposed_date_to)`.
  - Any `pending` request with `expires_at_date <= today_jm` automatically transitions to `expired` when queried, before creating a new request, during approval attempts (returning `409 BOOKING_NOT_MODIFIABLE`), and during the daily `complete-stays` cron sweep.

### 1.4. Payment Settlement Gateway Boundary
- `balance_due = proposed_total_price - previous_total_price`:
  - `balance_due == 0.00` $\rightarrow$ `ChangeSettlementStatus::NotRequired`.
  - `balance_due > 0.00` (guest owes difference) $\rightarrow$ `PaymentSettlementGateway::charge_difference(...)` $\rightarrow$ `ChangeSettlementStatus::Charged` with `gateway_reference`.
  - `balance_due < 0.00` (guest owed partial refund) $\rightarrow$ `PaymentSettlementGateway::issue_refund(...)` $\rightarrow$ `ChangeSettlementStatus::Refunded` with `gateway_reference`.
- If gateway settlement fails, the request records `ChangeSettlementStatus::Failed` and returns `422 PAYMENT_SETTLEMENT_FAILED` without mutating the `booking`, `tax_document`, or `host_payout_ledger` rows.

---

## 2. Edge Cases
| Edge Case | Expected System Behavior |
| :--- | :--- |
| Guest attempts to change `date_from` on or after check-in day (`booking.date_from <= today_jm`) | Rejected with `409 BOOKING_NOT_MODIFIABLE` ("Check-in date cannot be modified once check-in has arrived"). |
| Guest/Host shortens `date_to` during an active stay (`booking.date_from <= today_jm < proposed_date_to < booking.date_to`) | Allowed if `proposed_date_to - booking.date_from >= required_min_nights`; recalculates stay and issues revised `REC-...-R{n}` with negative `balance_due`. |
| Guest/Host extends `date_to` during an active stay, but another booking overlaps the extension dates | Rejected with `409 DATE_UNAVAILABLE` on creation (or on approval under `FOR UPDATE` lock if booked in the interim). |
| Counterparty approves a pending change request after `expires_at_date <= today_jm` | Request is atomically marked `expired` and approval returns `409 BOOKING_NOT_MODIFIABLE`. |
| Initiator tries to approve their own change request (non-Admin) | Rejected with `403 FORBIDDEN` ("Initiator cannot approve their own reservation change request"). |
| Party count increased above `listing.max_guests` or `num_adults == 0` | Rejected with `400 VALIDATION_ERROR`. |
| Party count increased within `guests_included` (`proposed_number_of_persons <= listing.guests_included`) with unchanged dates | Allowed; `balance_due = 0.00`, `host_payout_delta = 0.00`, upon approval updates `booking.number_of_persons` and `metadata`, records `booking_history`, and issues revised `REC-...-R{n}`. |
| Booking is cancelled while a `pending` change request exists | `update_booking` transitions any `pending` change request for `booking_id` to `cancelled`. |
| Concurrent approval & new checkout on the same listing dates | Global lock hierarchy (`listing FOR UPDATE` $\rightarrow$ `booking FOR UPDATE` $\rightarrow$ `reservation_change_request FOR UPDATE`) prevents `40P01` deadlocks and guarantees serializable availability checks. |

---

## 3. Technical Implementation

### 3.1. Database Schema (`db_core/migrations/20261018000000_reservation_change_requests.sql`)
```sql
-- 1. Listing Extra-Guest Pricing Columns
ALTER TABLE listing
  ADD COLUMN guests_included INTEGER NOT NULL DEFAULT 1 CHECK (guests_included >= 1),
  ADD COLUMN extra_guest_fee NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (extra_guest_fee >= 0);

ALTER TABLE listing_history
  ADD COLUMN guests_included INTEGER,
  ADD COLUMN extra_guest_fee NUMERIC(12,2);

-- 2. Reservation Change Request Enums & Table
CREATE TYPE reservation_change_status AS ENUM ('pending', 'approved', 'declined', 'cancelled', 'expired');
CREATE TYPE change_initiator_role AS ENUM ('guest', 'host', 'admin');
CREATE TYPE change_settlement_status AS ENUM ('not_required', 'pending_charge', 'charged', 'pending_refund', 'refunded', 'failed');

CREATE TABLE reservation_change_request (
    id                         UUID PRIMARY KEY DEFAULT uuidv7(),
    booking_id                 UUID NOT NULL REFERENCES booking(id) ON DELETE CASCADE,
    listing_id                 UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    initiated_by_id            UUID NOT NULL REFERENCES "user"(id),
    initiated_by_role          change_initiator_role NOT NULL,
    status                     reservation_change_status NOT NULL DEFAULT 'pending',
    previous_date_from         DATE NOT NULL,
    previous_date_to           DATE NOT NULL,
    proposed_date_from         DATE NOT NULL,
    proposed_date_to           DATE NOT NULL,
    previous_number_of_persons INTEGER NOT NULL,
    proposed_number_of_persons INTEGER NOT NULL CHECK (proposed_number_of_persons >= 1),
    proposed_num_adults        INTEGER NOT NULL CHECK (proposed_num_adults >= 1),
    proposed_num_children      INTEGER NOT NULL DEFAULT 0 CHECK (proposed_num_children >= 0),
    proposed_num_infants       INTEGER NOT NULL DEFAULT 0 CHECK (proposed_num_infants >= 0),
    proposed_num_pets          INTEGER NOT NULL DEFAULT 0 CHECK (proposed_num_pets >= 0),
    currency                   CHAR(3) NOT NULL,
    previous_total_price       NUMERIC(12,2) NOT NULL,
    proposed_total_price       NUMERIC(12,2) NOT NULL,
    balance_due                NUMERIC(12,2) NOT NULL,
    previous_net_payout        NUMERIC(12,2) NOT NULL,
    proposed_net_payout        NUMERIC(12,2) NOT NULL,
    host_payout_delta          NUMERIC(12,2) NOT NULL,
    pricing_snapshot           JSONB NOT NULL,
    settlement_status          change_settlement_status NOT NULL DEFAULT 'not_required',
    gateway_reference          VARCHAR(128),
    reason                     TEXT,
    responder_id               UUID REFERENCES "user"(id),
    response_note              TEXT,
    responded_at               TIMESTAMPTZ,
    expires_at_date            DATE NOT NULL,
    created_at                 TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at                 TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uq_reservation_change_pending
    ON reservation_change_request (booking_id)
    WHERE status = 'pending';

CREATE INDEX idx_reservation_change_booking ON reservation_change_request (booking_id, created_at DESC);
CREATE INDEX idx_reservation_change_status_expiry ON reservation_change_request (status, expires_at_date) WHERE status = 'pending';
```

### 3.2. Shared Domain Crate (`common`)
- `common/src/pricing.rs`:
  - Add `extra_guests: i32`, `extra_guest_fee_per_night: Decimal`, and `extra_guest_fee_total: Decimal` to `BookingCalculator`.
  - Add `BookingCalculator<BaseRate>::with_extra_guests(self, number_of_persons: i32, guests_included: i32, extra_guest_fee_per_night: Decimal) -> Self`, which computes `extra_guest_fee_total` and adds it to `sub_total_price` and `discounted_subtotal` before `apply_discounts(...)` is called.
  - Add pure validation helper `validate_reservation_change_dates(current_from, current_to, proposed_from, proposed_to, today_jm) -> Result<NaiveDate, ReservationChangeValidationError>` returning `expires_at_date`.
- `common/src/models.rs`:
  - Add `guests_included: i32` and `extra_guest_fee: Decimal` (or `Option<Decimal>` on create/update requests) to `ListingResponse`, `NewListingRequest`, `UpdatedListingRequest`.
  - Add `ReservationChangeStatus`, `ChangeInitiatorRole`, `ChangeSettlementStatus`, `ReservationChangePricingSnapshot`, `ReservationChangeQuoteRequest`, `ReservationChangeQuoteResponse`, `CreateReservationChangeRequest`, `RespondReservationChangeRequest`, and `ReservationChangeRequestResponse`.
  - Update `UpdatedBookingRequest` to also support `number_of_persons: Option<i32>` for Admin direct override.
- `common/src/email.rs`:
  - Add `EmailTemplate::ReservationChangeRequested`, `EmailTemplate::ReservationChangeApproved`, `EmailTemplate::ReservationChangeDeclined` and their strongly-typed zero-float payloads (`ReservationChangeRequestedPayload`, `ReservationChangeApprovedPayload`, `ReservationChangeDeclinedPayload`).

### 3.3. Database Layer (`db_core`)
- `db_core/src/listing.rs`:
  - Include `guests_included` and `extra_guest_fee` in `create_listing`, `update_listing`, `listing_history` snapshots, and all `SELECT` queries.
- `db_core/src/reservation_change.rs` [NEW]:
  - `expire_past_due_change_requests(executor, booking_id: Option<Uuid>, today_jm: NaiveDate) -> Result<u64>`
  - `create_change_request(pool, &NewReservationChangeRequest, today_jm: NaiveDate) -> Result<ReservationChangeRequestRow>`
  - `get_change_request_by_id(executor, id: Uuid) -> Result<Option<ReservationChangeRequestRow>>`
  - `list_change_requests_for_booking(pool, booking_id: Uuid, today_jm: NaiveDate) -> Result<Vec<ReservationChangeRequestRow>>`
  - `decline_change_request(pool, id: Uuid, responder_id: Uuid, response_note: Option<&str>, today_jm: NaiveDate) -> Result<ReservationChangeRequestRow>`
  - `cancel_change_request(pool, id: Uuid, actor_id: Uuid, is_admin: bool, today_jm: NaiveDate) -> Result<ReservationChangeRequestRow>`
  - `approve_change_request(pool, id: Uuid, responder_id: Uuid, response_note: Option<&str>, settlement_status: ChangeSettlementStatus, gateway_reference: Option<&str>, supplier: &SupplierInfo, today_jm: NaiveDate) -> Result<(ReservationChangeRequestRow, Booking)>`:
    1. Acquires locks in global hierarchy: `listing FOR UPDATE` $\rightarrow$ `booking FOR UPDATE` $\rightarrow$ `reservation_change_request FOR UPDATE`.
    2. Verifies request is `pending` and `expires_at_date > today_jm` (if expired, updates status to `expired`, commits, and returns `DbError::BookingNotModifiable`).
    3. Verifies booking is `Confirmed`, `date_to > today_jm`, and any `host_payout_ledger` entry is `pending`.
    4. Re-verifies date availability (`check_availability_excluding`) if dates changed.
    5. Updates `booking` (`date_from`, `date_to`, `number_of_persons`, `metadata`, and all financial columns from `pricing_snapshot`).
    6. Inserts `booking_history` entry with `changed_by_id = Some(responder_id)` and descriptive reason.
    7. Calls `tax_document::issue_payment_receipt` (which supersedes active `REC` and issues `REC-YYYY-{code}-R{n}` with `amount_previously_paid` and `balance_due`).
    8. Updates `pending` `host_payout_ledger` entry with recalculated gross, platform fee, and net payout.
    9. Marks `reservation_change_request` as `approved` with `settlement_status`, `gateway_reference`, `responder_id`, `response_note`, and `responded_at = NOW()`.
- `db_core/src/booking.rs`:
  - Update `update_booking` so direct Admin date/guest updates also support `number_of_persons`, and cancelling a booking automatically cancels any `pending` `reservation_change_request`.

### 3.4. Backend Microservices (`listing_api`, `booking_api`, `email_worker`)
- `app_api/listing_api/src/apis.rs`:
  - Validate `guests_included >= 1` (`<= max_guests`) and `0 <= extra_guest_fee <= 1_000_000`.
- `app_api/booking_api/src/apis.rs`:
  - Update `create_booking` to pass `guests_included` and converted `extra_guest_fee` into `BookingCalculator::with_extra_guests(...)` and enforce `number_of_persons <= listing.max_guests`.
  - Restrict direct date/guest mutation in `PATCH /api/v1/bookings/{id}` to `UserRole::Admin` only (returning `403 Forbidden` if a non-admin guest or host attempts direct mutation).
  - Add endpoints under `/api/v1/bookings`:
    - `POST /api/v1/bookings/{id}/change-quote` $\rightarrow$ `ReservationChangeQuoteResponse`
    - `POST /api/v1/bookings/{id}/change-requests` $\rightarrow$ `201 Created` (`ReservationChangeRequestResponse`)
    - `GET /api/v1/bookings/{id}/change-requests` $\rightarrow$ `200 OK` (`Vec<ReservationChangeRequestResponse>`)
    - `POST /api/v1/bookings/{id}/change-requests/{request_id}/approve` $\rightarrow$ `200 OK` (`ReservationChangeRequestResponse`)
    - `POST /api/v1/bookings/{id}/change-requests/{request_id}/decline` $\rightarrow$ `200 OK` (`ReservationChangeRequestResponse`)
    - `POST /api/v1/bookings/{id}/change-requests/{request_id}/cancel` $\rightarrow$ `200 OK` (`ReservationChangeRequestResponse`)
  - Extend `PATCH /api/v1/internal/cron/complete-stays` to also expire past-due `pending` reservation change requests (`expire_past_due_change_requests`).
- `app_api/email_worker`:
  - Add HTML templates `reservation_change_requested.html`, `reservation_change_approved.html`, and `reservation_change_declined.html` and register them in `processor.rs`.

### 3.5. Topcoat SSR Frontends (`web_app_common_tc`, `web_app_tc`, `web_app_admin_tc`)
- `web_app_common_tc`:
  - Add client methods for change-quote and change-requests CRUD/actions.
- `web_app_tc`:
  - `pages/booking_date_change.rs` (`/bookings/{id}/change-dates`): upgrade to a full "Modify Reservation (Dates & Guests)" page supporting pre-stay and mid-stay rules (disabling check-in date input when `today_jm >= booking.date_from`), adult/children/infant/pet inputs capped at `listing.max_guests`, live quote preview showing `balance_due`, and submitting a change request for host approval.
  - `pages/bookings.rs` (`/bookings`): display active `pending` change request banner on each booking card:
    - If `initiated_by_role == Host` (or Admin for Guest): show proposed dates/guests, price delta (`balance_due`), and **"Approve Change"** / **"Decline"** buttons.
    - If `initiated_by_role == Guest`: show "Change Request Pending Host Approval" badge and **"Withdraw Request"** button.
- `web_app_admin_tc`:
  - `pages/listings_admin.rs`: add `guests_included` and `extra_guest_fee` inputs to the listing create/edit forms.
  - `pages/bookings_admin.rs`: display pending guest-initiated change requests with **"Approve Change"** / **"Decline"** actions (showing both guest `balance_due` and `host_payout_delta`), plus a modal/form for Hosts/Admins to propose a reservation change to the guest (or force-apply as Admin).

---

## 4. Unit & Integration Test Cases
1. **Unit (`common::pricing`)**:
   - Extra-guest fee within `guests_included` adds `0.00`.
   - Extra-guest fee above `guests_included` adds `extra_guests * extra_guest_fee * total_days` to `sub_total_price`.
   - Weekly (7+ nights) and monthly (28+ nights) discounts apply to the combined `sub_total_price` (base + extra-guest fee).
   - Reconciliation property test holds across arbitrary nightly rates, extra-guest counts, discounts, cleaning fees, and damage deposits.
   - Pre-stay vs. mid-stay date validation tests: changing `date_from` when `current.date_from <= today_jm` fails; changing `date_to` when `current.date_from <= today_jm < proposed_date_to` succeeds; shortening `date_to` below ` proposed_date_from` or `<= today_jm` fails.
2. **Integration (`db_core` & `booking_api`)**:
   - Guest creates change request (dates + extra guests) $\rightarrow$ status is `pending`, availability not blocked for other queries, duplicate `pending` request returns `409`.
   - Guest cannot approve own request (`403`); Host approves request $\rightarrow$ `booking` updated, `REC-YYYY-{code}-R2` issued with `balance_due`, `host_payout_ledger` updated, settlement status `charged` / `refunded`.
   - Host initiates change request (mid-stay checkout change or guest count change) $\rightarrow$ Guest approves $\rightarrow$ booking, receipt, and payout ledger updated.
   - Approval fails with `409 DATE_UNAVAILABLE` if another confirmed booking took the requested extension dates before approval.
   - Pending change request auto-expires when `expires_at_date <= today_jm`.
   - Non-admin direct `PATCH /api/v1/bookings/{id}` with `date_from`/`date_to`/`number_of_persons` returns `403`; Admin direct `PATCH` succeeds.
3. **Playwright End-to-End (`playwright/e2e/`)**:
   - **Guest Portal (`playwright/e2e/guest-portal/reservation-changes.spec.ts`)**:
     - Form Rendering & Dynamic Pricing Preview: Navigate to `/bookings/{id}/change-dates`, verify check-in/out inputs, guest count breakdown controls (adults, children, infants, pets), input change triggering `#btn-preview-change-quote`, and verify `#change-quote-preview` renders updated subtotal, extra-guest fee total, statutory Jamaican GCT (15%), and positive/negative `balance_due`.
     - Request Submission: Guest submits modification request via `#btn-submit-date-change`, verifies success alert (`#date-change-success`) and non-reload transition.
     - Bookings List Status & Cancellation: Navigate to `/bookings`, verify active pending change banner on booking card, and verify "Withdraw Request" button cancels the pending change request.
     - Mid-Stay Boundary Behavior: For an active mid-stay booking (`current.date_from <= today_jm < current.date_to`), navigate to `/bookings/{id}/change-dates`, verify `#mid-stay-notice` is displayed, `#new-check-in` is `readonly`, and `#new-check-out` and guest count inputs remain editable.
     - Host-Initiated Change Review & Approval: Navigate to `/bookings` when host has proposed a change, verify counterparty card displays proposed terms and price delta, click "Approve Change", and verify status updates to approved with revised total.
   - **Admin Portal (`playwright/e2e/admin-portal/reservation-changes-admin.spec.ts`)**:
     - Listing Extra-Guest Fee Configuration: Navigate to `/admin/listings` (or listing editor), verify `guests_included` and `extra_guest_fee` input fields, update values, submit, and verify persistence upon reload.
     - Master Booking Schedule & Change Audit: Navigate to `/admin/bookings`, open booking details modal (`#admin-booking-details-dialog`), verify `#admin-booking-change-requests-section` displays pending guest-initiated change requests with dates, guest breakdown, guest `balance_due`, and host payout delta.
     - Host/Admin Approval & Receipt Supersession: Click "Approve Change" in audit modal, verify request status transitions to `approved`, and verify revised receipt (e.g. `REC-YYYY-{code}-R2`) appears under `#admin-booking-documents-list`.
     - Propose Change to Guest: Use host change proposal form in booking details modal to submit proposed terms to guest, and verify new pending request row.
     - Admin Force-Apply Direct Change: Execute administrative direct update and verify booking updates immediately.
   - **Backend API Direct Validation (`playwright/e2e/api/reservation-changes-api.spec.ts`)**:
     - `POST /api/v1/bookings/{id}/change-quote` calculations and pricing snapshot accuracy.
     - `POST /api/v1/bookings/{id}/change-requests` creation and duplicate request prevention (`409 CHANGE_REQUEST_ALREADY_PENDING`).
     - Authorization enforcement: initiator self-approval rejected (`403 FORBIDDEN`), non-admin direct `PATCH /api/v1/bookings/{id}` rejected (`403 FORBIDDEN`).
     - State machine lifecycle: decline, cancel/withdraw, and approve endpoints.

---

## 5. Acceptance Criteria
- [ ] `listing` and `listing_history` persist `guests_included` and `extra_guest_fee`; admin listing forms and APIs support creating/updating them.
- [ ] `BookingCalculator` includes `extra_guest_fee` in `sub_total_price` so weekly/monthly discounts apply to the overall stay, and both guest invoices and host payouts reflect extra-guest charges.
- [ ] Pre-stay and mid-stay validation rules allow check-in changes only before check-in arrives, and check-out/guest changes before or during an active stay.
- [ ] Guest-initiated change requests require Host (or Admin) approval; Host-initiated change requests require Guest (or Admin) approval; Admins can propose or force-apply.
- [ ] Counterparty approval acquires `FOR UPDATE` locks, executes payment gateway settlement for `balance_due`, updates `booking` + `booking_history`, supersedes and issues `REC-YYYY-{code}-R{n}`, and updates `host_payout_ledger`.
- [ ] Pending change requests auto-expire once their relevant check-in or check-out cutoff date arrives.
- [ ] Transactional emails (`reservation_change_requested`, `reservation_change_approved`, `reservation_change_declined`) render and queue via `email_outbox`.
- [ ] Playwright E2E test suites (`guest-portal-chromium`, `admin-portal-chromium`, `backend-api`) pass with 100% green runs.
- [ ] All quality gates in `CONSTRAINTS.md` pass (`cargo fmt --check`, `cargo clippy -D warnings`, `cargo sqlx prepare --check`, unit, integration & E2E tests).
