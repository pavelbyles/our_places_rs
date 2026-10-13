# Spec 60: Automated GCT Tax Invoicing & Receipt Generator

## Overview
This feature introduces an automated, statutory-compliant tax invoicing and receipt generation module for the Our Places platform implementing the **Dual Document Model**:
1. **At `BookingStatus::Confirmed` (Payment Time)**: An immutable **Booking & Payment Receipt** (`REC-YYYY-{code}`) is generated immediately to serve as verifiable proof of card payment, itemizing the accommodation subtotal, cleaning fee, damage deposit escrow hold, and statutory 15% Jamaican General Consumption Tax (GCT) collected at transaction time.
2. **At `BookingStatus::Completed` (Check-out Time)**: A definitive, locked **Statutory Tax Invoice** (`INV-YYYY-{code}`) is generated post-stay to reflect the final settled transaction, account for any stay modifications or deposit claims/releases, and establish the permanent tax compliance record for Tax Administration Jamaica (TAJ).

If a booking is cancelled prior to stay completion, a **Refund Receipt** (`REF-YYYY-{code}`) is generated instead of a tax invoice, eliminating unnecessary invoice churn, cancellation credits, and confusion.

All documents are rendered via print-optimized, semantic HTML (`@media print`) executing in $<10\text{ms}$ under Cloud Run's 256MB memory budget without external headless browser dependencies. Guests and administrators can view, print, or download compliant documents across both portals (`web_app_tc` and `web_app_admin_tc`), while `email_worker` dispatches itemized payment receipts in booking confirmations and invoice links upon stay completion.

---

## 1. Domain Decisions & Business Rules

### 1.1. Dual Document Lifecycle
```
+---------------------------------------------------------------------------------------------------+
|                                      BOOKING LIFECYCLE                                            |
|                                                                                                   |
|  [Guest Checkout] ----> [BookingStatus::Confirmed] --------------------> [BookingStatus::Completed]
|                                  |                                                 |              |
|                                  v                                                 v              |
|                      +------------------------+                        +----------------------+   |
|                      | Booking Payment Receipt|                        | Statutory Tax Invoice|   |
|                      |  REC-2026-HMEJYC3P9B   |                        |  INV-2026-HMEJYC3P9B |   |
|                      +------------------------+                        +----------------------+   |
|                                  |                                                                |
|                                  | (If Cancelled Pre-Stay)                                        |
|                                  v                                                                |
|                      +------------------------+                                                   |
|                      |     Refund Receipt     |                                                   |
|                      |  REF-2026-HMEJYC3P9B   |                                                   |
|                      +------------------------+                                                   |
+---------------------------------------------------------------------------------------------------+
```

- **Phase 1 — Payment Receipt (`Confirmed`)**:
  - Generated synchronously in the confirmation transaction.
  - Number format: `REC-YYYY-{confirmation_code}`.
  - Document Title: *"Booking & Payment Receipt"*.
  - Purpose: Proof of card transaction for guest expenses and instant reassurance.
- **Phase 2 — Statutory Tax Invoice (`Completed`)**:
  - Generated when booking transitions to `Completed` (post-stay checkout).
  - Number format: `INV-YYYY-{confirmation_code}`.
  - Document Title: *"Statutory Tax Invoice"*.
  - Purpose: Final compliance document for TAJ audit reconciliation and corporate B2B tax filings.
- **Cancellation Prior to Completion**:
  - Generates a `Refund Receipt` (`REF-YYYY-{confirmation_code}`).
  - No `INV-...` tax invoice is ever created, avoiding invoice voids or credit notes.

### 1.2. Statutory Tax Base & Decimal Precision
- **Statutory Rate**: 15% Jamaican GCT loaded from static reference data (`common::reference::SupportedCountry::LIST`).
- **Taxable Line Items**:
  - Accommodation Nightly Subtotal (`discounted_subtotal` = $\sum \text{nightly rates} - \text{stay discounts}$).
  - Cleaning Fee (statutorily classified as a taxable hospitality service under Jamaican GCT legislation).
