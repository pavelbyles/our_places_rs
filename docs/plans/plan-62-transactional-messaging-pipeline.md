# Engineering Plan #62: Transactional Messaging Pipeline (Pre-Arrival Guide & Notifications)

## Metadata
- **Feature/Issue**: #62 Transactional Messaging Pipeline (Pre-Arrival Guide & Notifications)
- **Branch**: `feat/62-transactional-messaging-pipeline-pre-arrival-guide-not` (Base: `main`)
- **Status**: Locked (Approved for Execution)
- **Target Crates**: `common`, `db_core`, `app_api/booking_api`, `app_api/email_worker`, `app_api/api_core`, `web_app_admin_tc`, `infra`
- **Blast Radius**: 10 files across shared models, database schema, API services, worker templates, and admin portal.

---

## 1. Architecture Review & Component Boundaries

### 1.1. ASCII Data Flow Diagram
```
+---------------------------------------------------------------------------------------------------+
| 1. EVENT-DRIVEN NOTIFICATIONS (Confirmation, Material Update, Cancellation)                      |
|                                                                                                   |
|  Client (Web / Admin)                                                                             |
|      |                                                                                            |
|      | POST /api/v1/bookings/booking/{id} (status: confirmed, cancelled, or updated)             |
|      v                                                                                            |
|  Actix Service (app_api/booking_api)                                                              |
|      |                                                                                            |
|      | 1. Execute state transition within PostgreSQL transaction                                  |
|      | 2. Atomic Idempotency Check: INSERT INTO booking_notification_log                          |
|      |    ON CONFLICT (booking_id, notification_type, recipient_user_id) DO NOTHING               |
|      | 3. If log inserted: INSERT INTO email_outbox (guest and/or host records)                   |
|      | 4. Commit DB Transaction                                                                   |
|      | 5. Async Non-blocking Publish: api_core::EmailPublisher::publish_email_event(email_id)     |
|      v                                                                                            |
|  GCP Pub/Sub (email-notifications-topic) ---> app_api/email_worker (Renders HTML & Dispatches)    |
+---------------------------------------------------------------------------------------------------+

+---------------------------------------------------------------------------------------------------+
| 2. SCHEDULED NOTIFICATIONS (48-Hour Pre-Arrival, 48-Hour Host Alert, 2-Hour Hold Expiry)         |
|                                                                                                   |
|  GCP Cloud Scheduler (Recurring Hourly Cron: 0 * * * *)                                           |
|      |                                                                                            |
|      | POST /api/v1/internal/cron/process-scheduled-notifications (Header: Bearer CRON_SECRET)    |
|      v                                                                                            |
|  Actix Service (app_api/booking_api)                                                              |
|      |                                                                                            |
|      | 1. Authenticate CRON_SECRET token                                                          |
|      | 2. Query 48h Confirmed Bookings:                                                           |
|      |    SELECT b.* FROM booking b WHERE b.status = 'confirmed'                                  |
|      |      AND b.date_from = CURRENT_DATE + INTERVAL '2 days'                                    |
|      |      AND NOT EXISTS (SELECT 1 FROM booking_notification_log WHERE booking_id = b.id ...)   |
|      | 3. Query Expiring 2h Holds:                                                                |
|      |    SELECT b.* FROM booking b WHERE b.status = 'pending'                                    |
|      |      AND b.created_at BETWEEN NOW() - INTERVAL '105 minutes'                               |
|      |                          AND NOW() - INTERVAL '75 minutes'                                 |
|      |      AND NOT EXISTS (SELECT 1 FROM booking_notification_log WHERE booking_id = b.id ...)   |
|      | 4. For each candidate:                                                                     |
|      |    - Atomically log to booking_notification_log                                            |
|      |    - Insert corresponding email_outbox record                                              |
|      |    - Emit Pub/Sub notification event                                                       |
|      v                                                                                            |
|  GCP Pub/Sub (email-notifications-topic) ---> app_api/email_worker (SMTP Delivery)                 |
+---------------------------------------------------------------------------------------------------+
```

---

### 1.2. State Machine Diagram

Notification lifecycle mapping against booking statuses and recipient permissions:

