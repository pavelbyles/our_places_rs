# Implementation Plan: Topcoat 0.11 → 0.12 (#120)

**Spec Reference:** `docs/specs/spec-120-feat-update-ui-fwk.md`
**Baseline Commit:** `35b60fa0`

## Execution notes

- **Why the slices follow crates:** a feature-by-feature split is not possible here. Once Task 3 bumps the version, nothing compiles until each crate is migrated, so the plan works through the dependency order `web_client` → `web_ui_common` → `web_ui_guest` ∥ `web_ui_admin`. Each portal is still finished end to end (router → params → components → tests) before it is marked done.
- **Gate for a partial step:** a crate only compiles cleanly once all of its tasks are done. Until then, a task passes when the compiler reports **no errors in that task's files**:
  ```bash
  # usage: errs <crate> <path-regex>  → must print 0
  errs() { SQLX_OFFLINE=true cargo check -p "$1" --all-targets --message-format=short 2>&1 | grep -E "^$2" | grep -c 'error'; }
  ```
- **Mechanical batches:** batches of 8–10 files are labelled S even though they go over the 5-file guideline. Each edit is the same one-line signature change (`cx: &Cx` first), which carries little risk.
- **Commits:** one commit per task, with `SQLX_OFFLINE=true` for all builds. Push to `origin/feat/120-update-ui-fwk`.

## Dependency graph

```
T1 spike ─┬─► T3 bump ─► T4 web_client ─► T5 common infra ─┬─► T6a ─┐
T2 baseline┘                                               ├─► T6b ─┼─► T7 common tests ─┬─► T8 guest router ─► T9 guest params ─► T10a ─► T10b ─┐
                                                           ├─► T6c ─┤                    └─► T11 admin router ─► T12a ─► T12b ─► T13a ─► T13b ─┼─► T14 fmt ─► T15 verify ─► T16 e2e
                                                           └─► T6d ─┘                                                                        │
T1 ─► T17 tooling/docs (parallel) ───────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

## Tasks

### Phase 0: Risk first

- [x] **Task 1: Spike to verify the 0.12 APIs the release notes don't cover**
  - **Scope:** XS (no production code)
  - **Description:** Build a scratch crate in `/tmp` against `topcoat = "0.12.0"` and answer the spec's open items. Record the answers in a new spec section `## 2a. API verification (Q1–Q9)`, one `Q<n>:` line each, and update R1 and R6.
    1. Does `RouterBuilder::assets(AssetBundle)` still exist?
    2. Is the `Layer` trait signature `handle(&self, cx, body, next) -> LayerFuture` unchanged?
    3. Is `topcoat::session::SessionState` still in context when there is only `CookieLayer`? Do `token_hash` and `refresh` still behave the same?
    4. Does `CookieTokenStore::new().name(..)` still exist, and does it work inside `SessionTokenStore::new(..)`?
    5. Does `Cx::default()` exist, or is it `Cx::new()`?
    6. Is the `topcoat_asset::{BundlerConfig, Bundler}` API unchanged?
    7. What is 0.12's `rust-version` (MSRV)?
    8. Do the feature names still exist: `htmx`, `tailwind`, `session`, `ui`, `icon-iconify`, `font-fontsource`, `asset`, `bundler`?
    9. Does layer nesting order differ between `.layer(CookieLayer)` and the custom layers?
  - **Files Touched:** `docs/specs/spec-120-feat-update-ui-fwk.md`
  - **Acceptance Criteria:**
    - [ ] All 9 questions are answered in the spec with a link to the docs or source.
    - [ ] Any API that is gone has a replacement noted in the matching task below.
  - **Verification Command:** `grep -cE '^- \*\*Q[1-9]:' docs/specs/spec-120-feat-update-ui-fwk.md` → `9`
  - **Dependencies:** None

