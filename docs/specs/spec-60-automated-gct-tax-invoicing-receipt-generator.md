# Spec 60: Automated GCT Tax Invoicing & Receipt Generator

## Overview
This feature introduces an automated, statutory-compliant tax invoicing and receipt generation module for the Our Places platform. Whenever a booking is confirmed (`BookingStatus::Confirmed`), an immutable statutory tax invoice (`tax_invoice`) is generated and linked to both the booking record and its immutable `booking_history` audit snapshot. 

The system computes statutory Jamaican General Consumption Tax (GCT at 15%) on accommodation subtotals and taxable service fees (cleaning fees), while isolating non-taxable refundable deposits (damage deposits). Invoices are rendered via print-optimized, semantic HTML (`@media print`) that executes in $<10\text{ms}$ under Cloud Run's 256MB memory budget without external headless browser dependencies. Guests and administrators can view, print, or download compliant receipts across both portals (`web_app_tc` and `web_app_admin_tc`), while `email_worker` automatically dispatches itemized tax summaries in confirmation notifications.

---

## 1. Domain Decisions & Business Rules

### 1.1. Statutory Tax Base & Decimal Precision
- **Statutory Rate**: 15% Jamaican GCT loaded from static reference data (`common::reference::SupportedCountry::LIST`).
- **Taxable Line Items**:
  - Accommodation Nightly Subtotal (`discounted_subtotal` = $\sum \text{nightly rates} - \text{stay discounts}$).
  - Cleaning Fee (statutorily classified as a taxable hospitality service under Jamaican GCT legislation).
- **Non-Taxable Escrow Items**:
  - Damage Deposit (classified as a temporary refundable security hold, exempt from GCT).
- **Tax Calculation Formula**:
  $$\text{taxable\_base} = \text{discounted\_subtotal} + \text{cleaning\_fee}$$
  $$\text{gct\_amount} = (\text{taxable\_base} \times 0.1500).\text{round\_dp}(2)$$
  $$\text{total\_invoice\_amount} = \text{taxable\_base} + \text{gct\_amount} + \text{damage\_deposit} + \text{platform\_fee}$$
- **Zero-Float Policy**: All rates, subtotals, taxes, and totals strictly use `rust_decimal::Decimal`. Floating point types (`f32`/`f64`) are strictly prohibited per `CONSTRAINTS.md`.

### 1.2. Tri-Currency Flow & Exchange Rate Snapshots
- Invoices are denominated in the **Guest Payment Currency** (e.g. JMD, USD, GBP, EUR, CAD) selected at checkout.
- If the listing's base currency differs from the payment currency, the active exchange rate from `currency_exchange_rates` is permanently snapshotted in `tax_invoice.exchange_rate`.
- For foreign currency invoices, the Jamaican Dollar (JMD) statutory equivalent total and GCT amount are recorded to satisfy Tax Administration Jamaica (TAJ) audit reporting.

### 1.3. Merchant of Record & TRN Attribution
- **Platform Entity**: "Our Places Luxury Villas Ltd" acts as the centralized Merchant of Record.
- **Supplier Identification**: The invoice header prominently displays the platform's registered name, address, and 9-digit Taxpayer Registration Number (`TAJ_SUPPLIER_TRN`).
- **Configuration Enforcement**: `TAJ_SUPPLIER_TRN` is a mandatory configuration property loaded through typed application settings (`settings.rs`) conforming to rule `RS-W1015`. If unset, invoice generation fails fast.
- **B2B Guest Deductibility**: Corporate guests traveling on business (`is_business_trip = true` in `BookingMetadata`) may optionally provide their `business_company_name` and `business_tax_id` (Guest TRN) to claim input tax credits with TAJ.

### 1.4. Invoice Numbering & Immutability
- **Deterministic Format**: `INV-YYYY-{confirmation_code}` (e.g., `INV-2026-HMEJYC3P9B`). This ensures idempotency, uniqueness, and human readability.
- **Strict Immutability**: Once created, a `tax_invoice` record is immutable. No `UPDATE` or `DELETE` operations are permitted on issued invoices.
- **Cancellations & Credit Notes**: If a confirmed booking is subsequently cancelled, an explicit credit note (`CN-YYYY-{confirmation_code}`) is generated and linked in `booking_history`, preserving the original invoice intact.

