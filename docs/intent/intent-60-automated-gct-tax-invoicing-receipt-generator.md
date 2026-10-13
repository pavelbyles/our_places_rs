# Intent: Automated GCT Tax Invoicing & Receipt Generator

## Metadata
- **Author**: Originator (via GitHub Issue #60)
- **Date**: 2026-10-12
- **Status**: Approved
- **Target Area**: Fullstack (`db_core`, `common`, `app_api/booking_api`, `web_app_tc`, `web_app_admin_tc`)

---

## 1. Problem Statement & Motivation
Currently, when a guest completes a booking reservation across our Jamaican luxury properties, the platform records financial aggregates (`sub_total_price`, `discount_value`, `tax_value`, and `fee_breakdown`), but does not generate formal, compliance-grade tax invoices or receipts.

This presents critical operational, legal, and financial deficiencies:
- **Tax Administration Jamaica (TAJ) Non-Compliance**: Jamaican tax law mandates that registered businesses supply itemized tax invoices detailing the statutory General Consumption Tax (GCT at 15%), the supplier's Taxpayer Registration Number (TRN), serialized invoice numbering, and distinct tax treatment of accommodation vs ancillary fees.
- **Guest Accounting Pain**: Corporate and international travelers requiring itemized tax receipts for corporate accounting or tax reclamation cannot obtain a verified receipt directly from their guest portal.
- **Audit Vulnerability**: Because invoice snapshots are not immutably stored and linked to the `booking_history` audit trail, price recalculations or post-booking rate changes could compromise historical financial reporting.
- **Manual Overhead**: Platform administrators must manually compose invoice summaries or respond to guest inquiries for formal VAT/GCT receipts using external spreadsheets.

Solving this automates the generation of immutable, TAJ-compliant tax invoices and receipts directly linked to booking state transitions, backed by tri-currency decimal mathematics and lightweight print/PDF rendering.

---

## 2. Proposed Outcome (The Vision)
An automated, tamper-resistant statutory tax invoicing and receipt generation engine utilizing the **Dual Document Model**:
1. **Immediate Payment Receipt (`BookingStatus::Confirmed`)**: Confirmation of a reservation generates an immutable **Booking & Payment Receipt** (`REC-YYYY-{code}`) confirming captured funds, itemized charges, and GCT collected at payment.
2. **Final Statutory Tax Invoice (`BookingStatus::Completed`)**: Post-stay completion generates the definitive, locked **Statutory Tax Invoice** (`INV-YYYY-{code}`) reflecting finalized stay numbers, post-checkout deposit releases, and final tax remittance records.
3. **Strict Jamaican Statutory Itemization**: Clear, accurate separation of charges:
   - Accommodation Nightly Subtotal (net of stay discounts).
   - Statutory 15% Jamaican GCT (`common::reference::SupportedCountry::LIST`).
   - Cleaning Fee (statutorily taxable service at 15% GCT).
   - Refundable Damage Deposit (statutorily non-taxable escrow hold).
   - Platform Guest Service Fee (with transparent tax categorization).
4. **Dual-Format Rendering (Printable HTML & Lightweight PDF)**:
   - High-performance, print-optimized semantic HTML template (`@media print` paged styles) rendering in $<50\text{ms}$ under Cloud Run's 256MB memory budget without headless browser bloat.
   - Serverless download/export capability for guests and administrators.
5. **Immutable Audit Association**: Receipts and invoices are cryptographically identified, sequentially or deterministically numbered (e.g. `REC-2026-HMEJYC3P9B` / `INV-2026-HMEJYC3P9B`), and permanently referenced in `booking_history`. If a booking is cancelled prior to completion, a Refund Receipt (`REF-YYYY-...`) is issued without generating an unneeded final tax invoice.
6. **Portal Integration**:
   - Guests can view, download, or print receipts and tax invoices in `web_app_tc` under `/bookings/{id}`.
   - Admins can inspect, verify, and reprint documents in `web_app_admin_tc` under `/admin/bookings`.

---

## 3. Scope Boundaries

### In Scope (MVP)
- **Data Persistence (`db_core`)**:
  - PostgreSQL ENUM `tax_document_type`: `'payment_receipt'`, `'tax_invoice'`, `'credit_note'`.
  - PostgreSQL table `tax_document` (aliased as `tax_invoice`):
    - Primary key `id UUID PRIMARY KEY DEFAULT uuidv7()`.
    - `document_type tax_document_type NOT NULL`.
    - `document_number VARCHAR(32) NOT NULL UNIQUE` (`REC-YYYY-...` or `INV-YYYY-...`).
    - `booking_id UUID NOT NULL REFERENCES booking(id) ON DELETE RESTRICT`.
    - `booking_history_id UUID REFERENCES booking_history(id)`.
    - `recipient_id UUID NOT NULL REFERENCES "user"(id)`.
    - `recipient_name VARCHAR(255) NOT NULL`.
    - `recipient_company_name VARCHAR(255)` (optional B2B company name).
    - `recipient_trn VARCHAR(32)` (optional guest/business TRN for input tax credits).
    - `listing_id UUID NOT NULL REFERENCES listing(id)`.
    - `issue_date TIMESTAMPTZ NOT NULL DEFAULT NOW()`.
    - `supplier_trn VARCHAR(32) NOT NULL`, `supplier_name VARCHAR(255) NOT NULL`.
    - `currency CHAR(3) NOT NULL` (Payment currency), plus exchange rate snapshot if converted from base currency.
    - `subtotal_amount DECIMAL(12, 2) NOT NULL`.
    - `taxable_services_amount DECIMAL(12, 2) NOT NULL` (e.g. Cleaning Fee).
    - `tax_rate DECIMAL(5, 4) NOT NULL DEFAULT 0.1500` (15% GCT).
    - `tax_amount DECIMAL(12, 2) NOT NULL`.
    - `non_taxable_amount DECIMAL(12, 2) NOT NULL` (e.g. Damage Deposit).
    - `total_amount DECIMAL(12, 2) NOT NULL`.
    - `itemized_lines JSONB NOT NULL DEFAULT '[]'::jsonb`.
  - Database indexes on `booking_id`, `document_number`, `recipient_id`, `document_type`, and `issue_date`.
  - Unique constraint on `(booking_id, document_type)` ensuring exactly one receipt on confirmation and one invoice on completion.
- **Domain Logic & Math (`common`)**:
  - Shared domain models: `TaxDocument`, `TaxDocumentType`, `InvoiceLineItem`, `InvoiceTaxCategory`, `TaxReceiptSummary`.
  - Expansion of `BookingMetadata` / `BookingMetadataResponse` to optionally capture `business_company_name` and `business_tax_id` when `is_business_trip = true`.
  - Expansion of `BookingCalculator` in `common/src/pricing.rs` to support itemized tax calculations (15% GCT applied to accommodation subtotal and cleaning fee, excluding damage deposit) using pure `rust_decimal::Decimal`.
- **API Endpoints (`app_api/booking_api`)**:
  - Internal generators:
    - Receipt generator invoked synchronously on `BookingStatus::Confirmed`.
    - Tax invoice generator invoked synchronously on `BookingStatus::Completed`.
  - `GET /api/v1/bookings/{id}/receipt`: Returns JSON / HTML payment receipt.
  - `GET /api/v1/bookings/{id}/invoice`: Returns JSON / HTML statutory tax invoice (available upon completion).
  - `GET /api/v1/bookings/{id}/documents`: Returns list of all generated tax documents for the reservation.
  - `GET /api/v1/bookings/{id}/documents/{doc_id}/render`: Returns print-ready, semantic HTML formatted for browser PDF printing.
- **Transactional Notifications (`app_api/email_worker`)**:
  - Enhance `booking_confirmation_guest` template to display itemized GCT payment breakdown and link to `/bookings/{id}/receipt`.
  - In post-stay check-out email notification, include direct link to download the final `/bookings/{id}/invoice`.
- **Frontend Portals (`web_app_tc` & `web_app_admin_tc`)**:
  - Guest Portal: "View Payment Receipt" available at booking confirmation; "Download Official Tax Invoice" becomes available upon completion.
  - Admin Portal: Upgraded documents tab under `/admin/bookings` viewing both receipts and final statutory invoices with print triggers.

### Out of Scope / Non-Goals
- Real-time automated API sync or live electronic filing with TAJ's revenue portal (manual/CSV reporting handles batch filings).
- Dynamic multi-country VAT/GST lookup beyond Jamaican GCT (15%) and configured static countries.
- Direct credit card chargeback dispute automation.
- Retroactive bulk generation of invoices for bookings completed prior to this release.

---

## 4. Affected Systems & Stakeholders

- **User Personas**:
  - **Guests**: Travelers requiring official, itemized proof of payment for tax filing and company expense reimbursement.
  - **Property Hosts**: Owners seeking transparent visibility into guest taxes collected and remitted.
  - **Platform Administrators**: Operators reconciling monthly GCT returns for Jamaican tax compliance.
- **System Components**:
  - `db_core`: Migration for `tax_invoice` table and SQLx entities.
  - `common`: `TaxInvoice` DTOs, line item models, and pure `rust_decimal` tax calculation routines.
  - `app_api/booking_api`: Invoice generation service, HTTP endpoints, auth guards (`BookingViewer` / `AdminRole`).
  - `web_app_tc`: Guest booking receipt viewer and printable stylesheet.
  - `web_app_admin_tc`: Admin booking tax invoice viewer.

---

## 5. Constraints & Non-Negotiable Invariants

- **Financial Precision ("Zero Floats")**:
  - Absolute prohibition of floating point types (`f32`/`f64`). All pricing, tax rates, subtotals, and totals strictly use `rust_decimal::Decimal`.
- **Zero Panic Primitives**:
  - Complete elimination of `.unwrap()` and `.expect()` in production code paths. Monadic error propagation (`Result<T, E>`) throughout.
- **Cloud Run Performance Budgets**:
  - Renderer must execute in $<50\text{ms}$ with zero memory spikes. Heavy headless browser tools (Puppeteer, Chromium, wkhtmltopdf) are strictly forbidden to honor the 256MB container budget and $<300\text{ms}$ cold start target.
- **Immutability & Audit Trail**:
  - Invoices cannot be modified in-place once issued. Cancellations or refunds append credit note records linked to `booking_history`.
- **Tenant Isolation & Security**:
  - Guests can only access their own invoices (`WHERE recipient_id = claims.sub`).
  - Hosts can access invoices for bookings on their listings.
  - Platform Admins can access all invoices.

---

## 6. Success Metrics & Acceptance Signals

- **Leading Indicators**:
  - 100% test pass rate across `cargo test --workspace`.
  - Zero clippy warnings with `-D warnings` and `-D missing_docs`.
  - Mathematical precision test verifying zero cent-rounding drift across tri-currency conversions and 15% GCT compounding.
  - Invoice generation completes synchronously within the booking confirmation transaction without adding $>30\text{ms}$ overhead.
- **Lagging Indicators**:
  - Zero guest support tickets requesting manual tax invoices or itemized receipts.
  - 100% audit compliance for TAJ GCT filings.

---

## 7. Open Questions & Policy Concerns

- [x] **Tax Treatment Resolution**: Accommodation subtotal and Cleaning fee are both subject to 15% Jamaican GCT; Damage Deposit is an exempt, refundable escrow hold.
- [x] **Invoice Numbering Convention**: Deterministic booking-code format: `INV-YYYY-{confirmation_code}` (e.g., `INV-2026-HMEJYC3P9B`).
- [x] **Rendering Architecture**: Print-optimized semantic HTML (`@media print` paged CSS) for $<10\text{ms}$ rendering under Cloud Run memory limits without headless browser dependencies, supporting native browser "Save as PDF".
- [x] **TRN & Merchant of Record**: Platform entity `Our Places Luxury Villas Ltd` acts as centralized Merchant of Record. Mandatory `TAJ_SUPPLIER_TRN` must be configured in `.env` / typed settings across all environments; generation fails fast if absent (test fixtures and local development explicitly provide `TAJ_SUPPLIER_TRN=123-456-789` in their local configuration/`.env.example`).
- [x] **Corporate Guest TRN (B2B)**: Business travelers (`is_business_trip = true`) can optionally supply `business_company_name` and `business_tax_id` in booking metadata to be itemized on the statutory invoice for input tax deduction.
- [x] **Email Outbox Integration**: `email_worker` will render an inline itemized GCT receipt summary table inside `booking_confirmation_guest` and provide a primary action button linking to `/bookings/{id}/receipt`.


