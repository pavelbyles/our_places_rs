# Spec 65: Host Financial Earnings & Payout Ledger

## Overview
This feature introduces a financial ledger and payout tracking module for property hosts and platform administrators. Whenever a booking is confirmed, a corresponding ledger entry is created to track gross booking value, platform commission percentages, statutory tax withholdings, currency conversion adjustments, and net host payout amounts. Platform admins can view aggregated financial summaries, filter earnings by property and status, mark payouts as processed, and export formatted CSV financial reports for accounting and tax filing in the admin portal (`web_app_admin_tc`).

---

## Key Decisions & Domain Rules

### 1. Gross Booking Value & Calculation Base
- **Gross Amount Definition**: `gross_amount` is defined as the **Room Subtotal** (`discounted_subtotal` = nights × nightly rate minus stay discounts), excluding guest taxes (GCT 15%) and platform guest service fees.
- **Host Net Payout Formula**:
  $$\text{platform\_fee\_amount} = \text{gross\_amount} \times \text{platform\_fee\_pct}$$
  $$\text{net\_payout\_amount} = \text{gross\_amount} - \text{platform\_fee\_amount} - \text{tax\_withheld\_amount}$$
- **Strict Decimal Arithmetic**: All monetary amounts, percentages, and exchange rates strictly use `rust_decimal::Decimal` (0 `f32`/`f64`).

### 2. Currency Denomination & Tri-Currency Rules
- **Denominated in Host's Preferred Currency**: Ledger records are denominated in the host's `user.default_currency`.
- **Exchange Rate Snapshot**: If the listing's `base_currency` differs from the host's `default_currency`, the exchange rate is retrieved from `currency_exchange_rates` at confirmation time and stored immutably in `exchange_rate`. If currencies match, `exchange_rate` is `1.000000`.

### 3. Commission Configuration
- **Listing-Level Commission**: Stored directly on `listing` as `commission_pct DECIMAL(5, 4) NOT NULL DEFAULT 0.0000`.
- Platform admins configure commission per property. At booking confirmation time, `listing.commission_pct` is snapshotted to `host_payout_ledger.platform_fee_pct`.

### 4. Statutory Tax Withholdings
- **Zero Withholding Default**: For Jamaican properties, the 15% GCT is paid by the guest at checkout and remitted by the platform. `tax_withheld_amount` is initialized to `0.00` unless a statutory withholding rule applies.

### 5. Payout Lifecycle & Execution
- **State Machine**: `pending` $\rightarrow$ `processing` $\rightarrow$ `paid` (with terminal states `cancelled` for pre-disbursement voids or `refunded` for post-disbursement reversals).
- **Trigger**: Generated synchronously with status `pending` upon booking status transition to `confirmed`.
- **Manual & Gateway Execution**: Platform admins review pending earnings and manually transition statuses or update them when settlement is processed via payment gateways (e.g. MPGS, Powertranz).
- **Historical Bookings**: Forward-looking only; no retroactive backfill of historical bookings.

### 6. Booking Cancellations & Reversals
- **Pending Payouts**: If a booking is cancelled while the payout is `pending`, the ledger record status automatically transitions to `cancelled`.
- **Paid / Processing Payouts**: If the payout is already `processing` or `paid`, the ledger entry remains intact, and a warning is logged/flagged for manual admin intervention or subsequent status update to `refunded`.

### 7. CSV Report Generation
- Standardized, downloadable CSV format containing:
  `Ledger ID`, `Booking Confirmation`, `Property Name`, `Check-In Date`, `Check-Out Date`, `Gross Amount`, `Currency`, `Platform Fee Pct`, `Platform Fee Amount`, `Tax Withheld`, `Net Payout Amount`, `Status`, `Gateway Reference`, `Payout Date`, `Created At`.

---

## Technical Implementation

### 1. `db_core` (Database & Schema)
- **Migration**: `db_core/migrations/20260920000000_create_host_payout_ledger.sql`
  - Add `commission_pct DECIMAL(5, 4) NOT NULL DEFAULT 0.0000` to `listing`.
  - Add `commission_pct DECIMAL(5, 4) NOT NULL DEFAULT 0.0000` to `listing_history`.
  - Create ENUM `payout_status`: `'pending'`, `'processing'`, `'paid'`, `'cancelled'`, `'refunded'`.
  - Create TABLE `host_payout_ledger`:
    - `id` (UUID, PK, default `uuidv7()`)
    - `booking_id` (UUID, UNIQUE, FK to `booking(id)`)
    - `listing_id` (UUID, FK to `listing(id)`)
    - `host_id` (UUID, FK to `"user"(id)`)
    - `currency` (CHAR(3))
    - `gross_amount` (DECIMAL(12, 2))
    - `platform_fee_pct` (DECIMAL(5, 4))
    - `platform_fee_amount` (DECIMAL(12, 2))
    - `tax_withheld_amount` (DECIMAL(12, 2))
    - `exchange_rate` (DECIMAL(12, 6))
    - `net_payout_amount` (DECIMAL(12, 2))
    - `status` (payout_status)
    - `gateway_reference` (VARCHAR(255), NULLABLE)
    - `failure_reason` (TEXT, NULLABLE)
    - `payout_date` (TIMESTAMPTZ, NULLABLE)
    - `created_at` (TIMESTAMPTZ)
    - `updated_at` (TIMESTAMPTZ)
  - Create indexes on `host_id`, `listing_id`, `status`, `booking_id`, and `gateway_reference`.