- **Non-Taxable Escrow Items**:
  - Damage Deposit (classified as a temporary refundable security hold, exempt from GCT).
- **Tax Calculation Formula**:
  $$\text{taxable\_base} = \text{discounted\_subtotal} + \text{cleaning\_fee}$$
  $$\text{gct\_amount} = (\text{taxable\_base} \times 0.1500).\text{round\_dp}(2)$$
  $$\text{total\_document\_amount} = \text{taxable\_base} + \text{gct\_amount} + \text{damage\_deposit} + \text{platform\_fee}$$
- **Zero-Float Policy**: All rates, subtotals, taxes, and totals strictly use `rust_decimal::Decimal`. Floating point types (`f32`/`f64`) are strictly prohibited per `CONSTRAINTS.md`.

### 1.3. Tri-Currency Flow & Exchange Rate Snapshots
- Documents are denominated in the **Guest Payment Currency** (e.g. JMD, USD, GBP, EUR, CAD) selected at checkout.
- If the listing's base currency differs from the payment currency, the active exchange rate from `currency_exchange_rates` is permanently snapshotted in `tax_document.exchange_rate`.
- For foreign currency transactions, the Jamaican Dollar (JMD) statutory equivalent total and GCT amount are permanently stored to satisfy TAJ audit requirements.

### 1.4. Merchant of Record & TRN Attribution
- **Platform Entity**: "Our Places Luxury Villas Ltd" acts as the centralized Merchant of Record.
- **Supplier Identification**: Document headers prominently display the platform's registered name, address, and 9-digit Taxpayer Registration Number (`TAJ_SUPPLIER_TRN`).
- **Configuration Enforcement**: `TAJ_SUPPLIER_TRN` is a mandatory configuration property loaded through typed application settings (`settings.rs`) conforming to rule `RS-W1015`. If unset, document generation fails fast.
- **B2B Guest Deductibility**: Corporate guests traveling on business (`is_business_trip = true` in `BookingMetadata`) may optionally provide their `business_company_name` and `business_tax_id` (Guest TRN) to claim input tax credits with TAJ.

---

## 2. Technical Specification

### 2.1. `db_core` (Schema & Persistence)

#### Migration: `db_core/migrations/20261013000000_create_tax_document_table.sql`
```sql
CREATE TYPE tax_document_type AS ENUM ('payment_receipt', 'tax_invoice', 'credit_note', 'refund_receipt');
CREATE TYPE tax_document_status AS ENUM ('issued', 'credited', 'voided');

CREATE TABLE IF NOT EXISTS tax_document (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    document_type tax_document_type NOT NULL,
    document_number VARCHAR(32) NOT NULL UNIQUE,
    booking_id UUID NOT NULL REFERENCES booking(id) ON DELETE RESTRICT,
    booking_history_id UUID REFERENCES booking_history(id),
    recipient_id UUID NOT NULL REFERENCES "user"(id),
    listing_id UUID NOT NULL REFERENCES listing(id),
    
    -- Status & Dates
    status tax_document_status NOT NULL DEFAULT 'issued',
    issue_date TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Supplier (Merchant of Record) Snapshot
    supplier_name VARCHAR(255) NOT NULL,
    supplier_address TEXT NOT NULL,
    supplier_trn VARCHAR(32) NOT NULL,
    
    -- Recipient Snapshot
    recipient_name VARCHAR(255) NOT NULL,
    recipient_email VARCHAR(255) NOT NULL,
    recipient_company_name VARCHAR(255),
    recipient_trn VARCHAR(32),
    
    -- Financials & Currency Snapshot
    currency CHAR(3) NOT NULL,
    exchange_rate DECIMAL(12, 6) NOT NULL DEFAULT 1.000000,
    
    -- Itemized Monetary Totals
    accommodation_subtotal DECIMAL(12, 2) NOT NULL,
    cleaning_fee DECIMAL(12, 2) NOT NULL DEFAULT 0.00,
    taxable_amount DECIMAL(12, 2) NOT NULL,
    gct_rate DECIMAL(5, 4) NOT NULL DEFAULT 0.1500,
    gct_amount DECIMAL(12, 2) NOT NULL,
    damage_deposit DECIMAL(12, 2) NOT NULL DEFAULT 0.00,
    platform_fee DECIMAL(12, 2) NOT NULL DEFAULT 0.00,
    total_amount DECIMAL(12, 2) NOT NULL,
    
    -- JMD Equivalent (for TAJ compliance on foreign currency bookings)
    jmd_gct_amount DECIMAL(12, 2) NOT NULL,
    jmd_total_amount DECIMAL(12, 2) NOT NULL,
    
    -- Line Item Detail Snapshot
    itemized_lines JSONB NOT NULL DEFAULT '[]'::jsonb,
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Ensure exactly one payment receipt and one final tax invoice per booking
    CONSTRAINT uq_booking_document_type UNIQUE (booking_id, document_type)
);

-- Performance Indexes
CREATE INDEX idx_tax_document_booking_id ON tax_document(booking_id);
CREATE INDEX idx_tax_document_recipient_id ON tax_document(recipient_id);
CREATE INDEX idx_tax_document_listing_id ON tax_document(listing_id);
CREATE INDEX idx_tax_document_issue_date ON tax_document(issue_date);
CREATE INDEX idx_tax_document_type ON tax_document(document_type);
```

