# Quality Constraints Contract — Our Places

Last updated: 2026-10-12 by @pav

This contract defines the explicit, machine-checkable quality bar for the `our_places_rs` monorepo. Agents and contributors must satisfy these constraints before opening or merging pull requests. This document must not be weakened to make failing changes pass.

---

## 1. Universal Floor (Zero Setup, Non-Negotiable)

- **Zero Suppression Pragmas**: No new `#[allow(...)]`, `@ts-ignore`, `eslint-disable`, `# noqa`, or `// skipcq:` attributes without an entry in the Exceptions log.
- **Zero Panic Primitives in Production**: Never call `.unwrap()` or `.expect()` in HTTP handlers, domain services, or shared libraries (`app_api/`, `common/`, `db_core/`, `web_client/`, `web_ui_*/`). Always use monadic `Result<T, E>` and `Option<T>` combinator chains (`.and_then()`, `.map()`, `?`).
- **Zero Floats in Financial Paths**: NEVER use `f32` or `f64` for money, rates, or statutory taxes. Always use `rust_decimal::Decimal`.
- **Zero Double-Booking Risk**: Availability checks and hold placements MUST acquire PostgreSQL row-level locks (`SELECT ... FOR UPDATE`). Volatile in-memory caches are forbidden for date holds.
- **Zero Direct Image Streaming**: Never stream raw image bytes through Actix HTTP servers. Always issue GCP V4 signed URLs for direct client-to-GCS uploads.
- **Zero Async Worker Blocking**: Never execute blocking synchronous I/O or heavy CPU-bound computations on Tokio async worker threads. Always delegate to `tokio::task::spawn_blocking`.
- **Zero Migration Alterations**: Applied database migration files in `db_core/migrations/` are strictly immutable; modifying applied scripts invalidates SQLx checksums.
- **Zero Raw String Literals in `env::var` (`RS-W1015`)**: Disallow raw string literals in `std::env::var("KEY")`. Declare a `static KEY_ENV: &str = "KEY";` constant or use strongly-typed config structs.
- **Zero Hardcoded `/tmp` Paths (`RS-S1003`)**: Never hardcode `"/tmp"` or `"/tmp/..."`. Always use `std::env::temp_dir().join(...)`.
- **Zero Undocumented Public APIs (`RS-D1001`)**: Every new public struct, enum, function, public field, and enum variant must have a `///` doc comment.
- **Zero Unfinished Stubs**: No `todo!()`, `unimplemented!()`, or hollow placeholder blocks in production code paths.
- **Zero Silenced Tests**: No test deletions, `#[ignore]`, or `.skip` markers without an explicit GitHub issue link and rationale.
- **Zero Secrets in Source**: No API tokens, JWT secrets, database credentials, or private keys committed to version control.
- **Immutable Gate**: This document cannot be relaxed to make a failing PR pass.

---

## 2. Enforced Dimensions

| Dimension | Rule | Checked By | Stage |
| :--- | :--- | :--- | :--- |
| **Formatting** | Rust code standard format | `cargo fmt --check` | Fast loop (<5s) |
| **Compilation** | Offline compile-time SQLx verification | `SQLX_OFFLINE=true cargo check --workspace --exclude protoproj --all-features --tests` | Fast loop (<10s) |
| **Strict Linting** | Zero Clippy warnings across all workspace members | `SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features -- -D warnings` | Task completion (<30s) |
| **Financial Precision** | Zero floats in pricing and booking logic | `! rg -q "f(32\|64)" common/src/pricing.rs app_api/booking_api/` | Task completion (<5s) |
| **Panic Prevention** | Zero `.unwrap()` / `.expect()` in production crates | `! rg -q "\.(unwrap\|expect)\(" app_api/ common/ db_core/ web_client/src web_ui_common/src web_ui_guest/src web_ui_admin/src` | Task completion (<5s) |
| **Env Var Hygiene** | No raw string literals in `env::var` calls | `! rg -q 'env::var\("[A-Z0-9_]+"\)' app_api/ common/ db_core/` | Task completion (<5s) |
| **Doc Coverage** | 100% public structs, fields, enums, variants documented | `SQLX_OFFLINE=true cargo clippy --workspace --all-features -- -D missing_docs` | Task completion (<30s) |
| **SQLx Sync** | Offline metadata matches database queries | `cargo sqlx prepare --workspace --check` | Pre-merge & CI |
| **Backend Tests** | 100% pass rate across API & DB test targets | `cargo test --verbose --workspace --exclude web_client --exclude web_ui_common --exclude web_ui_guest --exclude web_ui_admin --exclude protoproj` | Task completion (<60s) |
| **Web UI Tests** | 100% pass rate across the Topcoat UI portals and their shared crates | `cargo test -p web_client -p web_ui_common -p web_ui_guest -p web_ui_admin --verbose` | Task completion (<60s) |
| **Dependency CVEs** | Zero unmitigated vulnerabilities | `cargo audit` (abiding by `.cargo/audit.toml`) | Pre-merge & CI |
| **Static Hygiene** | DeepSource / SonarQube quality gate | DeepSource & SonarCloud CI scanners | CI |

---

## 3. Measured Baselines (Ratchets)

Metrics that are tracked continuously and must not degrade:

| Metric | Target / Baseline | Invariant | Enforcement |
| :--- | :--- | :--- | :--- |
| **Cloud Run Cold Start (p50)** | $< 300\text{ms}$ | Must not degrade | Performance audit (`/cloudrun-perf-audit`) |
| **Cloud Run Cold Start (p95)** | $< 1\text{s}$ | Must not degrade | Performance audit (`/cloudrun-perf-audit`) |
| **Reservation Hold TTL** | 15 minutes (`expires_at`) | Invariant | Booking audit (`/audit-booking-flow`) |
| **Local Sanity Check Runtime** | $\le 90\text{s}$ total | Must not exceed 120s | Developer workflow feedback |

---

## 4. Exceptions Log

Every exception must have an approved tracking ID, concrete reason, designated owner, and strict expiration date. Expired exceptions automatically block CI.

| ID | Rule | Target File / Module | Justification | Owner | Expiration |
| :--- | :--- | :--- | :--- | :--- | :--- |
| EX-001 | `clippy::too_many_arguments` | `app_api/listing_api/src/handlers/search.rs` | Refactoring into `SearchQuery` DTO in progress | @pav | 2026-11-30 |
