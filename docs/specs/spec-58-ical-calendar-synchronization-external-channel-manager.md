# Spec 58: iCal Calendar Synchronization (External Channel Manager)

**Intent:** [`docs/intent/intent-58-ical-calendar-synchronization-external-channel-manager.md`](../intent/intent-58-ical-calendar-synchronization-external-channel-manager.md)
**Branch:** `feat/58-ical-calendar-synchronization-external-channel-manager`
**Status:** Draft rev 2. Design locked through grill-me on 2026-11-08; panel revisions applied 2026-11-09.

## Overview
Hosts list the same villas on Airbnb and VRBO as well as on Our Places. This feature keeps the calendars in step using iCal (RFC 5545) feeds:

- **Import**: each listing can have up to 5 external `.ics` feeds. Their events become **blocked dates** on the listing.
- **Manual blocks**: hosts and the admin block nights by hand.
- **Export**: every listing gets one public, busy-only `.ics` feed. It contains Our Places stays, holds, manual blocks and blocks imported from other channels, so Our Places acts as the hub.
- **Conflicts**: when an imported event lands on a stay that already exists here, the conflict is recorded and the host is told. Bookings are never changed automatically.
- **Ownership checks**: this spec also closes a defence-in-depth gap. `listing_api` write endpoints currently check neither the JWT nor ownership on the server (§1.8).

**Personas**: hosts (their own listings), the admin (any listing), guests (no longer offered nights sold elsewhere), external channels (read the export feed).

### Design decisions
| # | Decision |
|---|---|
| D1 | One export URL per listing: `https://<guest-portal>/listings/{listing_id}/calendar.ics`, with **no token**. The originator accepted the occupancy-exposure risk (§5). Imported events are re-exported with their **original `UID`**, so a channel can drop its own events as duplicates. No per-channel URLs. |
| D2 | The turnover gap (`days_between_bookings`) applies to **imported** blocks, not to **manual** blocks. The export always carries the raw dates, without the gap. |
| D3 | A manual block that overlaps a confirmed stay, live hold or live request is refused with 409 `DATE_UNAVAILABLE`, checking exact nights without the gap and under the listing lock. Blocks may overlap each other. |
| D4 | An imported block that lands on an existing hold or stay is kept, and the booking is untouched. A pending hold can still be paid. A conflict row is created, and the host is emailed when it opens or reopens (§1.5). |
| D5 | A conflict stays open until the host **acknowledges** it. It is marked **stale**, with a reason, once either side goes away. No email is sent when it goes stale. |
| D6 | "Sync now" runs **inline in `listing_api`** for one listing, at most once per 60 s, within a 20 s deadline. The scheduled sweep is a Rust **Cloud Run Job**. Both share the same code. |
| D7 | Feed parsing rules are listed in §1.3. |
| D8 | The host is emailed once after **4 failed scheduled syncs in a row**. The count resets after a successful sync. "Sync now" failures don't count. Adding a feed runs a first sync, and the feed is saved even if that sync fails. |
| D9 | Ownership is enforced on the server for every `listing_api` write, existing ones included, and portals and clients pass the user's JWT (§1.8). |
| D10 | Deployment uses gcloud in CI, not Terraform. The job is deployed on every branch; the Cloud Scheduler trigger (every 15 min) exists on `main` only. Non-prod environments sync by hand. Scheduler jobs total 3 (sweeps on dev and main, plus calendar sync on main), which is the free tier. |
| D11 | The job runs as a **dedicated service account**, `SA_CALENDAR_JOB`, with only Cloud SQL client and Pub/Sub publisher on the email topic. It is created once by hand (§3.7). |

---

## 1. Requirements

### 1.1. Calendar Feeds (Import)
1. A listing manager (§1.8) can add a feed with a `name` and a `url`.
   - `name`: 1–60 chars after trimming. Control characters are stripped.
   - `url`: ≤ 2048 chars, `https` only, a host required, no user info. It must pass `validate_feed_url` (§3.2).
2. Limits:
   - A listing can have at most **5** feeds; otherwise 409 `CALENDAR_FEED_LIMIT`.
   - Each URL can be added once per listing (compared after normalising with the `url` crate); otherwise 409 `CALENDAR_FEED_DUPLICATE`.
   - Both checks run under `listing FOR UPDATE` before the insert. A `23505` from the unique index is also mapped to `CALENDAR_FEED_DUPLICATE` **before anything is logged**, because the Postgres error detail contains the URL.
3. A URL whose path is `/listings/{this listing id}/calendar.ics`, on any host, is refused with 422 `CALENDAR_FEED_SELF`.
4. Adding a feed saves it and runs a first sync through the normal claim (§1.4.4), so the 60 s limit applies. The response contains the feed and that sync's result. A failed first sync still returns **201**, with `last_status = failed` and the error visible.
5. A manager can rename a feed, change its URL, switch it on or off, or delete it. Each of these takes the locks `listing → feed`.
   - **URL change**: the URL is validated again (§1.1.1, §1.1.3). `sync_generation` is incremented, `last_attempt_at` is set to NULL so the new URL can be claimed straight away, and `last_status` is reset to `never` with the error fields cleared. A sync then runs through the claim. The old blocks **stay until the new URL syncs successfully**, which replaces them, so the change is fail-safe. A sync of the old URL that is still in flight is thrown away (§1.4.2).
   - **Switching off**: syncing stops and the blocks are **kept**. The UI marks them "from a disabled calendar".
   - **Delete**: removes the feed's blocks. Their conflicts become stale (`event_removed`).
6. Each feed shows `last_status` (`never` | `ok` | `warning` | `failed`), `last_success_at`, `last_attempt_at`, `last_error`/`last_warning`, `event_count` and `consecutive_failures`.
7. **Feed URLs count as secrets.** Airbnb and VRBO export URLs contain a read token.
   - Only listing managers receive them.
   - Logs and spans carry `feed_id` and the URL's **host** only. Instrumented functions use `#[instrument(skip_all, fields(feed_id, listing_id, host))]`.
   - The feed row type's `Debug` impl redacts the URL.
   - `reqwest` errors are stripped of their URL (`.without_url()`) and turned into a `FetchError` enum whose messages never contain a URL, path or query.
   - URLs are never stored in `last_error`.

### 1.2. Blocked Dates
1. A block is a half-open set of nights `[date_from, date_to)` on a listing. Its source is `ical_feed` or `manual`.
2. **Manual blocks**: a manager creates one with `date_from`, `date_to` and an optional `note` (≤ 500 chars, private, never exported).
   - `date_from ≥ today`, where today is in Jamaica time (§3.2).
   - `date_to > date_from`.
   - At most 365 nights.
   - `date_to ≤ today + 24 months` (`checked_add_months`).
   - A manual block with exactly the same dates as another manual block on the listing gives 409 `CALENDAR_BLOCK_DUPLICATE`, enforced by a partial unique index. This catches double submits.

   A manager can delete a manual block. Deleting an imported block returns 409 `CALENDAR_BLOCK_MANAGED_BY_FEED`.
3. Creating a manual block takes `listing … FOR UPDATE`. It is refused with 409 `DATE_UNAVAILABLE` if the **exact** nights, without the gap, overlap a confirmed booking, a live pending hold or a live request (using `clock_timestamp()`). Overlapping other blocks is allowed (D3).
4. **Availability**: a block makes its nights unavailable wherever bookings and requests already do:

   | Path | Location |
   |---|---|
   | `check_availability` | `db_core/src/booking/availability.rs` (display only, no lock) |
   | `check_availability_excluding` | same; used by `booking/update.rs` and `reservation_change.rs`, both under the listing lock |
   | `get_unavailable_ranges` | same; the guest calendar |
   | Instant hold | `db_core/src/booking/hold.rs`, under the listing lock |
   | Request create, quote and convert (`check_dates_free`) | `db_core/src/booking_request/request.rs`, `quote.rs`, `convert.rs` |
   | **Request accept** | `db_core/src/booking_request/respond.rs::accept_request`. It **adds** a `check_dates_free`-style block check under its existing listing lock. Today accept does no date check, so accepting a request over a new block would otherwise succeed. |
   | Date search | `db_core/src/listing/queries.rs` |

   Overlap rule for a block `k` against a stay `[$in, $out)`, where `g = LEAST(GREATEST(l.days_between_bookings,0),365)` and `w = CASE WHEN k.source = 'ical_feed' THEN g ELSE 0 END`:
   `k.date_from - w < $out AND k.date_to + w > $in`.

   Every query also adds the bounding predicates `k.date_from < $out + g AND k.date_to > $in - g`. They are a superset of the rule above, so the result doesn't change, but they let `idx_blocked_dates_listing_range` narrow the scan.
