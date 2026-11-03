# Intent: Search by Dates and Guest Count

## Metadata
- **Author**: Originator (via GitHub Issue #101)
- **Date**: 2026-11-02
- **Status**: Approved (2026-11-02)
- **Target Area**: Fullstack (`common`, `db_core`, `app_api/listing_api`, `app_api/booking_api`, `web_client`, `web_ui_common`, `web_ui_guest`)

---

## 1. Problem Statement & Motivation
The home page asks guests for check-in, check-out and a guest count, then ignores all three. `/listings` reads only the name, host and currency. As a result, guests see villas that are already booked, too small for their party, or unable to take a stay of that length. They find out only on the listing page or at checkout, and the price they've been comparing is a nightly rate, not what the stay will cost.

---

## 2. Proposed Outcome (The Vision)
1. **Available stays only**: with dates, search returns only listings free for the whole range. It counts confirmed bookings, and pending holds while `expires_at > NOW()`, and it keeps each listing's `days_between_bookings` gap around them.
2. **Big enough**: with a guest count, search returns only listings whose `max_guests` fits the party.
3. **Stay length fits**: with dates, search leaves out listings whose stay rules don't allow that number of nights.
4. **Total price on the card**: with dates, each card shows the total for the stay in the guest's currency, alongside the nightly rate.
5. **Shareable searches**: dates and guests stay in the `/listings` URL, so a search can be shared or bookmarked. They carry through to the listing page and checkout.
6. **Clear refusals**: invalid input returns a typed 400 with a readable message such as "Invalid date range", and the page shows it instead of "no listings".
7. **Search by place**: the search text matches a listing's name, city or country, so "Runaway Bay" or "Jamaica" finds villas.
8. **No past stays**: quotes and holds refuse dates in the past, as search does.
9. **The booking gap is enforced**: `days_between_bookings` is applied wherever availability is decided, including holds, changes to a booking and the listing calendar, so a host's turnaround time is actually protected.

---

## 3. Scope Boundaries
### In Scope (MVP)
- **Search API**: optional `check_in`, `check_out` and `guests` on `GET /api/v1/listings` and in `ListingSearchParams`, `ListingQueryParams` and `ListingFilter`. Existing callers that don't send them, including the admin portal and booking lookups, behave as they do today.
- **Validation**: a 400 with a typed code and a message the page can show as written. Example wording:
  - `check_out` not after `check_in`: "Invalid date range: check-out must be after check-in."
  - Dates in the past: "Invalid date range: dates can't be in the past." Check-in today is allowed, and "today" is the Jamaica date (UTC−5) already used by `web_ui_guest/src/support.rs` and `booking_api`.
  - Only one of the two dates: "Invalid date range: choose both check-in and check-out."
  - More nights than `application.max_stay_nights`: "Invalid date range: stays can be at most N nights."
  - `guests` below 1: "Guests must be at least 1."
- **Availability filter**: part of the search query itself, so it's one batched query and `LIMIT`/`OFFSET` count only matching listings. It uses the gap-aware overlap rule below.
- **Gap-aware availability everywhere**: a booking blocks a stay when `date_from - days_between_bookings < check_out AND date_to + days_between_bookings > check_in`. With a gap of 0, the default, same-day turnover stays bookable. Every place that decides availability changes together:

  | Path | Code | Change |
  |---|---|---|
  | New hold | `db_core/src/booking/hold.rs:48` | Has its own copy of the overlap query. Applies the gap, read from the listing row it already locks. A guest's own live hold still doesn't conflict, so they can replace it |
  | Availability check | `check_availability` | Applies the gap, which also covers `GET /bookings/availability` |
  | Booking updates | `check_availability_excluding` via `booking/update.rs:194` | Applies the gap to other bookings, never to the booking being changed |
  | Reservation changes | `check_availability_excluding` via `reservation_change.rs:313`, `:831` and `reservation_changes.rs:120` | Same, at quote, request and approval |
  | Listing calendar | `get_unavailable_ranges` | Each range widens to `[date_from - gap, date_to + gap)`, and the query also finds bookings just outside the window whose gap reaches into it |

  - Quotes never check availability, so they don't change.
  - The calendar needs no new date-picking logic. `ui.js` already lets a guest check out on the first unavailable night, which is now the last day the gap allows.
  - The response still contains dates only, so a guest can't tell gap nights from booked ones.
  - Admin and host date edits are refused inside the gap too. No override is added.
- **Text search on name, city or country**: a case-insensitive partial match on `listing.name`, `listing.city` or `listing.country`, used by the "Where" field and the `/listings` search box.
  - It uses a new `q` parameter, so the API's `name` filter keeps its meaning.
  - `%` and `_` in the text are escaped, so they match literally.
  - `country` is free text matched as stored. A listing stored as "JM" won't match "Jamaica", but the admin form's default is "Jamaica".
- **Past dates refused at quote and hold**: `price_stay` rejects them with the same typed 400. All three of its callers are covered: `create_booking`, the public quote and the signed-in quote. Booking updates and reservation changes already have their own past-date rules, which don't change.
- **Guest filter**: `guests <= max_guests`, the same comparison as `check_guest_count`.
- **Minimum stay filter**: the nights must meet the larger of `listing.minimum_stay` and the `min_nights` of any price override covering a night of the stay. This matches `calculate_dynamic_quote` in `common/src/pricing.rs`, so search won't offer a stay that pricing then refuses.
- **Batched stay totals**: rates and price overrides are each loaded in one query for the whole page of results. The total comes from the same calculation as the quote and the hold (see Constraints).
- **Listing card**: a total-price line when dates are given, showing the quote's `total_amount` and labelled to make clear it includes the refundable damage deposit. The card link carries `check_in`, `check_out`, `guests` and `currency`. Cards without dates, including the home page carousel, don't change.
- **`/listings` page**:
  - Date and guest inputs, pre-filled from the URL. The name search form keeps dates, guests, host and currency, and so does "Show all listings" where that makes sense.
  - Uses `display_currency`, as the listing page and checkout do. Today it forwards the raw `?currency=`.
  - **Failures are no longer hidden.** The page uses `.unwrap_or_default()` and the client's `bail!` drops the error code, so any failure reads as "No listings match". A typed 400 must show its message, and a server failure must show an error state.
  - An old bookmark whose dates have passed gets the validation message and a form to pick new dates.
  - The search box placeholder says it searches by name or place.
- **Default of 1 guest everywhere**: the home form (`home.rs:53`), the listing page form (`listing_detail.rs:164`) and checkout (`checkout.rs:330`) default to 2 today and change to 1. The listing page quote (`listing_detail.rs:314`) and the API already use 1. A card total, listing page and checkout must agree for the same URL.
- **Docs and tests**: `openapi.yaml`, `openapi-prod.yaml`, `docs/ARCHITECTURE.md`, and any `.sqlx/` metadata. Database tests, plus Playwright coverage for the guest portal.

### Out of Scope / Non-Goals
- A per-listing maximum stay. None exists today, so the only limit is the app-wide `max_stay_nights`.
- Applying the gap to existing bookings. Bookings already closer together than the gap stay as they are, including when a host raises it later. A booking is only rechecked if its dates change.
- Host-blocked dates. The schema has no such concept, and iCal sync is #58.
- Promo codes on card totals. A code needs a signed-in guest and is entered at checkout.
- Price filtering or sorting by stay total, flexible dates, and suggesting alternative dates.
- Location or map search (#61, #103).
- Date filtering on the admin portal's listing pages.
- Showing a total on the home page carousel, which has no dates to price.
- Changing how holds are locked. The listing row lock and lock order stay as they are. Only the past-date and gap checks are added.

---

## 4. Affected Systems & Stakeholders
- **User Personas**:
  - **Guest**: searches, compares real totals, and shares links.
  - **Host and Admin**: no new screens. Their listings appear only for searches they can serve, and the gap they set now takes effect. Their own date edits must respect it too.
- **System Components**:

  | Component | Change |
  |---|---|
  | `common/src/models.rs` | New query params and filter fields, optional stay total on `ListingResponse` |
  | `common/src/app_client/listings.rs`, `web_client` | Send the new params, keep the error code so pages can show the reason |
  | `common/src/pricing.rs` | Shared stay-total calculation, if pricing moves here (see Open Questions) |
  | `common` | Shared stay-date validation and Jamaica date, used by search and `price_stay` |
  | `db_core/src/booking/availability.rs`, `hold.rs` | Gap-aware overlap rule for checks, holds and calendar ranges |
  | `common/src/availability.rs`, `web_ui_common/.../date_range_calendar.rs` | Range docs and wording now cover gap nights as well as booked ones |
  | `db_core/src/listing/queries.rs` | Availability with gap, guest, minimum-stay and name/city/country conditions in `get_listings`, batched override loading |
  | `app_api/listing_api/src/apis.rs` | Validation, typed errors, stay totals next to `convert_search_prices` |
  | `app_api/api_core/src/error.rs` | New typed codes. `DbError::ValidationError` returns `{"error": msg}` with no code, so it isn't enough |
  | `app_api/booking_api/src/apis/stay_pricing.rs` | Uses the shared calculation with the same prices, and refuses past dates |
  | `web_ui_common/.../listing_card.rs`, `villa_card.rs` | Total line, links that carry the search |
  | `web_ui_guest/src/pages/listings.rs`, `home.rs`, `listing_detail.rs`, `checkout.rs` | Inputs, URL state, error display, default of 1 guest |

---

## 5. Constraints & Non-Negotiable Invariants
- **Domain Invariants**:
  - **One availability rule**: search, the hold, updates, reservation changes and the calendar all use the same status set, overlap condition and gap. There are copies of this SQL today, including the hold's, and tests must prove they agree at gaps of 0 and above.
  - **Gap read under the lock**: the hold reads `days_between_bookings` in the transaction holding the listing row lock, so two concurrent holds can't both land inside each other's gap.
  - **One date check**: search and `price_stay` share the validation, so they can't disagree on what counts as a past date.
  - **One price**: the card total must equal the public quote and the hold for the same listing, dates, guests and currency, with no promo. It must come from shared code, not a second formula.
  - **Search doesn't place holds**: it's a lock-free read, like `get_unavailable_ranges`. `CONSTRAINTS.md`'s `FOR UPDATE` rule still applies to the hold, which stays the only authority. A listing can be taken between search and hold, and the hold then refuses it. Today that refusal is an untyped 400, "Listing is not available for the selected dates" (see Open Questions).
  - **No guessed prices** (#100): a listing with no rate to the requested currency stays in the results with no total and "Price unavailable in CUR". It's never given a fallback rate, and it never fails the whole search.
  - `rust_decimal::Decimal` only for money, and no `.unwrap()`/`.expect()`.
  - An expired hold frees its dates in search as soon as `expires_at` passes, without waiting for a cleanup job.
- **Service Boundaries**: APIs depend on `common` and `db_core`, never on each other. The microservice split stays.
- **Performance Budgets**:
  - Availability adds no per-listing query. Rates and overrides are one query each, whatever the page size.
  - The portal doesn't call the quote endpoint once per card. Search pages request up to 50 listings.
  - Cold start budget is unchanged (<300ms p50, <1s p95).
  - `booking` has an index on `listing_id` only. A new index, in a new timestamped migration, is added only if testing shows a need.
- **Security & Compliance**:
  - The search stays public and returns only active listings unless `include_inactive` is set, as it does today.
  - Availability results expose no booking details.
  - Dates and guests are bound as SQL parameters. `get_listings` uses `QueryBuilder`, which isn't compile-time checked, so database tests must cover each new condition.
  - `CONSTRAINTS.md` applies in full, including doc comments on all new public items.

---

## 6. Success Metrics & Signals
- **Leading Indicators**:
  - Test: a listing is left out for a confirmed booking or a live hold, and returned for an expired hold and a cancelled booking.
  - Test: with a gap of 0, a same-day turnover is returned in either direction.
  - Test: with a gap of N, a stay N−1 days before or after a booking is left out and one N days away is returned. Live holds count, and expired holds don't.
  - Test: for the same bookings and gaps of 0 and above, search, `check_availability`, the hold and the calendar all agree on which stays are free.
  - Test: a hold, a booking update, and a reservation change at quote, request and approval are each refused N−1 days from another booking and accepted N days away.
  - Test: concurrent holds inside each other's gap produce exactly one success.
  - Test: a guest can replace their own live hold with one inside its gap.
  - Test: changing a booking's dates ignores its own gap, and is still checked against its neighbours.
  - Test: a booking just outside the calendar window whose gap reaches into it greys out those nights, and the last check-out the gap allows can be selected.
  - Test: raising a gap leaves existing bookings and their status changes alone, including confirming a live hold.
  - Test: the text search matches on name, city and country, ignoring case, and combines with the date and guest filters. `%` and `_` match literally.
  - Test: a past check-in returns the typed 400 from search, `create_booking`, the public quote and the signed-in quote, and creates no hold. Check-in on the Jamaica date for today is accepted, including when the UTC date has already moved on.
  - Test fixtures with fixed dates that go through these paths use relative dates, so they don't start failing as the dates pass.
  - Test: with no guest count, the card total, listing page and checkout all price 1 guest.
  - Test: `guests` equal to `max_guests` matches, and one more doesn't.
  - Test: stays at and below the minimum nights behave correctly, including an override with a higher `min_nights` that covers only some of the stay.
  - Test: a page of results uses the same number of queries for 1 and 50 listings.
  - Test: pagination returns full pages when earlier listings were filtered out.
  - Test: each invalid input returns a 400 with its typed code and readable message, and bad input never reaches the database.
  - Test: the card total equals the public quote and the created hold, with extra guests, an override, a weekly discount and a non-base currency.
  - Test: a missing rate gives "Price unavailable" on that card and normal results for the others.
  - Test: searches without the new params return what they return today.
  - Playwright: a home search reaches `/listings` with the filters applied, the URL reloads to the same results, and the card link keeps dates and guests through to checkout.
  - Playwright: an expired bookmark shows the validation message and a way to pick new dates.
  - Database tests run against the local Postgres in the devenv shell, not Docker.
- **Lagging Indicators**:
  - Fewer availability and minimum-stay refusals at checkout.
  - No new booking lands inside a listing's gap.
  - No reports of a card total that differs from checkout.
  - Search latency stays in line with today's listing search.

---

## 7. Open Questions & Policy Concerns
### Resolved (originator, 2026-11-02)
- [x] **Maximum stay**: listings have no maximum-stay rule, so only the app-wide `max_stay_nights` applies. Going over it returns a 400 with a readable message such as "Invalid date range".
- [x] **`days_between_bookings`**: enforced everywhere availability is decided, including holds, not only in search. Booking and search still share one rule, as the issue asks, and that rule now includes the gap.
- [x] **What "total" includes**: the quote's `total_amount`, including tax, fees and the refundable damage deposit, labelled so the deposit is clear. The card matches checkout.
- [x] **Default guests**: 1 everywhere, replacing today's default of 2 on the home form, listing page form and checkout.
- [x] **Text search**: matches a listing's name, city or country.
- [x] **Past dates at booking**: fixed in this branch, for holds and both quote endpoints.

### Still Open
**Design (Stage 2)**
- [ ] **Hold refusal code**: `create_booking` refuses unavailable dates with `DbError::ValidationError`, an uncoded 400, while updates and reservation changes return `DATE_UNAVAILABLE` (409). *Proposed: the hold also returns `DATE_UNAVAILABLE`, once the spec confirms what the checkout page and tests expect.*
- [ ] **Where stay totals are calculated**: `price_stay` lives in `booking_api`, and `listing_api` can't depend on it. *Proposed: move the promo-free calculation into `common`, used by both services, with `listing_api` returning the totals in the search response.* This gives one request and one set of batched lookups. The alternative is a new batch-quote endpoint in `booking_api`, which keeps pricing in one service but adds a second call per search and a new way to get a partial failure.
- [ ] **Error codes**: exact names, and whether they reuse the `{code, message}` shape used by the currency errors. `promo_refusal_in_error` is the precedent for reading a code in the portal.

Issue to open, if wanted: per-listing maximum stay.