#### SQLx Model & Queries: `db_core/src/tax_invoice.rs` [NEW]
- `TaxDocumentRow` struct deriving `sqlx::FromRow`.
- `insert_tax_document(pool, doc: &NewTaxDocument) -> Result<TaxDocumentRow, sqlx::Error>`.
- `get_document_by_booking_and_type(pool, booking_id: Uuid, doc_type: TaxDocumentType) -> Result<Option<TaxDocumentRow>, sqlx::Error>`.
- `get_document_by_number(pool, document_number: &str) -> Result<Option<TaxDocumentRow>, sqlx::Error>`.
- `list_documents_for_booking(pool, booking_id: Uuid) -> Result<Vec<TaxDocumentRow>, sqlx::Error>`.
- `list_documents_for_user(pool, user_id: Uuid, limit: i64, offset: i64) -> Result<Vec<TaxDocumentRow>, sqlx::Error>`.
- `list_documents_for_host(pool, host_id: Uuid, limit: i64, offset: i64) -> Result<Vec<TaxDocumentRow>, sqlx::Error>`.
- `list_all_documents_admin(pool, limit: i64, offset: i64) -> Result<Vec<TaxDocumentRow>, sqlx::Error>`.

---

### 2.2. `common` (Isomorphic Types & Pricing Math)

