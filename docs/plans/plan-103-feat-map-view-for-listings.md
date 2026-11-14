# Implementation Plan: Map View for Listings (#103)

**Spec Reference:** [`docs/specs/spec-103-feat-map-view-for-listings.md`](../specs/spec-103-feat-map-view-for-listings.md)
**Intent:** [`docs/intent/intent-103-feat-map-view-for-listings.md`](../intent/intent-103-feat-map-view-for-listings.md)
**Baseline Commit:** `9a4da641`
**Branch:** `feat/103-map-view-for-listings`
**Merges Into:** `dev`

> **Engineering Review Status**: LOCKED (rev 3). The QA round-2 command fixes (actionlint moved to T3; the `yq` and `pipefail` checks in T10 and T25) were applied exactly as the reviewer specified and weren't re-audited.
> **Review Date**: 2026-11-14
> **Review Panel**: Lead Architect, Edge Case Analyst, Security Reviewer, DB Guardian, QA Strategist
> **Scores**: round 1: Architecture 7 · Edge Cases 6 · Security 7 · DB 7 · QA 6. Round 2: Architecture 9 · Edge Cases 8 · Security 8.5 · DB 8.5 · QA 7, rising to 8+ once the command fixes were applied.

## Cross-Cutting Rules (apply to every task)
- **Local database**: Postgres runs in the devenv shell, not Docker. Run `sqlx`, `psql` and the database tests through `devenv shell --`. Other commands use `SQLX_OFFLINE=true`.
- **SQLx metadata**: after changing a macro query, run `devenv shell -- cargo sqlx prepare --workspace -- --all-targets` and commit the root `.sqlx/` changes in the same task. `get_listings` is a runtime `QueryBuilder`, so only a DB test catches a missing column there, not `prepare --check`.
- **Task gates**:
  - `cargo fmt --check`
  - `SQLX_OFFLINE=true cargo clippy -p <touched crates> --all-targets --all-features -- -D warnings`
  - `SQLX_OFFLINE=true cargo check --workspace --exclude protoproj --all-features --tests`

  Every new public item has a `///` comment. Use no `unwrap` or `expect` in production code, and declare env var names as `static …_ENV` constants. Gates are in `CONSTRAINTS.md` at the repo root.
- **Tests come first**: write the failing test, then the code. Each task creates the new, uniquely named test module or spec file that its command filters on. Two kinds of filter don't count as green:
  - a filter that matches no tests, which reports "0 passed";
  - a filter that also matches older tests, because it can pass before the work exists.
- **Private code**: Topcoat page components are private, so their tests are inline `#[cfg(test)] mod <unique_name>` in the page file, as plan-101 did, and are run with `cargo test -p <crate> <unique_name>`. Integration tests under `tests/` only reach public items.
- **Log assertions**: any criterion that says "logged at warn" or "not logged" uses the existing `capture_logs` and `Logged` writer pattern (`listing_api/src/stay_total_tests.rs`, `web_ui_guest/src/pages/listings.rs` tests). A log is never assumed.
- **Database test isolation**:
  - `db_core` listing tests run in a rolled-back transaction on the real `DATABASE_URL`, scoped by `user_id` or `host`.
  - Anything that opens its own transaction uses `sqlx_db_tester::TestPg`, with `#[tokio::test(flavor = "multi_thread")]` and no sleeps.
- **Targeted tests while iterating**: run only the task's own command. The full suites run once, in Task 25.
- **The real point never leaks**:
  - The `api_core` listing mappers **fail closed**: coordinates and privacy default to `None`.
  - Only `listing_api`'s public mapper (T5) fills public coordinates, and only `with_stored_location` fills stored ones, on manager paths.
  - Coordinates and `LocationKey` never go into logs or span fields. `#[instrument]` on code that carries a listing uses `skip_all` with explicit fields.
- **Floats**: `f64` appears only in `common::location`, `db_core` row structs and `listing_api`. booking_api code and tests use `ListingLocation` and never write `f64`, so `! rg -q "f(32|64)" app_api/booking_api/` still passes.
- **Service boundary**: `listing_api`, `booking_api` and `user_api` never depend on each other. Shared types live in `common`, and queries live in `db_core`.
- **Client code**:
  - `ui.js` and `map.js` are wired through `data-*` attributes only. No inline script or style, no `eval`, `new Function`, `innerHTML`, `outerHTML`, `insertAdjacentHTML`, `setHTML` or `document.write`, and no new CSP `unsafe-*`.
  - Map attribution is set with `customAttribution` in `map.js`, so no HTML from the bucket is rendered.
- **Vendored JS**: exact versions, at least two weeks old, each with a `sha256` checksum in `asset!` that has been checked against the npm tarball, as htmx is.
- **CI scripts**: downloaded build tools run only in the `build-map-tiles` job, which has no secrets. Each workflow change passes `devenv shell -- actionlint .github/workflows/cicd_full_stack.yml`.
- **Browser tests and CSP**:
  - `ui-js` map tests serve their page from a fake http origin with `page.route`, sending the T12 policy as a real header, because `about:blank` plus `setContent` applies no CSP.
  - Every map spec, `ui-js`, guest or admin, fails on any CSP violation, any console message containing `Refused to`, or any `requestfailed` event on a map URL.
  - `csp-listener.js` only sees violations on `document`. The console check covers the Worker.
- **Firefox WebGL**: the `ui-js-firefox` map runs aren't gating if headless WebGL is unavailable on the runner. `ui-js-chromium` is gating.
- **E2E runs**: restart the APIs and portals from this worktree first (`apis-stop`, `frontends-stop`, then `apis-start`). The portals run on :3010 (guest) and :3012 (admin).
- **Struct literals**: adding a field to `NewListing`, `NewListingRequest`, `ListingResponse` or `BookingResponse` breaks every literal of that struct. Each of those tasks lists the literal sites and updates them in one mechanical change, setting the new field only.
- **Existing tests**: update them only where the spec requires a change, and never weaken an assertion. The `villa_carousel` tests stay, because the home page still uses it.

## Dependency Graph
Each line is `task ← dependencies`, matching every task's **Dependencies** field. The riskiest work comes first:
- the redaction, together with the echo guard and the admin form that would otherwise overwrite stored points;
- the Phase 0 spike, before any map UI.

```text
Phase 1: redaction (ships first, no map visible)
T1  common::location            ← —
T2  location_privacy column     ← T1
T3  offset key at startup, CI   ← T1
T4  privacy and coordinate input← T2
T5  public mapper, echo guard   ← T2, T3, T4
T6  user_api drops coordinates  ← T2
T7  manager location endpoint   ← T2
T8  admin edit from endpoint    ← T4, T7

Phase 0: spike (needs the -dev bucket, created by the Phase 1 push)
T9  spike                       ← Phase 1 checkpoint pushed

Phase 2: map infrastructure
T10 build tiles, no secrets     ← T9
T11 upload, lock, prod copy     ← T10
T12 local tiles in devenv       ← T9, T10
T13 map origin, CSP             ← T9
T14 portal deploy wiring        ← T11, T13
T15 vendored assets, lazy load  ← T9, T12, T13

Phase 3: guest maps
T16 listing page map            ← T5, T15
T17 card map data, templates    ← T5
T18 /listings grid and toggle   ← T17
T19 search map: badges, popups  ← T15, T18

Phase 4: reveal and picker
T20 booking listing_location    ← T2
T21 /bookings location block    ← T15, T20
T22 admin pin picker            ← T8, T15

Phase 5: finish
T23 docs, egress alert          ← T14, T22
T24 Playwright flows            ← T16, T19, T21, T22
T25 prod version, full gates    ← T23, T24
```

## Spec Coverage