---

## 2. Technical Specification

### 2.1. `db_core` (Schema & Persistence)

#### Migration: `db_core/migrations/20261013000000_create_tax_invoice_table.sql`
```sql
CREATE TYPE invoice_status AS ENUM ('issued', 'credited', 'voided');

CREATE TABLE IF NOT EXISTS tax_invoice (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    invoice_number VARCHAR(32) NOT NULL UNIQUE,
    booking_id UUID NOT NULL UNIQUE REFERENCES booking(id) ON DELETE RESTRICT,
    booking_history_id UUID REFERENCES booking_history(id),
    recipient_id UUID NOT NULL REFERENCES "user"(id),
    listing_id UUID NOT NULL REFERENCES listing(id),
    
    -- Status & Dates
    status invoice_status NOT NULL DEFAULT 'issued',
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
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Performance Indexes
CREATE INDEX idx_tax_invoice_booking_id ON tax_invoice(booking_id);
CREATE INDEX idx_tax_invoice_recipient_id ON tax_invoice(recipient_id);
CREATE INDEX idx_tax_invoice_listing_id ON tax_invoice(listing_id);
CREATE INDEX idx_tax_invoice_issue_date ON tax_invoice(issue_date);
```

#### SQLx Model & Queries: `db_core/src/tax_invoice.rs` [NEW]
- `TaxInvoiceRow` struct deriving `sqlx::FromRow`.
- `insert_tax_invoice(pool, invoice: &NewTaxInvoice) -> Result<TaxInvoiceRow, sqlx::Error>`.
- `get_invoice_by_booking_id(pool, booking_id: Uuid) -> Result<Option<TaxInvoiceRow>, sqlx::Error>`.
- `get_invoice_by_number(pool, invoice_number: &str) -> Result<Option<TaxInvoiceRow>, sqlx::Error>`.
- `list_invoices_for_user(pool, user_id: Uuid, limit: i64, offset: i64) -> Result<Vec<TaxInvoiceRow>, sqlx::Error>`.
- `list_invoices_for_host(pool, host_id: Uuid, limit: i64, offset: i64) -> Result<Vec<TaxInvoiceRow>, sqlx::Error>`.
- `list_all_invoices_admin(pool, limit: i64, offset: i64) -> Result<Vec<TaxInvoiceRow>, sqlx::Error>`.

---

### 2.2. `common` (Isomorphic Types & Pricing Math)