#### Domain Models: `common/src/tax_invoice.rs` [NEW]
```rust
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Type of tax compliance document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaxDocumentType {
    /// Initial payment confirmation receipt issued at checkout.
    PaymentReceipt,
    /// Definitive statutory tax invoice issued upon stay completion.
    TaxInvoice,
    /// Post-completion adjustment or refund credit note.
    CreditNote,
    /// Pre-completion cancellation refund receipt.
    RefundReceipt,
}

impl TaxDocumentType {
    /// Returns the standardized code prefix for document numbering.
    pub fn prefix(&self) -> &'static str {
        match self {
            Self::PaymentReceipt => "REC",
            Self::TaxInvoice => "INV",
            Self::CreditNote => "CN",
            Self::RefundReceipt => "REF",
        }
    }

    /// User-facing document title.
    pub fn display_title(&self) -> &'static str {
        match self {
            Self::PaymentReceipt => "Booking & Payment Receipt",
            Self::TaxInvoice => "Statutory Tax Invoice",
            Self::CreditNote => "Tax Credit Note",
            Self::RefundReceipt => "Refund Receipt",
        }
    }
}

/// Category of tax applied to an individual line item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceTaxCategory {
    /// Standard Jamaican 15% General Consumption Tax.
    Gct15,
    /// Statutorily exempt supply or security escrow hold.
    Exempt,
    /// Zero-rated transaction.
    ZeroRated,
}

/// An itemized line entry within a statutory tax document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct InvoiceLineItem {
    pub description: String,
    pub quantity: i32,
    pub unit_price: Decimal,
    pub net_amount: Decimal,
    pub tax_category: InvoiceTaxCategory,
    pub tax_amount: Decimal,
    pub total_amount: Decimal,
}

/// Structured response DTO representing a tax receipt or invoice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TaxDocumentResponse {
    pub id: Uuid,
    pub document_type: TaxDocumentType,
    pub document_number: String,
    pub document_title: String,
    pub booking_id: Uuid,
    pub confirmation_code: String,
    pub status: String,
    pub issue_date: DateTime<Utc>,
    
    // Supplier
    pub supplier_name: String,
    pub supplier_address: String,
    pub supplier_trn: String,
    
    // Recipient
    pub recipient_name: String,
    pub recipient_email: String,
    pub recipient_company_name: Option<String>,
    pub recipient_trn: Option<String>,
    
    // Villa Details
    pub listing_id: Uuid,
    pub listing_title: String,
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
    pub total_nights: i32,
    
    // Financials
    pub currency: String,
    pub exchange_rate: Decimal,
    pub accommodation_subtotal: Decimal,
    pub cleaning_fee: Decimal,
    pub taxable_amount: Decimal,
    pub gct_rate: Decimal,
    pub gct_amount: Decimal,
    pub damage_deposit: Decimal,
    pub platform_fee: Decimal,
    pub total_amount: Decimal,
    pub jmd_gct_amount: Decimal,
    pub jmd_total_amount: Decimal,
    
    // Line items
    pub line_items: Vec<InvoiceLineItem>,
}
```

#### Metadata Expansion: `common/src/models.rs`
Update `BookingMetadata` and `BookingMetadataResponse`:
```rust
pub struct BookingMetadataResponse {
    // Existing fields...
    pub is_business_trip: bool,
    /// Optional corporate registered business name for tax invoicing.
    pub business_company_name: Option<String>,
    /// Optional corporate Taxpayer Registration Number (TRN) for tax deduction.
    pub business_tax_id: Option<String>,
}
```

#### Pricing Math Expansion: `common/src/pricing.rs`
Add `calculate_statutory_gct_breakdown`:
```rust
/// Computes itemized Jamaican GCT (15%) for accommodation and ancillary fees.
pub fn calculate_statutory_gct_breakdown(
    discounted_subtotal: Decimal,
    cleaning_fee: Decimal,
    damage_deposit: Decimal,
    platform_fee: Decimal,
) -> (Decimal, Decimal, Decimal) {
    let taxable_amount = discounted_subtotal + cleaning_fee;
    let gct_rate = Decimal::from_parts(15, 0, 0, false, 2); // 0.15
    let gct_amount = (taxable_amount * gct_rate).round_dp(2);
    let total_amount = taxable_amount + gct_amount + damage_deposit + platform_fee;
    (taxable_amount, gct_amount, total_amount)
}
```

---

### 2.3. `app_api/booking_api` (Backend Handlers & Rendering)

#### Configuration: `app_api/booking_api/src/settings.rs`
Add strongly-typed supplier settings adhering to `RS-W1015`:
```rust
#[derive(Debug, Deserialize, Clone)]
pub struct TaxSettings {
    pub taj_supplier_trn: String,
    pub supplier_name: String,
    pub supplier_address: String,
}
```

