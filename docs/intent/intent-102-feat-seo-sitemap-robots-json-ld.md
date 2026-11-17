# Intent: SEO — sitemap.xml, robots.txt and JSON-LD Structured Data

## Metadata
- **Author**: Originator (via GitHub Issue #102)
- **Date**: 2026-11-14
- **Status**: Approved (all questions resolved 2026-11-14)
- **Target Area**: Frontend (`web_ui_guest`, `web_ui_common`), with possible read-path additions in `app_api/listing_api`, `web_client`, `db_core`

---

## 1. Problem Statement & Motivation
Search engines are the main sales channel for a villa site, and today the guest portal gives them very little to work with:

- There is no `/sitemap.xml`, so crawlers find listings only by following links from `/listings`.
- There is no `/robots.txt`. Crawlers are free to hit auth-only and transactional routes (`/checkout/{slug}`, `/bookings`, `/profile`, `/requests`, messages), and the dev environment can be indexed alongside prod.
- Listing pages carry no structured data, so they can't show up as rich results with price, rating, photos or location.
- The issue mentions "some OpenGraph tags", but the guest `<head>` (`web_ui_common/src/shell_guest.rs`) currently emits only `charset`, `viewport`, `<title>` and `htmx-config`. There are **no** OpenGraph, Twitter card, `description` or canonical tags, so shared links render with no preview and duplicate URLs (query strings, `run.app` hosts) are not consolidated.

---

## 2. Proposed Outcome (The Vision)
1. **`/sitemap.xml`** served by `web_ui_guest`: static public pages (`/`, `/listings`, `/about`, the four policy pages) plus one `<url>` per active, non-deleted listing (`/listings/{slug}`) with `<lastmod>`. Valid against the sitemaps.org 0.9 schema, with absolute URLs on the canonical origin.
2. **`/robots.txt`** served by `web_ui_guest`, driven by an explicit `SEO_INDEXABLE` flag (default `false`; Q3):
   - **Indexable (prod only)**: allow public pages; disallow `/checkout/`, `/bookings`, `/profile`, `/requests`, `/reviews/submit`, `/login`, `/logout`, `/register`, `/verify`, `/currency/`, and listing sub-routes that aren't content (`/listings/*/quote`, `/listings/*/enquire`, `/listings/*/calendar`); include `Sitemap: <canonical-origin>/sitemap.xml`.
   - **Not indexable (every other environment)**: `Disallow: /` for all agents, plus an `X-Robots-Tag: noindex, nofollow` header on every guest response.
   - **Admin portal** (`web_ui_admin`, every environment): `/robots.txt` returns `Disallow: /`, and every response carries `X-Robots-Tag: noindex, nofollow` (Q7).
3. **JSON-LD on listing detail pages** (`/listings/{slug}`): a `VacationRental` (Q5), including the properties Google requires for that type (`containsPlace` with an `Accommodation` and its `occupancy`, bedrooms, bathrooms), with name, description, URL, images, `address` (locality, country), `geo` (public coordinates only), amenities (`amenityFeature`), occupancy/bedrooms, `aggregateRating` from `ListingRatingSummary` (only when `review_count > 0`), and an `Offer` with the base nightly rate (`price_per_night`) in the listing's `base_currency` (Q6). The `Offer` is left out when `price_per_night` is `None`.
4. **Canonical + social meta** on home and listing detail pages, and a default on other public pages. Each page's canonical is its own path without the query string (Q8). Pages without a listing photo use the default OG image (Q9). The tags are: `<link rel="canonical">`, `<meta name="description">`, OpenGraph (`og:title`, `og:description`, `og:url`, `og:image`, `og:type`, `og:site_name`) and Twitter card (`summary_large_image`) tags.
5. **CSP-compatible emission**: JSON-LD ships as `<script type="application/ld+json">`, and the policy in `web_ui_common/src/security_headers.rs` (`script-src 'self'`, no `'unsafe-inline'`) stays unweakened.
6. **Playwright coverage** (`playwright/`): checks that `/sitemap.xml` parses and lists only live listings, that `/robots.txt` carries the expected rules for the environment, and that each listing page's JSON-LD parses and has the required properties.

---

## 3. Scope Boundaries
### In Scope (MVP)
- `/sitemap.xml` and `/robots.txt` routes in `web_ui_guest` (Topcoat `#[route(GET ...)]`, like `calendar_export.rs`).
- `SEO_INDEXABLE`-driven robots output and the `X-Robots-Tag` header (guest), plus disallow-all robots and `noindex` on `web_ui_admin`.
- A shared head-meta mechanism in the guest shell (`GuestShell` gains optional SEO fields: description, canonical path, OG image, JSON-LD payload).
- JSON-LD on listing detail, plus canonical, OG and Twitter tags on home and listing detail.
- A new `listing.updated_at TIMESTAMPTZ NOT NULL` column (backfilled from `added_at`), set on every listing write, including photo and content changes, and exposed on the public listing DTO for `lastmod` (Q2).
- A default OG image bundled as a hashed asset (Q9).
- CI changes in `.github/workflows/cicd_full_stack.yml`: `GUEST_PORTAL_URL_DEV` applies only on `refs/heads/dev`; feature-branch deploys use their own deployed guest service URL for both the guest's `GUEST_PORTAL_URL` and admin's calendar-export link (Q1b). `SEO_INDEXABLE=true` is set only on the main/prod guest deploy.
- Playwright checks, plus Rust unit tests for the sitemap, robots and JSON-LD builders.

### Out of Scope / Non-Goals
- Sitemap index files and splitting (5 listings is far below the 50k-URL limit).
- Image sitemaps, hreflang or multi-language content.
- Per-host profile pages (`/listings?host=`) in the sitemap.
- Search Console registration, sitemap ping or IndexNow submission.
- Generated OG images; the listing's primary photo is used as-is.
- Admin portal SEO beyond blocking it (Q7).
- Page-speed or Core Web Vitals work.
- `Event`, `FAQPage` or `BreadcrumbList` schema.
- `noindex`-ing filtered `/listings` URLs or policy pages (Q8).

---

## 4. Affected Systems & Stakeholders
- **User Personas**: Prospective guests arriving from search or shared links, the owners/partners (acquisition channel), and search engine crawlers.
- **System Components**:
  - `web_ui_common/src/shell_guest.rs`: head rendering, `GuestShell` SEO fields.
  - `web_ui_common/src/security_headers.rs` and `web_ui_common/tests/security_headers.rs`: confirm `ld+json` works under the existing CSP and that no CSP change is needed.
  - `web_ui_guest/src/pages/`: new `seo.rs` (sitemap and robots routes), `listing_detail.rs` (JSON-LD and meta), `home.rs` (meta).
  - `web_client`: a listing enumeration call that pages through all public listings, if the existing search isn't enough.
  - `db_core`: a new migration adding `listing.updated_at` (mirrored to `listing_history` if that table mirrors listing columns), and listing queries that set and return it.
  - `app_api/listing_api`: write paths (listing update, photo and content edits) bump `updated_at`; public listing responses include it.
  - `web_ui_admin`: robots route and `X-Robots-Tag` header.
  - `.github/workflows/cicd_full_stack.yml`: per-branch portal URL selection and `SEO_INDEXABLE`.
  - `common`: pure, unit-testable builders for the sitemap XML, robots text and JSON-LD (`serde_json`).
  - Config / `docs/ENVIRONMENT.md`: the canonical origin and an "indexable" flag per environment.
  - `playwright/`: the new spec.

---

## 5. Constraints & Non-Negotiable Invariants
- **CSP is not weakened**: no `'unsafe-inline'`, no hashes or nonces added to `script-src`. `type="application/ld+json"` is a passive data block the browser never executes, so `script-src` doesn't apply to it (confirmed, Q4). A Playwright console listener still guards against regressions.
- **JSON-LD injection safety**: the payload is built with `serde_json` (never string-concatenated), and `<`, `>`, `&` and U+2028/9 are escaped so host-edited text (name, description) can't close the `<script>` element.
- **Location privacy (#103)**: `geo` uses only the public coordinates the API already returns (approximate when `location_privacy = Approximate`). JSON-LD must never carry exact coordinates or a street address. `address` is limited to locality and country.
- **Money**: `Offer.price` comes from `Decimal` and is serialised as a string or exact decimal. No `f64` on the price path. The rating stays `f64` because it's a score, not money.
- **No leakage of unpublished data**: the sitemap includes only `is_active = true AND deleted_at IS NULL`, through the same public listing path guests use.
- **Cold start / runtime budget**: sitemap and robots are cheap. The sitemap calls `listing_api` per request, or is cached briefly in memory. Nothing is computed at startup, so the < 300 ms p50 cold start is unaffected.
- **Startup config**: the guest service reads `GUEST_PORTAL_URL` once at startup and validates it: it must be present, an absolute `http(s)` URL with no path, query or fragment, and `https` in prod. If any check fails, the service logs a `tracing::error!` naming the variable and the reason (never the raw value when it's malformed) and exits non-zero before binding the port. It never falls back to the request `Host` header, which would allow canonical and sitemap poisoning via a spoofed Host. This follows the `MapConfig::from_lookup` pattern and must be unit-tested with an injected lookup.
- **No panics**: no `.unwrap()`/`.expect()` in the handlers. If the listing API fails, the sitemap returns 503 (logged) rather than a partial or empty sitemap (error-handling standard).
- **Caching headers**: sitemap and robots are public and cacheable (`public, max-age=…`), unlike HTML's `private, no-cache`. They aren't cookie-dependent.
- **Microservice boundaries**: `web_ui_guest` reads listings only via `web_client` → `listing_api`, never directly from the DB.

---

## 6. Success Metrics & Signals
- **Leading Indicators**:
  - `/sitemap.xml` validates against the sitemaps.org XSD and lists exactly the active listings plus the static pages (Playwright).
  - `/robots.txt` on dev returns `Disallow: /`; on prod it disallows every auth-only route and names the sitemap (Playwright plus a unit test).
  - Every listing detail page has exactly one parseable JSON-LD block with `@type`, `name`, `url`, `image`, `address`, and an `offers` entry with `price` and `priceCurrency` (Playwright). Google Rich Results Test passes on one prod listing (manual).
  - No CSP violation is reported in the browser console on listing pages (Playwright console listener).
  - Home and listing pages carry canonical, `og:*` and `twitter:*` tags with absolute URLs on the canonical origin.
- **Lagging Indicators**:
  - All 5 listings are indexed in Search Console within ~4 weeks of the prod domain going live.
  - Organic search sessions and organic-attributed bookings grow (baseline: zero, since no domain is mapped yet).

---

## 7. Open Questions & Policy Concerns
- [x] **Q1 Canonical origin** (decided 2026-11-14): canonical, sitemap and OG URLs use `GUEST_PORTAL_URL` (fed from `GUEST_PORTAL_URL_*` in CI; prod is `https://ourplaces.io`). If the value is missing or invalid, the guest service refuses to start (see §5 Startup config).
- [x] **Q1a Dev origin** (decided 2026-11-14): the GitHub Actions variable `GUEST_PORTAL_URL_DEV` is set (2026-11-15) to the dev guest service's own Cloud Run URL (`https://our-places-web-ui-guest-dev-…run.app`). CI already selects `_DEV` on every branch except main and passes it to the guest service as `GUEST_PORTAL_URL`, the same way `_PROD` works. Local dev uses `http://localhost:3010` from `.env`. Dev robots.txt stays `Disallow: /` (Q3), so these canonical URLs are never indexed. No domain mapping is needed. A future `dev.ourplaces.io` only needs the variable changed.
- [x] **Q1b Feature-branch deploys** (decided): feature branches deploy their own suffixed services (`SERVICE_SUFFIX`), so they use their own deployed guest service URL. `GUEST_PORTAL_URL_DEV` applies only on `refs/heads/dev`, and `_PROD` only on main. This also restores admin's calendar-export fallback (`cicd_full_stack.yml:1610`) for feature branches.
- [x] **Q2 `lastmod` source**: option (a), a new `listing.updated_at` column maintained on every listing write, including photo and content edits.
- [x] **Q3 Indexable flag**: an explicit `SEO_INDEXABLE` flag, default `false`, set to `true` only on the prod guest deploy. Every non-indexable response also carries `X-Robots-Tag: noindex, nofollow`.
- [x] **Q4 CSP**: `ld+json` is a passive data block, so `script-src` doesn't apply. It's emitted inline with no CSP change.
- [x] **Q5 Schema type**: `VacationRental`.
- [x] **Q6 Offer price**: the base nightly rate (`price_per_night`, base currency, exact decimal). The `Offer` is omitted when there's no price.
- [x] **Q7 Admin portal**: in scope. Disallow-all robots plus `X-Robots-Tag: noindex, nofollow` in every environment.
- [x] **Q8 Extra pages**: let search engines crawl everything naturally. Policy pages stay in the sitemap and are indexable. Canonicals are the page's own path **without the query string** (revised 2026-11-16 during spec: `/listings?…` and `/listings/{slug}?check_in=…&currency=…` fold into the bare path, while crawlers can still fetch them). No `BreadcrumbList`.
- [x] **Q9 Default OG image**: the owner supplied a 1350×900 JPEG, now at `web_ui_common/assets/8ac5cca0-cf4e-11f1-9294-9dd6950248f8.jpg`. During the build, move it to `web_ui_common/src/assets/og-default.jpg` (where `asset!()` bundles from, next to `hero.jpg`) and serve it as a hashed asset. Crop and compress it to 1200×630 if the spec confirms that Google's or OG's preferred ratio matters. It's 3:2 today and 374 KB.