5. **Changing an existing booking** (`check_availability_excluding`: reservation change, booking update): blocks are checked **only against nights the booking doesn't already hold**. This lets a conflicted booking be shortened or moved off the block, which is the host's main way to resolve a conflict. In SQL, the block overlap is tested against `[$in, LEAST($out, old.date_from))` and `[GREATEST($in, old.date_to), $out)`, where `old` is the excluded booking's current range. **Each half is guarded so it can't be empty**: `$in < LEAST($out, old.date_from) AND …` and `GREATEST($in, old.date_to) < $out AND …`. Without the guards, shortening a stay inside a block would be refused. Bookings and requests keep the existing rule.
6. Blocks never expire. An imported block stays until its event leaves the feed after a **successful** sync, or the feed is deleted.

### 1.3. Feed Parsing (D7)
The parser is a small, hand-written subset of RFC 5545 in `common`. Only the top-level properties of each `VEVENT` are read; `VTIMEZONE`, `STANDARD`, `DAYLIGHT` and `VALARM` components and their properties are skipped.

| Input | Rule |
|---|---|
| Body encoding | It must be UTF-8 (a leading BOM is stripped); otherwise the feed **fails**. Lines are unfolded (CRLF or LF followed by a space or tab). `\,` `\;` `\n` `\\` are unescaped. Names are case-insensitive. A line over 8 KB fails the feed. |
| Structure | The body must have `BEGIN:VCALENDAR` … `END:VCALENDAR`, and every `BEGIN:VEVENT` must have a matching `END:VEVENT`. Anything else, including a body **cut off** mid-transfer, **fails** the feed, and its existing blocks are kept. |
| `DTSTART;VALUE=DATE` / `DTEND;VALUE=DATE` | The nights `[start, end)`. A missing `DTEND`, or `DTEND ≤ DTSTART`, gives **1 night**. |
| `DATE-TIME` at exactly UTC midnight (`T000000Z`) | Treated as a **DATE** with that calendar date, because some channels write all-day events this way. |
| Other `DATE-TIME` (UTC `Z`, `TZID=`, or floating) | Converted to **Jamaica dates** (§3.2). The event covers every night it touches: `start = date(DTSTART)`, `end = date(DTEND)`, or `+1 day` when `DTEND` isn't exactly local midnight. An empty or inverted range gives 1 night. A `TZID` other than UTC or an `America/Jamaica` alias is handled as floating time. An event ending at 11:00 local time therefore also blocks that night (documented; acceptable). |
| `DURATION` instead of `DTEND` | `P<n>D` and `P<n>W` are supported. Any other duration is rounded up to whole days. |
| Missing `DTSTART` | The event is skipped and the feed records a warning. |
| `STATUS:CANCELLED` | Skipped, so the block is released if it existed. |
| `RRULE` / `RDATE` / `EXDATE` | Only the first occurrence is used, and the feed records a warning. |
| `UID` ending in `@ourplaces` | **Skipped.** These are our own exported events echoing back through another channel or a channel manager, and would otherwise cause false conflicts. |
| Window | An event whose `date_to ≤ today` is ignored, and so is one whose `date_from > today + 24 months`. Only **`date_to` is clipped**, to `today + 24 months`. `date_from` is never clipped, so an ongoing stay keeps stable dates and doesn't churn. |
| `UID` normalisation (gives unique, ≤ 512-byte UIDs) | Control characters are removed. A UID over 500 bytes, or one empty after cleaning, is replaced by `op-h-<sha256 hex>` of the original. A missing UID becomes `op-derived-<sha256(feed_id ‖ start ‖ end)>`. Duplicates in a feed are numbered `<uid>#2`, `#3`, … in **(date_from, date_to) order**, so a feed that reorders its events gives the same names. If a derived name collides with a real UID, numbering moves to the next free suffix. The parser **guarantees** the output UIDs are unique and ≤ 512 bytes. |
| Body > 2 MB, or > 5,000 `VEVENT`s | The feed **fails** and its existing blocks are kept. |
| Event text (`SUMMARY`, `DESCRIPTION`, …) | Never stored or logged, because it may contain guest names. |

Parsing a body over 64 KB runs in `spawn_blocking`.

### 1.4. Sync
1. **Fetching happens outside any transaction**, through `GuardedFetcher` (§3.4):
   - Always `https`, at most 3 redirects, and each hop checked again.
   - Timeouts: 5 s to connect and 10 s in total. Both can be injected for tests.
   - The body is streamed with a 2 MB cap.
   - Headers: `Accept: text/calendar, */*;q=0.1` and `User-Agent: OurPlaces-CalendarSync/1.0`.
   - An HTTP status ≥ 400 counts as failure.
2. **Reconciling is one transaction per feed.**
   - It starts with `SET LOCAL lock_timeout = '5s'`, the same as `promo.rs`. A lock timeout counts as a deferred result, not a failure.
   - Locks are taken **listing → feed**. Bookings are only read.

   Steps:
   1. `SELECT … FROM listing WHERE id = $1 FOR UPDATE`. This gives the gap and serialises with holds, manual blocks and feed edits.
   2. `SELECT sync_generation, is_active FROM listing_calendar_feed WHERE id = $1 FOR UPDATE`. If the row is gone, the feed is off, or `sync_generation ≠ claimed generation`, **do nothing**. This covers a URL change, a deletion or a re-add while the fetch was running.
   3. Upsert the parsed events: `INSERT … SELECT FROM UNNEST($uids::text[], $froms::date[], $tos::date[]) ON CONFLICT (feed_id, external_uid) DO UPDATE SET date_from = EXCLUDED.date_from, date_to = EXCLUDED.date_to, updated_at = now() WHERE (listing_blocked_dates.date_from, listing_blocked_dates.date_to) IS DISTINCT FROM (EXCLUDED.date_from, EXCLUDED.date_to)`. UIDs are unique (§1.3), so Postgres error 21000 can't happen.
   4. `DELETE FROM listing_blocked_dates WHERE feed_id = $1 AND NOT (external_uid = ANY($uids))`. `$uids` is always bound as a non-NULL `text[]`; an empty array deletes everything, which is what E2 needs.
   5. Detect conflicts (§1.5) and get back the conflicts that opened or reopened. Queue one host email for each, in the same transaction (`insert_email_outbox_tx`).
   6. Update the feed: `last_status` (`ok` or `warning`), `last_success_at = now()`, `consecutive_failures = 0`, `failure_alerted_at = NULL`, `event_count`, `last_error = NULL`, and `last_warning`.
3. **Recording a failure** (fetch, size or parse) takes a short transaction that locks the feed row only, with no listing lock and so no lock cycle. It applies the same generation check as step 2.
   - It sets `last_status = failed` and `last_error`: a sanitised one-line reason taken from the `FetchError`/`FeedError` code, with no URL.
   - For **scheduled** runs only, it adds 1 to `consecutive_failures`.
   - When `consecutive_failures ≥ 4 AND failure_alerted_at IS NULL`, it sets `failure_alerted_at = now()` and queues one host email (D8).
   - **Blocks are not touched.**
4. **Claiming.** Before fetching, a run claims feeds atomically, and no lock is held across HTTP. `claim_*` returns `(id, listing_id, url, sync_generation)`.
   ```sql
   WITH due AS (
     SELECT f.id FROM listing_calendar_feed f JOIN listing l ON l.id = f.listing_id
     WHERE f.is_active AND l.deleted_at IS NULL [AND f.listing_id = $listing]
       AND (f.last_attempt_at IS NULL OR f.last_attempt_at < now() - $min_interval)
     ORDER BY f.last_attempt_at NULLS FIRST, f.id
     LIMIT $n
     FOR UPDATE OF f SKIP LOCKED)
   UPDATE listing_calendar_feed f SET last_attempt_at = now() FROM due WHERE f.id = due.id
   RETURNING f.id, f.listing_id, f.url, f.sync_generation
   ```
   - Scheduled job: `$min_interval = 10 min`.
   - "Sync now", feed create, and URL change: `$min_interval = 60 s`, for this listing only.
   - "Sync now" when the listing has active feeds but none could be claimed: **429** `CALENDAR_SYNC_RATE_LIMITED`, with `Retry-After` set to the seconds until the earliest feed is claimable.
   - A partial claim returns `skipped` results for the feeds it couldn't claim.
5. **Processing order.** Feeds are fetched in parallel, up to the concurrency limit. **Reconciling happens one feed at a time within each listing**, so feeds of the same listing never hold several pool connections while waiting for the same listing lock. Different listings reconcile in parallel.
6. **Emails** are published after commit (`api_core::email_publisher`) from a detached task. A publish failure is logged at `error` with the outbox id. Recovery is §1.4.8.
7. **"Sync now"** (`POST …/calendar/sync`), **feed create** and **URL change** run within a **20 s handler deadline**, and the portal calls them with `.timeout(25 s)`.
   - The deadline only cuts off **fetch and parse**. A reconcile that has started always finishes; `lock_timeout` bounds it at 5 s.
   - Feeds cut off by the deadline return `deferred`: no failure is recorded, and the claim simply expires. The response lists each feed's result: `ok`, `warning`, `failed`, `skipped` or `deferred`.