#### Endpoints: `app_api/booking_api/src/apis.rs`
| Method | Route | Auth / Role | Description |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/bookings/{id}/receipt` | Guest (owner), Host, Admin | Returns JSON `TaxDocumentResponse` for the Payment Receipt. |
| `GET` | `/api/v1/bookings/{id}/receipt/render` | Guest (owner), Host, Admin | Returns print-optimized HTML (`text/html`) for Payment Receipt. |
| `GET` | `/api/v1/bookings/{id}/invoice` | Guest (owner), Host, Admin | Returns JSON `TaxDocumentResponse` for the Statutory Tax Invoice (404 if booking not completed yet). |
| `GET` | `/api/v1/bookings/{id}/invoice/render` | Guest (owner), Host, Admin | Returns print-optimized HTML (`text/html`) for Statutory Tax Invoice. |
| `GET` | `/api/v1/bookings/{id}/documents` | Guest (owner), Host, Admin | Returns array of all available tax documents for the booking. |
| `GET` | `/api/v1/admin/documents` | Admin only | Returns paginated list of all platform tax documents. |

#### Internal Trigger Integration:
1. **On Booking Confirmation (`BookingStatus::Confirmed`)**:
   - Inside confirmation transaction: Synchronously insert `tax_document` with `document_type = PaymentReceipt` and number `REC-YYYY-{code}`.
2. **On Booking Completion (`BookingStatus::Completed`)**:
   - Inside completion state transition: Synchronously insert `tax_document` with `document_type = TaxInvoice` and number `INV-YYYY-{code}`.
3. **On Booking Cancellation (`BookingStatus::Cancelled`)**:
   - If cancelled before completion: Synchronously insert `tax_document` with `document_type = RefundReceipt` (`REF-YYYY-{code}`).
   - If cancelled after completion: Synchronously insert `tax_document` with `document_type = CreditNote` (`CN-YYYY-{code}`).

#### Print-Optimized HTML Rendering Engine: `app_api/booking_api/src/invoice_renderer.rs` [NEW]
- Generates clean, accessible semantic HTML with inline CSS.
- Supports both `PaymentReceipt` and `TaxInvoice` modes with appropriate legal headings, watermarks, and labels.
- Implements CSS `@media print` rules:
  - `@page { size: A4; margin: 15mm; }`
  - Zero web fonts or external scripts (native typography stack: `-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif`).
  - Browser "Print / Save PDF" button with `window.print()` that automatically hides during print execution (`@media print { .no-print { display: none !important; } }`).
  - Escapes all recipient inputs to eliminate HTML/script injection.

---

### 2.4. `app_api/email_worker` (Transactional Email Integration)

#### Updated Templates:
1. **`booking_confirmation_guest`**:
   - Injects an itemized financial summary table (Accommodation subtotal, Cleaning fee, Statutory 15% GCT, Damage deposit, Total paid).
   - Injects a call-to-action button: `[View Payment Receipt]` pointing to `{BASE_URL}/bookings/{id}/receipt`.
2. **`booking_completed_guest`** (Post-Stay Notification):
   - Injects a call-to-action button: `[Download Official Tax Invoice]` pointing to `{BASE_URL}/bookings/{id}/invoice`.

---

### 2.5. Frontend Portals (`web_app_tc` & `web_app_admin_tc`)

#### Guest Portal (`web_app_tc`):
- `/bookings/{id}/receipt`: Dedicated printable Payment Receipt page.
- `/bookings/{id}/invoice`: Dedicated printable Tax Invoice page (active once completed).
- Booking detail page `/bookings/{id}`:
  - Displays "View Payment Receipt" badge upon confirmation.
  - Automatically activates "Download Statutory Tax Invoice" badge upon stay completion.

#### Admin Portal (`web_app_admin_tc`):
- `/admin/bookings` detail modal updated with a "Tax Documents" panel listing all generated documents (Receipt, Invoice, Refund, Credit Note).
- Admin navigation menu updated with an "Invoices & Receipts" audit page linking to `/admin/documents`.

---

## 3. Performance Considerations & Scalability Check

### 3.1. Big-O Complexity & Query Efficiency
- **Lookup Cost**: $O(1)$ indexed retrieval via `tax_document.booking_id` and `tax_document.document_number`.
- **Zero N+1 Queries**: All line items are stored as structured JSONB within the `tax_document` row. Fetching a document executes exactly **1 database query** with zero downstream foreign key joins.
- **Admin Pagination**: Admin queries strictly enforce `LIMIT` and `OFFSET` (capped at max 100 items per page) with compound index scanning on `issue_date DESC`.

### 3.2. Cloud Run Scale-to-Zero Constraints
- **Latency Budget**: $<10\text{ms}$ HTML render time; $<30\text{ms}$ database write inside confirmation and completion transactions. Well within the $<300\text{ms}$ (p50) cold start target.
- **Memory Footprint**: Pure Rust string formatting using pre-allocated `String` buffers (`String::with_capacity(4096)`). Zero headless browser processes (Chromium/Puppeteer), maintaining container memory consumption under 64MB of the 256MB threshold.
- **Zero Blocking Async I/O**: String formatting and template substitution are pure CPU operations completed in microseconds without blocking Tokio worker threads.

---

## 4. Threat Modeling & Security Mitigations (OWASP Top 10)

| Threat / Risk | OWASP Category | Architectural Mitigation |
| :--- | :--- | :--- |
| **Insecure Direct Object Reference (IDOR)** | Broken Access Control (A01) | Handlers check `claims.sub`: Only the booking guest, the property host, or platform administrators can view an invoice or receipt. Unauthorized access returns `404 Not Found` (to prevent ID enumeration). |
| **Cross-Site Scripting (XSS)** | Injection (A03) | All user-controlled strings (guest name, company name, address, notes) rendered in HTML invoices are strictly entity-escaped using a fast escaping utility (`&` $\rightarrow$ `&amp;`, `<` $\rightarrow$ `&lt;`, `>` $\rightarrow$ `&gt;`, `"` $\rightarrow$ `&quot;`, `'` $\rightarrow$ `&#x27;`). |
| **SQL Injection** | Injection (A03) | 100% compile-time verified parameterized queries using `sqlx::query!` and `sqlx::query_as!`. Zero raw SQL string interpolation. |
| **Tampering & Document Mutation** | Cryptographic & Integrity Failures (A08) | `tax_document` table has no `UPDATE` or `DELETE` API endpoints. Once created with status `'issued'`, records cannot be altered. |
| **PII & TRN Data Exposure** | Identification & Auth Failures (A07) | TRNs and financial data are excluded from unauthenticated endpoints. Sensitive TRN fields are logged only at `DEBUG` or `TRACE` levels and redacted in `INFO`/`WARN` logs. |
| **Document Number Collision / Race Condition** | Concurrency Vulnerabilities | `UNIQUE(booking_id, document_type)` and `UNIQUE(document_number)` database constraints guarantee that concurrent requests cannot produce duplicate documents for the same booking. |