- [x] **Task 2: Capture cookie and header behaviour on 0.11 as a baseline**
  - **Scope:** XS
  - **Description:** With both portals on 0.11, record the `Set-Cookie` lines (session, currency, theme) and the response security headers (CSP, HSTS and others) for: anonymous `/`, login POST, an authenticated page, and logout. Do this for guest and admin. Redact cookie values.
  - **Files Touched:** `tasks/baseline-120/{guest,admin}-headers.txt` (new), `tasks/baseline-120/capture.sh` (new)
  - **Result (2026-11-10, devenv, topcoat 0.11.0):**
    - Session cookies are `__Host-op_guest_session` and `__Host-op_admin_session`, with `HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age=2592000`. Sessions use sliding refresh: the cookie is re-issued on every authenticated GET.
    - `op_currency` is `HttpOnly; SameSite=Lax; Path=/; Max-Age=31536000` and is set on guest login and by `/currency/{code}`.
    - `op_theme` is set client-side by `ui.js`, so there is nothing server-side to compare.
    - The installed `topcoat` CLI is **0.10.0**, while the library is 0.11. `topcoat dev` already warns about the mismatch, so Task 3 must install the 0.12 CLI.
  - **Acceptance Criteria:**
    - [ ] Both files exist and hold the cookie names, attributes (Path, Secure, HttpOnly, SameSite, Max-Age) and the full CSP.
  - **Verification Command:** `test -s tasks/baseline-120/guest-headers.txt && test -s tasks/baseline-120/admin-headers.txt`
  - **Dependencies:** None (must be done **before** Task 3)

### Phase 1: Foundation

- [x] **Task 3: Bump Topcoat to 0.12 and remove the 0.10 build-dependency pins**
  - **Scope:** S
  - **Description:** In the workspace, set `topcoat` and `topcoat-asset` to `0.12.0`. Change the `[build-dependencies]` in all three UI crates from `0.10.0` to `{ workspace = true, default-features = false, features = [...] }`. Apply any feature renames found in Task 1. Run `cargo update -p topcoat -p topcoat-asset`. Install the matching CLI with `cargo install topcoat-cli@0.12 --locked`, after confirming with the user because it is a global install (the current CLI is 0.10.0).
  - **Files Touched:** `Cargo.toml`, `Cargo.lock`, `web_ui_common/Cargo.toml`, `web_ui_guest/Cargo.toml`, `web_ui_admin/Cargo.toml`
  - **Acceptance Criteria:**
    - [ ] Only one Topcoat version resolves.
    - [ ] Non-UI crates still build: `cargo check -p common -p db_core -p api_core`.
  - **Result:** Resolved to 0.12.1, the latest patch, which has no breaking changes. The build-deps use an explicit `version = "0.12.1", default-features = false` instead of `workspace = true`, because with `workspace = true` the workspace features would be added to the build scripts. The CLI is 0.12.1 in `~/.cargo/bin`, replacing 0.10.0. devenv puts its own `cargo-install/bin` after `~/.cargo/bin` on PATH.
  - **Verification Command:** `cargo tree -i topcoat@0.12.1 >/dev/null && ! cargo tree -i topcoat@0.10.0 2>/dev/null && ! cargo tree -i topcoat@0.11.0 2>/dev/null`
  - **Dependencies:** Task 1