8. **The scheduled job**:
   - Claims pages of **32** feeds (`claim_due(10 min, 32)`) and processes them with configurable concurrency (`calendar_sync.job_concurrency`, default 8).
   - Checks its **9-minute deadline before starting each feed**. Feeds not started are released (`UPDATE … SET last_attempt_at = $previous WHERE id = $id AND last_attempt_at = $this_claim_at`). The condition means a release never overwrites a newer claim, and the next run picks the feed up.
   - Runs with a pool of `job_concurrency + 2` connections.
   - **Republishes stuck emails**: `email_outbox` rows with `status = 'pending'`, `template_id LIKE 'calendar_%'`, `created_at < now() - 5 min` and `attempts < max_retries`, at most 100 per run. This covers publish failures, because the repo has no general outbox republish sweep today.
   - Logs a summary (`feeds_ok`, `feeds_warning`, `feeds_failed`, `feeds_deferred`, `conflicts_opened`, `emails_republished`, `duration_ms`).
   - Exits `0` when individual feeds fail. It exits non-zero only when it can't reach the database or load its configuration.
9. All "live hold" checks inside sync, conflict detection and manual blocks use `clock_timestamp()`, not `now()`, because `now()` is fixed when the transaction starts, before the lock wait. This follows `respond.rs` and `update.rs`.

### 1.5. Conflicts (D4, D5)
1. A conflict exists when an **imported** block, widened by the gap (§1.2.4), overlaps a booking that is `confirmed`, or `pending` with `expires_at > clock_timestamp()`. If the exact nights overlap, `kind = overlap`. If they don't overlap but there's no gap left for cleaning, `kind = turnover`. Manual blocks never create conflicts, because §1.2.3 refuses them. Live **requests** don't create conflicts either: create, quote, convert and accept all refuse once the block exists, and the host sees `DATE_UNAVAILABLE`.
2. **Identity and dedupe**: `UNIQUE (feed_id, external_uid, booking_id)`. Postgres treats NULLs as distinct, so rows whose feed was deleted (`feed_id` set NULL) never collide with new ones. Detection is an upsert:
   ```sql
   INSERT … ON CONFLICT (feed_id, external_uid, booking_id) DO UPDATE SET
     block_id = EXCLUDED.block_id, block_date_from = EXCLUDED.block_date_from, block_date_to = EXCLUDED.block_date_to,
     kind = EXCLUDED.kind,
     acknowledged_at = CASE WHEN <reopen> THEN NULL ELSE calendar_conflict.acknowledged_at END,
     acknowledged_by = CASE WHEN <reopen> THEN NULL ELSE calendar_conflict.acknowledged_by END,
     reopened_at     = CASE WHEN <reopen> THEN now() ELSE calendar_conflict.reopened_at END
   WHERE (calendar_conflict.block_id, calendar_conflict.block_date_from, calendar_conflict.block_date_to, calendar_conflict.kind)
         IS DISTINCT FROM (EXCLUDED.block_id, EXCLUDED.block_date_from, EXCLUDED.block_date_to, EXCLUDED.kind)
      OR (calendar_conflict.acknowledged_at IS NOT NULL AND <reopen>)
   RETURNING id, (xmax = 0) AS inserted, (reopened_at IS NOT DISTINCT FROM now()) AS reopened
   ```
   `<reopen>` is true when either of these holds:
   - **The conflict was stale.** Its `block_id` was NULL, or its booking had been released, and the overlap is now live again.
   - **It got worse.** It goes from `turnover` to `overlap`, or the dates change to cover nights it didn't cover before.

   `RETURNING` sees the **updated** row, so `<reopen>` can't be evaluated there. `reopened` is read back from `reopened_at = now()` instead, and `now()` is stable within the transaction. `RETURNING old.*` would need PostgreSQL 18, and production runs 17. The extra `OR` in the `WHERE` reopens a conflict that was acknowledged while no longer overlapping, once its event moves back onto the booking.

   Emails are queued only for rows where `inserted OR reopened`. A sync that changes nothing touches no rows and sends no email.
3. **Stale** is worked out when the conflict is read, not stored:
   - `event_removed`: `block_id IS NULL`.
   - `booking_released`: the booking is no longer `confirmed` or a live hold.
   - `no_longer_overlapping`: the current block dates, widened by the gap, no longer overlap the booking.
4. The host or admin **acknowledges** a conflict, which sets `acknowledged_at` and `acknowledged_by`. The list shows open, non-stale conflicts first, then open stale ones (greyed out, with the reason), then acknowledged ones. Acknowledged history is **paginated**, 20 per page.
5. A pending hold that has a conflict **can still be paid and confirmed** (D4). Neither payment nor confirmation code changes.
6. The conflict email (`calendar_conflict_host.html`, `EmailTemplate::CalendarConflictHost`) goes to the listing owner's email. It contains the listing name, feed name, dates, the booking's `confirmation_code`, the kind, whether this is a new or a reopened conflict, and a link to the calendar page in the admin portal. Every interpolated value is HTML-escaped.

### 1.6. Export
1. **Public URL**: `GET https://<guest-portal>/listings/{listing_id}/calendar.ics`. This is a raw Topcoat `#[route]` on `web_ui_guest`.
   - It calls `listing_api` `GET /api/v1/listings/{id}/calendar.ics` through `HttpClient::get_request`, which carries only the service identity (Cloud Run IAM). **No inbound header is forwarded**, `X-Forwarded-Authorization` included.
   - `listing_api` stays private (`--no-allow-unauthenticated`).
2. **Contents**: every item whose `date_to > today` and whose `date_from ≤ today + 24 months`:

   | Source | `UID` | `SUMMARY` | `DESCRIPTION` |
   |---|---|---|---|
   | Booking: `confirmed`, or `pending` with `expires_at > now()` | `booking-<uuid>@ourplaces` | `Reserved` | `Source: OurPlaces\nReservation #: <confirmation_code>` |
   | Manual block | `block-<uuid>@ourplaces` | `Reserved` | `Source: OurPlaces` |
   | Imported block | **original `UID`** (D1) | `Reserved` | `Source: OurPlaces (via <feed name>)` |

   Every event has `DTSTART;VALUE=DATE` and `DTEND;VALUE=DATE` with raw dates and no gap (D2), `DTSTAMP` set to the generation time, `STATUS:CONFIRMED` and `TRANSP:OPAQUE`.
   - Pending holds also use `CONFIRMED`, because some channels ignore `TENTATIVE`.
   - The calendar has `VERSION:2.0`, `PRODID:-//Our Places//Calendar Sync//EN`, `CALSCALE:GREGORIAN`, `METHOD:PUBLISH` and `X-WR-CALNAME:<listing name>`.
   - Every value, the imported UID included, goes through the text escape and 75-octet fold. Control characters (CR, LF and others) are removed from the listing and feed names before escaping.
   - Lines use CRLF.
   - No guest name, email, phone, note or message is ever included.
3. **Response**: `200 text/calendar; charset=utf-8`, with:
   - `Cache-Control: public, max-age=300`
   - `Content-Disposition: inline; filename="<listing-id>.ics"`
   - `ETag`: a strong hash of the body with the `DTSTAMP` lines left out. `If-None-Match` gets **304**.

   An unknown or deleted listing, or an id that isn't a UUID, returns **404** as plain text with no layout. A deactivated listing still serves its feed.
4. **Guest-portal cache**: an in-process cache with a 60 s TTL, keyed by listing id, holding at most 1,000 entries. **404s** are cached too, in a separate store of at most 200 entries, so a flood of random ids can't push the real listings out. Errors aren't cached.
5. The admin portal shows the export URL with a copy button. The base comes from a new admin-portal env var, `GUEST_PORTAL_URL`, set from `vars.GUEST_PORTAL_URL_<ENV>` and falling back to the guest service URL.

### 1.7. Admin Portal
1. New page **`/admin/listings/{id}/calendar`**, using `managed_listing` and linked next to "Pricing". It has four sections, each an HTMX swap target:
   - **Export link**: a read-only input, a copy button, and a one-line hint ("Paste this into Airbnb → Availability → Connect calendars").
   - **Imported calendars**:
     - A table: name, status badge (`badge-success`/`badge-warning`/`badge-error`/`badge-ghost`), last success (relative time), event count, and the error tooltip.
     - Actions: edit, switch on or off, remove (with confirmation).
     - An "Add calendar" form (name, URL).
     - A **Sync now** button (`hx-post`, `hx-indicator` spinner, `hx-disabled-elt="this"`). It shows the result for each feed (`ok`/`warning`/`failed`/`skipped`/`deferred`); a 429 shows "Synced less than a minute ago."
   - **Blocked dates**: manual blocks (dates, note, remove) and an add form with `hx-disabled-elt`. Imported blocks are listed read-only, grouped by feed. Blocks from a disabled feed are labelled "from a disabled calendar".
   - **Conflicts**: open first, then stale ones greyed out with the reason, then acknowledged ones collapsed and paginated. Each open one has an **Acknowledge** button; a reopened one shows a "Reopened" badge.