---

## 5. Comprehensive Test Plan

### 5.1. Unit Tests (`common`)
1. `test_statutory_gct_tax_calculation_precision`:
   - Validates that accommodation subtotal ($1,000.00) + cleaning fee ($150.00) yields exactly $172.50 GCT (15%).
   - Verifies that damage deposit ($500.00) is excluded from the tax base.
2. `test_zero_float_bankers_rounding`:
   - Tests fractional sub-cent calculations across odd night rates (e.g. $133.33/night × 3 nights) to ensure zero cent-rounding drift.
3. `test_tri_currency_jmd_tax_conversion`:
   - Validates that USD bookings converted to JMD record the exact statutory JMD GCT equivalent at the snapshotted exchange rate.
4. `test_deterministic_document_number_formatting`:
   - Asserts generated receipt code matches regex `^REC-\d{4}-[A-Z0-9]{10,12}$`.
   - Asserts generated invoice code matches regex `^INV-\d{4}-[A-Z0-9]{10,12}$`.
5. `test_html_entity_escaping`:
   - Verifies that malicious company names like `<script>alert(1)</script>` or `" onfocus="alert('xss')"` are sanitized to `&lt;script&gt;alert(1)&lt;/script&gt;`.

### 5.2. Integration & Database Tests (`db_core` & `app_api/booking_api`)
1. `test_payment_receipt_generated_on_confirmation`:
   - Transition booking to `confirmed`.
   - Verify `tax_document` with `PaymentReceipt` is synchronously created.
   - Verify `GET /api/v1/bookings/{id}/receipt` returns `200 OK`.
   - Verify `GET /api/v1/bookings/{id}/invoice` returns `404 Not Found` (stay not yet completed).
