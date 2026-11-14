# Spec 103: Map View for Listings

> Driven by GitHub Issue #103. Intent: [intent-103](../intent/intent-103-feat-map-view-for-listings.md) (Approved 2026-11-13). Follow-up: #124 (search this area).

## Overview
Guests can see where villas are. `/listings` gets a map of the results, and each listing page shows its location. Hosts set a listing's coordinates by placing a pin on a map in the admin form.

Each listing has a location privacy setting: **Approximate** (the default, for both existing and new listings) or **Exact**. An Approximate listing shows only a 500m circle, and its real coordinates never leave `listing_api` on public paths. The exact location is shown only to the booking's guest, on `/bookings`, once the booking is `confirmed` or `completed`.

There is one map stack. MapLibre GL JS (its CSP build) is vendored as hashed assets. The tiles are a Jamaica-only Protomaps `.pmtiles` extract, served straight from a dedicated public GCS bucket in `us-east1`. There is no CDN, no third-party tile server and no API key.

**Personas**:
- **Guest**: browses the map and sees the area around each villa. After booking, they see the exact location.
- **Host and Admin**: place the pin and choose the privacy setting. They always see exact coordinates in the admin portal.

**A correction to the intent**: the APIs are deployed with `--no-allow-unauthenticated`, so the internet can't call `GET /api/v1/listings` directly, and no portal page renders coordinates today. The exact-coordinate exposure is latent, not live. It becomes real the moment the map puts coordinates in HTML, so redaction still has to ship first (§9, Phase 1).

---

## 1. Requirements

### 1.1. Location Privacy
1. `listing.location_privacy` is a `location_privacy` enum with the values `approximate` and `exact`. It is `NOT NULL DEFAULT 'approximate'`.
2. The setting can be changed through create and update (`NewListingRequest` and `UpdatedListingRequest` gain `location_privacy: Option<LocationPrivacy>`). If it is left out on create, it is `approximate`. If it is left out on update, it doesn't change.
3. An unknown value returns 400 `VALIDATION_ERROR`, with field `location_privacy`.
4. **Coordinate input** (create and update; added at review): `latitude` and `longitude` are given both or neither, each is finite, and they lie within [−90, 90] and [−180, 180]. Anything else returns 400 `VALIDATION_ERROR`. Today a PATCH with only `latitude` keeps the old longitude through `COALESCE`, and Postgres accepts `NaN` and `Infinity`.
5. **Echo guard** (review blocker): on update of an `approximate` listing, each incoming component (latitude and longitude separately) equal, to 5 decimal places, to that component of `approximate_point(stored)` is treated as **unchanged**: the stored point is kept and a `warn` is logged with the listing ID only. This stops a stale form, which still pre-fills from the public detail, from saving the public point as the stored one. Because the offset is fixed, a saved public point would reveal the offset and with it the real point.

### 1.2. Public Coordinates
**Public coordinates** are what any caller without manager rights receives in `ListingResponse.latitude/longitude`:

| `location_privacy` | Stored coordinates | Public `latitude` / `longitude` | `location_precision` |
|---|---|---|---|
| `exact` | set | the stored values | `"exact"` |
| `approximate` | set | `approximate_point(stored, key, listing_id)` (§1.3) | `"approximate"` |
| either | latitude or longitude is null | both `null` | `null` |

- `ListingResponse` gains `location_precision: Option<LocationPrecision>`. This is the setting that applied to the coordinates in the response, so a guest-facing view knows whether to draw a circle or a pin. It also gains `location_privacy: Option<LocationPrivacy>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`, which is set only on manager paths. Serde can't skip a field on some paths and not others, so this has to be an `Option`.
- **The default is closed** (review): the `api_core` mappers produce `latitude`, `longitude`, `location_privacy` and `location_precision` as `None`. Each path opts in: the public mapper fills public coordinates, and manager paths fill stored ones through `with_stored_location`. A new path, such as #124's, shows no coordinates until someone decides which kind it should get.
- **Public paths** that must return public coordinates: `GET /api/v1/listings` (search) and `GET /api/v1/listings/{id}` (detail).
- **Manager paths** return the stored coordinates: the responses to `POST /api/v1/listings` and `PATCH /api/v1/listings/{id}`, and the new `GET /api/v1/listings/{id}/location` (§2.2). The caller has already passed `require_listing_manager` or `require_host_or_admin`.
- **`user_api` `GET /api/v1/users/{email}/listings`** sets `latitude`, `longitude` and `location_precision` to `null`. It has no offset key and no portal uses it, so it gets no coordinates at all rather than a second copy of the redaction logic.
- The rule lives in exactly one place: a `listing_api` mapper, `public_location(&mut ListingResponse, stored, privacy, &LocationKey)`, which every public path calls. JSON and XML (`Accept: application/xml`) responses are both covered.
- **Logs and traces**: coordinates never appear in span fields or logs. The `#[instrument]` on `db_core::listing::write::create_listing` and `create_listing_with_active_currency` currently records the whole `NewListing`, coordinates included, in its span fields. Both switch to `skip_all` with explicit non-location fields.

### 1.3. Approximation
The function is pure and lives in `common::location::approximate_point(lat, lng, key: &LocationKey, listing_id: Uuid) -> (f64, f64)`.

1. **Seed**: `HMAC-SHA256(key, listing_id.as_bytes())`. The first 8 bytes give `u_angle` and the next 8 give `u_dist`. Each is read as a little-endian `u64`, and its top 53 bits are divided by `2^53`, so each lies in [0, 1) exactly. A plain `as f64 / 2^64` can round up to 1.0.
2. **Bearing**: `θ = 2π · u_angle`.
3. **Distance**: area-uniform within the annulus [150m, 500m], as `d = sqrt(150² + u_dist · (500² − 150²))`.
4. **Offset**:
   - `Δlat = d·cos θ / M` and `Δlng = d·sin θ / (M · cos(lat_rad))`, where `M = 6_371_000 · π / 180` (≈ 111_195 m per degree), the same sphere as the haversine distance and the JavaScript circle
