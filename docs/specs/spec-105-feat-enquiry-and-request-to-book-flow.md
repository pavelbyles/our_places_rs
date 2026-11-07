# Spec 105: Enquiry and Request-to-Book Flow

> Driven by GitHub Issue #105. Intent: [intent-105](../intent/intent-105-feat-enquiry-and-request-to-book-flow.md) (Approved 2026-11-05). Add-ons are out of scope (no add-on concept exists). Rate limiting is deferred to #110.

## Overview
Each listing chooses how it sells: **instant book**, **request to book**, or **both**. Any signed-in guest can also send an **enquiry**, which is a question with dates and a guest count. It starts a message thread with the host and blocks nothing. A **request to book** is priced when it is submitted and blocks its dates while the host decides. The host (or admin) can accept it, decline it with a reason, or send a **custom quote** with a different stay price and special terms. An accepted request or a quote becomes an **offer** that keeps the dates blocked. When the guest proceeds, the offer turns into a normal `pending` hold in one transaction, and the existing hold → payment → confirmed flow takes over from there. Anything left unanswered or not taken up expires through the existing sweeps. Every step emails both parties, and the admin portal has an inbox with response-time tracking.

**Personas**: Guest (enquires, requests, accepts offers, withdraws) · Host (sets the booking mode, responds on own listings, measured on response time) · Admin (everything a host can do, on any listing, and monitors response times).

### Design decisions (resolved during spec interrogation, 2026-11-05)
| # | Decision | Choice |
|---|---|---|
| D1 | Storage | New tables: `booking_enquiry`, `booking_request`, `booking_request_quote`, `booking_request_history`, `conversation`. The `booking` table and `BookingStatus` are not changed. |
| D2 | Promo use on requests | A live request that carries a code **reserves** a use in the derived usage count, the same way a live `pending` hold does. Conversion never fails on usage. The `promo_code_redemption` row is still written at conversion. |
| D3 | Quote currency | The host enters the custom stay price in the **listing's base currency**. It is converted at the request's **snapshotted** exchange rate. |
| D4 | Window cap | Every `expires_at` (response window and offer window) is capped at check-in day 00:00 `America/Jamaica`. |
| D5 | pg_cron | Not extended. Expiry is enforced when availability is read, and the emails need booking_api, so `run_sweeps` does the housekeeping. |
| D6 | Host rescind | The host can decline an `accepted` or `quoted` offer as well as an `open` request, to release blocked dates. |

All other policy points (defaults 24/48/25, one live request per guest × listing, decline reasons, thread model, near-expiry reminder) are as resolved in the intent §7.

---

## 1. Requirements

### 1.1. Booking Mode
1. `listing.booking_mode` ∈ `instant` | `request` | `both`. The default is `instant`, so existing listings keep their behaviour.
2. The mode is editable on the admin listing form (host: own listings; admin: any listing), and returned on listing DTOs (public and admin) so the portals can show the right buttons.
3. It is enforced server-side, under the listing row lock:
   - `POST /api/v1/bookings` (instant hold) on a `request` listing returns **409 `BOOKING_MODE_NOT_ALLOWED`**.
   - Creating a request on an `instant` listing returns **409 `BOOKING_MODE_NOT_ALLOWED`**.
   - Enquiries are allowed in every mode.
   - Converting an existing offer ignores the current mode, so changing the mode never cancels open requests or offers.

### 1.2. Enquiry
1. A signed-in guest sends `listing_id`, `check_in`, `check_out`, `guests`, `currency` (their display currency) and `message` (required, the same rules as booking messages).
2. Dates and guest count are validated the same way as a quote (`validate_stay_dates`, `check_guest_count`, inactive listing → 409 `LISTING_UNAVAILABLE`). Availability is **not** checked, and the dates are **not** blocked.
3. Creating the enquiry upserts the guest × listing `conversation` and posts the message into it.
4. There is at most **one open enquiry per guest × listing**. A new one closes the previous one (`closed_reason = superseded`) in the same transaction.
5. An enquiry closes (`status = closed`, with a `closed_reason`) when:

   | Trigger | `closed_reason` |
   |---|---|
   | Guest submits a request from it | `request_submitted` |
   | Host or admin sends a quote on it | `quoted` |
   | Host or admin closes it manually | `closed_by_host` |
   | Guest places an instant hold on that listing | `booked` |
   | Check-in date passes (sweep) | `expired` |
   | A newer enquiry replaces it | `superseded` |

