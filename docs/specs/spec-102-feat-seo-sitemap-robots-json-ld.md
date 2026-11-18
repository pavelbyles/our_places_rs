# Spec 102: SEO — sitemap.xml, robots.txt and JSON-LD Structured Data

- **Issue**: #102
- **Branch**: `feat/102-seo-sitemap-xml-robots-txt-and-json-ld-structured-dat`
- **Intent**: [`docs/intent/intent-102-feat-seo-sitemap-robots-json-ld.md`](../intent/intent-102-feat-seo-sitemap-robots-json-ld.md) (Approved)
- **Date**: 2026-11-16

## Overview

Search is the main way guests find a villa, but the guest portal gives crawlers almost nothing to work with:
- There is no sitemap and no robots file.
- There is no structured data.
- The `<head>` (`web_ui_common/src/shell_guest.rs`) has only `charset`, `viewport`, a fixed `<title>` and `htmx-config`.

This feature adds:
1. `/sitemap.xml`.
2. An environment-aware `/robots.txt`.
3. A `VacationRental` JSON-LD block on each listing page.
4. Per-page title, description, canonical, OpenGraph and X card tags.
5. A block on all indexing of the admin portal.

**Personas**: prospective guests arriving from search or shared links, the owners (acquisition), and crawlers.

**Key design decisions**:

| # | Decision | Source |
|---|---|---|
| D1 | Head tags use Topcoat 0.12's built-in `topcoat::metadata` (`Metadata`, `tags()`, `BaseUrl`), not hand-written `<meta>` | Codebase (`.claude/skills/topcoat/references/metadata.md`) |
| D2 | Canonical = `Url::Current` (the request path **without** its query string) on every page | Grill 2026-11-16 (revises intent Q8) |
| D3 | The public origin comes from `GUEST_PORTAL_URL`. The guest service refuses to start if it's missing or invalid | Intent Q1 |
| D4 | Indexing is opt-in via `SEO_INDEXABLE=true`, set only on the main/prod deploy | Intent Q3 |
| D5 | JSON-LD is an inline `<script type="application/ld+json">`. No CSP change, because data blocks aren't subject to `script-src` | Intent Q4 |
| D6 | `@type: VacationRental`. `latitude`/`longitude` use the **public** coordinates (approximate when the host chose Approximate) | Intent Q5, grill 2026-11-16 |
| D7 | The price is the listing's base nightly rate, sent as `makesOffer` (see §1.4 for why not `offers`) | Intent Q6 |
| D8 | `lastmod` = `last_modified_at` = `GREATEST(listing.updated_at, latest processed original photo change)`. `listing.updated_at` is a new column maintained by a row trigger on `listing`. Photo changes are folded in **at read time**, with no triggers on `listing_image` (review panel: they deadlock with photo delete and the image worker) | Intent Q2, eng review 2026-11-16 |
| D10 | Listing canonical, `og:url` and JSON-LD `url` are built from `listing.slug`, never from the request path, because listings also render at `/listings/<uuid>` | Eng review 2026-11-16 |
| D9 | Feature-branch deploys use their own Cloud Run URL. `GUEST_PORTAL_URL_DEV` applies only on `dev` | Intent Q1b |

---

## 1. Requirements

### 1.1. Public origin config (`GUEST_PORTAL_URL`)
1. A new `web_ui_common::site_config::SiteConfig` is read once in `web_ui_guest/src/main.rs`, before the router is built. It follows the `MapConfig::from_lookup` pattern (injectable lookup, `thiserror` error enum, `static …_ENV: &str` constants per RS-W1015).
2. Fields:
   - `base_url: topcoat::context::BaseUrl`, from `GUEST_PORTAL_URL`.
   - `indexable: bool`, from `SEO_INDEXABLE`. Only `"true"` and `"1"` turn it on; anything else, including unset, means `false`.
   - `twitter_site: Option<String>`, from `SEO_TWITTER_SITE`.
3. `GUEST_PORTAL_URL` validation. Startup fails if any check fails:
   - (a) present and non-empty;
   - (b) accepted by `BaseUrl::new`: absolute `http(s)`, no query or fragment;
   - (c) **no path prefix** (`base.prefix()` is empty, after Topcoat strips a trailing `/`);
   - (d) scheme is `https` when `indexable` is true.
4. On failure:
   - `main` logs `tracing::error!(variable = "GUEST_PORTAL_URL", reason = %e, "invalid site configuration; refusing to start")` and returns the error, so the process exits non-zero before `TcpListener::bind`.
   - A malformed value is not logged verbatim. The `reason` names the failing rule (`missing`, `not an absolute http(s) URL`, `has a path`, `must be https when SEO_INDEXABLE=true`).
5. Never fall back to the request's `Host` or `X-Forwarded-Host` (host-header poisoning of canonical and sitemap URLs).
6. `SEO_TWITTER_SITE` is optional and cosmetic:
   - valid = `@` followed by 1–15 `[A-Za-z0-9_]`;
   - invalid → one `tracing::warn!` and omitted;
   - unset → omitted.
7. The router gets `.with(site.base_url.clone())`, `.with(site.clone())`, and `.with(Metadata::new()…)` holding the site-wide defaults (§1.5).