2. **Dashboard** (`/admin`): a card counting open, non-stale conflicts and feeds with `consecutive_failures ≥ 4` across the caller's listings, with links to each listing's calendar page. It is hidden when both counts are 0. If the overview fails, the card is hidden and the error logged at `warn`.
3. Every form posts with the user's service token (`service_token(cx, user)`). API errors become inline DaisyUI `alert`s, using the message for each error code.

### 1.8. Server-Side Listing Authorization (D9)
1. New helper `listing_api::authz::require_listing_manager(pool, claims, listing_id) -> Result<(User, ListingOwner), ApiError>`:
   - `Claims` are extracted first. A missing or invalid JWT gets **401** before any listing lookup.
   - The caller is loaded with the rules of booking_api's `caller`: an unknown, deactivated or deleted user gets 401.
   - The listing's `user_id` and `deleted_at` are loaded. A listing that doesn't exist or is deleted gets **404**.
   - An admin is always allowed. A host is allowed when `listing.user_id == caller.id`. Anyone else gets **403** `FORBIDDEN`, logged at `warn` with `user_id`, `listing_id` and `action`. The 403 reveals only that the listing exists, and the public `GET` already does that.
2. Applied to:
   - `PATCH /listings/{id}` and `DELETE /listings/{id}`
   - `POST /listings/{id}/images/presign` and `DELETE /listings/{id}/images/{image_id}`
   - `POST /listings/{id}/price-overrides`, plus `PUT` and `DELETE /listings/{id}/price-overrides/{override_id}`
   - **every** new calendar endpoint except the service-only export
3. `POST /listings` needs a JWT and the Admin or Host role (otherwise 403).
   - An admin can set `user_id` to any user who exists, is active, and has the Host or Admin role; otherwise 422 `VALIDATION_ERROR`.
   - For a host, `user_id` is forced to the caller's id.
   - `UpdatedListingRequest` has no owner field, so ownership can't be transferred through `PATCH`.
4. `GET` listing, search, reviews and `GET price-overrides` stay public.
5. The write functions in `common::app_client` and `web_client` take a `user_jwt: &str` argument and send it as `X-Forwarded-Authorization` (`HttpClient::user_request`). The admin portal passes `service_token(cx, user)`.
6. The Playwright API spec (`playwright/e2e/api/listing-api.spec.ts`) and the `listing_api` tests send JWTs, and there are new 401 and 403 cases.

### 1.9. Exclusions / Non-Goals
- Airbnb or VRBO partner APIs, syncing rates or listing content, importing guest details.
- Automatically cancelling or moving bookings because of a conflict, or refusing payment because of one.
- Expanding recurring events (`RRULE`) beyond the first occurrence.
- Per-channel, tokenised or regenerable export URLs (D1, accepted risk).
- Exporting live requests to book: they block here, but aren't exported.
- Blocks shorter than a night, or check-in/check-out times inside the feed.
- A Cloud Scheduler trigger on dev or feature branches.
- A visual calendar grid in the admin portal. Lists and forms are enough for the MVP.
- Per-listing time zones. All MVP listings are in Jamaica (UTC−5, no DST).
- A general outbox republish sweep for non-calendar emails (only `calendar_*` is covered, §1.4.8).

---

## 2. Edge Cases & Failure States
| # | Case | Behaviour | Test |
|---|---|---|---|
| E1 | A feed fetch times out, returns 5xx, or returns HTML instead of ICS | The feed is marked `failed` and its blocks are kept. Scheduled runs add to the count, and after 4 the host gets one email. | T10, T13 |
| E2 | A complete, valid `VCALENDAR` with **no events** | Success, so all of the feed's blocks are released. A feed that had ≥ 1 future block gets `last_status = warning` ("feed returned no events"). | T10 |
| E3 | A hold is placed while a sync is mid-flight for the same nights | Both take `listing FOR UPDATE`, so they run one after the other. Hold first: the sync records a conflict. Sync first: the hold is refused. | T11 race test |
| E4 | Two syncs of the same feed (job and "Sync now") | The claim, with `SKIP LOCKED`, lets only one run fetch. Two reconciles would run one after the other anyway, and both steps give the same result when repeated. | T9, T10 |
| E5 | A feed is deleted, re-added, or its URL is changed while its fetch is in flight | `sync_generation` (or a missing row) makes reconcile and `record_failure` do nothing. | T10 |
| E6 | Our export echoes back through a channel or a channel manager | Imported UIDs ending in `@ourplaces` are skipped (§1.3). A channel's own events coming back to it carry its original UID and are dropped as duplicates on its side. | T3, T19 |
| E7 | A host adds **our own export URL** as an import | 422 `CALENDAR_FEED_SELF`. Another listing's export URL is allowed. | T2, T17 |
| E8 | An event's dates change so it now overlaps a stay, or a turnover becomes an overlap | The upsert reopens the conflict and sends one new email (§1.5.2), even if it had been acknowledged. Unchanged dates send nothing. | T11 |
| E9 | A conflicted hold expires unpaid | The conflict shows `booking_released` (stale) until it's acknowledged. No email. | T11 |
| E10 | A guest instant-books nights already blocked | Refused under the lock. No conflict. | T8 |
| E11 | A host creates, quotes, converts **or accepts** a request over a new block | Refused with `DATE_UNAVAILABLE` (accept gets a new check, §1.2.4). | T8 |
| E12 | A reservation change into blocked nights | Refused. A change that only shortens the stay, or moves it off the block, is allowed (§1.2.5). | T7 |
| E13 | The turnover gap changes after blocks exist | Availability follows straight away, because the gap is applied when queried. Conflict `kind` and staleness are worked out again on read. | T7, T11 |
| E14 | A URL that resolves to a private IP, or is a private IP literal, including encoded forms (`2130706433`, `0x7f.1`, `[::ffff:7f00:1]`) | Refused before connecting, with `last_error = "address not allowed"`. | T2, T6 |
| E15 | A redirect to `http://` or to an internal **https** host | Refused, and the feed fails. | T6 |
| E16 | The listing is soft-deleted | The job skips its feeds, the export returns 404, and management returns 404. | T9, T19 |
| E17 | DST and time zones | Jamaica has no DST, so a fixed UTC−5 offset is exact. | T2 |
| E18 | An event lasting 2,000 nights, or one that started in the past | Only `date_to` is clipped (to today + 24 months). `date_from` stays as it is. | T3 |
| E19 | A feed has a 5,001st event | The whole feed fails, and nothing is partially applied. | T3, T10 |
| E20 | The email publish fails after commit | Logged at `error`. The job republishes pending `calendar_*` outbox rows older than 5 minutes (§1.4.8). | T13, T24 |
| E21 | A job instance is killed mid-run | Each feed's transaction is atomic. Claimed but unprocessed feeds become claimable again after 10 minutes. | T9 |
| E22 | A manual block over the gap days of a stay | Allowed (D3). | T12 |
| E23 | A 200 response whose body is cut off | No `END:VCALENDAR` or an unbalanced `VEVENT` fails the feed, so blocks aren't released. | T3 |
| E24 | A long, control-character or colliding `UID` | The parser normalises it (§1.3), so the database CHECK and unique constraints can't abort the reconcile. | T3, T10 |
| E25 | "Sync now" with 5 slow feeds | Fetches run in parallel and reconciles one at a time. Anything unfinished at 20 s returns `deferred`. Pool use for the listing is 1 connection. | T13, T17 |
| E26 | A lock wait over 5 s in reconcile | The result is `deferred`, the failure count isn't increased, and the next run retries. | T10 |
| E27 | A double-submitted manual block | 409 `CALENDAR_BLOCK_DUPLICATE`, and the UI disables the button while the request is in flight. | T12 |
| E28 | A disabled feed | Its blocks stay, are labelled in the UI, and aren't refreshed. | T21, T22 |

---

## 3. Technical Implementation

### 3.1. Data Model (new migration `db_core/migrations/<ts>_calendar_sync.sql`)
`20261106043200` is already on `origin/dev`, so this is a **new** file, created with `sqlx migrate add calendar_sync`.

