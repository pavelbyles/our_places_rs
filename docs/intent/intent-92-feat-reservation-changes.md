# Intent: Bilateral Reservation Changes (Dates & Guest Count)

## Metadata
- **Author**: Originator (via GitHub Issue #92)
- **Date**: 2026-10-17
- **Status**: Approved
- **Target Area**: Fullstack (`common`, `db_core`, `app_api/listing_api`, `app_api/booking_api`, `app_api/email_worker`, `web_app_common_tc`, `web_app_tc`, `web_app_admin_tc`)

---

## 1. Problem Statement & Motivation
Currently, post-booking modifications on confirmed reservations have three critical gaps:
1. **Unilateral Date Changes Without Counterparty Consent**: `PATCH /api/v1/bookings/booking/{id}` allows a guest to immediately mutate `date_from` and `date_to` on a `Confirmed` booking without host approval, while hosts cannot initiate date changes for guests to review and approve.
2. **Immutable Guest Counts & No Extra-Guest Pricing**: Neither guests nor hosts can modify `number_of_persons` (or guest breakdown metadata) after booking creation. Furthermore, `Listing` only tracks `max_guests` without configurable base included guests or per-extra-guest nightly fees, meaning guest count adjustments cannot accurately reflect host payout or guest invoicing changes.
3. **Lack of Structured Change Request Lifecycle & Quote Preview**: Counterparties have no way to preview the exact financial delta (revised accommodation total, extra-guest fees, GCT taxes, platform fee, balance due/refundable, and net host payout change) before approving or declining a proposed reservation change.

---

## 2. Proposed Outcome (The Vision)
1. **Listing Extra-Guest Pricing Configuration**:
   - Listings support optional extra-guest pricing via `guests_included` (default `1` or `max_guests` when no extra fee applies) and `extra_guest_fee` (`Decimal`, default `0.00`, in listing `base_currency` per extra guest per night above `guests_included`, strictly capped at `max_guests`).
   - Extra-guest charges are included in the overall stay accommodation (`sub_total_price`), so weekly and monthly percentage discounts apply to the overall stay (including `extra_guest_fee`), feeding directly into guest invoicing (GCT tax base, platform fee, total price) and host payout (`host_payout_ledger` gross and net payout).
2. **Bilateral Reservation Change Request Workflow**:
   - **Guest-Initiated**: Guest proposes a change to `date_from`, `date_to`, and/or `number_of_persons` (with updated `num_adults`/`num_children` breakdown) on a `Confirmed` booking $\rightarrow$ creates a `pending` `ReservationChangeRequest` awaiting **Host** (or Admin) approval.
   - **Host-Initiated**: Host proposes a change to `date_from`, `date_to`, and/or `number_of_persons` on a `Confirmed` booking $\rightarrow$ creates a `pending` `ReservationChangeRequest` awaiting **Guest** (or Admin) approval.
   - **Admin Override**: Administrators may either initiate a bilateral request on behalf of a party or force-apply a reservation change directly.
   - Counterparties can **Approve** or **Decline** a pending request; the initiator can **Cancel/Withdraw** their own pending request before a decision is made. Only one `pending` change request may exist per booking at any time.
   - **Auto-Expiry**: A `pending` change request automatically transitions to `expired` if its relevant cutoff date (`America/Jamaica`) arrives before counterparty approval:
     - If `date_from` is being changed: expires when `min(current.date_from, new_date_from) <= today`.
     - If only `date_to` and/or guest count is being changed (before or during an active stay): expires when `min(current.date_to, new_date_to) <= today`.
3. **Pre-Stay & Mid-Stay Date Validation & Atomic Locking on Approval**:
   - **Check-In vs. Check-Out Date Rules (evaluated on both Request Creation and Counterparty Approval in `America/Jamaica`)**:
     - **If `date_from` (check-in) is changing** (`new_date_from != current.date_from`): verify that the check-in date has not arrived and is not in the past (`current.date_from > today` and `new_date_from > today`), and that the target range is available.
     - **If `date_to` (check-out) is changing** (`new_date_to != current.date_to`): verify that `new_date_to > today` (in the future), `new_date_to > target_date_from`, minimum-stay rules are met, and the target range is available (note: `new_date_to` may be prior to `current.date_to` for an early departure/shortened stay, or after `current.date_to` for an extension, before or during a stay).
     - **If both `date_from` and `date_to` are changing**: validate both conditions above.
     - **If only guest count (`number_of_persons`) is changing**: allowed before or during a stay as long as `current.date_to > today` and `1 <= number_of_persons <= listing.max_guests`.
   - **On Counterparty Approval**: Re-verify the applicable date rules and availability under a PostgreSQL row-level lock (`SELECT ... FOR UPDATE`), execute or record payment settlement for any `balance_due`, atomically update the `booking` record, record an immutable `booking_history` entry, supersede and issue a revised Payment Receipt (`REC-YYYY-{code}-R{n}`), and update the `pending` entry in `host_payout_ledger`.
4. **Full Stay Re-Pricing, Financial Reconciliation & Payment Gateway Readiness**:
   - Approved changes recalculate the full stay using the booking's snapshotted `exchange_rate` and `tax_rate`, current active `PriceOverride` seasonal rates for any new dates, and the listing's extra-guest fee rules (with weekly/monthly discounts applied to the overall stay including `extra_guest_fee`).
   - The revised Payment Receipt (`REC-YYYY-{code}-R{n}`) records `amount_previously_paid` and the resulting positive or negative `balance_due` (`new_total_price - old_total_price`).
   - **Payment Gateway Settlement Architecture**: Track settlement lifecycle directly on `reservation_change_request` (`settlement_status`, `balance_due`, `gateway_reference`) and orchestrate approval through a clean payment settlement boundary so that live payment gateway capture (`balance_due > 0`) and refund execution (`balance_due < 0`) plug in seamlessly.
   - The `pending` `host_payout_ledger` entry is updated to reflect the revised net accommodation (including discounted extra-guest charges).

---

## 3. Scope Boundaries

### In Scope (MVP)
- **Database (`db_core`)**:
  - Single SQLx migration adding `guests_included` and `extra_guest_fee` to `listing` (and `listing_history`), plus a `reservation_change_request` table tracking proposed `date_from`, `date_to`, `number_of_persons`, guest metadata breakdown, snapshotted financial quote preview (old vs. new totals, guest `balance_due`, host payout delta), payment settlement fields (`settlement_status`, `gateway_reference`), `initiated_by_id`, `initiated_by_role` (`guest` | `host` | `admin`), `status` (`pending` | `approved` | `declined` | `cancelled` | `expired`), `responder_id`, `reason` / `response_note`, and timestamps.
- **Domain Pricing (`common`)**:
  - Extend `BookingCalculator` / quote calculation to include extra-guest charges (`max(0, number_of_persons - guests_included) * extra_guest_fee * total_days`) in `sub_total_price` in `rust_decimal::Decimal` so weekly/monthly discounts apply to the overall stay and the reconciliation invariant holds across quotes, bookings, tax documents, and host payouts.
- **Listing API (`app_api/listing_api`)**:
  - Expose `guests_included` and `extra_guest_fee` in listing create/update/read DTOs and quote endpoints.
- **Booking API (`app_api/booking_api`)**:
  - Quote preview endpoint for proposed reservation changes (`GET/POST /api/v1/bookings/booking/{id}/change-quote`).
  - CRUD/lifecycle endpoints for reservation change requests: create (`POST`), list/get (`GET`), approve (`POST .../approve`), decline (`POST .../decline`), and withdraw/cancel (`POST .../cancel`), including a payment-gateway-ready settlement hook for positive/negative `balance_due`.
  - Restrict direct date/guest mutation on `PATCH /api/v1/bookings/booking/{id}` to Administrators only (Guests and Hosts must use the change-request workflow).
- **Notifications (`app_api/email_worker` & `db_core::email_outbox`)**:
  - Transactional email notifications queued via `email_outbox` when a reservation change is requested, approved (with revised receipt link), or declined.
- **Frontend Portals (`web_app_common_tc`, `web_app_tc`, `web_app_admin_tc`)**:
  - Admin listing form inputs for `guests_included` and `extra_guest_fee`.
  - Guest portal (`web_app_tc`): update reservation modification UI to support both date and guest count changes (before or during stay per date rules) with live price/delta preview, submit a change request to the host, and review/approve/decline host-initiated change requests.
  - Host/Admin portal (`web_app_admin_tc`): initiate a reservation change request (dates and/or guest count) to the guest, review/approve/decline guest-initiated requests, and allow Admin direct-apply when needed.
- **End-to-End Test Automation (`playwright/e2e/`)**:
  - Automated Playwright browser tests across Guest and Admin portals validating:
    - Guest reservation modification form, live quote preview, and request submission.
    - Mid-stay date locking boundary rules (check-in locked, check-out and guests editable).
    - Guest booking list pending change banners, withdrawal, and counterparty approvals.
    - Admin listing extra-guest fee configuration and persistence.
    - Admin/Host master booking audit details modal: reviewing, approving, and declining change requests, plus proposing changes to guests.
    - Direct API E2E validation for quote pricing, atomic locks, anti-duplication, and bilateral authorization enforcement.

### Out of Scope / Non-Goals
- Third-party payment processor credentials/SDK wiring (the settlement state machine, database columns, and gateway trait/hook for positive `balance_due` capture and negative `balance_due` refund are included in scope so live gateway integration is a drop-in adapter).
- Applying cancellation policy penalties to shortened stays during an approved reservation change (approved changes recalculate the stay at full applicable rates).
- Modifying `Completed` or `Cancelled` bookings, or modifying reservations whose check-out date has already passed.
- Changing the property (`listing_id`) or currency (`currency`) of an existing booking.

---

## 4. Affected Systems & Stakeholders
- **User Personas**:
  - **Guests**: Request date or guest count changes, preview price adjustments, and approve/decline host-initiated changes.
  - **Hosts**: Configure extra-guest fees on listings, review/approve/decline guest change requests, and propose date or guest count changes to guests.
  - **Administrators**: Manage change requests across all bookings or force-apply administrative changes.
- **System Components**:
  - `common` (pricing calculator, DTOs, validation)
  - `db_core` (schema migration, `listing`, `booking`, `reservation_change_request`, `tax_document`, `payout_ledger`, `email_outbox`)
  - `app_api/listing_api` & `app_api/booking_api` (REST handlers, OpenAPI specs)
  - `app_api/email_worker` (email templates for change requested / approved / declined)
  - `web_app_common_tc`, `web_app_tc`, `web_app_admin_tc` (Topcoat SSR & HTMX UI views)

---

## 5. Constraints & Non-Negotiable Invariants
- **Domain & Financial Invariants**:
  - **Zero Floats**: All monetary amounts, rates, and fee deltas MUST use `rust_decimal::Decimal` (`NUMERIC`).
  - **Reconciliation Invariant**: Every revised booking and `REC-YYYY-{code}-R{n}` receipt must satisfy:
    `total_price = (sub_total_price - discount_value) + cleaning_fee + tax_value + platform_fee + damage_deposit`
    where extra-guest charges are deterministically accounted for in both guest invoicing and host payout calculations.
  - **Guest Capacity Bound**: `1 <= number_of_persons <= listing.max_guests` and `number_of_persons == metadata.num_adults + metadata.num_children` (with `num_adults >= 1`).
- **Concurrency & Locking**:
  - Approving a reservation change MUST acquire PostgreSQL row-level locks (`SELECT ... FOR UPDATE`) on the target booking and overlapping reservations before mutating dates, issuing revised tax documents, or updating `host_payout_ledger`.
  - At most one `pending` change request per `booking_id` enforced via partial unique index (`UNIQUE (booking_id) WHERE status = 'pending'`).
- **Performance & Code Quality Budgets**:
  - Cloud Run cold start $< 300\text{ms}$ (p50), $< 1\text{s}$ (p95) on $256\text{MB}$ RAM.
  - Zero `.unwrap()` or `.expect()` in production crates; strict monadic error handling and `#[tracing::instrument]` on async handlers.
  - Single immutable migration file in `db_core/migrations/`.

---

## 6. Success Metrics & Signals
- **Leading Indicator**: 100% of guest- and host-initiated date/guest changes transition through `pending` $\rightarrow$ `approved`/`declined` with atomic row-level availability verification and zero double-bookings.
- **Financial Accuracy Signal**: Property and integration tests verify that any approved date or guest count change produces a revised `REC-YYYY-{code}-R{n}` receipt whose `balance_due` equals `new_total_price - old_total_price` and updates the `pending` `host_payout_ledger` entry to match the recalculated net accommodation.
- **Quality Gate**: `cargo fmt --check`, `SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features -- -D warnings`, and full test suites pass with zero warnings.
- **E2E Playwright Reliability**: 100% pass rate across Guest Portal, Admin Portal, and Backend API Playwright test suites (`guest-portal-chromium`, `admin-portal-chromium`, `backend-api`) in both CI and local test execution.

---

## 7. Open Questions & Policy Concerns
- [x] **Guest count pricing model**: Add optional `guests_included` and `extra_guest_fee` (per extra guest per night, capped at `max_guests`) to `Listing`.
- [x] **Approval workflow**: Guest-initiated changes require Host approval; Host-initiated changes require Guest approval; Admins can either propose or force-apply.
- [x] **Calendar availability & pre/mid-stay date rules**: Check-in changes require `current.date_from > today` and `new_date_from > today`; check-out changes require `new_date_to > today` and availability (allowing early check-out or extension before or during a stay); validate both when both change. Re-verify with `SELECT ... FOR UPDATE` on approval.
- [x] **Price delta, refund rules & payment gateway readiness**: Recalculate full stay at snapshotted exchange/tax rates (plus active seasonal overrides for new dates), issue revised Payment Receipt (`REC-YYYY-{code}-R{n}`) with positive/negative `balance_due`, update the pending `host_payout_ledger` entry, and include payment settlement status/gateway hooks for `balance_due`.
- [x] **Extra-Guest Fee Discount & Payout Treatment**: Weekly/monthly percentage discounts are determined based on the overall stay and apply with consideration of `extra_guest_fee` (included in `sub_total_price` prior to discount and tax calculation, and included in host gross payout).
- [x] **Pending Request Auto-Expiry**: Any `pending` change request automatically transitions to `expired` if its applicable check-in (when changing `date_from`) or check-out cutoff date arrives before counterparty approval.
