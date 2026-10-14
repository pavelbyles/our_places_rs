# Intent: Automated GCT Tax Invoicing & Receipt Generator

## Metadata
- **Author**: Originator (via GitHub Issue #60)
- **Date**: 2026-10-13 (rev. 2 — post-evaluation)
- **Status**: Under Review
- **Target Area**: Fullstack (`db_core`, `common`, `app_api/booking_api`, `app_api/listing_api`, `app_api/email_worker`, `web_app_common_tc`, `web_app_tc`, `web_app_admin_tc`, `infra`)

---

## 1. Problem Statement & Motivation
Bookings record financial aggregates (`sub_total_price`, `discount_value`, `tax_value`, `fee_breakdown`, `total_price`) but the platform issues no compliance-grade receipts or tax invoices. Evaluation of the codebase surfaced additional defects that block trustworthy documents:

- **Two disagreeing tax rates**: `BookingCalculator::apply_taxes` hardcodes 10%; `common/src/reference.rs` declares 15% for Jamaica. Documents would contradict what guests were charged.
- **No cleaning fee / damage deposit**: Hosts cannot configure them, so they cannot be itemized.
- **Cancellations always refund 100%**: `booking_api` sets `refund_amount = total_price` regardless of policy.
- **No date changes**: `db_core::booking::update_booking` only updates status, metadata and door code.
- **Nothing completes stays**: `Completed` is only reachable by manual PATCH.

Consequences: Tax Administration Jamaica (TAJ) audit exposure, corporate guests unable to expense stays, and manual admin reconciliation.

---

## 2. Proposed Outcome (The Vision)
1. **Single tax source of truth**: GCT rate resolved from `SupportedCountry::LIST` by listing country, defaulting to **JM**. One calculator, used for quotes, bookings, and documents.
2. **Host-configurable fees**: Hosts optionally enable a **cleaning fee** and a **refundable damage deposit** per listing. Disabled ⇒ `0.00`.
3. **Reconciling totals**: Every document's line items sum exactly to its stated totals, and the receipt total equals `booking.total_price`.
4. **Dual Document Model**:
   - **Payment Receipt** (`REC-YYYY-{code}`) on `Confirmed`; re-issued as a revision (`-R2`, `-R3`…) when dates change.
   - **Statutory Tax Invoice** (`INV-YYYY-{code}`) on `Completed`.
   - **Refund Receipt** (`REF-YYYY-{code}`) on cancellation, computed from the booking's cancellation policy.
5. **Policy-driven partial refunds**: **Firm** and **Strict** cancellation policies, modelled with a typestate refund calculator.
6. **Automatic stay completion**: A Cloud Scheduler job (Terraform) calls an internal booking_api endpoint that transitions eligible stays to `Completed` via the standard update path.
7. **Cross-currency support**: Documents snapshot both the base→payment rate and the payment→JMD rate so JMD statutory equivalents are always recorded.
8. **Clean layering**: APIs return JSON only. HTML is rendered exclusively by Topcoat SSR frontends.

---

## 3. Scope Boundaries

### In Scope (MVP)
- **Tax**: `apply_taxes` takes the resolved country rate (default JM 15%). The rate is snapshotted on each booking.
- **Listing fees**: `listing.cleaning_fee`, `listing.damage_deposit` (+ `listing_history`); listing_api create/update/read; admin listing form toggles + amounts; guest quote shows fees.
- **Booking financial columns**: `cleaning_fee`, `damage_deposit`, `platform_fee`, `tax_rate`, `exchange_rate` on `booking` (+ `booking_history`), satisfying the reconciliation invariant.
- **Tax documents**: `tax_document` table supporting `payment_receipt`, `tax_invoice`, `refund_receipt` with revisions/superseding. Generated in `db_core::booking::update_booking` (payout-ledger pattern).
- **Cancellation policies**: add `firm` to `cancellation_policy`; implement Firm and Strict refund schedules (see §5); a refund quote endpoint; refund receipt; corrected cancellation email amounts.
- **Date changes**: PATCH supports `date_from`/`date_to` on confirmed bookings with availability lock, re-pricing, and a revised receipt.
- **Stay completion**: `PATCH /api/v1/internal/cron/complete-stays` (cron-secret protected) + Terraform `google_cloud_scheduler_job`.
- **FX**: payment→JMD rate lookup (direct pair, else inverse); fail fast if unavailable.
- **FeeItem serialization**: monetary fields serialized as strings (no floats in JSONB), backward-compatible deserialization of existing rows.
- **Emails**: GCT breakdown + receipt link in `booking_confirmation_guest`; correct refund in `booking_cancelled_guest`; revised-receipt link in `booking_updated_guest`; new **`booking_completed_guest`** template with tax invoice link.
- **Frontends**: shared `TaxDocumentView` component (`web_app_common_tc`) with print styles; guest receipt/invoice/refund pages, cancel-with-quote modal, date change form (`web_app_tc`); documents panel in admin booking modal (`web_app_admin_tc`).
- **B2B**: optional `business_company_name` / `business_tax_id` in `BookingMetadata`, printed on documents.

### Out of Scope / Non-Goals
- Credit notes.
- `/admin/documents` audit page.
- Damage deposit claims/deductions (deposit is always released on completion or cancellation in MVP).
- Payment gateway capture/refund execution of price deltas (documents record amounts due/refundable. Money movement remains manual).
- Live electronic filing with TAJ.
- Backfilling documents for bookings that predate this release.

---

## 4. Affected Systems & Stakeholders
- **Personas**: Guests (receipts, refunds, invoices), Hosts (fee configuration, retained amounts), Admins (reconciliation).
- **Components**: `db_core` (single feature migration, queries, hooks), `common` (pricing, cancellation typestate, DTOs), `booking_api` (endpoints, cron), `listing_api` (fee fields), `email_worker` (templates), `web_app_common_tc` / `web_app_tc` / `web_app_admin_tc` (UI), `infra/modules/scheduler` (Terraform).

---

## 5. Constraints & Non-Negotiable Invariants
- **Zero floats** in money/rates (including JSONB). `rust_decimal::Decimal` only.
- **Reconciliation invariant**:
  `total_price = (sub_total_price − discount_value) + cleaning_fee + tax_value + platform_fee + damage_deposit`
  `tax_value = round_bankers((sub_total_price − discount_value + cleaning_fee) × tax_rate, 2)`
- **Refund schedules** (days = check-in date − cancellation date, America/Jamaica):

| Timing | Firm | Strict |
| :--- | :--- | :--- |
| ≥ 30 days out | 100% | 100% only within 48h of booking *and* booked ≥ 14 days out; else 50% |
| 30 → 14 days out | 100% | 50% |
| 14 → 7 days out | 50% | 50% |
| < 7 days out | 0% | 0% |

- **Zero panics**, monadic error propagation, `#[instrument]` on async fns.
- **Row-level locking** (`SELECT … FOR UPDATE`) for date changes and stay completion.
- **Immutability**: issued documents are never updated except `status → superseded` when a revision is issued.
- **JSON-only APIs**. HTML only in Topcoat SSR.
- **Cloud Run budgets**: <300ms p50 cold start, 256MB.
- **Secrets**: `TAJ_SUPPLIER_TRN` and `CRON_SECRET` mandatory via typed settings (no fallback defaults).

---

## 6. Success Metrics & Acceptance Signals
- Property test: for any booking, document line items sum to document totals, and the receipt total equals `booking.total_price`.
- Refund calculator tests cover every boundary in §5 for both policies.
- Stays whose check-out has passed are `Completed` with an `INV-…` within 24h, without manual action.
- `cargo test --workspace`, `cargo clippy -D warnings`, `cargo sqlx prepare --check` all green.

---

## 7. Open Questions & Policy Concerns
- [x] **Tax rate**: Resolve from listing country, default JM (`SupportedCountry::LIST`).
- [x] **Tax base**: Accommodation (net of discounts) + cleaning fee. Damage deposit and platform fee are not taxed.
- [x] **Invoice numbering**: `{REC|INV|REF}-YYYY-{confirmation_code}[-R{n}]`.
- [x] **Merchant of record**: Our Places Luxury Villas Ltd; mandatory `TAJ_SUPPLIER_TRN`.
- [x] **Completion trigger**: Cloud Scheduler → internal cron endpoint → `update_booking` (fires all Completed hooks).
- [ ] **Flexible / Moderate policies**: Keep with current (100%) refund behavior, map to Firm, or remove?
- [ ] **Strict ≥ 30 days outside the 48h grace window**: Confirm 50%.
- [ ] **Refund components**: Recommended: damage deposit and cleaning fee always 100% refunded; policy % applies to accommodation; GCT refunded pro rata on refunded taxable amount; platform fee refunded only on 100% refunds.
- [ ] **Day boundaries**: Recommended: inclusive lower bound (exactly 30 days out ⇒ "≥ 30" bucket), America/Jamaica timezone.
- [ ] **Accountant confirmation**: GCT rate for villa accommodation, GCT on retained cancellation amounts, and whether a payment receipt must itself be a tax invoice.