5. **Result**: each component is rounded to 5 decimal places (about 1.1m). Then latitude is clamped to [−90, 90] and longitude is wrapped to [−180, 180). The wrap comes **after** rounding, because 179.999996 rounds to 180.0.
6. **The same offset every time**: for a given listing and key, the offset never changes. If the host moves the pin, the same offset is applied to the new position.

Constants: `APPROX_MIN_M = 150`, `APPROX_MAX_M = 500` (also the displayed circle radius), `APPROX_DECIMALS = 5`. Since `d ≤ 500`, the true point is always inside the 500m circle drawn around the public point. Coordinates are `f64` here; the `Decimal` rule covers money only.

### 1.4. Offset Key
1. `listing_api` reads `LOCATION_OFFSET_KEY` at startup, through a `static LOCATION_OFFSET_KEY_ENV` constant (RS-W1015).
2. The key must be at least 32 bytes once trimmed. If it is missing or shorter, the service refuses to start (`anyhow` context "Refusing to start"), the same way as `jwt_secret()`. There is no fallback, and listing_api never serves exact coordinates in place of approximate ones.
3. `LocationKey` wraps the bytes. Its `Debug` prints `LocationKey(**redacted**)`, and it is never logged. It is shared as `web::Data<LocationKey>`.
4. **Deploy**: GitHub secrets `LOCATION_OFFSET_KEY_DEV` and `LOCATION_OFFSET_KEY_PROD` are passed to `deploy-api` (listing_api only), as `JWT_SECRET` is. The step fails with `::error::` if the secret is empty.
5. **Rotation**: changing the key moves every Approximate point, and someone who saw both old and new points could narrow down the real location. Don't rotate it except after a compromise. Write this in `docs/ENVIRONMENT.md`.

### 1.5. Exact Location for Guests
1. `BookingResponse` gains `listing_location: Option<ListingLocation { latitude: f64, longitude: f64 }>`, with `#[serde(default, skip_serializing_if = "Option::is_none")]`.
2. It is filled only by `GET /api/v1/bookings/booking/{id}` and `GET /api/v1/bookings/user/{id}`, and only when all of these hold:
   - the booking's `status` is `confirmed` or `completed`. The check is done **in the same SQL that reads the location** (`JOIN booking … WHERE b.id = ANY($1) AND b.status IN ('confirmed','completed')`), so a status change between two reads can't reveal the location, and anything outside that allowlist fails closed;
   - the listing may be soft-deleted. A confirmed or completed stay still shows its location, because the guest may be on their way;
   - the existing authorization passed (`authorize_booking_party` for the single booking; for the list, the caller is the user or an admin);
   - the listing has both coordinates.
3. Every other path that returns a `BookingResponse` (updates, requests, enquiries, changes) leaves it `None`.
4. `email_worker` payloads, iCal exports and every `listing_api` response never contain it.
5. **Accepted policy** (review): a guest can book, see the exact location, and then cancel within the free-cancellation window. The exact location is the price of a confirmed booking, and this is recorded as accepted.
6. `listing_locations` returns `HashMap<Uuid, ListingLocation>`, never bare `f64` tuples, so booking_api code still passes the CONSTRAINTS float gate (`! rg "f(32|64)" app_api/booking_api/`).

### 1.6. `/listings` Map
**Layout**
1. The results become a responsive grid (`grid-cols-1 sm:grid-cols-2`) in place of `villa_carousel`. The home page carousel doesn't change.
2. At `md` and above, the layout is two columns: `[results | map]`. The map is `sticky top-24 h-[calc(100dvh-7rem)]`.
3. Below `md`, the guest switches between List and Map with a toggle.

**Toggle**
1. `?view=map` shows the map and `?view=list` (the default) shows the list. Any other value counts as `list`.
2. The toggle is a pair of `<a>` links to the same search with `view` set, so it works without JavaScript. `ui.js` makes it switch in place, without a reload, and updates the URL with `history.replaceState`.
3. The links use `aria-current="page"` on the active view.
4. `view` is never forwarded to the API. `SearchQuery::params` already limits what is forwarded.
5. When the currency changes, `view` is kept along with the rest of the search.

**Markers**
1. Each result card's wrapper (`#listing-card-{slug}`) carries `data-map-lat`, `data-map-lng`, `data-map-precision` and `data-map-price` when it has public coordinates. `map.js` builds the markers from the cards on the page, so the map always shows exactly what the list shows (up to `SEARCH_PAGE_SIZE` = 50).
2. Every listing gets a **price badge**: a DOM `maplibregl.Marker` whose element is a `<button>` labelled with the name and price, at least 44×44px. Badges can be reached with Tab.
3. An Approximate listing also gets a **500m circle**. This is a GeoJSON fill layer: a 64-point polygon around the public point, with radius `APPROX_MAX_M`, built in `map.js`.
4. An Exact listing gets its badge on a pin.

**Card**
1. Clicking a badge, or pressing Enter or Space on it, opens a `maplibregl.Popup`.
2. The popup holds a clone of `<template data-map-card="{slug}">`, a server-rendered compact card with the photo, name, price (or the stay total, as on the list card) and a link carrying the search. It is attached with `setDOMContent`; no HTML string is ever built in JavaScript.
3. Escape closes the popup and returns focus to the badge.

**View**
1. The map fits the bounds of the markers, with 48px padding and a maximum zoom of 13.
2. One marker centres the map at zoom 12.
3. No markers (every listing lacks coordinates): the map shows Jamaica's bounds with an overlay, "These listings aren't on the map yet."

**When there is no map**
1. If the search is `Refused` or `Failed`, or finds no listings, there is no map container.
2. If `MAP_TILES_BASE_URL` isn't configured (§1.10), there is no map container and no toggle.

