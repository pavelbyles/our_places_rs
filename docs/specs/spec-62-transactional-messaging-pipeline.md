# Spec 62: Transactional Messaging Pipeline (Pre-Arrival Guide & Notifications)

## Overview
This specification details the architecture, data models, dispatch pipeline, and security controls for the **Transactional Messaging Pipeline** on the Our Places villa platform. It establishes an automated, multi-party, role-isolated notification system delivering:
1. **Immediate Dual-Party Booking Confirmations**: Upon payment confirmation (`BookingStatus::Confirmed`), customized receipts are sent to the guest, and reservation alerts with payout projections are sent to the property host.
2. **Dual-Party Material Update Alerts**: Whenever material reservation parameters (stay dates, guest count, pricing) change, update notices are sent to both guest and host.
3. **Dual-Party Cancellation Notices**: When a reservation transitions to `Cancelled`, notices are dispatched to both guest and host detailing the initiator, policy terms, and refund calculations.
4. **48-Hour Pre-Arrival Guide (Guest ONLY)**: Exactly 48 hours prior to check-in, guests receive a curated stay guide with WiFi credentials, driving directions/GPS coordinates, check-in instructions, and the villa's digital door access code or lockbox PIN.
5. **48-Hour Host Arrival Reminder (Host ONLY)**: Exactly 48 hours prior to check-in, hosts receive an arrival notification detailing party size, estimated arrival time, and a villa turnover readiness checklist.
6. **Payment Hold Expiry Reminder (Guest ONLY)**: Guests with active checkout holds receive a warning before their 2-hour hold window expires, with a direct checkout link to secure their reservation.

The pipeline is 100% compliant with Cloud Run scale-to-zero workloads, utilizing a recurring hourly GCP Cloud Scheduler cron trigger driving an internal authenticated sweep endpoint in `booking_api`, with PostgreSQL as the dynamic single source of truth.

---

## 1. Technical Specification

### 1.1. Key Decisions & Domain Rules

#### 1.1.1. Multi-Party Notification Routing Matrix
Notifications are strictly partitioned by recipient role to safeguard sensitive property credentials and financial data:

| Notification Type | Trigger Event | Target Recipient(s) | Primary Content & Role-Specific Payload |
|---|---|---|---|
| **Booking Confirmation** | Booking status $\rightarrow$ `Confirmed` | **Guest & Host** | **Guest**: Confirmation code, property name/city, dates, total paid, host contact info.<br>**Host**: Confirmation code, guest name/email/phone, dates, party size, expected net host payout. |
| **Booking Updated** | Confirmed booking updated with **material changes** | **Guest & Host** | Summary diff of modified fields (date shift, guest count, price adjustment), updated confirmation summary. |
| **Booking Cancelled** | Booking status $\rightarrow$ `Cancelled` | **Guest & Host** | Cancellation notice, cancellation reason/initiator, statutory cancellation policy snapshot, refund amount. |
| **48-Hour Pre-Arrival Guide** | 48 hours prior to `date_from` | **Guest ONLY** | WiFi SSID/password, door access code / lockbox PIN, GPS coordinates & driving directions, check-in/out times, emergency contacts. |
| **48-Hour Host Arrival Reminder** | 48 hours prior to `date_from` | **Host ONLY** | Guest arrival alert, party size, arrival ETA, turnover checklist, guest contact info. |
| **Payment Hold Expiry Reminder** | Pending hold near expiration (2h lifecycle) | **Guest ONLY** | Hold countdown warning, direct checkout link to finalize payment and prevent date release. |

#### 1.1.2. Door Access Code Management
- **Booking-Level Storage**: Stored directly on `booking.door_access_code TEXT DEFAULT NULL` (and mirrored on `booking_history`).
- **Unified Booking Update via `PATCH /api/v1/bookings/{id}`**: Rather than creating a single-purpose ad-hoc endpoint, `door_access_code: Option<String>` is added to the standard `UpdatedBookingRequest` payload. When a host views the booking, they can update the door code alongside any other reservation details in one atomic `PATCH` request.
- **Host-Only Authorization Guard**: If `door_access_code` is provided in the update request, the handler verifies that `claims.sub` is the listing owner (`listing.user_id`) or an administrator. If an unauthorized guest attempts to set or mutate the door code, the request is rejected with `403 Forbidden`.
- **Access Code Secrecy**: The code is never returned in public listing queries or guest booking summaries prior to the 48-hour pre-arrival dispatch. It is omitted from host email templates.