```
+---------------------------------------------------------------------------------------------------+
| Booking State Transitions & Notification Dispatch Matrix                                          |
|                                                                                                   |
|  [Guest Initiates Checkout]                                                                       |
|         |                                                                                         |
|         v                                                                                         |
|    +---------+                                                                                    |
|    | Pending | ---- (At ~90m / 30m before 2h expiry) ---> [Guest ONLY: PaymentHoldExpiryReminder]   |
|    +----+----+                                                                                    |
|         |                                                                                         |
|         +-- (Payment Confirmed) -------------------------------------------------------------+     |
|         |                                                                                    |     |
|         v                                                                                    |     |
|   +-----------+                                                                              |     |
|   | Confirmed | <----------------------------------------------------------------------------+     |
|   +-----+-----+                                                                                    |
|         | ===> 1. [Guest: BookingConfirmationGuest] & [Host: BookingConfirmationHost]              |
|         |                                                                                          |
|         | ---> (Material Change: Dates/Guests/Rate)                                                |
|         |      ===> [Guest: BookingUpdatedGuest] & [Host: BookingUpdatedHost]                      |
|         |                                                                                          |
|         | ---> (T-48h to Arrival Date: date_from)                                                  |
|         |      ===> [Guest ONLY: PreArrivalGuideGuest] (WiFi, Door Access Code, GPS Directions)   |
|         |      ===> [Host ONLY: HostUpcomingArrival] (Guest ETA, Party Size, Turnover Checklist)  |
|         |                                                                                          |
|         +-- (Cancellation by Guest/Host/Admin) ---------------------------------------------+     |
|         |                                                                                   |     |
|         v                                                                                   |     |
|   +-----------+                                                                             |     |
|   | Cancelled | <---------------------------------------------------------------------------+     |
|   +-----------+                                                                                    |
|         ===> [Guest: BookingCancelledGuest] & [Host: BookingCancelledHost]                         |
+---------------------------------------------------------------------------------------------------+
```

---

### 1.3. Component Boundaries & Interfaces

| Component | Responsibility & Interface Contracts |
|---|---|
| **`db_core`** | • Migration: Add `booking.door_access_code TEXT DEFAULT NULL`.<br>• Migration: Create table `booking_notification_log (id, booking_id, notification_type, recipient_user_id, sent_at)` with unique index `(booking_id, notification_type, recipient_user_id)`.<br>• Migration: Update hold cleanup interval from 15 minutes to 2 hours.<br>• Query: `claim_and_log_notification(pool, booking_id, notification_type, recipient_user_id) -> Result<bool>`.<br>• Query: `get_bookings_for_48h_notifications(pool) -> Result<Vec<BookingPreArrivalContext>>`. |
| **`common`** | • `EmailTemplate` variants: `BookingConfirmationGuest`, `BookingConfirmationHost`, `BookingUpdatedGuest`, `BookingUpdatedHost`, `BookingCancelledGuest`, `BookingCancelledHost`, `PreArrivalGuideGuest`, `HostUpcomingArrival`, `PaymentHoldExpiryReminder`.<br>• Strongly typed DTO payloads for all templates in `common::email`.<br>• Helper: `is_material_booking_change(old: &Booking, updated: &UpdatedBooking) -> bool`. |
| **`app_api/booking_api`** | • Lifecycle Hooks: Dispatch confirmation, update, and cancellation notifications.<br>• Endpoint: `PATCH /api/v1/bookings/booking/{id}` (`UpdatedBookingRequest` includes optional `door_access_code`, validated via host/admin authorization guard).<br>• Internal Sweep Endpoint: `POST /api/v1/internal/cron/process-scheduled-notifications` (Hosted natively by `booking_api`, protected by `CRON_SECRET` header / OIDC Bearer token). |
| **`app_api/email_worker`** | • Templates: 7+ responsive HTML templates in `app_api/email_worker/templates/`.<br>• Processor: Updated `render_email_body` mapping all template variants safely.<br>• SMTP Dispatch: Letter/Mock provider with decoupled I/O and exponential backoff. |
| **`web_app_admin_tc`** | • Booking Admin View: Host-only editable input field for `door_access_code` with HTMX inline submit.<br>• Listing Settings View: Listing-level default check-in instructions, WiFi SSID, and WiFi password. |
| **`infra`** | • Terraform/Pulumi: GCP Cloud Scheduler cron resource (`0 * * * *`) targeting the Cloud Run `booking_api` internal sweep endpoint. |

