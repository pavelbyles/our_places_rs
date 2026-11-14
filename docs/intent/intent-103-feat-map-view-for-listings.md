# Intent: Map View for Listings

## Metadata
- **Author**: Originator (via GitHub Issue #103)
- **Date**: 2026-11-13
- **Status**: Approved (2026-11-13)
- **Target Area**: Fullstack (`common`, `db_core`, `app_api/listing_api`, `web_client`, `web_ui_common`, `web_ui_guest`, `web_ui_admin`)

---

## 1. Problem Statement & Motivation
Every listing already has a latitude and longitude. Hosts enter them in the admin form, and `listing_api` reverse-geocodes them through Nominatim (`common/src/geocode.rs`) to fill `city`. Guests never see a map, though. On `/listings` they can't tell which villas are close to each other, to the beach or to the airport. The listing page's Location section (`web_ui_guest/src/pages/listing_detail.rs`, `#location`) shows a decorative gradient with a dot instead of a real map.

Hosts have the opposite problem. They have to type raw decimal coordinates into two text boxes and can't see where the pin will land, so a single mistyped digit can put a villa in the sea.

There is also a privacy gap. The public `Listing` DTO (`common/src/models.rs`) carries exact `latitude` and `longitude`. The APIs are IAM-only and no page renders coordinates today, so the exposure is latent (spec Overview), while the listing page tells guests that "the exact address is shared after you book." Putting a map on screen would make that leak visible, so this feature has to close it.

---

## 2. Proposed Outcome (The Vision)
1. **Search results map**: `/listings` shows a map with one marker for each listing in the current results, using the same filters, dates and guest count as the list. Clicking a marker opens a small card with the photo, name and price (the total for the stay when dates are set, as on the list card), and the card links to the listing.
2. **Responsive layout**: on desktop, the list and the map sit side by side. On mobile, a toggle switches between List and Map, and the choice is kept in the URL so it can be shared and survives a refresh.
3. **Listing page map**: the `#location` section shows a real map of the villa's location in place of the placeholder.
4. **Host-controlled privacy**: each listing has a location-privacy setting, either **Exact** or **Approximate**. When it is Approximate, guests who have not booked see a 500m area circle on both the search map and the listing page, and the API never sends the real coordinates. Exact coordinates and the address are shown only after a booking is confirmed.
5. **Pin picker in admin**: the listing create and edit form has a map. The host clicks or drags a pin to set the coordinates, the latitude and longitude fields stay in step with the pin, and typing in the fields still works.
6. **No cost to first paint**: map scripts, styles and tiles load only when a map container is about to be shown, through lazy loading triggered when it scrolls into view or on toggle. Pages without a map load nothing extra.
7. **CSP updated narrowly**: both portals' Content-Security-Policy (`web_ui_common/src/security_headers.rs`) allow only the specific tile, style and worker origins the map needs.
8. ~~**Search this area**~~: moved to follow-up issue #124.

---

## 3. Scope Boundaries
### In Scope (MVP)
- A search-results map with markers and a popup card (outcomes 1 and 2).
- A map on the listing page that respects the privacy setting (outcomes 3 and 4).
- A new `location_privacy` attribute on listings, with server-side approximation of public coordinates (outcome 4).
- A pin-picker map on the admin listing form (outcome 5).
- Lazy loading and CSP changes (outcomes 6 and 7).
- **A single map stack (decided 2026-11-13): MapLibre GL with Protomaps tiles**, used for all three maps (search, listing page, admin pin picker). There will be no second provider, which means no Google Maps or Embed API.
  - MapLibre GL is vendored using its CSP build (`maplibre-gl-csp.js` plus a worker file served from our own origin), with its CSS, the `pmtiles` protocol plugin and the Protomaps basemap style. All of these are hashed and served like the vendored htmx in `web_ui_common`. Nothing is loaded from a third-party CDN.
  - A Jamaica-only `.pmtiles` extract, cut with `pmtiles extract` from the Protomaps daily build, along with the glyph and sprite files, is hosted in a **dedicated public map-tile bucket** (one per environment, created by CI; see spec §1.10) and read with HTTP range requests.
  - The bucket's CORS rules allow `GET` with a `Range` header from both portal origins.
  - CI fills each map version automatically: set `MAP_TILES_VERSION_{DEV,PROD}` to a Protomaps build date, and the next run fills it (spec §1.10).
  - The map is limited to Jamaica's bounds (`maxBounds`), and the light and dark basemap styles follow the site theme.

### Out of Scope / Non-Goals
- Forward geocoding or address autocomplete. Hosts can drop a pin, but searching for an address is deferred.
- Collecting or storing a street address field. No such field exists today, and adding one is a separate issue.
- Marker clustering. Five properties in the MVP don't need it, but the design shouldn't rule it out.
- Directions, routing, points of interest, or distance-to-beach calculations.
- Map-based search on the home page.
- Satellite or aerial imagery. Protomaps only provides a street map, and adding imagery would need a second provider.
- A custom tile server or self-hosted tiles. The tiles come from a provider.
- "Search this area" and bounding-box search: deferred to #124.

---

## 4. Affected Systems & Stakeholders
- **User Personas**: Guests browsing and booking, hosts creating and editing listings, and admins.
- **System Components**:
  - `db_core`: a new migration adding the `location_privacy` column, mirrored to the listing history table, and updated listing queries.
  - `common`: listing DTOs, plus a pure function that approximates coordinates, kept deterministic per listing so repeated requests can't be averaged to find the real point.
  - `app_api/listing_api`: the search, detail and create/update handlers. Public responses send approximated coordinates or none, the location-privacy setting is validated on write, and the search can return the map markers.
  - `app_api/booking_api` and `web_ui_guest/src/pages/bookings.rs`: the exact location appears on the guest's booking page only for `confirmed` or `completed` bookings.
  - `web_client`: client methods and types for the new fields.
  - `web_ui_common`: the vendored map library and its CSS, a shared lazy-loading map component, the CSP constants and their tests (`web_ui_common/tests/security_headers.rs`), and the hashed asset pipeline.
  - `web_ui_guest`: `pages/listings.rs` for the map, toggle and markers, and `pages/listing_detail.rs` for the location map.
  - `web_ui_admin`: `pages/listings/form.rs` for the pin picker and the privacy setting.

---

## 5. Constraints & Non-Negotiable Invariants
- **Privacy**: approximate coordinates come from a pure `common` function, `(true_point, keyed_seed) -> approx_point`, using a 500m radius and a keyed HMAC seed. The function is unit-tested so that the result is the same on every call, is never more than 500m from the true point, and is never closer than the minimum distance. When a listing is Approximate, no response to an unauthenticated or non-booked caller may contain the real coordinates, whether rendered HTML, JSON, `data-` attributes or the API itself. The approximation happens on the server and never in the browser.
- **CSP**: no `'unsafe-inline'` and no `'unsafe-eval'`. Any `worker-src` or `blob:` the map library needs must be justified and kept to the minimum. Allowed origins are listed explicitly, with no wildcards beyond the tile provider's documented subdomains.
- **No inline scripts or styles**: map setup reads its configuration from `data-` attributes in the server-rendered markup and runs from the hashed `ui.js` bundle or a lazily loaded module.
- **Tile bucket isolation**: the map-tile bucket holds only public, versioned map assets. It is never used for listing photos or anything uploaded by users, and the CSP allows it by its full bucket path (`https://storage.googleapis.com/<tile-bucket>/`) rather than all of `storage.googleapis.com`. MapLibre fetches tiles and glyphs with `fetch`, so the guest portal's `connect-src` needs this origin, which today is `'self'` only.
- **Tile data and licence**: tiles come only from our own Protomaps extract on GCS. No third-party tile server and no API keys are involved. "© OpenStreetMap contributors" (ODbL) and Protomaps attribution are always visible on the map.
- **Performance budgets**: no change to LCP or first paint on `/listings` and `/listings/{slug}`, because nothing map-related is fetched before the map is needed. The listing services keep their Cloud Run cold-start budget (< 300ms p50) and gain no new blocking dependencies.
- **Coordinates are not money**: `f64` for latitude and longitude stays acceptable. The `Decimal` rule covers money, rates and taxes only.
- **No panics; typed errors**: an out-of-range coordinate or an unknown privacy value returns a typed 400 with a structured error code.
- **Immutable migrations**: add a new timestamped migration and leave existing ones untouched.
- **Graceful degradation**: if JavaScript or tiles fail, the list view and listing page still work in full. The map area shows the city/location text instead, and no blank box is left behind.
- **Accessibility**: the map is supplementary, so every listing in it is also in the list. Markers and popups can be reached by keyboard, and the toggle has a proper accessible name and state.

---

## 6. Success Metrics & Signals
- **Leading Indicators**:
  - [ ] Every listing in a `/listings` result that has coordinates appears as a marker, and clicking it opens a card that links to the right listing.
  - [ ] On a viewport under the `md` breakpoint, the List/Map toggle switches views and the choice survives a refresh through the URL. At `md` and above, both are visible.
  - [ ] For an Approximate listing, an automated test confirms that the public JSON and HTML never contain the stored coordinates, and that repeated requests return the same offset point.
  - [ ] On the guest's own `confirmed` or `completed` booking page, the exact location is shown. For a `pending` or `cancelled` booking, another guest's booking, or a signed-out visitor, it is not, and those requests are refused with a typed 403 or 404. Emails never contain the exact coordinates.
  - [ ] Dragging the admin pin updates the lat/lng inputs, and saving persists the new values.
  - [ ] On pages that have a map, the network trace shows no map JS, CSS or tile request before the map container is visible or toggled.
  - [ ] Map tile load latency (p95) is measured for real guests. This is the signal for deciding later whether to add Cloud CDN.
  - [ ] The CSP tests pass, and the browser console shows no CSP violations on any page that has a map.
- **Lagging Indicators**:
  - Higher click-through from search to listing pages.
  - Fewer guest enquiries asking where a villa is.

---

## 7. Open Questions & Policy Concerns
- [x] **Tile provider**: Protomaps PMTiles (a Jamaica extract) in our GCS bucket. We considered Google Maps (its paid JavaScript API conflicts with our CSP) and the Google Maps Embed API (no multiple markers, popups or pin picker). We rejected both to keep a single map stack.
- [x] **Library**: MapLibre GL, CSP build. The spec must confirm against the current MapLibre docs that `setWorkerUrl` with a worker served from our own origin avoids `worker-src blob:`, and it must list the final CSP directives (`worker-src`, `connect-src` for the bucket and the glyphs).
- [x] **Tile delivery**: serve straight from the GCS bucket, with no Cloud CDN for now (decided 2026-11-13). The extract has a version in its filename (for example `jamaica-2026-11.pmtiles`) and is served with `Cache-Control: public, max-age=31536000, immutable`. Bucket CORS allows `GET` and the `Range` header. Cloud CDN, which needs an HTTPS load balancer at about $18/month fixed, is a follow-up to add only if real-guest map tile latency is slow. The map code must not need to change when the CDN is added: only the tile origin and the CSP change.
- [x] **Tile bucket**: a dedicated public-read bucket for the map, one per environment, kept separate from the listing-photo bucket (decided 2026-11-13). It holds the `.pmtiles` extract, the glyphs, the sprites and the style JSON. *Superseded by spec §1.10: it is created by an idempotent gcloud step in CI, like the photo buckets, and writes use the CI deployer credentials.* It uses uniform bucket-level access, with public read for `allUsers` (`roles/storage.objectViewer`, as on the photo bucket; spec §1.10 explains why the no-listing role was dropped). The CORS rules allow `GET` and `HEAD` from any origin (`*`, public data), and expose `ETag`, `Content-Length` and `Content-Range`.
- [x] **Bucket region and zoom**: the bucket is in the `us-east1` single region. The extract goes up to zoom 15, the Protomaps default, and MapLibre overzooms beyond that for closer views. Decided 2026-11-13.
- [ ] **Edge caching**: during the spec, check whether GCS's built-in edge caching applies to range requests. If it doesn't, we accept serving from `us-east1`, and the p95 tile-latency signal decides whether a CDN is added later.
- [x] **Approximation method** (decided 2026-11-13): the radius is **500m**, and the offset is **deterministic, seeded from the listing ID**. The approximate point is the true point moved by that offset, and guests see a 500m circle drawn around the approximate point. Because the offset is at most 500m, the true location always falls inside the circle.
  - **Keyed seed (security requirement)**: the seed is `HMAC-SHA256(server_secret, listing_id)`, not the listing ID by itself. Listing IDs are public, so an unkeyed seed would let anyone rerun the algorithm and undo the offset. The secret is a new required config value, stable and never rotated casually, because rotating it gives each listing a second approximate point and lets someone narrow down the true one. *Spec §1.4: it is stored as the GitHub secrets `LOCATION_OFFSET_KEY_{DEV,PROD}` and passed to the environment, as `JWT_SECRET` is.*
  - **Minimum distance**: the offset distance is drawn from a band such as 150–500m rather than the whole disc, so the approximate point never lands right on the villa. The spec must fix the lower bound.
  - **Metres to degrees**: the longitude offset is scaled by `cos(latitude)`, which is about 0.95 at 18°N.
  - **The offset is fixed per listing, not per position**: if the host moves the pin, the same offset is applied to the new position. Someone watching would see the change in position, but never the absolute location.
- [x] **How Approximate listings are drawn** (decided 2026-11-13): Approximate listings appear as a **500m circle on both the search map and the listing page**, never as a pin. An offset point that lands offshore is fine.
  - **Click target at low zoom**: on the island-wide search view, a 500m circle is only a few pixels across. The circle needs a minimum on-screen size, or a clickable centre badge such as the price, so it can still be tapped or clicked to open the card. The spec must define this, and it must also work from the keyboard.
  - Exact listings still use a pin.
- [x] **Default privacy** (decided 2026-11-13): **existing listings default to Approximate**. The migration adds `location_privacy` with the values `approximate` and `exact` as `NOT NULL DEFAULT 'approximate'`, and mirrors it in the listing history table. New listings also default to Approximate unless the host chooses Exact, so the safe option applies unless someone opts out.
  - **Deploy order**: because every listing becomes Approximate, the HMAC offset secret has to be configured in each environment before `listing_api` ships. Otherwise the API can't compute public coordinates. If the secret is missing, `listing_api` must not quietly fall back to sending exact coordinates: it either fails to start or leaves coordinates out, and logs the problem.
- [x] **Where the exact location is revealed** (decided 2026-11-13): **only on the guest's booking page, and only once the booking is confirmed.**
  - **Who**: the guest who made the booking. The listing's host and admins always see the exact location in the admin portal.
  - **When**: the booking status is `confirmed` or `completed`. A `pending` booking (a checkout hold, or a request to book or enquiry not yet accepted) does not qualify, and neither does a `cancelled` one.
  - **Where**: the booking page only. It is **not** sent in confirmation or reminder emails (`email_worker`), it is not on the public listing page even when the guest is signed in and has a booking, and it is not in calendar exports.
  - **How**: the exact coordinates come from a booking endpoint that checks authorization (`booking_api`, or `listing_api` checking the booking's ownership). They are never added to the public `Listing` DTO. The access rule follows the same pattern as `booking_policy.rs`, and it is covered by authorization tests for the guest, another guest, a signed-out visitor and the host, each with a pending, confirmed, completed and cancelled booking.
- [x] **Completed bookings** (decided 2026-11-13): the exact location stays visible on the guest's booking page after the stay ends (`completed`). A `cancelled` booking never shows it, including a booking that was confirmed before it was cancelled.
- [x] **Listings without coordinates** (decided 2026-11-13): these are left off the search map; they stay in the results list as usual. On the listing page, the `#location` section shows the city or location text with a "Location not mapped" note in place of a map. No map assets are loaded for that page. Admin form validation keeps coordinates optional.
- [x] **Bounding-box search** (decided 2026-11-13): moved to follow-up #124. Filtering there must use approximate coordinates, so that a guest can't find a listing's exact location by repeatedly shrinking the search area.
- [x] **Exposure of the existing API** (decided 2026-11-13): the redaction ships **as part of this feature**, not as a separate fix. Until #103 is released, the public `Listing` DTO keeps returning exact coordinates; this is an accepted, known risk. The implementation plan must make redaction its **first slice**: the `location_privacy` column, the keyed offset function, and public DTOs that return only approximate coordinates. Every map UI slice builds on it, so no map ships before the redaction. The redaction slice includes a regression test confirming that the public listing and search responses never return the stored coordinates of an Approximate listing.
