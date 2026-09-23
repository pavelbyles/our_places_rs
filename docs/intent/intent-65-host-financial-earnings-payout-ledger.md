# Intent: Host Financial Earnings & Payout Ledger

## Metadata
- **Author**: Originator (via GitHub Issue #65)
- **Date**: 2026-09-20
- **Status**: Approved
- **Target Area**: Fullstack (`db_core`, `common`, `app_api/booking_api`, `web_app_admin_tc`)

---

## 1. Problem Statement & Motivation
Currently, when a guest completes a booking, the platform records the gross reservation amount and guest fees, but there is no dedicated, immutable financial ledger tracking host earnings, platform commission deductions, statutory tax withholdings, currency conversion adjustments, or payout execution.

This creates several operational and accounting challenges:
- **Lack of Transparency**: Property hosts and partners cannot inspect net earnings, deductions, or payout statuses for their villas in one unified dashboard.
- **Manual Reconciliation**: Platform administrators must manually calculate host payouts, exchange rates, and commission fees using external spreadsheets, which is prone to human error and rounding discrepancies.
- **Audit & Tax Exposure**: Absence of an immutable ledger makes statutory tax reporting (e.g. Jamaican GCT compliance) and end-of-year tax reconciliation difficult to audit.
- **Payment Gateway Disconnect**: No mechanism exists to link host payouts to external payment rails or gateways (such as MPGS or Powertranz) for transaction tracking and post-settlement reversals.

Solving this establishes a double-entry ready financial ledger tied to booking lifecycles, with automated status transitions, exportable accounting reports, and administrator payout controls.

---

## 2. Proposed Outcome (The Vision)
A robust, tamper-resistant financial ledger and payout tracking system:
1. **Automated Ledger Allocation**: Confirmation of a reservation automatically creates an immutable ledger entry recording the host's gross accommodation subtotal, property-specific platform commission, currency exchange rate snapshot, and calculated net payout.
2. **Type-Safe Payout State Machine**: Payouts follow a strict lifecycle (`pending` $\rightarrow$ `processing` $\rightarrow$ `paid`, with terminal states `cancelled` and `refunded`) enforced by Rust's type system to prevent double-disbursements or invalid state jumps.
3. **Administrator Dashboard (`web_app_admin_tc`)**: Platform administrators can review real-time financial metrics (gross revenue, commission fees, net payouts, pending disbursements), filter entries across villas and date ranges, update payout statuses with gateway transaction references, and trigger CSV downloads for accounting.
4. **Gateway Integration Readiness**: Native schema support for external payment identifiers (`gateway_reference`, `failure_reason`) enabling seamless hooks into Mastercard Payment Gateway Services (MPGS) or Powertranz.
5. **Auditable Tax & Currency Handling**: Tri-currency aware calculations with snapshot exchange rates and pure decimal arithmetic guaranteeing 0 floating-point rounding errors.

---

## 3. Scope Boundaries

### In Scope (MVP)
- **Data Persistence (`db_core`)**:
  - PostgreSQL ENUM `payout_status`: `'pending'`, `'processing'`, `'paid'`, `'cancelled'`, `'refunded'`.
  - Property commission configuration on `listing` (`commission_pct DECIMAL(5, 4) NOT NULL DEFAULT 0.0000`) and mirrored on `listing_history`.
  - PostgreSQL table `host_payout_ledger` with foreign keys to `booking`, `listing`, and `user`, monetary amounts stored as `DECIMAL(12, 2)`, exchange rates as `DECIMAL(12, 6)`, `gateway_reference`, and audit timestamps.
  - Efficient indexes on `host_id`, `listing_id`, `status`, `booking_id`, and `gateway_reference`.
- **Domain Logic & Precision Math (`common`)**:
  - Pure `rust_decimal::Decimal` calculation function `calculate_host_payout`.
  - Type-system enforced state transitions via compile-time typestates (`PayoutRecord<State>`) and runtime validation (`PayoutStatus::transition_to`).
  - Standardized RFC 4180 CSV serializer for accounting report exports.
- **Booking State Integration (`app_api/booking_api`)**:
  - Synchronous ledger record creation inside the booking confirmation transaction (`BookingStatus::Pending` $\rightarrow$ `BookingStatus::Confirmed`).
  - Automatic cancellation sync when a booking is cancelled while the payout is `pending`.
- **API Endpoints (`app_api/booking_api`)**:
  - `GET /api/v1/hosts/ledger`: Paginated and filterable ledger entries (with host isolation).
  - `GET /api/v1/hosts/ledger/summary`: Aggregated financial stats (total gross, fees, net, paid, pending).
  - `GET /api/v1/hosts/ledger/export`: Downloadable `text/csv` accounting export.
  - `PATCH /api/v1/admin/ledger/{id}/status`: Admin-only status updates with optional gateway reference.
- **Admin UI (`web_app_admin_tc`)**:
  - Dedicated `/admin/payouts` dashboard with Topcoat SSR metric cards, HTMX filter bar, real-time status update actions, and CSV download.

### Out of Scope / Non-Goals
- Automated bank ACH/wire transfer execution (payout execution is manually triggered or recorded via gateway references).
- Automated backfill of historical bookings prior to migration deployment (forward-looking only).
- Direct host-facing UI in `web_app_tc` (partners and hosts manage villas exclusively via `web_app_admin_tc` for MVP).
- Automated clawback deductions from future bookings on refunded payouts (flagged for manual admin handling).

---

## 4. Affected Systems & Stakeholders

- **User Personas**:
  - **Property Hosts / Partners**: Villa owners receiving disbursements and tracking net earnings per property.
  - **Platform Administrators**: Financial managers reviewing earnings, approving payout batches, and exporting accounting reports.
- **System Components**:
  - `db_core`: Migration for `host_payout_ledger` table, `payout_status` enum, `listing.commission_pct`, and SQLx entity queries.
  - `common`: DTOs, typestate state machine, `rust_decimal` pricing arithmetic, and CSV export utilities.
  - `app_api/booking_api`: Booking transaction hooks, ledger endpoints, authorization guards.
  - `web_app_admin_tc`: Topcoat SSR dashboard `/admin/payouts` and HTMX controls.

---

## 5. Constraints & Non-Negotiable Invariants

- **Financial Precision ("Tri-Currency" Rule)**:
  - Absolute ban on floating-point types (`f32`/`f64`) for money, percentages, and rates. Strictly use `rust_decimal::Decimal`.
- **Zero Double-Disbursements & Concurrency**:
  - Database row-level locks (`SELECT ... FOR UPDATE`) and `UNIQUE(booking_id)` constraint prevent race conditions or duplicate ledger rows.
- **Zero Panic Policy**:
  - Monadic error propagation (`Result<T, E>`) across all code paths. No `.unwrap()` or `.expect()` in production or HTTP handlers.
- **Data Isolation & Least Privilege**:
  - Hosts can only access their own ledger records (`WHERE host_id = claims.sub`).
  - Status mutations restricted to authenticated admins (`claims.roles.contains(&UserRole::Admin)`).
- **Scale-to-Zero Performance Target**:
  - Memory-conscious CSV generation within Cloud Run limits (256MB container).
  - Indexed queries ensuring sub-100ms response times.

---

## 6. Success Metrics & Acceptance Signals

- **Leading Indicators**:
  - 100% test pass rate across `cargo test -p common`, `db_core`, and `booking_api`.
  - Zero compiler warnings (`cargo clippy --workspace --all-targets -- -D warnings`).
  - Successful execution of status transition tests including rejection of illegal transitions (e.g. `paid` $\rightarrow$ `pending`).
- **Lagging Indicators**:
  - 100% match between gross booking amounts, platform fees, and net payouts across all confirmed reservations.
  - Zero discrepancies during monthly financial accounting and tax reconciliation.

---

## 7. Resolved Design Decisions (from /grill-me)

- [x] **Gross Calculation Base**: Defined as Room Subtotal (`discounted_subtotal`, excluding guest taxes & guest fees).
- [x] **Ledger Currency**: Denominated in host's preferred currency (`user.default_currency`) with snapshot `exchange_rate` applied from `currency_exchange_rates`.
- [x] **Commission Model**: Property-level only (`listing.commission_pct DECIMAL(5, 4) NOT NULL DEFAULT 0.0000`).
- [x] **Tax Withholding**: Default `0.00` withholding from host (guest pays Jamaican 15% GCT at checkout; platform remits).
- [x] **Payout State Machine**: `pending` $\rightarrow$ `processing` $\rightarrow$ `paid` (with terminal states `cancelled` and `refunded`).
- [x] **Cancellation Rules**: Auto-cancel pending ledger entries on booking cancellation; preserve paid entries for manual admin review.
- [x] **Historical Data**: Forward-looking only (no retroactive backfill of historical bookings).
- [x] **UI Placement**: Exclusively in `web_app_admin_tc` (`/admin/payouts`).