---

### 1.4. Error Paths & Failure Recovery Strategies

| Failure Point | Detection Mechanism | Recovery / Blast Radius Mitigation |
|---|---|---|
| **Pub/Sub Publish Fails after DB Commit** | `publisher.publish_email_event(id).await` returns `Err` | Record is already persisted in `email_outbox` with status `pending`. Subsequent periodic sweep or retry worker processes pending records. |
| **Concurrent Sweep Invocations** | Two Cloud Scheduler pings overlap or race | `INSERT INTO booking_notification_log ... ON CONFLICT DO NOTHING` ensures only the first query instance gets a truthy insert; second instance receives 0 rows and exits. |
| **Invalid/Missing Host or Guest Email** | `validator::validate_email` fails on recipient string | Log warning, abort outbox insert for invalid recipient; do not fail the parent booking transaction. |
| **Host Never Set Door Access Code** | `booking.door_access_code` is `None` at T-48h | Template falls back to listing-level default lockbox instructions (`"Contact host upon arrival"`), preventing template render panic or missing email. |
| **SMTP Delivery Timeout (> 3s)** | Tokio timeout in `email_worker` processor | Worker executes in-app exponential backoff up to `MAX_EMAIL_RETRIES` (3). If exhausted, marks `email_outbox.status = 'failed'` without DLQ poisoning. |
| **Stale Date Shift (Arrival shifted back by guest)** | Guest changes arrival date after T-48h email was already sent | Idempotency log already recorded for original arrival. System flags or optionally triggers `BookingUpdated` diff email with revised itinerary. |

---

## 2. Edge Case Analysis Matrix

| Scenario | Domain Category | Potential Risk | Planned Technical Mitigation |
|---|---|---|---|
| **Arrival Date Rescheduled (Date Shift)** | Edge Case / Data Flow | Cloud Scheduler triggers at wrong time or triggers twice | Cloud Scheduler stores zero state. The hourly sweep query dynamically inspects `booking.date_from = CURRENT_DATE + INTERVAL '2 days'`. Rescheduled bookings are evaluated against their active date at sweep time. |
| **Booking Cancelled Prior to T-48h** | Cancellation Flow | Guest receives access code for cancelled stay | Sweep query strictly filters `status = 'confirmed'`. Cancelled bookings are dropped immediately from pre-arrival queries. |
| **Unauthenticated / Guest Attempt to Edit Door Code** | Security & Auth | Guest tampering with access credentials | Handled by Actix middleware & claims extractor: endpoint rejects request with `403 Forbidden` unless caller is authenticated host of that listing or platform admin. |
| **Concurrent Booking Updates** | Concurrency | Race condition causes duplicate update emails | Row-level lock (`SELECT ... FOR UPDATE`) in `update_booking` serializes updates. `is_material_booking_change` compares snapshots. |
| **Guest Books < 48 Hours in Advance (Last-Minute Booking)** | Edge Case / Timing | T-48h sweep has already passed for that date | When a confirmed booking is created with `date_from <= CURRENT_DATE + INTERVAL '2 days'`, the confirmation flow immediately bundles and dispatches the pre-arrival access details or triggers the pre-arrival guide in tandem. |
| **Null/Empty WiFi Credentials on Listing** | Data Integrity | Template displays broken `{{wifi_ssid}}` syntax | `render_email_body` handles `None` with fallback text `("Provided upon check-in")`. |
| **2-Hour Hold Checkout Abandonment** | Lifecycle / Timeout | Stale hold locks villa inventory indefinitely | Database `pg_cron` cleanup job safely transitions `pending` holds older than 2 hours to `cancelled` and records immutable audit history. |

---

## 3. Comprehensive Test Matrix

