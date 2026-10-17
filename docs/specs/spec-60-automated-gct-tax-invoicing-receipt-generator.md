# Spec 60: Automated GCT Tax Invoicing & Receipt Generator (rev. 2)

> Supersedes rev. 1. Driven by the evaluation of 2026-10-13. Intent: [intent-60](../intent/intent-60-automated-gct-tax-invoicing-receipt-generator.md).

## Overview
Introduces host-configurable cleaning fees and damage deposits, a single country-resolved GCT rate (default JM), policy-driven partial refunds (typestate across 5 policies: Flexible, Moderate, Strict, Firm, Limited), date changes on confirmed bookings, automatic stay completion via Cloud Scheduler, and a **Dual Document Model** of immutable tax documents:

| Trigger | Document | Number |
| :--- | :--- | :--- |
| `pending → confirmed` | Payment Receipt | `REC-YYYY-{code}` |
| Date change on confirmed booking | Revised Payment Receipt (supersedes previous) | `REC-YYYY-{code}-R{n}` |
| `confirmed → completed` | Statutory Tax Invoice | `INV-YYYY-{code}` |
| `confirmed → cancelled` | Refund Receipt | `REF-YYYY-{code}` |

APIs return **JSON only**. All HTML is rendered by Topcoat SSR (`web_app_common_tc` component, consumed by `web_app_tc` and `web_app_admin_tc`).

---

## 1. Domain Rules

### 1.1. Tax Rate Resolution (single source of truth)
```rust
// common/src/reference.rs
pub const DEFAULT_TAX_COUNTRY: &str = "JM";

impl SupportedCountry {
    /// Resolves the statutory rate for an ISO2 code, falling back to JM.
    pub fn tax_rate_for(iso2: &str) -> Decimal { /* find(iso2).or_else(find(JM)).map(rate) */ }
}
```
- `BookingCalculator<Discounted>::apply_taxes(self, tax_rate: Decimal)` replaces the hardcoded `Decimal::new(10, 2)`.
- Callers pass `SupportedCountry::tax_rate_for(&listing.country)`. The listing quote (`/listings/{id}/quote`), `create_booking`, and date-change re-pricing all go through the same calculator.
- `booking.tax_rate` snapshots the rate used.

### 1.2. Pricing & Reconciliation Invariant
All amounts are in the **payment currency** and rounded to 2dp with banker's rounding.

```
accommodation_net = sub_total_price − discount_value
taxable_base      = accommodation_net + cleaning_fee
tax_value         = round(taxable_base × tax_rate)
platform_fee      = round(accommodation_net × 0.05)          // existing rule, untaxed
total_price       = taxable_base + tax_value + platform_fee + damage_deposit
```

| Document | `total_amount` | Must equal |
| :--- | :--- | :--- |
| Payment Receipt | `total_price` (incl. deposit held) | `booking.total_price` |
| Tax Invoice | `taxable_base + tax_value + platform_fee` (supply only) | `booking.total_price − damage_deposit` |
| Refund Receipt | `refund_total` | `Σ refund lines` (§1.4) |

- Damage deposit appears on receipts as a non-taxable "Security deposit (held)" line. On the invoice it's a memo line, "Security deposit — released in full", excluded from `total_amount`. Deposit claims are out of scope.
- `BookingCalculator` gains `with_fees(cleaning_fee, damage_deposit)` (on `Discounted`). `finalize()` computes `platform_fee` and `total_price` per the formula above, so the typestate order becomes `BaseRate → Discounted → (fees) → Taxed → finalize`.
- Listing fees are stored in the listing's `base_currency` and converted at the same `exchange_rate` as the nightly rate.

### 1.3. Exchange Rates
- `booking.exchange_rate`: base→payment rate used at pricing (already applied in `create_booking`; now snapshotted).
- `tax_document.jmd_exchange_rate`: payment→JMD rate at issue time.
- Lookup `db_core::exchange_rate::get_rate(base, target, at)`:
  1. `base == target` ⇒ `1`.
  2. Direct pair, latest `effective_at <= at`.
  3. Inverse pair ⇒ `1 / rate`, rounded to 6dp.
  4. Otherwise `Err(FxRateUnavailable)` ⇒ HTTP 422 `{ "code": "FX_RATE_UNAVAILABLE" }`. Never silently default.