### 1.7. Listing Page Map
1. The `#location` placeholder gradient becomes a map container. It keeps the same `min-h-56` box, so the layout doesn't shift.
2. Approximate: a 500m circle and no pin, at zoom 13.
3. Exact: a pin, at zoom 15.
4. The copy "The exact address is shared after you book." stays for Approximate. For Exact it becomes "Exact location shown."
5. Gestures:
   - `cooperativeGestures: true`, so the page doesn't trap scrolling;
   - `dragRotate` and `pitchWithRotate` disabled;
   - a `NavigationControl` without the compass.
6. **No coordinates**: the section shows `summary.location()` and the note "Location not mapped", with no map container, so no map assets load.
7. **Coordinates outside the Jamaica bounds** (§1.9): treated the same as no coordinates, and logged at `warn` with the listing ID.

### 1.8. Exact Location on `/bookings`
1. A booking card whose `listing_location` is set shows a "Location" block: a lazy map at zoom 15 with an exact pin, plus the line "Exact location for your stay."
2. Other cards show nothing new.
3. Several cards can each hold a map. Each one loads only when it scrolls into view (§1.11), and an IntersectionObserver ends a map with `map.remove()` once it is more than two viewports away, to cap WebGL contexts.

### 1.9. Admin Pin Picker
**Loading**
1. The edit page loads its coordinates and privacy setting from `GET /api/v1/listings/{id}/location`, with the user's JWT, through a new `web_client` call, `get_listing_location_as`. It no longer takes them from the public detail response.
2. **The location call fails**: the page returns 503 ("We couldn't load this listing's location. Please try again.") and the failure is logged. It must never render the form with public coordinates in the inputs, because saving would overwrite the stored point with the approximate one.

**The map**
1. Beside the existing Latitude and Longitude inputs, which stay as they are, there is a map with one draggable marker.
2. Clicking the map moves the marker there.
3. A `dragend` or click writes both inputs, rounded to 6 decimal places.
4. A valid typed value (within the existing `coord` limits) moves the marker on `change`.
5. With no coordinates, the map shows Jamaica's bounds and has no marker until the first click.
6. `maxBounds` is `JAMAICA_BOUNDS` = `[-78.60, 17.55, -75.95, 18.70]` (W, S, E, N). Typed coordinates outside it are still accepted by the form, and the map shows the note "Outside the mapped area".

**Privacy setting**
1. A radio group labelled "Location shown to guests before booking":
   - "Approximate area (500 m)", the default;
   - "Exact location".
2. Its value is sent as `location_privacy`.
3. Help text under the radio group: "Switching back to Approximate can't hide a location guests have already seen." 
3. On create, the form has no listing ID yet, so the map starts empty. The create response's coordinates are exact, because the caller is the manager.

### 1.10. Tiles and Map Assets
**Bucket**
1. One per environment: `${INFRA_GCS_MAP_TILES_BUCKET_PREFIX}-dev` and `-prod`. It is **not** per branch: feature branches share the dev bucket, so each branch isn't re-uploading tens of MB.
2. It is created in the CI `setup-gcs-image-gcs-items` job, using the existing idempotent `gcloud storage buckets describe || create` pattern, with:
   - `--location=us-east1`
   - `--uniform-bucket-level-access`
   - `--no-public-access-prevention`
   - `allUsers` → `roles/storage.objectViewer`, the same as the photo bucket. This also lets anyone list the bucket, which exposes only the version folder names, and those are public data anyway. `legacyObjectReader` (read objects, no listing) was dropped because it isn't clear whether it can be granted on a bucket with uniform bucket-level access.
   - Implemented as the step **"Create Map Tiles Bucket"** in `setup-gcs-image-gcs-items` (added 2026-11-14).
3. **CORS**: `[{origin:["*"], method:["GET","HEAD"], responseHeader:["Content-Range","Content-Length","ETag","Accept-Ranges"], maxAgeSeconds:86400}]`.
   - `*` is acceptable because the bucket holds only public, versioned map data and no credentials are ever sent.
   - It also avoids listing every branch's Cloud Run portal URL.
4. **Writes**: only the CI deployer credentials (`GCP_CREDENTIALS`), used by the populate step.
5. **Names**: `-dev` and `-prod` (decided 2026-11-13), from `INFRA_GCS_MAP_TILES_BUCKET_PREFIX` = `our-places-gcs-map-tiles`, which is already set. Dev uses `-dev` for every branch except `main`; `main` uses `-prod`.

**Layout** (each version is never changed once uploaded):

```text
gs://<bucket>/v<YYYYMMDD>/jamaica.pmtiles
gs://<bucket>/v<YYYYMMDD>/style-light.json  style-dark.json
gs://<bucket>/v<YYYYMMDD>/sprites/…          fonts/{fontstack}/{range}.pbf
gs://<bucket>/v<YYYYMMDD>/_READY             ← written last
```

The version **is the Protomaps daily build date** (`YYYYMMDD`), so every folder can be traced to, and rebuilt from, one specific upstream build.

- Every object is served with `Cache-Control: public, max-age=31536000, immutable`.
- `jamaica.pmtiles` is uploaded **without** `Content-Encoding` (`gcloud storage cp --no-gzip…` with no `-z` and no `-Z`). GCS decompressive transcoding ignores `Range`, and the map would break.

