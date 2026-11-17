# Implementation Plan: SEO — sitemap.xml, robots.txt and JSON-LD (#102)

**Spec Reference:** [`docs/specs/spec-102-feat-seo-sitemap-robots-json-ld.md`](../specs/spec-102-feat-seo-sitemap-robots-json-ld.md)
**Intent:** [`docs/intent/intent-102-feat-seo-sitemap-robots-json-ld.md`](../intent/intent-102-feat-seo-sitemap-robots-json-ld.md)
**Baseline Commit:** `796460aa`
**Branch:** `feat/102-seo-sitemap-xml-robots-txt-and-json-ld-structured-dat`

> **Engineering Review Status**: LOCKED (rev 4). Round 2 re-verification: no open blockers; 6 text fixes applied
> **Review Date**: 2026-11-16
> **Review Panel**: Lead Architect, Edge Case Analyst, Security Reviewer, DB Guardian, QA Strategist

## Cross-Cutting Rules (apply to every task)
- **No panics**: no `.unwrap()`/`.expect()` in `web_ui_*/src`, `web_client/src`, `common/`, `db_core/`, `app_api/`. Tests may use them.
- **Env vars**: `static FOO_ENV: &str = "FOO";` constants, never raw literals in `env::var` (RS-W1015).
- **Docs**: every new `pub` item and field has `///` (`-D missing_docs`).
- **Money**: JSON-LD `price` comes from `Decimal::to_string()` and never touches `f64`. The rating `f64` is a score, not money.
- **Strings**: truncate by `chars()`, never by byte index (multi-byte text panics).
- **`BaseUrl` in tests**: any render of `guest_document`/`tags()` puts `topcoat::context::BaseUrl` in the test `Cx`. Without it, Topcoat panics resolving canonical, OG URLs or asset images.
- **JSON escape sequences in source** (T1 finding): never type a backslash-u-plus-4-hex escape literally in files written by the agent; the write tool decodes it into the real character. Build such escapes at runtime (`concat!("\\", "u003c")`, or `format!("\\u{:04x}", c as u32)`), and check the bytes afterwards with `grep -c 'u003c'` or `od -c`. This applies above all to T13 (`escape_json_for_script`) and its tests.
- **One `Unescaped` site**: `topcoat::view::Unescaped::new_unchecked` is used **only** in `web_ui_common/src/components/json_ld.rs`. Check with `rg -l "Unescaped" web_ui_*/src`, which must list exactly that file.
- **HTTP mocking**: web UI tests that need `listing_api` use **wiremock**, with the setup in `web_ui_guest/src/pages/calendar_export_tests.rs`. The client must be built with the mock URL. `get_listing_by_id` ignores `listing_api_url` today (`web_client/src/client.rs:103-109`), so T8 fixes that first.
- **Test naming**: new tests live in named modules whose names are the verification filters: `site_config_tests`, `seo_tests`, `robots_tests`, `updated_at_tests`, `sitemap_tests`, `page_titles_tests`, `listing_detail_seo_tests`, `json_ld_tests`, `vacation_rental_tests`. A filter must match ≥ the stated test count, because "0 passed" is not green.
- **Field or signature changes**: any task that adds or removes a struct field or a function parameter also runs `SQLX_OFFLINE=true cargo check --workspace --exclude protoproj --all-features --tests`, so dependent crates are compiled too.
- **Targeted while iterating**: run only the task's command. The full suite runs once, in T17.
- **SQLx**: `cargo sqlx prepare --workspace -- --all-targets` after any query change. This regenerates `.sqlx/`, `db_core/.sqlx/` and `app_api/listing_api/.sqlx/`. Otherwise use `SQLX_OFFLINE=true`.
- **DB**: local Postgres via devenv (`devenv shell -- …`). Wait for another agent's devenv build before starting `db-start`. Portals started with `devenv up -d` keep the daemon's old env, so restart the guest by hand after T2.
- **Commits and pushes**: one commit per task (`feat(#102): …`), pushed to `origin/feat/102-…` only (never `dev`).
  - **T2 carries both the fail-fast startup and the CI wiring**, so they can't be pushed apart. Pushing the startup check alone would make the next CI deploy start a guest without `GUEST_PORTAL_URL`; the revision would fail and the deploy would break.
- **CI hygiene** (T2): new workflow steps read `vars.*`, `steps.*.outputs` and gcloud results only through `env:`, never through `${{ }}` inside `run:`.

