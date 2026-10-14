# Engineering Plan #60: Automated GCT Tax Invoicing & Receipt Generator (rev. 2)

## Metadata
- **Spec**: [spec-60 rev. 2](../specs/spec-60-automated-gct-tax-invoicing-receipt-generator.md)
- **Branch**: `feat/60-automated-gct-tax-invoicing-receipt-generator` (Base: `main`)
- **Status**: Draft (pending open questions in intent §7)
- **Blast radius**: ~30 files across 9 crates + `infra`. Exceeds the 5-file threshold, so it is split into 8 slices.

---

## 1. Data Flows

### 1.1. Checkout → Confirmation
```
Guest ─POST /api/v1/bookings─► booking_api
   1. Resolve listing, tax_rate_for(listing.country) (default JM), base→payment FX
   2. BookingCalculator: BaseRate → Discounted → with_fees → apply_taxes(rate) → finalize
   3. db_core::create_booking (listing FOR UPDATE, availability check)
        INSERT booking (status: 'pending', 15-min hold) + booking_history
   ◄─ 201 BookingResponse
             │  (payment)                        (pg_cron: unpaid > 15 min ⇒ 'cancelled', no documents)
             ▼
PATCH /api/v1/bookings/{id} {status: confirmed} ─► db_core::update_booking (one TX)
   booking FOR UPDATE → UPDATE status → booking_history
   ├─ hook: host_payout_ledger (existing)
   └─ hook: tax_document REC-YYYY-{code}  (payment→JMD FX snapshot)
   COMMIT → notification dispatch → booking_confirmation_guest (GCT table + receipt link)
```

### 1.2. Date Change (confirmed only)
```
PATCH /api/v1/bookings/{id} {date_from, date_to}
   booking_api: re-quote (same calculator) → BookingPricingSnapshot
   db_core::update_booking (one TX):
      listing FOR UPDATE + booking FOR UPDATE → availability (excl. self) ─✗─► 409 DATE_UNAVAILABLE
      UPDATE dates + financial columns → booking_history
      supersede active REC → issue REC-…-R{n} (amount_previously_paid, balance_due)
   COMMIT → booking_updated_guest (revised receipt link)
```

### 1.3. Cancellation
```
GET  /bookings/{id}/cancellation-quote ─► quote_refund(policy, amounts, now)  (read-only, shown in modal)
PATCH /bookings/{id} {status: cancelled} ─► update_booking TX:
      quote_refund → issue REF-YYYY-{code} (refund_pct, refund lines, retained_amount)
   COMMIT → booking_cancelled_guest (refund from REF document)
```

### 1.4. Completion
```
Cloud Scheduler (12:00 America/Jamaica) ─PATCH /api/v1/internal/cron/complete-stays (x-cron-secret)─►
   SELECT confirmed WHERE date_to <= today_jm LIMIT 500
   for each id: update_booking(Completed)  ── TX: review token (existing) + INV-YYYY-{code}
   dispatch booking_completed_guest (invoice link)
   ◄─ { completed, failed }
```

### 1.5. Document State Machine
```
REC r1 (issued) ──date change──► REC r1 (superseded) + REC r2 (issued) ──► … r{n}
      │
      ├──► completed ──► INV (issued)          [terminal]
      └──► cancelled ──► REF (issued)          [terminal; no INV ever]
Invariant: ≤ 1 issued document per (booking, type)  — partial unique index
```

---

## 2. Edge Cases
| Case | Handling |
| :--- | :--- |
| Listing country not in `SupportedCountry::LIST` | Falls back to JM rate. Logged at `warn`. |
| Fees disabled | `0.00`. Lines omitted from documents (not shown as zero). |
| Payment→JMD rate missing | 422 `FX_RATE_UNAVAILABLE`; transaction rolled back, so the booking isn't confirmed without a receipt. |
| Duplicate confirm PATCH | `status_changed = false` ⇒ hook skipped. Partial unique index as a backstop. |
| Date change overlaps another booking | 409, no state change. |
| Date change to past / on non-confirmed booking | 409 `BOOKING_NOT_MODIFIABLE`. |
| Cancel at or after check-in | `CancellationError::AfterCheckIn` ⇒ 409. |
| Cron overlap / retry | Per-booking `FOR UPDATE` + status re-check ⇒ idempotent. |
| Legacy bookings (pre-feature) | `tax_rate` backfilled to 0.10, fees 0, platform fee extracted from `fee_breakdown`. Documents are only issued on future transitions. |
| Legacy `fee_breakdown` numeric JSON | Default `Decimal` deserializer accepts numbers. Tested. |
| pg_cron hold-expiry inserts into `booking_history` | New history columns nullable. Tested. |

---

## 3. On Vertical Slicing with DB → common → API → UI

**Problem**: Almost every capability crosses all four layers, and AGENTS.md asks for **one migration per branch**, which pushes toward a big schema-first task.