**Populating the bucket automatically in CI** (decided 2026-11-13; build and upload were split at review, 2026-11-14)
1. **Version variables**: `MAP_TILES_VERSION_DEV` and `MAP_TILES_VERSION_PROD`, each a Protomaps build date such as `20261101`. A value that isn't 8 digits fails the job.
2. **Building runs with no secrets** (review: security). A new job, `build-map-tiles`, needs nothing and has **no GCP auth and no secrets**.
   1. It checks `https://storage.googleapis.com/<bucket>/v<version>/_READY` with an anonymous `curl`, which works because the bucket is public. A `200` means it outputs `needed=false` and stops.
   2. Otherwise it runs `scripts/build_map_tiles.sh <version> <base_url>`:
      - downloads the pinned `pmtiles` CLI and verifies its sha256;
      - runs `npm ci` from a committed `scripts/map-tiles/package-lock.json` that pins `@protomaps/basemaps`;
      - fetches `protomaps/basemaps-assets` by commit SHA with `git`;
      - extracts Jamaica (`--bbox=-78.60,17.55,-75.95,18.70 --maxzoom=15`) from `https://build.protomaps.com/<version>.pmtiles`;
      - generates the light and dark styles, with no source attribution HTML (see §1.11);
      - writes `MANIFEST` with the sha256 of each file.
   3. It uploads the result as a workflow artifact.

   Downloaded code therefore never runs where the deployer key exists.
3. **Uploading is a step in `setup-gcs-image-gcs-items`**. "Upload Map Tiles" runs after "Create Map Tiles Bucket", only when `needed=true`, and only once it has downloaded the artifact and checked it against `MANIFEST`.
   - **Lock**: `gcloud storage cp --if-generation-match=0` of `v<version>/_LOCK` succeeds for exactly one run, and that run releases it in an `EXIT` trap. A run that loses waits, polling for `_READY` for up to 10 minutes. On timeout it warns and continues with maps off; it doesn't fail the job. A `_LOCK` older than 10 minutes with no `_READY` is abandoned, and is deleted with a generation-matched delete. There is **no** job-level `concurrency` group: GitHub keeps only one pending run per group and would cancel other dev branches' setup jobs.
   - **Upload**: `Cache-Control: public, max-age=31536000, immutable`, with no `Content-Encoding` on `jamaica.pmtiles`. Afterwards, a ranged GET must return `206` before `_READY` is written **last**. `_READY` holds the build date, the tool versions and the manifest's sha256.
4. **Prod copies from dev** (review): when `v<version>/_READY` exists in `-dev`, the prod run copies that verified folder (`gcloud storage cp -r`, checked against its `MANIFEST`) instead of rebuilding. Protomaps keeps daily builds only briefly, so a prod date set weeks after dev can't depend on the build still being available. The build job is skipped for that case.
5. **Failure**: if the upstream build has expired or a download fails, the job fails, `_READY` is never written, and the portal deploys don't run. The next run retries.
6. **Updating and rolling back**: change the variable. Old folders are kept, and deleted only by hand.
7. **Version unset**: build and upload skip, and the portals run with no maps.

**Portal configuration**
1. `MAP_TILES_BASE_URL` (both portals) is the full base URL, e.g. `https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261101`. It is built by `deploy-web-ui-guest` and `deploy-web-ui-admin` from the bucket prefix, the environment suffix and `MAP_TILES_VERSION_{DEV,PROD}`, and only when `v<version>/_READY` exists for the **configured** version, checked in the deploy job. Otherwise it is empty, and the portals run with no maps. There is no "last populated version" fallback. The deploy job also compares `jamaica.pmtiles`'s `crc32c` (`objects describe`) with the value recorded in `_READY`.
2. **Validation is an allowlist** (review): the value must match `^https://storage\.googleapis\.com/[a-z0-9][a-z0-9._-]{1,61}[a-z0-9]/v\d{8}$`. In local development it may instead match the same shape under `common::gcs::emulator_endpoint()`, and only when that endpoint is set. Anything else counts as unset, with one `warn`. An empty bucket name, `?`, `#`, `%`, `..`, whitespace and a trailing `/` are all refused.
3. **Unset**: maps are not rendered (§1.6, §1.7), and there is one `warn` at startup. The portal still starts, because maps are an add-on.

**Attribution**: the style sources carry `© OpenStreetMap contributors` (linked to the ODbL copyright page) and `Protomaps`. `AttributionControl({compact: true})` is always on.

### 1.11. Lazy Loading and Client Code
**Assets** (in `web_ui_common/src/assets/`, each declared with `asset!(…, checksum: "sha256:…")`, like htmx):

| Asset | Notes |
|---|---|
| `maplibre-gl-csp-<v>.js` | Exact version pinned when implemented; at least two weeks old |
| `maplibre-gl-csp-worker-<v>.js` | |
| `maplibre-gl-<v>.css` | |
| `pmtiles-<v>.js` | |
| `map.js` | Ours: all map setup |

**Container**: every map is a `<div data-map="search|listing|booking|picker">` that carries:
- the hashed asset URLs (`data-map-js`, `data-map-worker`, `data-map-css`, `data-pmtiles-js`, `data-map-init`);
- `data-tiles-base`;
- for single-point maps, `data-lat`, `data-lng` and `data-precision`.

The container reserves its height, so loading causes no layout shift.

**Loading**
1. `ui.js` watches `[data-map]` with an IntersectionObserver (`rootMargin: "200px"`). It also loads a map when a hidden map becomes visible after the toggle.
2. On the first trigger it adds `<link rel=stylesheet>` and then the scripts, in order, once per page, and calls `OurPlacesMap.init(el)` from `map.js`.
3. Before that trigger, the page makes **no** request for map JS, CSS, styles or tiles.

**Map setup**
1. `maplibregl.setWorkerUrl(data-map-worker)` before the first map is created, so no `blob:` worker is needed.
2. `pmtiles.Protocol` is registered once.
3. Style: `style-light.json` for the `light` theme, `style-dark.json` for `dim` and `dark`. When the theme toggle changes the theme, `map.setStyle(...)` follows.
4. `maxBounds` is `JAMAICA_BOUNDS` and `maxZoom` is 17 (overzoomed above 15).

