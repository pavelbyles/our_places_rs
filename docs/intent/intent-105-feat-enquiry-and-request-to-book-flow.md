# Intent: Enquiry and Request-to-Book Flow

## Metadata
- **Author**: Originator (via GitHub Issue #105)
- **Date**: 2026-11-05
- **Status**: Approved (2026-11-05)
- **Target Area**: Fullstack (`common`, `db_core`, `app_api/booking_api`, `common/src/app_client`, `web_client`, `web_ui_guest`, `web_ui_admin`)

---

## 1. Problem Statement & Motivation
The platform only sells through instant booking: a guest picks dates, a `pending` hold is created and the guest pays. Villas usually don't sell that way, especially for large groups, weddings and long stays. Those guests want to ask questions first, or ask the host to approve the stay, and hosts want to vet them or quote a special price.

Today a guest with a question has nowhere to ask it. Messaging (#81) only exists on a booking, so the guest has to place a hold first. A host who wants to approve a group or offer a negotiated price has to do it off-platform. That loses the audit trail, the availability lock and the priced booking. Those enquiries then either go nowhere or turn into bookings arranged by hand.

---

## 2. Proposed Outcome (The Vision)
1. **Booking mode per listing**: each listing is set to *instant book*, *request to book*, or *both*. With *both*, the guest chooses between "Book now" and "Request to book" at checkout.
2. **Enquiry**: a signed-in guest sends a question with dates and guest count. This starts a message thread with the host (reusing booking messaging), commits the guest to nothing and **does not block dates**.
3. **Request to book**: a signed-in guest submits dates, guest count and an optional promo code. A pending request is created that **blocks the dates** for a configurable response window (`application.request_response_hours`, env-overridable like `pending_hold_minutes`).
4. **Host response**: the host (own listings) or admin (any listing) can:
   - **accept** the request,
   - **decline** it with a reason, or
   - **send a custom quote**: an adjusted stay price plus free-text special terms. Fees, GCT and the platform fee are recalculated from it by `BookingCalculator`. The guest can accept the quote until it expires (`application.quote_valid_hours`), and the dates stay blocked until then.
   - Decline reasons come from a fixed list (dates unavailable, group size, stay length, event not permitted, other) plus optional free text. The guest sees both.
5. **Conversion to booking**: an accepted request (or a sent quote) is an **offer**. It keeps the dates blocked for `application.quote_valid_hours` after acceptance, so a host accepting at 2am doesn't start the guest's payment clock. When the guest clicks "Proceed to checkout" (or accepts the quote), the offer becomes a normal `pending` hold in **one transaction**, with the existing `pending_hold_minutes` window. The dates are never released in between, and from there the existing hold → payment → confirmed flow applies.
6. **Enquiry → request / quote**: from an enquiry thread, the guest can submit a request to book (prefilled), and the host can reply with a custom quote. The same thread continues throughout.
7. **Expiry**: requests with no response and offers (accepted requests or quotes) the guest doesn't take up expire through the existing sweeps. Expiry releases the dates and notifies the guest and host. The host gets one reminder email when an unanswered request enters the near-expiry window.
8. **Notifications**: email to both guest and host at every step: enquiry received, message received, request submitted, accepted, declined, quote sent, quote accepted, expired and withdrawn.
9. **Admin inbox**: the admin portal lists open enquiries and requests. Each item shows its age, time to first host response, and time left before expiry, with a badge when it is close to expiring. The inbox also shows each host's median first-response time.
10. **Host earnings on an enquiry** (added 2026-11-07): the host's enquiry page shows an approximate amount they could earn from the stay: the listing's standard price for the dates and guests (length discounts and cleaning fee included, no promo) minus the platform commission, in the listing's base currency. It is labelled as an estimate, made before any custom quote, and is shown only to the host and admins.
11. **Unread messages in the left panel** (added 2026-11-07): the admin/host portal's left navigation shows the total number of unread messages from guests and potential guests next to Inbox ("Inbox (1)") across all of the host's listings (every listing, for an admin). It covers enquiry, request and booking threads, and clears as the host opens each thread.

---

## 3. Scope Boundaries
### In Scope (MVP)
- A `booking_mode` setting on listings in a new timestamped migration. Existing listings default to **instant book**, so behaviour doesn't change until a host opts in. It is editable in the admin listing form and enforced server-side: an instant hold on a request-only listing is refused with a typed error, and a request on an instant-only listing is refused the same way.
- Enquiry and request storage, with a request status lifecycle (e.g. `open → accepted / declined / quoted / expired / withdrawn / converted`). Every transition is written to an immutable history table.
- A message thread that can exist **before** a booking. Today `booking_message.booking_id` is `NOT NULL`, so the thread must be re-anchored on a **conversation per guest × listing**. Enquiries, requests, quotes and the resulting bookings for that pair all appear in that one thread.
- One live request per guest per listing. When a guest places an instant hold on a listing, any open request or offer they have on that listing is cancelled in the same transaction (mirroring hold supersession), with history and notifications.
- Changing a listing's booking mode leaves open requests and offers running until they are resolved or expire.
- A custom quote drops the guest's promo code (the host sets the final price). The quote says so.
- `booking_api` endpoints: create enquiry, create request (from scratch or from an enquiry), withdraw (guest), accept, decline with reason, send quote (host or admin), accept quote (guest), and list or get for the guest, host and admin views. Host ownership (`listing.user_id`) is enforced server-side, with 403 otherwise.
- Availability: a live request or quote blocks dates in **every** availability query, including the gap rule (`days_between_bookings`), listing search (#101), calendars, the instant hold check and reservation changes, in the same way a live `pending` hold does today.
- Sending a quote on an enquiry, which doesn't block dates, checks availability under the listing lock and then starts blocking the dates.
- Pricing: requests and quotes are priced through `stay_pricing::price_stay`. A custom quote overrides the stay price only, and the rest of the steps still run. The priced result is stored on the request, so the hold created at conversion carries exactly the price the guest accepted.
- Promo codes on requests: validated when the request is submitted and redeemed when the hold is created at conversion. These follow the #106 rules.
- Expiry: extend `run_sweeps`, and the `cleanup_stale_pending_holds` pg_cron job if it needs to cover the new rows. Availability must enforce expiry on read (`expires_at > NOW()`), so correctness never depends on a sweep having run.
- Email templates for each step through the existing outbox (`email_outbox`), deduplicated with `booking_notification_log` or an equivalent.
- Guest portal: booking-mode-aware CTAs on the listing and checkout pages, an enquiry form, a request form, a "My requests" view showing status, the quote and a countdown, quote acceptance into checkout, and the thread view.
- Admin/host portal: an inbox of open enquiries and requests with the response-time metrics described above, request detail, accept, decline and quote actions, and the thread.
- Admin/host portal: an estimated-earnings card on the enquiry page (host and admin only; never sent to the guest), and an unread-guest-messages count on the left-panel navigation.
- Updates to `openapi.yaml`, `openapi-prod.yaml`, `.sqlx/`, `docs/ARCHITECTURE.md` (request lifecycle section next to §Pending Hold Lifecycle), and Playwright coverage for the guest and admin portals.

### Out of Scope / Non-Goals
- **Add-ons**. The issue mentions them, but no add-on concept exists in the codebase (as also noted in #106). The request model must not prevent adding them later.
- Card authorization or pre-payment when a request is submitted. Payment happens only at checkout, after acceptance.
- Counter-offers that change dates or guest count. A quote changes price and terms only, so a guest who wants other dates submits a new request.
- Line-item quote overrides (nightly rate, cleaning fee or deposit edited separately).
- Signed-out enquiries or shadow-user enquiries. Sign-in is required, as at checkout.
- Automatic booking-mode rules (thresholds by guest count or nights).
- Guest-side negotiation over several rounds. Each request has at most one live quote, and a newer quote replaces the previous one.
- SMS, push or WhatsApp notifications. Email only.
- Auto-accept rules and host calendar sync.
- Rate limiting on enquiry or request creation (cross-reference #110).

---

## 4. Affected Systems & Stakeholders
- **User Personas**:
  - **Guest**: enquires, requests, accepts quotes, withdraws, and pays at checkout after acceptance.
  - **Host**: sets the booking mode, replies, accepts, declines and quotes on their own listings, and is measured on response time.
  - **Admin**: does everything a host can on any listing, and monitors the inbox and response times.
- **System Components**:

  | Component | Change |
  |---|---|
  | `db_core/migrations` | `listing.booking_mode`, enquiry/request/quote tables, history table, thread re-anchoring, availability index |
  | `db_core/src/booking/*` | Availability queries include live requests and quotes; atomic request → hold conversion; locking under the listing lock |
  | `db_core/src/models.rs` | `BookingMode`, request status enum with `can_transition_to`, DTOs |
  | `common` | Shared DTOs and typed error codes, `app_client` methods; pricing reused, with a custom stay-price override step |
  | `booking_api/apis` | New request/enquiry module, `bookings.rs` mode enforcement, `messages.rs` thread anchoring, `cron.rs` sweeps, `notifications.rs` templates, `routes.rs` |
  | `booking_api/config` | `request_response_hours` (default 24), `quote_valid_hours` (default 48), `request_near_expiry_pct` (default 25, the share of the window that counts as near expiry, driving the badge and the host reminder); all positive, validated at startup, env-overridable |
  | `listing_api` / admin listing form | Read and write `booking_mode` |
  | `web_ui_guest` | Listing and checkout CTAs, enquiry and request forms, My requests, thread |
  | `web_ui_admin` | Inbox, request detail and actions, booking mode on the listing form |

---

## 5. Constraints & Non-Negotiable Invariants
- **Domain Invariants**:
  - `rust_decimal::Decimal` only for quote prices, including form parsing. No `f32`/`f64`, and no `.unwrap()`/`.expect()`.
  - **Zero double-bookings**: creating a request, sending a quote on an enquiry, and converting to a hold all take the same `SELECT ... FOR UPDATE` listing lock as holds. They re-check availability under that lock and use `clock_timestamp()` for expiry checks.
  - **No release gap**: conversion of an offer to a `pending` hold happens in one transaction. The offer's block and the new hold never both lapse or both count at the same moment.
  - **Price integrity**: the hold created at conversion carries exactly the price the guest accepted (snapshot). A later rate, listing-price or exchange-rate change doesn't re-price an accepted request or quote. No fallback FX rates; `FX_RATE_UNAVAILABLE` applies (#100).
  - **Quote floor**: a custom stay price is strictly positive and within the booking money column limits. GCT and the platform fee are always recalculated, never typed in.
  - **Audit**: every request status change (and every quote issued) is written to an immutable history table with the actor and reason. Bookings created by conversion continue through `booking_history`.
  - **Typed refusals**: each refusal has its own error code, for example booking mode not allowed, dates unavailable, request expired, quote expired, request no longer open, or not the owner.
  - **Enquiries never block dates**. Only requests and quotes do.
  - Expiry is enforced when availability is read, and sweeps are only housekeeping (as with holds today).
- **Performance Budgets**: availability checks add only indexed lookups (a partial index on live requests). Cloud Run cold start stays within budget (<300ms p50, <1s p95).
- **Security & Compliance**:
  - Host ownership is enforced in the API on every action and every read. Guests can only see their own enquiries and requests.
  - Message text is user content: it is escaped in Topcoat and in email templates, and is not written in full to logs.
  - Accept, decline and quote actions are logged with the actor ID.
  - `CONSTRAINTS.md` applies in full, including doc comments on all new public items.
- **Microservices**: the work stays in `booking_api` (and `listing_api` for the listing field). No cross-service crate dependencies.

---

## 6. Success Metrics & Signals
- **Leading Indicators**:
  - Unit tests: request status transitions (`can_transition_to`), booking-mode rules, quote pricing (custom stay price, then fees and GCT recalculated) and the quote floor.
  - Test: an instant hold and a request racing for the same dates produce exactly one success.
  - Test: a live request or quote blocks dates in availability, search, the calendar and the instant hold. Once expired, the dates are free immediately, before any sweep.
  - Test: host acceptance blocks the dates without starting a hold. The guest proceeding to checkout creates a `pending` hold with the snapshotted price, and no other booking can claim the dates in between.
  - Test: an instant hold by the same guest on the same listing cancels their open request or offer, and a second live request on that listing is refused.
  - Test: a custom quote drops the promo code; the host reminder is sent exactly once, inside the configured near-expiry window.
  - Test: accepting an expired request or quote is refused with a typed error, and no hold is created.
  - Test: a quote on an enquiry whose dates were taken in the meantime is refused.
  - Test: an instant hold on a request-only listing is refused, and so is a request on an instant-only listing.
  - Test: a host gets 403 on another host's enquiry or request.
  - Test: the thread started on an enquiry is visible on the resulting booking.
  - Test: promo codes on requests follow #106 rules and are redeemed only when the request converts.
  - Test: every transition writes history, and every step enqueues the expected emails to both parties.
  - Test: the enquiry earnings estimate equals the shared host payout formula (`host_gross` then `calculate_host_payout` at the listing's commission) and is absent for the guest.
  - Test: the unread count includes only unread messages sent by guests, only on the host's own listings (all for an admin), and drops when a thread is marked read.
  - Database tests run against the local Postgres in the devenv shell, not Docker.
- **Lagging Indicators**:
  - Share of bookings that start as an enquiry or request.
  - Median host first-response time, and the share of requests that expire without a response.
  - Conversion rate from request to confirmed booking.
  - No double-bookings involving requests.

---

## 7. Open Questions & Policy Concerns
### Resolved (originator, 2026-11-05)
- [x] **"Both" mode**: the guest chooses between "Book now" and "Request to book". There are no threshold rules.
- [x] **Custom quote**: the stay price plus free-text terms. Fees, GCT and the platform fee are recalculated. Dates and guest count can't be changed.
- [x] **Enquiry auth**: sign-in is required.
- [x] **Responders**: the host for their own listings, the admin for any listing.
- [x] **Payment window after acceptance**: acceptance creates an offer that blocks dates for `quote_valid_hours`. The `pending_hold_minutes` hold starts only when the guest proceeds to checkout.
- [x] **Quote hold**: dates stay blocked until the quote expires (`quote_valid_hours`).
- [x] **Enquiry → request** in-thread: yes. **Quote on an enquiry**: yes. **Promo codes on requests**: yes.
- [x] **Response-time tracking**: per-item age, time to first response and time to expiry with a near-expiry badge, plus each host's median first-response time.

- [x] **Promo + custom quote**: the quote drops the promo code, and says so.
- [x] **Default windows**: `request_response_hours` = 24, `quote_valid_hours` = 48.
- [x] **One request per guest and listing**: one live request per guest per listing. An instant hold by that guest on the listing cancels their open request or offer.
- [x] **Decline reasons**: a fixed list plus optional free text, both shown to the guest.
- [x] **Switching booking mode**: open requests and offers continue until resolved or expired.
- [x] **Thread model**: one conversation per guest × listing.
- [x] **Near-expiry badge**: configurable (`request_near_expiry_pct`, default 25).
- [x] **Host reminder**: yes, one email per unanswered request when it enters the near-expiry window.

### Added (originator, 2026-11-07)
- [x] **Enquiry earnings**: the host's enquiry page shows an approximate amount they could earn from the stay.
- [x] **Unread messages**: the left panel shows the sum of all unread messages from guests and potential guests across all the host's listings.