- [x] **Task 4: Move `web_client` to the single context API**
  - **Scope:** XS
  - **Description:** Change `try_app_context` to `try_context` (`client.rs:453`). **Delete** the `try_request_context::<SessionState>` guard (`session.rs:112`), because `SessionState` is crate-private in 0.12 (spec §2a Q3). Update the doc comments that mention `app_context`. `#[memoize]` already takes `cx` first, so it needs no change.
  - **Files Touched:** `web_client/src/client.rs`, `web_client/src/session.rs`
  - **Result:** Deleting the guard on its own would let a bare `Cx` panic inside `topcoat_cookie::cookies`. I replaced it with `topcoat::cookie::try_cookies(cx).ok_or(Unauthenticated)?` and added the regression test `tests/session.rs::guards_without_cookie_layer_are_unauthenticated`, which was red (it panicked) and is now green.
  - **Acceptance Criteria:**
    - [ ] `app_context` and `request_context` no longer appear in `web_client/`.
    - [ ] The session guard tests pass. If `SessionState` behaves differently (Task 1, Q3), this task includes the fix.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_client`
  - **Dependencies:** Task 3

### Phase 2: `web_ui_common`

- [x] **Task 5: Common infrastructure (shells, layers, bundler)**
  - **Scope:** S
  - **Description:** Rename the `__cx` parameter to `cx` in `admin_shell` and `guest_shell`, and add `cx` first to the 2 shell components in each file. Fix the `Layer` impls and `topcoat_bundler` if Task 1 found changes. Update the `theme.rs:123` doc comment from `.cookies()` to `CookieLayer`.
  - **Files Touched:** `web_ui_common/src/shell_guest.rs`, `web_ui_common/src/shell_admin.rs`, `web_ui_common/src/security_headers.rs`, `web_ui_common/src/request_tracing.rs`, `web_ui_common/src/bin/topcoat_bundler.rs`, `web_ui_common/src/theme.rs`
  - **Acceptance Criteria:**
    - [ ] No compile errors in these files.
    - [ ] `__cx` no longer appears in `shell_*.rs`.
  - **Verification Command:** `errs web_ui_common 'web_ui_common/src/(shell_|security_headers|request_tracing|bin/|theme)'` → `0`
  - **Dependencies:** Task 4

- [x] **Task 6a: UI primitives batch A (`cx` first on 33 components)**
  - **Scope:** S (mechanical)
  - **Files Touched:** `web_ui_common/src/components/ui/{accordion,alert,alert_dialog,avatar,badge,breadcrumb,button,card,checkbox,dialog}.rs`
  - **Acceptance Criteria:**
    - [ ] Every `#[component]` in these files has `cx: &Cx` as its first parameter.
    - [ ] Missing prop-to-child commas are fixed where the compiler flags them.
  - **Verification Command:** `errs web_ui_common 'web_ui_common/src/components/ui/(accordion|alert|alert_dialog|avatar|badge|breadcrumb|button|card|checkbox|dialog)\.rs'` → `0`
  - **Dependencies:** Task 5

- [x] **Task 6b: UI primitives batch B (`cx` first on 39 components)**
  - **Scope:** S (mechanical)
  - **Files Touched:** `web_ui_common/src/components/ui/{dropdown_menu,field,input,label,pagination,progress,radio_group,separator,sheet}.rs`
  - **Acceptance Criteria:** Same as Task 6a.
  - **Verification Command:** `errs web_ui_common 'web_ui_common/src/components/ui/(dropdown_menu|field|input|label|pagination|progress|radio_group|separator|sheet)\.rs'` → `0`
  - **Dependencies:** Task 5

- [x] **Task 6c: UI primitives batch C (`cx` first on 43 components; sidebar has 23)**
  - **Scope:** S (mechanical)
  - **Files Touched:** `web_ui_common/src/components/ui/{sidebar,skeleton,spinner,switch,table,tabs,textarea,toggle,tooltip}.rs`
  - **Acceptance Criteria:** Same as Task 6a.
  - **Verification Command:** `errs web_ui_common 'web_ui_common/src/components/ui/(sidebar|skeleton|spinner|switch|table|tabs|textarea|toggle|tooltip)\.rs'` → `0`
  - **Dependencies:** Task 5

- [x] **Task 6d: Domain and app components (`cx` first on 21 components)**
  - **Scope:** S (mechanical)
  - **Description:** In `currency_selector.rs:52,193`, `cx` is currently a documented prop. Move it to the first parameter and remove its `///` prop doc.
  - **Files Touched:** `web_ui_common/src/components/{chat,date_range_calendar,framed_image,hero,navbar,star_rating,stat_tile,steps,svg_icon,toast}.rs`, `web_ui_common/src/components/domain/{currency_selector,listing_card,price_breakdown,responsive_image,tax_document_view,villa_card}.rs`
  - **Acceptance Criteria:** Same as Task 6a.
  - **Verification Command:** `errs web_ui_common 'web_ui_common/src/components/'` → `0`
  - **Dependencies:** Tasks 6a, 6b, 6c (the domain components call the primitives)