```sql
CREATE TYPE calendar_block_source  AS ENUM ('ical_feed', 'manual');
CREATE TYPE calendar_feed_status   AS ENUM ('never', 'ok', 'warning', 'failed');
CREATE TYPE calendar_conflict_kind AS ENUM ('overlap', 'turnover');

CREATE TABLE listing_calendar_feed (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    listing_id           UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    name                 VARCHAR(60)  NOT NULL CHECK (length(btrim(name)) > 0),
    url                  TEXT         NOT NULL CHECK (length(url) <= 2048),
    is_active            BOOLEAN      NOT NULL DEFAULT TRUE,
    sync_generation      INTEGER      NOT NULL DEFAULT 0,
    last_status          calendar_feed_status NOT NULL DEFAULT 'never',
    last_error           TEXT,
    last_warning         TEXT,
    last_attempt_at      TIMESTAMPTZ,
    last_success_at      TIMESTAMPTZ,
    consecutive_failures INTEGER      NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0),
    failure_alerted_at   TIMESTAMPTZ,
    event_count          INTEGER      NOT NULL DEFAULT 0 CHECK (event_count >= 0),
    created_by           UUID REFERENCES "user"(id) ON DELETE SET NULL,
    created_at           TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    UNIQUE (listing_id, url),
    UNIQUE (id, listing_id)                      -- target for the composite FKs below
);
CREATE INDEX idx_calendar_feed_due ON listing_calendar_feed (last_attempt_at NULLS FIRST, id) WHERE is_active;

CREATE TABLE listing_blocked_dates (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    listing_id   UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    source       calendar_block_source NOT NULL,
    feed_id      UUID,
    external_uid TEXT CHECK (length(external_uid) <= 512),
    date_from    DATE NOT NULL,
    date_to      DATE NOT NULL CHECK (date_to > date_from),
    note         VARCHAR(500),
    created_by   UUID REFERENCES "user"(id) ON DELETE SET NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (feed_id, listing_id) REFERENCES listing_calendar_feed (id, listing_id) ON DELETE CASCADE,
    CHECK (
      (source = 'ical_feed' AND feed_id IS NOT NULL AND external_uid IS NOT NULL AND note IS NULL)
      OR (source = 'manual' AND feed_id IS NULL AND external_uid IS NULL)
    ),
    UNIQUE (feed_id, external_uid)
);
CREATE INDEX idx_blocked_dates_listing_range ON listing_blocked_dates (listing_id, date_from, date_to);
CREATE INDEX idx_blocked_dates_listing_to    ON listing_blocked_dates (listing_id, date_to);
CREATE UNIQUE INDEX uq_blocked_dates_manual  ON listing_blocked_dates (listing_id, date_from, date_to) WHERE source = 'manual';

CREATE TABLE calendar_conflict (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    listing_id      UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    feed_id         UUID,
    feed_name       VARCHAR(60) NOT NULL,          -- kept for the audit trail after the feed is deleted
    external_uid    TEXT NOT NULL,
    block_id        UUID REFERENCES listing_blocked_dates(id) ON DELETE SET NULL,
    booking_id      UUID NOT NULL REFERENCES booking(id) ON DELETE CASCADE,   -- matches booking_history
    kind            calendar_conflict_kind NOT NULL,
    block_date_from DATE NOT NULL,
    block_date_to   DATE NOT NULL,
    detected_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reopened_at     TIMESTAMPTZ,
    acknowledged_at TIMESTAMPTZ,
    acknowledged_by UUID REFERENCES "user"(id) ON DELETE SET NULL,
    FOREIGN KEY (feed_id, listing_id) REFERENCES listing_calendar_feed (id, listing_id) ON DELETE SET NULL (feed_id),
    CHECK (acknowledged_by IS NULL OR acknowledged_at IS NOT NULL),
    UNIQUE (feed_id, external_uid, booking_id)     -- NULL feed_id rows never collide (§1.5.2)
);
CREATE INDEX idx_calendar_conflict_open ON calendar_conflict (listing_id) WHERE acknowledged_at IS NULL;
CREATE INDEX idx_calendar_conflict_history ON calendar_conflict (listing_id, acknowledged_at DESC) WHERE acknowledged_at IS NOT NULL;
```

`ON DELETE SET NULL (feed_id)` with a column list needs PostgreSQL 15 or later. The Cloud SQL and devenv versions are checked in T1; if they're older, use a trigger instead. The enums get Rust types with `#[derive(sqlx::Type)] #[sqlx(type_name = "…", rename_all = "snake_case")]`.

### 3.2. Domain (`common`)
- `common/src/reference.rs`: `pub const LISTING_UTC_OFFSET_SECS: i32 = -5 * 3600;` and `pub fn listing_today(now: DateTime<Utc>) -> NaiveDate`. A per-listing time zone is out of scope (§1.9).
- **New `common/src/calendar/`**, pure code with no IO. Test modules are declared as siblings in `calendar/mod.rs` (`#[cfg(test)] mod ics_parse_tests;`), so their paths are `calendar::<name>_tests`.
  - `ics_parse.rs`: `parse_feed(body: &[u8], feed_id: Uuid, today: NaiveDate) -> Result<ParsedFeed, FeedError>`, where `ParsedFeed { events: Vec<ExternalEvent>, warnings: Vec<FeedWarning> }` and `ExternalEvent { uid: String, date_from: NaiveDate, date_to: NaiveDate }`. It implements every §1.3 rule and **guarantees** unique UIDs of ≤ 512 bytes.
  - `ics_write.rs`: `write_feed(calendar_name: &str, events: &[ExportEvent], stamp: DateTime<Utc>) -> String`, plus `etag(body: &str) -> String`, which leaves out `DTSTAMP`.
  - `feed_url.rs`:
    - `validate_feed_url(raw: &str, own_listing_id: Uuid) -> Result<url::Url, FeedUrlError>`.
    - `ip_allowed(ip: IpAddr) -> bool`. It works on `url::Host::Ipv4`/`Host::Ipv6`, never on `host_str().parse()`.
    - `redact(&Url) -> String`, the host only.

    The full blocklist, used by both the URL check and the resolver:
    - IPv4: `0.0.0.0/8`, `10/8`, `100.64/10`, `127/8`, `169.254/16`, `172.16/12`, `192.0.0/24`, `192.0.2/24`, `192.168/16`, `198.18/15`, `198.51.100/24`, `203.0.113/24`, `224/4` multicast, `240/4` and `255.255.255.255`.
    - IPv6: `::/128`, `::1`, `::ffff:0:0/96` (the embedded IPv4 is checked again), `::/96` IPv4-compatible, `64:ff9b::/96` NAT64, `2002::/16` 6to4, `fc00::/7`, `fe80::/10`, `ff00::/8` and `2001:db8::/32`.
    - Metadata: `169.254.169.254` and `fd00:ec2::254`, which are **always** blocked.
  - `conflict.rs`: `ConflictKind::classify(block, booking, gap)` and `stale_reason(...)`.
  - DTOs in `models.rs` (each `ToSchema`): `CalendarFeedResponse`, `NewCalendarFeedRequest`, `UpdateCalendarFeedRequest`, `BlockedDateResponse`, `NewBlockedDateRequest`, `CalendarConflictResponse { …, stale_reason, reopened }`, `FeedSyncResult { feed_id, outcome: SyncOutcome /*ok|warning|failed|skipped|deferred*/, events, error_code, conflicts_opened }`, `CalendarOverview`, `Page<T>`.
- `common/src/email.rs`: `EmailTemplate::{CalendarConflictHost, CalendarFeedFailingHost}`, with their payload structs.
- `FeedError`, `FeedUrlError` and `FetchError` are `thiserror` enums with stable codes, and their `Display` never contains a URL.

### 3.3. Database Layer (`db_core`)
- **New `db_core/src/calendar/`**. Test modules are siblings declared in `mod.rs`.
  - `feeds.rs`:
    - `create`: listing lock, limit and duplicate checks, then insert; `23505` maps to duplicate.
    - `list`, `get`.
    - `update`: listing → feed locks; a URL change increments `sync_generation`.
    - `delete`: listing → feed locks.
    - `claim_due(pool, min_interval, limit)` and `claim_for_listing(pool, listing_id, min_interval)`, both using the CTE in §1.4.4.
    - `release_claim(feed_id, previous_attempt_at)`.
  - `blocks.rs`:
    - `create_manual_block`: listing lock, then the exact-night overlap check with `clock_timestamp()`.
    - `delete_manual_block`.
    - `list_blocks(listing_id, from, to)`, which includes `feed_name` and `feed_active`.
  - `reconcile.rs`: `reconcile_feed(pool, claim: &FeedClaim, parsed, trigger) -> Result<ReconcileOutcome>` (§1.4.2) and `record_failure(pool, claim, error_code, trigger) -> Result<FailureOutcome>` (§1.4.3). Both queue emails inside the transaction and return the outbox ids to publish. A lock timeout (`55P03`) becomes `ReconcileOutcome::Deferred`.
  - `conflicts.rs`: `detect_for_feed(tx, feed_id, gap)` (the §1.5.2 upsert, returning opened and reopened ids), `list_conflicts(listing_id, page)` with `stale_reason` worked out in SQL, `acknowledge(conflict_id, listing_id, user_id)`, and `overview_for_user(user_id, is_admin)`, which is bounded by a `LIMIT` on the listing link list.
  - `export.rs`: `export_events(pool, listing_id, today) -> Result<Option<(String, Vec<ExportEvent>)>>`.
  - `outbox.rs`: `stuck_calendar_emails(pool, older_than, limit) -> Vec<Uuid>`.