**Approach: contract-first, then capability slices.**
1. **Slice 0 lands only the shared contracts**: the feature migration (purely additive, all defaults), the DTO/enum types, and the `FeeItem` serde change. It's thin, has no behavior, and is verifiable with `sqlx migrate run` + `cargo check` + serde tests. It freezes the interfaces the parallel slices depend on.
2. **Every later slice is one user-visible capability that cuts through all layers**: the queries it needs → domain logic → endpoint → UI → tests, and it's shippable/demoable on its own. Each layer's change within a slice is small because the contract already exists.
3. **Tracer-bullet ordering**: the riskiest end-to-end path (receipt on confirm, slice 3) goes first after the pricing foundations, so integration problems surface early.
4. **Parallelism**: after slice 2, slices 4, 5 and 6 only share the document writer from slice 3, so they can run in parallel worktrees.

> If schema discovery changes during a slice, amend the **unmerged** feature migration and re-run `sqlx migrate revert && sqlx migrate run` locally. The migration is immutable only once applied on `main`.

```
S0 Contracts ─► S1 Tax source ─► S2 Listing fees + reconciled pricing ─► S3 Receipt on confirm (tracer)
                                                                              │
                                                  ┌───────────────────────────┼───────────────────────┐
                                                  ▼                           ▼                       ▼
                                          S4 Refund policies          S5 Date changes          S6 Completion + Terraform
                                                  └───────────────────────────┼───────────────────────┘
                                                                              ▼
                                                                    S7 Admin panel + E2E
```

---

## 4. Slices

### S0 — Shared contracts (S)
- **Files**:
  - `db_core/migrations/20261013000000_gct_tax_documents.sql` [NEW]
  - `db_core/src/models.rs` (`CancellationPolicy::Firm`, `TaxDocumentType/Status` sqlx types)
  - `common/src/tax_document.rs` [NEW] (DTOs, `document_number`)
  - `common/src/models.rs` (`FeeItem` string serde, new optional fields)
- **Accept**:
  - Migration applies on a seeded DB.
  - Legacy and new `FeeItem` JSON both deserialize.
  - `document_number` tests pass.
  - pg_cron insert SQL still valid.
- **Verify**: `sqlx migrate run && SQLX_OFFLINE=false cargo check --workspace && cargo test -p common -- fee_item document_number`

### S1 — Single tax source, JM default (S)
- **Files**:
  - `common/src/reference.rs`
  - `common/src/pricing.rs` (`apply_taxes(rate)`)
  - `app_api/booking_api/src/apis.rs` (create_booking passes rate, snapshots `tax_rate`/`exchange_rate`)
  - Listing quote handler
- **Accept**:
  - Quote and booking show 15% for JM; unknown country ⇒ 15%.
  - `booking.tax_rate` persisted.
  - Existing pricing tests updated from 10% → 15%.
- **Verify**: `cargo test -p common pricing && cargo test -p booking_api`

### S2 — Listing fees + reconciled pricing (M)
- **Files**:
  - `db_core/src/listing.rs`
  - `api_core/src/models.rs`
  - listing_api handlers
  - `common/src/pricing.rs` (`with_fees`, `platform_fee`, invariant)
  - `web_app_admin_tc/src/pages/listings_admin.rs` (toggles)
  - `web_app_common_tc/src/components/price_breakdown.rs`
- **Accept**:
  - Host can enable and set fees; off ⇒ 0.
  - Quote, booking and breakdown show the fees.
  - Property test for the reconciliation invariant passes.
  - Negative fee ⇒ 400.
- **Verify**: `cargo test -p common -- reconciliation && cargo test -p listing_api && cargo test -p web_app_admin_tc`

### S3 — Payment receipt on confirm (tracer bullet) (M)
- **Files**:
  - `db_core/src/exchange_rate.rs`
  - `db_core/src/tax_document.rs` [NEW]
  - `db_core/src/booking.rs` (Confirmed hook, `SupplierInfo` param)
  - `booking_api` settings (`TaxSettings`, `cron_secret`) + document endpoints
  - `web_app_common_tc/.../tax_document_view.rs` [NEW]
  - `web_app_tc` document page + booking detail list
  - B2B checkout fields
  - `booking_confirmation_guest` template
- **Accept**:
  - Confirm issues exactly one REC whose total equals `booking.total_price`.
  - JMD equivalents correct; EUR ⇒ 422 with rollback.
  - Auth matrix enforced.
  - Printable SSR page.
  - Email links to the receipt.
  - Startup fails without TRN.
- **Verify**: `cargo test -p db_core tax_document && cargo test -p booking_api documents && cargo test -p web_app_tc`