- [x] **Task 7: Migrate the `web_ui_common` tests and pass the crate gate**
  - **Scope:** S
  - **Description:** Change `let __cx = &Cx::default()` to `let cx = …` (or to whatever Task 1 Q5 says to use) in the test files and in the in-file `mod tests` of `currency_selector`, `listing_card` and `villa_card`.
  - **Files Touched:** `web_ui_common/tests/{components,date_range_calendar,domain_parity,money_parity,shells}.rs`, `web_ui_common/src/components/domain/{currency_selector,listing_card,villa_card}.rs`
  - **Acceptance Criteria:**
    - [ ] `__cx` no longer appears in `web_ui_common/`.
    - [ ] All existing tests pass unchanged. Do not change any rendered-HTML assertion unless the markup diff is understood and recorded in the commit message.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_common`
  - **Dependencies:** Task 6d

### Phase 3: `web_ui_guest` (can run in parallel with Phase 4)

- [x] **Task 8: Guest router and session wiring**
  - **Scope:** S (high risk: R1, R3)
  - **Description:** Make these router changes, keeping the relative order of `SecurityHeaders` and `RequestTracing` (Task 1 Q9):

    | Before | After |
    |---|---|
    | `.app_context(x)` | `.with(x)` |
    | `.cookies()` | `.layer(CookieLayer::new())` |
    | `SessionConfig` and `.sessions(..)` | `.with(SessionTokenStore::new(CookieTokenStore::new().name("op_guest_session")))` |

    In `support.rs:131`, change `try_app_context` to `try_context`.
  - **Files Touched:** `web_ui_guest/src/main.rs`, `web_ui_guest/src/support.rs`
  - **Acceptance Criteria:**
    - [ ] No compile errors in these files.
    - [ ] The cookie name is still `op_guest_session`.
    - [ ] `SessionConfig`, `.cookies()` and `app_context` no longer appear in `web_ui_guest/`.
  - **Verification Command:** `errs web_ui_guest 'web_ui_guest/src/(main|support)\.rs'` → `0`
  - **Dependencies:** Task 7

- [x] **Task 9: Guest path-parameter accessors**
  - **Scope:** S (mechanical, 10 files)
  - **Description:** Change `path_param::<Slug|Id|Code|…>(cx)` to `slug(cx)`, `id(cx)`, `code(cx)` and so on. Where a local binding hides a later accessor call (R5), rename the local (e.g. `let listing_id = id(cx)`). Keep the current `&str` handling; typed params are out of scope.
  - **Files Touched:** `web_ui_guest/src/pages/{booking_change,bookings,calendar_export,checkout,currency,documents,enquiries,listing_detail,messages,requests}.rs`
  - **Acceptance Criteria:**
    - [ ] `path_param::<` no longer appears in `web_ui_guest/`.
    - [ ] No compile errors from path params in these files.
  - **Verification Command:** `! grep -rn 'path_param::<' web_ui_guest && errs web_ui_guest 'web_ui_guest/src/pages/' | grep -v cx`
    - The second part is a manual review: no remaining errors should be about params.
  - **Dependencies:** Task 8

- [x] **Task 10a: Guest components batch A (`cx` first on 9 components)**
  - **Scope:** S (mechanical)
  - **Files Touched:** `web_ui_guest/src/pages/{auth,booking_change,bookings,checkout,enquiries,home,listing_detail}.rs`
  - **Acceptance Criteria:** `cx: &Cx` is first on every component in these files.
  - **Verification Command:** `errs web_ui_guest 'web_ui_guest/src/pages/(auth|booking_change|bookings|checkout|enquiries|home|listing_detail)\.rs'` → `0`
  - **Dependencies:** Task 9

- [ ] **Task 10b: Guest components batch B (13 components), tests, and the crate gate**
  - **Scope:** S
  - **Description:** Put `cx` first on the remaining components. Change `__cx` to `cx` in the test modules in `home.rs` and `listings.rs`.
  - **Files Touched:** `web_ui_guest/src/pages/{listings,messages,profile,register,requests,reviews,verify,home}.rs`
  - **Acceptance Criteria:**
    - [ ] `__cx` no longer appears in `web_ui_guest/`.
    - [ ] The crate compiles with all targets, and the tests pass.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_guest`
  - **Dependencies:** Task 10a

### Phase 4: `web_ui_admin` (can run in parallel with Phase 3)

- [ ] **Task 11: Admin router and session wiring**
  - **Scope:** S (high risk: R1, R3)
  - **Description:** Make the same router changes as Task 8, keeping the admin session cookie name from the baseline (Task 2). In `support.rs:64`, change `try_app_context` to `try_context`.
  - **Files Touched:** `web_ui_admin/src/main.rs`, `web_ui_admin/src/support.rs`
  - **Acceptance Criteria:** The same as Task 8, applied to the admin crate.
  - **Verification Command:** `errs web_ui_admin 'web_ui_admin/src/(main|support)\.rs'` → `0`
  - **Dependencies:** Task 7