## Dependency Graph
Each line is `task ← dependencies`.
```
T1  topcoat spike                     ← —
T2  SiteConfig + fail-fast + CI      ← —
T3  X-Robots-Tag (queued, default on) ← T2
T4  guest robots.txt                  ← T2, T3
T5  admin robots.txt                  ← T3
T6  migration (listing row trigger)   ← —
T7  last_modified_at + delete bump    ← T6
T8  web_client get_listing_by_id_at   ← —
T9  sitemap.xml                       ← T2, T7
T10 shell metadata + OG asset         ← T1, T2
T11 static page titles                ← T10
T12 listing metadata (+ errors, UUID) ← T1, T8, T10
T13 json_ld component                 ← T1
T14 vacation_rental builder           ← —
T15 JSON-LD on listing page           ← T12, T13, T14
T16 Playwright SEO specs              ← T4, T5, T9, T11, T15
T17 docs + full gates                 ← all
```
Risk-first: T1 settles the Topcoat unknowns before T10, T12 and T13. T6, T8 and T14 have no dependencies and can run in parallel with T2–T5.

## Checkpoints
- **A** (after T5): `cargo test -p web_ui_common -p web_ui_guest -p web_ui_admin`. The guest boots locally and refuses to boot without `GUEST_PORTAL_URL`. After the T2 push, the feature-branch deploy is green.
- **B** (after T9): backend + web UI tests. `curl localhost:3010/sitemap.xml` lists seeded listings with `lastmod`.
- **C** (after T15): web UI tests. A listing page renders JSON-LD and head tags; `/listings/nope` returns 404; `/listings/<uuid>` declares the slug canonical.
- **D** (T17): the full `CONSTRAINTS.md` §2 table plus the sanity-check workflow.

---

## Tasks

- [x] **T1: Topcoat risk spike** (kept as regression test `web_ui_common/tests/metadata_contract.rs`, 8 tests)
  - **Scope:** XS
  - **Description:** A test module with **probe pages** (not the real listing route, which needs a live `listing_api` and returns 500 offline). It answers:
    - **Q1**: a `#[page]` with `with(Metadata::from_fn(f))`, where `f` and the page both return `Err(not_found())`. Is the response a 404 with the normal body, or a 500?
    - **Q1b**: the same, where both return a 5xx error.
    - **Q1c**: the same, where `f` returns `Ok(Metadata::new())` and the page errors. Is the status correct and is the head the default?
    - **Q2**: does `<script type="application/ld+json">(Unescaped::new_unchecked(s))</script>` emit `s` byte-for-byte, with `&` not turned into `&amp;`?
    - **Q3**: does `tags()` with a canonical and an `asset!` OG image panic without `BaseUrl`, and pass with it?
  - **Files Touched:** `web_ui_common/tests/metadata_contract.rs` (moved from the planned `web_ui_guest/src/spike_tests.rs`; see Spike Results)
  - **Acceptance Criteria:**
    - [x] Answers recorded under "Spike Results".
    - [x] Q1c is the design (spec §1.5.4): `listing_metadata` swallows every load error. If Q1c doesn't give the right status, stop and revise T12 before building it.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_common --test metadata_contract` (8 tests ✅)
  - **Dependencies:** None

