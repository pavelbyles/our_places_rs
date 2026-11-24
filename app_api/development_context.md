# App API Development Context

## Overview

The `app_api` directory contains the backend services, built with **Actix-web** on Tokio, plus one Cloud Run Job. Services never depend on each other: they share code only through `api_core`, `db_core` and `common`. See [`docs/ARCHITECTURE.md`](../docs/ARCHITECTURE.md) for the full picture.

## Services

| Crate               | Kind            | Responsibility                                                                                                                                                                                                                                    |
| :------------------ | :-------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `api_core`          | lib             | Shared Actix pieces: `ApiError`, JWT `Claims` extractor, startup, actuator/metrics, tracing, settings, email publisher. `calendar/`: the SSRF-guarded feed fetcher and the sync orchestration (`sync_feeds`) shared by `listing_api` and the job. |
| `listing_api`       | service (:8082) | Listings, search, price overrides, images (V4 signed URLs). Server-side ownership checks (`authz`). Calendar sync routes: imported feeds, "Sync now", blocked dates, conflicts, overview, and the `.ics` export.                                  |
| `booking_api`       | service (:8081) | Holds and bookings, availability, requests to book, change requests, payouts, tax documents, cron sweeps.                                                                                                                                         |
| `user_api`          | service (:8083) | Registration, login (JWT), verification, profiles, admin user management.                                                                                                                                                                         |
| `email_worker`      | service (:8080) | Pub/Sub push handler: renders and sends `email_outbox` rows (calendar emails in `calendar_emails.rs`).                                                                                                                                            |
| `image_worker`      | service (:8084) | Pub/Sub push handler: resizes uploads to WebP variants.                                                                                                                                                                                           |
| `calendar_sync_job` | Cloud Run Job   | Scheduled calendar sync (#58): claims due feeds, syncs them, releases unstarted claims at its 9-minute deadline, republishes stuck `calendar_*` emails. Exits non-zero only for config or database failures.                                      |

## Shared Dependencies

- **db_core**: the only crate that runs SQL. Calendar storage lives in `db_core::calendar` (`feeds`, `blocks`, `reconcile`, `conflicts`, `export`, `outbox`).
- **common**: domain types and pure logic. `common::calendar` holds the `.ics` parser and writer, feed URL validation and IP blocklist, conflict rules and the calendar DTOs.

## Database

- **PostgreSQL** (devenv-managed locally on :5432; Cloud SQL in the cloud).
- **SQLx** compile-time checked queries. After changing a query, run `devenv shell -- cargo sqlx prepare --workspace -- --all-targets` and commit `.sqlx/`.
- Migrations live in `db_core/migrations/` and are immutable once applied upstream. Create new ones with `sqlx migrate add <name>`.

## Running a Service

Locally, start everything from the repo root in the devenv shell:

```bash
apis-start          # db, migrations, Pub/Sub + GCS emulators, the five services (detached)
devenv up -d web_ui_guest web_ui_admin
```

Or run one service by hand: `cd app_api/<service> && ./run_local.sh`.

The `devenv` process daemon keeps the environment it started with. After adding `env.*` to `devenv.nix`, restart the affected service by hand (or restart the daemon) so it sees the new variables.

Run the calendar job once: `cd app_api/calendar_sync_job && ./run_local.sh`. Start/stop a background sync loop in devenv: `calsync-start` (stop with `calsync-stop`). On a deployed branch: `gcloud run jobs execute calendar-sync-<branch> --region us-central1 --wait`.

Environment variables for every service are listed in [`docs/ENVIRONMENT.md`](../docs/ENVIRONMENT.md).

## API Structure & Conventions

- **Handlers → db_core**: handlers validate, authorize, then call `db_core` functions; domain rules stay pure in `common`.
- **Auth**: take `Claims` as the first handler argument, so a missing or invalid JWT is a 401 before any lookup. Listing writes call `listing_api::authz::require_listing_manager` (404 for a missing listing, 403 for a non-owner, admins always allowed).
- **Errors**: return `Result<impl Responder, ApiError>`. Business refusals use explicit `{code, message}` bodies (`DbError` variants, or `ApiError::Unprocessable` for 422). Every 5xx is logged at `error`.
- **Tracing**: instrument handlers and db functions (`#[tracing::instrument(skip_all, fields(...))]`). Spawned tasks should carry the caller's span and subscriber (`.instrument(...)`, `.with_current_subscriber()`).
- **No panics or floats**: no `unwrap`/`expect` outside tests; money is `rust_decimal::Decimal`.

## Calendar Sync (#58) Rules

- **Locks**: listing → feed, always. Never hold a transaction across an HTTP fetch. Reconciles and manual blocks use `SET LOCAL lock_timeout = '5s'`; a lock timeout means "deferred", not failure. Liveness checks use `clock_timestamp()`.
- **Availability**: blocks count in all seven availability paths. Imported blocks are widened by the turnover gap, manual ones aren't. Keep `booking::availability_parity_tests` passing when touching any of them.
- **Feed URLs are secrets**: never log them. Log `feed_id` and the host (`feed_url::redact`); call `.without_url()` on every `reqwest::Error`.
- **Local-only settings**: `EA__CALENDAR_SYNC__ALLOW_PRIVATE_HOSTS` and `EA__CALENDAR_SYNC__EXTRA_ROOT_CA_PATH` come from env only (never `config/*.toml`). Services refuse to start with them on Cloud Run.

## Testing

- Targeted while iterating: `devenv shell -- cargo test -p <crate> <module>::<tests>`.
- Database tests use `sqlx_db_tester::TestPg` (a fresh database per test).
- Feed fetch tests use the local https server in `api_core::calendar::test_server` (the `test-support` feature for other crates), never real DNS or real channels.
- E2E: Playwright's global setup serves `playwright/e2e/fixtures/ical/*.ics` over https on :8443, using the CA from devenv's `ical-fixture-ca`.