| Spec section | Tasks |
|---|---|
| §1.1 Privacy, coordinate input, echo guard | T2, T4, T5, T8 |
| §1.2 Public coordinates, fail closed, logs | T2, T5, T6, T7 |
| §1.3 Approximation | T1 |
| §1.4 Offset key | T3 |
| §1.5 Exact location for guests | T20 |
| §1.6 `/listings` map | T17, T18, T19 |
| §1.7 Listing page map | T16 |
| §1.8 `/bookings` reveal | T21 |
| §1.9 Admin pin picker | T8, T22 |
| §1.10 Tiles: build, upload, lock, prod copy, validation | T10, T11, T12, T13, T14 |
| §1.11 Lazy loading, lifecycle, attribution | T15, T19 |
| §1.12 CSP and composition | T13 |
| §2.4 CI and infra | T3, T10, T11, T14 |
| §5 Accepted risks, egress alert | T23 |
| §7 Phase 0 spike | T9 |
| §6 Test plan | each task's tests, plus T24 |
| Acceptance criteria | T25 checks each one |

## Test Priorities

| Priority | Tasks |
|---|---|
| P0 privacy (no leak) | T1, T2, T5, T6, T8, T20, T24 |
| P0 happy path | T16, T19, T21, T22 |
| P0 invalid input | T4, T13 |
| P1 authz | T7, T20 |
| P1 degradation (no tiles, no JS, no WebGL, timeout) | T13, T15, T16, T24 |
| P1 concurrency (CI lock, status change between reads, map init and remove) | T11, T15, T20 |

## Risks

| Risk | Guard |
|---|---|
| A stale admin form saves the public point over the real one | T5's echo guard on the server keeps the stored point and logs `warn`. This holds even if T8's portal deploy fails or is rolled back. T8 also prefills only from the manager endpoint |
| A public path is missed by the redaction | The mappers fail closed (T2). T5 tests JSON and XML. T6 covers `user_api`. T24 scans the HTML of the listing page and `/listings` |
| Coordinates end up in traces or logs | T2 switches the `create_listing` spans to `skip_all`. T5 captures logs across search, detail, create and update |
| The MapLibre CSP build doesn't work under our CSP | T9 proves it with real tiles before any UI work. If it fails, stop and take it back to the originator |
| Assets aren't compressed (Cloud Run doesn't compress) | T9 measures it. If they aren't, T15 ships precompressed variants, or the decision goes back to the originator |
| Downloaded tools run with the deployer key present | T10 builds in a job with no secrets. T11 only uploads, after checking `MANIFEST` |
| The tile file is uploaded with gzip encoding | T11 uploads with no `Content-Encoding` and requires a `206` before writing `_READY` |
| Setup jobs on different dev branches cancel each other | There's no concurrency group. T11 uses the `_LOCK` object |
| The prod build date has expired upstream | T11 copies the verified dev folder into prod |
| A portal-only push skips the setup job, and with it the portal deploys | T14 uses `!cancelled()` with explicit success checks, verified on a portal-only push |
| `MAP_TILES_BASE_URL` broadens the CSP to all of GCS | T13 validates with an allowlist regex and fixes the composition order with `csp_with_storage_origin` |
| The key is missing in an environment | T3: listing_api refuses to start, and the CI guard fails before deploy |
| fake-gcs-server doesn't support range requests | T9 checks it. If it doesn't, T12 serves the fixture from a local static server instead |

---

## Tasks

### Phase 1: Redaction

- [x] **Task 1: Add `common::location`**
  - **Scope:** S
  - **Description:** Add the module with:
    - `LocationPrivacy` (`#[default] Approximate`) and `LocationPrecision`. Both use serde `snake_case`, and `sqlx::Type` with `type_name = "location_privacy"` behind the `sqlx` feature, as `BookingMode` is (`common/src/booking_request.rs:24-30`).
    - `ListingLocation { latitude, longitude }`.
    - `LocationKey::from_secret(&str) -> Result<LocationKey, LocationKeyError>`: the trimmed value must be at least 32 bytes, and `Debug` is redacted.
    - `approximate_point` (spec §1.3): top 53 bits, and the wrap after rounding.
    - `JAMAICA_BOUNDS`, `in_bounds` and the `APPROX_*` constants.

    Add `hmac` as a workspace dependency of `common`.
  - **Files Touched:**
    - `common/src/location.rs`
    - `common/src/location_tests.rs` (`#[path]` from `location.rs`)
    - `common/src/lib.rs`
    - `common/Cargo.toml`
    - `Cargo.toml`
  - **Acceptance Criteria:**
    - [ ] The same key and ID give the same point. A different key or a different ID gives a different point.
    - [ ] For 10,000 random IDs, the haversine distance is within [150, 501.5]m, and the output has at most 5 decimal places.
    - [ ] The longitude offset is scaled by `cos(lat)`. Latitude clamps, and a point that rounds to 180.0 wraps to −180.0.
    - [ ] A key under 32 bytes, or a blank one, is refused. `format!("{key:?}")` doesn't contain the key.
    - [ ] `LocationPrivacy` round-trips through serde, an unknown value is refused, and the default is `Approximate`. `in_bounds` is inclusive at its edges.
    - [ ] `cargo audit` stays clean.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p common location_tests && cargo audit`
  - **Dependencies:** None

- [x] **Task 2: Add the `location_privacy` column, and make the mappers fail closed**
  - **Scope:** M (mostly mechanical literal updates)
  - **Description:**
    - **Migration**: add the spec §2.1 migration with `devenv shell -- sqlx migrate add map_view_location_privacy`.
    - **Columns**: add `location_privacy` at every column site, `queries.rs:112` (the runtime `QueryBuilder`), `:238`, `:274`, `:337`, `:359`, and `write.rs:38`, `:120`, `:204`, `:154/:160`. On update, bind with `COALESCE` next to `$28`. On create, bind `COALESCE($n, 'approximate'::location_privacy)`.
    - **Spans**: switch `#[instrument]` on `create_listing` and `create_listing_with_active_currency` (`write.rs:10,86`) to `skip_all`.
    - **Response fields**: `ListingResponse` gains `location_privacy: Option<LocationPrivacy>` and `location_precision: Option<LocationPrecision>`, both `#[serde(default, skip_serializing_if = "Option::is_none")]`.
    - **Mappers**: the `api_core` mappers set `latitude`, `longitude`, `location_privacy` and `location_precision` to `None`. Add `with_stored_location(&mut ListingResponse, &row)`, and call it on the create and update responses. The test `listing_mapper_fail_closed` is a new fn in the existing `mod tests` of `app_api/api_core/src/models.rs`, so the filter is guaranteed to match.
    - Update every struct literal.
    - Regenerate `.sqlx/`.
  - **Files Touched:**
    - `db_core/migrations/<ts>_map_view_location_privacy.sql`
    - `db_core/src/models.rs`
    - `db_core/src/listing/{queries,write,mod}.rs`
    - `db_core/src/listing/location_privacy_tests.rs`
    - `app_api/api_core/src/models.rs`
    - `common/src/models.rs`
    - **`NewListing` literals**: `db_core/src/{currency.rs:649, exchange_rate.rs:1398, reservation_change.rs:1107, tax_document.rs:986}`, `db_core/src/listing/tests.rs`, `db_core/src/booking/{tests.rs, unavailable_ranges_tests.rs}`, `app_api/booking_api/src/apis/{tests.rs, authz_tests.rs, rate_tests.rs}`, `app_api/listing_api/src/{apis.rs, apis_test.rs, apis_test_reviews.rs, stay_total_tests.rs}`
    - **`ListingResponse` literals**: `web_ui_guest/src/pages/listings.rs` (tests), `protoproj/src/lib.rs`, `app_api/listing_api/src/apis_test.rs`
    - `.sqlx/*`
  - **Acceptance Criteria:**
    - [ ] Existing rows read as `approximate`. A create with no privacy is `approximate`, and that db_core create doesn't fail on `NOT NULL`. An update with `None` keeps the stored value, and `Some(Exact)` changes it.
    - [ ] The history row records the setting as it was **before** the update (`write.rs:148-166` copies the row before the UPDATE). History rows from before this feature stay `NULL`.
    - [ ] `get_listings` against the DB returns `location_privacy` (the runtime query).
    - [ ] Mappers fail closed: a `ListingResponse` built from a row with coordinates has `None` coordinates until a caller opts in.
    - [ ] A JSON `ListingResponse` with no `location_*` keys still deserializes, which is the case for portals reading older APIs.
    - [ ] A test with a capturing subscriber shows no latitude or longitude value in the spans of a db_core create.
    - [ ] `sqlx prepare --check` passes.
  - **Verification Command:** `devenv shell -- sqlx migrate run && devenv shell -- cargo test -p db_core location_privacy_tests && devenv shell -- cargo test -p db_core --test listing_span_privacy && SQLX_OFFLINE=true cargo test -p api_core listing_mapper_fail_closed && devenv shell -- cargo sqlx prepare --workspace --check -- --all-targets`
  - **Built note:** the span check lives in its own integration binary (`db_core/tests/listing_span_privacy.rs`). Tracing caches callsite interest across the whole process, so a thread-local recorder missed spans whenever parallel tests had registered the same callsites first. Mutation-checked: putting back `skip(executor)` makes it fail.
  - **Dependencies:** Task 1