#### Domain Models: `common/src/models.rs` & `common/src/tax_invoice.rs` [NEW]
```rust
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

/// An itemized line entry within a statutory tax invoice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct InvoiceLineItem {
    /// Item description (e.g. "Accommodation (5 nights)", "Cleaning Fee").
    pub description: String,
    /// Quantity (e.g. number of nights or fee count).
    pub quantity: i32,
    /// Unit price in payment currency.
    pub unit_price: Decimal,
    /// Net amount before statutory tax.
    pub net_amount: Decimal,
    /// Tax category applicable to this line item.
    pub tax_category: InvoiceTaxCategory,
    /// Calculated tax amount for this line item.
    pub tax_amount: Decimal,
    /// Total amount including tax.
    pub total_amount: Decimal,
}

/// Structured response DTO representing a compliance tax invoice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TaxInvoiceResponse {
    pub id: Uuid,
    pub invoice_number: String,
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
Add `calculate_tax_breakdown`:
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
| `POST` | `/api/v1/bookings/{id}/invoice/generate` | Internal / Confirmation | Generates invoice record idempotently upon payment confirmation. |
| `GET` | `/api/v1/bookings/{id}/invoice` | Guest (owner), Host, Admin | Returns structured JSON `TaxInvoiceResponse`. |
| `GET` | `/api/v1/bookings/{id}/invoice/render` | Guest (owner), Host, Admin | Returns print-optimized semantic HTML (`text/html`). |
| `GET` | `/api/v1/admin/invoices` | Admin only | Returns paginated list of all platform tax invoices. |

#### Print-Optimized HTML Rendering Engine: `app_api/booking_api/src/invoice_renderer.rs` [NEW]
- Generates clean, accessible semantic HTML with inline CSS.
- Implements CSS `@media print` rules:
  - `@page { size: A4; margin: 15mm; }`
  - Zero web fonts or external scripts (uses native system typography stack: `-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif`).
  - Hides non-printable navigation, adds a browser "Print / Save PDF" button with `window.print()` that automatically disappears during print execution (`@media print { .no-print { display: none !important; } }`).
  - Escapes all recipient inputs to eliminate HTML/script injection.

---

### 2.4. `app_api/email_worker` (Transactional Email Integration)

#### Updated Template: `booking_confirmation_guest`
- Injects an itemized financial summary table:
  - Room subtotal
  - Cleaning fee
  - Statutory 15% Jamaican GCT
  - Refundable damage deposit
  - Total paid
- Injects a verified call-to-action button: `[View Official Tax Invoice / Receipt]` pointing to the guest portal URL: `{BASE_URL}/bookings/{id}/receipt`.

---

### 2.5. Frontend Portals (`web_app_tc` & `web_app_admin_tc`)

#### Guest Portal (`web_app_tc`):
- Dedicated route `/bookings/{id}/receipt`:
  - Directly proxies or renders the printable invoice view with Topcoat styling.
- Booking detail page `/bookings/{id}`:
  - Adds a prominent "View Tax Invoice" link in the payment summary card.

#### Admin Portal (`web_app_admin_tc`):
- `/admin/bookings` tax modal updated to load dynamic data from `/api/v1/bookings/{id}/invoice`.
- Admin navigation menu updated with an "Invoices & GCT" view linking to `/admin/invoices`.

---

## 3. Performance Considerations & Scalability Check

### 3.1. Big-O Complexity & Query Efficiency
- **Lookup Cost**: $O(1)$ indexed retrieval via `tax_invoice.booking_id` and `tax_invoice.invoice_number`.
- **Zero N+1 Queries**: All line items are stored as structured JSONB within the `tax_invoice` row. Fetching an invoice executes exactly **1 database query** with zero downstream foreign key joins.
- **Admin Pagination**: Admin queries strictly enforce `LIMIT` and `OFFSET` (capped at max 100 items per page) with compound index scanning on `issue_date DESC`.

### 3.2. Cloud Run Scale-to-Zero Constraints
- **Latency Budget**: $<10\text{ms}$ HTML render time; $<30\text{ms}$ database write inside the confirmation transaction. Well within the $<300\text{ms}$ (p50) cold start target.
- **Memory Footprint**: Pure Rust string formatting using pre-allocated `String` buffers (`String::with_capacity(4096)`). Zero headless browser processes (Chromium/Puppeteer), maintaining container memory consumption under 64MB of the 256MB threshold.
- **Zero Blocking Async I/O**: String formatting and template substitution are pure CPU operations completed in microseconds without blocking Tokio worker threads.

---

## 4. Threat Modeling & Security Mitigations (OWASP Top 10)

| Threat / Risk | OWASP Category | Architectural Mitigation |
| :--- | :--- | :--- |
| **Insecure Direct Object Reference (IDOR)** | Broken Access Control (A01) | Handlers check `claims.sub`: Only the booking guest, the property host, or platform administrators can view an invoice. Unauthorized access returns `404 Not Found` (to prevent ID enumeration). |
| **Cross-Site Scripting (XSS)** | Injection (A03) | All user-controlled strings (guest name, company name, address, notes) rendered in HTML invoices are strictly entity-escaped using a fast escaping utility (`v_htmlescape` / custom entity replacer: `&` $\rightarrow$ `&amp;`, `<` $\rightarrow$ `&lt;`, `>` $\rightarrow$ `&gt;`, `"` $\rightarrow$ `&quot;`, `'` $\rightarrow$ `&#x27;`). |
| **SQL Injection** | Injection (A03) | 100% compile-time verified parameterized queries using `sqlx::query!` and `sqlx::query_as!`. Zero raw SQL string interpolation. |
| **Tampering & Invoice Mutation** | Cryptographic & Integrity Failures (A08) | `tax_invoice` table has no `UPDATE` or `DELETE` API endpoints. Once created with status `'issued'`, invoices cannot be altered. Status changes are restricted to programmatic voids triggered by cancellations. |
| **PII & TRN Data Exposure** | Identification & Auth Failures (A07) | TRNs and invoice financial data are excluded from unauthenticated endpoints. Sensitive TRN fields are logged only at `DEBUG` or `TRACE` levels and redacted in `INFO`/`WARN` logs. |
| **Invoice Number Collision / Race Condition** | Concurrency Vulnerabilities | `UNIQUE(booking_id)` and `UNIQUE(invoice_number)` database constraints guarantee that two concurrent requests cannot produce duplicate invoices for the same booking. |

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
4. `test_deterministic_invoice_number_format`:
   - Asserts generated invoice code matches regex `^INV-\d{4}-[A-Z0-9]{10,12}$`.