### 1.3. Request to Book
1. A signed-in guest submits `listing_id`, `check_in`, `check_out`, `guests`, `currency`, an optional `promo_code`, an optional `message`, and an optional `enquiry_id` (prefill source; it must be the guest's own enquiry on the same listing).
2. The request is priced with `stay_pricing::price_stay` (`OnRefusal::Fail` for promo codes). The full priced result is stored as a `StayPriceSnapshot` (§3.2). Missing FX gives 422 `FX_RATE_UNAVAILABLE` with no fallback.
3. In one transaction, under the listing lock (lock order in §3.3):
   - Read `is_active` and `booking_mode`; refuse if inactive or `instant`.
   - Expire this guest's lapsed live rows on this listing inline, so the partial unique index can't trip on a request that is stale but not yet swept.
   - Refuse with **409 `REQUEST_ALREADY_LIVE`** if the guest still has a live request on this listing.
   - Check availability against confirmed bookings, other guests' live holds and other live requests, widened by the gap. The guest's **own** live `pending` hold on this listing doesn't conflict; it is cancelled (superseded) in the same transaction, with history.
   - If there is a promo code: `lock_and_count` → `check_status` → `check_usage` (the derived count now includes live requests, D2).
   - Insert the request with `status = open` and `expires_at = LEAST(now + request_response_hours, check-in 00:00 Jamaica)`. If the capped window is under 1 hour, refuse with **422 `REQUEST_WINDOW_TOO_SHORT`** (the guest should book instantly, if the listing allows it).
   - Close the source enquiry (`request_submitted`), write history, and post the optional message to the conversation.
4. There is at most **one live request** (`open`, `accepted`, `quoted`) per guest × listing, enforced by a partial unique index.

### 1.4. Host Response
Allowed for the listing owner (`listing.user_id`) and admins. Anyone else gets **403**.

| Action | From | To | Effect |
|---|---|---|---|
| Accept | `open` | `accepted` | Offer at the request's snapshot price. `expires_at = LEAST(now + quote_valid_hours, check-in cap)`. |
| Decline | `open`, `accepted`, `quoted` | `declined` | `decline_reason` (required, from the fixed list) plus `decline_note` (optional, ≤ 1000 chars). Both are shown to the guest. Dates are released. |
| Send quote | `open`, `quoted` | `quoted` | A new `booking_request_quote` row. Any previous live quote is marked `superseded_at`. `expires_at = LEAST(now + quote_valid_hours, cap)`. The promo is dropped (§1.5). |
| Quote on enquiry | enquiry `open` | new request `quoted` | Creates a request (`origin = enquiry_quote`) from the enquiry's dates, guests and currency, under the listing lock with a full availability check and the `REQUEST_ALREADY_LIVE` check. The enquiry closes (`quoted`). |
| Close enquiry | enquiry `open` | enquiry `closed` | `closed_by_host`. |

- Decline reasons (`DeclineReason`): `dates_unavailable`, `group_size`, `stay_length`, `event_not_permitted`, `other`. `other` requires a non-empty note.
- Acting on a row that isn't in an allowed state returns **409 `REQUEST_NOT_OPEN`**. Acting after `expires_at` (checked with `clock_timestamp()`) returns **409 `REQUEST_EXPIRED`**.
- Each action sets `first_response_at` if it is not already set, and logs `actor_id` (§5).

### 1.5. Custom Quote Pricing
1. Input: `stay_price` (Decimal, base currency, `> 0`, at most 2 dp; parsed from a string, never a float) and `terms` (free text, ≤ 2000 chars, may be empty).
2. Pricing: the same pipeline as `price_stay`, with two changes:
   - **Locked rate**: a quote on a request uses the request snapshot's `exchange_rate`. A quote on an enquiry has no snapshot, so it uses the current `RateUse::NewQuote` rate, and that rate is then snapshotted.
   - **Stay override**: after `price_before_promo` (which still validates dates, minimum stay and the guest count), the new step `BookingCalculator<Discounted>::with_custom_stay_price(price_in_payment_ccy)` replaces the accommodation subtotal (seasonal nights, extra-guest fees and weekly/monthly discount). Then `apply_promo(None)` → `apply_taxes` → `finalize` recalculate the cleaning fee, deposit, GCT, platform fee and total as usual. GCT and the platform fee can never be typed in.
3. **Quote floor**: `stay_price > 0`; the converted price `> 0` after rounding; and `check_amounts_fit` passes. Otherwise **400 `INVALID_QUOTE_PRICE`**.
4. **Promo dropped**: the quote snapshot has no promo, and the quote response, email and UI state "Your promo code doesn't apply to a custom quote." The code string is never echoed (#106 log rule) The request's reserved use is released, because a `quoted` request no longer counts (§3.3 usage predicate).
5. `daily_rate` on the resulting booking is `round(stay_total / nights, 2)`, for display only. `sub_total_price = discounted_subtotal = stay total`, and `discount_value = None`. `host_gross` works unchanged on these fields.
6. Preview: `POST …/quote-preview` returns the priced breakdown without saving anything, so the admin form can show the guest total before sending.

### 1.6. Conversion to Booking
1. The guest calls `POST /api/v1/booking-requests/{id}/checkout` with `{ quote_id: Option<Uuid> }`. A `quoted` request must name its current quote (otherwise **409 `QUOTE_SUPERSEDED`**), so the guest only ever pays the price they saw.
2. One transaction, with the lock order from §3.3:
   - Listing `FOR UPDATE`; refuse if inactive (409 `LISTING_UNAVAILABLE`).
   - Request `FOR UPDATE`; it must be `accepted` or `quoted`, owned by the caller (otherwise 403), and `expires_at > clock_timestamp()` (otherwise **409 `REQUEST_EXPIRED`** / **`QUOTE_EXPIRED`**).
   - Currency `ensure_active`.
   - Re-check availability, excluding this request. This is defence in depth: the request has blocked the dates all along.
   - Cancel the guest's other `pending` holds (as `create_booking` does today).
   - Promo (only on an `accepted` request with a code): `lock_code`, then insert the redemption **without** `check_usage`, because the use was reserved at submission (D2).
   - Insert the `pending` booking from the snapshot, with `expires_at = now + pending_hold_minutes`, and write `booking_history` ("Booking created from request {id}").
   - Request → `converted`, with `booking_id` set and history written.
3. The request stops blocking and the booking starts blocking **in the same commit**, so there is never a gap or a double count.
4. Response: 201 `BookingResponse`. The portal redirects to `/bookings?new_booking=true`, the existing payment entry point.
5. If the resulting hold later expires unpaid, it is released like any other hold. The request stays `converted`, and the guest has to submit a new request.

### 1.7. Withdrawal, Supersession & Expiry
1. **Withdraw** (guest): from `open`, `accepted` or `quoted` → `withdrawn`. The dates are released at once.
2. **Instant hold by the same guest on the same listing**: the guest's live request on that listing → `cancelled` (reason `superseded_by_instant_booking`) and their open enquiry → `closed` (`booked`), both inside `create_booking`'s transaction, with history and emails.
3. **Expiry**: `run_sweeps` gains `expire_stale_requests` (live and `expires_at <= NOW()`, `FOR UPDATE SKIP LOCKED` → `expired` + history) and `close_past_enquiries` (open, `date_from <= today` in Jamaica → `closed/expired`). Expired requests send emails to the guest and host, claimed in `request_notification_log` so a re-run never sends twice.
4. **Host reminder**: `run_notification_sweep` emails the host **once** per `open` request when `expires_at - NOW() <= window × request_near_expiry_pct / 100` and `expires_at > NOW()`, where `window = expires_at - created_at`. It is claimed in `request_notification_log` (`request_host_reminder`).
5. Correctness never depends on a sweep: every availability read filters on `expires_at > NOW()`, and every action re-checks with `clock_timestamp()`.

### 1.8. Conversation (Thread)
1. There is one `conversation` per (listing, guest). Enquiries, requests, quotes and the bookings for that pair all post into it.
2. `booking_message` gains `conversation_id NOT NULL` (backfilled), and `booking_id` becomes nullable. New nullable context tags `enquiry_id` and `request_id` label what a message relates to, and the UI shows them as small badges.
3. The existing `/api/v1/bookings/{id}/messages` endpoints keep their contract, but read and write the booking's conversation, so a thread started on an enquiry is visible on the booking. The existing rule stays: no new message through the booking endpoint while that booking is `cancelled`.
4. New `/api/v1/conversations/{id}/messages` (GET, POST) and `…/read` (PATCH) are for the enquiry and request views. Access is limited to the guest, the listing owner and admins.
5. `transfer_booking_guest` (shadow user promotion) moves that booking's messages to the new guest's conversation (upserted) in the same transaction.
6. Posting sets `first_response_at` on the guest's open enquiry or live request for that pair, if the poster is the host or an admin and the field is still null.
7. `conversation.last_message_at` is updated on every post, for inbox ordering.

### 1.9. Notifications
Every email goes through `email_outbox` via `enqueue_email`, after commit. User-supplied fields are passed through `escape_html` when rendered. Templates (`common::email::EmailTemplate` + `app_api/email_worker/templates/`):

| Event | Guest | Host |
|---|---|---|
| Enquiry sent | `enquiry_sent_guest` | `enquiry_received_host` |
| Message received | `guest_host_message_notification` (existing, with the link now to the thread) | same |
| Request submitted | `request_submitted_guest` | `request_received_host` |
| Accepted | `request_accepted_guest` (with the offer expiry) | `request_accepted_host` |
| Declined | `request_declined_guest` (reason + note) | `request_declined_host` |
| Quote sent | `quote_sent_guest` (price, terms, expiry, promo-dropped notice) | `quote_sent_host` |
| Converted / quote accepted | `request_converted_guest` (proceed to pay) | `request_converted_host` |
| Expired | `request_expired_guest` | `request_expired_host` |
| Withdrawn / superseded | `request_withdrawn_guest` | `request_withdrawn_host` |
| Near-expiry reminder | — | `request_host_reminder` |

Admin actions email the host and the guest in the same way. Scheduled emails (reminder, expiry) are deduplicated in `request_notification_log`. Event emails are sent once per committed transition, because the state machine refuses repeated transitions.

### 1.10. Admin Inbox & Response-Time Tracking
1. `/admin/inbox` lists open enquiries and live requests (host: own listings; admin: all). Filters: listing, kind, status. Ordered by `expires_at` ascending (requests first), then `last_message_at`. Paginated at 50 per page.
2. Each row shows the kind, listing, guest first name, dates, guests, status, **age** (`now - created_at`), **time to first response** (`first_response_at - created_at`, or "awaiting"), **time left** (`expires_at - now`, requests only), and a **near-expiry badge** when time left ≤ `request_near_expiry_pct`% of the window.
3. A header card shows each host's **median first-response time** over items created in the last 90 days (`percentile_cont(0.5)`), plus the count and share of requests that expired unanswered. Hosts see only their own; admins see every host.
4. **Estimated earnings on an enquiry** (added 2026-11-07). `/admin/enquiries/{id}` shows what the host could earn from the stay asked about:
   - **Pricing**: the enquiry's dates and guests are priced through `price_stay` in the listing's **base currency** (`locked_rate = 1`, so no FX lookup), with no promo and no stay override. Nightly overrides, length discounts, the extra-guest fee and the cleaning fee all apply, as for a request.
   - **Payout**: `gross = host_gross(discounted_subtotal, cleaning_net, 1)`, then `(commission, net) = calculate_host_payout(gross, listing.commission_pct, 0)`. This is the formula the payout ledger uses, so the estimate matches what a booking at standard rates would pay out. GCT, the guest platform fee and the damage deposit are never part of it.
   - **Display**: a card with the stay subtotal, cleaning fee, gross, commission (with its %) and the estimated payout, labelled "Approximate: standard rates for these dates, before any custom quote".
   - **Who sees it**: the host and admins only. `GET /api/v1/enquiries/{id}` returns `host_estimate` for those roles; the guest's response never includes it, because commission is the host's business.
   - **Can't price**: if the stay can't be priced (dates now in the past, stay too long, listing unavailable), `host_estimate` is `null`, the API logs a `warn!` with the refusal code, and the page says "An estimate isn't available for these dates". The enquiry itself still loads.
5. **Unread guest messages in the left panel** (added 2026-11-07). The admin portal's left navigation (desktop sidebar and mobile menu) shows the number of unread messages from guests next to **Inbox**, as "Inbox (1)":
   - **Counted**: `booking_message` rows with `read_at IS NULL` and `sender_id = conversation.guest_id`, on conversations whose listing the caller owns (every listing, for an admin). This covers enquiry, request and booking threads, since they share the guest × listing conversation. Messages from hosts and admins never count.
   - **Clearing**: opening a thread marks it read (the existing `…/messages/read` endpoints, which mark the whole conversation), so the count drops on the next page load.
   - **Display**: "Inbox" alone at 0, "Inbox (n)" up to 99, "Inbox (99+)" above, with `aria-label="N unread messages"` on the count.
   - **API**: `GET /api/v1/conversations/unread-count`, for hosts and admins (403 otherwise), returns `{ "unread_messages": n }`. It is one `COUNT` on `idx_booking_message_conv_unread`.
   - **Failure**: if the count can't be fetched, the shell renders plain "Inbox" and logs a `warn!`. A page never fails because of the count.

### 1.11. Exclusions / Non-Goals
As listed in intent §3: add-ons (the snapshot struct and request table leave room for them later), pre-authorisation of cards, changing dates or guests in a quote, line-item overrides, signed-out or shadow-user enquiries, auto-mode rules, more than one live quote per request, non-email channels, auto-accept, calendar sync, and rate limiting (#110). Extending the `cleanup_stale_pending_holds` pg_cron job is also out of scope (D5).

---

## 2. Edge Cases & Failure States

| # | Scenario | Behaviour |
|---|---|---|
| E1 | Instant hold and request race for the same dates | Both take the listing lock; the second re-reads, sees the first, and gets 409 `DATE_UNAVAILABLE`. Exactly one succeeds. |
| E2 | Two requests from the same guest for the same listing at once | Listing lock serialises them; the second gets 409 `REQUEST_ALREADY_LIVE` (the partial unique index is the backstop, mapped to the same code). |
| E3 | Host accepts while the guest withdraws | The request row lock serialises them. If the withdrawal lands first, the accept gets 409 `REQUEST_NOT_OPEN`; if the accept lands first, the withdrawal then withdraws the offer. Two terminal moves (decline vs withdraw) have exactly one winner. |
| E4 | Accept or convert arrives just after `expires_at`, before any sweep | `clock_timestamp()` check → 409 `REQUEST_EXPIRED` / `QUOTE_EXPIRED`; no hold is created. |
| E5 | Sweep runs while an action holds the row | `SKIP LOCKED`; the next sweep picks it up, or the action's state change wins. |
| E6 | Guest converts an old quote while the host sends a new one | Request lock serialises; a stale `quote_id` → 409 `QUOTE_SUPERSEDED`. |
| E7 | Host quotes on an enquiry whose dates were taken meanwhile | Availability check under the lock → 409 `DATE_UNAVAILABLE`; the enquiry stays open. |
| E8 | Host quotes on an enquiry, but the guest already has a live request on the listing | 409 `REQUEST_ALREADY_LIVE`; the host should respond to that request. |
| E9 | Check-in is near: the response window would run past check-in | Capped at check-in 00:00 Jamaica (D4). A capped window under 1h → 422 `REQUEST_WINDOW_TOO_SHORT` at submission. Accept and quote also cap; if the cap is already past, → 409 `REQUEST_EXPIRED`. |
| E10 | Listing deactivated with live requests or offers | They keep blocking until resolved or expired; new requests, accepts, quotes and conversion get 409 `LISTING_UNAVAILABLE`. Decline and withdraw still work. |
| E11 | Listing mode switched to `instant` with open requests | They continue, and can be accepted, quoted and converted (intent). |
| E12 | Currency deactivated or pair retired after submission | Accept uses the snapshot (no FX lookup). Conversion `ensure_active` → 422 `CURRENCY_INACTIVE`. A quote on a request uses the snapshot rate; a quote on an enquiry needs a `NewQuote` rate (422 `FX_RATE_UNAVAILABLE`). |
| E13 | Rate, listing price or override changed after submission | Accept and conversion use the snapshot unchanged. A quote recomputes cleaning fee and deposit from the current listing (a new price the guest sees before accepting). |
| E14 | Promo code deactivated or limit reached after submission | The use is already reserved (D2). Conversion inserts the redemption without re-checking status or usage, and the snapshot terms apply. |
| E15 | The converted hold expires unpaid | Normal hold expiry; the request stays `converted`, and the guest can submit a new request. |
| E16 | Guest has a pending hold on listing A and submits a request on A | The own hold is cancelled (superseded) in the request's transaction, with history. |
| E17 | Guest has a live request on A and instant-books listing B | Unaffected; supersession is per listing. Their pending holds on other listings are cancelled by conversion or `create_booking`, as today. |
| E18 | Shadow-user booking promoted to a full account | Messages move to the new guest's conversation (§1.8.5). Shadow users can't create enquiries or requests (an active, verified account is needed, as at checkout). |
| E19 | Host is also the guest (owner enquires on their own listing) | Refused with 403: the API rejects `listing.user_id == caller` for enquiry and request creation. |
| E20 | Message contains control characters or is empty | 400, the same as booking messages. |
| E21 | Email publish fails | The outbox row exists and is retried by email_worker; the API response is unaffected (existing detached publish). |
| E22 | Re-running sweeps | Transitions are idempotent (rows are no longer live), and emails are claimed in the log first. |
| E23 | Gap rule | Live requests are widened by `days_between_bookings` like bookings, in every query (§3.3). |
| E24 | Reservation change on another booking into dates held by a live request | `check_availability_excluding` now sees requests → 409 `DATE_UNAVAILABLE`. |

---

## 3. Technical Implementation

### 3.1. Data Model (one new migration: `sqlx migrate add enquiries_and_booking_requests`)
The last migration (`20261102055752_promo_codes`) is already on `main`, so this is a new file. No existing migration is edited.

```sql
CREATE TYPE booking_mode AS ENUM ('instant', 'request', 'both');
ALTER TABLE listing ADD COLUMN booking_mode booking_mode NOT NULL DEFAULT 'instant';
ALTER TABLE listing_history ADD COLUMN booking_mode booking_mode;

CREATE TABLE conversation (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    listing_id UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    guest_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_message_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_conversation_pair UNIQUE (listing_id, guest_id)
);

CREATE TYPE enquiry_status AS ENUM ('open', 'closed');
CREATE TYPE enquiry_closed_reason AS ENUM
    ('request_submitted', 'quoted', 'closed_by_host', 'booked', 'expired', 'superseded');
CREATE TABLE booking_enquiry (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    conversation_id UUID NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    listing_id UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    guest_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    date_from DATE NOT NULL, date_to DATE NOT NULL CHECK (date_to > date_from),
    number_of_persons INTEGER NOT NULL CHECK (number_of_persons >= 1),
    currency CHAR(3) NOT NULL REFERENCES currencies(code) ON UPDATE RESTRICT ON DELETE RESTRICT,
    status enquiry_status NOT NULL DEFAULT 'open',
    closed_reason enquiry_closed_reason,
    first_response_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK ((status = 'closed') = (closed_reason IS NOT NULL))
);
CREATE UNIQUE INDEX uq_enquiry_open_pair ON booking_enquiry (listing_id, guest_id) WHERE status = 'open';
CREATE INDEX idx_enquiry_open_listing ON booking_enquiry (listing_id, created_at) WHERE status = 'open';

CREATE TYPE booking_request_status AS ENUM
    ('open', 'accepted', 'quoted', 'declined', 'expired', 'withdrawn', 'cancelled', 'converted');
CREATE TYPE booking_request_origin AS ENUM ('guest_request', 'enquiry_quote');
CREATE TYPE decline_reason AS ENUM
    ('dates_unavailable', 'group_size', 'stay_length', 'event_not_permitted', 'other');
CREATE TABLE booking_request (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    conversation_id UUID NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    listing_id UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    guest_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    enquiry_id UUID REFERENCES booking_enquiry(id) ON DELETE SET NULL,
    origin booking_request_origin NOT NULL,
    status booking_request_status NOT NULL DEFAULT 'open',
    date_from DATE NOT NULL, date_to DATE NOT NULL CHECK (date_to > date_from),
    number_of_persons INTEGER NOT NULL CHECK (number_of_persons >= 1),
    currency CHAR(3) NOT NULL REFERENCES currencies(code) ON UPDATE RESTRICT ON DELETE RESTRICT,
    price_snapshot JSONB NOT NULL,              -- common::booking_request::StayPriceSnapshot (as requested)
    promo_code_id UUID REFERENCES promo_code(id) ON DELETE RESTRICT,  -- reserved use while open/accepted (D2)
    current_quote_id UUID,                      -- FK added after quote table
    expires_at TIMESTAMPTZ NOT NULL,
    first_response_at TIMESTAMPTZ,
    decline_reason decline_reason,
    decline_note TEXT CHECK (char_length(decline_note) <= 1000),
    booking_id UUID REFERENCES booking(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK ((status = 'declined') = (decline_reason IS NOT NULL))
    -- `converted` ⇒ booking_id is set at write time; not a CHECK, because a feature-flagged hard delete of the booking SETs it NULL.
);
CREATE UNIQUE INDEX uq_request_live_pair ON booking_request (listing_id, guest_id)
    WHERE status IN ('open', 'accepted', 'quoted');
CREATE INDEX idx_request_live_dates ON booking_request (listing_id, date_from, date_to, expires_at)
    WHERE status IN ('open', 'accepted', 'quoted');
CREATE INDEX idx_request_live_expiry ON booking_request (expires_at)
    WHERE status IN ('open', 'accepted', 'quoted');
CREATE INDEX idx_request_promo_live ON booking_request (promo_code_id)
    WHERE status IN ('open', 'accepted') AND promo_code_id IS NOT NULL;
CREATE INDEX idx_request_guest ON booking_request (guest_id, created_at DESC);

CREATE TABLE booking_request_quote (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    request_id UUID NOT NULL REFERENCES booking_request(id) ON DELETE CASCADE,
    stay_price_base NUMERIC(12, 2) NOT NULL CHECK (stay_price_base > 0),
    terms TEXT NOT NULL DEFAULT '' CHECK (char_length(terms) <= 2000),
    price_snapshot JSONB NOT NULL,              -- quoted price, promo-free
    valid_until TIMESTAMPTZ NOT NULL,
    superseded_at TIMESTAMPTZ,
    created_by UUID NOT NULL REFERENCES "user"(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
ALTER TABLE booking_request ADD CONSTRAINT fk_request_current_quote
    FOREIGN KEY (current_quote_id) REFERENCES booking_request_quote(id) ON DELETE SET NULL;

-- Immutable lifecycle log for enquiries and requests (append-only; never UPDATEd or DELETEd in code).
CREATE TABLE booking_request_history (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    enquiry_id UUID REFERENCES booking_enquiry(id) ON DELETE CASCADE,
    request_id UUID REFERENCES booking_request(id) ON DELETE CASCADE,
    from_status TEXT,                -- NULL on creation
    to_status TEXT NOT NULL,
    quote_id UUID REFERENCES booking_request_quote(id) ON DELETE SET NULL,
    actor_id UUID REFERENCES "user"(id) ON DELETE SET NULL,  -- NULL = system (sweep)
    actor_role message_sender_role,
    reason TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (num_nonnulls(enquiry_id, request_id) = 1)
);
CREATE INDEX idx_request_history_request ON booking_request_history (request_id, created_at);
CREATE INDEX idx_request_history_enquiry ON booking_request_history (enquiry_id, created_at);

CREATE TABLE request_notification_log (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    request_id UUID NOT NULL REFERENCES booking_request(id) ON DELETE CASCADE,
    notification_type VARCHAR(64) NOT NULL,
    recipient_user_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_request_notification UNIQUE (request_id, notification_type, recipient_user_id)
);

-- Thread re-anchoring: one conversation per (listing, guest).
INSERT INTO conversation (listing_id, guest_id, created_at, last_message_at)
SELECT b.listing_id, b.guest_id, MIN(m.created_at), MAX(m.created_at)
FROM booking_message m JOIN booking b ON b.id = m.booking_id
GROUP BY b.listing_id, b.guest_id
ON CONFLICT (listing_id, guest_id) DO NOTHING;
ALTER TABLE booking_message
    ADD COLUMN conversation_id UUID REFERENCES conversation(id) ON DELETE CASCADE,
    ADD COLUMN enquiry_id UUID REFERENCES booking_enquiry(id) ON DELETE SET NULL,
    ADD COLUMN request_id UUID REFERENCES booking_request(id) ON DELETE SET NULL;
UPDATE booking_message m SET conversation_id = c.id
FROM booking b JOIN conversation c ON c.listing_id = b.listing_id AND c.guest_id = b.guest_id
WHERE b.id = m.booking_id;
ALTER TABLE booking_message ALTER COLUMN conversation_id SET NOT NULL,
                            ALTER COLUMN booking_id DROP NOT NULL;
CREATE INDEX idx_booking_message_conversation ON booking_message (conversation_id, created_at);
CREATE INDEX idx_booking_message_conv_unread ON booking_message (conversation_id) WHERE read_at IS NULL;
```

- The `booking_message.booking_id` FK stays `ON DELETE CASCADE`. A hard-deleted booking still deletes its tagged messages, as today.
- Statuses in the history table are `TEXT`, so one table can log both lifecycles. The domain enums own validation.

### 3.2. Domain (`common`)
New module `common/src/booking_request.rs` (fully documented, unit-tested, no I/O):
- `BookingMode { Instant, Request, Both }` with `allows_instant()` and `allows_request()`.
- `RequestStatus` with `is_live()` and `can_transition_to(to)`, implementing the table in §1.4 and §1.7 (terminal: `declined`, `expired`, `withdrawn`, `cancelled`, `converted`; `quoted → quoted` allowed for a replacement quote).
- `EnquiryStatus`, `EnquiryClosedReason`, `DeclineReason` (with `requires_note()`).
- `StayPriceSnapshot`: every money field `NewBooking` needs (`daily_rate`, `total_days`, `sub_total_price`, `discount_value`, `cleaning_fee`, `damage_deposit`, `platform_fee`, `tax_rate`, `tax_value`, `fee_breakdown`, `total_price`, `exchange_rate`, `currency`, `cancellation_policy`) plus `promo: Option<{promo_code_id, terms: PromoTerms, stay_discount, cleaning_discount}>`. Decimals use the serde string representation already used by `promo_terms`. `from_calculator(&BookingCalculator<Taxed>, …)` is the only constructor.
- `RequestRefusal` enum with `code()` and `message()`: `BookingModeNotAllowed`, `RequestAlreadyLive`, `RequestNotOpen`, `RequestExpired`, `QuoteExpired`, `QuoteSuperseded`, `InvalidQuotePrice`, `RequestWindowTooShort`, `DeclineNoteRequired`, `OwnListing`.
- `response_window(now, requested_hours, check_in) -> Result<DateTime<Utc>, RequestRefusal>`: applies the D4 cap (check-in 00:00 Jamaica = 05:00 UTC; Jamaica is UTC-5 with no DST, as `common::cancellation` already assumes) and the 1h minimum.
- `is_near_expiry(created_at, expires_at, now, pct) -> bool`, shared by the badge and the reminder query's unit tests.
- DTOs: `CreateEnquiryRequest`, `CreateBookingRequestRequest`, `DeclineRequestBody`, `QuoteRequestBody { stay_price: Decimal (string), terms }`, `CheckoutRequestBody { quote_id }`, `EnquiryResponse`, `BookingRequestResponse` (with the current quote and the price breakdown), `InboxItem`, `InboxResponse { items, host_metrics }`, `HostResponseMetrics`, `ConversationMessagesWrapper`. All are `validator`-annotated and `utoipa::ToSchema`.
- `common::pricing`: `impl BookingCalculator<Discounted> { pub fn with_custom_stay_price(self, stay_total: Decimal) -> Self }`. It sets `sub_total_price = discounted_subtotal = stay_total`, `discount_value = None`, `extra_guest_fee_total = 0` and `actual_daily_rate = round(stay_total / days, 2)`.
- `common::models::Listing` (and the create/update DTOs) gain `booking_mode: BookingMode` with `#[serde(default)]`.
- `common::email::EmailTemplate` gains the variants in §1.9.
- `common/src/app_client/booking_requests.rs`: typed client methods for every endpoint in §3.4.

### 3.3. Database Layer (`db_core`)
New modules `db_core/src/booking_request/{mod,enquiry,request,convert,expiry,inbox,tests}.rs` and `db_core/src/conversation.rs`. `booking_message.rs` is re-pointed at conversations.

**Lock order** (each transaction takes a subset in this order): `listing FOR UPDATE` → `booking_request FOR UPDATE` → `currencies` (`ensure_active`) → the guest's `pending` bookings → `promo_code FOR UPDATE`. This extends §4.2.2's order without reordering it.

**Live-request predicate** (copied into each query because `sqlx::query!` needs literals; parity tests keep the copies in step):
```sql
r.status IN ('open','accepted','quoted') AND r.expires_at > NOW()
  AND r.date_from - LEAST(GREATEST(l.days_between_bookings, 0), 365) < $check_out
  AND r.date_to   + LEAST(GREATEST(l.days_between_bookings, 0), 365) > $check_in
```

| Query | Change |
|---|---|
| `availability::check_availability` / `check_availability_excluding` | `+ NOT EXISTS (live request)`; `_excluding` stays booking-only for its booking id. A new `check_availability_excluding_request(…, request_id)` is used by conversion. |
| `availability::get_unavailable_ranges` | `UNION ALL` the live requests, widened by the gap, then `ORDER BY date_from`. |
| `hold::create_booking` | Reads `booking_mode` with `is_active` under the lock → `BookingModeNotAllowed`. The overlap check adds live requests from **other** guests. The same guest's live request on this listing → `cancelled`, and their open enquiry → `closed/booked`, both with history. The tx returns the superseded ids so the handler can send emails. |
| `listing::queries::push_stay_conditions` | `+ AND NOT EXISTS (SELECT 1 FROM booking_request r WHERE r.listing_id = listing.id AND <live predicate>)`. |
| `reservation_change` availability | Inherits from `check_availability_excluding`. |
| `promo::lock_and_count` | Usage `+` live `booking_request` rows with `promo_code_id = $1 AND status IN ('open','accepted') AND expires_at > NOW()` (`total` and per-guest), as a second `COUNT` in the same statement. |
| `booking::hold::transfer_booking_guest` | Upserts the new guest's conversation and moves the booking's messages to it. |

New functions (each `#[tracing::instrument(skip(pool, …))]`, returning `db_core::error::Result`):
- `create_enquiry`, `close_enquiry`, `create_request`, `accept_request`, `decline_request`, `withdraw_request`, `send_quote` (on a request), `quote_enquiry`, `convert_offer_to_hold`. Each runs in one transaction and writes `booking_request_history`. Each returns the updated row plus the notification facts (guest and host ids, listing name) in one round trip, with no follow-up selects in a loop.
- `expire_stale_requests(pool) -> Vec<ExpiredRequest>`, `close_past_enquiries(pool, today) -> u64`, `requests_due_for_reminder(pool, pct) -> Vec<ReminderCandidate>`, `claim_request_notification(pool, request_id, kind, recipient) -> bool`.
- `inbox_items(pool, scope: InboxScope { host_id: Option<Uuid> }, filter, page)`: one query (a `UNION ALL` of enquiries and requests joined to `listing` and `user` for names), with `LIMIT/OFFSET`.
- `host_response_metrics(pool, host_id: Option<Uuid>)`: one aggregate grouped by `listing.user_id` over 90 days.
- `conversation::{upsert, get_with_parties, list_messages, insert_message, mark_read}`. `insert_message` also sets `first_response_at` and `last_message_at` in the same tx.
- `DbError::RequestRefused(common::booking_request::RequestRefusal)`. A unique violation on `uq_request_live_pair` is mapped to `RequestAlreadyLive`, and one on `uq_enquiry_open_pair` is retried once after closing the prior enquiry.

`db_core::models`: `DbBookingRequest`, `DbBookingEnquiry`, `DbRequestQuote`, `DbConversation`, and the `sqlx::Type` enums mirroring §3.2 (`BookingMode`, `RequestStatus`, …), with `From` into the `common` DTOs. `DbBookingMessage` gains `conversation_id`, `enquiry_id` and `request_id`, and its `booking_id` becomes `Option<Uuid>`.

### 3.4. API (`booking_api`)
New module `apis/booking_requests.rs` (handlers) and `apis/conversations.rs`. Routes are wired in `routes.rs` and registered in the OpenAPI document. All routes take `api_core::auth::Claims` and resolve the active user through the existing current-user helper (401 if deactivated).

| Method & Path | Caller | Success | Notable errors |
|---|---|---|---|
| `POST /api/v1/enquiries` | Guest | 201 `EnquiryResponse` | 400 stay input / message, 403 own listing, 404 listing, 409 `LISTING_UNAVAILABLE` |
| `GET /api/v1/enquiries/{id}` | Guest (own), host (own listing), admin | 200 (`host_estimate` for host and admin only, §1.10.4) | 403, 404 |
| `POST /api/v1/enquiries/{id}/close` | Host / admin | 200 | 403, 409 `REQUEST_NOT_OPEN` |
| `POST /api/v1/enquiries/{id}/quote` | Host / admin | 201 `BookingRequestResponse` | 400 `INVALID_QUOTE_PRICE`, 403, 409 `DATE_UNAVAILABLE` / `REQUEST_ALREADY_LIVE` / `REQUEST_NOT_OPEN`, 422 `FX_RATE_UNAVAILABLE` |
| `POST /api/v1/booking-requests` | Guest | 201 `BookingRequestResponse` | 400 stay input, 403 own listing, 409 `BOOKING_MODE_NOT_ALLOWED` / `DATE_UNAVAILABLE` / `REQUEST_ALREADY_LIVE` / `LISTING_UNAVAILABLE`, 422 `PROMO_*` / `FX_RATE_UNAVAILABLE` / `REQUEST_WINDOW_TOO_SHORT`, 503 lock timeout |
| `GET /api/v1/booking-requests/{id}` | Party / admin | 200 | 403, 404 |
| `GET /api/v1/booking-requests/mine` | Guest | 200 list of the guest's enquiries + requests (newest first, 50) | — |
| `POST /api/v1/booking-requests/{id}/withdraw` | Guest (own) | 200 | 403, 409 `REQUEST_NOT_OPEN` |
| `POST /api/v1/booking-requests/{id}/accept` | Host / admin | 200 | 403, 409 `REQUEST_NOT_OPEN` / `REQUEST_EXPIRED` / `LISTING_UNAVAILABLE` |
| `POST /api/v1/booking-requests/{id}/decline` | Host / admin | 200 | 400 `DECLINE_NOTE_REQUIRED`, 403, 409 `REQUEST_NOT_OPEN` |
| `POST /api/v1/booking-requests/{id}/quote-preview` | Host / admin | 200 price breakdown | 400 `INVALID_QUOTE_PRICE`, 403 |
| `POST /api/v1/booking-requests/{id}/quote` | Host / admin | 201 | 400, 403, 409 `REQUEST_NOT_OPEN` / `REQUEST_EXPIRED` |
| `POST /api/v1/booking-requests/{id}/checkout` | Guest (own) | 201 `BookingResponse` | 403, 409 `REQUEST_NOT_OPEN` / `REQUEST_EXPIRED` / `QUOTE_EXPIRED` / `QUOTE_SUPERSEDED` / `LISTING_UNAVAILABLE` / `DATE_UNAVAILABLE`, 422 `CURRENCY_INACTIVE` |
| `GET /api/v1/inbox?kind&status&listing_id&page` | Host (own listings) / admin (all) | 200 `InboxResponse` | 403 for non-staff |
| `GET /api/v1/conversations/{id}/messages` | Party / admin | 200 `ConversationMessagesWrapper` | 403, 404 |
| `POST /api/v1/conversations/{id}/messages` | Party / admin | 201 | 400, 403 |
| `PATCH /api/v1/conversations/{id}/messages/read` | Party / admin | 200 | 403 |
| `GET /api/v1/conversations/unread-count` | Host (own listings) / admin (all) | 200 `UnreadMessagesResponse` (§1.10.5) | 403 for non-staff |

- **Error payloads**: `{ "code", "message" }` via a new `ApiError::Database(DbError::RequestRefused(r))` arm in `api_core::error`, mirroring `PromoRefused` (status from `r.http_status()`: 400, 403, 409 or 422). `warn!` with `code`, `request_id` and `actor_id` on every refusal.
- **Authorization helper**: `authorize_request_party(req, pool, request_id) -> (RequestParties, User, PartyRole)`, the counterpart of `authorize_booking_party`. Host actions require `PartyRole::Host | Admin`, and guest actions require `PartyRole::Guest`.
- `bookings.rs::create_booking`: maps `BookingModeNotAllowed` and sends supersession emails for any cancelled request.
- `stay_pricing.rs`: `StayRequest` gains `rate: RateSource { Current, Locked(Decimal) }` and `stay_override: Option<Decimal>` (base currency). `price_stay` branches only at the rate lookup and between `price_before_promo` → `with_custom_stay_price`. `stay_override` forces `promo = None`.
- `messages.rs`: re-pointed to the booking's conversation; the request and response contract is unchanged.
- `cron.rs::run_sweeps`: `+ expire_stale_requests`, `+ close_past_enquiries`. The response gains `requests_expired` and `enquiries_closed`. `run_notification_sweep` gains `request_reminders_processed`.
- `notifications.rs`: `dispatch_request_notifications(pool, event, facts)`. One function maps an event to its guest and host templates and payloads (§1.9).
- **Config** (`api_core::settings::ApplicationSettings`, `config/Default.toml`, validated positive in `main.rs` next to `pending_hold_minutes`):

  | Key | Default | Env override | Valid |
  |---|---|---|---|
  | `request_response_hours` | 24 | `EA__APPLICATION__REQUEST_RESPONSE_HOURS` | 1–720 |
  | `quote_valid_hours` | 48 | `EA__APPLICATION__QUOTE_VALID_HOURS` | 1–720 |
  | `request_near_expiry_pct` | 25 | `EA__APPLICATION__REQUEST_NEAR_EXPIRY_PCT` | 1–99 |

`listing_api`: create, update, get and list read and write `booking_mode` (a `listing_history` snapshot includes it). It is validated by the enum, so no free text is accepted.

### 3.5. Clients & Frontend
`web_client` exposes the new `app_client` methods through `AuthenticatedClient`. Both portals map the typed error codes to friendly flash messages through the existing error-to-flash helper.

**Guest portal (`web_ui_guest`)**
| Route | Purpose |
|---|---|
| `/listings/{slug}` | Buttons depend on the mode: *Book now* (`instant`/`both`), *Request to book* (`request`/`both`), and *Ask a question* (always). A `badge` shows "Host approval required" on `request` listings. |
| `/listings/{slug}/enquire` (GET/POST) | Enquiry form: dates, guests and message, prefilled from the query string. Redirects to `/requests/enquiries/{id}`. |
| `/checkout/{slug}` | Mode-aware submit buttons: `action=book` (existing) and `action=request`. Request mode adds an optional message `textarea` and the copy "Your card isn't charged. The host has N hours to respond." The price preview and promo field are reused. `request`-only listings show only the request button. |
| `/requests` | "My requests": enquiries and requests as `card`s with a status `badge`, dates, total, and "expires in …" (server-rendered relative time; refreshed by an HTMX `hx-trigger="every 60s"` swap of the list fragment; no inline JS, so the CSP holds). |
| `/requests/{id}` | Detail: status timeline (history), the snapshot or quote breakdown, the quote terms (escaped), the promo-dropped notice, the decline reason and note, and *Proceed to checkout* / *Accept quote* (POST, carries `quote_id`) and *Withdraw* (POST, with a confirm `modal`). The thread is included. |
| `/requests/enquiries/{id}` | Enquiry detail with the thread, and *Submit request to book* (prefilled to `/checkout/{slug}?…&mode=request&enquiry_id=…`). |
| `/bookings/{code}/messages` | Unchanged route; now shows the whole conversation, with context badges. |

**Admin portal (`web_ui_admin`)**
| Route | Purpose |
|---|---|
| `/admin/inbox` | `table` of items (§1.10) with a near-expiry `badge-warning`, filters (`select`) and pagination; a `stats` card with the host median response time and the expired-unanswered share. The left-panel entry reads "Inbox (n)" with the unread guest messages (§1.10.5). |
| `/admin/requests/{id}` | Detail and actions: *Accept* (POST), *Decline* (`select` reason + `textarea` note), *Send quote* (stay price `input` in the base currency with the currency label, terms `textarea`, and an HTMX `hx-post` to the preview fragment showing the guest total, GCT and platform fee before sending). Thread and history. |
| `/admin/enquiries/{id}` | Thread, *Send quote*, *Close*, and the estimated-earnings card (§1.10.4). |
| `/admin/listings/{id}/edit` | A `booking_mode` `select` (Instant / Request / Both) with help text about open requests continuing. |

Form money fields are parsed with `Decimal::from_str` on the server. The HTML input is `type="text" inputmode="decimal"`, so the client never does float maths.

### 3.6. Background Workers
- **email_worker**: the new templates (§1.9) in `templates/`, rendered through `render_email_body`. Every user-supplied value (`message_text`, `terms`, `decline_note`, names, listing name) goes through `escape_html`. Money values are pre-formatted strings from booking_api.
- **Cloud Scheduler**: no new jobs. The existing hourly `run-sweeps` and scheduled-notifications jobs cover expiry and reminders. With a 24h window and 25%, the reminder fires 5–6h before expiry, which hourly granularity serves fine.
- **pg_cron**: unchanged (D5).

---

## 4. Performance Considerations
- **Availability**: each query adds one `NOT EXISTS` / `UNION ALL` probe on `idx_request_live_dates` (a partial index over live rows only, which stays tiny). That is O(log n) per check, with no extra round trip. Listing search adds a correlated `NOT EXISTS` per candidate listing on the same partial index, so there's no N+1.
- **Transactions**: the request, accept, quote and convert transactions each do ≤ 8 statements under one listing lock, the same order of cost as `create_booking`. Pricing (FX lookup, overrides) runs **before** the transaction, as `create_booking` does today, so the lock is held only for the checks and writes.
- **Unread count**: one indexed `COUNT` per full-page admin render (HTMX partials skip the shell, so they skip it too), on the partial index `idx_booking_message_conv_unread`. The enquiry estimate prices one stay with no FX lookup.
- **Inbox**: one paginated query plus one aggregate. The median is over ≤ 90 days of rows for ≤ N hosts and is bounded by the `listing_id` and `created_at` indexes. No per-row lookups.
- **Sweeps**: `expire_stale_requests` uses `idx_request_live_expiry`; the reminder candidate query uses the same index plus an anti-join on `uq_request_notification`.
- **Cold start**: no new dependencies, background tasks or startup I/O beyond reading three config ints. Budget unchanged (<300ms p50, <1s p95, 256MB).
- **Async**: no blocking work. Pricing is pure Decimal arithmetic; nothing needs `spawn_blocking`.
- **Migration**: the backfill touches only `booking_message` (small) and adds nullable columns, then `SET NOT NULL` once filled. The table is small enough that this is fine without batching.

---

## 5. Security Mitigations
| Threat | Mitigation |
|---|---|
| IDOR on enquiries, requests, conversations | Every handler resolves parties server-side (`guest_id`, `listing.user_id`) and refuses with 403 unless the caller is the guest, the owning host or an admin. Lists are scoped in SQL (`guest_id = $caller`; `listing.user_id = $caller` for hosts). An id from the URL is never trusted alone. |
| Guest acting as host or vice versa | Action-level role check (`PartyRole`). A host can't withdraw or convert, and a guest can't accept, decline or quote. A host can't enquire on their own listing (E19). |
| Date-blocking abuse (griefing) | One live request per guest × listing; a capped response window; host can decline at once. The residual risk (one guest holding all 5 listings for 24h) is accepted and tracked with rate limiting in #110. |
| Price tampering | The client never sends prices except the host's `stay_price`. GCT, fees and totals are always server-computed. Conversion copies the server-stored snapshot, and the guest must name the current `quote_id`. |
| Float or precision errors | `Decimal` end to end, including form parsing. `NUMERIC(12,2)` and `check_amounts_fit`. CI grep for `f32` and `f64`. |
| SQL injection | `sqlx::query!` / `query_as!` only; search uses `QueryBuilder::push_bind` for every value. |
| XSS (messages, terms, decline notes) | Topcoat escapes text nodes by default (no `raw` HTML). Email templates use `escape_html`, with a unit test per template that injects `<script>`. |
| Sensitive data in logs | Message text, terms and notes are never logged (lengths only). Promo codes keep the #106 hash-only rule. Spans skip request bodies. |
| CSRF on portal POSTs | Same session-cookie and form guard as the existing checkout and admin actions (SameSite cookies + existing CSRF handling). |
| AuthN | Claims extractor + active-user check (401 for deactivated accounts). Cron endpoints keep the OIDC + constant-time `x-cron-secret`. |
| Audit and repudiation | `booking_request_history` records actor id, role, reason and quote id for every transition. `info!` logs with `actor_id`, `request_id` and `action` for accept, decline and quote. |
| Commission disclosure | The enquiry earnings estimate (with commission) is built only for the host and admins; the guest's enquiry response omits `host_estimate`. The unread count is scoped in SQL to the caller's listings. |
| Panics | No `unwrap`/`expect` (CI grep). All new public items have `///` docs (`missing_docs`). |

---

## 6. Test Plan
Database tests run against the local devenv Postgres (`devenv shell`), not Docker. Iterate on the affected crates; run the full suite once at the end.

### Unit Tests (`cargo test -p common`)
- `RequestStatus::can_transition_to`: an exhaustive table over every (from, to) pair; terminal states have no outgoing edges; `quoted → quoted` is allowed.
- `BookingMode::allows_instant` / `allows_request` for all three modes.
- `response_window`: the normal window; the cap at check-in midnight Jamaica (DST-free zone, across UTC midnight); a window under 1h → `RequestWindowTooShort`; a check-in in the past.
- `is_near_expiry`: the 25% boundary (inclusive), 0%, 99%, and after expiry.
- `with_custom_stay_price`: replaces the subtotal, zeroes the discount and extra-guest fee, then GCT 15% and the platform fee recalculate correctly; golden case added to `stay_pricing_golden.json`.
- Quote floor: `0`, negative, 3 dp, and a value overflowing `NUMERIC(12,2)` are all refused.
- `StayPriceSnapshot` serde round trip preserves Decimal scale exactly.
- `DeclineReason::Other` requires a note.
- `RequestRefusal::code()` and HTTP status are unique and stable.
- `HostEarningsEstimate::from_stay`: gross, commission and net match `host_gross` + `calculate_host_payout` on a fixed case, with 0% commission and a net floored at 0.
- The unread label: none at 0, "(n)" up to 99, "(99+)" above, so the link reads "Inbox (n)".

### Integration Tests (`cargo test -p db_core -p booking_api`)
- **Race**: instant hold vs request on the same dates (two connections, `tokio::join!`) → exactly one success; request vs request from different guests → one success.
- **Blocking**: a live request and a live quote block `check_availability`, `get_unavailable_ranges`, listing search (`push_stay_conditions`), `create_booking` and a reservation change (gap included). Extend `availability_parity_tests` with request fixtures.
- **Expiry on read**: set `expires_at` in the past with no sweep → the dates are free in all five queries at once.
- **Offer window**: accept → the dates stay blocked and **no** booking row exists; convert → a `pending` booking with the snapshot price and `expires_at = now + pending_hold_minutes`; the request is `converted`; a rival hold between accept and convert → 409.
- **No release gap**: in the conversion tx, a concurrent rival hold waits on the listing lock and then gets 409.
- **Supersession**: an instant hold by the same guest on the same listing cancels their live request and closes their open enquiry (history rows, emails enqueued). A second live request → `REQUEST_ALREADY_LIVE`. A request on a listing where the guest holds a pending hold cancels the hold.
- **Mode enforcement**: instant hold on `request` → 409 `BOOKING_MODE_NOT_ALLOWED`; request on `instant` → 409; a mode switch leaves an open request convertible.
- **Expired actions**: accepting an expired request → `REQUEST_EXPIRED`; converting an expired quote → `QUOTE_EXPIRED`; no booking is created in either case.
- **Quote**: a quote on an enquiry whose dates were taken → `DATE_UNAVAILABLE`; a quote drops the promo and frees its reserved use (usage count drops); a stale `quote_id` → `QUOTE_SUPERSEDED`; a quote uses the snapshot FX rate even after a new rate version is published.
- **Promo (D2)**: a live request reserves a use (a limit-1 code then refuses another guest's hold); expiry or withdrawal frees it; conversion writes a redemption without re-checking usage, even after deactivation; the per-guest limit counts requests.
- **AuthZ**: a host gets 403 on another host's enquiry, request, quote preview and conversation; a guest gets 403 on another guest's request; a guest can't accept; a host can't convert; an admin can act on any listing; an owner enquiring on their own listing → 403.
- **Thread**: a message on an enquiry shows up on the resulting booking's `/messages`; the migration backfill maps existing messages to the right conversation; shadow-user transfer moves the messages.
- **History & email**: every transition writes exactly one `booking_request_history` row with the actor; each event enqueues the expected guest and host templates (assert `email_outbox` rows).
- **Sweeps**: `expire_stale_requests` expires only lapsed live rows and is idempotent across two runs (no duplicate emails); the host reminder fires exactly once inside the near-expiry window, not before it and not after expiry; past enquiries close.
- **Response metrics**: `first_response_at` is set by the first host message or action only; the median is correct on a fixed fixture; hosts see only their own metrics.
- **Config**: zero or out-of-range values fail startup validation.
- **Error mapping**: each `RequestRefusal` gives its status code and `{code, message}` body (mirrors the promo refusal test).
- **listing_api**: `booking_mode` round trip and history snapshot.
- **email_worker**: each new template renders, with `<script>` escaped in message, terms and note.
- **Enquiry estimate**: the host and an admin get `host_estimate` equal to the payout formula at the listing's commission; the guest gets none; an enquiry whose dates have passed returns `host_estimate: null` and still 200.
- **Unread count**: counts only unread guest-sent messages on the host's listings (another host's listing, host-sent and admin-sent messages, and read messages excluded); an admin counts every listing; a guest gets 403; marking the thread read drops the count.

### E2E Tests (Playwright — `playwright/`)
- **Guest, request mode**: the listing shows "Request to book" only → submit with a message → the request appears in My requests with a countdown.
- **Host and guest, accept → pay**: admin inbox shows the request with the near-expiry badge (fixture time) → accept → the guest proceeds to checkout → `/bookings?new_booking=true` shows the pending booking.
- **Quote**: the host sends a quote (preview shows the GCT and total) → the guest sees the terms and promo-dropped notice → accepts → a hold at the quoted total.
- **Decline**: the host declines with a reason and note → the guest sees both; the dates are free on the calendar.
- **Enquiry → request**: the guest asks a question → the host replies → the guest submits a prefilled request from the thread → the same thread shows on the booking.
- **Both mode**: checkout shows both buttons; *Book now* still works.
- **Admin listing form**: change the booking mode and see the guest's buttons change.
- **Enquiry earnings and unread badge**: a guest sends an enquiry → the host's left panel reads "Inbox (1)" → the enquiry page shows the estimated payout → after opening the thread it reads "Inbox" again.

### Gates
`cargo fmt --check` · `SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features -- -D warnings -D missing_docs` · `cargo sqlx prepare --workspace --check` · the backend and web UI test commands from CONSTRAINTS §2 · the float, `unwrap` and `env::var` greps · `npx playwright test`.

---

## 7. Acceptance Criteria
- [ ] 1. A new timestamped migration adds `booking_mode` (default `instant`), `conversation`, `booking_enquiry`, `booking_request`, `booking_request_quote`, `booking_request_history` and `request_notification_log`, and re-anchors `booking_message`. No existing migration is changed.
- [ ] 2. The admin listing form edits the booking mode; the API enforces it with 409 `BOOKING_MODE_NOT_ALLOWED` for instant holds and for requests.
- [ ] 3. A signed-in guest can send an enquiry; it creates or reuses the guest × listing conversation and blocks no dates.
- [ ] 4. A signed-in guest can submit a request (with an optional promo and message); it is priced, snapshotted and blocks dates until `expires_at` (24h default, capped at check-in).
- [ ] 5. Live requests and quotes block dates in availability, unavailable ranges, listing search, instant holds and reservation changes, including the gap; once expired they stop blocking immediately, with no sweep.
- [ ] 6. Host (own listings) and admin (any listing) can accept, decline (reason + note) and quote; other hosts get 403.
- [ ] 7. A quote takes a base-currency stay price, converts it at the snapshot rate, recalculates fees, GCT and the platform fee, drops the promo and says so, and expires after `quote_valid_hours`.
- [ ] 8. Acceptance creates an offer without starting a hold; *Proceed to checkout* converts it into a `pending` hold in one transaction at exactly the accepted price, with no gap in which the dates are free.
- [ ] 9. Expired, stale-quote and not-open actions are refused with their typed codes, and no hold is created.
- [ ] 10. One live request per guest × listing; an instant hold by that guest cancels their request or offer on that listing.
- [ ] 11. Live requests with a promo code reserve a use; conversion redeems it without re-checking.
- [ ] 12. `run_sweeps` expires lapsed requests and closes past enquiries; the guest and host are emailed once; the host gets exactly one near-expiry reminder.
- [ ] 13. Every enquiry and request status change and every quote writes `booking_request_history` with the actor.
- [ ] 14. Emails go to both parties for every event in §1.9, with user text HTML-escaped.
- [ ] 15. The admin inbox shows open items with age, time to first response, time left, the near-expiry badge and each host's median first-response time.
- [ ] 16. The guest portal has mode-aware buttons, the enquiry form, request at checkout, My requests, request and enquiry detail with the thread, and quote acceptance into checkout.
- [ ] 17. A thread started on an enquiry is visible on the resulting booking's messages.
- [ ] 18. Config keys `request_response_hours`, `quote_valid_hours` and `request_near_expiry_pct` are documented, env-overridable and validated at startup.
- [ ] 19. `openapi.yaml`, `openapi-prod.yaml`, `.sqlx/`, `docs/ARCHITECTURE.md` (request lifecycle next to §Pending Hold Lifecycle, §6 config table, §3.1 module tables) and `docs/ENVIRONMENT.md` are updated.
- [ ] 20. All CONSTRAINTS.md gates pass (fmt, clippy `-D warnings -D missing_docs`, sqlx check, backend and UI tests, float, unwrap and env greps), plus the Playwright flows in §6.
- [ ] 21. The host's enquiry page shows an approximate earnings figure (standard price for the dates minus commission, base currency), to the host and admins only (§1.10.4).
- [ ] 22. The admin portal's left panel shows the number of unread guest messages across the host's listings (all for admins), and it clears as threads are read (§1.10.5).