**Lifecycle** (review):
- each container has an init guard, using a generation token, so a `remove()` during a pending init doesn't leak a WebGL context;
- a map initialized while hidden calls `resize()` when the toggle shows it;
- DOM markers are created once and are **not** added again on `style.load`, because they survive `setStyle`. Only the circle layers and sources are added again.

**Attribution**: `map.js` sets `customAttribution` to fixed text: "© OpenStreetMap contributors", linked, and "Protomaps". The generated styles carry no source `attribution` HTML, so no HTML coming from the bucket is ever rendered.

**Failure**: if a script fails to load, WebGL is unavailable, the first `load` event hasn't fired within **10 seconds**, or a style or tile request fails before it:
- the container shows "Map unavailable" with the location text;
- the list and the page keep working;
- one `console.warn` is written, and nothing is sent to the server.

### 1.12. Content-Security-Policy
`SecurityHeaders` gains `.with_map_origin(Option<String>)`, which takes the validated `MAP_TILES_BASE_URL` and keeps only its path up to and including the bucket, with a trailing `/` (e.g. `https://storage.googleapis.com/<bucket>/`). When it is set, both portals' policies change as follows:

| Directive | Change |
|---|---|
| `connect-src` | add `https://storage.googleapis.com/<bucket>/`: the style, tiles (range fetches), glyphs and sprite JSON |
| `img-src` | add `https://storage.googleapis.com/<bucket>/` (sprite PNG), and `blob:` only if the Phase 0 spike shows MapLibre needs it |
| `worker-src` | new: `'self'` |

**Composition order**: `csp_with_storage_origin` currently rewrites every occurrence of `https://storage.googleapis.com`. It must run **before** `with_map_origin`, or be changed to replace only the host as a whole token, so the bucket path is never broadened. A test covers both origins being set together.

These don't change:
- `script-src 'self'` and `style-src 'self'`, with no `'unsafe-inline'` and no `'unsafe-eval'`.
- All of `storage.googleapis.com` is never allowed in `connect-src` on the guest portal.

---

## 2. Technical Implementation

### 2.1. Data Model
**Migration**: `sqlx migrate add map_view_location_privacy`. The last migration, `20261113033926`, is already on `origin/dev`, so this is a new file.

```sql
CREATE TYPE location_privacy AS ENUM ('approximate', 'exact');
ALTER TABLE listing ADD COLUMN location_privacy location_privacy NOT NULL DEFAULT 'approximate';
ALTER TABLE listing_history ADD COLUMN location_privacy location_privacy;
```

**`common`**
- `common::location`:
  - `LocationPrivacy` and `LocationPrecision`: serde `snake_case`, `sqlx::Type` behind the existing `sqlx` feature, as `BookingMode` is;
  - `LocationKey`;
  - `approximate_point`;
  - `JAMAICA_BOUNDS`;
  - the `APPROX_*` constants;
  - `in_bounds(lat, lng)`.
- `hmac` is added as a workspace dependency (RustCrypto, the same family as the existing `sha2`). `cargo audit` must stay clean.

**`db_core`**
- Every listing `SELECT` in `listing/queries.rs` and every `RETURNING` in `listing/write.rs` adds `location_privacy AS "location_privacy: LocationPrivacy"`.
- Create and update bind it, using `COALESCE($n, location_privacy)` on update.
- The `listing_history` insert in `update_listing` (`write.rs:150`) records it.
- New `db_core::booking::listing_locations(pool, &[booking_id]) -> HashMap<Uuid, ListingLocation>`, keyed by booking ID. It is a single query, `SELECT b.id, l.latitude, l.longitude FROM booking b JOIN listing l ON l.id = b.listing_id WHERE b.id = ANY($1) AND b.status IN ('confirmed','completed') AND l.latitude IS NOT NULL AND l.longitude IS NOT NULL`, so the bookings list is not N+1 and the status check runs in the same read. It is skipped when no booking qualifies.
- **Create binds privacy with a default** (review): the create `INSERT` names its columns explicitly, so a NULL bind would violate `NOT NULL`. Bind `COALESCE($n, 'approximate'::location_privacy)`, or `unwrap_or_default()` in Rust.
- **The search query is runtime-built** (`QueryBuilder` in `get_listings`, `queries.rs:112`), so `sqlx prepare --check` won't catch a missing column. A DB test must run `get_listings` and read `location_privacy`.
- Run `cargo sqlx prepare --workspace` afterwards.

### 2.2. API

| Method and path | Service | Auth | Response |
|---|---|---|---|
| `GET /api/v1/listings` | listing_api | none (IAM) | `ListingResponse[]` with public coordinates and `location_precision` |
| `GET /api/v1/listings/{id}` | listing_api | none (IAM) | `ListingDetails` with public coordinates |
| `GET /api/v1/listings/{id}/location` **(new)** | listing_api | `Claims` + `require_listing_manager` | `200 {latitude, longitude, location_privacy}`, sent with `Cache-Control: no-store`; 401, 403 (`warn`), 404 |
| `POST /api/v1/listings`, `PATCH /api/v1/listings/{id}` | listing_api | as today | accept `location_privacy`; return stored coordinates and `location_precision` = the stored setting |
| `GET /api/v1/bookings/booking/{id}` | booking_api | as today | `listing_location` per §1.5 |
| `GET /api/v1/bookings/user/{id}` | booking_api | as today | `listing_location` per §1.5, filled by one `listing_locations` call |
| `GET /api/v1/users/{email}/listings` | user_api | as today | coordinates and `location_precision` are `null` |

- The new route is registered in the `/api/v1/listings` scope, with the same `content_negotiation_middleware` and an `#[instrument(skip_all)]`.
- It is added to the OpenAPI `paths` and `schemas`, along with `LocationPrivacy`, `LocationPrecision` and `ListingLocation`.
- Errors use the existing `ApiError` shapes. There are no new error codes.

### 2.3. Frontend