- `jmd_tax_amount = round(tax_value × jmd_exchange_rate)` and `jmd_total_amount = round(total_amount × jmd_exchange_rate)`.

> [!WARNING]
> Seed data only contains USD/JMD/GBP pairs. EUR and CAD (advertised on the home page) will return `FX_RATE_UNAVAILABLE` until rates are added.

### 1.4. Cancellation Refund Policies (typestate)
`cancellation_policy` gains `firm` and `limited`. Timing is evaluated relative to `check_in` in `America/Jamaica` (lower bounds inclusive). Hours since booking is evaluated from `booked_at`.

| Policy | 100% Accommodation Refund Window | 50% Accommodation Refund Window | 0% Refund (100% Host Payout) |
| :--- | :--- | :--- | :--- |
| **Flexible** | ≥ 24 hours before check-in | — | < 24 hours before check-in |
| **Moderate** | ≥ 5 days before check-in | 5 days → 24 hours before check-in | < 24 hours before check-in |
| **Strict** | Within 48h of booking (if booked ≥ 14 days out) | ≥ 7 days before check-in (outside 48h grace) | < 7 days before check-in |
| **Firm** | ≥ 30 days before check-in (or within 48h if booked ≥ 14d out) | 7 to 30 days before check-in | < 7 days before check-in |
| **Limited** | ≥ 14 days before check-in | 7 to 14 days before check-in | < 7 days before check-in |

```rust
// common/src/cancellation.rs [NEW]
pub struct Flexible;
pub struct Moderate;
pub struct Strict;
pub struct Firm;
pub struct Limited;

/// Policy-specific refund schedule, resolved at compile time.
pub trait RefundSchedule {
    fn accommodation_refund_pct(t: &CancellationTiming) -> Decimal;
}

pub struct CancellationTiming {
    pub days_before_check_in: i64,
    pub hours_before_check_in: i64,
    pub hours_since_booking: i64,
    pub days_booked_ahead: i64,
}

// Typestate: Pending (amounts loaded) → Timed (timing evaluated) → Quoted (immutable result)
pub struct Pending; pub struct Timed; pub struct Quoted;

pub struct RefundCalculator<P: RefundSchedule, S> {
    amounts: RefundableAmounts,      // snapshot from booking columns
    timing: Option<CancellationTiming>,
    _p: PhantomData<(P, S)>,
}

impl<P: RefundSchedule> RefundCalculator<P, Pending> {
    pub fn new(amounts: RefundableAmounts) -> Self;
    pub fn at(self, booked_at: DateTime<Utc>, check_in: NaiveDate, cancelled_at: DateTime<Utc>)
        -> Result<RefundCalculator<P, Timed>, CancellationError>; // Err if cancelled_at ≥ check_in
}
impl<P: RefundSchedule> RefundCalculator<P, Timed> {
    pub fn quote(self) -> RefundQuote;
}

// Runtime dispatch from the persisted enum:
pub fn quote_refund(policy: CancellationPolicy, amounts: RefundableAmounts, ...) -> Result<RefundQuote, CancellationError>
```

**Refund component breakdown (watertight mathematical rules):**
```
accommodation_refund = round(accommodation_net × policy_pct)
cleaning_refund      = cleaning_fee                   // service never rendered, always 100% refunded
deposit_refund       = damage_deposit                 // escrow hold released, always 100% refunded
tax_refund           = round((accommodation_refund + cleaning_refund) × tax_rate)  // pro-rata on refunded taxable base
platform_fee_refund  = policy_pct == 1.00 ? platform_fee : 0                      // refunded only on 100% full refund
refund_total         = accommodation_refund + cleaning_refund + deposit_refund + tax_refund + platform_fee_refund
retained_total       = total_price − refund_total     // host payout + platform retained
```
When `policy_pct == 1.00` (full refund), `refund_total == total_price` and `retained_total == 0.00`.
When `policy_pct < 1.00`, the cleaning fee and damage deposit are still refunded in full, accommodation is refunded at `policy_pct`, GCT is refunded proportionally, and the platform fee is retained.

