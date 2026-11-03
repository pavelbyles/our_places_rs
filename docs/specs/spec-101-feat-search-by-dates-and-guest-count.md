# Spec 101: Search by Dates and Guest Count

> Driven by GitHub Issue #101. Intent: [intent-101](../intent/intent-101-feat-search-by-dates-and-guest-count.md) (Approved 2026-11-02).

## Overview
Guests can search `/listings` by dates, guest count and place. Results contain only stays that can really be booked, and each card shows the same total as checkout. The search lives in the URL, so it can be shared and it carries through to the listing page and checkout.

This work also changes these things for all bookings:
- `days_between_bookings` is enforced wherever availability is decided: holds, availability checks, booking updates, reservation changes and the listing calendar. Until now it was stored but had no effect.
- Quotes and holds refuse dates in the past.
- A hold refused for unavailable dates returns `DATE_UNAVAILABLE` (409), not an uncoded 400.
- The portal's default guest count is 1, not 2.

**Personas**: Guest (searches, compares totals, shares links) · Host and Admin (their gap setting now takes effect, and it limits their own date edits too).

---

## 1. Requirements

### 1.1. Search Parameters
`GET /api/v1/listings` gains four optional parameters. Without them, a search behaves as it does today.

| Param | Meaning |
|---|---|
| `check_in`, `check_out` | Stay dates, `YYYY-MM-DD`. Both or neither. An empty value counts as absent |
| `guests` | Party size, at least 1 |
| `q` | Text matched against name, city or country. The existing `name` filter keeps its meaning |

### 1.2. Validation
Invalid input returns a 400 with `{code, message}`, and the portal shows `message` as written. Checks run in this order and stop at the first failure.

| # | Input | Code | Message |
|---|---|---|---|
| 1 | A date that can't be parsed | `INVALID_STAY_DATES` | Invalid date range: please enter valid dates. |
| 2 | Only one of the two dates | `INVALID_STAY_DATES` | Invalid date range: choose both check-in and check-out. |
| 3 | `check_out <= check_in` | `INVALID_STAY_DATES` | Invalid date range: check-out must be after check-in. |
| 4 | `check_in` before today | `INVALID_STAY_DATES` | Invalid date range: dates can't be in the past. |
| 5 | More nights than `application.max_stay_nights` | `INVALID_STAY_DATES` | Invalid date range: stays can be at most N nights. |
| 6 | `guests` not a whole number, below 1, or too large for `i32` | `INVALID_GUESTS` | Guests must be at least 1. |

"Today" is the Jamaica date (UTC−5). Check-in today is allowed.