| Crate | Change |
|---|---|
| `web_ui_common` | `assets.rs`: the vendored MapLibre, pmtiles, CSS and `map.js` constants. `components/map.rs`: `map_container(kind, point, precision)` renders the container and its `data-*` attributes, or nothing when maps aren't configured. `security_headers.rs`: `with_map_origin`. `ui.js`: lazy loader and toggle enhancement. `map.js`: init for the 4 kinds, badges, circles, popups, picker and theme. `domain/listing_card.rs`: `ListingSummary` gains `map_point: Option<MapPoint{lat,lng,precision}>`, and a `map_card_template` component. |
| `web_ui_guest` | `pages/listings.rs`: grid, sticky map, toggle, `view` param, templates. `pages/listing_detail.rs`: the `#location` map or the "Location not mapped" note. `pages/bookings.rs`: the location block. `main.rs`: read `MAP_TILES_BASE_URL` and pass it to the context and `SecurityHeaders`. |
| `web_ui_admin` | `pages/listings/form.rs`: picker, privacy radios, `location_privacy` in the create and update payloads. `pages/listings/edit.rs`: prefill from `get_listing_location_as`, 503 on failure. `main.rs`: as in the guest portal. |
| `web_client` | `get_listing_location_as(id, jwt)`. Re-exports the new `common` types. |

### 2.4. CI and Infrastructure
1. **`cicd_full_stack.yml`**:
   - `setup-gcs-image-gcs-items`: the tile bucket steps (§1.10), which create the bucket, grant public object read and apply CORS. They run again on every run, so the settings can't drift;
   - a new job, `build-map-tiles`, with no secrets (§1.10), and the step "Upload Map Tiles" after "Create Map Tiles Bucket" (done), which uses the `_LOCK` object;
   - `deploy-api`: `LOCATION_OFFSET_KEY` for listing_api, with the empty-secret guard;
   - the two portal deploys: `needs` the setup job with the `always()` guard, a `_READY` check, and `MAP_TILES_BASE_URL`.
2. **New**:
   - `scripts/build_map_tiles.sh`, which runs with no credentials and pins the `pmtiles` CLI (with its sha256), `@protomaps/basemaps` through `scripts/map-tiles/package-lock.json`, and the `basemaps-assets` commit;
   - `scripts/upload_map_tiles.sh`, which handles the lock, the manifest check, the upload, the 206 check and `_READY`, and the prod copy from dev.

   Both are idempotent and safe to rerun.
3. **Repository variables**: `INFRA_GCS_MAP_TILES_BUCKET_PREFIX` (set 2026-11-14), `MAP_TILES_VERSION_DEV` and `MAP_TILES_VERSION_PROD` (not set yet; set one to a recent build date to turn maps on). **Secrets**: `LOCATION_OFFSET_KEY_DEV` and `LOCATION_OFFSET_KEY_PROD` (set 2026-11-14).
4. **Docs**: `docs/ENVIRONMENT.md` (the new variables, the key-rotation warning) and `docs/ARCHITECTURE.md` (the map data flow).

---

## 3. Edge Cases & Failure States

| Case | Behaviour |
|---|---|
| Listing has latitude but no longitude, or the reverse | Treated as no coordinates everywhere |
| Coordinates outside `JAMAICA_BOUNDS` | No marker. The listing page shows "Location not mapped" (`warn`). The picker shows "Outside the mapped area" |
| Approximate point lands offshore | Fine (intent decision). The circle still covers the true point |
| Host switches Exact → Approximate | The next response has the offset point. Coordinates already cached in a guest's browser can't be recalled (accepted) |
| Host moves the pin | Same offset applied to the new point (§1.3.6) |
| Admin edit: the location endpoint fails | 503. The form is never shown with public coordinates (§1.9) |
| Admin saves with empty lat/lng | `COALESCE` keeps the stored values, as today |
| Booking is cancelled after being confirmed | `listing_location` is `None` from then on |
| Booking `pending` (hold, request or enquiry) | `None` |
| Guest opens another guest's booking | 404, as today; never `listing_location` |
| `LOCATION_OFFSET_KEY` missing or too short | listing_api doesn't start; the deploy step fails first |
| `MAP_TILES_BASE_URL` unset or invalid | No maps, no toggle, no CSP change; pages work as today |
| JavaScript off | The toggle links still switch views server-side. The map container shows "Map needs JavaScript" in `<noscript>`, and the list is always available |
| WebGL unavailable, a tile fetch fails, or an old browser | "Map unavailable" fallback (§1.11) |
| `pmtiles` file uploaded gzip-encoded | Range requests return the whole body and the map breaks. The script uploads without `Content-Encoding`, and the Phase 0 check catches it |
| Tile version bumped while a page is open | Old versions are never deleted automatically, so open pages keep working |
| `MAP_TILES_VERSION_*` names an expired Protomaps build | `build-map-tiles` fails and the portals aren't deployed. Pick a recent build date. Prod copies the verified dev folder, so this mostly affects dev |
| CI run fails halfway through populating | No `_READY`. The next run repopulates. The portals were never pointed at the folder |
| Two CI runs fill the same version at once | The `_LOCK` object (`--if-generation-match=0`) lets one run upload. The other waits for `_READY`, up to 10 minutes. A stale lock (over 30 minutes, no `_READY`) is cleared |
| Admin portal on an old revision saves the public point | The echo guard (§1.1.5) keeps the stored point and logs `warn` |
| Booking status changes between reads | The status check is in the same SQL as the location read (§1.5) |
| 50 results with coordinates | 50 DOM badges; acceptable. Clustering is a non-goal |
| Theme toggled while a map is open | `setStyle`; markers and the circle layer are added again on `style.load` |
| Several booking cards with maps | Lazy load per card; maps far off-screen are removed (§1.8) |
| A Range request triggers a CORS preflight | Not expected (single `bytes=` ranges are safelisted). CORS still answers `GET` and `HEAD` for `*` |