- [x] **T2: Fail-fast `SiteConfig` with CI public-URL wiring (one commit, one push)**
  - **Scope:** L (kept as one task on purpose: shipping the startup check without the CI wiring breaks the next deploy)
  - **Description:**
    - **App**: add `web_ui_common::site_config` (spec §1.1):
      - `SiteConfig { base_url, indexable, twitter_site }`;
      - `SiteConfigError` (`Missing`, `NotAbsolute`, `HasPath`, `InsecureWhenIndexable`), using `BaseUrl::new` + `prefix()`;
      - `from_lookup`/`from_env`;
      - `SITE_DESCRIPTION`, `OG_DEFAULT_ALT` and `page_title()`.

      In the guest `main.rs`, `SiteConfig::from_env()` with `inspect_err(|e| tracing::error!(variable = GUEST_PORTAL_URL_ENV, reason = %e, …))?` runs before the router is built, followed by `.with(site.base_url.clone()).with(site.clone())`.
    - **Local env**: `devenv.nix`, `docker-compose.yml` (guest service) and the guest `webServer.env` in the Playwright config get `GUEST_PORTAL_URL=http://localhost:3010`.
    - **CI** (`.github/workflows/cicd_full_stack.yml`, spec §1.9):
      - **Resolve public URL** step: `main` → `_PROD`, `dev` → `_DEV` (`::error::` if empty), other branches → `https://$SERVICE_NAME-$PROJECT_NUMBER.$REGION.run.app`. Reject `,`, whitespace and newlines.
      - `GUEST_PORTAL_URL` and `SEO_INDEXABLE` go into the guest `env_vars`.
      - **Reconcile** step for feature branches. It reads the `run.googleapis.com/urls` annotation + `.status.url` through `jq`; confirm the field on one real `describe` first.
      - The `public_url` job output is the post-reconcile value; the admin job gets an equivalent.
      - `configure-email-links` uses the same rule, and its fallback `lookup` reads the guest service's `GUEST_PORTAL_URL` env, not `status.url`.
      - Everything goes through `env:`.
  - **Files Touched:**
    - `web_ui_common/src/site_config.rs`, `web_ui_common/src/site_config_tests.rs`, `web_ui_common/src/lib.rs`
    - `web_ui_guest/src/main.rs`
    - `devenv.nix`, `docker-compose.yml`, `playwright/playwright.config.ts`
    - `.github/workflows/cicd_full_stack.yml`
  - **Acceptance Criteria:**
    - [x] U1 passes (18 tests, including "malformed input not echoed").
    - [x] Startup fails as intended: exactly one `ERROR` line naming `GUEST_PORTAL_URL`, and the port is not bound (exit 1, verified).
    - [x] `npx --yes actionlint` (or `docker run rhysd/actionlint`) is clean on the workflow. Verified with `rhysd/actionlint:1.7.7`: 67 findings before and after, none new.
    - [x] After the push: the guest service env has `GUEST_PORTAL_URL` equal to its own reachable `run.app` URL and `SEO_INDEXABLE=false`. booking-api `BASE_URL` and admin `GUEST_PORTAL_URL` equal the same value. The feature-branch deploy is green. (Run 44003087150: reconcile replaced the deterministic URL, which wasn't issued for this 59-char name, with `status.url`; all three match; the guest returns 200.)
  - **Verification Command:** locally `SQLX_OFFLINE=true cargo test -p web_ui_common site_config_tests && ! GUEST_PORTAL_URL= SQLX_OFFLINE=true cargo run -q -p web_ui_guest 2>err.log; grep -q 'GUEST_PORTAL_URL' err.log && grep -q 'refusing to start' err.log` (it must fail **because of** the variable, not for an unrelated reason); after CI `for s in our-places-web-ui-guest-$SFX booking-api-$SFX our-places-web-ui-admin-$SFX; do gcloud run services describe $s --region us-central1 --format=json | jq -r '.spec.template.spec.containers[0].env[] | select(.name|test("GUEST_PORTAL_URL|BASE_URL|SEO_INDEXABLE")) | "\(.name)=\(.value)"'; done` with `SFX=feat-102-seo-sitemap-xml-robots-txt`
  - **Dependencies:** None

- [x] **T3: `X-Robots-Tag`: queued, on by default, private prefixes**
  - **Scope:** S
  - **Description:** Change `SecurityHeaders` (spec §1.6):
    - `noindex` defaults to `true` for `new()` and `admin()`;
    - the header is added to the **queued** `response_headers(cx)` set before `next.run(...)`, next to the CSP, so 404 and 500 responses carry it;
    - `.indexable(bool)` opts out;
    - `.noindex_prefixes(&'static [&'static str])` keeps it on matching paths when indexable.

    Guest: `.indexable(site.indexable).noindex_prefixes(seo::PRIVATE_PREFIXES)`. `PRIVATE_PREFIXES` is defined here, in `web_ui_guest/src/seo/mod.rs`. Add a non-HTML `#[route]` probe to the tests.
  - **Files Touched:**
    - `web_ui_common/src/security_headers.rs`, `web_ui_common/tests/security_headers.rs`
    - `web_ui_guest/src/main.rs`, `web_ui_guest/src/seo/mod.rs`, `web_ui_guest/src/lib.rs`
  - **Acceptance Criteria:**
    - [x] U11 passes (7 tests: HTML, non-HTML, 404 error, indexable `/`, indexable private prefix, admin incl. 404 and `indexable(true)`, wildcard matcher).
    - [x] The CSP constants are unchanged (existing tests green, untouched).
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_common --test security_headers`
  - **Dependencies:** T2

- [x] **T4: Guest `/robots.txt`**
  - **Scope:** S
  - **Description:** A pure `robots_txt(base: &BaseUrl, indexable: bool) -> String` in `seo/mod.rs`, and `#[route(GET "/robots.txt")]` in `pages/seo.rs` (`text/plain; charset=utf-8`, `public, max-age=3600`). Test U5 scans `include_str!` of the `pages/*.rs` files for guarded `#[page("…")]` paths, meaning files whose handler calls `require_guest`, and asserts each is disallowed. No hand-maintained expected list.
  - **Files Touched:**
    - `web_ui_guest/src/seo/mod.rs`, `web_ui_guest/src/seo/seo_tests.rs`
    - `web_ui_guest/src/pages/seo.rs`, `web_ui_guest/src/pages/mod.rs`
  - **Acceptance Criteria:**
    - [x] U2 and U5 pass (6 tests). U5 fails when a guarded page is added without a matching prefix (verified by temporarily removing `/profile`).
    - [ ] Local `curl -i localhost:3010/robots.txt` → `Disallow: /`, no `Sitemap`, `x-robots-tag` present. (Deferred to E3 in T16, which needs a running guest.)
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_guest seo_tests`
  - **Dependencies:** T2, T3

- [x] **T5: Admin `/robots.txt`**
  - **Scope:** XS
  - **Description:** `#[route(GET "/robots.txt")]` in `web_ui_admin/src/pages/robots.rs`. It always returns `User-agent: *\nDisallow: /\n` and has no auth guard. Tests go in `mod robots_tests`.
  - **Files Touched:** `web_ui_admin/src/pages/robots.rs`, `web_ui_admin/src/pages/mod.rs`
  - **Acceptance Criteria:**
    - [x] U3 passes (2 tests).
    - [ ] A signed-out `curl localhost:3012/robots.txt` → 200, disallow-all, `x-robots-tag`. (Live check deferred to E8 in T16; the header is covered by U11.)
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_admin robots_tests`
  - **Dependencies:** T3

- [x] **T6: Migration adding `listing.updated_at` and its row trigger**
  - **Scope:** S
  - **Description:** `sqlx migrate add listing_updated_at`, with the spec §1.7.1–2 SQL: add the column, backfill from `added_at`, `NOT NULL DEFAULT now()`, the `BEFORE UPDATE` trigger using `update_updated_at_column()`, and the `STABLE` function `listing_last_modified(uuid, timestamptz)`. **No `listing_image` triggers.**
  - **Files Touched:**
    - `db_core/migrations/<ts>_listing_updated_at.sql`
    - `db_core/src/listing/updated_at_tests.rs`, `db_core/src/listing/mod.rs`
  - **Acceptance Criteria:**
    - [x] I1 (via `information_schema`, which also asserts that the function exists and that `listing_image` has no triggers), I2 and I5 pass (6 tests), using a `2000-01-01` baseline so changes are visible (≥ 4 tests).
    - [x] Manual backfill check, recorded in the PR: on a DB copy at `20261114053147` with rows, `SELECT count(*) FROM listing WHERE updated_at <> added_at` → 0. (2026-11-17: 3 seeded rows, 0 mismatched.)
  - **Verification Command:** `devenv shell -- cargo test -p db_core updated_at_tests`
  - **Dependencies:** None

- [x] **T7: `last_modified_at` read path, photo-delete bump and plumbing**
  - **Scope:** M (compiler-driven, wide but mechanical)
  - **Description:**
    - Select `listing_last_modified(listing.id, listing.updated_at) AS "last_modified_at!"` (the function comes from T6's migration; spec §1.7.2) in **every** select that fills `Listing`/`ListingWithOwner`: `queries.rs` `get_listings`, `get_listings_by_user_id`, `get_listing_by_id` and `get_listing_by_id_or_slug` (both branches). Check with `EXPLAIN` that the inner query uses `idx_listing_image_listing_id`.
    - `delete_listing_image` runs `UPDATE listing SET updated_at = NOW() WHERE id = $1` as its **first** statement.
    - Add `updated_at` and `last_modified_at` (both `DateTime<Utc>`) to `Listing`/`ListingWithOwner`, and both columns to the `RETURNING` lists of the `write.rs:25,121,174` `query_as!` calls.
    - Add `last_modified_at` to `common::models::ListingResponse` (`#[serde(default)] Option`, for rollout skew).
    - Map it in `api_core/src/models.rs:9,109` and `listing_api` `manager_listing_response` (`apis.rs:412`).
    - Fix every `ListingResponse` literal listed in spec §1.7.5.
    - Run `cargo sqlx prepare`.
  - **Files Touched:**
    - `db_core/src/models.rs`, `db_core/src/listing/{queries,write,images}.rs`, `db_core/src/listing/updated_at_tests.rs`
    - `common/src/models.rs`
    - `app_api/api_core/src/models.rs`, `app_api/listing_api/src/{apis,apis_test}.rs`
    - `web_ui_guest/src/pages/{listings,listing_detail}.rs` (test literals), `web_ui_admin/tests/listing_location.rs`, `protoproj/src/lib.rs`
    - `.sqlx/`, `db_core/.sqlx/`, `app_api/listing_api/.sqlx/`
  - **Acceptance Criteria:**
    - [x] I3, I4, I6, I7 and **I8 (concurrent photo delete vs rename/processed, no 40P01 or 55P03)** pass (11 tests: 8 db_core, 1 listing_api, 2 common). `EXPLAIN` shows the photo lookup uses the `listing_id` index.
    - [x] `cargo sqlx prepare --workspace --check` is clean, and the workspace `cargo check --tests` passes. Only the root `.sqlx/` holds these queries; `db_core/.sqlx` and `listing_api/.sqlx` are stale legacy directories (last touched in #96) and never held them. `protoproj`'s `ListingResponse` is an unrelated protobuf type, and the guest/admin test "literals" are JSON, so neither needed changes.
  - **Verification Command:** `devenv shell -- cargo test -p db_core updated_at_tests && devenv shell -- cargo test -p listing_api last_modified && SQLX_OFFLINE=true cargo test -p common listing_response_last_modified && SQLX_OFFLINE=true cargo check --workspace --exclude protoproj --all-features --tests`
  - **Dependencies:** T6

- [x] **T8: `TopcoatApiClient::get_listing_by_id_at` (mockable listing client)**
  - **Scope:** S
  - **Description:** Add `common::app_client::get_listing_by_id_at(client, base_url, audience, id_or_slug, currency)`, following the `search_listings_at` pattern, and make `TopcoatApiClient::get_listing_by_id` call it with `self.listing_api_url`. The existing free function keeps its behaviour for other callers.
  - **Files Touched:** `common/src/app_client/listings.rs`, `web_client/src/client.rs`, `web_client/src/client_tests.rs` (or the existing tests module)
  - **Acceptance Criteria:**
    - [x] A wiremock test proves `get_listing_by_id` hits the configured `listing_api_url` (2 tests; the second shows the slug is percent-encoded, so a crafted path value can't add upstream query parameters).
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_client get_listing_by_id_at && SQLX_OFFLINE=true cargo check --workspace --exclude protoproj --all-features --tests`
  - **Dependencies:** None

- [x] **T9: Guest `/sitemap.xml`**
  - **Scope:** M
  - **Description:**
    - In `seo/mod.rs`: `sitemap_for(listings) -> Sitemap` builds **Topcoat's `Sitemap`** (`topcoat::router::content::sitemap`, `sitemap` feature on `web_ui_guest`; owner request 2026-11-17). It holds the 7 static paths, then listings sorted and deduplicated by slug, with `last_modified(SystemTime::from(last_modified_at))`. Topcoat writes, escapes and resolves the XML against `BaseUrl`.
    - `collect_listings(&TopcoatApiClient)` pages `search_listings` at 100 (`sitemap_search(page)`: `ListingSearchParams::default()` plus paging). The route returns the `Sitemap` with a queued `cache-control: public, max-age=3600`. Any error **or** reaching the 50-page cap → `error!` + 503 (`retry-after: 300`, `no-store`). Success → `application/xml` (set by Topcoat).
    - Add `quick-xml` and `reqwest` as dev-dependencies (the tests serve the real route over HTTP).
  - **Files Touched:**
    - `web_ui_guest/src/seo/mod.rs`, `web_ui_guest/src/seo/seo_tests.rs`, `web_ui_guest/src/seo/sitemap_tests.rs`
    - `web_ui_guest/src/pages/seo.rs`, `web_ui_guest/Cargo.toml`
  - **Acceptance Criteria:**
    - [x] U4 + U4b pass: 7 HTTP tests against the real route and a wiremock `listing_api` (empty, sorted/deduplicated/dated, hostile slug, 101 → 2 calls, public-only params, page-2 500 → 503 + headers, cap → 503).
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_guest seo::seo_tests && SQLX_OFFLINE=true cargo test -p web_ui_guest seo::sitemap_tests` (cargo takes one filter per run)
  - **Dependencies:** T2, T7

- [x] **T10: Shell metadata, router defaults and the OG asset**
  - **Scope:** M
  - **Description:**
    - Crop the image once: `magick web_ui_common/assets/8ac5cca0-cf4e-11f1-9294-9dd6950248f8.jpg -resize 1200x630^ -gravity center -extent 1200x630 -quality 82 -strip web_ui_common/src/assets/og-default.jpg`. Keep it ≤ 250 KB, then `git rm`/remove the original and `web_ui_common/assets/`.
    - Add `OG_DEFAULT_IMAGE` in `assets.rs`.
    - `guest_document`: `topcoat::metadata::tags()` is the first child of `<head>`, and the hand-written charset, viewport and title go.
    - Remove `title` from `GuestShell`/`guest_shell` and update `layout.rs`.
    - Router-level defaults through `Metadata::from_fn(site_defaults)`, with canonical `base.join(path)` (**not** `Url::Current`, because of the `//` cross-origin case).
    - Update `tests/shells.rs`, which uses `GuestShell.title` at line 19, to render with `BaseUrl`.
  - **Files Touched:**
    - `web_ui_common/src/assets/og-default.jpg`, `web_ui_common/src/assets.rs`
    - `web_ui_common/src/shell_guest.rs`, `web_ui_common/tests/shells.rs`
    - `web_ui_guest/src/layout.rs`, `web_ui_guest/src/main.rs`
  - **Acceptance Criteria:**
    - [x] U10 passes: 7 HTTP tests in `web_ui_common/tests/site_metadata.rs` (a probe layout rendering `tags()`, the router set up like the guest's), including the `//evil.example/x` canonical. The defaults come from `site_config::site_metadata` (`Metadata::from_fn`), with the image in `default_image_metadata()`.
    - [x] `file og-default.jpg` reports 1200x630; size ≤ 250 KB (190 KB). The owner's full-size original is left **untracked** in `web_ui_common/assets/`, not deleted: it's their only copy. The alt text was changed to "Garden path to a villa entrance with the sea beyond" to match the photo.
    - [x] The admin shell is unchanged. (The asset OG image and the full document head are checked end to end in E6, because `cargo test` has no asset bundle.)
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_common --test shells && SQLX_OFFLINE=true cargo check --workspace --exclude protoproj --all-features --tests`
  - **Dependencies:** T1, T2

- [x] **T11: Static page titles**
  - **Scope:** S
  - **Description:** Add `with(Metadata::new().title(page_title("…")))` to `/listings` ("Villas in Jamaica"), `/about`, the 4 policy pages, `/login` and `/register`. `/` keeps the default. Table-driven render test in `mod page_titles_tests`.
  - **Files Touched:** `web_ui_guest/src/pages/{listings,about,policies,auth,register}.rs`, `web_ui_guest/src/pages/page_titles_tests.rs`
  - **Acceptance Criteria:**
    - [x] Each page's `<title>` is its page title plus ` · Our Places`: 2 tests (`seo::page_titles_tests`) over the `seo::titles` table and a source scan proving each of the 8 pages attaches its entry. Rendered titles are checked in E6, because `cargo test` can't render guest pages (the layout needs the asset bundle and a session store).
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_guest page_titles_tests` (2 ✅)
  - **Dependencies:** T10

- [x] **T12: Listing metadata (slug canonical, error-safe)**
  - **Scope:** M
  - **Description:**
    - Make `load` `#[memoize(as_ref)] async fn load(cx: &Cx, slug: &str) -> Result<Loaded>`, and adapt the call sites.
    - Add `listing_metadata`:
      - canonical `base.join("/listings/{listing.slug}")`, not the request path;
      - title `page_title(name)`;
      - `meta_description` (char-based, with fallbacks for `city = None` and `bedrooms = 0`);
      - `og:image` = `images[0]` desktop variant with a fallback.
      - **Any** load error → `Ok(Metadata::new())` + `debug!`.
    - Attach it with `#[page("/listings/{slug}", with(Metadata::from_fn(listing_metadata)))]`.
  - **Files Touched:** `web_ui_guest/src/pages/listing_detail.rs`, `web_ui_guest/src/pages/listing_detail_seo_tests.rs`
  - **Acceptance Criteria:**
    - [x] U12 passes (6 tests, including emoji and multi-byte text and both fallbacks). `truncate_on_word` and `meta_description` live in `crate::seo` (pure; the JSON-LD description cap reuses them).
    - [x] U13 passes with wiremock (4 tests: one detail call per request, `.expect(1)`, shared by the page and `listing_metadata`; UUID path → slug canonical; mocked 404 → 404; mocked 500 → 5xx). They use an explicit router (probe layout rendering `tags()`, probe page using the real `load`), because full guest pages need the asset bundle. On errors Topcoat sends its plain error body, not a page with a head (T1).
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_guest listing_detail_seo_tests`
  - **Dependencies:** T1, T8, T10

- [x] **T13: `json_ld` component and script-safe escaping**
  - **Scope:** S
  - **Description:** Add `web_ui_common::components::json_ld`:
    - `escape_json_for_script(&str) -> String`: each of `<` `>` `&` U+2028 U+2029 becomes its six-character JSON escape (hex `003c` `003e` `0026` `2028` `2029`; spec §1.4.7);
    - `#[component] json_ld(value: serde_json::Value)`, which emits `<script type="application/ld+json">` via `Unescaped::new_unchecked`. A serialisation failure → `warn!` and render nothing.

    Tests go in `mod json_ld_tests`, named `json_ld_*`.
  - **Files Touched:** `web_ui_common/src/components/json_ld.rs`, `web_ui_common/src/components/mod.rs`, `web_ui_common/tests/components.rs`
  - **Acceptance Criteria:**
    - [x] U8 and U9 pass (6 tests). The output has no raw `<`, `>`, `&`, U+2028 or U+2029. `serde_json::from_str(escaped) == original` for hostile inputs (`</script><script>…`, `<!--`, `<script`, `&amp;`, a literal backslash before `<`). `&` is not HTML-escaped.
    - [x] `rg -l "Unescaped" web_ui_*/src` lists only `json_ld.rs`. No backslash-u sequence was typed: escapes are built from code points, and `grep -c u003c` returns 0 in both files.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_common json_ld_`
  - **Dependencies:** T1

- [ ] **T14: `vacation_rental` JSON-LD builder**
  - **Scope:** M
  - **Description:** Typed serde structs (`VacationRental`, `PostalAddress`, `Accommodation`, `QuantitativeValue`, `LocationFeatureSpecification`, `AggregateRating`, `Offer`, `UnitPriceSpecification`, all with `skip_serializing_if`) in `web_ui_guest/src/seo/json_ld.rs`, and `vacation_rental(details, canonical) -> VacationRental` per the spec §1.4 table. Rules:
    - `price` is `Decimal::to_string()`;
    - `numberOfBeds` is omitted when 0;
    - occupancy uses `max(1)`;
    - the description cap is char-based;
    - `makesOffer`, not `offers`.
  - **Files Touched:** `web_ui_guest/src/seo/json_ld.rs`, `web_ui_guest/src/seo/vacation_rental_tests.rs`, `web_ui_guest/src/seo/mod.rs`
  - **Acceptance Criteria:**
    - [ ] U6 and U7 pass (≥ 12 tests).
    - [ ] `rg -n "f64|f32" web_ui_guest/src/seo/json_ld.rs` matches only the rating field.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_guest vacation_rental_tests`
  - **Dependencies:** None

- [ ] **T15: Render JSON-LD on the listing page**
  - **Scope:** XS
  - **Description:** At the end of the listing content, render `json_ld(value: serde_json::to_value(vacation_rental(&details, &canonical))?)`, where `canonical` is the same slug-based value T12 uses. Extract it into one helper, `listing_canonical(cx, &listing)`.
  - **Files Touched:** `web_ui_guest/src/pages/listing_detail.rs`, `web_ui_guest/src/pages/listing_detail_seo_tests.rs`
  - **Acceptance Criteria:**
    - [ ] Extend U13: exactly one `ld+json` block; it parses; its `url` equals the canonical `<link>`, including on a `/listings/<uuid>` request.
    - [ ] The existing `listing_detail` tests are green.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_guest listing_detail`
  - **Dependencies:** T12, T13, T14

- [ ] **T16: Playwright SEO specs (guest and admin)**
  - **Scope:** M
  - **Description:**
    - `playwright/e2e/guest-portal/seo.spec.ts` covers E1–E7 and imports the **base fixture**, which already fails any test on a CSP violation (`base.fixture.ts:11-26`), for E5.
    - E1 asserts only on listings it creates with `createListingCopy()`: one active, one deactivated, one soft-deleted via `psql`.
    - E4 seeds reviews with `psql "UPDATE listing SET review_count = 3, overall_rating = 4.67 …"`.
    - E2 validates against a vendored `playwright/e2e/fixtures/sitemap-0.9.xsd` using `xmllint-wasm`, a dev-dependency in the **root** `package.json`/`package-lock.json`.
    - E3 covers the non-indexable mode only.
    - `playwright/e2e/admin-portal/seo.spec.ts` covers E8, including a 404.
    - Tag both `@new-only`. Restart any `devenv up` portal so it picks up `GUEST_PORTAL_URL` before running.
  - **Files Touched:**
    - `playwright/e2e/guest-portal/seo.spec.ts`, `playwright/e2e/admin-portal/seo.spec.ts`
    - `playwright/e2e/fixtures/sitemap-0.9.xsd`
    - `package.json`, `package-lock.json`
  - **Acceptance Criteria:**
    - [ ] E1–E8 pass in the 4 portal projects; `security.spec.ts` passes unchanged (E9).
  - **Verification Command:** `npx playwright test seo --project=guest-portal-chromium --project=guest-portal-firefox --project=admin-portal-chromium --project=admin-portal-firefox`
  - **Dependencies:** T4, T5, T9, T11, T15

- [ ] **T17: Docs and full quality gates**
  - **Scope:** S
  - **Description:**
    - `docs/ENVIRONMENT.md`: `GUEST_PORTAL_URL` (guest, **required**), `SEO_INDEXABLE`, `SEO_TWITTER_SITE`, the CI resolution rule, and the `devenv up -d` restart note.
    - Keep `web_ui_common/tests/metadata_contract.rs` (T1) as the Topcoat-upgrade regression guard.
    - Run the sanity-check workflow in full, including `npm ci`, `git branch -vv`, `cargo update --dry-run`/`cargo audit` and `cargo check --tests`, plus every `CONSTRAINTS.md` §2 gate.
  - **Files Touched:** `docs/ENVIRONMENT.md`, this plan (tick boxes)
  - **Acceptance Criteria:**
    - [ ] `cargo fmt --check`; clippy `-D warnings`; clippy `-D missing_docs`; the unwrap/expect, float and env-var `rg` gates; `cargo sqlx prepare --workspace --check`; backend tests; web UI tests; `cargo audit`. All green.
    - [ ] Spec Acceptance Criteria 1–12 ticked, or explicitly deferred with a reason.
  - **Verification Command:** see the `CONSTRAINTS.md` §2 table and `.claude/skills/sanity-check-workflow/`
  - **Dependencies:** T1–T16

---

## Round-1 Review Resolutions
| Finding (reviewer) | Resolution |
|---|---|
| Image triggers deadlock with photo delete and image worker (DB, Edge) | No `listing_image` triggers. Photo changes folded in at read time; the delete bumps `listing` first (T6/T7, I8) |
| T6 file list incomplete (DB, Edge) | T7 lists `write.rs`, `api_core`, all `ListingResponse` literals and the 3 `.sqlx` dirs |
| `/listings/<uuid>` duplicate canonical (Edge) | Slug-based canonical, `og:url` and JSON-LD `url` (D10, T12/T15, U13, E6) |
| Spike only covers 404 (Edge) | T1 Q1b/Q1c; `listing_metadata` swallows every load error |
| Page cap vs no-partial rule (Edge) | Cap → 503 (T9) |
| `docker-compose` and `devenv up -d` env (Edge, QA) | T2 files; restart note in the rules and in T17 |
| CI `status.urls` field and long names (Edge) | Annotation + `status.url` via `jq`, verified on a real describe; reconcile is the expected path; the email fallback reads the guest env (T2) |
| Spec escape text corrupted (Security) | Rewritten with hex spelled out; U8 is the source of truth |
| `X-Robots-Tag` missing on error responses (Security) | Queued before `next.run`, default on, plus private prefixes (T3) |
| `//` cross-origin canonical (Security) | `base.join(path)` instead of `Url::Current` (T10, U10) |
| CI expression injection (Security) | `env:` only, plus value validation (T2) |
| `json_ld` signature mismatch (Security) | `serde_json::Value` everywhere |
| "One API call" untestable (QA) | T8 makes the client mockable; U13 uses wiremock `.expect(1)` |
| Sitemap HTTP contract untested (QA) | U4b wiremock tests (T9) |
| I1/I3 unobservable (QA) | `information_schema` + manual backfill check; `2000-01-01` baselines; I3 redefined for the read-time design |
| Filters, naming, cross-crate compile (QA) | Named modules = filters; workspace `cargo check --tests` on field changes |
| E1 flaky, E4 no reviews (QA) | Own `createListingCopy()` listings; `psql` review seeding |
| T2 pushed before CI (Lead Architect) | CI wiring merged into T2: one commit, one push |

## Round-2 Re-verification Fixes
| Finding | Fix |
|---|---|
| T9 verify passed two filters to cargo (invalid) | Two `cargo test` runs |
| `get_listing_by_id_or_slug` uses `query_as!(Listing, …)`; approach for `last_modified_at` unstated | SQL function `listing_last_modified` (T6) selected in every `Listing` query, including `RETURNING`; a non-optional DB field |
| Spec §1.9.3 referred to the wrong task | Now plan task T2 |
| Admin email-link fallback unclear | Spec §1.9.5: admin keeps the `status.url` fallback |
| T2 startup check could pass vacuously | Asserts stderr names `GUEST_PORTAL_URL` and `refusing to start` |
| I8 would time out on its own wait | Explicit sequence: detect the wait via `pg_locks`, commit tx 1, then conn 2 completes; 10s `lock_timeout` |

## Spike Results (filled by T1)
Run 2026-11-16. Kept as `web_ui_common/tests/metadata_contract.rs` (8 tests), not in `web_ui_guest`: there, `discover()` would also pick up the real guest layout, which calls the API.
- **Q1** (loader and page both 404): **404**.
- **Q1b** (both 5xx): **500**.
- **Q1c** (loader `Ok`, page 404 / 500): **404 / 500**, so the page status is kept. **The T12 design (swallow every load error) is confirmed.**
- **Extra**: loader 500 + page 404 → **500**. A failing loader overrides the page's status, which is why swallowing is required, not optional.
- **Extra**: error responses are Topcoat's plain bodies (`not found`, `internal server error`) and are **not** rendered through layouts. There is no head and no `tags()` on errors. The spec's "normal 404 page" means this plain 404, which is today's guest behaviour (the guest has no error boundary).
- **Q2** (`Unescaped` in `ld+json`): emitted byte-for-byte; `&` is not turned into `&amp;`.
- **Q3**: a site-relative canonical without `BaseUrl` **panics → 500**; with `BaseUrl` it gives an absolute URL. `tags()` also emits `charset`, `viewport`, `og:type=website` and `twitter:card` by default.
- **Asset OG image + `BaseUrl`**: not covered here, because an asset needs the asset bundle in the test binary. T10 (`tests/shells.rs`) covers it.
- **Tooling quirk found**: the agent's file-writing tool decodes a backslash-u-plus-4-hex sequence into the literal character. The spike's first JSON-LD probe silently became a real `</script>`. See the new cross-cutting rule.

## Open Items (carried from the spec)
- [ ] X handle for `SEO_TWITTER_SITE`, not yet supplied. Doesn't block the build.
- [ ] Before merging to main: confirm the `ourplaces.io` Cloud Run domain mapping exists.