#### 1.1.3. 2-Hour Reservation Hold Window
- In accordance with Feature 62 requirements, pending checkout holds (`BookingStatus::Pending`) have an active validity window of **2 hours** (extended from 15 minutes).
- The database `pg_cron` cleanup procedure is adjusted to release holds older than 2 hours:
  `WHERE status = 'pending' AND created_at < NOW() - INTERVAL '2 hours'`.
- The hold expiry reminder is evaluated for active pending holds created between 75 and 105 minutes prior (targeting the ~30-minute remaining mark).

#### 1.1.4. Material Booking Update Filtering
To prevent notification fatigue, update emails are triggered **only on material changes**:
$$\text{is\_material} \iff (\Delta\text{date\_from} \lor \Delta\text{date\_to} \lor \Delta\text{number\_of\_persons} \lor \Delta\text{total\_price})$$
Non-material edits (such as internal admin notes or minor metadata updates) execute silently without email dispatch.
Evaluation is encapsulated in the strongly-typed domain model (`common::models`):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookingMaterialTerms {
    pub date_from: chrono::NaiveDate,
    pub date_to: chrono::NaiveDate,
    pub number_of_persons: i32,
    pub total_price: rust_decimal::Decimal,
}

pub fn is_material_booking_change(
    old_terms: &BookingMaterialTerms,
    new_terms: &BookingMaterialTerms,
) -> bool {
    old_terms.is_materially_different(new_terms)
}
```

#### 1.1.5. Scheduled Sweep Architecture (Cloud Scheduler + PostgreSQL)
- **Stateless Cloud Scheduler**: GCP Cloud Scheduler executes a single recurring cron job (`0 * * * *`, hourly) targeting:
  `POST /api/v1/internal/cron/process-scheduled-notifications` on `booking_api`.
- **Dynamic Resilience to Date Shifts & Cancellations**:
  - Cloud Scheduler stores **no booking records or individual timestamps**.
  - All date modifications and cancellations are persisted directly in PostgreSQL.
  - When the hourly sweep runs, `booking_api` queries PostgreSQL for confirmed bookings where `date_from = CURRENT_DATE + INTERVAL '2 days'`. Rescheduled bookings are automatically evaluated on their new date without reconfiguring Cloud Scheduler.
  - Cancelled bookings drop out of `status = 'confirmed'` and are excluded immediately.

---

### 1.2. Database Schema & Migrations (`db_core`)

#### Migration: `db_core/migrations/20260927000000_add_door_code_and_notification_log.sql`
```sql
-- 1. Add door access code to booking and booking_history
ALTER TABLE booking ADD COLUMN door_access_code TEXT DEFAULT NULL;
ALTER TABLE booking_history ADD COLUMN door_access_code TEXT DEFAULT NULL;

-- 2. Create notification tracking log for multi-party idempotency
CREATE TABLE booking_notification_log (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    booking_id UUID NOT NULL REFERENCES booking(id) ON DELETE CASCADE,
    notification_type VARCHAR(64) NOT NULL,
    recipient_user_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_booking_notification UNIQUE (booking_id, notification_type, recipient_user_id)
);

CREATE INDEX idx_booking_notification_lookup 
ON booking_notification_log (booking_id, notification_type);

CREATE INDEX idx_booking_notification_recipient 
ON booking_notification_log (recipient_user_id);

-- 3. Update pending hold expiration cron to 2 hours
DO $$
BEGIN
    IF current_database() NOT LIKE '%test%' THEN
        BEGIN
            PERFORM cron.unschedule('cleanup_stale_pending_holds');
            PERFORM cron.schedule('cleanup_stale_pending_holds', '*/15 * * * *', 
                'WITH expired_bookings AS (
                    UPDATE booking
                    SET status = ''cancelled'', updated_at = NOW()
                    WHERE status = ''pending'' 
                      AND created_at < NOW() - INTERVAL ''2 hours''
                    RETURNING *
                )
                INSERT INTO booking_history (
                    booking_id, confirmation_code, guest_id, listing_id, status,
                    date_from, date_to, currency, daily_rate, number_of_persons, total_days,
                    sub_total_price, discount_value, tax_value, fee_breakdown, total_price,
                    cancellation_policy, metadata, door_access_code, change_reason, created_at
                )
                SELECT 
                    id, confirmation_code, guest_id, listing_id, status,
                    date_from, date_to, currency, daily_rate, number_of_persons, total_days,
                    sub_total_price, discount_value, tax_value, fee_breakdown, total_price,
                    cancellation_policy, metadata, door_access_code, ''Hold expired after 2 hours'', NOW()
                FROM expired_bookings;'
            );
        EXCEPTION WHEN OTHERS THEN
            RAISE NOTICE 'Skipping pg_cron adjustment: %', SQLERRM;
        END;
    END IF;