---

## 4. Performance Considerations
- **First paint**: unchanged. Nothing map-related loads before a container is about to be shown. Container heights are reserved, so CLS is 0.
- **Map payload**: loaded lazily, once per page. MapLibre's CSP build is about 220KB gzipped plus its worker. Phase 0 must confirm that Topcoat's asset serving sends compressed responses, because Cloud Run doesn't compress. If it doesn't, precompressed `.br` and `.gz` variants are a Phase 0 decision.
- **Tiles**: a first view is the header and root directory (one range) plus about 15–30 tile ranges from `us-east1`. Later views come from the browser cache (`immutable`).
- **listing_api**: one HMAC-SHA256 per listing that has coordinates, a few µs each, which is negligible for 50 rows. There are no new queries on search or detail; `location_privacy` is a column on the same row. The key is parsed once at startup, so the cold start doesn't change.
- **booking_api**: one extra indexed `ANY($1)` query per bookings-list call, on the primary key. Not N+1.
- **Memory**: no new server-side caches. Portal assets live in the bundle directory, not in memory, so the 256MB limit isn't affected.
- **Bucket egress**: public read on a us-east1 bucket. Add a billing budget alert on GCS egress for the tile bucket (operational, not code).

---

## 5. Security Mitigations

| Threat | Mitigation |
|---|---|
| Exact location leaks for Approximate listings | A single redaction mapper on every public path (§1.2). `user_api` returns `null`. Regression test: serialized public JSON and rendered HTML never contain the stored values (§6) |
| Reversing the offset | Keyed HMAC seed (§1.3). The key is secret, at least 32 bytes and never logged. Without it the offset can't be predicted |
| Averaging repeated requests | Deterministic offset: every request returns the same point |
| Searching ever-smaller boxes to find the true point | No box search here. #124 must filter on public coordinates |
| IDOR on the location endpoint | `require_listing_manager`: not the owner is 403 (`warn`), unknown or deleted is 404 |
| IDOR on booking location | Existing `authorize_booking_party` returns 404 to anyone who isn't a party, plus the status gate |
| Edit form overwrites the true point with the approximate one | Prefill only from the manager endpoint; 503 on failure (§1.9) |
| XSS through the popup | Cards are server-rendered `<template>`s escaped by Topcoat and attached with `setDOMContent`. `map.js` never uses `innerHTML` |
| CSP loosened too far | Only the bucket path is added, plus `worker-src 'self'`. No `unsafe-*`. `MAP_TILES_BASE_URL` is validated against the directive-breaking characters, like `csp_with_storage_origin` |
| Supply chain (vendored JS) | Exact versions with `sha256` checksums in `asset!`, checked against the npm tarball. `build_map_tiles.sh` pins its tools and checksums them, and runs with no secrets. Licences: MapLibre BSD-3, pmtiles BSD-3; data ODbL with attribution |
| Bucket abuse | Public read via `objectViewer`, which allows listing; it exposes only version folder names (§1.10). CORS `GET` and `HEAD` only. Writes only from CI credentials. Downloaded build tools run in a job with no secrets. An egress budget alert is set up by hand (plan T22) |
| Tampered upstream build | The CLI checksum is pinned, and the extract comes from a named build date. `_READY` records the file's sha256, so a later run or a person can check the object hasn't changed |
| Secret in source | The key comes only from a GitHub secret into the environment. Nothing goes in `.env.example` beyond a placeholder |
| Key readable from Cloud Run config | Anyone with `run.services.get` can read plaintext env vars. Accepted for parity with `JWT_SECRET` (originator decision, 2026-11-13). Moving both to Secret Manager is a follow-up |
| Leaks that remain (accepted and documented) | The same property listed under two IDs gets two independent offsets, and their circles intersect. `city` is reverse-geocoded from the exact point and can fall back to a neighbourhood name. Free-text `listing_details` written by the host may mention an address. Switching Exact → Approximate can't un-publish a point. Photos are safe: guests get only re-encoded WebP with no EXIF, and the raw originals stay private |

---

## 6. Test Plan
Run targeted tests while iterating, and the whole suite once at the end.

**Unit (`cargo test -p common`)**
- **`approximate_point`**:
  - deterministic for the same key and ID;
  - a different key or ID gives a different point;
  - over 10,000 random IDs, the haversine distance is in [150, 500 + 1.5]m (the margin covers rounding);
  - the longitude scaling holds at 18°N;
  - latitude clamps and longitude wraps near the limits;
  - output has at most 5 decimal places.
- **`LocationKey`**: refuses keys under 32 bytes; `Debug` is redacted.
- **`LocationPrivacy`**: serde round trip; unknown values are refused.
- **`in_bounds`**: edges.

**Integration**
- **`listing_api`**:
  - search and detail for an Approximate listing: the response JSON contains neither stored value as a string, the public point is within 500m, and `location_precision` is `"approximate"`;
  - an Exact listing returns the stored values;
  - no coordinates gives `null`s;
  - the location endpoint: owner 200, admin 200, another host 403, no JWT 401, deleted 404;
  - create defaults to `approximate`;
  - update changes the setting, and leaving it out keeps it;
  - an unknown value is 400;
  - a history row records the setting;
  - startup refuses without the key.
- **`booking_api`**: `listing_location` for each status (`pending`, `confirmed`, `completed`, `cancelled`) and each caller (guest, host, admin, other guest → 404), for both the single and list endpoints. The list endpoint makes one locations query. Other `BookingResponse` paths return `None`.
- **`user_api`**: the user's listings have null coordinates.
- **`db_core`**: the migration default for existing rows; `listing_locations` skips rows with null coordinates.