- [ ] **Task 12a: Admin path-parameter accessors, batch A**
  - **Scope:** S (mechanical)
  - **Description:** Same change as Task 9. `listings/calendar.rs` declares `id`, `fid` and `bid` in one module, so check it for clashes with local names.
  - **Files Touched:** `web_ui_admin/src/pages/{aliases,currencies,exchange_rate_history}.rs`, `web_ui_admin/src/pages/bookings/{detail,messages}.rs`, `web_ui_admin/src/pages/listings/{calendar,create,edit}.rs`
  - **Acceptance Criteria:** `path_param::<` no longer appears in these files.
  - **Verification Command:** `! grep -n 'path_param::<' web_ui_admin/src/pages/{aliases,currencies,exchange_rate_history}.rs web_ui_admin/src/pages/bookings/{detail,messages}.rs web_ui_admin/src/pages/listings/{calendar,create,edit}.rs`
  - **Dependencies:** Task 11

- [ ] **Task 12b: Admin path-parameter accessors, batch B**
  - **Scope:** S (mechanical)
  - **Files Touched:** `web_ui_admin/src/pages/{payouts,pricing,promo_codes,requests}.rs`, `web_ui_admin/src/pages/listings/{images,mod}.rs`, `web_ui_admin/src/pages/users/{actions,form}.rs`
  - **Acceptance Criteria:** `path_param::<` no longer appears in `web_ui_admin/`.
  - **Verification Command:** `! grep -rn 'path_param::<' web_ui_admin`
  - **Dependencies:** Task 12a

- [ ] **Task 13a: Admin components batch A (`cx` first on 18 components)**
  - **Scope:** S (mechanical)
  - **Files Touched:** `web_ui_admin/src/pages/{auth,currencies,dashboard,exchange_rate_history,exchange_rates}.rs`, `web_ui_admin/src/pages/bookings/{detail,messages}.rs`, `web_ui_admin/src/pages/listings/calendar.rs`
  - **Acceptance Criteria:** `cx: &Cx` is first on every component in these files.
  - **Verification Command:** `errs web_ui_admin 'web_ui_admin/src/pages/(auth|currencies|dashboard|exchange_rate_history|exchange_rates|bookings/|listings/calendar)'` → `0`
  - **Dependencies:** Task 12b

- [ ] **Task 13b: Admin components batch B (19 components), tests, and the crate gate**
  - **Scope:** S
  - **Description:** Put `cx` first on the remaining components. Change `__cx` to `cx` in `dashboard_tests.rs`, `listings/calendar_tests.rs`, and the test modules in `currencies.rs`, `exchange_rates.rs`, `exchange_rate_history.rs` and `promo_codes.rs`.
  - **Files Touched:** `web_ui_admin/src/pages/{pricing,promo_codes,register,requests,dashboard_tests,currencies,exchange_rates,exchange_rate_history}.rs`, `web_ui_admin/src/pages/listings/{form,images,calendar_tests}.rs`, `web_ui_admin/src/pages/users/form.rs`
  - **Acceptance Criteria:**
    - [ ] `__cx` no longer appears in `web_ui_admin/`.
    - [ ] The crate compiles with all targets, and the tests pass.
  - **Verification Command:** `SQLX_OFFLINE=true cargo test -p web_ui_admin`
  - **Dependencies:** Task 13a

### Phase 5: Integration and verification

- [ ] **Task 14: Run `topcoat fmt` and `cargo fmt` (format-only commit)**
  - **Scope:** XS (large diff, no logic)
  - **Description:** Run `topcoat fmt` in each UI crate using the 0.12 CLI, then run `cargo fmt --all`. Commit with the message "style: topcoat fmt (0.12)" and nothing else in the commit.
  - **Files Touched:** UI crates (formatting only)
  - **Acceptance Criteria:**
    - [ ] `cargo fmt --check` is clean.
    - [ ] `git diff --stat` touches only `*.rs` in the `web_*` crates.
  - **Verification Command:** `cargo fmt --all --check && SQLX_OFFLINE=true cargo check --workspace --all-targets`
  - **Dependencies:** Tasks 10b, 13b