END
$$;
```

#### SQLx Query Functions (`db_core/src/booking.rs` & `db_core/src/notification_log.rs`)
- `claim_and_log_notification(pool, booking_id, notification_type, recipient_id) -> Result<bool, DbError>`:
  Executes `INSERT INTO booking_notification_log ... ON CONFLICT DO NOTHING RETURNING id`. Returns `true` if claimed, `false` if already dispatched.
- `update_booking_door_code(pool, booking_id, door_code) -> Result<Booking, DbError>`:
  Updates `door_access_code` and records an immutable entry in `booking_history`.
- `get_48h_pre_arrival_candidates(pool) -> Result<Vec<PreArrivalBookingRecord>, DbError>`:
  Single batch query fetching confirmed bookings where `date_from = CURRENT_DATE + INTERVAL '2 days'` joining listing WiFi/directions and guest/host contact info.
- `get_expiring_hold_candidates(pool) -> Result<Vec<ExpiringHoldRecord>, DbError>`:
  Fetches pending holds created between 75 and 105 minutes prior without an existing `payment_hold_reminder` log.

---

### 1.3. Domain Types & Email Templates

#### 1.3.1. `common::email` Additions
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailTemplate {
    UserVerificationOtp,
    PasswordResetOtp,
    BookingConfirmation, // Legacy alias
    BookingConfirmationGuest,
    BookingConfirmationHost,
    BookingUpdatedGuest,
    BookingUpdatedHost,
    BookingCancelledGuest,
    BookingCancelledHost,
    PreArrivalGuideGuest,
    HostUpcomingArrival,
    PaymentHoldExpiryReminder,
    GuestHostMessageNotification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreArrivalGuidePayload {
    pub guest_name: String,
    pub listing_name: String,
    pub check_in_date: String,
    pub check_in_time: String,
    pub check_out_time: String,
    pub address: String,
    pub directions: Option<String>,
    pub wifi_ssid: Option<String>,
    pub wifi_password: Option<String>,
    pub door_access_code: Option<String>,
    pub host_name: String,
    pub host_phone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostUpcomingArrivalPayload {
    pub host_name: String,
    pub listing_name: String,
    pub guest_name: String,
    pub guest_email: String,
    pub guest_phone: Option<String>,
    pub party_size: i32,
    pub check_in_date: String,
    pub estimated_arrival_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentHoldReminderPayload {
    pub guest_name: String,
    pub listing_name: String,
    pub confirmation_code: String,
    pub expires_in_minutes: i32,
    pub total_price: String,
    pub checkout_url: String,
}
```

#### 1.3.2. Templates in `app_api/email_worker/templates/`
- `booking_confirmation_guest.html`: Summary of property, check-in dates, breakdown of costs (`Decimal`), host contact.
- `booking_confirmation_host.html`: Guest name, phone, dates, party size, host payout summary.
- `booking_updated_guest.html` & `booking_updated_host.html`: Clear visual diff of modified fields with updated totals.
- `booking_cancelled_guest.html` & `booking_cancelled_host.html`: Cancellation reason, policy terms, refund status.
- `pre_arrival_guide_guest.html`: Guest-only villa access credentials, WiFi, directions, house rules.
- `host_upcoming_arrival.html`: Host-only turnover reminder, guest ETA, party details.
- `payment_hold_expiry_reminder.html`: Guest-only countdown reminder and direct checkout action link.

---

### 1.4. Dispatch Pipeline & API Handlers (`app_api/booking_api`)

#### 1.4.1. Event-Driven Hooks
1. **Booking Confirmation Hook (`update_booking` / payment capture)**:
   - When status transitions from `Pending` $\rightarrow$ `Confirmed`:
     - Dispatches `BookingConfirmationGuest` to `guest.email`.
     - Dispatches `BookingConfirmationHost` to `host.email`.
