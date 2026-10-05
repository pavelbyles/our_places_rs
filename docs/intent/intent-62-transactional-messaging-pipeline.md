# Intent: Transactional Messaging Pipeline (Pre-Arrival Guide & Notifications)

## Metadata
- **Author**: Originator (via GitHub Issue #62)
- **Date**: 2026-09-27
- **Status**: Approved
- **Target Area**: Fullstack / Backend Infrastructure (`db_core`, `common`, `app_api/booking_api`, `app_api/email_worker`, `web_app_admin_tc`)

---

## 1. Problem Statement & Motivation
Guests and property hosts expect seamless, automated, and timely communication throughout the reservation and stay lifecycle. Currently, the platform lacks a scheduled, multi-party, and context-aware messaging dispatch pipeline:

1. **Incomplete & Single-Sided Confirmation Flow**: Confirmation emails are triggered opportunistically upon hold placement rather than after verified payment confirmation (`BookingStatus::Confirmed`), sent only to the guest, and leave the property host unaware of confirmed bookings without manual admin checks.
2. **Missing Booking Modification & Cancellation Alerts**: When a reservation is modified (dates changed, party size adjusted) or cancelled by either party or admin, neither host nor guest receives automated notification, creating confusion and operational risk.
3. **Missing Check-In & Pre-Arrival Onboarding (Guest-Only)**: Guests arriving at villas often struggle with logistics (finding property locations, obtaining WiFi passwords, and unlocking digital doors/lockboxes) without a dedicated 48-hour pre-arrival onboarding guide.
4. **Missing 48-Hour Arrival Readiness Alert (Host-Only)**: Hosts do not receive proactive reminders 48 hours before an incoming guest's arrival, risking unprepared villas, delayed turnovers, or unattended check-ins.
5. **Unnotified Pending Payment Holds (Guest-Only)**: While the platform enforces reservation holds, guests receive no proactive warnings when their pending hold is nearing expiration, resulting in abandoned bookings and unexpected date release.
6. **Disjointed Dispatch Lifecycle**: No automated, scale-to-zero compatible background mechanism exists to evaluate and dispatch time-sensitive transactional emails without keeping Cloud Run instances perpetually active.

Implementing a robust transactional messaging pipeline with role-aware recipient routing ensures operational readiness, prevents lost bookings, and builds high guest-host trust.

---

## 2. Proposed Outcome (The Vision)
An automated, event-driven, and time-aware transactional messaging pipeline that provides role-tailored notifications across every reservation lifecycle milestone:

1. **Dual-Party Booking Confirmations**: Dispatches rich confirmation emails immediately upon confirmed payment (`BookingStatus::Confirmed`) to **both Guest and Host** with role-appropriate details (guest receives access guidelines and receipts; host receives guest details and expected payout totals).
2. **Dual-Party Modification Alerts**: Dispatches clear update notifications to **both Guest and Host** whenever material booking parameters (dates, guests, pricing) change.
3. **Dual-Party Cancellation Notices**: Dispatches cancellation notifications to **both Guest and Host** upon cancellation, detailing reason, cancellation policy snapshot, and refund breakdown.
4. **48-Hour Pre-Arrival Guide (Guest ONLY)**: Automatically emails the guest 48 hours before check-in with critical stay instructions (WiFi SSID/password, digital door codes/lockbox PINs, driving directions/GPS coordinates, check-in/out times, and property house rules).
5. **48-Hour Upcoming Arrival Alert (Host ONLY)**: Automatically emails the host 48 hours before guest check-in with guest name, party size, arrival ETA, and a property turnover readiness checklist.
6. **Payment Hold Expiry Reminder (Guest ONLY)**: Proactively alerts guests before their 2-hour payment hold expires, providing a direct checkout link to secure their reservation.
7. **Scale-to-Zero & Zero Duplicate Dispatches**: Operates within Cloud Run scale-to-zero constraints via GCP Cloud Scheduler periodic cron sweeps calling an internal authenticated API endpoint, enforcing database-level idempotency so each recipient receives each message type at most once.

---

## 3. Scope Boundaries

### Notification Routing Matrix

| Notification Type | Trigger Event | Recipient(s) | Primary Content & Payload |
|---|---|---|---|
| **Booking Confirmation** | Booking transitions to `Confirmed` | **Guest & Host** | **Guest**: Confirmation code, villa address, dates, total paid, host contact info.<br>**Host**: Confirmation code, guest name & contact, dates, party size, expected net payout. |
| **Booking Updates / Changes** | Confirmed booking updated with **material changes** (dates, guest count, price) | **Guest & Host** | Modified fields summary (date shift, guest count), adjusted price/totals, updated confirmation summary. |
| **Booking Cancellation** | Booking status becomes `Cancelled` | **Guest & Host** | Cancellation confirmation, initiator/reason, cancellation policy snapshot, refund amount (if applicable). |
| **48-Hour Pre-Arrival Guide** | 48 hours prior to `date_from` | **Guest ONLY** | WiFi credentials, door access code / lockbox PIN, GPS coordinates & driving directions, check-in/out times, emergency contacts. |
| **48-Hour Host Arrival Reminder** | 48 hours prior to `date_from` | **Host ONLY** | Guest arrival alert, party size, arrival ETA, turnover checklist, guest contact info. |
| **Payment Hold Expiry Reminder** | Pending hold near expiration (2-hour hold lifecycle) | **Guest ONLY** | Hold expiration warning, countdown remaining, direct link to checkout page to finalize payment. |

### In Scope (MVP)
- **Transactional Email Templates (`app_api/email_worker/templates`)**:
  - `booking_confirmation_guest.html`: Villa summary, host contact, financial breakdown, initial instructions.
  - `booking_confirmation_host.html`: Guest details, dates, expected net payout, booking code.
  - `booking_updated_guest.html` & `booking_updated_host.html`: Modification diffs and revised reservation summary.
  - `booking_cancelled_guest.html` & `booking_cancelled_host.html`: Cancellation reason, policy terms, refund status.
  - `pre_arrival_guide.html`: Guest-only villa access credentials, WiFi, directions, house rules.
  - `host_upcoming_arrival.html`: Host-only turnover reminder, guest ETA, party details.
  - `payment_hold_reminder.html`: Guest-only countdown reminder and direct checkout action link.
- **Data Model & Schema Additions (`db_core` & `common`)**:
  - **Host-Only Door Access Code on Booking**: Add `door_access_code: Option<String>` to `booking` (and `booking_history`). This field is strictly editable by the property host/admin and read-only for guests.
  - **Listing Check-In Details**: Structured property directions, WiFi credentials (`wifi_ssid`, `wifi_password`), and default check-in/out times on `listing`.
  - **2-Hour Hold Window**: Update reservation hold expiry duration from 15 minutes to 2 hours (`expires_at = created_at + INTERVAL '2 hours'`).
  - **Notification Idempotency Log**: `booking_notification_log` table in PostgreSQL tracking `(booking_id, notification_type, recipient_user_id, sent_at)` with a unique constraint to ensure exactly-once delivery per recipient and milestone.
  - **Email Template Enum**: Variants in `common::email::EmailTemplate` for all 7+ multi-party notification types.
- **Dispatch Pipeline & Periodic Sweep (`app_api/booking_api` & GCP Cloud Scheduler)**:
  - **Event-Driven Dispatches**: Synchronous/transactional event hooks in `app_api/booking_api` emitting outbox records immediately upon booking confirmation, material update, and cancellation.
  - **Recurring Sweep Trigger (Cloud Scheduler)**:
    - GCP Cloud Scheduler runs as a single recurring cron job (e.g. hourly `0 * * * *`), triggering an internal authenticated endpoint: `POST /api/v1/internal/cron/process-scheduled-notifications`.
    - **No booking state in Cloud Scheduler**: Cloud Scheduler stores zero booking records or timestamps.
    - **Dynamic Resilience to Date Changes**: When a booking's arrival date is modified or cancelled, PostgreSQL stores the change immediately. When the hourly sweep runs, it queries PostgreSQL for confirmed bookings where `date_from = CURRENT_DATE + INTERVAL '2 days'`. Rescheduled bookings are automatically evaluated on their new arrival date without modifying Cloud Scheduler.
- **Admin & Host Configuration (`web_app_admin_tc`)**:
  - Booking detail view in admin portal allows hosts/admins to set or update the `door_access_code` for that specific reservation.
  - Listing edit view allows hosts/admins to configure default WiFi credentials and arrival directions.

### Out of Scope / Non-Goals
- SMS/WhatsApp messaging rails (strictly transactional email via existing Pub/Sub `email_worker` pipeline for MVP).
- Real-time IoT smart-lock API integration (door codes are entered manually by hosts in booking details for MVP).
- Dynamic WYSIWYG email drag-and-drop builder (static responsive HTML/CSS templates with placeholder interpolation).
- Marketing, newsletter, or promotional drip campaigns.

---

## 4. Affected Systems & Stakeholders

- **User Personas**:
  - **Guests**: Receive timely confirmations, material update alerts, cancellations, pre-arrival guides, and hold reminders.
  - **Hosts & Villa Owners**: Configure booking door codes, receive confirmed booking alerts, change/cancellation notices, and 48-hour turnover reminders.
  - **Platform Administrators**: Gain visibility into email dispatch status and monitor automated cron sweep execution.
- **System Components**:
  - `db_core`: `booking.door_access_code`, `booking_notification_log` table, 2-hour hold cleanup adjustment, SQLx queries.
  - `common`: `EmailTemplate` variants, DTOs for each notification type payload, material change detection diff helper.
  - `app_api/email_worker`: HTML templates and rendering logic for all 7+ notification types.
  - `app_api/booking_api`: State transition hooks (confirm, update, cancel), internal cron sweep endpoint, outbox insertion.
  - `web_app_admin_tc`: Host-only booking door access code editor and listing check-in settings.
  - `infra`: Terraform definition for GCP Cloud Scheduler recurring sweep job.

---

## 5. Constraints & Non-Negotiable Invariants

- **Role-Based Content & Edit Isolation**:
  - Door access codes and WiFi passwords are sent **exclusively to the guest** and **never** to unconfirmed, cancelled, or pending bookings.
  - Door access code on the booking is **host-only editable**; guests can never view or modify this field via API endpoints prior to the pre-arrival guide dispatch.
  - Payment hold alerts are sent **exclusively to the guest**; expected payout totals are sent **exclusively to the host**.
- **Material Change Filtering**: Update notifications are triggered only on material booking mutations (dates, guest count, price/fees), preventing email spam on minor internal metadata edits.
- **Idempotency & Zero Duplicate Emails**: Unique constraint on `(booking_id, notification_type, recipient_user_id)` ensures no recipient receives duplicate emails for the same milestone.
- **Scale-to-Zero Budget**: Zero persistent in-memory background polling loops in Cloud Run. Scheduled sweeps are initiated via GCP Cloud Scheduler HTTP pings (< 300ms p50 cold start).
- **Security & PII Protection**: Door codes and guest credentials must not be logged to unencrypted log sinks or exposed to unauthorized users.
- **Panic-Free Rust**: Zero `.unwrap()` or `.expect()` calls in production paths; strict monadic error handling (`Result<T, E>`) mapped to `AppError`.
- **Financial Precision**: All monetary totals rendered in confirmation, update, and cancellation emails must use `rust_decimal::Decimal` formatted with ISO currency codes (tri-currency compliance).

---

## 6. Success Metrics & Signals

- **Leading Indicator**: 100% of confirmed, modified, and cancelled reservations generate corresponding outbox entries for both guest and host without API latency regression.
- **Lagging Indicator**: Zero reported missed check-in instructions or host turnover surprises; 0 duplicate email reports; improved hold-to-confirmation conversion rate.

---

## 7. Open Questions & Policy Concerns (Resolved)

- [x] **Hold Duration vs Reminder Window**: Checkout hold duration is updated to **2 hours** (extended from 15 minutes). The payment hold reminder will be dispatched to the guest prior to hold expiration (e.g. at the 30-minute remaining mark).
- [x] **Door Code Configuration**: Statically set by the host on the booking details. This is implemented as a **host-only editable field** on the booking record.
- [x] **Scheduled Sweep Infrastructure**: Handled via **GCP Cloud Scheduler** running as a recurring hourly cron trigger hitting an internal authenticated endpoint on `booking_api`. Individual bookings are **not** stored in Cloud Scheduler; the database is the dynamic single source of truth, guaranteeing that date shifts and cancellations automatically update notification schedules without touching Cloud Scheduler.
- [x] **Booking Update Trigger Scope**: Restricted to **material changes only** (dates, party size, total pricing) to eliminate spam on non-material metadata edits.
