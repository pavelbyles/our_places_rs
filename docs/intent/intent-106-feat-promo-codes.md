# Intent: Promo Codes at Checkout

## Metadata
- **Author**: Originator (via GitHub Issue #106)
- **Date**: 2026-11-01
- **Status**: Under Review
- **Target Area**: Fullstack (`common`, `db_core`, `app_api/booking_api`, `common/src/app_client`, `web_client`, `web_ui_guest`, `web_ui_admin`)

---

## 1. Problem Statement & Motivation
The platform has one kind of discount: the automatic weekly/monthly percentage set on each listing. The admin and hosts have no way to run a targeted offer, such as a seasonal campaign, a returning-guest code or a low-occupancy promotion. Any such offer has to be arranged and priced by hand, outside checkout.

A promo code changes what the guest pays, the tax charged and what the host receives, so it can't be bolted on as a display-only line. It has to be part of the priced booking and appear consistently on quotes, receipts, emails and payouts.

---

## 2. Proposed Outcome (The Vision)
1. **Creating codes**: the admin (any listing) and hosts (their own listings only) create codes with:
   - a percentage or fixed-amount discount (fixed amounts in a stated currency, converted at booking time)
   - a valid-from/valid-to period and optional stay-date restrictions
   - a minimum number of nights or minimum booking value
   - a total usage limit and a per-guest usage limit
   - the listings it applies to (all or selected)
   - whether it applies to the base stay only or also to fees
2. **Using a code**: a guest enters a code at checkout. The quote shows either the discount line or a clear reason why the code isn't valid.
3. **Defined pricing order**: `BookingCalculator` applies promo discounts at a fixed, documented point: after weekly/monthly discounts and before tax.
4. **Recorded and counted**: the booking stores the code and the discount amount. Usage is counted atomically, so concurrent checkouts can't exceed a limit.
5. **Visible everywhere**: the discount appears on confirmation emails, tax documents and the host payout ledger.
6. **Host-funded**: the host pays for the discount. It reduces the host's payout, and the platform fee is calculated on the discounted amount.
7. **Managed in the portal**: the admin portal lists codes with usage stats and can deactivate them. Codes that have been used are never hard-deleted.

---

## 3. Scope Boundaries
### In Scope (MVP)
- Promo code and redemption storage in a new timestamped migration, with case-insensitive unique codes.
- Create, list and deactivate endpoints in `booking_api`, enforced server-side: admin for any listing, host for owned listings only (`listing.user_id`), otherwise 403.
- A new `BookingCalculator` step, used by all three pricing paths: `bookings.rs` (hold), `booking_updates.rs` (update) and `reservation_changes.rs` (change quote).
- **A full checkout quote endpoint.** This doesn't exist today. The guest checkout page shows `web_ui_guest/src/quote.rs` output, which is built from the seasonal subtotal and has no fees or tax from `BookingCalculator`. The discount line needs a server-priced quote.
- Typed validation failures, with one error code per reason: not found or not applicable, not yet valid, expired, deactivated, minimum not met, listing or stay dates not eligible, usage limit reached, per-guest limit reached.
- **Host payout includes the cleaning fee, for all bookings and not just promo bookings.** Today host gross is `accommodation_net × rate` and leaves the cleaning fee out. All payout calculations change together: `db_core/src/booking/update.rs` (confirmation and revision) and `reservation_changes.rs` (previous and proposed payout).
- Rate limiting on code attempts. No rate limiting exists anywhere in the repo today, so this is new infrastructure (see the design in §7).
- Discount shown in: confirmation emails, tax documents (`db_core/src/tax_document.rs`), host ledger views and CSV export (`common/src/payout.rs`, `common/src/csv.rs`), and booking display-currency conversion (`booking_api/src/apis/mod.rs`). The ledger also shows the cleaning fee, so hosts can see what makes up their gross.
- Documenting the pricing order in `docs/ARCHITECTURE.md` §4.2.
- Updates to `openapi.yaml`, `openapi-prod.yaml` and `.sqlx/` metadata, plus Playwright coverage for the guest and admin portals.

### Out of Scope / Non-Goals
- Add-ons. The issue mentions them, but no add-on concept exists in the codebase. The discount scope must not block adding them later.
- Stacking multiple promo codes on one booking (one code per booking).
- A platform-funded discount option. The host always pays in this iteration.
- Auto-applied codes, referral programs, guest-specific or single-recipient codes, and bulk code generation.
- Editing a code's financial terms after creation. To change the terms, deactivate the code and create a new one.
- A hard-delete endpoint. The issue only rules out deleting used codes, but the MVP has no delete at all.
- Adding a code to a booking that has already been placed.
- Re-pricing existing bookings when a code is deactivated.

---

## 4. Affected Systems & Stakeholders
- **User Personas**:
  - **Guest**: enters a code and sees the discount or the reason it was refused.
  - **Host**: creates codes for their own listings, sees only their own codes and stats, and funds the discount through a lower payout.
  - **Admin**: creates codes for any listing, sees all codes and stats, and deactivates any code.
- **System Components**:

  | Component | Change |
  |---|---|
  | `common/src/pricing.rs` | New promo step and type-state, discount rules, cap logic, pure unit tests |
  | `common` | Promo DTOs and error reasons, `app_client` methods |
  | `db_core` | Promo tables, atomic redemption in `booking/hold.rs`, booking and `booking_history` snapshot columns |
  | `db_core/src/models.rs` | `accommodation_net()` must account for the promo. It drives host gross, tax documents and retained amounts on cancellation |
  | `db_core/src/booking/update.rs` | Payout creation and revision, promo handling on changed terms |
  | `db_core/src/tax_document.rs` | Discount line. The taxable amount must match what was quoted |
  | `booking_api` | Promo management, quote endpoint, rate limiting, email payloads (`bookings.rs`, `notifications.rs`) |
  | `web_ui_guest` | `pages/checkout.rs` code entry and quote display, `booking_change.rs` |
  | `web_ui_admin` | Code list, create form, usage stats, deactivate action, ledger discount column |

---

## 5. Constraints & Non-Negotiable Invariants
- **Domain Invariants**:
  - `rust_decimal::Decimal` only, including form parsing and percentage handling. No `f32`/`f64`, and no `.unwrap()`/`.expect()`.
  - **Pricing order**: seasonal subtotal + extra-guest fees → weekly/monthly discount → promo discount → tax → guest platform fee → total. Tax is calculated on the post-promo amount.
  - **Both percentage fees follow the discounted amount**: the guest platform fee (5% of `discounted_subtotal`) and the host commission (`commission_pct` on ledger gross).
  - **Host gross**: `(accommodation_net + cleaning fee after promo) × host exchange rate`. The host therefore pays for the whole discount in either scope.
  - **Taxable amount**: `accommodation_net + cleaning fee after promo`. A host-funded discount reduces the amount GCT is charged on. Quote, booking and tax documents all use this same figure.
  - **Cancellation**: the cleaning fee stays 100% refunded to the guest (service never rendered), so the host's retained payout stays accommodation only.
  - **Never discounted**: the guest platform fee, the refundable damage deposit and GCT itself (GCT only falls because its base falls).
  - **Discount cap**: a discount can never exceed its eligible amount, and no amount or total goes negative.
  - **One rule set**: quote and hold use the same validation and calculation code, so they can't disagree.
  - **No silent re-pricing**: if a code stops being valid between quote and hold, the hold fails with the typed reason. It must not proceed at full price without the guest seeing the new total.
  - **Snapshot on the booking**: code, discount type and value, scope, discount amount and any conversion rate are stored on the booking at hold time. Later edits or deactivation of the code never change an existing booking, its tax documents or its ledger row.
  - **No fallback rates**: a missing conversion pair for a fixed-amount code returns a typed `FX_RATE_UNAVAILABLE`, as set by #100. It is never treated as a 1.0 rate or a relabelled currency.
  - **Currency reference**: any new currency column has a foreign key to `currencies` (rule from #100). A code can't be created in an inactive currency.
  - **History retained**: used codes and their redemption records are never hard-deleted. Foreign keys to them use `ON DELETE RESTRICT`.
  - Status transitions and booking snapshots continue through `booking_history`, which must include the new promo fields.
- **Concurrency**:
  - Redemption runs in the same transaction as the hold in `create_booking`, taking `SELECT ... FOR UPDATE` on the promo row. No in-memory counters or caches.
  - The lock order is extended and documented as listing → currency → promo code, matching the existing listing-then-currency order.
  - `create_booking` cancels the guest's other pending holds in the same transaction. That release must happen before limits are checked, or a guest re-placing a hold with a one-use code would be rejected by their own abandoned hold.
  - Pending holds end in several places, including a pg_cron SQL job with no application code (`cleanup_stale_pending_holds`), `expire_stale_holds` and hold supersession. A stored counter would have to be decremented on every path and would drift if one were missed, so the design must handle all of them.
- **Performance Budgets**: validation adds only indexed lookups. The promo row lock is held only for the hold transaction, and only bookings using the same code contend. The Cloud Run cold start budget is unchanged (<300ms p50, <1s p95).
- **Security & Compliance**:
  - Rate limiting must work across Cloud Run instances. An in-process counter resets on scale-to-zero and isn't shared between instances.
  - Host ownership is enforced in the API, including on every listing in a "selected listings" set. A host's "all listings" means only that host's own listings.
  - Code creation and deactivation are logged with the actor ID (`tracing` plus a persisted record).
  - Validation failures are logged without turning logs into a list of valid codes.
  - `CONSTRAINTS.md` applies in full, including doc comments on all new public items.

---

## 6. Success Metrics & Signals
- **Leading Indicators**:
  - Unit tests in `common`: percentage and fixed discounts, the cap, rounding, the order relative to weekly/monthly discounts and tax, both fee calculations, and a deposit that never changes.
  - Test: N concurrent holds on a code with one remaining use produce exactly one success and N−1 typed failures.
  - Test: concurrent holds can't exceed the per-guest limit.
  - Test: an abandoned or expired hold, including one cancelled by the pg_cron job, doesn't permanently consume a use.
  - Test: a guest replacing their own hold can reuse a one-use code.
  - Test: the quote total equals the created hold's total for the same inputs.
  - Test: a code invalidated between quote and hold returns a typed failure and creates no discounted or full-price hold.
  - Test: for one booking, the discount, net amount and tax agree across quote, booking, email, tax document and ledger row.
  - Test: deactivating a code, or publishing a new exchange rate, leaves existing bookings unchanged.
  - Test: a host gets 403 when creating a code for, or viewing or deactivating a code on, another host's listing.
  - Test: `SUMMER`, `summer` and ` Summer ` resolve to one code, and a case-variant duplicate is rejected.
  - Test: repeated failed attempts return a typed 429 across instances (shared store), at a configured threshold. A correct code is also refused during lockout.
  - Test: failed codes submitted at hold count towards the same limit as those submitted at quote.
  - Test: host gross includes the cleaning fee at confirmation, on revision and in the reservation-change quote, with and without a promo.
  - Test: cancelling a booking with a retained amount pays the host accommodation only, with no cleaning fee.
  - Database tests run against the local Postgres in the devenv shell, not Docker.
- **Lagging Indicators**:
  - Share of bookings using a code, and redemptions per code.
  - No usage-limit overruns in production.
  - No disputes caused by differences between the quoted discount, the receipt and the host's payout.
  - Rate-limit rejections visible in logs, so guessing attempts can be spotted.

---

## 7. Open Questions & Policy Concerns
### Resolved (originator, 2026-11-01)
- [x] **Host payout includes the cleaning fee.** This applies to all bookings. It also settles how the host funds a discount on the cleaning fee.
- [x] **Fee scope**: "also applies to fees" means the cleaning fee. Extra-guest fees are already in the subtotal, so they're discounted in either scope. The guest platform fee, deposit and GCT are never discounted.
- [x] **Taxable amount**: a host-funded discount reduces the taxable amount.
- [x] **"The platform fee"** means both the guest platform fee and the host commission. Both are calculated on the discounted amount.
- [x] **Admin-created codes** reduce the host's payout, as the issue states, with the discount visible in the host's ledger.
- [x] **Reservation changes**: the code stays attached using its snapshotted terms, and validity dates and usage limits aren't rechecked. A percentage is recalculated on the new subtotal. A fixed amount is kept, subject to the cap. If the new stay falls below the minimum nights or value, or outside the stay dates, the discount is lost, and the change quote shows this before the guest accepts.
- [x] **Usage**: consumed when the hold is created, and counted while the hold is live or the booking is confirmed or completed. Released when a hold expires or is superseded, and when a confirmed booking is cancelled, even if the host keeps part of the payment.
- [x] **Stacking**: the promo stacks with weekly/monthly discounts and applies to the amount after them.
- [x] **Stay-date restrictions**: every night of the stay must fall inside the window.
- [x] **Minimum booking value**: measured on the eligible amount before the promo and before tax, in the code's stated currency. Each code has one stated currency.
- [x] **Deactivation**: no reactivation in the MVP. Create a new code instead.

### Originator decisions still open
- [ ] **Is host commission charged on the cleaning fee?** Including the cleaning fee in gross means `commission_pct` applies to it unless it's excluded. *Proposed*: yes, commission applies to the whole gross, so there's one formula. The alternative keeps the cleaning fee commission-free.
- [ ] **What happens to bookings that already have a ledger row?** Their gross leaves out the cleaning fee. Without a decision, an older booking's payout would jump the next time its terms are revised.
  - *Proposed*: the migration recalculates `pending` rows once, so every unpaid payout uses one formula. `paid` rows are never changed.
  - *To decide separately*: whether hosts are owed the cleaning fee on payouts already paid. Report the total before deciding.

### Stage 2 design
- [ ] **Conversion rate for fixed-amount codes.** "The booking's rate" (`booking.exchange_rate`) only converts listing base currency to payment currency. A code in a third currency needs its own lookup, and no triangulation exists (#100 excluded it). *Proposed*: look up code currency → payment currency with `get_rate(RateUse::NewQuote)` at hold time and snapshot that rate. A missing pair makes the code unusable for that checkout.
- [ ] **Usage counting model.** *Proposed*: one redemption row per booking, with usage derived under the promo row lock by joining to live booking status. This handles the pg_cron path without changing that job. Confirm index needs and how admin stats are read.
- [ ] **Rate limiter.** *Proposed design*:
  - **Store**: a Postgres table of fixed-window counters, updated with one atomic upsert (`INSERT ... ON CONFLICT DO UPDATE SET count = count + 1 RETURNING count`). Postgres is used because counters must be shared across Cloud Run instances and survive scale-to-zero, and the stack has no Redis. Losing counters in a crash is acceptable, so the table can be `UNLOGGED`.
  - **Keys**: authenticated guest ID and client IP, each with its own limit. IPs are stored hashed. The guest key stops one account guessing, and the IP key stops one source using many accounts.
  - **What counts**: failed attempts only, at both quote and hold, so the hold can't be used to bypass the limit. Thresholds and the window live in `application` settings, overridable by environment variable like `pending_hold_minutes`.
  - **Order**: check the limit before validating the code, so a correct code gets the same refusal during lockout.
  - **Response**: 429 `PROMO_RATE_LIMITED` with `Retry-After`. `ApiError` has no 429 variant today, so one is added. Rejections are logged.
  - **Cleanup**: expired windows are removed by a scheduled sweep, using the existing internal cron endpoints or pg_cron.
  - **Client IP**: the guest portal calls `booking_api` server to server, so the API only sees the portal's address. The portal must read the guest's IP and forward it in a header. The API may trust that header only from an authenticated portal, or a caller could fake a new IP on each request. Check how `X-Forwarded-For` arrives in the deployed setup, and whether anything other than the portals can reach the API.
  - **Known limit**: this slows guessing but doesn't stop a distributed attempt on a guessable code like `SUMMER`. Usage limits cap the cost of a guessed code.
- [ ] **Quote access for signed-out guests.** The hold requires a token, but the checkout quote may be reached earlier. Per-guest limits can only be fully checked once the guest is known, so they're enforced at hold at the latest. Decide whether code entry requires sign-in, which would also make the guest rate-limit key always available.
- [ ] **Guest identity for per-guest limits.** Limits key on `guest_id`, and redemptions follow a booking through `transfer_booking_guest`. A person with several accounts can get around the limit. *Proposed*: accept this for the MVP and note it as a known limitation.
- [ ] **Reason messages versus code guessing.** The issue asks for clear reasons, but "expired" confirms a code exists where "not found" doesn't. *Proposed*: give specific reasons, relying on rate limiting. Also, a global uniqueness check lets a host discover that a code exists by trying to create it. Accept this or return a neutral message.
- [ ] **Storage shape on the booking.** Keep promo amounts separate from `discount_value`, so weekly/monthly and promo lines can be displayed and audited separately. `accommodation_net()` and its callers need to change together, and the cleaning fee needs a clear gross and after-promo figure. Line-level detail on tax documents also needs a column or `line_items` decision.
- [ ] **Stale discount on date changes (possible existing bug).** `db_core/src/booking/update.rs:217` writes `discount_value = COALESCE($9, discount_value)`. When a stay is shortened below the weekly threshold, the calculator returns `None`, so the old discount looks like it's kept. Confirm this, and don't copy the pattern for promo columns, where a removed discount has to be clearable.