**Web UI (`cargo test -p web_client -p web_ui_common -p web_ui_guest -p web_ui_admin`)**
- **`/listings`**:
  - cards carry the `data-map-*` attributes with **public** coordinates, plus one template per card;
  - `view=map` and `view=list` render, and an invalid value counts as list;
  - `view` isn't sent to the API;
  - no map on Refused, Failed or empty;
  - no map when `MAP_TILES_BASE_URL` is unset.
- **Listing page**: map container for Exact and Approximate; "Location not mapped" when there are no coordinates or they are out of bounds.
- **`/bookings`**: the location block appears only when `listing_location` is set.
- **Admin edit**: prefilled from the location endpoint; a failure is 503 with no coordinate inputs; the privacy radio is posted.
- **CSP** (`web_ui_common/tests/security_headers.rs`):
  - the map origin is added only to `connect-src` and `img-src`;
  - `worker-src 'self'`;
  - an invalid origin is refused;
  - no `unsafe-*`.

**E2E (Playwright, `/playwright-e2e`)**
- **`/listings` on desktop**:
  - no map requests before load;
  - the map loads;
  - the number of badges equals the number of cards with coordinates;
  - clicking a badge opens the card, and its link goes to the listing;
  - Tab and Enter work on the badges;
  - the console has no CSP violations.
- **Mobile viewport**: the toggle switches views, and `?view=map` survives a reload.
- **Listing page**: the map loads only after scrolling to `#location`. Approximate shows a circle and Exact shows a pin.
- **Guest with a confirmed booking**: the exact pin is on `/bookings`. A pending booking has none.
- **Admin**: dragging the pin updates the inputs; save, then reload shows the stored values; the privacy radio persists.

---

## 7. Phase 0 Spike (before Phase 2)
Record the results in this spec before building the UI.
- [ ] Confirm against the current MapLibre docs and dist that the pinned MapLibre CSP build runs with `setWorkerUrl` under the §1.12 CSP, with no violations. Decide on `img-src blob:`.
- [ ] Topcoat asset compression (§4).
- [ ] With a test `pmtiles` file in a us-east1 bucket: range requests return 206, CORS works from a portal origin, and there is no transcoding. Record whether GCS edge caching applies to range responses (the open question in the intent). If it doesn't, we accept serving from `us-east1` and track the p95 tile latency.
- [ ] The size of the Jamaica z15 extract, the fonts and the sprites.

## 8. Exclusions / Non-Goals
- Search by visible area: #124.
- Forward geocoding and address autocomplete.
- A street address field.
- Marker clustering.
- Routing and directions.
- Points of interest.
- Satellite imagery.
- A home-page map.
- A CDN in front of the tiles (deferred; triggered by the p95 tile latency).
- Exact location in emails or iCal.
- Exact location for guests on the admin portal or the public listing page.

## 9. Delivery Order
1. **Phase 1, redaction** (ships first; no map is visible yet):
   - the migration;
   - `common::location`;
   - the key at startup and in CI;
   - the public mapper;
   - `user_api` nulls;
   - the location endpoint;
   - the admin edit form switched to the location endpoint, with the privacy radio;
   - the regression tests.
2. **Phase 0 spike.**
3. **Phase 2, map infrastructure**: the `build-map-tiles` job and the "Upload Map Tiles" step with their scripts (the bucket step is already in `cicd_full_stack.yml`), the portal-deploy `needs` and the `_READY` and `crc32c` guard, and setting `MAP_TILES_VERSION_DEV` so the first push fills dev; vendored assets, `map.js`, the lazy loader, CSP and `MAP_TILES_BASE_URL`.
4. **Phase 3, guest maps**: the listing page, then `/listings` (grid, toggle, badges, popups).
5. **Phase 4, reveal and picker**: `listing_location` in booking_api and the `/bookings` block; the admin pin picker.
6. **Phase 5**: E2E and docs. Set `MAP_TILES_VERSION_PROD` before merging to `main`, so the `main` run creates and fills `-prod`.

---

## Acceptance Criteria
1. [ ] Every existing and new listing defaults to `approximate`, and hosts can choose `exact` in the admin form.
2. [ ] No public listing response, and no guest-portal HTML, contains the stored coordinates of an Approximate listing (automated test).
3. [ ] Approximate public points are deterministic and 150–500m from the true point, and use a keyed seed.
4. [ ] listing_api refuses to start without a `LOCATION_OFFSET_KEY` of at least 32 bytes, and CI refuses to deploy without it.
5. [ ] `/listings` shows a sticky map beside a results grid at `md` and above, and a List/Map toggle (`?view=`) below `md`.
6. [ ] Each result with coordinates has a keyboard-reachable price badge. Approximate listings also have a 500m circle. Clicking a badge opens a card that links to the listing.
7. [ ] The listing page shows a circle (Approximate) or a pin (Exact), or "Location not mapped".
8. [ ] The exact location appears on `/bookings` only for the guest's `confirmed` or `completed` bookings, and never in emails.
9. [ ] The admin edit form loads exact coordinates from `GET /api/v1/listings/{id}/location`. The pin and the inputs stay in step, and a load failure never renders the form with public coordinates.
10. [ ] No map JS, CSS or tile request happens before a map container is visible or toggled on. Container heights are reserved.
11. [ ] CI creates the `-dev` and `-prod` tile buckets and fills `v<YYYYMMDD>/` once per version, gated on `_READY`. Runs where the folder is already filled skip in seconds. Tiles come only from the dedicated `us-east1` bucket. The CSP adds only that bucket path and `worker-src 'self'`, with no `unsafe-*`, and the console shows no CSP violations.
12. [ ] With `MAP_TILES_BASE_URL` unset, or JavaScript or WebGL unavailable, every page still works and shows the location text.
13. [ ] OSM and Protomaps attribution is visible on every map.
14. [ ] Every gate in CONSTRAINTS.md passes: fmt, clippy `-D warnings`, `missing_docs`, no unwrap or expect, env-var hygiene, `sqlx prepare --check`, backend and web UI tests, `cargo audit`.
