# Implementation Plan: Automated GCT Tax Invoicing & Receipt Generator (#60)

**Spec Reference:** [spec-60 rev. 2](file:///home/pav/code/our_places_rs-feat-60-automated-gct-tax-invoicing-receipt-generator/docs/specs/spec-60-automated-gct-tax-invoicing-receipt-generator.md)  
**Baseline Commit:** `1673948`  
**Target Branch:** `feat/60-automated-gct-tax-invoicing-receipt-generator`  

---

## Tasks

- [x] **Task 0: Shared Contracts, Schema Migration, and FeeItem Serde**
  - **Scope:** S (4 files)
  - **Description:** Create the single feature migration adding `cancellation_policy` enum values (`firm`, `limited`), listing fee columns, booking financial snapshot columns, and the `tax_document` table. Implement string-serialized Decimal serde for `FeeItem` with backward-compatible deserialization and define `tax_document` DTOs.
  - **Files Touched:**
    - `db_core/migrations/20261013000000_gct_tax_documents.sql`
    - `db_core/src/models.rs`
    - `common/src/tax_document.rs`
    - `common/src/models.rs`
  - **Acceptance Criteria:**
    - [x] Migration applies cleanly against the database schema (`sqlx migrate run`).
    - [x] `FeeItem.amount` serializes as a JSON string and deserializes from both strings and legacy numbers.
    - [x] `document_number` generator correctly formats `REC-YYYY-{code}[-R{n}]`, `INV-YYYY-{code}`, and `REF-YYYY-{code}`.
    - [x] `pg_cron` reservation hold cleanup query remains valid with new nullable history columns.
  - **Verification Command:** `sqlx migrate run && SQLX_OFFLINE=false cargo check --workspace && cargo test -p common -- fee_item document_number`
  - **Dependencies:** None

- [x] **Task 1: Single Statutory Tax Source with JM Default**
  - **Scope:** S (4 files)
  - **Description:** Replace the hardcoded 10% tax rate in `BookingCalculator::apply_taxes` with the country-resolved statutory rate from `SupportedCountry::tax_rate_for` (defaulting to JM 15%). Pass this rate in `create_booking` and listing quotes, persisting `booking.tax_rate` and `booking.exchange_rate`.
  - **Files Touched:**
    - `common/src/reference.rs`
    - `common/src/pricing.rs`
    - `app_api/booking_api/src/apis.rs`
    - `app_api/listing_api/src/apis.rs`
  - **Acceptance Criteria:**
    - [x] Quotes and bookings for Jamaican listings calculate GCT at statutory 15%.
    - [x] Listings with unrecognized country code default safely to 15% (JM) with a warning log.
    - [x] `booking.tax_rate` and `booking.exchange_rate` are snapshotted in the database on insert.
    - [x] Unit pricing tests pass with updated 15% expectations.
  - **Verification Command:** `cargo test -p common pricing && cargo test -p booking_api`
  - **Dependencies:** Task 0

- [x] **Task 2A: Listing Fee Backend & Reconciled Pricing Invariant**
  - **Scope:** M (4 files)
  - **Description:** Add host-configurable `cleaning_fee` and `damage_deposit` to `listing` persistence and listing API endpoints. Update `BookingCalculator` with `with_fees(cleaning, deposit)` and enforce the financial reconciliation invariant: `total_price = taxable_base + tax_value + platform_fee + damage_deposit`.
  - **Files Touched:**
    - `db_core/src/listing.rs`
    - `app_api/api_core/src/models.rs`
    - `app_api/listing_api/src/apis.rs`
    - `common/src/pricing.rs`
  - **Acceptance Criteria:**
    - [x] Listing create and update endpoints accept optional `cleaning_fee` and `damage_deposit` (validating `0 <= fee <= 1,000,000` with 2 decimal places).
    - [x] Listings with fees disabled default to `0.00`.
    - [x] Property-based tests verify `total_price == (accommodation_net + cleaning) + tax + platform_fee + damage_deposit` across edge values.
  - **Verification Command:** `cargo test -p common -- reconciliation && cargo test -p listing_api`
  - **Dependencies:** Task 1

- [x] **Task 2B: Listing Fee Admin Form & Price Breakdown Component**
  - **Scope:** S (2 files)
  - **Description:** Expose cleaning fee and damage deposit inputs with toggle switches in the admin listing management UI, and render itemized cleaning fee and damage deposit lines in the shared guest price breakdown component.
  - **Files Touched:**
    - `web_app_admin_tc/src/pages/listings_admin.rs`
    - `web_app_common_tc/src/components/price_breakdown.rs`
  - **Acceptance Criteria:**
    - [x] Admin listing editor allows enabling/disabling cleaning fee and damage deposit with currency inputs.
    - [x] Zero or disabled fees are omitted from the guest price breakdown view.
    - [x] Non-zero fees render itemized before booking submission.
  - **Verification Command:** `cargo test -p web_app_admin_tc && cargo test -p web_app_common_tc`
  - **Dependencies:** Task 2A

- [x] **Task 3A: Tax Document Engine & Payment Receipt Hook**
  - **Scope:** M (5 files)
  - **Description:** Implement `db_core::tax_document` queries, JMD FX rate lookup with direct/inverse resolution, and the transaction hook in `update_booking` that generates a `payment_receipt` (`REC-YYYY-{code}`) upon transitioning to `Confirmed`. Wire typed `TaxSettings` and expose JSON document API endpoints in `booking_api`.
  - **Files Touched:**
    - `db_core/src/exchange_rate.rs`
    - `db_core/src/tax_document.rs`
    - `db_core/src/booking.rs`
    - `app_api/booking_api/src/settings.rs`
    - `app_api/booking_api/src/apis.rs`
  - **Acceptance Criteria:**
    - [x] Transitioning booking to `Confirmed` issues exactly one `tax_document` with type `payment_receipt`.
    - [x] Document `total_amount` matches `booking.total_price` and snapshots `jmd_total_amount`.
    - [x] Missing FX rates return HTTP 422 `FX_RATE_UNAVAILABLE` and abort the transaction.
    - [x] Application fails to start if `TAJ_SUPPLIER_TRN` is missing from environment.
    - [x] `GET /api/v1/bookings/{id}/documents` returns issued documents with authorization guards.
  - **Verification Command:** `cargo test -p db_core tax_document && cargo test -p booking_api documents`
  - **Dependencies:** Task 2B

- [x] **Task 3B: Guest Tax Document SSR View & Confirmation Email Link**
  - **Scope:** S (4 files)
  - **Description:** Build the printable Topcoat SSR `TaxDocumentView` component in `web_app_common_tc`, mount the guest document route in `web_app_tc`, and update `booking_confirmation_guest` to include an itemized GCT breakdown table and receipt view link.
  - **Files Touched:**
    - `web_app_common_tc/src/components/tax_document_view.rs`
    - `web_app_tc/src/pages/booking_documents.rs`
    - `app_api/email_worker/templates/booking_confirmation_guest.html`
    - `app_api/email_worker/src/processor.rs`
  - **Acceptance Criteria:**
    - [x] Document route renders compliance-grade layout with supplier TRN, itemized lines, and print CSS.
    - [x] Confirmation email renders itemized GCT tax summary and direct link to the receipt.
    - [x] Unauthorized guest access returns HTTP 404.
  - **Verification Command:** `cargo test -p web_app_tc && cargo test -p email_worker`
  - **Dependencies:** Task 3A

- [ ] **Task 4A: 5-Policy Typestate Refund Calculator**
  - **Scope:** S (2 files)
  - **Description:** Implement `common::cancellation` with compile-time typestate `RefundCalculator` enforcing the exact schedules for all 5 policies: Flexible, Moderate, Strict, Firm, and Limited. Implement the watertight partial refund component formulas (accommodation at policy %, cleaning & deposit 100%, GCT pro-rata, platform fee only on 100% full refund).
  - **Files Touched:**
    - `common/src/cancellation.rs`
    - `common/tests/cancellation_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Flexible: >=24h 100% accommodation refund; <24h 0% (100% host payout).
    - [ ] Moderate: >=5d 100%; 5d to 24h 50%; <24h 0%.
    - [ ] Strict: <=48h from booking (if booked >=14d out) 100%; >=7d 50%; <7d 0%.
    - [ ] Firm: >=30d (or <=48h if booked >=14d out) 100%; 7d to 30d 50%; <7d 0%.
    - [ ] Limited: >=14d 100%; 7d to 14d 50%; <7d 0%.
    - [ ] Component invariant: `refund_total + retained_total == total_price` holds for all percentages.
  - **Verification Command:** `cargo test -p common --test cancellation_tests`
  - **Dependencies:** Task 0

- [ ] **Task 4B: Cancellation Hook, Refund Document & Cancellation Flow**
  - **Scope:** M (4 files)
  - **Description:** Hook `issue_refund_receipt` (`REF-YYYY-{code}`) into `update_booking` on transition to `Cancelled`. Expose `GET /api/v1/bookings/{id}/cancellation-quote`, wire HTMX cancel quote preview in `web_app_tc`, and update `booking_cancelled_guest` to show the correct refunded and retained totals.
  - **Files Touched:**
    - `db_core/src/booking.rs`
    - `app_api/booking_api/src/apis.rs`
    - `web_app_tc/src/components/cancel_modal.rs`
    - `app_api/email_worker/templates/booking_cancelled_guest.html`
  - **Acceptance Criteria:**
    - [ ] Cancellation quote endpoint returns accurate preview without mutating database state.
    - [ ] Cancelling a confirmed booking issues a `REF-YYYY-{code}` document.
    - [ ] Host payout and retained amounts match the policy quote.
    - [ ] Cancellation email displays the REF document amount, not `total_price`.
  - **Verification Command:** `cargo test -p db_core cancellation && cargo test -p booking_api cancel`
  - **Dependencies:** Task 3B, Task 4A

- [ ] **Task 5: Confirmed Booking Date Changes & Revised Receipts**
  - **Scope:** M (5 files)
  - **Description:** Add `date_from` and `date_to` mutation support to `PATCH /api/v1/bookings/{id}` for confirmed bookings. Re-price with the standard calculator, acquire `FOR UPDATE` locks, check date availability excluding self, update dates/financials, supersede active receipt, issue `REC-YYYY-{code}-R{n}` with `amount_previously_paid` and `balance_due`, and dispatch revised receipt email.
  - **Files Touched:**
    - `db_core/src/booking.rs`
    - `common/src/models.rs`
    - `app_api/booking_api/src/apis.rs`
    - `web_app_tc/src/pages/booking_date_change.rs`
    - `app_api/email_worker/templates/booking_updated_guest.html`
  - **Acceptance Criteria:**
    - [ ] Date change on confirmed booking supersedes previous receipt and issues revision `R{n}`.
    - [ ] Date overlap with another booking returns HTTP 409 `DATE_UNAVAILABLE` with no mutation.
    - [ ] Attempted change to past date or non-confirmed booking returns HTTP 409 `BOOKING_NOT_MODIFIABLE`.
    - [ ] Revised receipt reflects `amount_previously_paid` and `balance_due`.
    - [ ] `booking_updated_guest` email includes link to revised receipt.
  - **Verification Command:** `cargo test -p db_core date_change && cargo test -p booking_api date_change`
  - **Dependencies:** Task 3B

- [ ] **Task 6: Automatic Stay Completion, Tax Invoice & Cloud Scheduler**
  - **Scope:** M (5 files)
  - **Description:** Add `PATCH /api/v1/internal/cron/complete-stays` endpoint protected by `x-cron-secret` that queries past-checkout confirmed stays and transitions them to `Completed`. Hook `issue_tax_invoice` (`INV-YYYY-{code}`) in `update_booking`, dispatch `booking_completed_guest` with invoice link, and write Terraform Cloud Scheduler definition.
  - **Files Touched:**
    - `db_core/src/booking.rs`
    - `app_api/booking_api/src/apis.rs`
    - `common/src/email.rs`
    - `app_api/email_worker/templates/booking_completed_guest.html`
    - `infra/modules/scheduler/main.tf`
  - **Acceptance Criteria:**
    - [ ] `complete-stays` transitions eligible confirmed stays to `Completed` idempotently.
    - [ ] Transitioning to `Completed` issues `tax_invoice` (`INV-YYYY-{code}`) excluding damage deposit from invoice total.
    - [ ] Missing or invalid `x-cron-secret` returns HTTP 401.
    - [ ] `booking_completed_guest` email sent with invoice URL.
    - [ ] Terraform configuration passes `terraform fmt -check` and `terraform validate`.
  - **Verification Command:** `cargo test -p booking_api complete_stays && cargo test -p email_worker && (cd infra && terraform fmt -check && terraform validate)`
  - **Dependencies:** Task 3B

- [ ] **Task 7: Admin Documents Panel, OpenAPI Sync & E2E Validation**
  - **Scope:** S (5 files)
  - **Description:** Integrate the documents panel in the admin booking modal (`web_app_admin_tc`), update OpenAPI specifications, update test db seed data with string decimal fee amounts, and write Playwright E2E tests validating the full receipt, date change, refund, and invoice flows.
  - **Files Touched:**
    - `web_app_admin_tc/src/pages/bookings_admin.rs`
    - `openapi.yaml`
    - `playwright/e2e/setup/db-seed.ts`
    - `playwright/e2e/guest-portal/tax-documents.spec.ts`
    - `playwright/e2e/admin-portal/listing-fees-and-documents.spec.ts`
  - **Acceptance Criteria:**
    - [ ] Admin portal booking details modal lists all issued and superseded tax documents.
    - [ ] OpenAPI schema accurately documents all new endpoints and string-serialized decimal fee structures.
    - [ ] Full Playwright E2E suite passes for guest receipt generation, date modification, cancellation refund, and tax invoice access.
  - **Verification Command:** `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo sqlx prepare --workspace --check && npx playwright test`
  - **Dependencies:** Task 4B, Task 5, Task 6