### 1.5. Date Changes
- Allowed when `status = confirmed` and `new_check_in > today (America/Jamaica)`. Changes on any other status ⇒ 409 `BOOKING_NOT_MODIFIABLE`.
- `booking_api` re-prices via the same quote pipeline (overrides, discounts, fees, tax, FX). `db_core` then:
  1. Locks the listing row and the booking row (`FOR UPDATE`), the same as `create_booking`.
  2. Re-checks availability, excluding this booking ⇒ 409 `DATE_UNAVAILABLE`.
  3. Updates dates and all financial columns, and records `booking_history`.
  4. Supersedes the active receipt and issues `REC-…-R{n}` with `amount_previously_paid` and `balance_due` (positive = guest owes, negative = refundable).
- No change fee in MVP. Money movement for `balance_due` is manual (out of scope).

### 1.6. Stay Completion
- `PATCH /api/v1/internal/cron/complete-stays` (header `x-cron-secret`), triggered daily at 12:00 America/Jamaica.
- Selects `id FROM booking WHERE status = 'confirmed' AND date_to <= (NOW() AT TIME ZONE 'America/Jamaica')::date LIMIT 500`, then calls `db_core::booking::update_booking(id, status: Completed)` for each. That fires the existing review-token hook plus the new invoice hook, followed by the standard notification dispatch.
- Idempotent: `update_booking` re-reads status under `FOR UPDATE` and skips anything not `confirmed`. Per-booking failures are logged (`tracing::error!`) and counted, and don't abort the batch. Response: `{ "completed": n, "failed": m }`.

### 1.7. Merchant of Record & B2B
- Supplier: `TaxSettings { taj_supplier_trn, supplier_name, supplier_address }`, mandatory, no defaults (fail at startup).
- `CRON_SECRET` moves into typed settings, and the `"dev-cron-secret"` fallback is removed (the current `unwrap_or_else` violates the no-fallback rule).
- `BookingMetadata` gains `business_company_name: Option<String>` and `business_tax_id: Option<String>`, each with a max of 255/32 chars. These are printed on documents when present.

---

## 2. Technical Specification

### 2.1. `db_core` — Single Feature Migration
`db_core/migrations/20261013000000_gct_tax_documents.sql` (one migration for the branch, per AGENTS.md §6):