### S4 — Firm/Strict refund policies (M)
- **Files**:
  - `common/src/cancellation.rs` [NEW]
  - `db_core/src/booking.rs` (Cancelled hook → REF)
  - `booking_api` (`cancellation-quote` endpoint, cancel email uses REF)
  - `web_app_tc` cancel modal (HTMX quote)
  - `booking_cancelled_guest` template
  - Checkout policy selector includes Firm
- **Accept**:
  - Boundary table tests pass for both policies.
  - `refund_total + retained_total = total_price`.
  - REF issued on cancel; no INV afterwards.
  - Email refund equals the REF amount.
- **Verify**: `cargo test -p common cancellation && cargo test -p booking_api cancel`

### S5 — Date changes (M)
- **Files**:
  - `db_core/src/booking.rs` (`UpdatedBooking` dates/pricing, locks, availability, REC revision)
  - `common/src/models.rs` (`UpdateBookingRequest`)
  - `booking_api` PATCH re-quote
  - `web_app_tc` date-change form
  - `booking_updated_guest` template
- **Accept**:
  - Change ⇒ REC-R2 issued, R1 superseded, `balance_due` correct.
  - Overlap ⇒ 409, nothing written.
  - Non-confirmed ⇒ 409.
  - Concurrent change vs. new booking: one wins.
- **Verify**: `cargo test -p db_core date_change && cargo test -p booking_api date_change`

### S6 — Automatic completion + invoice + Terraform (M)
- **Files**:
  - `booking_api` (`complete-stays` PATCH, dispatch `booking_completed_guest`)
  - `db_core/src/booking.rs` (Completed hook → INV, transition guard)
  - `common/src/email.rs` (`BookingCompletedGuest`)
  - `email_worker` template
  - `web_app_tc` invoice link
  - `infra/modules/scheduler/main.tf`
- **Accept**:
  - Eligible bookings completed; second run reports `completed = 0`.
  - INV total = `total_price − damage_deposit`.
  - Review token still issued.
  - Wrong secret ⇒ 401.
  - Completion email sent.
  - Terraform validates.
- **Verify**: `cargo test -p booking_api complete_stays && cargo test -p email_worker && (cd infra && terraform fmt -check && terraform validate)`

### S7 — Admin documents panel + E2E hardening (S)
- **Files**:
  - `web_app_admin_tc/src/pages/bookings_admin.rs` (HTMX documents panel; remove the `openAdminInvoiceDialog` mock)
  - `playwright/e2e/guest-portal/tax-documents.spec.ts` [NEW]
  - `playwright/e2e/admin-portal/listing-fees-and-documents.spec.ts` [NEW]
  - `playwright/e2e/setup/db-seed.ts` (string fee amounts)
  - `openapi*.yaml`
- **Accept**:
  - Admin sees REC/INV/REF per booking.
  - Spec §5.3 E2E flows pass.
  - Full sanity suite green.
- **Verify**:
  - `cargo test --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo sqlx prepare --workspace --check`
  - `npx playwright test`

---

## 5. Test Matrix
| Scenario | Layer | Slice | P |
| :--- | :--- | :---: | :---: |
| Legacy/new FeeItem JSON | Unit | S0 | P0 |
| Migration on seeded DB + pg_cron insert | Integration | S0 | P0 |
| JM default rate | Unit | S1 | P0 |
| Reconciliation property test | Unit | S2 | P0 |
| Fee validation | Integration | S2 | P1 |
| REC issued once, totals match | Integration | S3 | P0 |
| FX direct/inverse/missing | Integration | S3 | P0 |
| Document auth matrix | Integration | S3 | P0 |
| Firm/Strict boundaries | Unit | S4 | P0 |
| REF on cancel, email amount | Integration | S4 | P0 |
| Date change revision + overlap 409 | Integration | S5 | P0 |
| Concurrent date change | Integration | S5 | P1 |
| complete-stays idempotency + auth | Integration | S6 | P0 |
| Terraform validate | Infra | S6 | P1 |
| Guest/admin E2E | E2E | S7 | P1 |

---

## 6. Review Verdict
| Dimension | Score | Notes |
| :--- | :---: | :--- |
| Architecture clarity | 8 | Hooks follow the payout-ledger pattern; JSON-only APIs. The `update_booking` signature grows (`SupplierInfo`, pricing). Watch its complexity. |
| Error handling | 8 | Explicit codes (`FX_RATE_UNAVAILABLE`, `DATE_UNAVAILABLE`, `BOOKING_NOT_MODIFIABLE`). Cron isolates per-booking failures. |
| Test coverage | 8 | Boundary-driven refund tests, reconciliation property test, idempotency tests. |
| Security | 8 | Auth matrix, server-side pricing, mandatory secrets. The cron secret in TF state is a known follow-up (OIDC). |
| Performance | 9 | Bounded batches, single-query reads, no new heavy deps. |

**Blocking before S4/S5 start**: resolve intent §7 open questions (Flexible/Moderate mapping, Strict ≥30-day rule, refund components, day boundaries). S0–S3 can proceed now.
