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
An automated, tamper-resistant statutory tax invoicing and receipt generation engine:
1. **Automated Tax Invoice Generation**: Confirmation of a reservation (`BookingStatus::Confirmed`) automatically generates an immutable `tax_invoice` record tied to the booking and its corresponding `booking_history` snapshot.
2. **Strict Jamaican Statutory Itemization**: Clear, accurate separation of charges:
   - Accommodation Nightly Subtotal (net of stay discounts).
   - Statutory 15% Jamaican GCT (`common::reference::SupportedCountry::LIST`).
   - Cleaning Fee (statutorily taxable service at 15% GCT).
   - Refundable Damage Deposit (statutorily non-taxable escrow hold).
   - Platform Guest Service Fee (with transparent tax categorization).
3. **Dual-Format Rendering (Printable HTML & Lightweight PDF)**:
   - High-performance, print-optimized semantic HTML template (`@media print` paged styles) rendering in $<50\text{ms}$ under Cloud Run's 256MB memory budget without headless browser bloat.
   - Serverless download/export capability for guests and administrators.
4. **Immutable Audit Association**: Invoices are cryptographically identified, sequentially or deterministically numbered (e.g. `INV-2026-HMEJYC3P9B`), and permanently referenced in `booking_history`. Subsequent cancellations or refunds generate explicit Credit Notes (`CN-YYYY-...`) rather than overwriting original invoices.
5. **Portal Integration**:
   - Guests can view, download, or print official receipts in `web_app_tc` under `/bookings/{id}`.
   - Admins can inspect, verify, and reprint tax invoices in `web_app_admin_tc` under `/admin/bookings`.

---

## 3. Scope Boundaries

### In Scope (MVP)
- **Data Persistence (`db_core`)**:
  - PostgreSQL table `tax_invoice` recording:
    - Primary key `id UUID PRIMARY KEY DEFAULT uuidv7()`.
    - `invoice_number VARCHAR(32) NOT NULL UNIQUE`.
    - `booking_id UUID NOT NULL REFERENCES booking(id) ON DELETE RESTRICT`.
    - `booking_history_id UUID REFERENCES booking_history(id)`.
    - `recipient_id UUID NOT NULL REFERENCES "user"(id)`.
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
  - Database indexes on `booking_id`, `invoice_number`, `recipient_id`, and `issue_date`.
- **Domain Logic & Math (`common`)**:
  - Shared domain models: `TaxInvoice`, `InvoiceLineItem`, `InvoiceTaxCategory`, `TaxReceiptSummary`.
  - Expansion of `BookingCalculator` in `common/src/pricing.rs` to support itemized tax calculations (15% GCT applied to accommodation subtotal and cleaning fee, excluding damage deposit) using pure `rust_decimal::Decimal`.
- **API Endpoints (`app_api/booking_api`)**:
  - `POST /api/v1/bookings/{id}/invoice/generate`: Internal/transactional generator called upon booking confirmation.
  - `GET /api/v1/bookings/{id}/invoice`: Returns JSON invoice DTO with itemized lines.
  - `GET /api/v1/bookings/{id}/invoice/render`: Returns print-ready, semantic HTML formatted for PDF printing/saving.
- **Frontend Portals (`web_app_tc` & `web_app_admin_tc`)**:
  - Guest Portal: "View Tax Receipt" button on booking detail page rendering full itemized modal or standalone print view.
  - Admin Portal: Upgraded "Statutory Tax Receipt / Invoice" modal in `/admin/bookings` populated dynamically from backend invoice API.

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
- [ ] **TRN & Merchant of Record**: Platform entity `Our Places Luxury Villas Ltd` acts as merchant of record with configurable TRN (`TAJ_SUPPLIER_TRN` in environment config, defaulting to staging test TRN `123-456-789`).
- [ ] **Email Outbox Integration**: Include tax invoice reference and printable receipt link in `booking_confirmation_guest` email notification template dispatched via `email_worker`.