```sql
-- Cancellation policy (ADD VALUE is safe in-tx on PG ≥ 12 as long as it is not used in this migration)
ALTER TYPE cancellation_policy ADD VALUE IF NOT EXISTS 'firm';
ALTER TYPE cancellation_policy ADD VALUE IF NOT EXISTS 'limited';

-- Listing fees (base currency). 0 = not offered.
ALTER TABLE listing
  ADD COLUMN cleaning_fee   NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (cleaning_fee >= 0),
  ADD COLUMN damage_deposit NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (damage_deposit >= 0);
ALTER TABLE listing_history
  ADD COLUMN cleaning_fee   NUMERIC(12,2),
  ADD COLUMN damage_deposit NUMERIC(12,2);

-- Booking financial snapshot. Existing rows: tax_rate backfilled to the 10% actually charged.
ALTER TABLE booking
  ADD COLUMN cleaning_fee   NUMERIC(10,2) NOT NULL DEFAULT 0,
  ADD COLUMN damage_deposit NUMERIC(10,2) NOT NULL DEFAULT 0,
  ADD COLUMN platform_fee   NUMERIC(10,2) NOT NULL DEFAULT 0,
  ADD COLUMN tax_rate       NUMERIC(5,4)  NOT NULL DEFAULT 0.1000,
  ADD COLUMN exchange_rate  NUMERIC(15,6) NOT NULL DEFAULT 1;
UPDATE booking SET platform_fee = COALESCE(
  (SELECT SUM((f->>'amount')::numeric) FROM jsonb_array_elements(fee_breakdown) f
    WHERE f->>'name' = 'Platform Fee'), 0);
ALTER TABLE booking ALTER COLUMN tax_rate DROP DEFAULT;  -- new rows must supply it

-- booking_history: nullable so existing pg_cron INSERTs (explicit column lists) keep working
ALTER TABLE booking_history
  ADD COLUMN cleaning_fee NUMERIC(10,2), ADD COLUMN damage_deposit NUMERIC(10,2),
  ADD COLUMN platform_fee NUMERIC(10,2), ADD COLUMN tax_rate NUMERIC(5,4),
  ADD COLUMN exchange_rate NUMERIC(15,6);

-- Tax documents
CREATE TYPE tax_document_type   AS ENUM ('payment_receipt', 'tax_invoice', 'refund_receipt');
CREATE TYPE tax_document_status AS ENUM ('issued', 'superseded');

CREATE TABLE tax_document (
  id                     UUID PRIMARY KEY DEFAULT uuidv7(),
  document_type          tax_document_type   NOT NULL,
  document_number        VARCHAR(40)         NOT NULL UNIQUE,
  revision               INTEGER             NOT NULL DEFAULT 1 CHECK (revision >= 1),
  status                 tax_document_status NOT NULL DEFAULT 'issued',
  supersedes_id          UUID REFERENCES tax_document(id),
  booking_id             UUID NOT NULL REFERENCES booking(id) ON DELETE RESTRICT,
  booking_history_id     UUID REFERENCES booking_history(id),
  recipient_id           UUID NOT NULL REFERENCES "user"(id),
  listing_id             UUID NOT NULL REFERENCES listing(id),
  issue_date             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  supplier_name          VARCHAR(255) NOT NULL,
  supplier_address       TEXT         NOT NULL,
  supplier_trn           VARCHAR(32)  NOT NULL,
  recipient_name         VARCHAR(255) NOT NULL,
  recipient_email        VARCHAR(255) NOT NULL,
  recipient_company_name VARCHAR(255),
  recipient_trn          VARCHAR(32),
  currency               CHAR(3)       NOT NULL,
  exchange_rate          NUMERIC(15,6) NOT NULL,   -- base → payment
  jmd_exchange_rate      NUMERIC(15,6) NOT NULL,   -- payment → JMD
  accommodation_net      NUMERIC(12,2) NOT NULL,
  cleaning_fee           NUMERIC(12,2) NOT NULL,
  taxable_amount         NUMERIC(12,2) NOT NULL,
  tax_rate               NUMERIC(5,4)  NOT NULL,
  tax_amount             NUMERIC(12,2) NOT NULL,
  platform_fee           NUMERIC(12,2) NOT NULL,
  damage_deposit         NUMERIC(12,2) NOT NULL,
  total_amount           NUMERIC(12,2) NOT NULL,
  amount_previously_paid NUMERIC(12,2),            -- revised receipts only
  balance_due            NUMERIC(12,2),            -- revised receipts only (+ owed / − refundable)
  refund_pct             NUMERIC(5,4),             -- refund receipts only
  retained_amount        NUMERIC(12,2),            -- refund receipts only
  jmd_tax_amount         NUMERIC(14,2) NOT NULL,
  jmd_total_amount       NUMERIC(14,2) NOT NULL,
  line_items             JSONB NOT NULL DEFAULT '[]'::jsonb,  -- string-serialized decimals
  created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (booking_id, document_type, revision)
);
-- Exactly one active document per type per booking
CREATE UNIQUE INDEX uq_tax_document_active ON tax_document (booking_id, document_type) WHERE status = 'issued';
CREATE INDEX idx_tax_document_booking   ON tax_document (booking_id);
CREATE INDEX idx_tax_document_recipient ON tax_document (recipient_id);
```

**Queries** (`db_core/src/tax_document.rs` [NEW], all `sqlx::query!`/`query_as!`):
- `insert_tax_document(tx, &NewTaxDocument)`
- `supersede_active(tx, booking_id, doc_type) -> Option<TaxDocumentRow>` (`UPDATE … SET status='superseded' WHERE status='issued' RETURNING *`)
- `list_for_booking(pool, booking_id)`
- `get_by_number(pool, booking_id, number)`

**Exchange rates** (`db_core/src/exchange_rate.rs`): `get_rate(executor, base, target, at)` per §1.3.

