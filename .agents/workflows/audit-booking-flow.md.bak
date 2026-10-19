---
description: Domain & financial guardrails audit — verify tri-currency math, statutory tax precision, PostgreSQL row-level locks, 15-minute hold timeouts, and status history tracking.
---

# /audit-booking-flow — Booking & Financial Logic Audit

## Goal
Stress-test and audit booking engine modifications against the project's non-negotiable domain rules, concurrency guarantees, and financial precision requirements.

## When to Use
Run `/audit-booking-flow` whenever modifying code in:
- `common/src/pricing.rs` or currency conversion modules
- `app_api/booking_api/`
- `db_core/src/queries/bookings.rs` or booking migrations

---

## Audit Checklist

### 1. Financial Precision & Floating-Point Ban
> [!CAUTION]
> **NEVER use `f32` or `f64` for money, rates, or taxes.** Always use `rust_decimal::Decimal`.

- Scan the workspace for illegal float usage in monetary contexts:
  ```bash
  rg --type rust "f32|f64" common/ app_api/booking_api/ db_core/
  ```
- Verify that currency conversions strictly follow the Tri-Currency Flow:
  $$\text{Base Currency (Villa Price)} \longrightarrow \text{Payment Currency (Checkout)} \longrightarrow \text{Taxes \& Totals}$$
- Verify that statutory taxes (e.g. Jamaican GCT 15%) are sourced from static reference constants in `common/src/reference.rs`, while exchange rates are queried from database tables.

### 2. Concurrency & Zero Double-Booking Guarantee
- Verify that all availability checks and reservation hold creations utilize PostgreSQL row-level locks:
  ```sql
  SELECT ... FROM property_availability WHERE ... FOR UPDATE;
  ```
- Ensure locks are held within an active SQL transaction and never bypassed in favor of caching layers.

### 3. Reservation Hold Lifecycle & Shadow Users
- Check that new checkout sessions initialize with a `pending_payment` status and a strict 15-minute `expires_at` window.
- Verify shadow user creation: guest checkouts must attach to the 15-minute hold and seamlessly promote to full users without releasing date holds.
- Verify status transitions are immutably appended to `booking_status_history`.

### 4. Automated Verification
Run pure unit tests in `common` and database integration tests:
```bash
cargo test -p common
cargo test -p booking_api
```