- [x] **Task 3: Load the offset key at startup and in CI**
  - **Scope:** S
  - **Description:**
    - Add `listing_api::location_key::load(get_env) -> Result<LocationKey>`, reading `static LOCATION_OFFSET_KEY_ENV`.
    - In `main.rs`, call it after `jwt_secret()` with the context "Refusing to start", and register it as `web::Data<LocationKey>`.
    - Every test app that builds `configure_routes` registers a fixed test key: `apis_test.rs` (mounted as `apis::tests`), `apis_test_reviews.rs`, `authz_tests.rs`, `calendar_tests.rs` and `stay_total_tests.rs`.
    - `deploy-api` passes `LOCATION_OFFSET_KEY` to listing_api only, behind an `::error::` guard for an empty value.
    - Document the local key in `.env` (already present in this worktree) and add a placeholder to `.env.example` if the file exists.
    - Add `pkgs.actionlint` to `devenv.nix`, because this task's own check needs it.
  - **Files Touched:**
    - `app_api/listing_api/src/location_key.rs`
    - `app_api/listing_api/src/location_key_tests.rs`
    - `app_api/listing_api/src/{lib,main}.rs`
    - `app_api/listing_api/src/{apis_test,apis_test_reviews,authz_tests,calendar_tests,stay_total_tests}.rs`
    - `.github/workflows/cicd_full_stack.yml`
    - `devenv.nix`
  - **Acceptance Criteria:**
    - [ ] With the variable missing, blank or 31 bytes, `load` returns an error that names the variable and not the value. With 32 bytes or more, it returns `Ok`.
    - [ ] The whole listing_api crate passes with the test key registered.
    - [ ] `actionlint -shellcheck=` is clean, and the number of shellcheck findings doesn't go up. The baseline already has 67, all from before this branch, so a plain `actionlint` can't exit 0. The new guard step uses the same shape as the `JWT_SECRET` guard, and the key appears only in the listing_api `env_vars`.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p listing_api location_key_tests && devenv shell -- cargo test -p listing_api && devenv shell -- actionlint -shellcheck= .github/workflows/cicd_full_stack.yml`, and `actionlint` findings compared with `git show <base>:.github/workflows/cicd_full_stack.yml` (67 → 67)
  - **Built note:** there's no `.env.example` in the repo, so no placeholder was added. The local key is already in this worktree's `.env`. The plan's later "actionlint clean" criteria (T10, T11, T14) use the same no-new-findings rule.
  - **Dependencies:** Task 1

- [x] **Task 4: Validate privacy and coordinate input**
  - **Scope:** S
  - **Description:**
    - `NewListingRequest` and `UpdatedListingRequest` gain `location_privacy: Option<LocationPrivacy>`, passed through to `db_core`.
    - Coordinate validation (spec §1.1.4): both or neither, finite, within range. Bad input gets 400 `VALIDATION_ERROR` with the field name.
    - Update the OpenAPI schemas and the `NewListingRequest` literals in `apis_test.rs`.
  - **Files Touched:**
    - `common/src/models.rs`
    - `app_api/listing_api/src/apis.rs`
    - `app_api/listing_api/src/location_input_tests.rs`
    - `app_api/listing_api/src/apis_test.rs`
  - **Acceptance Criteria:**
    - [ ] A create with no privacy reads back as `approximate`, and a create with `"exact"` reads back as `exact`. A PATCH with no privacy keeps the stored value.
    - [ ] Each of these gets 400 naming its field:
      - `"nearby"` as the privacy value;
      - only `latitude`;
      - `latitude: 91`;
      - `longitude: -181`;
      - a `NaN` or `Infinity` sent as a string-encoded float.
  - **Verification Command:** `devenv shell -- cargo test -p listing_api location_input_tests`
  - **Built note:** an unknown privacy value is refused by actix's JSON extractor with a 400 whose body names the bad value (`nearby`), not the field. Coordinate errors come from `common::location::validate_coordinates` and are keyed by field. JSON can't carry `NaN` or `Infinity`: a `"NaN"` string or `1e400` is refused with a 400 by deserialization, and the validator also refuses non-finite values. Mutation-checked: removing the validation call makes the tests fail. `web_ui_admin` sends `location_privacy: None` until T8.
  - **Dependencies:** Task 2

- [x] **Task 5: Public coordinates mapper and echo guard**
  - **Scope:** M
  - **Description:**
    - Add `public_location(&mut ListingResponse, stored, privacy, &LocationKey)`, implementing spec §1.2, and call it in `get_listings` and `get_listing_by_id` before the currency conversion.
    - Public paths leave `location_privacy` `None`.
    - Add the echo guard to `update_listing` (spec §1.1.5).
  - **Files Touched:**
    - `app_api/listing_api/src/public_location.rs`
    - `app_api/listing_api/src/public_location_tests.rs`
    - `app_api/listing_api/src/{apis,lib}.rs`
  - **Acceptance Criteria:**
    - [ ] For an Approximate listing in search and detail, as both JSON and `Accept: application/xml`:
      - the raw body contains neither stored value;
      - the public point is 150–500m away;
      - `location_precision` is `approximate`;
      - there's no `location_privacy` key;
      - two requests return the same point.
    - [ ] Exact returns the stored values with `exact`. A missing coordinate returns `null`s.
    - [ ] Create and update responses carry the stored values and `location_privacy`.
    - [ ] Echo guard, applied **per component**:
      - PATCHing an Approximate listing with its own public point leaves the stored point unchanged and logs `warn` with no coordinates (captured);
      - a PATCH where only one component equals the public value keeps that component stored and saves the other;
      - a genuinely new point is saved.
    - [ ] Captured logs and spans across search, detail, create and update contain no coordinate value.
  - **Verification Command:** `devenv shell -- cargo test -p listing_api public_location_tests`
  - **Built note:**
    - **New query:** `db_core::listing::get_listing_location` (stored point and privacy, excluding deleted listings). The echo guard uses it, and T7 will reuse it.
    - **Leak check:** "the body contains neither stored value" is checked per component. An offset that runs purely east–west can leave latitude unchanged at 5 decimal places, so a component is checked only when the public value differs from the stored one. The whole point must always differ.
    - **Mutation checks:** making the mapper return the stored point fails the search/detail test. Disabling the guard fails the echo test.
  - **Dependencies:** Task 2, Task 3, Task 4

- [x] **Task 6: `user_api` drops coordinates**
  - **Scope:** XS
  - **Description:** `GET /api/v1/users/{email}/listings` uses the fail-closed mapper and never opts in, so coordinates, `location_privacy` and `location_precision` are all `None` (spec §1.2).
  - **Files Touched:**
    - `app_api/user_api/src/apis/profile.rs`
    - `app_api/user_api/src/apis/tests.rs` (new test fn `user_listings_drop_coordinates`)
  - **Acceptance Criteria:**
    - [ ] A listing with stored coordinates appears with no coordinate or privacy keys, and every other field is unchanged.
  - **Verification Command:** `devenv shell -- cargo test -p user_api user_listings_drop_coordinates`
  - **Built note:** the test passed the first time it ran, because T2's fail-closed mapper already removes the coordinates. T6 pins that behaviour and adds a comment at the call site so a later change doesn't opt in by mistake.
  - **Dependencies:** Task 2

- [x] **Task 7: Add `GET /api/v1/listings/{id}/location`**
  - **Scope:** S
  - **Description:** Add a handler that takes `Claims`, calls `authz::require_listing_manager(…, "get_listing_location")`, and returns `{latitude, longitude, location_privacy}` with `Cache-Control: no-store`. Register it in the `/api/v1/listings` scope with `content_negotiation_middleware` and `#[instrument(skip_all)]`. Add it to the OpenAPI paths and schemas.
  - **Files Touched:**
    - `app_api/listing_api/src/apis.rs`
    - `app_api/listing_api/src/listing_location_tests.rs`
  - **Acceptance Criteria:**
    - [ ] The owner and an admin get 200 with the stored values (Approximate included), and `Cache-Control: no-store`.
    - [ ] Another host gets 403, with a captured `warn`. No JWT gets 401. A deleted or unknown listing gets 404. No coordinates gets 200 with `null`s.
  - **Verification Command:** `devenv shell -- cargo test -p listing_api listing_location_tests`
  - **Built note:** the response type is `common::location::ListingLocationResponse`, which `web_client` reuses in T8. The handler builds it from `db_core::listing::get_listing_location`, the T5 query, not from the mapper.
  - **Dependencies:** Task 2