2. **Booking Modification Hook (`update_booking`)**:
   - Compares previous booking snapshot against new values using `is_material_booking_change`.
   - If material:
     - Dispatches `BookingUpdatedGuest` to `guest.email`.
     - Dispatches `BookingUpdatedHost` to `host.email`.
3. **Booking Cancellation Hook (`delete_booking` / `update_booking`)**:
   - When status transitions to `Cancelled`:
     - Dispatches `BookingCancelledGuest` to `guest.email`.
     - Dispatches `BookingCancelledHost` to `host.email`.

#### 1.4.2. Unified Booking Update Endpoint (`PATCH /api/v1/bookings/{id}`)
```rust
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdatedBookingRequest {
    pub status: Option<BookingStatus>,
    pub metadata: Option<BookingMetadata>,
    pub door_access_code: Option<String>,
}

#[utoipa::path(
    patch,
    path = "/api/v1/bookings/{id}",
    request_body = UpdatedBookingRequest,
    responses(
        (status = 200, description = "Booking updated successfully"),
        (status = 403, description = "Forbidden: Caller is not authorized to edit door access code"),
        (status = 404, description = "Booking not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "bookings"
)]
async fn update_booking(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    body: web::Json<UpdatedBookingRequest>,
) -> Result<impl Responder, ApiError>;
```
- **Authorization Guard**: When `body.door_access_code.is_some()`, the handler extracts `Claims` from the request and verifies that the caller (`claims.sub`) is either the owner of the listing (`listing.user_id`) or possesses the `Admin` role. Guests attempting to update the door access code receive `403 Forbidden`.
- **Atomic History Recording**: Any mutation to `door_access_code`, `status`, or `metadata` is logged immutably in `booking_history`.

#### 1.4.3. Internal Scheduled Sweep Endpoint (`POST /api/v1/internal/cron/process-scheduled-notifications`)
- **Hosting Service**: Hosted natively inside **`app_api/booking_api`**.
  - *Rationale*: `booking_api` manages the booking domain, owns the SQLx database queries for reservations and availability, has direct access to `booking_notification_log` and `email_outbox`, and holds the `api_core::EmailPublisher` client. `email_worker` remains decoupled as a downstream asynchronous consumer.
```rust
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessedScheduledNotificationsResponse {
    pub pre_arrival_notifications_dispatched: usize,
    pub expiring_hold_notifications_dispatched: usize,
    pub execution_time_ms: u128,
}

#[utoipa::path(
    post,
    path = "/api/v1/internal/cron/process-scheduled-notifications",
    responses(
        (status = 200, description = "Sweep completed successfully", body = ProcessedScheduledNotificationsResponse),
        (status = 401, description = "Unauthorized cron secret")
    ),
    tag = "internal"
)]
async fn process_scheduled_notifications(
    req: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ApiError>;
```
- **Execution Flow**:
  1. Authenticate custom HTTP header `x-cron-secret` against the `CRON_SECRET` environment variable.
  2. Execute 48-Hour Pre-Arrival Sweep:
     - Query eligible confirmed bookings via `db_core::notification_log::get_48h_pre_arrival_candidates`.
     - For each booking:
       - Atomically claim in `booking_notification_log` (`pre_arrival_guide_guest` and `host_upcoming_arrival`).
       - Write records to `email_outbox`.
       - Asynchronously emit `EmailNotificationEvent` to Pub/Sub.
  3. Execute 2-Hour Payment Hold Expiry Sweep:
     - Query eligible pending holds via `db_core::notification_log::get_expiring_hold_candidates`.
     - Atomically claim `payment_hold_reminder`.
     - Write record to `email_outbox` and emit Pub/Sub event.
  4. Return JSON summary:
     ```json
     {
       "pre_arrival_notifications_dispatched": 2,
       "expiring_hold_notifications_dispatched": 1,
       "execution_time_ms": 42
     }
     ```

---

### 1.5. Admin Portal UI Integration (`web_app_admin_tc`)

