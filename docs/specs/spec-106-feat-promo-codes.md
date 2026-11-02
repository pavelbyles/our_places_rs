# Spec 106: Promo Codes

> Driven by GitHub Issue #106. Intent: [intent-106](../intent/intent-106-feat-promo-codes.md) (Approved 2026-11-01). Rate limiting is deferred to #110.

## Overview
The admin and hosts can create promo codes, and signed-in guests can redeem them at checkout. A discount is priced by `BookingCalculator` after weekly/monthly discounts and before tax, so it reduces the guest's total, the GCT, the guest platform fee, the host's gross and the host's commission. Each booking keeps a snapshot of the code's terms. Usage is derived from live booking status under a row lock, so concurrent checkouts can't exceed a limit and expired holds free their use without any cleanup.

This work also changes four things for all bookings, promo or not:
- Host payout now includes the cleaning fee, with commission charged on the whole gross.
- Direct booking updates no longer keep a weekly/monthly discount that has stopped applying.
- Retained payouts on partial refunds are actually written to the ledger.
- The listing page and checkout show the same server-calculated total as the hold.

**Personas**: Guest (redeems a code) · Host (manages their own codes, funds every discount on their listings) · Admin (manages all codes).

---

## 1. Requirements

### 1.1. Promo Codes
1. **Code format**: trimmed, then uppercased; must match `^[A-Z0-9-]{4,32}$`. Unique across the platform, so a deactivated code's string can't be reused.
2. **Discount**: `percentage` (`0 < value <= 100`) or `fixed` (`value > 0`, in the code's stated currency).
3. **Currency**: required when the discount is `fixed` or `min_booking_value` is set; otherwise empty. Must be an active currency at creation.
4. **Validity period**: `valid_from` and `valid_to` are both required, with `valid_to > valid_from`.
5. **Stay window** (optional): `stay_from` and/or `stay_to`, where `stay_to` is the last eligible night. Every night of the stay must fall inside the window.
6. **Minimums** (optional): `min_nights` and `min_booking_value`. If both are set, both must pass.
7. **Limits** (optional): `total_usage_limit` and `per_guest_limit`, each `>= 1`.
8. **Listings**: all or selected.
   - An admin's "all" covers every listing, including ones added later.
   - A host's "all" covers that host's listings, including ones added later.
   - Selected listings must be owned by the host when a host creates the code, otherwise 403.
9. **Scope**: `stay` or `stay_and_cleaning`. Extra-guest fees are in the subtotal, so they're discounted in both. The guest platform fee, damage deposit and GCT are never discounted.
10. **Immutable terms**: there is no edit endpoint. Deactivation is one-way and there is no delete.

### 1.2. Authorization
| Action | Admin | Host |
|---|---|---|
| Create | Any listing | Own listings only |
| List and view | All codes | Own codes only (`owner_host_id = caller`) |
| Deactivate | Any code | Own codes only |

- Admin-created codes have `owner_host_id = NULL`. A host can't see or deactivate them, even when they apply to that host's listing. The discount still appears in the host's ledger.
- A host-owned code applies only while `listing.user_id = owner_host_id`, including selected listings. A later ownership change can't make a different host fund it.
- Any other staff access returns 403. Non-staff callers get 403 through `require_staff`.

### 1.3. Pricing Order
All amounts are in the payment currency. `round` is `round_dp(2)`, as used throughout `common::pricing`.

```text
S  = seasonal nights + extra-guest fees      (sub_total_price)
L  = weekly/monthly discount                 (discount_value)
A  = S − L
C  = cleaning fee

percentage (p):  P_A = round(A × p / 100)
                 P_C = round(C × p / 100), or 0 for scope `stay`
fixed:           F   = round(value × promo_rate)
                 P_A = min(F, A)
                 P_C = min(F − P_A, C), or 0 for scope `stay`

A' = A − P_A                                 (accommodation_net)
C' = C − P_C                                 (cleaning_net)

tax       = round((A' + C') × tax_rate)
platform  = round(A' × 5%)
total     = A' + C' + tax + platform + deposit

gross      = round((A' + C') × host_rate)
commission = round(gross × commission_pct)
net        = max(0, gross − commission)
```

**Worked example**: `S` 1,000, weekly discount 100, cleaning 100, deposit 200, 15% GCT, 10% commission, host rate 1, promo 10% with scope `stay_and_cleaning`.

| Line | Amount |
|---|---|
| `A` / `P_A` / `A'` | 900 / 90 / 810 |
| `C` / `P_C` / `C'` | 100 / 10 / 90 |
| Tax | 135.00 |
| Guest platform fee | 40.50 |
| Guest total | 1,275.50 |
| Host gross / commission / net | 900 / 90 / 810 |

A fixed discount goes to the stay first, and only the remainder to the cleaning fee. A 100% discount is allowed: the guest then pays only the deposit and any remaining taxable amounts.

### 1.4. Validation
Checks run in this order and stop at the first failure, so the reason shown is predictable. All are pure functions in `common::promo`, except the lookups that supply their inputs.

| # | Check | Code | Rechecked under lock at hold | On reservation change |
|---|---|---|---|---|
| 1 | Malformed or unknown code | `PROMO_NOT_FOUND` | — | — |
| 2 | Deactivated | `PROMO_DEACTIVATED` | Yes | No |
| 3 | Before `valid_from` | `PROMO_NOT_YET_VALID` | Yes | No |
| 4 | At or after `valid_to` | `PROMO_EXPIRED` | Yes | No |
| 5 | Listing not covered | `PROMO_LISTING_NOT_ELIGIBLE` | No (immutable) | No |
| 6 | Conversion rate missing or currency inactive | `FX_RATE_UNAVAILABLE` | No | No (uses the snapshot) |
| 7 | Below `min_nights` | `PROMO_MIN_NIGHTS_NOT_MET` | No | Yes |
| 8 | Outside stay window | `PROMO_STAY_DATES_NOT_ELIGIBLE` | No | Yes |
| 9 | Below `min_booking_value` | `PROMO_MIN_VALUE_NOT_MET` | No | Yes |
| 10 | Total limit reached | `PROMO_USAGE_LIMIT_REACHED` | Yes | No |
| 11 | Per-guest limit reached | `PROMO_GUEST_LIMIT_REACHED` | Yes | No |

`min_booking_value` is compared with the eligible amount before the promo and tax: `A` for scope `stay`, `A + C` for `stay_and_cleaning`. The minimum is converted with the same rate as `F`.

### 1.5. Usage
A use counts when its redemption isn't released and its booking is live:

```sql
r.released_at IS NULL AND (
  b.status IN ('confirmed', 'completed')
  OR (b.status = 'pending' AND b.expires_at > NOW())
)
```

| Event | Effect on usage |
|---|---|
| Hold created | Counted |
| Hold expires (either sweep, or not yet swept) | Freed immediately, with no job changes |
| Guest places a new hold, cancelling the old one | Freed before the new limit check |
| Booking cancelled, even with a partial refund | Freed |
| Reservation change loses the discount | Freed (`released_at` set) |
| Hold transferred with `transfer_booking_guest` | Follows the booking's guest |

Per-guest counts join to `booking.guest_id`, so the redemption has no guest column to drift.

### 1.6. Reservation Changes
Both repricing paths use the booking's snapshot, not the live code. `reprice` is in `booking_updates.rs` and the change quote is in `reservation_changes.rs`.
1. Run checks 7–9 against the new stay and the snapshot.
2. **Passes**: recalculate with the snapshotted terms and rate. A percentage follows the new amounts. A fixed amount stays the same, subject to the cap.
3. **Fails**: the discount is removed, and the promo fields are cleared on the booking. The redemption is released, and the snapshot stays in `booking_history`. The guest's change page shows this before they accept.
4. Validity dates, deactivation and usage limits aren't rechecked.

### 1.7. Host Payout (All Bookings)
1. Every gross calculation uses `common::pricing::host_gross(accommodation_net, cleaning_net, rate)`. There is one formula in the code.
2. Commission applies to the whole gross.
3. The migration doesn't change existing ledger rows.
4. Revising an older booking uses the new formula, so its gross gains the cleaning fee at that point (`update.rs:321`, `reservation_changes.rs:228,237`).
5. **Change approval recalculates.** `approve_change_request` writes `snap.host_gross_amount` today, so a request created before this release would apply the old formula. It now calculates gross and net from the updated booking, using the ledger row's stored `exchange_rate` and `platform_fee_pct`.
6. On cancellation, the cleaning fee is still refunded in full, so the retained payout is accommodation only.

### 1.7.1. Retained Payouts (All Bookings)
Today a partial refund pays the host nothing, based on reading the code. `settle_cancellation` marks the pending ledger row `cancelled`, then inserts the retained payout, but `booking_id` is `UNIQUE` and the insert uses `ON CONFLICT (booking_id) DO NOTHING`, so the insert does nothing.

| Payout state at cancellation | Retained amount | Result |
|---|---|---|
| `pending` | Above 0 | The same row returns to `pending` with the retained gross, commission and net; `cleaning_fee_amount` and `promo_discount_amount` are 0 |
| `pending` | 0 | The row stays `cancelled` |
| `processing` or `paid` | Any | The row is unchanged and a warning is logged. Reconciling money already sent is out of scope |

Cancellations already processed aren't corrected.

### 1.7.2. One Stay Total Everywhere (All Bookings)
The listing page and checkout currently use `stay_price`, a portal-side calculation with a hardcoded `GCT_RATE`. It leaves out the guest platform fee, weekly/monthly discounts and extra-guest fees, so both pages can show less than the hold charges. Both pages now use `price_stay` through the API, and the portal-side calculation is removed.

For the same listing, dates, guests, currency and instant, with no code, the listing page, checkout and hold show the same total.

### 1.8. Display
| Surface | Change |
|---|---|
| Listing page | Server quote, including the platform fee, weekly/monthly discount and extra-guest fees. No promo entry |
| Checkout | Same server quote, plus the discount line or the refusal reason |
| Guest and admin booking detail | Code and discount |
| Guest confirmation email | Promo line; `accommodation_net` and `cleaning_fee` are after-promo figures |
| Host confirmation and update emails | Promo line; `total_payout` and `currency` come from the ledger row (net, host currency) instead of `sub_total_price` |
| Tax documents | "Includes promo discount (CODE)" in the totals. Line items stay net, so they still sum to the total |
| Host ledger and CSV | Cleaning fee, promo code and promo discount |
| `docs/ARCHITECTURE.md` §4.2 | Pricing order, lock order and payout formula |

### 1.9. Exclusions / Non-Goals
- Rate limiting (#110), add-ons, stacking codes, platform-funded discounts.
- Editing, reactivating or deleting codes; bulk generation; auto-applied or guest-specific codes.
- Adding a code to a booking that's already placed.
- Changing existing ledger rows, correcting paid payouts, or correcting existing stale discounts.
- Promo code entry on the listing page.
- Reconciling payouts already `processing` or `paid` on cancellation, or correcting past cancellations.

---

## 2. Edge Cases & Failure States
| Case | Behaviour |
|---|---|
| Two guests race for the last use | The promo row lock serialises them. One hold succeeds, the other gets `PROMO_USAGE_LIMIT_REACHED` |
| Code becomes invalid between quote and hold | Hold returns 422 with the reason. The whole transaction rolls back, including cancelling the guest's earlier hold. No hold is created at full price |
| Guest re-places a hold with a one-use code | Earlier holds are cancelled in the same transaction before counting, so it succeeds |
| Hold expired but not yet swept | Not counted. Confirming it still returns `HOLD_EXPIRED` |
| Code deactivated during a live hold | The hold keeps its discount and can still be confirmed |
| Code expires during a live hold | Same as deactivation |
| Fixed discount larger than the eligible amount | Capped. Totals are never negative |
| Scope `stay_and_cleaning` with no cleaning fee | `P_C = 0`, with no error |
| Rate published after the hold | Snapshot rate is kept for the booking's life |
| Code's currency deactivated or pair retired | New quotes and holds get `FX_RATE_UNAVAILABLE`. Existing bookings are unaffected |
| Exchange-rate failure at quote | The quote still returns, without the discount, with `promo_refusal.code = FX_RATE_UNAVAILABLE` |
| Duplicate code at creation, in any case | 409 `PROMO_CODE_EXISTS` |
| Deactivating twice | 409 `PROMO_ALREADY_DEACTIVATED` |
| Hard-deleting a booking with a redemption | Refused by the foreign key through the existing `BookingNotModifiable` handling |
| Change request pending when the discount would be lost | Each new quote re-evaluates, and approval applies the stored snapshot's outcome |
| Change request created before this release | `promo_retained` defaults to `false`. These bookings have no promo, so nothing changes. Payout uses the new formula (§1.7) |
| Blank code on the form | Treated as no code |
| Listing page dates that can't be quoted (below minimum stay, too many guests, no rate) | No price is shown, as today. Never a fallback figure |

---

## 3. Technical Implementation

### 3.1. Data Model
One new timestamped migration, created with `sqlx migrate add`.

```sql
CREATE TYPE promo_discount_type AS ENUM ('percentage', 'fixed');
CREATE TYPE promo_scope AS ENUM ('stay', 'stay_and_cleaning');

CREATE TABLE promo_code (
  id                UUID PRIMARY KEY DEFAULT uuidv7(),
  code              VARCHAR(32) NOT NULL UNIQUE CHECK (code ~ '^[A-Z0-9-]{4,32}$'),
  discount_type     promo_discount_type NOT NULL,
  discount_value    NUMERIC(12,2) NOT NULL CHECK (discount_value > 0),
  currency          CHAR(3) REFERENCES currencies(code) ON DELETE RESTRICT ON UPDATE RESTRICT,
  scope             promo_scope NOT NULL,
  valid_from        TIMESTAMPTZ NOT NULL,
  valid_to          TIMESTAMPTZ NOT NULL,
  stay_from         DATE,
  stay_to           DATE,
  min_nights        INTEGER CHECK (min_nights >= 1),
  min_booking_value NUMERIC(12,2) CHECK (min_booking_value > 0),
  total_usage_limit INTEGER CHECK (total_usage_limit >= 1),
  per_guest_limit   INTEGER CHECK (per_guest_limit >= 1),
  applies_to_all    BOOLEAN NOT NULL,
  owner_host_id     UUID REFERENCES "user"(id) ON DELETE RESTRICT,
  created_by        UUID NOT NULL REFERENCES "user"(id) ON DELETE RESTRICT,
  deactivated_at    TIMESTAMPTZ,
  deactivated_by    UUID REFERENCES "user"(id) ON DELETE RESTRICT,
  created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CHECK (valid_to > valid_from),
  CHECK (stay_from IS NULL OR stay_to IS NULL OR stay_to >= stay_from),
  CHECK (discount_type <> 'percentage' OR discount_value <= 100),
  CHECK (discount_type <> 'fixed' OR currency IS NOT NULL),
  CHECK (min_booking_value IS NULL OR currency IS NOT NULL)
);
CREATE INDEX idx_promo_code_owner ON promo_code (owner_host_id);

CREATE TABLE promo_code_listing (
  promo_code_id UUID NOT NULL REFERENCES promo_code(id) ON DELETE RESTRICT,
  listing_id    UUID NOT NULL REFERENCES listing(id) ON DELETE RESTRICT,
  PRIMARY KEY (promo_code_id, listing_id)
);

CREATE TABLE promo_code_redemption (
  id            UUID PRIMARY KEY DEFAULT uuidv7(),
  promo_code_id UUID NOT NULL REFERENCES promo_code(id) ON DELETE RESTRICT,
  booking_id    UUID NOT NULL UNIQUE REFERENCES booking(id) ON DELETE RESTRICT,
  released_at   TIMESTAMPTZ,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_promo_redemption_code ON promo_code_redemption (promo_code_id) WHERE released_at IS NULL;

ALTER TABLE booking
  ADD COLUMN promo_code_id           UUID REFERENCES promo_code(id) ON DELETE RESTRICT,
  ADD COLUMN promo_terms             JSONB,
  ADD COLUMN promo_stay_discount     NUMERIC(10,2) NOT NULL DEFAULT 0,
  ADD COLUMN promo_cleaning_discount NUMERIC(10,2) NOT NULL DEFAULT 0;

ALTER TABLE booking_history
  ADD COLUMN promo_code_id           UUID,
  ADD COLUMN promo_terms             JSONB,
  ADD COLUMN promo_stay_discount     NUMERIC(10,2),
  ADD COLUMN promo_cleaning_discount NUMERIC(10,2);

ALTER TABLE host_payout_ledger
  ADD COLUMN cleaning_fee_amount   NUMERIC(12,2) NOT NULL DEFAULT 0,
  ADD COLUMN promo_discount_amount NUMERIC(12,2) NOT NULL DEFAULT 0;

ALTER TABLE tax_document
  ADD COLUMN promo_code     VARCHAR(32),
  ADD COLUMN promo_discount NUMERIC(12,2) NOT NULL DEFAULT 0;
```

- **Currency rule**: `promo_code.currency` has its foreign key, following the #100 rule.
- **`booking.cleaning_fee`** stays the gross fee, and `Booking::cleaning_net()` is the after-promo figure.
- **`tax_document.cleaning_fee` and `accommodation_net`** hold net figures, so `taxable_amount` and the line items still add up.
- **Ledger amounts** are in the row's host currency. `cleaning_fee_amount` and `promo_discount_amount` are display figures rounded separately, so the accommodation part is shown as `gross − cleaning_fee_amount`. Old rows and retained-payout rows hold 0.
- **Actors**: `created_by` and `deactivated_by` are the persisted record, since those are the only two mutations.
- **pg_cron**: reschedule `cleanup_stale_pending_holds` in the migration so its `booking_history` insert includes the four promo columns, following `20261031010000_booking_hold_expires_at.sql`. Its logic is otherwise unchanged.

### 3.2. Domain (`common`)
**New `common/src/promo.rs`**:
- `PromoTerms`: the snapshot stored in `booking.promo_terms`. Holds code, type, value, scope, currency, snapshotted rate, minimums and stay window.
- `PromoRefusal`: one variant per §1.4 code, with `code()` and a guest-facing `message()`.
- `normalize_code(&str) -> Option<String>`.
- `check_redeemable(status, usage, now) -> Result<(), PromoRefusal>`: checks 2–4 and 10–11.
- `check_stay_rules(terms, stay, eligible_amount) -> Result<(), PromoRefusal>`: checks 7–9.

**`common/src/pricing.rs`**:
- Add a `PromoApplied` type-state. `apply_taxes` moves from `Discounted` to `PromoApplied`, so no pricing path can skip the step:

```rust
.apply_discounts(monthly, weekly)
.with_fees(cleaning, deposit)
.apply_promo(terms)   // Option<&PromoTerms>; None for no promo
.apply_taxes(rate)
.finalize()
```

- New fields: `promo_stay_discount`, `promo_cleaning_discount` and `cleaning_net`. `discounted_subtotal` becomes `A'`, so the platform fee follows it without change.
- `apply_taxes` and `finalize` use `cleaning_net`.
- No promo line in `fee_breakdown`: the code that reads it looks up fees by name, and the discount has its own fields.
- Add `host_gross(accommodation_net, cleaning_net, rate)`.

**DTOs in `common/src/models.rs`**:
- `NewBookingRequest` gains `#[serde(default)] promo_code: Option<String>`.
- `BookingResponse` gains `promo_code` and `promo_discount`.
- `ReservationChangePricingSnapshot` gains `#[serde(default)]` fields `promo_stay_discount`, `promo_cleaning_discount`, `cleaning_net` and `promo_retained`.
- `ReservationChangeQuoteResponse` gains `promo_outcome`: none, kept, or lost with a reason.

**Email and ledger**: payloads in `common/src/email.rs` gain `#[serde(default)]` promo fields. `common/src/payout.rs` and `common/src/csv.rs` gain the new ledger columns, added at the end of the CSV header.

### 3.3. Database Layer (`db_core`)
**`db_core/src/promo.rs`**: create, list with stats (one grouped query per page), get, deactivate, get by code, listing eligibility, and `lock_and_count`.

**`Booking`**:
- Add the four columns to the struct and to every query that returns it, in `booking/hold.rs`, `booking/queries.rs`, `booking/update.rs`, `booking/history.rs`, `reservation_change.rs` and `tax_document.rs`.
- `accommodation_net()` also subtracts `promo_stay_discount`.
- Add `cleaning_net()`.

**`create_booking`** gains promo handling after the guest's earlier holds are cancelled:

```text
lock listing → ensure currency active → overlap check → cancel guest's other holds
→ SELECT promo_code ... FOR UPDATE → count usage → check_redeemable
→ INSERT booking → INSERT redemption → history → COMMIT
```

The lock order is listing → currency → promo code. A refusal returns `DbError::PromoRefused(PromoRefusal)` and rolls back the transaction.

**Stale discount fix**: `UpdatedBooking` gains `replace_discounts: bool`, set by `reprice`. In `write_booking_update`:

```sql
discount_value = CASE WHEN $21 THEN $9 ELSE COALESCE($9, discount_value) END
```

The promo columns follow the same flag. When a promo is lost, the same transaction sets `released_at`. `approve_change_request` does the same from the snapshot, and §1.7 item 5 applies.

**Host payout**: `start_host_payout`, `revise_confirmed_terms` and `approve_change_request` use `host_gross`. They also write `cleaning_fee_amount` and `promo_discount_amount`.

**Retained payouts**: `settle_cancellation` uses the ledger ID returned by `cancel_pending_payout_for_booking`. A new `reopen_payout_as_retained` updates that row `WHERE id = $1 AND status = 'cancelled'`, setting gross, commission percentage and amount, net, exchange rate and `pending` status. `create_payout_ledger_entry` keeps `DO NOTHING`, which makes confirmation safe to repeat.

**Tax documents**: all three builders (`tax_document.rs:394`, `:554`, `:760`) use `cleaning_net()`. `RefundableAmounts` also takes the net cleaning fee, so the refund matches what the guest paid.

### 3.4. API (`booking_api`)
A new `apis/promo_codes.rs` handles management. The stay pricing in `bookings.rs` moves to a shared `price_stay` function that the quote and the hold both call, so they can't disagree.

| Method & path | Auth | Success | Errors |
|---|---|---|---|
| `GET /api/v1/bookings/listing/{id}/quote?check_in&check_out&guests&currency` | Public | 200 `StayQuoteResponse` without a promo | Same stay and currency errors as the hold, through `map_pricing_error` |
| `POST /api/v1/bookings/quote` | Signed-in | 200 `StayQuoteResponse` | Same stay and currency errors as the hold; a refused code is not an error |
| `POST /api/v1/bookings` (add `promo_code`) | Existing | 201 | Existing, plus 422 for any §1.4 code |
| `GET /api/v1/admin/promo-codes` | Staff | 200 page with stats | 403 |
| `POST /api/v1/admin/promo-codes` | Staff | 201 | 400, 403, 409 `PROMO_CODE_EXISTS`, 422 `CURRENCY_INACTIVE` |
| `GET /api/v1/admin/promo-codes/{id}` | Staff | 200 | 403, 404 |
| `POST /api/v1/admin/promo-codes/{id}/deactivate` | Staff | 200 | 403, 404, 409 `PROMO_ALREADY_DEACTIVATED` |

```jsonc
// StayQuoteResponse: a refused code still returns the full-price quote
{
  "nights": 7, "currency": "USD", "effective_nightly_rate": "142.86",
  "subtotal": "1000.00", "stay_discount": "100.00",
  "promo": { "code": "SUMMER", "discount": "100.00" },
  "promo_refusal": null,            // or { "code": "PROMO_EXPIRED", "message": "..." }
  "cleaning_fee": "90.00", "tax_amount": "135.00",
  "platform_fee": "40.50", "damage_deposit": "200.00", "total_amount": "1275.50"
}
```

- The public quote is registered before the protected scope, like `unavailable-ranges`, and on its own path. A public resource at `/api/v1/bookings/quote` would answer the signed-in `POST` with 405 instead of falling through to the scope. It takes no promo code, so codes can't be tested signed out.
- Register `POST /quote` before any parameterised route in the scope.
- Per-guest counts at quote use `claims.sub`. The hold already rejects `guest_id != claims.sub` (`bookings.rs:52`).
- Host emails read the ledger row. If it's missing, log an error and don't send a payout figure: no fallback amount.
- Update `apis/mod.rs` display conversion, `apis/routes.rs` and the utoipa registration.
- Add the new paths to `openapi.yaml` and `openapi-prod.yaml`, or the gateway won't route them.

### 3.5. Clients & Frontend
**`common/src/app_client/promo.rs` and `web_client`**: methods for the new endpoints.

**Listing page (`web_ui_guest/src/pages/listing_detail.rs`)**: `initial_breakdown` calls the public quote, passing the form's guest count. The `hx-get` fragment and `quote_panel` are otherwise unchanged.

**Remove the old calculation**: delete `stay_price`, `GCT_RATE` and the old `quote_for` from `web_ui_guest/src/quote.rs`, along with their tests. Also delete `common::app_client::get_pricing_quote` and its `web_client` wrappers, which then have no callers.

**Checkout (`web_ui_guest/src/pages/checkout.rs`)**:
- Use the signed-in server quote. Checkout totals now match the hold, including the platform fee and weekly/monthly discount.
- Add a code field and an "Apply" button inside the existing form: `name="intent" value="apply_promo" formnovalidate`, placed before "Confirm reservation". It works without JavaScript and keeps what the guest typed, and pressing Enter applies the code instead of placing the booking.
- `checkout_submit` re-renders on `apply_promo` without placing a booking.
- Show the discount in the breakdown, or show the refusal message next to the field. A "Remove" button clears the code.
- `place_booking` sends the code and maps `PROMO_*` errors to `PromoRefusal::message()`, keeping the entered code.

**`web_ui_common` `price_breakdown`**: `PriceBreakdown` gains `platform_fee`, `promo_code` and `promo_discount`, with new IDs `breakdown-platform-fee` and `breakdown-promo`. The listing page leaves the promo fields as `None`.

**Change page (`booking_change.rs`)**: warn when `promo_outcome` is lost, with the reason.

**Admin (`web_ui_admin/src/pages/promo_codes.rs`)**: follows `currencies.rs`.
- `GET /admin/promo-codes`, `POST /admin/promo-codes` and `POST /admin/promo-codes/{id}/deactivate`, using the same card and `hx-post` pattern.
- List columns: code, discount, validity, listings, status, active holds, confirmed uses and remaining uses.
- The create form covers everything in §1.1. Dates are picked as Jamaica calendar days and sent as the start of `valid_from` and the end of `valid_to`.
- The listing picker shows a host's own listings, or all listings for an admin.
- Add a "Promo codes" link to both navs in `shell_admin.rs`, for all staff.

**Payouts and bookings**: add the ledger columns to `payouts.rs` and the promo line to both booking detail pages.

### 3.6. Background Workers
No new jobs. In `email_worker/src/processor.rs`, templates use plain `{{placeholder}}` replacement, so the worker builds a `{{promo_line}}` value that's empty when there's no promo. Update `booking_confirmation_guest.html`, `booking_confirmation_host.html` and `booking_updated_host.html`. The hold-time email enqueued in `bookings.rs` also gains the discount.

---

## 4. Performance Considerations
- **Listing page**: each date or guest change makes one API call that replaces the portal's listing and price-override fetches. The public quote runs the same read-only queries as the hold's pricing, takes no lock, and is unthrottled like the public availability endpoint.
- **Signed-in quote**: adds at most three indexed lookups: the code, listing eligibility and the usage count. Rate lookup is the existing single indexed query.
- **Usage count**: scans only unreleased redemptions for that code through `idx_promo_redemption_code`. Cost grows with a code's uses and each lookup is by primary key on `booking`, which is fine at this scale.
- **Promo row lock**: held for the hold transaction only, and only holds using the same code contend. Calls with no code take no extra lock.
- **Admin list**: one grouped stats query per page, with no N+1.
- **Cold start**: no new startup work or dependencies. `sha2` is already in `booking_api`.
- **Async**: no blocking or CPU-heavy work added.

---

## 5. Security Mitigations
| Risk | Mitigation |
|---|---|
| IDOR on code management | Ownership checked in the API on every read and write. Each selected listing is checked at creation |
| One host funding another's code | §1.2 `listing.user_id = owner_host_id` check at redemption |
| Usage limit bypass through a race | `FOR UPDATE` in the hold transaction, with no in-memory counters |
| Public quote | Read-only, returns only prices the listing page already shows, and takes no code |
| Tampered price from the portal | The server prices everything from the code; the client sends only the code string |
| Injection | `sqlx` parameterised queries only; code format validated before lookup |
| XSS | Codes are limited to `[A-Z0-9-]`, and Topcoat escapes output |
| Code guessing | **Not mitigated until #110 ships.** Failed attempts log the guest ID, the reason and a SHA-256 hash of the code, never the plain code. Usage limits cap the cost of a guessed code |
| Existence of a code leaking | Accepted: specific refusal reasons, and `PROMO_CODE_EXISTS` at creation, both confirm a code exists |
| Multiple accounts bypassing per-guest limits | Accepted for the MVP |

---

## 6. Test Plan
Database tests run against the local Postgres in the devenv shell, not Docker.

### Unit Tests (`cargo test -p common`)
- Percentage, fixed, cap and 100% discounts, for each scope.
- The §1.3 worked example, exactly.
- Fixed discount allocated to the stay first, then cleaning.
- Deposit never changes; platform fee follows `A'`; tax follows `A' + C'`.
- Promo applied after weekly/monthly discounts.
- `host_gross` includes net cleaning; commission on whole gross.
- `RefundQuote` invariant with a promo: `refund_total + retained_total == total_price`.
- Every §1.4 check in order, including boundaries: `valid_to` instant, last eligible night, exact minimum value.
- `normalize_code`: `" Summer "` → `SUMMER`; rejects bad characters and lengths; blank → none.
- Rejected inputs for the §1.1 rules.
- CSV columns and `{{promo_line}}` rendering.

### Integration Tests (`cargo test -p db_core -p booking_api`)
- N concurrent holds on one remaining use: exactly one success, N−1 `PROMO_USAGE_LIMIT_REACHED`.
- Concurrent holds can't exceed the per-guest limit.
- Expired hold, including the `expire_stale_holds` path and an unswept hold, doesn't consume a use.
- Guest re-placing a hold reuses a one-use code; a refused hold leaves the earlier hold in place.
- Cancelling a confirmed booking frees the use.
- Public quote, signed-in quote and hold totals are equal for the same inputs with no code, including extra guests and a weekly discount.
- The public quote needs no auth and ignores any code sent to it; `POST /quote` without auth returns 401.
- Code invalidated between quote and hold: 422 and no hold.
- Deactivating a code or publishing a new rate leaves existing bookings, documents and ledger rows unchanged.
- For one booking, discount, net and tax agree across quote, booking, email payload, tax document and ledger.
- Authorization: host 403 on creating for, viewing or deactivating another host's or an admin's code; guest 403 on management.
- Host-owned code refused on a listing transferred to another host.
- Case-variant duplicate returns 409.
- **Reservation changes**: percentage recalculated; fixed kept and capped; discount lost when below minimums, with the use released; deactivated or expired code still kept.
- **Host payout**: gross includes the cleaning fee at confirmation, on revision, in the change quote and on approval, with and without a promo.
- **Migration and older rows**: existing ledger rows unchanged by the migration; revising an older booking uses the new formula; approving a request created under the old formula uses the new one.
- **Retained payouts**: a partial refund leaves a `pending` row with the retained accommodation amount and no cleaning fee; commission is on the retained gross; a full refund leaves the row `cancelled`; a `paid` row is unchanged, with a warning logged; repeating confirmation doesn't duplicate or overwrite a row.
- **Stale discount**: shortening below the weekly threshold through a direct update clears it; a status-only update keeps it.
- **Host email**: `total_payout` equals the ledger net, in the ledger currency.
- Update existing tests for the new `apply_promo` step and for gross now including cleaning fees, without weakening their assertions.

### E2E Tests (Playwright)
- `guest-portal/checkout-flow.spec.ts`: apply a valid code, see the discount, confirm, and see it on the booking; an expired code shows its reason and full price; remove a code; entered details are kept after applying. The listing page and checkout show the same total.
- Update `checkout-flow.spec.ts:35`: the expected total changes from 3973.75 to 4136.25, because it now includes the 162.50 platform fee (5% of 3250.00). This corrects the page to match what the hold charges, and isn't a weakened assertion.
- New `admin-portal/promo-codes.spec.ts`: admin creates, lists and deactivates; host sees only their own and only their listings in the picker.

### Gates
All of `CONSTRAINTS.md` §2, including `cargo sqlx prepare --workspace --check`, the `f32`/`f64` and `.unwrap()`/`.expect()` checks, and `-D missing_docs`.

---

## 7. Acceptance Criteria
- [ ] 1. The admin can create a code for any listing, and a host for their own only, with every option in §1.1.
- [ ] 2. Codes are case-insensitive and unique.
- [ ] 3. A signed-in guest can apply a code at checkout and see the discount line or a specific reason.
- [ ] 4. The pricing order in §1.3 is enforced by the type-state in all three pricing paths and documented in `docs/ARCHITECTURE.md`.
- [ ] 5. Concurrent checkouts can't exceed a total or per-guest limit.
- [ ] 6. Abandoned, expired and cancelled bookings free their use without any cleanup job.
- [ ] 7. A code invalidated between quote and hold refuses the hold, and no full-price hold is created.
- [ ] 8. The booking stores the code, terms and discount, and later code or rate changes don't affect it.
- [ ] 9. Reservation changes follow §1.6, and the guest sees a lost discount before accepting.
- [ ] 10. The discount appears on confirmation emails, tax documents, and the host ledger and CSV.
- [ ] 11. The guest platform fee and host commission are calculated on discounted amounts, and the deposit and platform fee are never discounted.
- [ ] 12. Every future host gross includes the net cleaning fee, with commission on the whole gross, through one function.
- [ ] 13. The migration changes no existing ledger rows.
- [ ] 14. Host emails state the real net payout in the host's currency.
- [ ] 15. Direct updates clear a discount that no longer applies.
- [ ] 16. A partial refund writes the retained payout to the ledger; a `processing` or `paid` payout is left unchanged and logged.
- [ ] 17. With no code, the listing page, checkout and hold show the same total, and the portal-side price calculation is gone.
- [ ] 18. The portal lists codes with usage stats and deactivates them; used codes can't be deleted.
- [ ] 19. Failed attempts are logged with a hash of the code, never the code itself.
- [ ] 20. All `CONSTRAINTS.md` gates pass.