- **SQLx Entities & Queries**:
  - `db_core/src/payout_ledger.rs` [NEW]: SQLx queries for inserting ledger records, fetching host ledger listings with pagination/filters, computing aggregated totals, updating payout statuses with optional gateway reference/notes, and handling cancellations.

### 2. `common` (Isomorphic Crate)
- **`common/src/models/payout.rs` [NEW]**:
  - `PayoutLedgerEntry` struct with Serde attributes (including `gateway_reference` and `failure_reason`).
  - `PayoutStatus` enum with Serde serialization (`Pending`, `Processing`, `Paid`, `Cancelled`, `Refunded`).
  - **Type-System Enforced State Machine**:
    - Compile-time Typestate markers: `PendingPayout`, `ProcessingPayout`, `PaidPayout`, `CancelledPayout`, `RefundedPayout`.
    - `PayoutRecord<State>` typestate wrapper allowing only compile-time valid transformations (e.g. `begin_processing(self) -> PayoutRecord<ProcessingPayout>`, `mark_paid(...) -> PayoutRecord<PaidPayout>`, `cancel(self) -> PayoutRecord<CancelledPayout>`, `refund(...) -> PayoutRecord<RefundedPayout>`).
    - Dynamic transition validator on enum: `PayoutStatus::transition_to(&self, next: PayoutStatus) -> Result<PayoutStatus, PayoutTransitionError>`.
    - `PayoutTransitionError` enum using `thiserror` (e.g. `InvalidTransition { from: PayoutStatus, to: PayoutStatus }`).
  - `PayoutSummary` struct (total gross, total platform fee, total net earnings, paid amount, pending amount).
  - `PayoutFilter` query parameters (listing_id, status, date_from, date_to).
- **`common/src/pricing.rs`**:
  - Add helper function `calculate_host_payout(gross_amount: Decimal, platform_fee_pct: Decimal, tax_withheld: Decimal) -> (Decimal, Decimal)`.
- **`common/src/csv.rs` [NEW]**:
  - Utility to format slice of `PayoutLedgerEntry` objects into a compliant CSV string.

### 3. `app_api/booking_api` (Backend Service)
- **Automatic Ledger Creation**: On booking status transition to `confirmed`, compute and insert `host_payout_ledger` entry in the same transaction.
- **Cancellation Sync**: On booking cancellation, auto-cancel pending ledger entries.
- **REST Endpoints**:
  - `GET /api/v1/hosts/ledger`: Retrieve ledger entries with filtering and pagination.
  - `GET /api/v1/hosts/ledger/summary`: Retrieve aggregated host financial stats.
  - `GET /api/v1/hosts/ledger/export`: Return `text/csv` attachment for accounting software.
  - `PATCH /api/v1/admin/ledger/{id}/status`: Admin endpoint to update payout status (`pending` $\rightarrow$ `processing` $\rightarrow$ `paid` $\rightarrow$ `refunded` / `cancelled`) with optional `gateway_reference`.

### 4. `web_app_admin_tc` (Topcoat SSR & HTMX UI)
- **Admin Payouts & Earnings Dashboard** (`/admin/payouts`):
  - Aggregated financial metric cards (Gross Revenue, Commission Fees, Net Payouts, Pending Payouts).
  - HTMX-driven filterable data table (by listing, status, date range).
  - Real-time payout status dropdown/action for administrators.
  - CSV export download button.

---

## Unit Test Cases
1. `test_calculate_host_payout_precision`: Verify exact decimal net host payout arithmetic without rounding errors.
2. `test_payout_ledger_csv_formatting`: Verify CSV output header and data row alignment.
3. `test_ledger_creation_on_booking_confirmation`: Verify automatic insertion of ledger record when booking is confirmed.
4. `test_payout_status_transitions`: Verify valid status transitions (`pending` $\rightarrow$ `processing` $\rightarrow$ `paid` $\rightarrow$ `refunded`) and rejection of invalid state transitions (e.g. `paid` $\rightarrow$ `pending`).
5. `test_booking_cancellation_sync`: Verify pending ledger is cancelled on booking cancellation, while paid ledger is preserved.

---

## Acceptance Criteria
- [ ] Database migration for `host_payout_ledger` table, `payout_status` enum, and `listing.commission_pct` created and compile-time verified via `sqlx`.
- [ ] Pure decimal arithmetic in `common::pricing` for platform fee and net payout logic (0 `f32`/`f64` types).
- [ ] API endpoints in `booking_api` for listing ledger entries, getting financial summaries, updating payout statuses, and downloading CSV reports.
- [ ] Topcoat SSR UI components in `web_app_admin_tc` rendering host earnings metrics, filterable table, status transitions, and CSV export.
- [ ] Unit tests in `common` and database integration tests in `db_core` / `booking_api` passing cleanly.