The Topcoat SSR Admin Portal integrates host/admin door code configuration directly into the Master Booking Schedule view ([`web_app_admin_tc/src/pages/bookings_admin.rs`](file:///home/pav/code/our_places_rs-feat-62-transactional-messaging-pipeline-pre-arrival-guide-not/web_app_admin_tc/src/pages/bookings_admin.rs)):

1. **Table Action Row**:
   - The booking row details button exposes `data-door-code=(b.door_access_code.clone().unwrap_or_default())`.
2. **Audit & Details Dialog (`#admin-booking-details-dialog`)**:
   - Features a dedicated **Keyless Access & Arrival Guide** section.
   - Includes input `#detail-door-code-input`, status badge `#detail-door-code-badge` ("Not Set" vs. "Configured"), and trigger `#btn-save-door-code`.
3. **Authenticated API Forwarding (`PATCH /api/bookings/{id}`)**:
   - The portal route extracts the authenticated admin session and signs a short-lived user JWT (`generate_jwt_for_user(user.id)`).
   - Forwards the payload with `Authorization: Bearer <jwt>` to `booking_api` (`PATCH /api/v1/bookings/{id}`), successfully passing the host/admin authorization guard.

---

### 1.6. Cloud Infrastructure & Scheduler (`infra/modules/scheduler/`)

The scheduled sweep infrastructure is codified via Terraform in [`infra/modules/scheduler/`](file:///home/pav/code/our_places_rs-feat-62-transactional-messaging-pipeline-pre-arrival-guide-not/infra/modules/scheduler/):
- **Resource**: `google_cloud_scheduler_job.scheduled_notifications_sweep`
- **Schedule**: `0 * * * *` (Hourly, UTC)
- **Target**: HTTP POST `${var.booking_api_url}/api/v1/internal/cron/process-scheduled-notifications`
- **Headers**:
  ```hcl
  headers = {
    "Content-Type"  = "application/json"
    "x-cron-secret" = var.cron_secret
  }
  ```
- **Retry Policy**: 3 retry attempts with exponential backoff.

---

## 2. Performance & Scalability Considerations

### 2.1. Big-O Complexity & Elimination of N+1 Queries
- **Single-Query Batch Sweep**: Instead of iterating over all bookings and performing subsequent queries for user/listing info ($O(N)$ query cascades), the sweep executes a single joined query:
  $$\mathcal{O}(B) \quad \text{where } B \text{ is the number of bookings arriving in 48h (typically } \le 50\text{)}$$
  ```sql
  SELECT b.id, b.confirmation_code, b.date_from, b.door_access_code,
         g.email as guest_email, g.first_name as guest_name,
         h.email as host_email, h.first_name as host_name,
         l.name as listing_name, l.city, l.listing_details
  FROM booking b
  JOIN "user" g ON b.guest_id = g.id
  JOIN listing l ON b.listing_id = l.id
  JOIN "user" h ON l.user_id = h.id
  WHERE b.status = 'confirmed'
    AND b.date_from = CURRENT_DATE + INTERVAL '2 days'
    AND NOT EXISTS (
        SELECT 1 FROM booking_notification_log 
        WHERE booking_id = b.id AND notification_type = 'pre_arrival_guide_guest'
    );
  ```
- **Indexed Anti-Join**: The `NOT EXISTS` clause is accelerated by the unique index on `(booking_id, notification_type)` yielding $O(1)$ index probe time per candidate row.

### 2.2. Scale-to-Zero & Latency Budgets
- **Zero Persistent In-Memory Polling**: No background sleep loops running inside Tokio worker threads. When no cron or HTTP requests are active, container CPU scales to zero.
- **Cold Start Latency Budget**: The internal sweep endpoint executes in $< 120\text{ms}$ (excluding container startup), remaining comfortably under the platform's $300\text{ms}$ p50 cold start budget.
- **Decoupled SMTP Network I/O**: `booking_api` never waits for SMTP socket connections. It writes to PostgreSQL and emits a lightweight Pub/Sub event in $< 10\text{ms}$. SMTP delivery is offloaded entirely to `email_worker`.

---

## 3. Threat Modeling & Security Mitigations

### 3.1. OWASP Top 10 Risk Analysis & Mitigations

| Vulnerability Threat | Risk Level | Mitigation Strategy |
|---|---|---|
| **Broken Access Control (BOLA/IDOR)**: Malicious user alters door access code for arbitrary villa. | **Critical** | Handled by strict Actix authorization checks: `claims.sub` must match `listing.user_id` or possess `admin` role. Attempted updates by guests or unauthorized users return `403 Forbidden`. |
| **Sensitive Data Exposure**: Door access codes leaked in public listings or unconfirmed reservations. | **High** | 1. Door code is stripped from all public listing API models.<br>2. Door code is suppressed in guest reservation endpoints prior to T-48h.<br>3. Door code is never dispatched to cancelled, refunded, or pending reservations. |
| **Unauthorized Cron Sweep Triggering**: External attacker invokes internal cron sweep repeatedly to cause DoS or email spam. | **High** | Endpoint is secured via `x-cron-secret` header matching `CRON_SECRET` env var. Requests with invalid or missing credentials receive immediate `401 Unauthorized`. |
| **Email Injection & XSS in Templates**: Malicious input in guest name or special requests injected into HTML email templates. | **Medium** | All template placeholder substitutions sanitize inputs via HTML entity encoding (`&`, `<`, `>`, `"`, `'`). Recipient addresses are strictly validated via `validator::validate_email`. |
| **PII & Credential Logging**: Door access codes, WiFi passwords, or guest emails logged in cleartext. | **Medium** | Structured tracing macros explicitly skip or redact sensitive fields: `#[tracing::instrument(skip(door_access_code, payload))]`. |

---

## 4. Comprehensive Test Plan

### 4.1. Unit Tests

| Test Case | Target Module | Description | Priority |
|---|---|---|---|
| `test_email_template_variants_serde` | `common::email` | Verify all 9 notification enum variants serialize and deserialize correctly. | P0 |
| `test_material_booking_change_detection` | `common::models` | Verify `is_material_booking_change` with `BookingMaterialTerms` returns `true` for date/guest/price changes, and `false` for identical terms. | P0 |
| `test_render_pre_arrival_guide_html` | `email_worker::processor` | Verify `pre_arrival_guide_guest.html` interpolates WiFi, door code, directions, and provides fallbacks when fields are null. | P0 |
| `test_render_host_upcoming_arrival_html` | `email_worker::processor` | Verify `host_upcoming_arrival.html` correctly formats party size, guest contact, and turnover notes. | P0 |
| `test_render_payment_hold_reminder_html` | `email_worker::processor` | Verify `payment_hold_expiry_reminder.html` renders countdown minutes and valid checkout URL. | P0 |

### 4.2. Integration Tests

| Test Case | Target Module | Description | Priority |
|---|---|---|---|
| `test_migration_schema_and_indexes` | `db_core` | Execute migration and verify `booking.door_access_code` and `booking_notification_log` table and indexes. | P0 |
| `test_notification_idempotency_log` | `db_core::notification_log` | Verify concurrent calls to `claim_and_log_notification` allow exactly one winner (`true`) and subsequent calls return `false`. | P0 |
| `test_48h_pre_arrival_query_filtering` | `db_core::booking` | Verify query selects bookings with `date_from = +2 days`, excludes cancelled bookings, and excludes previously logged bookings. | P0 |
| `test_2h_hold_expiry_query_and_cleanup` | `db_core::booking` | Verify pending holds older than 2 hours are cancelled by cleanup procedure and reminder candidates are identified. | P1 |
| `test_booking_confirmation_triggers_dual_outbox` | `app_api/booking_api` | Confirm a booking and verify two `email_outbox` rows are inserted (one for guest, one for host). | P0 |
| `test_material_update_triggers_dual_outbox` | `app_api/booking_api` | Modify booking dates and verify both guest and host receive update notifications. | P0 |
| `test_cancellation_triggers_dual_outbox` | `app_api/booking_api` | Cancel a booking and verify cancellation emails are queued for guest and host. | P0 |
| `test_update_door_code_authorization` | `app_api/booking_api` | Verify host can update door code (`200 OK`), while unauthenticated or guest caller receives `403 Forbidden`. | P0 |
| `test_internal_cron_endpoint_auth` | `app_api/booking_api` | Verify valid `x-cron-secret` executes sweep (`200 OK`), invalid or missing secret returns `401 Unauthorized`. | P0 |
| `test_end_to_end_worker_dispatch` | `app_api/email_worker` | Verify Pub/Sub event triggers `email_worker`, dispatches via Mock provider, and updates status to `sent`. | P0 |

### 4.3. Performance & Stress Benchmarks
- **Sweep Execution Time**: 500 simulated confirmed bookings with 50 matching the T-48h window must execute query and outbox insertion in $< 100\text{ms}$.
- **Idempotency Under Concurrent Load**: 50 simultaneous concurrent requests attempting to log the same notification type must result in exactly 1 outbox record and 1 email dispatched.