- [ ] **Task 15: Workspace lint and tests, plus checks against the baseline**
  - **Scope:** S
  - **Description:** Run clippy and the full test suite once. Rerun `devenv shell -- tasks/baseline-120/capture.sh <guest|admin> tasks/baseline-120/<kind>-headers.after.txt` on 0.12 and diff it against the baseline. Ignore only the first 2 comment lines. Run the acceptance grep from spec §4.
  - **Files Touched:** `tasks/baseline-120/*-headers.after.txt` (new)
  - **Acceptance Criteria:**
    - [ ] Clippy reports no warnings.
    - [ ] `cargo test --workspace` passes.
    - [ ] Cookie names, cookie attributes and the CSP are identical to the baseline.
    - [ ] There are 0 hits for `app_context|request_context|path_param::<|__cx|SessionConfig|\.cookies\(\)` in the `web_*` crates.
    - [ ] There are no new `.unwrap()` or `.expect()` calls.
  - **Verification Command:** `SQLX_OFFLINE=true cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && diff <(grep -iE 'set-cookie|content-security' tasks/baseline-120/guest-headers.txt) <(grep -iE 'set-cookie|content-security' tasks/baseline-120/guest-headers.after.txt)`
    - Run the same `diff` for the admin files.
  - **Dependencies:** Task 14

- [ ] **Task 16: End-to-end, dev loop and Docker smoke test**
  - **Scope:** S
  - **Description:**
    - Run `devenv up` with the 0.12 `topcoat` CLI and check that `topcoat dev` hot-reloads both portals.
    - Run Playwright on both portals. Cover: login, logout, session persistence across pages, the currency cookie, the theme cookie, listing detail (slug param), the admin listing calendar (`id`/`fid`/`bid`), both CSV exports, image presign, and the guest `.ics` export.
    - Build `Dockerfile.web_ui_guest` and `Dockerfile.web_ui_admin` (this includes the bundler step).
  - **Files Touched:** None (Playwright specs only change if a selector broke)
  - **Acceptance Criteria:**
    - [ ] Playwright passes.
    - [ ] Both Docker images build, and the containers serve `/` with assets loaded.
  - **Verification Command:** `npx playwright test && docker build -f Dockerfile.web_ui_guest . && docker build -f Dockerfile.web_ui_admin .`
  - **Dependencies:** Task 15

### Side track (can start after Task 1)

- [ ] **Task 17: Tooling and agent-skill documentation for 0.12**
  - **Scope:** M
  - **Description:**
    - In `devenv.nix:27-31`, update the toolchain comment and the rustc version to match the MSRV from Task 1.
    - Rewrite the topcoat skill references for 0.12:
      - Merge `context.md` and `app_context.md`.
      - Update `router.md`, `cookie.md`, `session.md`, `mail.md`, `path_param.md`, `component.md`, `view.md`, `asset.md` and `fmt.md`.
      - Update `REFERENCE.md` and `SKILL.md`.
    - Keep `.agents/skills/topcoat` and `.claude/skills/topcoat` in sync, or turn one into a symlink.
  - **Files Touched:** `devenv.nix`, `.agents/skills/topcoat/**`, `.claude/skills/topcoat/**`
  - **Acceptance Criteria:**
    - [ ] The skill docs have no remaining references to `app_context`, `request_context`, `path_param::<`, `cookie!`, `mail!` or `cx =>`.
    - [ ] `devenv shell` starts on the new toolchain.
  - **Verification Command:** `! grep -rnE 'app_context|request_context\(|path_param::<|cookie!|mail!|cx =>' .agents/skills/topcoat .claude/skills/topcoat && devenv shell -- rustc --version`
  - **Dependencies:** Task 1. It must be merged with or before Task 16, so the dev loop docs match.

## Handoff checklist
- [x] Every task has a verification command.
- [x] The dependency graph has no cycles.
- [x] Foundation tasks (bump, `web_client`, common) come before the portals.
- [x] Risk-first: the API spike (T1) and the cookie/CSP baseline (T2) come before any code change, and the session wiring (T8, T11) comes first in each portal.