### 1.3. Search Filters
1. **Availability**: uses the rule in §1.5.
2. **Guests**: `max_guests >= guests`, the same comparison as `check_guest_count`.
3. **Minimum stay**: nights must be at least `listing.minimum_stay`, and at least the `min_nights` of any price override covering a night of the stay. This matches `calculate_dynamic_quote`.
4. **Bookable only**: with dates, inactive listings and listings with no `price_per_night` are left out, even if `include_inactive` is set. Neither can be booked.
5. **Text**: a case-insensitive partial match on `listing.name`, `listing.city` or `listing.country`, with `%`, `_` and `\` matched literally. `country` is matched as stored.
6. Filters run inside the search query, so `LIMIT` and `OFFSET` count only matching listings.

### 1.4. Stay Total
1. With dates, each listing that can be priced carries a `stay_total`. The amount is the quote's `total_amount`, which includes tax, fees and the refundable damage deposit, with no promo code.
2. It's priced for `guests`, or 1 if omitted, in `currency`, or the listing's base currency if omitted.
3. **No rate to the requested currency**: the listing stays in the results with no total and `price_unavailable_in`, as it does today.
4. **Any other pricing failure**, such as a total too large for the booking's money columns: the listing stays in the results with no total, and the server logs a warning with the listing ID. One listing can't fail the search.

### 1.5. Availability Rule
A booking blocks a stay when all of these hold:

```text
same listing
AND (status = 'confirmed' OR (status = 'pending' AND expires_at > NOW()))
AND date_from - gap < check_out
AND date_to + gap > check_in
```

`gap` is the listing's `days_between_bookings`. With a gap of 0, the default, this is today's rule, and same-day turnover stays bookable. With a gap of N, a stay can start N days after a booking's check-out, and end N days before its check-in.

| Path | Code | Behaviour |
|---|---|---|
| New hold | `db_core/src/booking/hold.rs:48` | Uses the rule, reading the gap after taking the listing row lock. A guest's own live hold still doesn't conflict |
| Availability check | `check_availability` | Uses the rule. Also serves `GET /api/v1/bookings/availability` |
| Booking update | `check_availability_excluding`, from `booking/update.rs:194` | Uses the rule against other bookings, only when dates change |
| Reservation change | `check_availability_excluding`, from `reservation_changes.rs:120`, `reservation_change.rs:313` and `:831` | Same, at quote, request and approval |
| Calendar | `get_unavailable_ranges` | Each range widens to `[date_from - gap, date_to + gap)`. The query also finds bookings just outside the window whose gap reaches into it |
| Search | `get_listings` | Uses the rule as a `NOT EXISTS` |

Quotes don't check availability, so they don't change.

**Limits on the gap**: `days_between_bookings` must be between 0 and 365. The current validation has a minimum only, on create. A very large gap would overflow `date + int`, and that SQL error would break search for every listing.

### 1.6. Past Dates and Hold Refusals
1. `price_stay` runs the validation in §1.2, rows 3–5, through the same function as search. This covers all three of its callers: `create_booking`, the public quote and the signed-in quote. Its existing date and length checks are replaced.
2. Booking updates and reservation changes keep their own past-date rules. They don't use `price_stay`.
3. `create_booking` in `db_core` returns `DbError::DateUnavailable` on a conflict, which the API returns as 409 `DATE_UNAVAILABLE`.

### 1.7. Portal
**`/listings`**
1. One search form holds `q`, `check_in`, `check_out` and `guests`, pre-filled from the URL, with hidden `host` and `currency` where set. The date inputs use `min` set to today, as a convenience only: the API decides what's valid.
2. Query values are read as strings and forwarded unchanged, so `check_in=abc` gets the API's message. Today one bad value makes `parse_query_params` fail, and `.unwrap_or_default()` then drops the whole search.
3. It uses `display_currency`, as the listing page and checkout do.
4. Results:

   | API result | Page shows |
   |---|---|
   | Success | Cards, or an empty state that mentions dates when they were given |
   | 4xx with `{code, message}` | `message` in a `role="alert"` block, plus the form, with no listings. Every listing shown here would look free for those dates |
   | Any other failure | "We couldn't load listings. Please try again." The failure is logged |

5. "Show all listings" removes only `host`.
6. The search box placeholder says it searches by name or place.

**Listing cards** (`villa_card`, `listing_card`)
1. With a `stay_total`: `USD 3420.00 total for 5 nights`, then `Includes taxes, fees and a USD 500.00 refundable deposit`. With no deposit, the second line is `Includes taxes and fees`. Totals use `price_breakdown::money`, so they keep their cents and match checkout.
2. The nightly line stays alongside.
3. With dates but no total, and no `price_unavailable_in`: `Total unavailable for these dates`.
4. The photo, name and "View Listing" links carry `check_in`, `check_out`, `guests` and `currency`. When dates are given without guests, the link says `guests=1`.
5. Cards with no search, including the home page carousel, don't change.

**Keeping the search when the currency changes**
The currency menu links to `?currency=X`, which replaces the whole query string, so switching currency loses the search. It must keep `q`, `host`, `check_in`, `check_out` and `guests`, and only those, with `currency` first. This applies on `/listings`, the listing page and checkout.

**Default guests**
Change to 1: `home.rs:53`, `listing_detail.rs:164`, `checkout.rs:330`.

**Home form**
Date inputs get `min` set to today, and a hidden `currency` when one is chosen.

**Checkout error messages** (`booking_error_message`)
Check error codes before the word match, and add `INVALID_STAY_DATES`, which shows its own message. Today `text.contains("available")` also matches "Foreign exchange rate unavailable" in the `FX_RATE_UNAVAILABLE` body, so a currency problem tells the guest their dates are gone.

### 1.8. Exclusions / Non-Goals
- A per-listing maximum stay.
- Applying the gap to existing bookings. Bookings already closer than the gap stay as they are, and confirming a live hold doesn't recheck it.
- An admin override of the gap.
- Promo codes on card totals, and sorting or filtering by total.
- Flexible dates, suggesting other dates, and map or radius search (#61, #103).
- Host-blocked dates (iCal is #58).
- Date filters in the admin portal, and totals on the home page carousel.
- Carrying the search through the card's "Hosted by" link.
- Fixing the wider OpenAPI drift (#113).
- Rate limiting search.
- Excluding a signed-in guest's own hold from search. Search is public and doesn't know who is asking, so their hold hides the listing until it expires, as it does on the calendar today.

---

## 2. Edge Cases & Failure States

| Case | Behaviour |
|---|---|
| Listing taken between search and hold | The hold refuses with 409 `DATE_UNAVAILABLE`, and checkout asks for different dates |
| Hold expires | Its dates are free in search and on the calendar as soon as `expires_at` passes, with no cleanup job needed |
| Two holds race inside each other's gap | The listing lock runs them in turn. The second sees the first and gets 409 |
| Guest replaces their own live hold with one inside its gap | Allowed: their own holds don't conflict, and the old one is cancelled in the same transaction |
| Booking changes its own dates | It's excluded from its own check, so its old dates and gap don't block it. Its neighbours still do |
| Host raises the gap | Later checks use the new value. Existing bookings stay. A booking too close under the new gap can stay as it is, but can't change dates to anything still inside the gap |
| Host lowers the gap | Nights free up on the next read |
| Booking just outside the calendar window | Its gap can still grey out nights inside the window |
| Last check-out before a gap | `ui.js` already allows checking out on the first unavailable night, which is the last day the gap allows |
| Bookmark with passed dates | Validation message and form, no listings |
| Bookmarked total goes out of date | It's recalculated on every load. There's no cached price |
| `check_in=` (empty, from the home form) | Treated as absent |
| `guests=2` with no dates | Filters by size and shows nightly prices with no total. The card link keeps `guests` |
| Guests over a listing's maximum via a hand-edited link | The search leaves it out. The listing page and checkout keep their current handling |
| Currency not active | `display_currency` drops it. If the currency list can't load, the API refuses and the page shows its message |
| Rate published between lookups | One request uses one rate map. The hold may later use a newer rate, and checkout shows the server's figure before the guest commits, as it does today |
| Override with a higher `min_nights` covering only part of the stay | It applies, as in `calculate_dynamic_quote` |
| Override added between the search query and the override load | Pricing may refuse that listing, which then shows with no total and a logged warning |
| 10-year date range | Allowed up to `max_stay_nights`. See Performance |

---

## 3. Technical Implementation

### 3.1. Data Model
No migration. `listing.days_between_bookings`, `booking` and `listing_price_overrides` already hold everything needed. `idx_booking_listing_id` and `idx_listing_price_overrides_lookup` cover the new conditions.

### 3.2. Domain (`common`)

**`common/src/availability.rs`**

```rust
/// Today's date in Jamaica (UTC−5), the platform's calendar for stays.
pub fn jamaica_today(now: DateTime<Utc>) -> NaiveDate;