- **Availability changes**: §1.2.4 and §1.2.5, applied to all 7 paths. The module doc in `availability.rs` is updated. `respond.rs::accept_request` gains the block check.
- **`connection.rs`**: a new `create_connection_pool_sized(url, max_connections)`. The job calls it with `job_concurrency + 2`, and the existing `create_connection_pool` behaves as before.
- **Parity tests** (`availability_parity_tests.rs`): `Kind::ImportedBlock` and `Kind::ManualBlock` rows, including gap cases, across every path. The accept path gets its own test.
- **New `DbError` variants**: `CalendarFeedLimit`, `CalendarFeedDuplicate`, `CalendarBlockManagedByFeed`, `CalendarBlockDuplicate`, `CalendarSyncRateLimited { retry_after_secs }`. Each has explicit arms in `status_code` and `error_response`.

### 3.4. Shared Sync Code (`api_core`)
`common` has no networking, and the services may not depend on each other, so the IO code shared by `listing_api` and the job lives in `api_core`.
- `api_core/src/calendar/fetch.rs`: `GuardedFetcher::new(FetcherConfig)`, where `FetcherConfig { connect_timeout, total_timeout, max_bytes, max_redirects, allow_private_hosts, extra_root_ca: Option<Certificate>, resolver: Option<Arc<dyn Resolve>> }`. The defaults are the production values; the test-only fields default to `None`.
  - **https always**, in every configuration, tests included.
  - **IP literals**: hyper connects to IP literals directly without calling `dns_resolver`. So the first URL and every redirect target that is an IP literal is checked with `ip_allowed`, both in `validate_feed_url` and in the redirect policy.
  - **Hostnames** go through the custom `dns_resolver`, which uses `tokio::net::lookup_host` and filters with `ip_allowed`. This runs at connect time, for redirects too, which closes DNS rebinding.
  - Also `no_proxy()`; a redirect policy (`Policy::custom`) capped at 3 hops that checks https and IP literals and gives `FetchError::Redirect` otherwise; and the body read through `chunk()` with the size cap.
  - Every `reqwest::Error` goes through `.without_url()` before it becomes a `FetchError`.
  - `allow_private_hosts = true` lets loopback and private ranges through. **Metadata addresses stay blocked.** The test `resolver` hook is a constructor argument only, never read from settings or env, and `ip_allowed` always wraps it.
- `api_core/src/calendar/sync.rs`: `sync_feeds(pool, publisher, fetcher, claims, trigger, concurrency, deadline) -> Vec<FeedSyncResult>`. It runs §1.4.5: fetches in parallel, reconciles one at a time per listing, parses in `spawn_blocking` above 64 KB, then publishes. It's `#[instrument(skip_all, fields(listing_id, feeds = claims.len()))]`, and each feed gets a child span with `feed_id` and `host`.
- **Settings** (`api_core::settings`): `CalendarSyncSettings { allow_private_hosts: bool /*default false*/, job_concurrency: u16 /*default 8*/, extra_root_ca_path: Option<PathBuf> }`.
  - **These are read from environment variables only** (`EA__CALENDAR_SYNC__*`). They are never written into any `config/*.toml`, because cloud services currently run with `RUN_ENV` unset, so `Development.toml` is loaded in prod.
  - `allow_private_hosts` and `extra_root_ca_path` are set only in `.env` and devenv.
  - The listing_api and job deploys set `EA__CALENDAR_SYNC__ALLOW_PRIVATE_HOSTS=false` explicitly.
  - **Startup refuses to run** (it logs at `error` and exits non-zero) when `allow_private_hosts` is `true` or `extra_root_ca_path` is set while `K_SERVICE` or `CLOUD_RUN_JOB` is present.

### 3.5. API (`listing_api`)
Every route below is under `/api/v1/listings/{id}/calendar` and requires `require_listing_manager`, except `calendar.ics`.