- [x] **Task 8: Admin edit form loads from the location endpoint**
  - **Scope:** M
  - **Description:**
    - Add `web_client::TopcoatApiClient::get_listing_location_as(id, jwt)`.
    - Add a pure function `edit_prefill(&TopcoatApiClient, id, jwt) -> Result<Prefill, EditPrefillError>`. The edit page uses it, and a failure renders 503 with the spec §1.9 message and no form.
    - Create and edit get the privacy radio group with the help text from spec §1.9, and post `location_privacy`.
    - Add `wiremock` as a dev-dependency of `web_ui_admin`.
  - **Files Touched:**
    - `web_client/src/client.rs`
    - `web_client/tests/listing_location_client.rs`
    - `web_ui_admin/Cargo.toml`
    - `web_ui_admin/src/pages/listings/{form,edit}.rs`
    - `web_ui_admin/tests/listing_location.rs`
  - **Acceptance Criteria:**
    - [ ] The client maps 200, 403, 404 and 5xx to typed results (wiremock).
    - [ ] `edit_prefill` uses the endpoint's point, not the detail's. Wiremock serves two different points, and the test checks which one is used. A 5xx gives `Err`, and the page renders no `name="latitude"` input.
    - [ ] The radio defaults to Approximate on create and reflects the stored value on edit, and the posted form carries `location_privacy`.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_client --test listing_location_client && SQLX_OFFLINE=true cargo test -p web_ui_admin --test listing_location`
  - **Built note:**
    - **Prefill:** `ListingFields::from_listing` no longer reads coordinates. `with_location` fills them from the endpoint.
    - **Failure:** a failed location load returns topcoat's `service_unavailable(5)` and logs the listing ID. The page shows the framework's generic 503, not the spec §1.9 wording.
    - **Test scope:** the test checks the prefill source, the radio rendering and the posted value. It doesn't drive the full page route.
  - **Dependencies:** Task 4, Task 7

> **Phase 1 checkpoint**: run T1–T8's commands together, then push. This deploys the redaction and the echo guard. The push's backend changes trigger the setup job, which creates `our-places-gcs-map-tiles-dev`. Confirm with `gcloud storage buckets describe gs://our-places-gcs-map-tiles-dev`.

### Phase 0: Spike