/// Why stay dates or a guest count were refused.
pub enum StayInputError { DateMalformed, DateMissing, CheckOutNotAfterCheckIn, InPast, TooLong { max_nights: i64 }, InvalidGuests }
impl StayInputError { pub fn code(&self) -> &'static str; pub fn message(&self) -> String; }

/// Rows 3–5 of the validation. Shared by search and `price_stay`.
pub fn validate_stay_dates(check_in: NaiveDate, check_out: NaiveDate, today: NaiveDate, max_nights: i64)
    -> Result<(), StayInputError>;

/// Parses raw search input. An empty value counts as absent.
pub fn parse_stay_search(check_in: Option<&str>, check_out: Option<&str>, guests: Option<&str>, today: NaiveDate, max_nights: i64)
    -> Result<StaySearch, StayInputError>;

/// Validated search input. `dates` is both or neither, so a half-filled range can't exist.
pub struct StaySearch { pub dates: Option<(NaiveDate, NaiveDate)>, pub guests: Option<i32> }
```

`jamaica_today` replaces the copies in `web_ui_guest/src/support.rs:92`, `booking_updates.rs:148` and `cron.rs:290`.

**`common/src/pricing.rs`**: the promo-free steps of `price_stay` move here without changing any result, including when values are rounded.

```rust
/// Everything needed to price a stay, in the listing's base currency.
pub struct StayPricingInput<'a> {
    pub base_nightly_rate: Decimal,
    pub minimum_stay: i32,
    pub overrides: &'a [PriceOverride],
    pub rate: Decimal,
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
    pub number_of_persons: i32,
    pub guests_included: i32,
    pub extra_guest_fee: Decimal,
    pub cleaning_fee: Decimal,
    pub damage_deposit: Decimal,
    pub weekly_discount_percentage: Option<Decimal>,
    pub monthly_discount_percentage: Option<Decimal>,
}