| Method & path | Body / query | Success | Errors |
|---|---|---|---|
| `GET /feeds` | — | 200 `[CalendarFeedResponse]` | 401, 403, 404 |
| `POST /feeds` | `{name,url}` | **201** `{feed, sync: FeedSyncResult}` | 409 `CALENDAR_FEED_LIMIT`/`_DUPLICATE`, 422 `CALENDAR_FEED_INVALID_URL`/`_SELF` |
| `PATCH /feeds/{feed_id}` | `{name?, url?, is_active?}` | 200 `{feed, sync?}` | 404, 409, 422 |
| `DELETE /feeds/{feed_id}` | — | 204 | 404 |
| `POST /sync` | — | 200 `[FeedSyncResult]` (`[]` if there are no active feeds) | 429 `CALENDAR_SYNC_RATE_LIMITED` + `Retry-After` |
| `GET /blocks` | `?from&to` (default today to +12 months, at most 24 months) | 200 `[BlockedDateResponse]` | 422 |
| `POST /blocks` | `{date_from,date_to,note?}` | 201 `BlockedDateResponse` | 409 `DATE_UNAVAILABLE`/`CALENDAR_BLOCK_DUPLICATE`, 422 |
| `DELETE /blocks/{block_id}` | — | 204 | 404, 409 `CALENDAR_BLOCK_MANAGED_BY_FEED` |
| `GET /conflicts` | `?page` | 200 `{open: [..], history: Page<..>}` | — |
| `POST /conflicts/{conflict_id}/acknowledge` | — | 200 (repeating it is harmless) | 404 |
| `GET /api/v1/listings/{id}/calendar.ics` | `If-None-Match?` | 200 `text/calendar` + `ETag`, or 304 | 404 |
| `GET /api/v1/calendar/overview` | — | 200 `CalendarOverview` (the caller's listings, all of them for the admin) | 401 |

- Child ids are always matched **together with** `listing_id` in SQL, and the composite FKs (§3.1) guarantee the parent and child agree.
- Errors use the existing `{code, message}` format. Every 5xx is logged at `error` with `listing_id`/`feed_id`, and never with a URL.
- OpenAPI: every new path and schema goes into `ApiDoc`, and `openapi.yaml` is regenerated.
- `listing_api` deploy env: `EMAIL_PUBSUB_TOPIC_ID`, `GCP_PROJECT_ID` and `EA__CALENDAR_SYNC__ALLOW_PRIVATE_HOSTS=false`. `SA_LISTING_API` gets `roles/pubsub.publisher` on the email topic.

### 3.6. Clients & Frontend
- `common/src/app_client/listings.rs` and `web_client/src/client.rs`:
  - The existing write functions gain `user_jwt: &str` (§1.8.5).
  - New `common/src/app_client/calendar.rs`: feeds, sync, blocks, conflicts and overview (each taking the user JWT), and `get_calendar_ics(listing_id, if_none_match) -> Result<IcsFetch /*Ok{body,etag}|NotModified|NotFound*/>`, which uses `get_request`, the service identity only.
- **`web_ui_guest`**: `pages/calendar_export.rs` adds `#[route(GET "/listings/{id}/calendar.ics")]`.
  - It parses the UUID (anything else is a 404), checks the §1.6.4 cache, then calls `get_calendar_ics`.
  - It returns the §1.6.3 headers and maps an upstream 304 or 404 to the same status.
  - Any other upstream failure gives **503** plain text, logged at `error`, without `Cache-Control`.
  - Inbound request headers are never forwarded.
- **`web_ui_admin`**:
  - `pages/listings/calendar.rs`: the page plus HTMX fragment routes `POST /admin/listings/{id}/calendar/{feeds | feeds/{fid}/edit | feeds/{fid}/toggle | feeds/{fid}/remove | sync | blocks | blocks/{bid}/remove | conflicts/{cid}/ack}`, each returning its section partial.
  - A nav link on the listing page, and the card in `dashboard.rs`.
  - The existing write call sites (`listings/edit.rs`, `listings/create.rs`, `listings/images.rs`, `pricing.rs`) pass `service_token(cx, user)`.
  - The admin portal reads `GUEST_PORTAL_URL`.

### 3.7. Background Workers & Deployment
- **New crate `app_api/calendar_sync_job`** (a workspace member, binary `calendar_sync_job`):
  - `main` calls `run(config, deps) -> anyhow::Result<RunSummary>`. It's injectable, so tests can call it, and `main` maps `Err` to a non-zero exit.
  - `run` does: settings and the cloud guard (§3.4), a sized pool (no migrations), the publisher, the claim and sync loop (§1.4.8), republishing stuck calendar emails, the summary, and a telemetry flush.
- `Dockerfile.calendar_sync_job`, modelled on `Dockerfile.email_worker` (cargo-chef, `--bin calendar_sync_job`, no `EXPOSE`, **no `config/` copy** needed beyond the base settings).
- **Email templates** in `app_api/email_worker/templates/`: `calendar_conflict_host.html` and `calendar_feed_failing_host.html`, registered in `processor.rs`, each value going through `escape_html`. Payload keys: `listing_name`, `feed_name`, `date_from`, `date_to`, `confirmation_code`, `kind`, `reopened`, `calendar_url`, `error_code`.
- **Service account, created once by hand** (documented in `docs/ENVIRONMENT.md`; CI never creates service accounts):
  ```bash
  gcloud iam service-accounts create calendar-sync-job-<env> --display-name="Calendar sync job (<env>)"
  gcloud projects add-iam-policy-binding $PROJECT_ID --member=serviceAccount:calendar-sync-job-<env>@… --role=roles/cloudsql.client
  gcloud pubsub topics add-iam-policy-binding <email-topic> --member=serviceAccount:calendar-sync-job-<env>@… --role=roles/pubsub.publisher
  gcloud iam service-accounts add-iam-policy-binding calendar-sync-job-<env>@… --member=serviceAccount:<CI deployer SA> --role=roles/iam.serviceAccountUser
  gcloud iam service-accounts add-iam-policy-binding calendar-sync-job-<env>@… --member=serviceAccount:service-<PROJECT_NUMBER>@gcp-sa-cloudscheduler.iam.gserviceaccount.com --role=roles/iam.serviceAccountTokenCreator
  # then set the repo variables INFRA_SA_CALENDAR_JOB_DEV / INFRA_SA_CALENDAR_JOB_PROD
  ```
- **CI (`.github/workflows/cicd_full_stack.yml`)**:
  1. `SA_CALENDAR_JOB: ${{ vars[format('INFRA_SA_CALENDAR_JOB_{0}', …)] }}`. If it's empty, a guard step **fails** with the bootstrap command shown above.
  2. Build and push `calendar-sync-job:${sha}`, using the cargo-chef cache scope `calendar-sync-job`.
  3. On **every branch**:
     ```bash
     gcloud run jobs deploy calendar-sync-$SUFFIX --image … --region $REGION \
       --service-account $SA_CALENDAR_JOB@… --set-cloudsql-instances … \
       --set-env-vars <EA__DATABASE__* as listing_api>,EMAIL_PUBSUB_TOPIC_ID=…,GCP_PROJECT_ID=…,EA__CALENDAR_SYNC__ALLOW_PRIVATE_HOSTS=false \
       --task-timeout=600s --max-retries=0 --memory=256Mi --cpu=1 --quiet
     ```
     **No `JWT_SECRET`, `GCS_*` or SMTP variables** go into the job's environment.
  4. On **`main` only** (`if: github.ref == 'refs/heads/main'`):
     ```bash
     gcloud run jobs add-iam-policy-binding calendar-sync-$SUFFIX --region $REGION --member=serviceAccount:$SA_CALENDAR_JOB@… --role=roles/run.invoker
     gcloud scheduler jobs create|update http calendar-sync-$SUFFIX --location=$REGION --schedule="*/15 * * * *" --time-zone=UTC \
       --uri="https://run.googleapis.com/v2/projects/$PROJECT_ID/locations/$REGION/jobs/calendar-sync-$SUFFIX:run" \
       --http-method=POST --oauth-service-account-email=$SA_CALENDAR_JOB@… --attempt-deadline=60s --max-retry-attempts=0
     ```
     This follows the create-or-update pattern of "Schedule Hourly Booking Sweeps". The free-tier comment is updated to read "sweeps (dev and main) plus calendar sync (main) = 3".
  5. The listing_api deploy gets the §3.5 env and IAM, and the admin portal deploy gets `GUEST_PORTAL_URL`.
  6. An `actionlint` step (`docker run --rm -v "$PWD":/repo -w /repo rhysd/actionlint:1.7.7`) runs in the lint job.
- **`cleanup_feature.yml`**: `gcloud run jobs delete calendar-sync-$SUFFIX --region $REGION --quiet || echo …`.
- **Manual trigger on non-prod** (`docs/ENVIRONMENT.md`, `ARCHITECTURE.md` §7): `gcloud run jobs execute calendar-sync-<suffix> --region <region> --wait`, or "Sync now" in the portal.

---

## 4. Performance Considerations
| Concern | Budget and approach |
|---|---|
| Availability queries | One more `EXISTS`. The bounding predicates (§1.2.4) let `idx_blocked_dates_listing_range` narrow by `listing_id` and date, and manual blocks building up over time also hit `idx_blocked_dates_listing_to`. The search `NOT EXISTS` per listing uses the same index. The #116 query-count assertion guards against N+1. |
| Reconcile | O(events) with one `UNNEST` upsert, one `DELETE`, and one conflict upsert joined on `(listing_id, date range)`. The listing lock is held for milliseconds, and `lock_timeout` is 5 s. |
| Pool | `listing_api` has a pool of 5. "Sync now" holds **1** connection, because reconciles run one at a time per listing and fetches hold none. The job's pool is `concurrency + 2`. |
| Parse | Linear in body size, ≤ 2 MB. `spawn_blocking` above 64 KB. |
| Export | One `UNION ALL` over bookings and blocks for one listing, bounded to 24 months. The ETag gives 304s, and the guest portal's 60 s cache, which also caches 404s, covers repeated and random-id requests. |
| "Sync now" latency | 20 s handler deadline, and unfinished feeds return `deferred`. `AuthenticatedClient` uses `Client::new()`, which has no timeout, so the portal's call to this endpoint uses a per-request `.timeout(25s)`. |
| Job | Pages of 32, with the deadline checked before each feed. **Known limit, recorded in ARCHITECTURE §7.** At MVP scale (≤ 25 feeds) a run takes seconds. At 1000× (25k feeds), 8 workers with 1 s fetches handle about 4k feeds per run, so the refresh interval grows to about 90 minutes and the "4 failures" alert to about 6 hours. Concurrency is configurable, and moving to per-listing fan-out is a follow-up. All fetches come from one egress IP. |
| Cold start | No new startup work in `listing_api`: the fetcher is built lazily with `OnceLock`. No heavy dependencies. Re-measured in T27. |

## 5. Security Mitigations
| Threat | Mitigation |
|---|---|
| **SSRF** (A10) | https in every configuration. IP literals checked with `url::Host` (hyper skips the resolver for literals). Hostnames go through the resolver filter at connect time, which closes rebinding. Redirects capped at 3, each checked again. `no_proxy`. Full IPv4 and IPv6 blocklist, including mapped, NAT64 and 6to4 addresses. Metadata IPs always blocked. 2 MB and 10 s caps. Dev toggles come from env only, startup refuses them on Cloud Run, and deploys set them `false`. |
| **IDOR** | `require_listing_manager` checks 401 before the lookup, then 404 or 403. Child ids are filtered by `listing_id`, and composite FKs back that up. Tests cover host A against host B for every endpoint. |
| **Broken access control** on existing writes (A01) | §1.8 adds server-side checks. Admin-set owners are validated. |
| **PII leak** through the export | Busy-only fields. A test asserts that a booking with a known guest name, email and note exports none of them. Names are stripped of control characters and escaped. Imported text is never stored. |
| **Occupancy exposure** through the public export | **Accepted risk** (D1, originator decision 2026-11-09). The export reveals booked dates and reservation numbers to anyone who has the listing UUID. `confirmation_code` isn't a credential: no unauthenticated lookup uses it. Note that adding a token later would break every URL already pasted into a channel. |
| **Feed URL secrets** | `.without_url()` on every reqwest error. A redacting `Debug` on the feed row type. `23505` mapped before logging. `instrument(skip_all)`. Tests assert that captured logs never contain the URL's path or query. |
| **ICS injection and parser DoS** | No recursion, 8 KB lines at most, 5,000 events at most, a window on dates, unique and bounded UIDs, and every exported value escaped and folded. |
| **Export abuse** | ETag and 304, the guest-portal cache including 404s, a bounded query, and a UUID path. |
| **Request amplification through feed create or URL change** | Both go through the 60 s claim. |
| **SQL injection** | Only `sqlx::query!`. Search keeps using `push_bind`. |
| **Job identity** | The dedicated `SA_CALENDAR_JOB` has only Cloud SQL client, Pub/Sub publisher, and `run.invoker` on its own job, for the Scheduler. The job has no HTTP endpoint and no JWT or GCS secrets in its environment. The database password is a plain env var, which is the existing pattern; Secret Manager is a follow-up. |

---

## 6. Test Plan

### Unit Tests (`cargo test -p common`)
- `calendar::ics_parse_tests`:
  - **Fixtures**: Airbnb and VRBO (in `common/tests/fixtures/ical/`).
  - **Encoding**: folded lines, escaping, LF-only input, lowercase property names, a BOM, invalid UTF-8.
  - **Dates**: `DATE`, a DATE with an inverted range, `T000000Z` taken as a DATE, `DATE-TIME` with UTC, `TZID` or floating time, an exact-midnight end, `DURATION`, a missing `DTEND`, a missing `DTSTART`, and a `DTSTART` inside `VTIMEZONE` or `VALARM` (ignored).
  - **Event handling**: `CANCELLED`, an `RRULE` producing a warning, an `@ourplaces` UID skipped, the window with only `date_to` clipped, Feb 29 + 24 months.
  - **UIDs**: a missing UID, duplicates numbered in date order (stable when the feed reorders), a derived name colliding with a real UID, a UID over 500 bytes, control characters.
  - **Failures**: more than 5,000 events, a line over 8 KB, no `VCALENDAR`, a **truncated** body, an unbalanced `VEVENT`, an empty calendar.
- `calendar::ics_write_tests`: escaping, including a UID containing `,;\` and newlines; 75-octet folding that never splits a multibyte character; CRLF; required headers; the ETag ignoring `DTSTAMP`; round trip.
- `calendar::feed_url_tests`:
  - Refused: http, user info, the self URL.
  - Every blocklist range, including `[::ffff:7f00:1]`, `2130706433`, `0x7f.1`, `017700000001`, `64:ff9b::7f00:1` and `2002:7f00:1::`.
  - Allowed: another listing's export URL.
  - `redact` returns the host only.
- `calendar::conflict_tests`: `classify` with gaps of 0, 1 and 3, and `stale_reason`.
- `reference::tests::listing_today` at the UTC−5 boundary.

### Integration Tests
These run through `devenv shell --`.
- `db_core` `calendar::reconcile_tests`:
  - Insert, date change, removal, and running twice with the same result.
  - A failure keeps blocks, the count rises for scheduled runs only, the alert fires once at 4 and is re-armed.
  - The E2 warning.
  - A generation mismatch or deleted feed does nothing (E5).
  - A lock timeout gives `Deferred` (E26).
  - Normalised UIDs never violate a constraint.
  - Two reconciles of one feed run one after the other.
- `db_core` `calendar::conflict_tests`:
  - Overlap and turnover detected; an expired hold gives none; re-sync gives no new row or email.
  - Remove then re-add reopens with one email.
  - The same UID on two feeds gives two rows.
  - Turnover to overlap reopens an acknowledged conflict with an email.
  - Each stale reason, acknowledging, and pagination.
  - **Race**: a hold and a reconcile give either a refused hold or a recorded conflict.
  - A conflicted pending hold still confirms.
- `db_core` `calendar::feeds_tests`:
  - The limit, a duplicate (including the `23505` path), IDOR by id, and a URL change increments the generation.
  - Concurrent `claim_due` never double-claims.
  - A claim inside the interval returns nothing.
  - `release_claim`.
  - Deleted listings are skipped.
- `db_core` `calendar::block_tests`:
  - Refused over a stay, hold or request; allowed over gap days and over other blocks.
  - Deleting an imported block gives `ManagedByFeed`.
  - A duplicate gives `CalendarBlockDuplicate`.
- `db_core` `calendar::export_tests`: contents and exclusions, raw dates, a deleted listing gives `None`, an inactive listing is served.
- `db_core` `calendar::outbox_tests`: stuck calendar emails are selected, and other templates are not.
- `db_core` `booking::availability_parity_tests`: block rows across all 7 paths. The §1.2.5 shorten case. `booking_request::respond_tests` covers accept over a block.
- `api_core` `calendar::fetch_tests`:
  - A local **https** server with an `rcgen` certificate and an injected root CA. An injected `Resolve` for the name-to-private-IP cases.
  - Injected timeouts of about 200 ms.
  - Covered: the happy path, the default refusing loopback, an IP literal skipping the resolver yet still refused, a redirect to http, a redirect to an internal https host, more than 2 MB, a timeout, the metadata IP blocked even with `allow_private_hosts`, the `Accept`/`User-Agent` headers, and no URL in the `FetchError` message.
- `api_core` `calendar::settings_tests`: the startup guard refuses `allow_private_hosts`/`extra_root_ca_path` when `K_SERVICE` or `CLOUD_RUN_JOB` is set.
- `api_core` `calendar::sync_tests`: a mix of ok, failing and malformed feeds; reconciles of one listing run one at a time (pool high-water mark ≤ 1 for that listing); the deadline gives `deferred`; publish runs after commit; a publish failure is logged at `error`; captured logs contain no URL path.
- `listing_api` `calendar_tests`:
  - Every endpoint, with 401, 403 (host A against B), 404 for a child of another listing, 409, 422, and 429 with `Retry-After`.
  - PATCH URL: validated, generation incremented, re-synced, old blocks kept on failure.
  - Toggling off keeps blocks.
  - Feed create is rate-limited.
  - The `.ics` content type, headers, ETag and 304, and PII.
- `listing_api` `apis::tests` and `authz_tests`: §1.8 for every existing write; owner validation on create.
- `calendar_sync_job` `run_tests`: the summary and exit code, an unreachable DB returning `Err`, the deadline releasing unstarted claims, and republishing.
- Portals:
  - `web_ui_guest` `calendar_export`: 200, 304, 404, 503, the cache with negative entries, and inbound XFA not forwarded. The mocks use `wiremock`, added to the dev-deps.
  - `web_ui_admin` `listings::calendar`: fragment renders.
  - `web_ui_admin` `pages::dashboard::tests`: the card.

### E2E Tests (Playwright; config `playwright/playwright.config.ts`)
- `playwright/e2e/admin-portal/calendar-sync.spec.ts` (`admin-portal-chromium`):
  - Uses a Node **https** fixture server started in global setup, with a self-signed CA passed to listing_api through `EA__CALENDAR_SYNC__EXTRA_ROOT_CA_PATH`, and `ALLOW_PRIVATE_HOSTS=true` in the local env.
  - Steps: add a feed, check its status is `ok` and the blocks appear; "Sync now" twice to get the rate-limit message; add and remove a manual block; acknowledge a seeded conflict.
- `playwright/e2e/guest-portal/calendar-export.spec.ts` (`guest-portal-chromium`): `.ics` content type, `Reservation #` present and the guest email absent; a blocked night greyed out and refused at checkout.
- `playwright/e2e/api/listing-api.spec.ts` (`backend-api`): JWTs, plus the 401 and 403 cases.

### Gates
- `cargo fmt --check`
- `SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features -- -D warnings`, and again with `-D missing_docs`
- `cargo sqlx prepare --workspace --check`
- The `CONSTRAINTS.md` greps
- `cargo audit`
- `actionlint`

Targeted tests run while iterating, and the full suite once at the end. The sanity-check wall time is recorded against the 90 s target.

---

## 7. Acceptance Criteria
- [ ] 1. The migration creates the 3 tables with the §3.1 constraints, the composite FKs and the indexes. `sqlx prepare` is up to date.
- [ ] 2. A feed fixture with N future events produces N blocks, and a second sync changes nothing.
- [ ] 3. A removed event is released on the next successful sync, and an event whose dates changed updates its block. A truncated or malformed body releases nothing.
- [ ] 4. Fetch, size and parse failures keep blocks and record a sanitised error code. 4 failed scheduled syncs in a row send exactly one email, and a success re-arms it.
- [ ] 5. Imported blocks (with the gap) and manual blocks (without) are refused or excluded by all 7 availability paths, including accept, which the parity tests prove. A conflicted booking can still be shortened.
- [ ] 6. A manual block over a stay, hold or request is refused with 409 `DATE_UNAVAILABLE`. Blocks may overlap each other, and exact duplicates give 409.
- [ ] 7. An imported event over a live hold or confirmed stay opens a conflict with one email. Removing and re-adding the event, or a turnover becoming an overlap, reopens it with one email. Bookings are untouched, and a pending hold can still be confirmed.
- [ ] 8. A conflict stays until acknowledged and shows a stale reason once either side goes away.
- [ ] 9. A hold racing a sync never produces an unrecorded double-booking. A URL change during a fetch never applies the old URL's events.
- [ ] 10. `GET <guest>/listings/{id}/calendar.ics` returns valid RFC 5545 with confirmed stays, live holds, manual blocks, and imported blocks with their original `UID`, plus `Reservation #` for bookings. It contains no guest PII, and it has an ETag and 304 support.
- [ ] 11. "Sync now" syncs inline within 20 s while holding at most 1 pool connection, and returns a 429 with `Retry-After` when repeated within 60 s. Feed create and URL change share that limit.
- [ ] 12. The SSRF guard refuses http, private, loopback, metadata, mapped and encoded IPs, user info and the self URL, and checks redirects again. Dev toggles are refused on Cloud Run. No feed URL path or query appears in logs.
- [ ] 13. Every `listing_api` write, existing and new, returns 401 without a JWT and 403 for a host who doesn't own the listing. The portal and Playwright send JWTs.
- [ ] 14. The admin calendar page manages feeds, blocks and conflicts. The dashboard shows open conflicts and failing feeds.
- [ ] 15. CI deploys `calendar-sync-$SUFFIX` with `SA_CALENDAR_JOB` on every branch. The 15-minute Scheduler exists **only** on `main`, there are ≤ 3 Scheduler jobs in total, and `cleanup_feature.yml` deletes the job. Stuck calendar emails are republished.
- [ ] 16. The `CONSTRAINTS.md` gates and `actionlint` pass, and `listing_api` cold start p50 is under 300 ms.