2. `test_tax_invoice_generated_on_completion`:
   - Transition confirmed booking to `completed`.
   - Verify `tax_document` with `TaxInvoice` is synchronously created.
   - Verify `GET /api/v1/bookings/{id}/invoice` returns `200 OK`.
3. `test_refund_receipt_on_early_cancellation`:
   - Cancel confirmed booking prior to completion.
   - Verify `tax_document` with `RefundReceipt` is generated.
   - Verify no `TaxInvoice` is ever created.
4. `test_tenant_isolation_and_authorization`:
   - Asserts guest $A$ receives `404` when requesting guest $B$'s receipt or invoice URL.
   - Asserts listing host receives `200 OK`.
   - Asserts platform admin receives `200 OK`.
   - Asserts unauthenticated caller receives `401 Unauthorized`.
5. `test_strict_taj_trn_env_validation`:
   - Asserts `TaxSettings` returns a configuration validation error if `TAJ_SUPPLIER_TRN` is empty or missing.
6. `test_html_render_endpoint`:
   - Asserts `GET /api/v1/bookings/{id}/receipt/render` returns `Content-Type: text/html; charset=utf-8` containing correct itemized numbers and `@media print` styles.

### 5.3. End-to-End Tests (`playwright`)
1. `guest-portal/tax-receipt-and-invoice.spec.ts`:
   - Guest logs in and navigates to confirmed booking `/bookings/{id}`.
   - Clicks "View Payment Receipt" and verifies itemized breakdown.
   - After booking status is transitioned to `completed`, verifies "Download Statutory Tax Invoice" button appears and functions.
2. `admin-portal/tax-documents.spec.ts`:
   - Admin logs into `/admin/bookings`.
   - Opens booking details and clicks "Tax Documents".
   - Verifies both Payment Receipt and Tax Invoice appear and are viewable/printable.
   - Navigates to `/admin/documents` and verifies all records appear in the paginated audit table.

---

## 6. Acceptance Criteria
- [ ] Database migration `20261013000000_create_tax_document_table.sql` created and verified with compile-time SQLx offline metadata.
- [ ] Shared DTOs (`TaxDocumentResponse`, `TaxDocumentType`, `InvoiceLineItem`, `InvoiceTaxCategory`) and pure decimal math implemented in `common`.
- [ ] `BookingMetadata` updated to accept optional corporate B2B company name and guest TRN.
- [ ] Synchronous receipt generation hook integrated into `BookingStatus::Confirmed`.
- [ ] Synchronous statutory invoice generation hook integrated into `BookingStatus::Completed`.
- [ ] REST API endpoints for JSON retrieval and print-optimized HTML rendering implemented and documented in OpenAPI schema.
- [ ] Guest portal (`web_app_tc`) and admin portal (`web_app_admin_tc`) updated to view and print both receipts and invoices.
- [ ] Transactional email templates updated in `email_worker`.
- [ ] 100% test pass rate across `cargo test --workspace` with zero Clippy warnings (`-D warnings`).
- [ ] Zero floating-point types (`f32`/`f64`) and zero panics (`unwrap`/`expect`) across all production paths.