/// Converts and prices the stay up to the promo step: the shared path.
pub fn price_before_promo(input: StayPricingInput<'_>) -> Result<BookingCalculator<Discounted>, PricingError>;

/// Largest amount the booking's `NUMERIC(10,2)` columns hold. Moved from `stay_pricing.rs`.
pub fn fits_booking_columns(calculator: &BookingCalculator<Taxed>) -> bool;
```

`price_stay` calls `price_before_promo`, then resolves the promo and finishes. Search finishes with `.apply_promo(None).apply_taxes(SupportedCountry::tax_rate_for(&country)).finalize()`, then `fits_booking_columns`.

**`common/src/models.rs`**

```rust
/// A stay's total, set by search when dates are given and it can be priced.
pub struct StayTotal {
    pub nights: i32,
    pub total_amount: Decimal,   // serialized as a string, like FeeItem
    pub damage_deposit: Decimal, // serialized as a string
    pub currency: String,
}
```

- `ListingResponse` gains `stay_total: Option<StayTotal>`, with `default` and `skip_serializing_if`. Struct literals in `api_core/src/models.rs` and tests need the new field.
- `ListingQueryParams` gains `q`, `check_in`, `check_out` and `guests` as `Option<String>`. Typed params would make Actix return its own untyped 400 on bad input.
- `ListingFilter` gains `q: Option<String>`, `check_in`, `check_out`: `Option<NaiveDate>`, and `guests: Option<i32>`.
- `NewListingRequest` and the update request: `days_between_bookings` validated to 0–365. Add validation to the update request if it's missing, and to the admin form.

### 3.3. Database Layer (`db_core`)

**`booking/availability.rs` and `booking/hold.rs`**: all queries use the rule in §1.5, joining `listing` for the gap.

```sql
SELECT count(*) AS count
FROM booking b JOIN listing l ON l.id = b.listing_id
WHERE b.listing_id = $1
  AND (b.status = 'confirmed' OR (b.status = 'pending' AND b.expires_at > NOW()))
  AND b.date_from - l.days_between_bookings < $3
  AND b.date_to + l.days_between_bookings > $2
```

- `get_unavailable_ranges` selects the widened dates as `date_from` and `date_to`, and `merge_ranges` still applies. Its doc comments and those in `common/src/availability.rs` change from "booked" to "unavailable".
- The hold keeps its existing lock order and takes the listing row lock first. It returns `DbError::DateUnavailable` on a conflict.
- **New error**: `DbError::StayInput(StayInputError)`, with a 400 and `{code, message}` in `api_core/src/error.rs`, alongside `PromoRefused`.

The rule stays as SQL in each query, because `sqlx::query!` needs literals and a shared SQL function would mean a migration. A table-driven parity test (§6) keeps the copies in step.

**`listing/queries.rs`**: `get_listings` adds to its existing `QueryBuilder`, with every value bound:

```sql
-- with dates
AND listing.is_active = TRUE
AND listing.price_per_night IS NOT NULL
AND listing.minimum_stay <= $nights
AND NOT EXISTS (
  SELECT 1 FROM listing_price_overrides o
  WHERE o.listing_id = listing.id AND o.start_date < $check_out AND o.end_date > $check_in
    AND o.min_nights > $nights)
AND NOT EXISTS (
  SELECT 1 FROM booking b
  WHERE b.listing_id = listing.id
    AND (b.status = 'confirmed' OR (b.status = 'pending' AND b.expires_at > NOW()))
    AND b.date_from - listing.days_between_bookings < $check_out
    AND b.date_to + listing.days_between_bookings > $check_in)
-- with guests
AND listing.max_guests >= $guests
-- with q, the pattern escaped and wrapped in %
AND (listing.name ILIKE $p ESCAPE '\' OR listing.city ILIKE $p ESCAPE '\' OR listing.country ILIKE $p ESCAPE '\')
```

**`listing/price_overrides.rs`**: a new batched load:

```rust
/// Overrides touching `[check_in, check_out)` for many listings, in one query.
pub async fn get_active_overrides_for_listings<'e, E>(executor: E, listing_ids: &[Uuid], check_in: NaiveDate, check_out: NaiveDate)
    -> Result<HashMap<Uuid, Vec<PriceOverride>>>;
```

### 3.4. API

**`listing_api::get_listings`**

1. Call `parse_stay_search` with `jamaica_today(Utc::now())` and `settings.application.max_stay_nights`. A refusal is logged at `warn!` and returned as a 400 before any query runs.
2. Run `db_listing::get_listings` with the filter.
3. With dates, load overrides in one query.
4. Load rates once with `get_rates_for_bases` when `currency` is given. Without it, the rate is 1, and no rate lookup is needed: #100 refuses to deactivate a currency a listing uses, so the quote's active-currency check can't fail here.
5. **Compute totals from the database rows, before mapping and converting.** `convert_search_prices` overwrites `price_per_night` and `base_currency` on the response, so pricing after it would convert twice.
6. `convert_search_prices` takes the rate map from step 4, so a request makes one rate lookup.
7. Attach `stay_total`. One aggregated `warn!` names any listings that couldn't be priced.

`listing_api/src/main.rs` calls `validate_max_stay_nights` at startup, as `booking_api` does.

**`booking_api`**
- `price_stay` uses `validate_stay_dates` and `price_before_promo`.
- `check_amounts_fit` uses `fits_booking_columns`.

**Contract changes**

| Endpoint | Change |
|---|---|
| `GET /api/v1/listings` | New params, `stay_total` on results, 400 `INVALID_STAY_DATES` / `INVALID_GUESTS` |
| `POST /api/v1/bookings` | Conflict is 409 `DATE_UNAVAILABLE`, was an uncoded 400. Past dates are 400 `INVALID_STAY_DATES` |
| `GET /api/v1/bookings/listing/{id}/quote`, `POST /api/v1/bookings/quote` | Past dates and over-limit stays are 400 `INVALID_STAY_DATES`. Over-limit was `{"error": msg}` |
| `GET /api/v1/bookings/listing/{id}/unavailable-ranges` | Ranges include gap nights |
| `GET /api/v1/bookings/availability` | Takes the gap into account |

### 3.5. Clients & Frontend

**`common/src/app_client/listings.rs`**
- `ListingSearchParams` gains `q`, `check_in`, `check_out` and `guests` as `Option<String>`, sent only when non-empty.
- On a 4xx with `{code, message}`, `search_listings` returns `anyhow::Error::new(ListingSearchRefusal { code, message })`, so `/listings` can use `downcast_ref` and other callers don't change.

**`web_ui_common`**
- `ListingSummary` gains `stay_total`, `search_query: Option<String>` and `href()`, used by both cards. The query is built with proper URL encoding.
- `currency_menu` takes the allow-listed params and builds its links with them. `shell_guest.rs:68` supplies them from the request URI.
- Calendar text says "unavailable" rather than "booked" for screen readers and `ui.js` announcements. `data-state` doesn't change.

**`web_ui_guest`**
- `pages/listings.rs`, `home.rs`, `listing_detail.rs` and `checkout.rs`, as §1.7 describes.
- `support.rs` uses `common::availability::jamaica_today`.

### 3.6. Background Workers
None. Expiry and cleanup jobs don't change: availability reads `expires_at` directly.

### 3.7. Documentation
- `docs/ARCHITECTURE.md`: the availability rule with the gap, search filters and totals, and pricing shared through `common`.
- `openapi.yaml`, `openapi-prod.yaml`: only the routes in §3.4 that are already listed. Anything missing belongs to #113.
- `.sqlx/`: regenerate for changed macros with `cargo sqlx prepare --workspace`.

---

## 4. Performance Considerations
- **Queries per search**: one listing query, one override query, and one rate lookup with its active-currency check. The count doesn't depend on page size.
- **Availability**: `NOT EXISTS` probes `idx_booking_listing_id` once per candidate listing, ending at the first match. Fine at today's scale. Check with `EXPLAIN` if listings or bookings grow by orders of magnitude, and add an index only if it shows a need.
- **Portal**: one API call per search, with no per-card quote calls. `per_page` is 50, and the API clamps at 100.
- **Pricing cost**: `calculate_dynamic_quote` is O(nights × overrides) per listing and allocates a nightly breakdown. The worst case is about 3,650 nights × 100 listings, run one at a time, with each breakdown freed before the next. This is short CPU work with a small memory peak, so `spawn_blocking` isn't needed. Lowering `max_stay_nights` tightens it.
- **Cold start**: no new dependencies or startup work beyond one settings check. The budget of <300ms p50 and <1s p95 is unchanged.
- **Hold**: a join to the row it has already locked adds almost no time, and the lock isn't held for longer in any meaningful way.

---

## 5. Security Mitigations
- **Trust boundary**: all new input is an untrusted public query string. It's parsed in `parse_stay_search` before any query runs, and every value is bound as a SQL parameter.
- **LIKE injection**: `%`, `_` and `\` in `q` are escaped, so a guest can't turn the text into a broad pattern. Length is bounded by the server's request limits.
- **IDOR and data exposure**: search and the calendar return no booking IDs, guest data or statuses. Gap nights can't be told apart from booked ones. The gap itself is already public on the listing.
- **Inactive listings**: dated searches never return them, regardless of `include_inactive`. The public reach of that flag on undated searches already exists and isn't changed here.
- **Double bookings**: search and the calendar are lock-free reads that decide nothing. Holds, updates and change approvals still run under the listing row lock, as `CONSTRAINTS.md` requires.
- **Reflected input**: Topcoat escapes the form values shown back to the guest. Card and currency links are relative, with URL-encoded values, and the currency menu copies only allow-listed params.
- **Error detail**: 4xx bodies contain only fixed messages. Server failures show a generic message and log the detail.
- **Date overflow**: the 0–365 limit on the gap stops a host's setting from causing SQL errors for other listings' searches.
- **Abuse**: search has no rate limit, as today. Costs are bounded by the `per_page` clamp and `max_stay_nights`.

---

## 6. Test Plan

### Unit Tests (`cargo test -p common`)
- `parse_stay_search`: each row of §1.2 with its code and message, the check order, empty values as absent, `guests` at 0, -1, "abc" and above `i32`.
- `validate_stay_dates`: check-in today passes, yesterday fails, exactly `max_nights` passes, one more fails.
- `jamaica_today`: at 03:00 UTC it returns the previous date.
- `price_before_promo`: gives the same figures as the old `price_stay` steps, with extra guests, an override, weekly and monthly discounts, a non-base currency, and a rate of 1.
- `merge_ranges`: widened ranges merge when they touch.

### Integration Tests (`cargo test -p db_core -p listing_api -p booking_api`)
Run against the local Postgres in the devenv shell.

**Availability parity (table-driven)**: for each case below at gaps of 0, 1 and 3, `check_availability`, the hold, search and the calendar must agree.
- Booked: confirmed, live hold, expired hold, cancelled, completed.
- Position: overlap, same-day turnover either way, N−1 days and N days from the booking on each side.

**Holds and changes**
- A hold, a booking update, and a reservation change at quote, request and approval are each refused at N−1 days and accepted at N days.
- Concurrent holds inside each other's gap produce exactly one success.
- A guest can replace their own live hold with one inside its gap.
- A booking changing its dates ignores itself and is checked against its neighbours.
- Raising a gap leaves existing bookings alone, and confirming a live hold still works.
- A conflicting hold returns 409 `DATE_UNAVAILABLE`. Update any test that expects the old 400.

**Calendar**
- A booking just outside the window greys out the nights its gap reaches.
- A gap of 0 returns the same ranges as today.

**Search**
- `max_guests` boundary: equal matches, one more doesn't.
- Minimum stay at and below the limit, including a partial override with a higher `min_nights`.
- Text matches on name, city and country, ignoring case, and combines with dates and guests. `%` and `_` match literally.
- Dated searches leave out inactive listings and listings with no price, even with `include_inactive`.
- Pagination returns full pages when earlier listings were filtered out.
- The same number of queries runs for 1 and 50 listings.
- Each invalid input returns a 400 with its code and message, and runs no query.
- Searches with no new params return what they return today.
- A gap above 365 is refused on create and update.

**Totals**
- The card total equals the public quote and the created hold, with extra guests, an override, a weekly discount and a non-base currency. A test that converted prices twice would fail here.
- A missing rate gives `price_unavailable_in` on that listing and normal totals on the others.
- A total too large to store gives no total, a warning, and normal results for the others.

**Past dates**
- A past check-in returns 400 `INVALID_STAY_DATES` from search, `create_booking`, the public quote and the signed-in quote, and creates no hold.
- Check-in on the Jamaica date is accepted when the UTC date has already moved on.

API tests that create holds use 2030 dates, and past-dated fixtures insert directly with SQL, so neither should need changing. The full suite will confirm this.

### Web UI Tests (`cargo test -p web_client -p web_ui_common -p web_ui_guest`)
- Cards: total line with and without a deposit, the unavailable line, nothing new without a search, links that carry the search with URL encoding, `guests=1` when dates have no guests.
- `currency_menu`: keeps the allow-listed params, drops others, and puts `currency` first.
- `booking_error_message`: a full `FX_RATE_UNAVAILABLE` body gives the currency message, `INVALID_STAY_DATES` shows its message, `DATE_UNAVAILABLE` gives the dates message.
- With no guest count, the listing page form and checkout default to 1.

### E2E Tests (Playwright)
- A home search reaches `/listings` with filters applied, and reloading the URL gives the same results.
- A card link keeps dates and guests through to checkout, and the card total equals checkout's total.
- Switching currency on `/listings` keeps the search and changes the totals. Check `checkout-flow.spec.ts:108` still fits, since dates now survive the switch.
- A listing with a gap: search leaves out a stay inside it, the calendar greys out those nights, and the last allowed check-out can be selected.
- An old bookmark shows the validation message and a form to pick new dates.
- Searching a city and a country finds the expected listings.

### Gates
Everything in `CONSTRAINTS.md` §2, including `cargo sqlx prepare --workspace --check`, `-D missing_docs`, and the no-float and no-panic checks. Run targeted tests while iterating and the full suite once at the end.

---

## 7. Acceptance Criteria
- [ ] 1. `GET /api/v1/listings` accepts `q`, `check_in`, `check_out` and `guests`. Without them, results don't change.
- [ ] 2. Every invalid input in §1.2 returns a 400 with its code and message, before any query runs.
- [ ] 3. Search returns only listings that are free under §1.5, fit the party, and meet the minimum stay, including overrides.
- [ ] 4. Text search matches name, city or country, with `%` and `_` matched literally.
- [ ] 5. Filters run in one query, and pagination counts only matching listings.
- [ ] 6. `check_availability`, the hold, booking updates, reservation changes, the calendar and search all apply the gap, and the parity test proves they agree at gaps of 0, 1 and 3.
- [ ] 7. Concurrent holds inside each other's gap produce exactly one success, and a guest can still replace their own hold.
- [ ] 8. Existing bookings aren't changed or rechecked when a gap changes, and `days_between_bookings` is limited to 0–365.
- [ ] 9. A conflicting hold returns 409 `DATE_UNAVAILABLE`.
- [ ] 10. Past dates return 400 `INVALID_STAY_DATES` from search, holds and both quote endpoints, using the Jamaica date.
- [ ] 11. The card total equals the public quote and the hold for the same inputs, from shared code in `common`.
- [ ] 12. A listing that can't be priced stays in the results with no total and never fails the search. No fallback rate is used.
- [ ] 13. Query count doesn't depend on page size, and the portal makes no per-card quote calls.
- [ ] 14. `/listings` keeps the search in the URL, shows typed refusals and server failures as different messages, and never shows a failure as "no listings".
- [ ] 15. Card links and the currency menu keep the search through to the listing page and checkout.
- [ ] 16. The portal defaults to 1 guest on the home form, listing page and checkout.
- [ ] 17. Checkout shows the right message for currency and date errors.
- [ ] 18. `docs/ARCHITECTURE.md`, the OpenAPI entries for changed routes, and `.sqlx/` are updated.
- [ ] 19. All `CONSTRAINTS.md` gates pass, and no migration was added.
