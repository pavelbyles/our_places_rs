# Spec 56: Refactor Pricing into its Own Module

## Overview
Currently, property pricing and checkout financial calculations are implemented independently across backend services and the WebAssembly frontend:
- **Backend (`app_api/booking_api/src/apis.rs`)**: Implements a `BookingCalculator` struct using a compile-time safe typestate pattern (`BaseRate` -> `Discounted` -> `Taxed`) to compute nightly sub-totals, discounts, taxes, and platform fees.
- **Frontend (`web_app/src/components/checkout.rs`)**: Contains duplicated, hardcoded arithmetic (e.g. manually applying a 10% tax multiplier via `Decimal::new(1, 1)`).

This divergence violates the core monorepo architecture principle of **Isomorphic Domain Logic** outlined in `docs/ARCHITECTURE.md`. This specification details the extraction and centralization of the pricing engine into a shared, zero-dependency isomorphic module (`common/src/pricing.rs`), making the exact same financial math executable in both native Actix backend services and WebAssembly Leptos frontend clients.

---

## Requirements

### Isomorphic Pricing Engine (`common/src/pricing.rs`)
- **Centralized Typestate Engine**: Move `BookingCalculator` and its associated typestates (`BaseRate`, `Discounted`, `Taxed`) into `common/src/pricing.rs`.
- **Financial Precision Standard**: All monetary calculations, exchange rates, discounts, and statutory taxes must strictly use `rust_decimal::Decimal`. Floating-point types (`f32`/`f64`) are prohibited.
- **Shared Data Models**: Move `FeeItem` into `common/src/models.rs` (or `common/src/pricing.rs`) so both native backend services and WASM web applications can share fee breakdown structures.
- **Standardized Tax & Fee Calculations**: Standardize tax calculations (e.g., Jamaican GCT or configured rate) and fee rules across backend and frontend, eliminating hardcoded ad-hoc arithmetic in UI components.
- **Multi-Currency Conversion Logic**: Provide clean, isomorphic conversion helpers within `common::pricing` to handle tri-currency operations (Base Currency -> Payment Currency -> Final Charges).

### Backend Integration (`app_api/booking_api`)
- Remove internal definitions of `BookingCalculator`, `BaseRate`, `Discounted`, and `Taxed` from `app_api/booking_api/src/apis.rs`.
- Update booking creation handlers, listing quote logic, and OpenAPI spec schemas to consume `common::pricing::BookingCalculator`.

### Frontend Integration (`web_app`)
- Remove hardcoded arithmetic (`let tax_value = sub_total_price * Decimal::new(1, 1);`) in `web_app/src/components/checkout.rs`.
- Import and use `common::pricing::BookingCalculator` inside Leptos server functions and component signals to compute subtotals, tax values, fee breakdowns, and final totals identically to the backend.

---

## Technical Implementation

### 1. `common` (Isomorphic Crate)
- **`common/Cargo.toml`**: Ensure `rust_decimal` and `serde` are enabled with `wasm32-unknown-unknown` compatibility.
- **`common/src/pricing.rs` [NEW]**:
  - Implement typestates: `BaseRate`, `Discounted`, `Taxed`.
  - Implement `BookingCalculator<State>` with fluent state transitions:
    - `new(actual_daily_rate: Decimal, total_days: i32) -> BookingCalculator<BaseRate>`
    - `apply_discounts(monthly_pct: Option<Decimal>, weekly_pct: Option<Decimal>) -> BookingCalculator<Discounted>`
    - `apply_taxes(tax_rate: Option<Decimal>) -> BookingCalculator<Taxed>`
    - `finalize() -> BookingCalculator<Taxed>`
  - Implement dynamic fee calculation routines and tri-currency multiplier helpers.
- **`common/src/models.rs`**: Add `FeeItem` struct definition with Serde attributes.
- **`common/src/lib.rs`**: Export `pub mod pricing;`.

### 2. `app_api/booking_api` (Backend Service)
- **`app_api/booking_api/src/apis.rs`**:
  - Delete local `BookingCalculator` struct and typestate implementations.
  - Import `use common::pricing::{BookingCalculator, BaseRate, Discounted, Taxed};` and `use common::models::FeeItem;`.
  - Update `create_booking` and quote handlers to use the shared calculator.