5. `test_html_entity_escaping`:
   - Verifies that malicious company names like `<script>alert(1)</script>` or `" onfocus="alert('xss')"` are sanitized to `&lt;script&gt;alert(1)&lt;/script&gt;`.

### 5.2. Integration & Database Tests (`db_core` & `app_api/booking_api`)
1. `test_insert_and_retrieve_tax_invoice`:
   - Verifies inserting a `tax_invoice` row and loading all fields via SQLx compile-time query.
2. `test_duplicate_invoice_booking_rejection`:
   - Asserts database constraint violation if inserting a second invoice for the same `booking_id`.
3. `test_invoice_creation_on_booking_confirmation`:
   - Drives booking from `pending` to `confirmed` and asserts `tax_invoice` record is synchronously created and linked to `booking_history`.
4. `test_tenant_isolation_and_authorization`:
   - Asserts guest $A$ receives `404` when requesting guest $B$'s invoice URL.
   - Asserts listing host receives `200 OK`.
   - Asserts platform admin receives `200 OK`.
   - Asserts unauthenticated caller receives `401 Unauthorized`.
5. `test_strict_taj_trn_env_validation`:
   - Asserts `TaxSettings` returns a configuration validation error if `TAJ_SUPPLIER_TRN` is empty or missing.
6. `test_html_render_endpoint`:
   - Asserts `GET /api/v1/bookings/{id}/invoice/render` returns `Content-Type: text/html; charset=utf-8` containing correct itemized numbers and `@media print` styles.

### 5.3. End-to-End Tests (`playwright`)
1. `guest-portal/tax-receipt.spec.ts`:
   - Guest logs in and navigates to confirmed booking `/bookings/{id}`.
   - Clicks "View Tax Receipt".
   - Verifies modal / receipt page displays:
     - Supplier Name & TRN
     - Guest Name
     - Accommodation Subtotal
     - Cleaning Fee
     - Statutory 15% Jamaican GCT
     - Damage Deposit
     - Total Paid
   - Verifies browser print stylesheet trigger exists and is clickable.
2. `admin-portal/tax-invoices.spec.ts`:
   - Admin logs into `/admin/bookings`.
   - Opens booking details and clicks "Tax Invoice".
   - Verifies invoice details match database records.
   - Navigates to `/admin/invoices` and verifies invoice appears in the paginated audit table.

---

## 6. Acceptance Criteria
- [ ] Database migration `20261013000000_create_tax_invoice_table.sql` created and verified with compile-time SQLx offline metadata.
- [ ] Shared DTOs (`TaxInvoiceResponse`, `InvoiceLineItem`, `InvoiceTaxCategory`) and pure decimal math implemented in `common`.
- [ ] `BookingMetadata` updated to accept optional corporate B2B company name and guest TRN.
- [ ] Automatic invoice generation hook integrated into the booking confirmation flow in `booking_api`.
- [ ] REST API endpoints for JSON retrieval and print-optimized HTML rendering implemented and documented in OpenAPI schema.
- [ ] Guest portal (`web_app_tc`) and admin portal (`web_app_admin_tc`) updated to view and print invoices.
- [ ] Transactional email template in `email_worker` updated to include itemized GCT receipt summary and direct portal link.
- [ ] 100% test pass rate across `cargo test --workspace` with zero Clippy warnings (`-D warnings`).
- [ ] Zero floating-point types (`f32`/`f64`) and zero panics (`unwrap`/`expect`) across all production paths.
