# Intent: iCal Calendar Synchronization (External Channel Manager)

## Metadata
- **Author**: Originator (via GitHub Issue #58)
- **Date**: 2026-11-08
- **Status**: Approved (2026-11-08)
- **Target Area**: Fullstack + Infra (`common`, `db_core`, `app_api/listing_api`, new `app_api/calendar_sync_job`, `app_api/email_worker`, `web_client`, `web_ui_admin`, `web_ui_guest`, `.github/workflows`)

---

## 1. Problem Statement & Motivation
Hosts list the same villas on Airbnb and VRBO as well as Our Places. Our Places doesn't know about stays booked on those channels, and they don't know about stays booked here. Today the only protection against a cross-channel double-booking is the host updating every calendar by hand.

Inside Our Places, availability comes only from pending holds, confirmed bookings and live requests to book (`db_core/src/booking/availability.rs`). There's no way to mark dates unavailable for any other reason, so hosts can't block dates for their own use or for maintenance either.

iCal (RFC 5545 `.ics`) feeds are the common format every major channel supports (Airbnb's "Sync calendars": export a link, import other links). Supporting it lets hosts connect channels without a paid channel manager.

---

## 2. Proposed Outcome (The Vision)
1. **Import**: a host (own listings) or the admin (any listing) adds one or more external `.ics` feed URLs to a listing, each with a name (e.g. "Airbnb", "VRBO"). Every event in a feed becomes blocked dates on that listing.
2. **Automatic sync**: a Rust **Cloud Run Job** polls every active feed and reconciles it. In production (`main`), **Cloud Scheduler runs it every 15 minutes**. Non-prod environments (`dev` and feature branches) have no schedule; the job is run by hand there (`gcloud run jobs execute` or "Sync now"). Events that are new get blocked, events that changed get updated, and events that are gone get released. Hosts also get a **"Sync now"** action per listing. Each feed shows its last-synced time and status.
3. **Manual blocks**: hosts and the admin block and unblock date ranges by hand in the admin portal, with an optional private note.
4. **One availability rule**: blocked dates (imported or manual) prevent holds, requests to book, reservation changes and date search, everywhere the existing status set already applies. They also appear as unavailable on the guest calendar.
5. **Export**: a public `GET /listings/:id/calendar.ics` endpoint in `listing_api` returns an RFC 5545 feed of the listing's unavailable dates. It covers confirmed stays, pending holds, manual blocks and blocks imported from other channels. Events are busy-only: `SUMMARY` is source-specific ("Reserved (Our Places)", "Reserved (Manual)", or "Reserved via <feed_name>"), the source is "OurPlaces", and the booking's reservation number (`confirmation_code`) is included. No guest name, email or notes.
6. **Hub model**: because imported blocks are re-exported, each channel only has to import the Our Places feed to see every other channel's bookings.
7. **Conflict alerts**: if an imported event overlaps a stay that is already held or confirmed on Our Places, both are kept. The existing booking is never changed or cancelled. The conflict is flagged in the admin portal and the host is emailed through the existing email pipeline.

---

## 3. Scope Boundaries
### In Scope (MVP)
- A new timestamped migration adding:
  - a generic `listing_blocked_dates` table with a source (`ical_feed` | `manual`), an optional feed reference, the external event `UID`, a `[date_from, date_to)` range and timestamps
  - a `listing_calendar_feed` table holding the name, URL, active flag, last sync time, last status/error and consecutive failure count
- Reconciliation per feed, keyed on the event `UID`: upsert present events and delete blocks whose `UID` has left the feed, in one transaction per feed.
- Fail-safe sync: if a fetch or parse fails, the feed's existing blocks are **kept**, not wiped. The error and failure count are recorded, and repeated failures are surfaced to the host.
- Blocked dates added to `check_availability`, `check_availability_excluding`, `get_unavailable_ranges`, hold creation (under the existing `listing ... FOR UPDATE` lock), booking-request creation, reservation changes and search by dates (#101).
- Feed CRUD, manual block CRUD and "Sync now" endpoints in `listing_api`, enforced server-side: admin for any listing, host for owned listings (`listing.user_id`), otherwise 403.
- The public export endpoint (`text/calendar`), with all-day `VALUE=DATE` events, an exclusive `DTEND`, stable `UID`s and `PRODID` for Our Places.
- A new `calendar_sync_job` Cloud Run Job binary, deployed through the CI workflow with gcloud, not Terraform. This matches how every other service and the hourly booking sweeps are deployed today.
  - `cicd_full_stack.yml`, all branches: build and push the image, then `gcloud run jobs deploy calendar-sync-$SUFFIX`, which creates or updates the job.
  - `cicd_full_stack.yml`, `main` only: create or update a Cloud Scheduler HTTP job, every 15 minutes, that calls the Cloud Run Admin API `jobs/…:run` with an OAuth service-account token. It follows the create-or-update pattern of the existing "Schedule Hourly Booking Sweeps" step.
  - `dev` and feature branches get no Scheduler job. Sync there runs by hand.
  - `cleanup_feature.yml`: delete `calendar-sync-$SUFFIX` together with the branch's services.
- Admin portal pages:
  - a per-listing calendar sync screen with the export link and copy button, the imported feeds with their status, and a "Sync now" button
  - manual blocks on the listing calendar
  - a conflict list
- A conflict alert email sent through the existing `email_outbox` and Pub/Sub path.

### Out of Scope / Non-Goals
- API-level channel integrations (Airbnb or VRBO partner APIs), rate or price sync, or pushing listing content to channels.
- Importing guest details, prices or booking data from external events. Imported events only block dates.
- Automatically cancelling or moving any booking because of a conflict.
- Real-time sync or webhooks. Channels only offer polled iCal.
- A per-guest or personal calendar subscription feed.
- Sub-day (hourly) blocking. Blocks are whole nights, like stays.

---

## 4. Affected Systems & Stakeholders
- **User Personas**:
  - **Hosts** manage feeds and manual blocks and receive conflict alerts.
  - **The admin** has the same powers on every listing.
  - **Guests** see fewer bookable dates and can't book dates already sold on another channel.
  - **External channels** consume the export feed.
- **System Components**:

| Component | Change |
| :--- | :--- |
| `db_core/migrations` | New migration: `listing_blocked_dates`, `listing_calendar_feed` |
| `db_core/src/booking/availability.rs`, `hold.rs`, `booking_request/*`, `reservation_change.rs`, search queries | Treat blocked dates as unavailable |
| `common` | ICS event model, date-range and overlap helpers, export serializer, parsing (pure and unit-tested) |
| `app_api/listing_api` | Public `calendar.ics`, feed CRUD, manual block CRUD, "Sync now" trigger |
| `app_api/calendar_sync_job` (new) | Cloud Run Job: fetch, parse, reconcile, flag conflicts |
| `app_api/email_worker` | Conflict alert template |
| `web_client`, `web_ui_admin` | Calendar sync and blocks UI, conflicts list |
| `web_ui_guest` | Calendar shows blocks (through `get_unavailable_ranges`) |
| `.github/workflows/cicd_full_stack.yml`, `cleanup_feature.yml` | Deploy the job on all branches with `gcloud run jobs deploy`; a Cloud Scheduler job on `main` only; IAM; delete the job when a branch is cleaned up |

---

## 5. Constraints & Non-Negotiable Invariants
- **Zero double-bookings**: blocked dates are checked inside the existing locked hold transaction (`SELECT ... FOR UPDATE` on `listing`). A sync that inserts blocks takes the same listing lock, so a block and a hold can't both commit for the same nights unseen. If they collide, the conflict is recorded, never silently dropped.
- **One availability rule**: blocks use the same `[date_from, date_to)` overlap rule as bookings, so the calendar, search and holds agree. This extends the existing `availability_parity_tests`.
- **Never mutate bookings from sync**: imports only create, update or delete rows in `listing_blocked_dates` whose source is the feed. Manual blocks and bookings are untouched by sync.
- **Fail-safe**: a failed, empty or unparseable fetch never releases dates. Release happens only after a successful parse.
- **SSRF and abuse hardening on feed fetch**:
  - `https` only
  - private, loopback and link-local IP ranges refused after DNS resolution
  - connect and read timeouts
  - a response size cap and a limit on events per feed
  - no redirects to disallowed hosts
- **No blocking on Tokio**: large ICS parsing runs in `spawn_blocking`.
- **Privacy**: the export never contains guest name, email, phone or messages.
- **Scale-to-zero**: the job runs to completion and exits, leaving no idle instance. Adding the export endpoint must not regress the `listing_api` cold start (< 300ms p50, < 1s p95).
- **Stay within the Cloud Scheduler free tier (3 jobs per billing account)**: the sweeps jobs on `dev` and `main` use 2, and calendar sync on `main` adds the 3rd. Adding any further scheduled job is out of scope unless an existing one is consolidated.
- **Project floor**: no `unwrap`/`expect`, docs on every public item, `sqlx::query!` macros, a new migration only (never edit applied ones), and `tracing` with `#[instrument]`. See `CONSTRAINTS.md`.

---

## 6. Success Metrics & Signals
- **Acceptance (testable)**:
  - [ ] A feed fixture with N events produces N blocks. Re-running is idempotent, with no duplicates.
  - [ ] An event removed from the feed releases its block on the next successful sync. An event whose dates changed updates its block.
  - [ ] A fetch that fails (timeout, 5xx, bad ICS) keeps the existing blocks and records the error.
  - [ ] A hold, request to book, reservation change or date search that overlaps a block is refused or excluded with the existing `DATE_UNAVAILABLE`-style error.
  - [ ] A concurrent hold and sync on the same nights never both succeed silently. Either the hold is refused or a conflict is recorded.
  - [ ] An imported event that overlaps a held or confirmed booking creates a conflict record and one host email, and the booking stays unchanged.
  - [ ] `calendar.ics` validates as RFC 5545, includes confirmed stays, pending holds, manual and imported blocks, and contains no guest PII.
  - [ ] Feed and block endpoints return 403 for a host on someone else's listing.
  - [ ] Feed URLs that resolve to private or loopback addresses, or use non-https schemes, are rejected.
  - [ ] Round trip: the exported feed, re-imported, yields the same date ranges.
  - [ ] CI deploys `calendar-sync-$SUFFIX` on every branch. A Cloud Scheduler job for it exists on `main` only, and `cleanup_feature.yml` removes the job.
- **Leading Indicator**: 100% of active feeds synced successfully within the last 30 minutes, visible in the feed status.
- **Lagging Indicator**: zero cross-channel double-bookings reported across the 5 MVP properties after hosts connect their channels.

---

## 7. Open Questions & Flagged Concerns
- [ ] **Echo loop with re-export**: there is one export URL per listing, so Our Places can't tell which channel is fetching it. That means Airbnb's own bookings come back to Airbnb in our feed. Options:
  - per-feed export URLs that leave out that channel's events
  - preserving the source `UID` and relying on channels to dedupe
  - accepting the duplicate block

  Decide during Design.
- [ ] **Reservation # in an unauthenticated feed**: `/listings/:id/calendar.ics` is guessable, and listing ids are public. If `confirmation_code` is ever used for guest lookup or support identification, publishing it leaks a credential-like value. Consider an opaque per-booking export id, or a tokenized URL that can be regenerated, before shipping.
- [ ] **Pending holds in the export**: holds that expire (120 min) will appear and then disappear on external calendars, which may briefly block sellable dates there. This is accepted for now. Confirm it's acceptable.
- [ ] **Turnover gap**: should `days_between_bookings` widen blocked ranges, imported and manual, the way it widens bookings? Channels usually export their own prep days already.
- [ ] **Overlapping events in one feed, or an `RRULE`/`EXDATE` in a feed**: do we support recurrence, or reject it and record a feed error?
- [ ] **"Sync now" path**: run the job on demand for a single listing (Cloud Run Jobs execute API with overrides), or sync inline in `listing_api` with a short timeout? Running the job needs IAM scoped to that one job, plus the override permission if a single listing is passed (`roles/run.jobsExecutorWithOverrides` or `run.developer`); confirm which.
- [ ] **Failure alerting threshold**: after how many consecutive failures should the host be emailed (e.g. 4 failures, about 1 hour)?
- [ ] **Past events**: ignore events that ended before today on import, and leave past stays out of the export to keep feeds small?
- [ ] **Local dev**: run the job as a CLI binary against local Postgres, with feed fixtures served locally, since the SSRF guard blocks localhost. This needs a dev-only allowlist toggle.