**Hooks**: these go in `db_core::booking::update_booking`, inside the existing transaction, following the payout-ledger block at [booking.rs#L267](file:///home/pav/code/our_places_rs-feat-60-automated-gct-tax-invoicing-receipt-generator/db_core/src/booking.rs#L267-L276).

| Transition (status_changed) | Hook |
| :--- | :--- |
| `→ Confirmed` | `tax_document::issue_payment_receipt(&mut tx, &booking, &supplier)` |
| dates changed while `Confirmed` | `supersede_active(PaymentReceipt)` then `issue_payment_receipt` (revision n+1) |
| `→ Completed` (only from `Confirmed`) | `issue_tax_invoice` |
| `→ Cancelled` (from `Confirmed`) | `issue_refund_receipt` using `common::cancellation::quote_refund` |

- `update_booking` gains a `supplier: &SupplierInfo` parameter, because db_core must not read env vars.
- `UpdatedBooking` gains `date_from`, `date_to`, and `pricing: Option<BookingPricingSnapshot>`.
- Invalid transitions (e.g. `Cancelled → Completed`) ⇒ `Err(InvalidTransition)`.

### 2.2. `common`
| File | Change |
| :--- | :--- |
| `reference.rs` | `DEFAULT_TAX_COUNTRY`, `tax_rate_for()` |
| `pricing.rs` | `apply_taxes(tax_rate)`, `with_fees(cleaning, deposit)`, `platform_fee` field, reconciliation per §1.2 |
| `cancellation.rs` [NEW] | §1.4 typestate, `RefundQuote`, `CancellationError` (thiserror) |
| `tax_document.rs` [NEW] | `TaxDocumentType`, `TaxDocumentStatus`, `InvoiceTaxCategory { Gct, Exempt }`, `InvoiceLineItem`, `TaxDocumentResponse`, `document_number(type, year, code, revision)` |
| `models.rs` | `FeeItem.amount` string serde; listing DTOs gain `cleaning_fee`, `damage_deposit`; `BookingResponse` gains fee/tax/fx fields; `BookingMetadataResponse` gains B2B fields; `UpdateBookingRequest` gains `date_from`, `date_to` |
| `email.rs` | `EmailTemplate::BookingCompletedGuest` + `BookingCompletedGuestPayload`; confirmation payload gains GCT breakdown and `receipt_url` |

**FeeItem serialization (no floats in JSONB):**
```rust
pub struct FeeItem {
    pub name: String,
    /// Serialized as a JSON string. Deserializes from string *or* legacy number.
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    pub amount: Decimal,
}
```
- Deserialization uses `Decimal`'s default `Deserialize`, which accepts both strings and numbers, so existing JSONB rows still load.
- `InvoiceLineItem` money fields use `#[serde(with = "rust_decimal::serde::str")]`.
- Both shapes need tests. API consumers of `fee_breakdown[].amount` change from number to string, so update the OpenAPI specs and `playwright/e2e/setup/db-seed.ts`.

### 2.3. `app_api/listing_api`
- Create/update payloads and responses carry `cleaning_fee` / `damage_deposit` (`Option<Decimal>`). Validation: `0 ≤ x ≤ 1,000,000` and 2dp, else 400 `INVALID_FEE`.
- `db_core/src/listing.rs`: add both columns to the insert, update (`COALESCE`), and every listing `SELECT` (~6 queries) plus the `listing_history` snapshot. Update `api_core/src/models.rs` mappers.
- Quote endpoint returns `cleaning_fee`, `damage_deposit`, `platform_fee`, `tax_rate`, `tax_value`, `total_price`.

### 2.4. `app_api/booking_api`
| Method | Route | Auth | Response |
| :--- | :--- | :--- | :--- |
| `PATCH` | `/api/v1/bookings/{id}` | guest owner, admin | `BookingResponse` (now accepts `date_from`/`date_to`) |
| `GET` | `/api/v1/bookings/{id}/cancellation-quote` | guest owner, host, admin | `RefundQuote` (not persisted) |
| `GET` | `/api/v1/bookings/{id}/documents` | guest owner, host, admin | `Vec<TaxDocumentResponse>` (incl. superseded, flagged) |
| `GET` | `/api/v1/bookings/{id}/documents/{number}` | guest owner, host, admin | `TaxDocumentResponse` |
| `PATCH` | `/api/v1/internal/cron/complete-stays` | `x-cron-secret` | `{ completed, failed }` |

- `settings.rs`: `TaxSettings`, `cron_secret`. Both mandatory.
- Notification dispatch:
  - **Cancel**: `refund_amount` comes from the issued `REF` document, not `total_price`.
  - **Date change**: the `booking_updated_guest` payload gains a `receipt_url`.
  - **Completed**: send `booking_completed_guest` with an `invoice_url`.
  - The cron endpoint reuses the same dispatch function.
- Document number in links; unauthorized ⇒ 404.

### 2.5. `app_api/email_worker`
- `booking_confirmation_guest`: itemized table (accommodation, cleaning, GCT @ rate, platform fee, deposit held, total), "View Payment Receipt" → `{BASE_URL}/bookings/{id}/documents/{number}`.
- `booking_cancelled_guest`: refund %, refund total, retained total.
- `booking_updated_guest`: "View Revised Receipt" link and balance due/refundable.
- `booking_completed_guest` [NEW]: thank-you text, "Download Tax Invoice" link, deposit-released note.

### 2.6. Frontends (Topcoat SSR)
- `web_app_common_tc/src/components/tax_document_view.rs` [NEW]:
  - `view!` component rendering a `TaxDocumentResponse` with title, supplier/TRN, recipient/B2B, a line-item table, totals, the JMD equivalents, a "Superseded" watermark, and a revised-receipt balance block.
  - Print styles via `@media print` (A4, hides `.no-print`).
  - All text goes through `view!` escaping. No raw HTML.
- `web_app_tc`:
  - `/bookings/{id}` lists documents.
  - `/bookings/{id}/documents/{number}` is a printable page.
  - The cancel modal fetches `cancellation-quote` (HTMX) and shows the refund before confirming.
  - A date-change form shows a re-quote and the balance due.
  - Checkout captures optional B2B fields when "business trip" is checked.
  - The listing detail breakdown shows cleaning fee and deposit.
- `web_app_admin_tc`:
  - The listing create/edit form gets "Charge cleaning fee" and "Require damage deposit" toggles with amounts (off ⇒ 0).
  - The booking modal gets a documents panel (HTMX fragment) that replaces the client-side `openAdminInvoiceDialog` mock.

### 2.7. `infra` — Terraform
Add to `infra/modules/scheduler/main.tf`:
```hcl
resource "google_cloud_scheduler_job" "complete_stays" {
  name             = "${var.env_prefix}-complete-stays"
  description      = "Daily transition of confirmed bookings past check-out to completed (issues tax invoices)"
  project          = var.project_id
  region           = var.region
  schedule         = "0 12 * * *"
  time_zone        = "America/Jamaica"
  attempt_deadline = "320s"

  retry_config { retry_count = 3 }

  http_target {
    http_method = "PATCH"
    uri         = "${var.booking_api_url}/api/v1/internal/cron/complete-stays"
    headers = {
      "Content-Type"  = "application/json"
      "x-cron-secret" = var.cron_secret
    }
  }

  depends_on = [google_project_service.cloud_scheduler_api]
}
```
Verify with `terraform fmt -check` and `terraform validate`.

> [!NOTE]
> This keeps the module's existing `x-cron-secret` pattern. The secret ends up in Terraform state. Moving both jobs to `oidc_token` (Cloud Run invoker service account) is a recommended follow-up.

---

## 3. Performance Considerations
- Document reads: a single indexed query, with line items in JSONB (no N+1).
- `complete-stays`: a bounded batch (500) of short per-booking transactions, so no long-held locks. Daily volume for 5 villas is trivial.
- Date change: two row locks plus one availability query, the same cost profile as `create_booking`.
- There's no HTML rendering in the APIs. SSR rendering uses existing Topcoat paths. No new heavy dependencies, so the cold-start budget is unaffected.

---

## 4. Security Mitigations
| Risk | Mitigation |
| :--- | :--- |
| IDOR on documents, quotes, date changes | Guest owner, listing host (`listing.user_id`), or admin. Otherwise 404. Date changes are guest owner or admin only. |
| Cron endpoint abuse | Constant-time comparison of `x-cron-secret`, mandatory setting (no fallback), endpoint idempotent. |
| XSS in documents | No API HTML. Topcoat `view!` escaping. B2B fields length-limited and trimmed. |
| Fee tampering | Fees come only from the listing row at pricing time. Client-supplied amounts are ignored. Validation 0–1,000,000. |
| Price manipulation via date change | Server-side re-pricing under row locks. Client dates only. |
| Document tampering | No update/delete endpoints. The only mutation is `issued → superseded` inside the hook transaction. |
| SQL injection | `sqlx::query!` everywhere. |
| PII | TRNs never logged above DEBUG. |

---

## 5. Test Plan

### 5.1. Unit (`common`)
- `tax_rate_for` returns JM for `"JM"`, unknown codes, and empty strings.
- `BookingCalculator` reconciliation: a property test (random rates/nights/fees) asserts the §1.2 invariant, and that the receipt line items sum to `total_price`.
- Fees off ⇒ `cleaning_fee = damage_deposit = 0` and the total matches the pre-feature formula with JM rate.
- `RefundCalculator<Firm>` / `<Strict>`: table-driven over days-out boundaries 0, 6, 7, 13, 14, 29, 30, 31; Strict grace at 47h/49h × booked 13/14 days ahead; `cancelled_at ≥ check_in` ⇒ Err.
- Refund components sum to `refund_total`, and `refund_total + retained_total = total_price`.
- `FeeItem` serializes the amount as a string, and deserializes legacy number and new string JSON.
- `document_number` formats: `REC-2026-HMEJYC3P9B`, `REC-2026-HMEJYC3P9B-R2`, `INV-…`, `REF-…`.

### 5.2. Integration (`db_core`, `listing_api`, `booking_api`)
- The migration applies to a DB containing pre-feature bookings, and the platform fee backfills correctly. The existing pg_cron hold-expiry SQL still inserts into `booking_history`.
- Listing create/update/read round-trips `cleaning_fee`/`damage_deposit`, and a negative value is rejected with 400.
- `→ Confirmed` issues exactly one `REC`, and a second identical PATCH doesn't duplicate it.
- A date change issues `REC-R2` and supersedes R1, with `balance_due` correct. An overlapping date ⇒ 409 `DATE_UNAVAILABLE`, with no document issued.
- `→ Cancelled` under Firm 10 days out issues a `REF` with 50% accommodation, and the email payload refund matches.
- `complete-stays`: completes eligible bookings and skips future/cancelled ones. Running it twice gives `completed = 0` the second time. A wrong secret gives 401.
- `→ Completed` issues an `INV` with `total_amount = total_price − damage_deposit`, plus the review token (existing hook still fires).
- FX: USD booking ⇒ JMD amounts use rate 160. GBP→JMD direct. JMD→GBP inverse. EUR ⇒ 422.
- Authorization matrix for documents, quotes, and date-change endpoints (guest A/B, host, admin, anonymous).
- Typed settings fail without `TAJ_SUPPLIER_TRN` or `CRON_SECRET`.

### 5.3. E2E (Playwright)
- **Guest:** book → confirm → open receipt (print button visible, totals match) → change dates → revised receipt → cancel with quote shown → refund receipt.
- **Admin:** enable cleaning fee and deposit on a listing → guest quote reflects them → booking modal documents panel lists `REC`/`INV`.
- **Completion:** seed a past-checkout confirmed booking → call the cron endpoint → guest sees "Download Tax Invoice".

### 5.4. Infra
- `terraform fmt -check && terraform validate` in `infra/`.

---

## 6. Acceptance Criteria
- [ ] One tax calculator, using the country-resolved rate (default JM), drives quotes, bookings, date changes, and documents.
- [ ] Hosts can enable cleaning fee and damage deposit; disabled ⇒ 0.
- [ ] The reconciliation invariant holds and is property-tested.
- [ ] REC/INV/REF documents are issued from `db_core::booking::update_booking` hooks; date changes produce superseding revisions.
- [ ] All 5 refund policies (Flexible, Moderate, Strict, Firm, Limited) use the typestate calculator with partial refund component rules, and the cancellation email shows the correct refund.
- [ ] Cloud Scheduler Terraform job completes stays daily via the PATCH cron endpoint.
- [ ] JMD equivalents are recorded with explicit FX failure handling.
- [ ] APIs are JSON-only; documents render via the `web_app_common_tc` SSR component.
- [ ] No floats in JSONB (`FeeItem`, `line_items`).
- [ ] The `booking_completed_guest` email exists; there are no credit notes and no `/admin/documents` page.
- [ ] `cargo test --workspace`, `clippy -D warnings`, and `sqlx prepare --check` are green; no new `unwrap`/`expect` in production paths.