### 1.2. `/sitemap.xml` (guest)
1. `#[route(GET "/sitemap.xml")]` in a new `web_ui_guest/src/pages/seo.rs`, returning Topcoat's **`topcoat::router::content::sitemap::Sitemap`** (`sitemap` feature on `web_ui_guest`'s `topcoat` dependency; decided 2026-11-17 at the owner's request). Topcoat writes the XML, escapes text, resolves root-relative paths against the router's `BaseUrl`, and formats `<lastmod>` as RFC 3339; no hand-written XML.
2. Body: sitemaps.org 0.9 `<urlset>` with the XML declaration, `content-type: application/xml` (Topcoat's `Sitemap` sets it).
3. Static URLs, with no `<lastmod>` (we have no truthful date for them): `/`, `/listings`, `/about`, `/terms`, `/cancellation-policy`, `/privacy-policy`, `/house-rules` (the last four from `POLICY_LINKS`).
4. Listing URLs: one `<url>` per listing returned by the public search (`get_api_client(cx).search_listings(…)`):
   - Paged at `per_page = 100` until a short page. There is a hard cap of 50 pages; hitting it is treated as a failure (`error!` + 503, item 7), because serving what was collected would be a partial sitemap. 5000 listings is far beyond the MVP, and the fix at that scale is a sitemap index (out of scope).
   - No dates, guests or currency in the request.
   - `include_inactive` stays `false`, so the DB filters `deleted_at IS NULL AND is_active AND owner is_active` (`db_core/src/listing/queries.rs:131-137`).
   - `<loc>` = `base_url.join(&format!("/listings/{slug}"))`.
   - `<lastmod>` = `last_modified_at` in W3C datetime (`%Y-%m-%dT%H:%M:%SZ`), omitted if `None`.
   - The request is built from `ListingSearchParams::default()` with only `page`/`per_page` set. A unit test asserts `include_inactive == false` and that `owner`/`host`/`user_id` are unset, so the privacy filter can't be loosened by accident.
5. All text is XML-escaped by Topcoat's `Sitemap` (text-node escaping of `&`, `<`, `>`), even though slugs are `[a-z0-9-]`. A test feeds a hostile slug through the real route.
6. Listings are ordered by slug and deduplicated by slug, so output is deterministic.
7. If any page of the listing search fails:
   - log `tracing::error!` (with page number and error);
   - respond `503`, `retry-after: 300`, `cache-control: no-store`, plain-text body;
   - **never** a partial or empty sitemap (error-handling standard).
8. Success headers: `cache-control: public, max-age=3600`. `SecurityHeaders` adds `private, no-cache` and `Vary: Cookie` only to `text/html`, so this survives (`security_headers.rs:185-194`).
9. The sitemap is served the same way in every environment. Robots stops non-prod crawling, and the canonical origin is correct per environment.

### 1.3. `/robots.txt` (guest and admin)
1. Guest: `#[route(GET "/robots.txt")]` in `seo.rs`. `content-type: text/plain; charset=utf-8`, `cache-control: public, max-age=3600`.
2. Guest, `indexable = true`:
   ```
   User-agent: *
   Disallow: /checkout/
   Disallow: /bookings
   Disallow: /profile
   Disallow: /requests
   Disallow: /reviews/
   Disallow: /login
   Disallow: /logout
   Disallow: /register
   Disallow: /verify
   Disallow: /currency/
   Disallow: /listings/*/quote
   Disallow: /listings/*/enquire
   Disallow: /listings/*/calendar
   Allow: /

   Sitemap: <base_url>/sitemap.xml
   ```
   - `/listings/*/calendar` also covers `calendar.ics` (prefix match).
   - Assets (CSS, JS, fonts, images) stay crawlable so pages render for Googlebot.
3. Guest, `indexable = false`: `User-agent: *` / `Disallow: /`, with no `Sitemap:` line.
4. Admin: `#[route(GET "/robots.txt")]` in `web_ui_admin` always returns `User-agent: *` / `Disallow: /`. The route needs no auth guard.
5. The route list is a `const` slice (`PRIVATE_PREFIXES`) beside the guest route definitions. Test U5 derives the set of guarded routes by scanning `include_str!` of each `web_ui_guest/src/pages/*.rs` for `#[page("…")]` paths in files whose handler calls `require_guest`. It then asserts that each path is matched by a `Disallow` rule, so a new private page fails the test until it's listed. A hand-maintained expected list isn't allowed.

### 1.4. Listing JSON-LD (`/listings/{slug}`)
1. Emitted only on a successful 200 render of an active listing. `load()` already 404s inactive and missing listings (`listing_detail.rs:232-235`).
2. Placed once in the page body, at the end of `<main>`'s content. Google reads JSON-LD anywhere in the document, so the shell doesn't need to change for it.
3. Shape. Fields with no value are omitted, never `null`:

| Property | Source | Notes |
|---|---|---|
| `@context` | `"https://schema.org"` | |
| `@type` | `"VacationRental"` | D6 |
| `@id` | `<canonical>#rental` | |
| `identifier` | `listing.id` (UUID string) | Google VR required |
| `name` | `listing.name` | |
| `description` | `listing.description`, trimmed, max 5000 chars | omitted if empty |
| `url` | canonical = `base_url.join(&format!("/listings/{}", details.listing.slug))` (D10) | never the request path |
| `image` | all `details.images[].url`, in display order | Google VR wants ≥ 8; we emit what exists |
| `address` | `{ "@type": "PostalAddress", "addressLocality": city, "addressCountry": country }` | never a street address |
| `latitude`, `longitude` | public coords from `ListingResponse` | omitted when `None`; approximate when privacy = Approximate (D6) |
| `containsPlace` | `{ "@type": "Accommodation", "additionalType": "EntirePlace", "occupancy": { "@type": "QuantitativeValue", "value": max_guests.max(1) }, "numberOfBedrooms", "numberOfBeds": beds (omitted when 0, which means "not entered"), "numberOfFullBathrooms", "numberOfPartialBathrooms", "numberOfBathroomsTotal": full + half, "floorSize": { "@type": "QuantitativeValue", "value": square_meters, "unitCode": "MTK" }, "amenityFeature": [...] }` | `floorSize` only when `square_meters` is set |
| `amenityFeature` items | `{ "@type": "LocationFeatureSpecification", "name": item, "value": true }` for each item of `amenity_groups_for(...)`, deduplicated, max 50 | free text; mapping to Google's amenity vocabulary is out of scope |
| `aggregateRating` | `{ "@type": "AggregateRating", "ratingValue": overall (2 dp), "reviewCount": n, "bestRating": 5, "worstRating": 1 }` | only when `review_count > 0` **and** `overall_rating` is `Some` and finite |
| `makesOffer` | `{ "@type": "Offer", "url": canonical, "priceSpecification": { "@type": "UnitPriceSpecification", "price": "<Decimal>", "priceCurrency": "<ISO 4217>", "unitCode": "DAY", "unitText": "night" } }` | only when `price_per_night` is `Some` (D7) |

4. Why `makesOffer` and not `offers`: schema.org doesn't allow `offers` on `LodgingBusiness` or `VacationRental`, while `makesOffer` (from `Organization`) is valid. The Schema.org validator must report 0 errors.
5. **Price currency**: the `Offer` shows the price the page displays.
   - `listing_api` replaces `price_per_night` and `base_currency` when a display currency is requested (`apis.rs:473-481`).
   - Crawlers carry no currency cookie, and the canonical URL has no `?currency=`, so search engines always see the base rate in the base currency.
   - A human with a currency cookie sees an internally consistent converted pair, which matches the visible price as Google's guidelines require.
   - When `no_rate_for` is set, the page shows the base currency, and so does the JSON-LD.
6. **Money precision**: `price` is the `Decimal` rendered with `to_string()`, as a JSON string. It never passes through `f64`. `rust_decimal`'s `serde-with-str` or a manual `Value::String` keeps this explicit.
7. **Serialisation and injection safety**:
   - Build with typed `serde::Serialize` structs, using `#[serde(skip_serializing_if = "Option::is_none")]`.
   - Serialise with `serde_json::to_string`, then replace each of these five characters with its six-character JSON escape (a backslash, `u`, then four lowercase hex digits): `<` → hex `003c`, `>` → hex `003e`, `&` → hex `0026`, U+2028 → hex `2028`, U+2029 → hex `2029`.
     - After this step the output contains **no raw `<`**, so `</script`, `<!--` and `<script` (the script-data escaped and double-escaped states) are all impossible.
     - Replacing after serialisation can't corrupt the JSON. Serde has already written every literal backslash as two backslashes, so an escaped backslash followed by an inserted escape still decodes to the original text.
     - The result is still valid JSON that parses to the same value.
     - (In this document the escapes are spelled out as hex because the literal backslash sequences were lost in an earlier edit. Unit test U8 is the source of truth.)
   - Emit it through `topcoat::view::Unescaped::new_unchecked(...)`. Topcoat's normal `Text` escaping would turn `&` into `&amp;` inside the raw-text `<script>`, and browsers wouldn't decode it, which would corrupt names.
   - The only `Unescaped` call site is a single `web_ui_common::components::json_ld::json_ld(value: serde_json::Value)` component that does the escaping itself. No other code may pass strings to `Unescaped`. Callers convert with `serde_json::to_value(...)`.

### 1.5. Head metadata (title, description, canonical, OpenGraph, X)
1. `guest_document` (`shell_guest.rs`):
   - replace the hand-written `<meta charset>`, `<meta name="viewport">` and `<title>` with `topcoat::metadata::tags()`, as the **first** child of `<head>` so charset stays in the first 1024 bytes;
   - keep `htmx-config`, fonts, stylesheet and scripts after it.
2. Site-wide defaults on the router:
   ```rust
   Metadata::new()
       .title("Luxury Jamaica Stays · Our Places")
       .description(SITE_DESCRIPTION)                  // §1.5.6
       // Not Url::Current: it skips the base URL for paths starting with
       // `//` (topcoat-metadata field/url.rs `needs_base`), so a request for
       // `//evil.example/x` would get a cross-origin canonical.
       .canonical_from(|cx| base_url(cx).join(uri(cx).path()))   // via Metadata::from_fn

       .open_graph(OpenGraph::new().site_name("Our Places")
           .image(Image::new(OG_DEFAULT_IMAGE).size(1200, 630).alt(OG_DEFAULT_ALT)))
       .twitter(Twitter::new().card(TwitterCard::SummaryLargeImage) /* .site(h) if set */)
   ```
   - `GuestShell.title` and the `title` argument of `guest_shell` are removed. The layout no longer sets a title.
   - Pages may override any field.
   - `canonical_from` above is shorthand. In code, the router-level default is a `Metadata::from_fn(site_defaults)` that returns the static fields plus `.canonical(base.join(path))`. `BaseUrl::join` always resolves under the configured origin, so the canonical is always same-origin and query-free. Test U10 includes a `//evil.example/x` request.
3. Per-page titles: a helper `page_title(&str) -> String` appends ` · Our Places`. Pages that set a title:
   - `/` keeps the default;
   - `/listings` uses `"Villas in Jamaica"`;
   - `/about`, the four policy pages, `/login` and `/register` use static titles via `#[page(..., with(Metadata::new().title(...)))]`.
4. `/listings/{slug}`:
   - `#[page("/listings/{slug}", with(Metadata::from_fn(listing_metadata)))]`.
   - `listing_metadata` calls the existing loader, made `#[memoize(as_ref)] async fn load(cx: &Cx, slug: &str) -> Result<Loaded>`, so the page and the metadata share **one** API call per request.
   - Canonical = `base.join("/listings/{listing.slug}")` (D10). A visit to `/listings/<uuid>` declares the slug URL canonical.
   - **Any** `load` error (404, 5xx, timeout) makes `listing_metadata` return `Ok(Metadata::new())` with a `debug!`. The page body raises the real error once, so the error page renders with its correct status and the site-default head, and the loader error is never propagated twice through `tags()`. T1 confirms this, for both a 404 and a 5xx.
   - Title = `page_title(&listing.name)`.
   - Description = the first 160 **characters** (`chars()`, never byte slicing, which panics on multi-byte text) of `listing.description`, collapsed whitespace, cut at a word boundary with `…`.
     - Fallback: `"{n}-bedroom {structure} in {place}"`, where `place` = `"{city}, {country}"`, or `country` when `city` is `None`.
     - When `bedrooms == 0`, the fallback becomes `"{structure} in {place}"`.
     - The JSON-LD `description` cap (5000) is also character-based.
   - `og:type` stays `website`. Schema.org has no OG "rental" type.
   - `og:image` = the first image in display order (`details.images[0]`; the DTO has no `is_primary`), using `variant_url(url, "desktop")` (1920w) with a fallback to `url`; it falls back to the site default image.
   - `og:image:alt` = `photo_alt(...)` of that image.
5. Canonical, `og:url` and image URLs resolve against `BaseUrl` (D1, D2). Topcoat **panics** when it resolves a site-relative URL without `BaseUrl` in context, so:
   - `BaseUrl` is mandatory at startup (§1.1);
   - every render test of `guest_document` must add `BaseUrl` to its test `Cx`.
6. Default copy (owner-editable constants in `web_ui_common::site_config`):
   - `SITE_DESCRIPTION` = "Private luxury villas and apartments in Jamaica, hosted by their owners. Book direct with Our Places.";
   - `OG_DEFAULT_ALT` = "A villa on the Jamaican coast".
7. HTMX partial requests (`hx_request && !hx_boosted`) return only the slot, as now. No head tags are sent, and the metadata loader isn't awaited.
8. No `noindex` meta tag is used anywhere on the guest portal. Non-indexable environments use the `X-Robots-Tag` header (§1.6).

### 1.6. `X-Robots-Tag` header
1. `SecurityHeaders` gains a `noindex` flag that **defaults to `true`** (fail closed), for both `new()` and `admin()`.
   - When it's set, `x-robots-tag: noindex, nofollow` is added to the **queued** headers through `response_headers(cx)`, **before** `next.run(...)`, like the CSP (`security_headers.rs:173-183`). That way error responses (404/500) and assets carry it too.
   - A header inserted after the response wouldn't work: `next.run(...)?` returns early on `Err`.
2. Guest: `.indexable(site.indexable)` opts out of `noindex` only when indexing is on.
   - Even then, `x-robots-tag: noindex` is still sent on every request whose path matches `PRIVATE_PREFIXES` (§1.3), via `.noindex_prefixes(PRIVATE_PREFIXES)`.
   - `Disallow` alone doesn't stop URL-only indexing of externally linked private URLs, such as `/verify?token=…` from emails.
3. Admin: `SecurityHeaders::admin()` keeps `noindex = true` and has no way to turn it off.

### 1.7. `listing.updated_at`
1. New migration `sqlx migrate add listing_updated_at`. The last migration (`20261114053147_map_view_location_privacy.sql`) has already shipped to `dev`, so a new file is required:
   ```sql
   ALTER TABLE listing ADD COLUMN updated_at TIMESTAMPTZ;
   UPDATE listing SET updated_at = added_at;
   ALTER TABLE listing ALTER COLUMN updated_at SET NOT NULL,
                       ALTER COLUMN updated_at SET DEFAULT NOW();

   CREATE TRIGGER update_listing_updated_at
   BEFORE UPDATE ON listing
   FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
   ```
   - It reuses the existing `update_updated_at_column()` (`20260203021617_refactor_user_profiles.sql`).
   - The trigger is created after the backfill, so the backfill keeps `added_at`.
   - **No triggers on `listing_image`.** The review panel found they deadlock (40P01):
     - `delete_listing_image` (`images.rs:173-199`) deletes photo A, then updates photo B in the same transaction.
     - Meanwhile, auto-commit writers on B (rename `images.rs:208`; the image worker's `mark_listing_image_processed`, `image_worker/src/apis.rs:228-233`) would lock B and then wait in the trigger for `listing`.
     - The worker swallows that error, which would leave B stuck in `Processing`.
2. **Photo changes are folded in at read time.** The migration also creates one SQL function, so the expression is defined in one place:
   ```sql
   CREATE FUNCTION listing_last_modified(p_listing_id UUID, p_updated_at TIMESTAMPTZ)
   RETURNS TIMESTAMPTZ LANGUAGE sql STABLE AS $$
     SELECT GREATEST(p_updated_at,
            (SELECT MAX(li.updated_at) FROM listing_image li
              WHERE li.listing_id = p_listing_id AND li.parent_id IS NULL
                AND li.status = 'Processed'))
   $$;
   ```
   - **Every** select that fills `db_core::models::Listing` or `ListingWithOwner` adds `listing_last_modified(listing.id, listing.updated_at) AS "last_modified_at!"`. That covers `queries.rs` (`get_listings`, `get_listings_by_user_id` ~235, `get_listing_by_id` ~271, `get_listing_by_id_or_slug` ~334/356) and the `RETURNING` lists in `write.rs:25,121,174` (scalar function calls are valid in `RETURNING`).
   - `GREATEST` ignores `NULL`, so the result is never `NULL`. The row structs therefore get a non-optional `last_modified_at: DateTime<Utc>`, with no second struct and no `NULL` placeholders.
   - Only processed originals count, so presign (`PendingUpload`) and `Processing`/`Failed` transitions don't move `lastmod`.
   - Rename, the processed transition and the primary change all already set `listing_image.updated_at` (`images.rs:108,188,220`).
   - Photo **deletes** leave no row behind. `delete_listing_image` therefore runs `UPDATE listing SET updated_at = NOW() WHERE id = $listing_id` as its **first** statement, before the `DELETE`.
   - Lock order in that transaction is then `listing` → photo rows. No photo writer ever locks `listing`, so no cycle is possible.
   - Booking transactions lock `listing` but never `listing_image`, so a delete simply waits behind a short booking hold.
3. What moves `last_modified_at`:
   - every `UPDATE listing` (host edits, activation, location, the review-aggregate refresh in `db_core/src/review.rs:305`);
   - a processed original photo being renamed, finishing processing or becoming primary;
   - a photo delete.
   - Price overrides (`listing_price_override`) **don't** move it (rates, not content; intent Q2).
4. `listing_history` doesn't mirror `updated_at`; history rows have their own timestamps.
5. Plumbing. The compiler enforces all of these, because `ListingResponse` has no `Default`:
   - `db_core` `Listing` (`models.rs:727`) and `ListingWithOwner` (`:815`) gain `updated_at: DateTime<Utc>` and `last_modified_at: DateTime<Utc>`.
   - The `get_listings` `QueryBuilder` and the `query_as!` selects in `queries.rs` add both columns.
   - The three `query_as!(Listing, …)` calls in `write.rs:25,121,174` add both columns to `RETURNING`.
   - Row → response mapping lives in `app_api/api_core/src/models.rs:9,109`, plus `listing_api` `manager_listing_response` (`apis.rs:412`).
   - `ListingResponse` struct literals need the new field: `api_core/src/models.rs:376`, `listing_api/src/apis_test.rs:468`, `web_ui_guest/src/pages/listings.rs:496`, `web_ui_guest/src/pages/listing_detail.rs:1551`, `web_ui_admin/tests/listing_location.rs:37`, `protoproj/src/lib.rs:18`.
   - `cargo sqlx prepare --workspace -- --all-targets` regenerates all three metadata directories: `.sqlx/`, `db_core/.sqlx/` and `app_api/listing_api/.sqlx/`.
6. DTO: `common::models::ListingResponse` gains `#[serde(default)] pub last_modified_at: Option<DateTime<Utc>>`. It's `Option`, so a guest portal deployed ahead of `listing_api` (rollout skew) still deserialises; the sitemap omits `lastmod` while the field is absent.

### 1.8. Default OG image
1. Move the owner-supplied image `web_ui_common/assets/8ac5cca0-cf4e-11f1-9294-9dd6950248f8.jpg` (1350×900, 374 KB) to `web_ui_common/src/assets/og-default.jpg`.
2. Centre-crop and resize it once to **1200×630**, JPEG quality ~82, target ≤ 250 KB, and commit the result. Remove the original and the empty `web_ui_common/assets/` directory.
3. Register `pub const OG_DEFAULT_IMAGE: Asset = asset!("src/assets/og-default.jpg");` in `web_ui_common/src/assets.rs`, next to `HERO_IMAGE`. The hashed URL resolves to an absolute URL through `BaseUrl`.

### 1.9. CI (`.github/workflows/cicd_full_stack.yml`)
1. **Resolve the guest public URL before deploy** (new step in `deploy-web-ui-guest`, after `branch_name`):
   - `main` → `vars.GUEST_PORTAL_URL_PROD`;
   - `dev` → `vars.GUEST_PORTAL_URL_DEV`;
   - any other branch → `https://${SERVICE_NAME}-${PROJECT_NUMBER}.${REGION}.run.app`, Cloud Run's deterministic URL. `PROJECT_NUMBER` comes from `gcloud projects describe` (pattern already used at lines 370, 520).
   - If `main` or `dev` resolves to empty, the step fails with an explicit `::error::` (no silent fallback).
2. Deploy `env_vars` add `GUEST_PORTAL_URL=<resolved>` and `SEO_INDEXABLE=${{ github.ref == 'refs/heads/main' && 'true' || 'false' }}`.
3. **Reconcile** (feature branches only, after deploy). This is the **expected** path, not a rare one: this branch's service name, `our-places-web-ui-guest-feat-102-seo-sitemap-xml-robots-txt`, is 59 chars, so `-<project number>` exceeds the 63-char DNS label limit and Cloud Run won't issue the deterministic URL.
   - Read the issued URLs with `gcloud run services describe "$SERVICE" --region "$REGION" --format=json | jq -r '.metadata.annotations["run.googleapis.com/urls"] // "[]" | fromjson[]'`, plus `.status.url`.
   - If the resolved URL isn't among them: `::warning::`, then `gcloud run services update … --update-env-vars "GUEST_PORTAL_URL=$STATUS_URL"`.
   - The first revision still starts, because the computed value is well-formed. Only its canonicals are briefly wrong, on a non-indexed environment.
   - Verify the annotation name against one real `describe` during plan task T2. If it's absent, compare against `.status.url` only.
4. The job output `public_url` is the value **after** reconcile. The admin job outputs an equivalent value.
5. `configure-email-links` uses the same rule (D9):
   - `GUEST_URL` / `ADMIN_URL` = `_PROD` on main, `_DEV` on dev, otherwise the deploy job's `public_url`.
   - When the **guest** portal was skipped in the run, `GUEST_URL`'s `lookup` fallback reads the guest service's **`GUEST_PORTAL_URL` env value** (`gcloud run services describe … --format=json | jq '.spec.template.spec.containers[0].env[] | select(.name=="GUEST_PORTAL_URL") | .value'`), not `status.url`. That keeps email `BASE_URL` identical to the guest's own canonical origin. When the **admin** portal was skipped, `ADMIN_URL` keeps today's fallback (the admin service's `status.url`), because the admin has no canonical-origin variable.
   - This fixes feature-branch email links and the admin calendar-export link, which since 2026-11-15 point at the dev guest.
6. The admin deploy is unchanged apart from (5). Admin's `noindex` is hard-wired, with no env var.
7. **Workflow hygiene**:
   - The new steps read `vars.*`, `steps.*.outputs` and gcloud results only through `env:`, never through inline `${{ }}` inside `run:` (expression injection). This follows the existing `configure-email-links` pattern (`cicd_full_stack.yml:1609-1611`).
   - The resolve step rejects any value containing `,`, whitespace or a newline (`::error::`) before it reaches `env_vars:`, which the deploy action splits on newlines and commas.
8. **Ordering**: these CI changes must reach `origin` in the **same push** as, or before, the fail-fast startup (§1.1). Otherwise the next feature-branch deploy starts a guest without `GUEST_PORTAL_URL`, the revision fails, and the deploy breaks.

### 1.10. Local and test environments
1. `devenv.nix`: `env.GUEST_PORTAL_URL = "http://localhost:3010";` (the repo has no `.env.example`; `.env` is untracked and may override it). `SEO_INDEXABLE` stays unset.
   - `docker-compose.yml` (guest service, ~lines 135-150) gets the same value, so `docker compose up` doesn't crash-loop.
   - Portals already started with `devenv up -d` keep the daemon's old env, so restart the guest by hand after pulling this change.
2. `playwright/playwright.config.ts` guest `webServer.env`: `GUEST_PORTAL_URL: process.env.GUEST_PORTAL_URL || 'http://localhost:3010'`.
3. `docs/ENVIRONMENT.md`:
   - `GUEST_PORTAL_URL` becomes **required** for the guest service;
   - add `SEO_INDEXABLE` and `SEO_TWITTER_SITE`;
   - document the CI resolution rule (§1.9).

---

## 2. Technical Implementation

### 2.1. Data model
- Migration `db_core/migrations/<ts>_listing_updated_at.sql` (§1.7).
- `db_core/src/models.rs`: `updated_at` on the listing row structs. `db_core/src/listing/queries.rs`: add the column to the four listing selects. `.sqlx/` regenerated.
- `common/src/models.rs`: `ListingResponse.updated_at: Option<DateTime<Utc>>` (documented field, `#[serde(default)]`).
- `app_api/listing_api`: map `updated_at` in the row → response conversion. There are no new endpoints and no handler logic changes; the triggers do the work.

### 2.2. New and changed modules

| File | Change |
|---|---|
| `web_ui_common/src/site_config.rs` (+ `site_config_tests.rs`) | `SiteConfig`, `SiteConfigError` (`Missing`, `NotAbsolute`, `HasPath`, `InsecureWhenIndexable`), `from_lookup`, `from_env`, `SITE_DESCRIPTION`, `OG_DEFAULT_ALT`, `page_title` |
| `web_ui_common/src/components/json_ld.rs` | `json_ld(value)` component and `escape_json_for_script(&str) -> String`, the only `Unescaped` use |
| `web_ui_common/src/security_headers.rs` | `noindex` default on, queued before `next.run`; `.indexable(bool)`; `.noindex_prefixes(&[..])`; tests in `web_ui_common/tests/security_headers.rs` |
| `web_ui_common/src/shell_guest.rs` | `tags()` in `<head>`; remove `title` from `GuestShell`/`guest_shell` |
| `web_ui_common/src/assets.rs` | `OG_DEFAULT_IMAGE` |
| `web_ui_guest/src/seo/mod.rs` (+ `seo_tests.rs`, `sitemap_tests.rs`) | `PRIVATE_PREFIXES`, `robots_txt(base, indexable)`, `sitemap_search(page)`, `collect_listings(&client)` (paging, cap), `sitemap_for(listings) -> Sitemap` (Topcoat type), `vacation_rental(...)` (serde structs) |
| `web_ui_guest/src/pages/seo.rs` | `/sitemap.xml` and `/robots.txt` routes (I/O only; logic in `seo/`) |
| `web_ui_guest/src/pages/listing_detail.rs` | `#[memoize(as_ref)] load`; `listing_metadata` (slug canonical, swallows load errors); `json_ld(...)` in the view |
| `web_client/src/client.rs`, `common/src/app_client/listings.rs` | `get_listing_by_id_at(client, base, …)`, so the listing client honours `listing_api_url` (testable with wiremock) |
| `db_core/src/listing/images.rs` | `delete_listing_image` bumps `listing.updated_at` first |
| `web_ui_guest/src/pages/{home,listings,about,policies,auth,register}.rs` | static `Metadata` titles |
| `web_ui_guest/src/layout.rs` | drop the title argument |
| `web_ui_guest/src/main.rs` | `SiteConfig::from_env()?` (fail fast), `.with(BaseUrl)`, `.with(SiteConfig)`, `.with(Metadata defaults)`, `.with_noindex(...)` |
| `web_ui_admin/src/pages/robots.rs`, `main.rs` | admin robots route |

The intent placed the pure builders in `common`. The spec moves them to `web_ui_guest/src/seo/`, which is their only consumer, so `common` (the isomorphic domain) gains no XML or robots formatting. They stay pure and unit-tested.

### 2.3. API endpoints
- No new backend endpoints.
- `GET /api/v1/listings` and `GET /api/v1/listings/{id}` gain a field, `updated_at`. This is additive and backwards compatible.

### 2.4. Frontend views
- No visible UI change apart from the browser tab titles, which become per-page.
- JSON-LD and meta tags are not rendered visibly.

### 2.5. Background workers
- None. `image_worker` writes listing images through `db_core`, so the triggers cover it with no worker change.

---

## 3. Edge Cases & Failure States

| Case | Behaviour |
|---|---|
| `GUEST_PORTAL_URL` unset, empty, relative, has a path, has a query, or is `http` while indexable | Startup error log naming the variable and the reason; exit non-zero before binding; Cloud Run revision fails its health check, so the previous revision keeps serving |
| `SEO_INDEXABLE` unset or misspelled (`TRUE `, `yes`) | Treated as `false` (safe default: not indexable) |
| `SEO_TWITTER_SITE` invalid | One `warn!`; tag omitted; startup continues |
| `listing_api` down or erroring during sitemap paging | `503` + `retry-after: 300` + `no-store`; `error!` logged with page number; never a partial sitemap |
| 0 active listings | Valid sitemap with the 7 static URLs only |
| Listing search pagination reaches 50 pages | `error!` + 503, never a partial sitemap. At 100 per page that's 5000 listings, far beyond the MVP |
| Listing requested at `/listings/<uuid>` | Renders as before. Canonical, `og:url` and JSON-LD `url` use the slug URL (D10) |
| `listing_api` 5xx or timeout on a listing page | Error page with its 5xx status and the default head. `listing_metadata` swallows the error (logged once by the page) |
| Request path starts with `//` (`//evil.example/x`) | Canonical is `base.join(path)`, so always same-origin |
| Description with multi-byte or emoji text | Truncated by `chars()`, never by byte index |
| `bedrooms == 0`, `beds == 0`, `max_guests == 0`, `city = None` | Fallback description adapts; `numberOfBeds` omitted; occupancy `max(1)` |
| Photo deleted while the image worker processes another photo of the same listing | No deadlock: no `listing_image` triggers; the delete locks `listing` first, and photo writers never lock `listing` (I8) |
| The same slug appears on two pages (data shifting between page requests) | Deduplicated by slug |
| Listing has `last_modified_at = None` (old `listing_api` during rollout) | `<lastmod>` omitted for that URL |
| Listing becomes inactive or deleted | Gone from the next sitemap; its page already 404s |
| Listing `price_per_night` `None` | `makesOffer` omitted |
| Display currency lacks an FX rate (`no_rate_for`) | Page and JSON-LD both use the base currency |
| `review_count = 0`, or the rating is `None`/NaN | `aggregateRating` omitted |
| Coordinates `None` | `latitude`/`longitude` omitted (not eligible for Google VR, still valid schema.org) |
| Location privacy Approximate | Approximate public coordinates, identical to the map's; exact coordinates never reach the guest service on public paths (spec 103 §1.2) |
| Name or description contains `</script>`, `<!--`, `&`, U+2028 | Each of `<` `>` `&` U+2028 U+2029 becomes its six-character JSON escape (§1.4.7); no raw `<` remains, JSON parses to the original text, and nothing breaks out of the element |
| No listing images | `image` omitted from JSON-LD; `og:image` falls back to the default image |
| `/listings/{slug}` for an unknown or inactive slug | Normal 404 page. `Metadata::from_fn` sees the same memoized `Err(not_found)`. **Must be tested**: the layout's `tags()` must not turn the 404 into a 500 or a blank page. If Topcoat propagates the loader error through the error page's head, `listing_metadata` maps load errors to the site defaults (`Ok(Metadata::new())`), and the page body still raises the 404 |
| HTMX partial request to a listing page | No head, no `tags()`; the memoized `load` is still called once by the page |
| Query strings (`?check_in`, `?currency`, `?q`) | Canonical = bare path (D2) |
| Request arrives on the `run.app` host in prod (domain mapped) | Canonical and sitemap still use `https://ourplaces.io`, consolidating ranking on the custom domain |
| Prod `ourplaces.io` domain mapping not yet live when main deploys | Canonicals point at a host that doesn't resolve. **Pre-merge gate**: confirm the mapping before merging to main (memory: domain mappings unverified) |
| Feature-branch service name too long for the deterministic URL (the normal case for long branch names) | Reconcile step (§1.9.3) rewrites `GUEST_PORTAL_URL` to `status.url` with a CI warning; `public_url` and email links use the reconciled value |

---

## 4. Performance Considerations
- **Cold start**: `SiteConfig::from_env` is a few string parses, with no I/O. Nothing is prefetched at startup. The < 300 ms p50 budget is unaffected.
- **Listing page**: no extra API calls. `#[memoize]` shares `load` between the metadata loader and the page. JSON-LD serialisation is O(images + amenities), at most ~60 small items, and takes microseconds.
- **Sitemap**: `ceil(N/100)` API calls per request. With N = 5 that's 1 call (~50–150 ms warm). `cache-control: public, max-age=3600` lets crawlers and any CDN cache it. There's no in-process cache: it would be lost on scale-to-zero, and at this volume it isn't needed.
- **robots.txt**: a constant string, plus `base_url` formatting.
- **DB**: the `listing.updated_at` column needs no index (it's only selected, never filtered). `last_modified_at` adds one correlated `MAX` subquery per listing row, served by the existing `listing_image` FK/listing_id index (verify with `EXPLAIN` in T7). At ≤ 100 rows per page this costs microseconds. There are no triggers on `listing_image`, so the image pipeline gains no extra writes or lock waits.
- **Memory**: the sitemap string for 5000 listings is ~600 KB in the worst case. That's acceptable on a 512 Mi instance, and realistically under 5 KB.
- **No blocking I/O on async threads**. All work is async HTTP or in-memory formatting.

## 5. Security Mitigations
- **Host-header poisoning**: absolute URLs come only from validated `GUEST_PORTAL_URL`, never from request headers (§1.1.5).
- **XSS via JSON-LD**: typed serde structs, plus a script-safe escape of `<`, `>`, `&`, U+2028/9 before a single audited `Unescaped` site. Unit tests use hostile names and descriptions (§6, U8).
- **CSP unchanged**: `script-src 'self'` with no `unsafe-inline`, hashes or nonces. An `ld+json` data block isn't executed, so CSP `script-src` doesn't govern it. Playwright asserts there are zero `securitypolicyviolation` events.
- **Location privacy**: JSON-LD uses only what `listing_api` already returns on public paths. That's approximate when privacy = Approximate, and there's never a street address.
- **Information disclosure through the sitemap**: it uses only the public search with `include_inactive = false`, so inactive, deleted and inactive-owner listings are excluded by the same SQL that guards `/listings`.
- **Private routes**: robots `Disallow` is advisory and not a security control. Those pages stay protected by their existing server-side guards; robots only keeps them out of crawl budgets.
- **Non-prod leakage**: `Disallow: /` plus `X-Robots-Tag: noindex, nofollow` on every response. Indexing needs an explicit opt-in flag that only the main deploy sets.
- **Admin portal**: always `Disallow: /` and `noindex, nofollow`, regardless of configuration.
- **XML injection in the sitemap**: every interpolated value is XML-escaped.
- **No secrets** are involved. The variables added are public URLs and flags.
- **IDOR / auth**: no new authenticated surface. `/sitemap.xml` and `/robots.txt` are intentionally public and read-only.

---

## 6. Test Plan

Iterate with targeted tests; run the full suite once at the end.

### 6.1. Unit tests (`cargo test -p web_ui_common -p web_ui_guest -p web_ui_admin`)

| ID | Target | Cases |
|---|---|---|
| U1 | `SiteConfig::from_lookup` | malformed input is never echoed (`!err.to_string().contains(input)`); missing → `Missing`; `""` → `Missing`; `ourplaces.io` → `NotAbsolute`; `https://x.io/app` → `HasPath`; `https://x.io/?a=1` → `NotAbsolute`; `http://localhost:3010` + not indexable → ok; `http://…` + indexable → `InsecureWhenIndexable`; trailing `/` accepted; `SEO_INDEXABLE` ∈ {`true`,`1`} → true, {`TRUE `,`yes`,`0`,unset} → false; Twitter handle valid/invalid/unset |
| U2 | `robots_txt` | indexable: every `PRIVATE_PREFIXES` entry present, `Allow: /`, `Sitemap:` line with absolute URL; not indexable: exactly `User-agent: *\nDisallow: /\n`, no `Sitemap` |
| U3 | admin robots | always disallow-all |
| U4 | `/sitemap.xml` served over HTTP (real route, Topcoat `Sitemap`, `BaseUrl` on the router) against a **wiremock** `listing_api` | parses as XML (`quick-xml`); 7 static URLs in order; listing URLs absolute, sorted and deduplicated; `<lastmod>` RFC 3339 and omitted when `None`; hostile slug escaped; the search has no `include_inactive`, `user_id`, `host`, `owner` or `currency` parameter |
| U4b | same harness, paging and failures | 101 listings → 2 page calls; page 2 returns 500 → 503 + `retry-after: 300` + `cache-control: no-store`, no `<urlset>`; 50 full pages → 503 (cap); success → `application/xml` + `public, max-age=3600` |
| U5 | robots coverage | every auth-guarded guest route prefix (`/checkout`, `/bookings`, `/profile`, `/requests`, `/reviews/submit`, `/verify`, …) is matched by a `Disallow` rule |
| U6 | `vacation_rental` | full listing → all properties; no price → no `makesOffer`; `review_count = 0` → no `aggregateRating`; NaN rating → none; coordinates `None` → no lat/long; no images → no `image`; amenities deduplicated and capped at 50; `price` serialised as a string equal to `Decimal::to_string()`; `numberOfBathroomsTotal = full + half`; `floorSize` only when set |
| U7 | `vacation_rental` currency | converted pair passes through; `no_rate_for` → base currency |
| U8 | `escape_json_for_script` | `</script><script>alert(1)</script>`, `<!--`, `&amp;`, U+2028/9 → no `<` `>` `&` U+2028/9 in the output; `serde_json::from_str(escaped) == original` |
| U9 | `json_ld` component render | output starts with `<script type="application/ld+json">`; `&` in a name is **not** HTML-escaped to `&amp;` |
| U10 | `guest_document` render (with `BaseUrl` in a test `Cx`) | `<meta charset` within the first 1024 bytes; exactly one `<title>`; canonical absolute without query; a request path of `//evil.example/x` → canonical on the configured origin; `og:site_name`, `og:image` absolute and hashed; `twitter:card=summary_large_image`; `twitter:site` only when configured |
| U11 | `SecurityHeaders` (needs a new non-HTML `#[route]` probe in `tests/security_headers.rs`, which has only `#[page]` probes today) | default (`new()`) → `x-robots-tag` on HTML, non-HTML **and a 404 error response**; `.indexable(true)` → absent on `/`, still present on `/bookings` and `/verify` (private prefixes); admin → always present, including on a 404; sitemap `cache-control` not overwritten |
| U12 | listing description meta | ≤ 160 **chars** with multi-byte and emoji input (no panic, valid UTF-8), word boundary, whitespace collapsed; fallbacks for `city = None` and `bedrooms = 0` |
| U13 | listing page with **wiremock** `listing_api` (needs the new `TopcoatApiClient::get_listing_by_id_at`, matching `search_listings`, because `get_listing_by_id` ignores the client URL today, `client.rs:103-109`) | full-page render → the detail endpoint is called `.expect(1)` time (memoize shared with metadata); `/listings/<uuid>` → canonical and JSON-LD `url` use the slug; mocked 404 → 404 status; mocked 500 → 5xx status with the error page and default head, not a blank page |

### 6.2. Integration tests (`cargo test -p db_core`, `-p listing_api`; local Postgres via devenv)

| ID | Case |
|---|---|
| I1 | Column shape via `information_schema.columns`: `updated_at` is `timestamp with time zone`, `NOT NULL`, default `now()`; the trigger `update_listing_updated_at` exists. (`TestPg` migrates an empty DB, so the backfill is verified manually: run `sqlx migrate run` on a copy of a DB at `20261114053147` with rows, then `SELECT count(*) FROM listing WHERE updated_at <> added_at` → 0, recorded in the PR) |
| I2 | Set the fixture listing's `updated_at = '2000-01-01'`, then `UPDATE listing` (any column) → `updated_at > '2000-01-01'` |
| I3 | `last_modified_at` from the public select: a processed original photo with a newer `updated_at` raises it; a `PendingUpload`/`Processing`/`Failed` photo, or a variant row (`parent_id IS NOT NULL`), does not |
| I4 | From a `2000-01-01` baseline: photo rename advances `last_modified_at`; `delete_listing_image` advances `listing.updated_at`; a listing hard delete (no bookings) succeeds |
| I5 | From a `2000-01-01` baseline: creating, updating or deleting a price override leaves `updated_at` unchanged |
| I6 | `GET /api/v1/listings` and `GET /api/v1/listings/{id}` include `last_modified_at`; inactive and deleted listings are absent from the list |
| I7 | `ListingResponse` JSON without `last_modified_at` deserialises to `None` (rollout-skew guard, a `common` unit test) |
| I8 | Concurrency, with two `TestPg` connections and no sleeps: (1) tx 1 runs `delete_listing_image(A)` of a primary photo, which bumps `listing` and promotes B, and stays open; (2) conn 2 starts `rename_listing_image(B)` in a spawned task; (3) the test polls `pg_locks` until conn 2 is seen **waiting** (pattern of `db_core/src/booking_request/hold_request_tests.rs:201-215`), then **commits tx 1**; (4) conn 2 completes. Repeat with `mark_listing_image_processed(B)`. Neither raises `40P01`, and conn 2 uses `SET lock_timeout = '10s'`, so the wait is never misread as `55P03` |

### 6.3. E2E (Playwright, `playwright/e2e/guest-portal/seo.spec.ts`, `admin-portal/seo.spec.ts`)

| ID | Case |
|---|---|
| E1 | `GET /sitemap.xml`: 200, `application/xml`; parses with `DOMParser`; root `urlset` in the sitemaps 0.9 namespace; contains `/`, `/listings`, `/about` and the 4 policy URLs; every `<loc>` starts with `GUEST_PORTAL_URL`; every `<lastmod>` is ISO-8601. Listing assertions use **only listings this test creates** with `createListingCopy()` (other specs mutate listings in parallel, `fullyParallel`): its active copy is present; its deactivated copy and its soft-deleted copy (`psql "UPDATE listing SET deleted_at = now() …"`) are absent |
| E2 | Sitemap validates against the sitemaps.org 0.9 XSD, vendored at `playwright/e2e/fixtures/sitemap-0.9.xsd`, using the `xmllint-wasm` npm dev-dependency (pure WASM, so no system `xmllint` is needed; the host doesn't have one). Never skipped |
| E3 | `GET /robots.txt` (local = not indexable): `Disallow: /`, no `Sitemap`; `x-robots-tag: noindex, nofollow`. The indexable mode is covered by U2/U11 plus a manual prod `curl` (§6.4); no second `SEO_INDEXABLE=true` web server |
| E4 | Listing page (on a `createListingCopy()` listing given reviews via `psql "UPDATE listing SET review_count = 3, overall_rating = 4.67 …"`, since no fixture seeds reviews): exactly one `script[type="application/ld+json"]`; `JSON.parse` succeeds; `@type == "VacationRental"`; has `name`, `url` (== canonical), `identifier`, `address.addressCountry`, `containsPlace.occupancy.value`; `makesOffer.priceSpecification.price` matches `/^\d+(\.\d+)?$/` and `priceCurrency` is 3 letters; `aggregateRating.reviewCount == 3` |
| E5 | Zero CSP violations on `/`, `/listings` and a listing page, through the existing base fixture (`playwright/e2e/fixtures/base.fixture.ts:11-26` already fails a test on any CSP violation) |
| E6 | Head tags on `/` and a listing page: `link[rel=canonical]` absolute and without query, even when visited with `?check_in=…&currency=EUR`; visiting `/listings/<uuid>` gives the slug canonical; `og:title`, `og:description`, `og:url`, `og:image` (absolute), `og:site_name`; `twitter:card`; listing `<title>` contains the listing name |
| E7 | Unknown slug → 404 status with the normal 404 page (no 500), no JSON-LD |
| E8 | Admin: `GET /robots.txt` → `Disallow: /`; `/login` and a 404 response both have `x-robots-tag: noindex, nofollow` |
| E9 | Existing `security.spec.ts` CSP assertions still pass unchanged |

### 6.4. Manual (post-deploy, recorded in the PR)
- Google Rich Results Test and the Schema.org validator on one dev listing URL (via URL-inspection paste of the rendered HTML, since dev is `noindex`): 0 errors.
- `curl -sI` on the dev guest: `x-robots-tag` present. On prod after the main merge: absent, and `robots.txt` lists `Sitemap: https://ourplaces.io/sitemap.xml`.
- A shared-link preview (WhatsApp, X card validator) shows the listing photo and title.

---

## 7. Exclusions / Non-Goals
- Sitemap index or splitting, image sitemaps, hreflang.
- Search Console setup, sitemap ping, IndexNow.
- Generated per-listing OG images.
- Mapping free-text amenities to Google's VacationRental amenity vocabulary.
- `BreadcrumbList`, `FAQPage`, `Event` schema.
- `noindex` for filtered `/listings` URLs or policy pages (they stay indexable; canonical folds the query).
- Admin SEO beyond blocking it.
- Core Web Vitals and page-speed work.
- Changing `/listings` card or listing page visuals.

---

## 8. Delivery Order

| Phase | Slice | Verify |
|---|---|---|
| 1 | `SiteConfig` + fail-fast startup + `BaseUrl` on router + local, devenv, docker-compose and Playwright env + `ENVIRONMENT.md`, **pushed together with the CI wiring (phase 7 moves up, §1.9.8)** | U1; guest starts locally; `GUEST_PORTAL_URL= cargo run -p web_ui_guest; test $? -ne 0` |
| 2 | `X-Robots-Tag` (queued, default on, private prefixes) + guest and admin `/robots.txt` | U2, U3, U5, U11, E3, E8 |
| 3 | Migration + listing row trigger + `last_modified_at` read path + delete bump + plumbing (`db_core`, `api_core`, `common`, `listing_api`, struct literals) + `sqlx prepare` | I1–I8 |
| 4 | `/sitemap.xml` | U4, E1, E2 |
| 5 | Metadata: `tags()` in the shell, defaults, OG image asset, per-page titles, listing `from_fn` + memoized `load` | U10, U12, E6, E7 |
| 6 | JSON-LD component + `vacation_rental` builder + listing page | U6–U9, E4, E5 |
| 7 | CI: URL resolution, reconcile step, `SEO_INDEXABLE`, `configure-email-links` rule | Feature-branch deploy: guest `GUEST_PORTAL_URL` == its own `run.app` URL; email `BASE_URL` == own guest URL |
| 8 | Full suite, clippy `-D warnings`, `missing_docs`, panic and env-var greps, manual checks in §6.4 | `CONSTRAINTS.md` §2 table green |

---

## 9. Open Items
- [ ] **X handle**: the owner confirmed an X account exists, but the handle hasn't been supplied. It's set via `SEO_TWITTER_SITE` (a GitHub Actions var per env, or prod only), so it isn't a code blocker.
- [ ] **Pre-merge to main**: verify the `ourplaces.io` Cloud Run domain mapping exists. Otherwise prod canonicals point at a host that doesn't resolve.

---

## Acceptance Criteria
- [x] 1. The guest service refuses to start, with a clear `error!` log, when `GUEST_PORTAL_URL` is missing or invalid, or is `http` while `SEO_INDEXABLE=true`. No request-host fallback exists. ✅ (U1; startup check verified locally)
- [x] 2. `/sitemap.xml` validates against the sitemaps 0.9 schema. It lists the 7 static pages plus exactly the active, non-deleted listings of active owners, with absolute canonical-origin URLs and `lastmod` from `listing.updated_at`. It returns 503 rather than a partial sitemap on API failure. ✅ (Topcoat `Sitemap`; U4/U4b, E1, E2 XSD)
- [x] 3. `listing.updated_at` exists (backfilled from `added_at`) and advances on any listing update and on photo delete. Public `last_modified_at` also reflects processed original photo changes. Neither moves on price overrides or in-progress uploads, and there are no triggers on `listing_image` (I8: no deadlock). ✅ (I1–I8)
- [x] 4. Guest `/robots.txt` disallows every auth-only and transactional prefix and names the sitemap when `SEO_INDEXABLE=true`. Otherwise it is `Disallow: /`, and every guest response carries `X-Robots-Tag: noindex, nofollow`. ✅ for the non-indexable mode (E3, U11); indexable mode by U2/U11, live check after the main deploy (§6.4)
- [x] 5. Admin `/robots.txt` is `Disallow: /`, and every admin response carries `X-Robots-Tag: noindex, nofollow`, in all environments. ✅ (U3, U11, E8)
- [x] 6. Every active listing page has exactly one valid `VacationRental` JSON-LD block, with `identifier`, `name`, `url`, `image`, `address`, public `latitude`/`longitude`, `containsPlace` (occupancy, rooms, amenities), and `aggregateRating` only when reviews exist. `makesOffer` carries the `Decimal` base nightly rate as a string with its currency. The Schema.org validator reports 0 errors. ✅ (U6/U7, U13, E4). Schema.org validator run on a deployed page still to do (§6.4, manual)
- [x] 7. JSON-LD containing hostile text cannot break out of its `<script>`, and the CSP is byte-for-byte unchanged with zero violations reported. ✅ (U8/U9, E5 via the base fixture; CSP constants untouched)
- [x] 8. Home, listings, about, policy and listing pages render a per-page `<title>`, a description, a query-free absolute canonical, `og:title`, `og:description`, `og:url`, `og:image` (listing photo or default 1200×630 asset), `og:site_name`, and `twitter:card=summary_large_image`. ✅ (U10, U12/U13, E6)
- [x] 9. Unknown or inactive listing slugs still return the normal 404 page. ✅ (U13, E7)
- [x] 10. CI: main uses `GUEST_PORTAL_URL_PROD` + `SEO_INDEXABLE=true`; dev uses `GUEST_PORTAL_URL_DEV`; feature branches use their own guest service URL for the guest, email links and admin's calendar-export link. ✅ for feature branches (run 44003087150: reconciled own URL). The `main`/`dev` rules take effect on their next deploy
- [x] 11. Playwright `seo.spec.ts` (guest and admin) passes in Chromium and Firefox. Unit and integration tests in §6 pass. `cargo clippy -D warnings`, `missing_docs`, the panic grep and the env-var grep are clean. ✅ (all 4 projects; full suite: no #102-related failures)
- [x] 12. `docs/ENVIRONMENT.md`, `devenv.nix` and the Playwright config document and set `GUEST_PORTAL_URL`, `SEO_INDEXABLE` and `SEO_TWITTER_SITE`. ✅ (`SEO_TWITTER_SITE` documented; not set anywhere until the handle is known)