- [x] **Task 9: Spike: MapLibre CSP build, asset compression, GCS ranges, CI lock**
  - **Built:** all criteria met; results in spec §7. Served from a local CSP server on :3099 rather than the guest portal (originator's choice). `img-src` needs no change; MapLibre 6 is ESM only.
  - **Scope:** S, with no production code. Use a scratch directory.
  - **Description:**
    - Pin the candidate MapLibre GL JS and `pmtiles` versions.
    - Cut a `--maxzoom=12` Jamaica extract and upload it to `gs://our-places-gcs-map-tiles-dev/spike/`, with no `Content-Encoding`.
    - Serve a bare page from the guest portal (:3010) under the spec §1.12 CSP, using `setWorkerUrl`.
    - Record the results in spec §7, then delete `spike/`.
  - **Acceptance Criteria (each objective):**
    - [x] The map renders: there's a non-background canvas pixel, zero CSP violations, and no `Refused to` messages in the console. Record whether `img-src blob:` is needed.
    - [x] Topcoat's asset route sends `Content-Encoding: br` or `gzip`, yes or no. If no, record the chosen fix in spec §4.
    - [x] A ranged **GET** returns `206`, and `access-control-allow-origin` matches the origin. Record any edge-cache headers on a repeat request.
    - [x] fake-gcs-server returns `206` for a range request, yes or no (this decides T12).
    - [x] `gcloud storage cp --if-generation-match=0` fails on the second attempt (this decides T11's lock).
    - [x] Record the size of the full z15 extract, the fonts and the sprites.
  - **Verification Command:** `curl -s -D - -o /dev/null -H 'Range: bytes=0-127' -H 'Origin: http://localhost:3010' https://storage.googleapis.com/our-places-gcs-map-tiles-dev/spike/jamaica.pmtiles | grep -iE '^HTTP|access-control-allow-origin|content-range'`, plus the bare page's console log.
  - **Dependencies:** Phase 1 checkpoint pushed

> **Gate**: if the CSP build can't run under our policy, stop and take the library choice back to the originator.

### Phase 2: Map Infrastructure

- [x] **Task 10: Build tiles in a job with no secrets**
  - **Built:** `build-map-tiles` outputs `needed`, `mode` (`build`, `copy-from-dev` or `none`) and `version`, and builds only for `mode=build`. The workflow-level `PROJECT_ID` (from a secret) is blanked in the job. The script also writes `BUILD_INFO` (build date, source, tool versions), which `MANIFEST` covers. A local build of `20261113` gave 781 files, 52MB, and `MANIFEST` verifies.
  - **Scope:** M
  - **Description:**
    - Write `scripts/build_map_tiles.sh <version> <base_url> <out_dir>` and `scripts/map-tiles/package.json` with a committed `package-lock.json` that pins `@protomaps/basemaps`.
    - The script:
      - pins the `pmtiles` CLI and checks its sha256;
      - runs `npm ci`;
      - fetches `basemaps-assets` by commit SHA with `git`;
      - extracts Jamaica;
      - generates styles with no source attribution;
      - writes `MANIFEST`.
    - Add the job `build-map-tiles`. It has no `auth` step and no secrets. It checks `_READY` with an anonymous `curl`, with a cache-busting query string (`?t=<run_id>`) and `Cache-Control: no-cache`, and outputs `needed`. It builds only when `needed=true`, and only when the env is dev or the dev folder is missing (the prod copy is in T11). It uploads the artifact.
    - Add `shellcheck`, `yq-go` and the `pmtiles` CLI to `devenv.nix` packages. `actionlint` was added in T3.
  - **Files Touched:**
    - `scripts/build_map_tiles.sh`
    - `scripts/map-tiles/package.json`
    - `scripts/map-tiles/package-lock.json`
    - `.github/workflows/cicd_full_stack.yml`
    - `devenv.nix`
  - **Acceptance Criteria:**
    - [x] `shellcheck` and `actionlint` are clean. The job has no `secrets.*` reference and no `google-github-actions/auth` step (grep check).
    - [x] A version that isn't 8 digits exits non-zero. `--dry-run` writes nothing.
    - [x] A local run with a recent build date produces `jamaica.pmtiles`, both styles, the fonts, the sprites and a `MANIFEST` whose sha256 values verify. The styles contain no `"attribution"` key.
  - **Verification Command:** `devenv shell -- shellcheck scripts/build_map_tiles.sh && devenv shell -- actionlint .github/workflows/cicd_full_stack.yml && devenv shell -- bash -o pipefail -c 'yq -e ".jobs[\"build-map-tiles\"]" .github/workflows/cicd_full_stack.yml > /tmp/bmt.yml && ! grep -qE "secrets\.|google-github-actions/auth" /tmp/bmt.yml' && devenv shell -- scripts/build_map_tiles.sh --dry-run 20261101 https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261101 "$(mktemp -d)"`
  - **Dependencies:** Task 9

- [x] **Task 11: Upload step, lock and prod copy**
  - **Built:** `MAP_TILES_VERSION_DEV=20261113` set. CI run 43593856950 built the tiles (51.5MB artifact) and published `v20261113/` (783 objects, lock released, ranged `206`, identity encoding). Its `manifest_sha256` matches the local build, so the build is reproducible. The two-run and cancel cases were tested on fake-gcs, not in CI. Tested on fake-gcs:
    - publish: 783 objects;
    - re-run skips in 1.5s;
    - two concurrent runs give one upload, and the other waits and exits 0;
    - a stale lock is cleared and taken;
    - a held lock times out with `::warning::` and exit 0;
    - a modified file, an extra file and a symlink are each refused with nothing uploaded;
    - `--copy-from-dev` gives an identical manifest and `crc32c`.

    The generation-matched lock release was checked on the real bucket, because fake-gcs reports an error on a delete that succeeds. Deviations:
    - `--copy-from-dev` takes the dev bucket from `DEV_BUCKET`, downloads the folder and runs the same allowlist and `MANIFEST` checks as an artifact;
    - the lock is released on every exit;
    - the setup job now tolerates a failed `build-map-tiles` (`!cancelled()` with the other needs checked explicitly), so a bad upstream date can't block the image worker;
    - unpublished tiles also start the setup job.
  - **Scope:** M
  - **Description:**
    - Write `scripts/upload_map_tiles.sh <bucket> <version> <artifact_dir|--copy-from-dev>`. It:
      - takes `_LOCK` with `--if-generation-match=0`, and releases it in a `trap … EXIT` (generation-matched) if `_READY` wasn't written;
      - if another run holds it, waits for `_READY` for up to 10 minutes. On timeout it emits `::warning::` and exits 0, so the setup job and `deploy-image-worker` aren't blocked; the portals then deploy with maps off, because `_READY` is missing;
      - clears a stale lock (over **10 minutes**, no `_READY`) only with `--if-generation-match=<observed generation>`, so one run never deletes a lock another run just took. The workflow's `cancel-in-progress: true` can kill a run that holds the lock;
      - verifies `MANIFEST`;
      - uploads only a fixed allowlist of expected paths, refuses symlinks and anything outside the artifact directory, and never executes or sources anything from the artifact;
      - takes the artifact only from `download-artifact` of the current run, never by `run-id`;
      - uploads with `Cache-Control: public, max-age=31536000, immutable` and no `Content-Encoding` on `.pmtiles`;
      - requires a ranged GET `206`;
      - writes `_READY` last, holding the build date, the tool versions, the manifest sha256 and the `.pmtiles` `crc32c`.

      For prod, when `-dev/v<version>/_READY` exists, it copies the dev folder and checks it against its `MANIFEST` instead of building.
    - Add the step "Upload Map Tiles" in `setup-gcs-image-gcs-items` after "Create Map Tiles Bucket". The job adds `needs: build-map-tiles`, and the step downloads the artifact when `needed=true`.
    - Set `MAP_TILES_VERSION_DEV` with `gh variable set`.
  - **Files Touched:**
    - `scripts/upload_map_tiles.sh`
    - `.github/workflows/cicd_full_stack.yml`
  - **Acceptance Criteria:**
    - [x] `shellcheck` and `actionlint` are clean.
    - [x] The first CI run fills `v<version>/`, and `_READY` holds all four fields. A second run skips in seconds.
    - [x] Two runs started together (re-run a job while another is in progress) produce one upload, and the other run waits and succeeds.
    - [x] A run cancelled mid-upload leaves no `_LOCK` (trap), or one that the next run clears after 10 minutes. No run fails because of a lock alone.
    - [x] A ranged GET on the uploaded `.pmtiles` returns `206` with no `Content-Encoding`.
    - [x] `--copy-from-dev` against a scratch bucket copies the folder and the manifest verifies.
  - **Verification Command:** `devenv shell -- shellcheck scripts/upload_map_tiles.sh && devenv shell -- actionlint .github/workflows/cicd_full_stack.yml && gcloud storage cat gs://our-places-gcs-map-tiles-dev/v$(gh variable list --json name,value --jq '.[]|select(.name=="MAP_TILES_VERSION_DEV").value')/_READY`
  - **Dependencies:** Task 10

- [x] **Task 12: Local tiles in devenv**
  - **Built:** `map-tiles-seed [YYYYMMDD]` (default: yesterday, UTC) builds z12 with the CI script into `.devenv/state/map-tiles/`, uploads it to fake-gcs with `curl` (no gcloud login needed), and requires a ranged `206`. It writes the base URL to `MAP_TILES_BASE_URL_FILE`. A version folder can't be named in static `env`, because Protomaps builds expire, so the portal processes and `enterShell` export `MAP_TILES_BASE_URL` from that file.
  - `build_map_tiles.sh` gained `--maxzoom` and accepts the emulator base URL only when `STORAGE_EMULATOR_HOST` is set.
  - `gcs-start` now allows the `range` request header in CORS; this applies the next time the emulator starts.
  - Fixture: z10, 1.39MB, 16 files, at `https://storage.googleapis.com/our-places-gcs-map-tiles-test/v20261113`.
  - **Scope:** S
  - **Description:**
    - Add the devenv script `map-tiles-seed`, which runs `build_map_tiles.sh` locally with `--maxzoom=12` and uploads to `our-places-gcs-map-tiles-local` in fake-gcs-server. If T9 showed fake-gcs-server can't serve ranges, it serves the folder from a local static server that can.
    - Set `env.MAP_TILES_BASE_URL` for the portals.
    - Commit a fixture under 2MB at `playwright/e2e/ui-js/fixtures/map/` for the `ui-js` project.
  - **Files Touched:**
    - `devenv.nix`
    - `playwright/e2e/ui-js/fixtures/map/*`
  - **Acceptance Criteria:**
    - [x] After `gcs-start` and `map-tiles-seed`, a ranged GET against the local base URL returns `206`.
    - [x] The fixture is under 2MB.
  - **Verification Command:** `devenv shell -- gcs-start && devenv shell -- map-tiles-seed && devenv shell -- bash -c 'curl -s -o /dev/null -w "%{http_code}" -H "Range: bytes=0-127" "$MAP_TILES_BASE_URL/jamaica.pmtiles"'` returns `206`, and `du -sb playwright/e2e/ui-js/fixtures/map` is under 2097152.
  - **Dependencies:** Task 9, Task 10

- [x] **Task 13: Map origin config and CSP**
  - **Built:**
    - `MapConfig::parse` is a hand-written allowlist; no `regex` dependency.
    - `from_lookup` and `from_env` give one `warn` for unset or invalid.
    - Portals register `MapConfig` with `.with(..)` only when it's valid, so `map_config(cx)` returning `None` means maps are off.
    - `with_map_origin` takes `Option<&MapConfig>`.
    - `csp_with_map_origin` adds the bucket path to `connect-src` and `worker-src 'self'`, and runs after the storage origin.
    - Mutation-checked: dropping the `..` guard, and swapping the composition order.
  - **Scope:** M
  - **Description:**
    - Add `web_ui_common::map_config::MapConfig::from_env()`, which reads `static MAP_TILES_BASE_URL_ENV` and validates it with the spec §1.10 allowlist regex. The emulator form is accepted only when `emulator_endpoint()` is set. Anything else counts as unset, with one `warn`.
    - Add `SecurityHeaders::with_map_origin`, implementing spec §1.12, including the composition order with `csp_with_storage_origin`.
    - Both portals' `main.rs` register `MapConfig` and pass the origin to `SecurityHeaders`.
  - **Files Touched:**
    - `web_ui_common/src/map_config.rs` (with inline `mod map_config_tests`)
    - `web_ui_common/src/security_headers.rs`
    - `web_ui_common/src/lib.rs`
    - `web_ui_common/tests/security_headers.rs` (new `map_origin_*` tests)
    - `web_ui_guest/src/main.rs`
    - `web_ui_admin/src/main.rs`
  - **Acceptance Criteria:**
    - [x] Valid values: both policies gain `https://storage.googleapis.com/<bucket>/` in `connect-src` only (spec §7: `img-src` unchanged), plus `worker-src 'self'`. `script-src` and `style-src` don't change.
    - [x] Each of these is refused, with a captured `warn`, and leaves the policies byte-identical to today's:
      - `https://storage.googleapis.com/`
      - `…/bucket` with no version
      - `…/bucket/v2026-11`
      - `…/bucket/v20261101/`
      - `?`, `#`, `%`, `..`, `\t` and `\r` variants
      - `http://…`
      - the emulator form with no emulator set
    - [x] With both a storage origin and a map origin set, the guest `connect-src` contains the bucket path and never the bare `https://storage.googleapis.com`.
    - [x] No policy contains `unsafe-`.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_common map_config_tests && SQLX_OFFLINE=true cargo test -p web_ui_common --test security_headers map_origin`
  - **Dependencies:** Task 9

- [ ] **Task 14: Portal deploy wiring**
  - **Built (local):** both portal jobs need the setup and tile-build jobs and use the `!cancelled()` guard. A "Resolve Map Tiles" step sets `MAP_TILES_BASE_URL` only when `_READY` and the `crc32c` match. actionlint is clean. The deploy acceptance checks wait on the next push.
  - **Scope:** S
  - **Description:**
    - `deploy-web-ui-guest` and `deploy-web-ui-admin` add `setup-gcs-image-gcs-items` and `build-map-tiles` to `needs`, with `if: ${{ !cancelled() && needs.deploy-api.result == 'success' && needs.test-web-ui.result == 'success' && needs.setup-gcs-image-gcs-items.result != 'failure' && needs.build-map-tiles.result != 'failure' && <existing conditions> }}`. It's wrapped in `${{ }}` because a bare `!` starts a YAML tag. A failed build therefore blocks the portal deploys, as spec §1.10.5 says, and doesn't just skip the setup job.
    - Each one sets `MAP_TILES_BASE_URL` only when `_READY` for the configured version exists and its `crc32c` matches `objects describe`. Otherwise the value is empty, with a `::warning::`.
  - **Files Touched:**
    - `.github/workflows/cicd_full_stack.yml`
  - **Acceptance Criteria:**
    - [ ] actionlint is clean.
    - [ ] A **portal-only** push, where the setup job is skipped and `deploy-api` runs, deploys both portals.
    - [ ] A failed setup job, or a failed `build-map-tiles`, blocks both portal deploys.
    - [ ] With `_READY` present, the revisions have `MAP_TILES_BASE_URL` set. With it missing, or with a mismatching `crc32c`, the value is empty.
  - **Verification Command:** `devenv shell -- actionlint .github/workflows/cicd_full_stack.yml`, plus `gcloud run services describe our-places-web-ui-guest-<branch-suffix> --region us-central1 --format='value(spec.template.spec.containers[0].env)' | grep MAP_TILES_BASE_URL` after a portal-only push.
  - **Dependencies:** Task 11, Task 13

- [x] **Task 15: Vendored map assets and the lazy loader**
  - **Built:**
    - Vendored verbatim from the npm tarballs (registry `sha512` integrity checked) with `sha256` `asset!` checksums, plus MapLibre's licence file.
    - `map.js` is a classic script that `import()`s MapLibre. It exposes `OurPlacesMap.{init, remove, resize}` and sets `data-map-state` (`loading`, `ready`, `failed` or `idle`).
    - The `ui.js` loader uses a 200px observer to start a map, and an observer at two viewports (`200%`) to end it. It also watches `htmx:load`.
    - `map_container(kind, point, label, attrs)`.
    - 8 Playwright tests:
      - nothing loads before scroll, then a CSP-clean render with attribution;
      - a circle for approximate, a pin for exact;
      - a style 500;
      - no WebGL;
      - the 11-second timeout;
      - the generation token;
      - a quick scroll.
    - The generation-token test was mutation-checked: it finds 3 canvases without the guard. The quick-scroll test alone didn't catch that, because IntersectionObserver batches its callbacks.
  - **Scope:** M
  - **Description:**
    - Vendor the versions T9 pinned (`maplibre-gl` 6.15.0 `.mjs`, its module worker, its CSS, `pmtiles` 4.5.0 IIFE) as checksummed `asset!` constants. MapLibre loads with `import()` (spec §1.11); styles are prebuilt JSON from T10, so `@protomaps/basemaps` is build-time only.
    - Add `map.js` with `OurPlacesMap.init(el)` for the kinds `listing` and `booking`, each with one point and either a circle or a pin. Any other kind renders "Map unavailable"; this is real behaviour, not a stub. Include:
      - the light and dark style switch;
      - `maxBounds`;
      - `customAttribution`;
      - an init guard with a generation token;
      - `resize()` when the map is shown;
      - the 10-second load timeout and the "Map unavailable" fallback.
    - Add the lazy loader to `ui.js`.
    - Add the component `web_ui_common::components::map::map_container`, which renders nothing without `MapConfig` and reserves the container's height.
  - **Files Touched:**
    - `web_ui_common/src/assets/` (vendored files, `map.js`)
    - `web_ui_common/src/assets.rs`
    - `web_ui_common/src/assets/ui.js`
    - `web_ui_common/src/components/map.rs` (inline `mod map_component_tests`)
    - `web_ui_common/src/components/mod.rs`
    - `playwright/e2e/ui-js/map.spec.ts`
  - **Acceptance Criteria:**
    - [x] ui-js tests, on the fake-origin page with the T13 CSP header and the fixture served with real 206 slicing:
      - zero map requests happen before the container scrolls into view;
      - the canvas renders after it scrolls in;
      - the attribution text is visible;
      - a 500 on the style shows "Map unavailable";
      - with `getContext` stubbed to return `null`, "Map unavailable" shows and the location text is still there;
      - a style that never responds shows "Map unavailable" within 11 seconds;
      - scrolling out and in quickly leaves one live map.
    - [x] Approximate draws a circle and no pin. Exact draws a pin.
    - [x] `map_container` renders nothing without `MapConfig`, and otherwise carries the hashed URLs and a fixed height.
    - [x] `map.js` contains none of the forbidden DOM or eval APIs (grep).
  - **Verification Command:** `! grep -nE "innerHTML|outerHTML|insertAdjacentHTML|setHTML|document\.write|new Function|eval\(" web_ui_common/src/assets/map.js && SQLX_OFFLINE=true cargo test -p web_ui_common map_component_tests && devenv shell -- npx playwright test --config=playwright/playwright.config.ts --project=ui-js-chromium map.spec`
  - **Dependencies:** Task 9, Task 12, Task 13

### Phase 3: Guest Maps

- [x] **Task 16: Listing page map**
  - **Scope:** S
  - **Description:** In `#location`, add `map_container(listing, public point, precision)`, at zoom 13 for Approximate and 15 for Exact, with cooperative gestures. Update the copy as spec §1.7 describes. With no coordinates, or coordinates outside the bounds, show "Location not mapped" and no container. The out-of-bounds case logs `warn`.
  - **Files Touched:**
    - `web_ui_guest/src/pages/listing_detail.rs` (inline `mod listing_map_tests`)
  - **Acceptance Criteria:**
    - [x] Approximate: the container shows the public point, `data-precision="approximate"`, and the "shared after you book" copy.
    - [x] Exact: `data-precision="exact"` and "Exact location shown."
    - [x] No coordinates, or out of bounds: "Location not mapped" and no `data-map`. The out-of-bounds `warn` is captured.
    - [x] Without `MapConfig`: no `data-map`, and the location text is still there.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_guest listing_map_tests`
  - **Dependencies:** Task 5, Task 15
  - **Built:** `listing_map_point` (a point of unknown precision is drawn as approximate; out of bounds logs `warn` with the listing ID and no coordinates) and `location_map`. With maps off, the old decorative placeholder stays, so the box doesn't change. `#location` now also shows when there are coordinates but no drive times. 6 tests; the bounds check and the approximate default are mutation-checked.

- [x] **Task 17: Map data and popup templates on listing cards**
  - **Scope:** S
  - **Description:** `ListingSummary` gains `map_point: Option<MapPoint>`, taken from the public coordinates. It's `None` when either coordinate is missing or out of bounds. Add `map_card_template` and `listing_card_map_attrs`, as spec §1.6 describes.
  - **Files Touched:**
    - `web_ui_common/src/components/domain/listing_card.rs`
    - `web_ui_common/tests/components.rs` (new `map_card_*` tests)
  - **Acceptance Criteria:**
    - [x] The attributes carry the public point and precision, and nothing when `None`.
    - [x] The template escapes the name and carries the card's href, search included.
    - [x] The whole `components` suite still passes, so the existing snapshots haven't changed.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_common --test components`
  - **Dependencies:** Task 5
  - **Built:** `MapPoint::of_listing` (shared with T16's `listing_map_point`, which adds the `warn`). `listing_card_map_attrs(cx, &summary)` also sets `data-map-name` for the badge label. `map_card_template` renders `<template data-map-card="{slug}">`. 3 `map_card_*` tests; the bounds filter is mutation-checked through T16's test.

- [ ] **Task 18: `/listings` grid, sticky map and toggle**
  - **Scope:** M
  - **Description:**
    - Replace `villa_carousel` on `/listings` with a grid whose card wrappers carry the T17 attributes and templates.
    - At `md` and above, the layout is `[grid | sticky map_container(search)]`.
    - Below `md`, the List/Map toggle links to `?view=`, enhanced by `ui.js` to switch in place (with `replaceState` and a `resize()` on show).
    - `SearchQuery` gains `view`. It isn't sent to the API, it's kept on currency switch, and any other value counts as `list`.
    - There's no map on Refused, Failed or empty, or when `MapConfig` is unset.
  - **Files Touched:**
    - `web_ui_guest/src/pages/listings.rs` (inline `mod listings_map_layout_tests`)
    - `web_ui_common/src/assets/ui.js`
    - `web_ui_common/src/components/domain/currency_selector.rs` (only if the kept-params list lives there)
  - **Acceptance Criteria:**
    - [ ] With `view=map`, the list carries a mobile-hidden class. With `view=list` or `view=junk`, the map does. At `md` both are visible, checked by class.
    - [ ] `params()` never includes `view`, and the currency links keep it.
    - [ ] There's no `data-map` on Refused, Failed or empty, or when unconfigured.
    - [ ] The existing `listings_page_tests` still pass.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_guest listings_map_layout_tests && SQLX_OFFLINE=true cargo test -p web_ui_guest listings_page_tests`
  - **Dependencies:** Task 17

- [ ] **Task 19: Search map: badges, circles and popups**
  - **Scope:** M
  - **Description:** In `map.js`, add the `search` kind (spec §1.6):
    - a `<button>` badge for each card, at least 44px;
    - a 500m circle layer for Approximate listings;
    - a popup built from the template with `setDOMContent`;
    - Enter or Space to open, and Escape to close and restore focus;
    - fit to bounds, or the single-marker or no-marker views.

    DOM markers are created once. Only the circle layers are added again on `style.load`. Add seed coordinates for the E2E listings.
  - **Files Touched:**
    - `web_ui_common/src/assets/map.js`
    - `playwright/e2e/ui-js/map-search.spec.ts`
    - `playwright/e2e/guest-portal/listings-map.spec.ts`
    - `playwright/e2e/fixtures/test-data.ts` (or the seed it uses): known Approximate and Exact points
  - **Acceptance Criteria:**
    - [ ] The number of badges equals the number of cards with `data-map-lat`, and Approximate listings have circles. After a theme toggle, and after a toggle **before** the first `load`, the badge count still equals the card count.
    - [ ] Clicking a badge opens the popup, and its link goes to the listing with the search.
    - [ ] Keyboard: Tab reaches the badges, Enter opens, and Escape closes and restores focus.
    - [ ] On the guest portal, there are no map requests before the map container is visible, and no CSP violations, `Refused to` messages or failed map requests.
  - **Verification Command:** `! grep -nE "innerHTML|outerHTML|insertAdjacentHTML|setHTML|document\.write|new Function" web_ui_common/src/assets/map.js && devenv shell -- npx playwright test --config=playwright/playwright.config.ts --project=ui-js-chromium map-search && devenv shell -- npx playwright test --config=playwright/playwright.config.ts --project=guest-portal-chromium listings-map`
  - **Dependencies:** Task 15, Task 18

### Phase 4: Reveal and Picker

- [ ] **Task 20: `listing_location` on bookings**
  - **Scope:** M
  - **Description:**
    - Add `db_core::booking::listing_locations(pool, &[booking_id]) -> HashMap<Uuid, ListingLocation>`, using the spec §2.1 join. The status filter is a lowercase allowlist in SQL, and soft-deleted listings are included.
    - `BookingResponse` gains `listing_location`.
    - `get_booking_by_id` and `get_user_bookings` fill it. There's at most one query, and none when no booking qualifies.
    - Update the `BookingResponse` literals in `api_core/src/models.rs:173` (`map_booking_to_response`).
  - **Files Touched:**
    - `db_core/src/booking/{queries,mod}.rs`
    - `db_core/src/booking/listing_location_tests.rs`
    - `common/src/models.rs`
    - `app_api/api_core/src/models.rs`
    - `app_api/booking_api/src/apis/{bookings,mod}.rs`
    - `app_api/booking_api/src/apis/listing_location_api_tests.rs`
    - `.sqlx/*`
  - **Acceptance Criteria:**
    - [ ] Matrix: each of the four `booking_status` enum values (`pending`, `confirmed`, `completed`, `cancelled`) against each caller (guest, host, admin, another guest → 404), on both endpoints. The location is present only for `confirmed` and `completed`, and never for another guest. Don't add enum values to make a test pass.
    - [ ] db_core: `listing_locations` returns nothing for `pending` and `cancelled` booking IDs. The SQL names an allowlist, so any value added to the enum later stays excluded.
    - [ ] T7 builds its response from the row, not from the fail-closed mapper.
    - [ ] A confirmed-then-cancelled booking returns `None`. A soft-deleted listing with a confirmed booking returns the location. A missing coordinate returns `None`.
    - [ ] The list endpoint makes at most one locations query (captured span count, using the `stay_total_tests.rs:351` pattern), and none when no booking qualifies.
    - [ ] Update, request, enquiry and change responses don't include the field. A confirmed-booking notification payload contains no coordinates (existing `request_notification_tests` pattern).
    - [ ] `! rg -q "f(32|64)" app_api/booking_api/` passes.
  - **Verification Command:** `devenv shell -- cargo test -p db_core listing_location_tests && devenv shell -- cargo test -p booking_api listing_location_api_tests && ! rg -q "f(32|64)" app_api/booking_api/`
  - **Dependencies:** Task 2

- [ ] **Task 21: `/bookings` location block**
  - **Scope:** S
  - **Description:** A card with `listing_location` renders the spec §1.8 block with `map_container(booking, exact point, Exact)`. Other cards are unchanged. A map more than two viewports away is removed, and created again on return, through the T15 init guard.
  - **Files Touched:**
    - `web_ui_guest/src/pages/bookings.rs` (inline `mod bookings_location_tests`)
    - `web_ui_common/src/assets/map.js`
    - `playwright/e2e/ui-js/map-booking.spec.ts`
  - **Acceptance Criteria:**
    - [ ] The block appears only with `listing_location`, carrying exactly that point. Other cards render as they do today.
    - [ ] With three confirmed cards, at most two maps are live while scrolling, and fast scrolling leaks no context (ui-js).
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_guest bookings_location_tests && devenv shell -- npx playwright test --config=playwright/playwright.config.ts --project=ui-js-chromium map-booking`
  - **Dependencies:** Task 15, Task 20

- [ ] **Task 22: Admin pin picker**
  - **Scope:** M
  - **Description:**
    - Add `map_container(picker, point, Exact)` next to the inputs on create and edit.
    - The point comes from the T8 prefill, or from the posted values when the form is shown again after a validation error.
    - The `picker` kind in `map.js` behaves as spec §1.9 describes.
  - **Files Touched:**
    - `web_ui_admin/src/pages/listings/form.rs` (inline `mod pin_picker_tests`)
    - `web_ui_common/src/assets/map.js`
    - `playwright/e2e/ui-js/map-picker.spec.ts`
  - **Acceptance Criteria:**
    - [ ] Dragging updates both inputs to 6 decimal places, and typing moves the marker. An empty form has no marker until the first click. An out-of-bounds point shows "Outside the mapped area".
    - [ ] The container's point is the prefill value, or the posted value on a validation error. It's never the public detail's.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_admin pin_picker_tests && devenv shell -- npx playwright test --config=playwright/playwright.config.ts --project=ui-js-chromium map-picker`
  - **Dependencies:** Task 8, Task 15

### Phase 5: Finish

- [ ] **Task 23: Documentation and egress alert**
  - **Scope:** S
  - **Description:**
    - `docs/ENVIRONMENT.md`: `LOCATION_OFFSET_KEY` (32 bytes, never rotated, how to generate it), `MAP_TILES_BASE_URL`, `MAP_TILES_VERSION_{DEV,PROD}` (a build date, plus updating and rolling back) and `INFRA_GCS_MAP_TILES_BUCKET_PREFIX`.
    - `docs/ARCHITECTURE.md`: the map data flow and the redaction boundary, including the fail-closed mappers.
    - Spec §5 accepted risks: link to them.
    - Ask the originator to create a GCS egress budget alert for the tile buckets by hand. It needs billing permissions, so record it as a checklist item.
  - **Files Touched:**
    - `docs/ENVIRONMENT.md`
    - `docs/ARCHITECTURE.md`
  - **Acceptance Criteria:**
    - [ ] Each of the 5 names is documented, with who sets it and what happens when it's unset.
    - [ ] The rotation warning is present.
    - [ ] The budget alert is confirmed by the originator, or recorded as pending.
  - **Verification Command:** `for v in LOCATION_OFFSET_KEY MAP_TILES_BASE_URL MAP_TILES_VERSION_DEV MAP_TILES_VERSION_PROD INFRA_GCS_MAP_TILES_BUCKET_PREFIX; do grep -q "$v" docs/ENVIRONMENT.md || { echo "missing $v"; exit 1; }; done && grep -qi "never rotate\|don't rotate\|do not rotate" docs/ENVIRONMENT.md`
  - **Dependencies:** Task 14, Task 22

- [ ] **Task 24: Playwright end-to-end flows**
  - **Scope:** M
  - **Description:** Guest and admin specs that cover spec §6's E2E list, against the local seeded tiles (T12) and the T19 seed coordinates, with the CSP and console checks from the cross-cutting rules.
  - **Files Touched:**
    - `playwright/e2e/guest-portal/listings-map.spec.ts` (extend)
    - `playwright/e2e/guest-portal/listing-detail-map.spec.ts`
    - `playwright/e2e/guest-portal/bookings-location.spec.ts`
    - `playwright/e2e/admin-portal/listing-pin-picker.spec.ts`
  - **Acceptance Criteria:**
    - [ ] Desktop `/listings`:
      - the map loads lazily;
      - the number of badges equals the number of cards;
      - a badge opens its card and the link works;
      - the keyboard path works;
      - the attribution is visible.
    - [ ] Mobile `/listings`: there are **no map requests before the toggle**, the toggle works, and `?view=map` survives a reload.
    - [ ] The HTML of the listing page **and** of `/listings` never contains an Approximate seed listing's stored coordinates.
    - [ ] Listing page: the map loads only after scrolling. Approximate shows a circle and Exact shows a pin. With `javaScriptEnabled: false`, the location text is shown and the `<noscript>` note appears.
    - [ ] `/bookings`: a confirmed booking shows the exact pin, and a pending one shows none.
    - [ ] Admin: drag the pin, save, reload, and the stored values show. The privacy radio persists.
    - [ ] Zero CSP violations, `Refused to` messages or failed map requests across all of the above.
  - **Verification Command:** `devenv shell -- npm run test:e2e:guest -- --grep "map|location" && devenv shell -- npm run test:e2e:admin -- --grep "pin picker"`
  - **Dependencies:** Task 16, Task 19, Task 21, Task 22

- [ ] **Task 25: Production version and full gates**
  - **Scope:** S
  - **Description:**
    - Run each `CONSTRAINTS.md` §2 gate as its own command, the way plan-101's Task 22 did. The panic and env-var checks already fail on the baseline, so check only the lines this branch adds. `missing_docs` has baseline errors in `common`, so run it as a warning and compare against the lines this branch adds. Report any baseline failures to the originator.
    - Walk the spec's acceptance criteria and tick each one with evidence.
    - Before the merge to `main`, set `MAP_TILES_VERSION_PROD` to the dev version, so prod copies the verified dev folder.
  - **Acceptance Criteria:**
    - [ ] fmt, clippy `-D warnings`, `sqlx prepare --check`, the backend and web UI tests and `cargo audit` pass.
    - [ ] No `.unwrap()`, `.expect()` or raw `env::var` literal in the lines this branch adds to production code.
    - [ ] `missing_docs` reports nothing on the items this branch adds.
    - [ ] The float gate passes.
    - [ ] Each of spec acceptance criteria 1–14 is ticked.
    - [ ] The local sanity check, measured with `time`, stays within 90s.
    - [ ] CI-only evidence is recorded as run URLs in the plan: T3's guard, T11's concurrent runs and cancel-mid-upload, T14's portal-only push and the failed-setup block.
    - [ ] Inline `#[cfg(test)]` modules added in page files use `match` or `panic!`, not `.unwrap()`, because the added-lines check doesn't exclude them.
  - **Verification Command:**
    ```bash
    set -o pipefail
    cargo fmt --check \
      && SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features -- -D warnings \
      && ! rg -q "f(32|64)" common/src/pricing.rs app_api/booking_api/ \
      && ! git diff 9a4da641 -U0 -- app_api common db_core web_client/src web_ui_common/src web_ui_guest/src web_ui_admin/src ':!*test*' | rg '^\+.*(\.(unwrap|expect)\(|env::var\("[A-Z0-9_]+"\))' \
      && devenv shell -- cargo test --workspace --exclude web_client --exclude web_ui_common --exclude web_ui_guest --exclude web_ui_admin --exclude protoproj \
      && SQLX_OFFLINE=true cargo test -p web_client -p web_ui_common -p web_ui_guest -p web_ui_admin \
      && devenv shell -- cargo sqlx prepare --workspace --check -- --all-targets \
      && cargo audit
    SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features --message-format=short -- -W missing_docs   # compare against this branch's added items
    ```
  - **Dependencies:** Task 23, Task 24