### 3. `web_app` (WASM Frontend Application)
- **`web_app/src/components/checkout.rs`**:
  - Replace manual arithmetic in `initiate_booking` with `BookingCalculator`:
    ```rust
    let calculator = BookingCalculator::new(daily_rate, total_days)
        .apply_discounts(listing.monthly_discount_percentage, listing.weekly_discount_percentage)
        .apply_taxes(None)
        .finalize();
    ```
  - Update UI price detail rendering in `CheckoutPage` to use output fields directly from `calculator` / `BookingResponse`.

---

## Performance & Scalability Considerations

- **Zero Allocation in Core Calculation**: `BookingCalculator` operates entirely on small `rust_decimal::Decimal` structs (16 bytes each) on the stack, avoiding dynamic heap allocation until `fee_breakdown` initialization.
- **WASM Footprint**: Extracted module has zero heavy dependencies (no `tokio`, `actix`, or DB crates), ensuring minimal impact on WebAssembly binary bundle size (< 15 KB compiled WASM overhead).
- **Time & Space Complexity**:
  - `BookingCalculator` construction & state transitions: $\mathcal{O}(1)$ time complexity, $\mathcal{O}(1)$ space complexity.
  - Fee summation: $\mathcal{O}(k)$ where $k$ is the number of fee items (typically $k \le 5$).
- **No N+1 Database or Network Calls**: The pricing engine is purely computational; all inputs (rates, stay durations, discount percentages) are passed in by the caller.

---

## Threat Modeling & Security Mitigations

- **Client-Side Tampering Mitigation**: The WASM frontend executes `BookingCalculator` for real-time UI rendering only. When the user submits a booking hold request, `app_api/booking_api` re-executes `BookingCalculator` independently using authoritative database rates. Client-provided monetary totals are never trusted.
- **Arithmetic Precision & Overflow**: Standard `f64` operations suffer from binary floating-point representation drift (e.g. `0.1 + 0.2 != 0.3`). Utilizing `rust_decimal::Decimal` guarantees fixed-point precision with up to 28 decimal digits, preventing rounding exploitation or penny drift vulnerabilities.
- **State Machine Enforcement via Typestate**: The compile-time typestate pattern (`BaseRate` -> `Discounted` -> `Taxed`) makes it impossible to finalize a booking total without applying discounts and taxes in the exact required sequence.

---

## Test Plan

### Automated Unit Tests (`common/src/pricing.rs`)
- `test_calculator_basic_stay`: Verify subtotal for a standard 3-night stay without discounts or tax.
- `test_weekly_discount_application`: Verify a 7+ night stay applies weekly percentage discount correctly.
- `test_monthly_discount_application`: Verify a 28+ night stay applies monthly percentage discount over weekly discount.
- `test_tax_and_platform_fee_math`: Verify exact tax and 5% platform fee calculations against known decimal test vectors.
- `test_zero_and_negative_days_guard`: Verify calculator rejects or zero-guards invalid night durations.

### Backend Integration Tests (`app_api/booking_api/src/apis_test.rs`)
- `test_create_booking_pricing_parity`: Assert that booking records created via Actix API endpoints persist exact totals matching `common::pricing`.
- `test_currency_conversion_pricing`: Verify multi-currency conversion calculations maintain decimal precision across backend database exchange rates.

### WASM & UI Verification (`web_app`)
- Execute Leptos WASM component build (`cargo check --target wasm32-unknown-unknown -p web_app`).
- Verify price breakdown card rendering on checkout page matches backend calculated totals.

---

## Acceptance Criteria

- [ ] `common/src/pricing.rs` is created and exported in `common/src/lib.rs`.
- [ ] `BookingCalculator` and typestates (`BaseRate`, `Discounted`, `Taxed`) are fully implemented in `common::pricing`.
- [ ] `FeeItem` is moved to `common` and accessible in both backend and frontend crates.
- [ ] All monetary operations strictly use `rust_decimal::Decimal` with no floating-point arithmetic.
- [ ] Internal `BookingCalculator` implementation in `app_api/booking_api/src/apis.rs` is removed and replaced with `common::pricing::BookingCalculator`.
- [ ] Hardcoded tax calculations (`Decimal::new(1, 1)`) in `web_app/src/components/checkout.rs` are eliminated and replaced with `common::pricing::BookingCalculator`.
- [ ] Unit test suite in `common/src/pricing.rs` passes completely (`cargo test -p common`).
- [ ] Integration tests in `app_api/booking_api` pass without regression (`cargo test -p booking_api`).
- [ ] Both native target (`cargo build --workspace`) and WASM target (`cargo check --target wasm32-unknown-unknown -p web_app`) compile cleanly without warnings.