| Test Scenario | Test Type | Target Crate / Service | Priority | Status |
|---|---|---|---|---|
| `EmailTemplate` Display and Serde for All 7+ Variants | Unit | `common` | P0 | ☐ Planned |
| `is_material_booking_change` Diff Evaluator (Date, Guests, Total Price) | Unit | `common` | P0 | ☐ Planned |
| HTML Template Rendering with Populated & Null Payload Fallbacks | Unit | `app_api/email_worker` | P0 | ☐ Planned |
| Migration: `door_access_code` & `booking_notification_log` Table Creation | Integration | `db_core` | P0 | ☐ Planned |
| Atomic Idempotency Logging (`claim_and_log_notification` under concurrency) | Integration | `db_core` | P0 | ☐ Planned |
| 48-Hour Pre-Arrival Sweep Query Correctness (Matches T-48h, ignores cancelled) | Integration | `db_core` | P0 | ☐ Planned |
| 2-Hour Payment Hold Expiry Reminder Sweep Query | Integration | `db_core` | P1 | ☐ Planned |
| Booking Confirmation Triggers Dual Outbox Records (Guest & Host) | Integration | `app_api/booking_api` | P0 | ☐ Planned |
| Booking Material Update Triggers Dual Outbox Records | Integration | `app_api/booking_api` | P1 | ☐ Planned |
| Booking Cancellation Triggers Dual Outbox Records | Integration | `app_api/booking_api` | P0 | ☐ Planned |
| Host-Only Door Access Code Update Authorization (`403` vs `200`) | Integration | `app_api/booking_api` | P0 | ☐ Planned |
| Internal Cron Endpoint Authentication (`401` vs `200`) | Integration | `app_api/booking_api` | P0 | ☐ Planned |
| End-to-End Email Dispatch Pipeline via Mock Email Provider | Integration | `app_api/email_worker` | P0 | ☐ Planned |

---

## 4. Security & Compliance Review

1. **Authentication & Authorization**:
   - Host-only door access code endpoint (`PATCH /api/v1/bookings/booking/{id}/door-code`) verifies that `claims.sub` matches the listing owner (`listing.user_id`) or possesses `admin` role. Guests receive `403 Forbidden`.
   - Internal sweep endpoint (`POST /api/v1/internal/cron/process-scheduled-notifications`) verifies `Authorization: Bearer <CRON_SECRET>` or GCP OIDC token issued to Cloud Scheduler service account.
2. **Access Code Isolation**:
   - Door access codes are **never** included in public listing GET responses or search endpoints.
   - Access codes are suppressed in guest booking endpoints until 48 hours prior to confirmed check-in.
   - Door codes are omitted entirely from host email templates (sent to guest only).
3. **Input Validation**:
   - `door_access_code` is validated for length ($\le 64$ chars) and sanitized against script/markup injection.
   - All email addresses are verified with `validator::validate_email` before outbox persistence.
4. **Panic-Free Guarantee**:
   - Zero `.unwrap()` or `.expect()` calls in production API handlers or worker rendering pipelines.

---

## 5. Review Verdict & Dimensional Scores

| Dimension | Score (0-10) | Evaluation & Justification |
|---|---|---|
| **Architecture Clarity** | **10 / 10** | Clear separation: Cloud Scheduler as stateless cron trigger, PostgreSQL as single source of truth, Pub/Sub for asynchronous SMTP dispatch. |
| **Error Handling Completeness** | **9.5 / 10** | Atomic idempotency via unique constraints, decoupled SMTP timeouts, fallback template rendering, and safe retry caps without DLQ thrashing. |
| **Test Coverage Plan** | **9.5 / 10** | Comprehensive matrix covering unit serialization, SQLx query correctness, concurrency deduplication, auth guards, and end-to-end worker delivery. |
| **Security Posture** | **10 / 10** | Strict host-only door code editing, role-isolated template payloads, internal endpoint bearer protection, and PII log redaction. |
| **Performance & Scale-to-Zero** | **10 / 10** | 100% Cloud Run scale-to-zero compliant: zero persistent async sleep loops, bounded batch queries, and sub-300ms p50 cold start budget. |

**Overall Verdict**: **APPROVED & LOCKED (Score: 9.8 / 10)**.
Ready to proceed with execution.
