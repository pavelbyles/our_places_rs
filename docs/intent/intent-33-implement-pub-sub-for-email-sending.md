# Intent: Event-Driven Pub/Sub Queue for Asynchronous Email Delivery

## Metadata
- **Author**: Originator (via GitHub Issue #33)
- **Date**: 2026-09-23
- **Status**: Approved
- **Target Area**: Fullstack / Backend Infrastructure (`db_core`, `common`, `app_api/email_worker`, `app_api/user_api`, `app_api/booking_api`, `infra`)

---

## 1. Problem Statement & Motivation
Currently, transactional emails and notifications (e.g., user registration verification OTPs, password reset codes, booking status updates, guest-host messages) are either logged synchronously to `tracing::info!` stdout or executed in-line during HTTP request handling. Synchronous email processing introduces significant drawbacks:

1. **API Latency & Blocking**: Synchronous HTTP request handlers wait on third-party email provider latency or blocking IO, degrading user response times.
2. **Failure Cascades & Lost Notifications**: If an external email service experiences transient downtime, API requests fail or log errors without reliable retries or status persistence.
3. **Lack of Auditability & State Tracking**: Without an outbox repository in PostgreSQL, administrators cannot track whether a specific email was delivered, failed, or retried.

To resolve this, we will introduce an asynchronous Event-Driven Notification Pattern using Google Cloud Pub/Sub and PostgreSQL:
- Services insert email records into an `email_outbox` database table.
- A lightweight Pub/Sub event containing only the `email_id` (UUID) is published to an email topic.
- A dedicated microservice worker (`email_worker`) consumes the event, retrieves the `email_id` from PostgreSQL, validates the current status, dispatches the email via an external provider (or mock provider in dev/test), and updates the status (`sent` or `failed`).

---

## 2. Proposed Outcome (The Vision)

1. **Transactionally Safe Email Outbox (`db_core`)**:
   - Outbound transactional emails are written to an `email_outbox` table in PostgreSQL with status tracking (`pending`, `processing`, `sent`, `failed`).
2. **Lightweight Pub/Sub Event Notification (`common` & `app_api/api_core`)**:
   - Upstream services (`user_api`, `booking_api`) publish a minimal `EmailNotificationEvent { email_id: Uuid }` to GCP Pub/Sub post-DB transaction.
3. **Dedicated `email_worker` Microservice (`app_api/email_worker`)**:
   - A Cloud Run-compatible worker service receives Pub/Sub push/pull events, fetches the record by `email_id`, checks DB status to guarantee idempotency, dispatches the message, and updates email state.
4. **Idempotency & Status Validation**:
   - On duplicate message delivery, `email_worker` checks the current database state (`SELECT ... FOR UPDATE` or optimistic transition) to ensure emails are sent exactly once.
5. **Seamless Service Integration**:
   - User verification codes, password reset OTPs, booking confirmations, and guest-host messaging events are refactored to use the Pub/Sub email pipeline.

---

## 3. Scope Boundaries

### In Scope (MVP)
- **Database Schema & Migrations (`db_core`)**:
  - `email_outbox` table: `id` (UUID), `recipient_email` (VARCHAR), `subject` (VARCHAR), `template_id` (VARCHAR), `payload` (JSONB), `status` (VARCHAR/ENUM), `attempts` (INT), `last_error` (TEXT), `sent_at` (TIMESTAMPTZ), `created_at` (TIMESTAMPTZ), `updated_at` (TIMESTAMPTZ).
  - SQLx functions for inserting email records, fetching by ID with row locks, and updating status atomically.
- **Domain Models & Event Types (`common`)**:
  - `EmailStatus` enum (`Pending`, `Processing`, `Sent`, `Failed`).
  - `EmailNotificationEvent` message payload schema containing `email_id`.
- **Pub/Sub Client Helper (`app_api/api_core`)**:
  - Shared publisher client for GCP Pub/Sub email queue topic.
- **Worker Microservice (`app_api/email_worker`)**:
  - Actix-web Pub/Sub Push HTTP endpoint (`POST /pubsub/email-events`) or worker runtime.
  - Email sending trait/abstraction (`EmailProvider`) supporting a mock implementation for development/testing and provider integration for production.
- **Upstream API Integration (`user_api`, `booking_api`)**:
  - Refactor verification code generation, password reset, and message notifications to record email entries in `email_outbox` and emit Pub/Sub events.
- **Deployment & Infrastructure (`infra`, `Dockerfile.email_worker`)**:
  - `Dockerfile.email_worker` multi-stage build.
  - GCP Pub/Sub topic (`email-notifications-topic`) and Push subscription configuration.

### Out of Scope / Non-Goals
- WYSIWYG email template editor or complex dynamic HTML layout designer.
- Marketing email campaign management or newsletter subscription lists (strictly transactional MVP).
- Client-side direct publishing to Pub/Sub queues (all events must originate from server microservices).

---

## 4. Affected Systems & Stakeholders

- **User Personas**: Guests, Hosts, Admins (receive reliable, non-blocking transactional emails).
- **System Components**:
  - `common`: `EmailStatus`, `EmailNotificationEvent` domain types.
  - `db_core`: `email_outbox` table, SQLx entities, query migrations.
  - `app_api/api_core`: Pub/Sub publisher integration utilities.
  - `app_api/email_worker`: Dedicated email background worker microservice.
  - `app_api/user_api`: User registration OTP & password reset notification refactoring.
  - `app_api/booking_api`: Booking notification & message event refactoring.
  - `infra`: Terraform/Pulumi GCP Pub/Sub infrastructure definitions.

---

## 5. Constraints & Non-Negotiable Invariants

- **Idempotency & Concurrency Locking**: Row-level locking (`SELECT ... FOR UPDATE`) or strict status transition validation in `db_core` ensures no duplicate email dispatches occur upon Pub/Sub redelivery.
- **Panic-Free Implementation**: Zero `.unwrap()` or `.expect()` calls in production paths; all errors handled via `Result<T, E>` and mapped through `AppError`.
- **Cloud Run Latency & Resource Budgets**: Cold start target < 300ms (p50), memory footprint < 256MB on Cloud Run scale-to-zero.
- **Non-Blocking Handlers**: HTTP handlers in `user_api` and `booking_api` must not wait for external SMTP calls.
- **Security & Privacy**: PII and sensitive tokens (OTPs) must be protected; email body content must not be printed to unencrypted log sinks.

- **Application-Level Retries & No DLQ**: Retries for transient/appropriate failures are handled internally within the application worker (`email_worker`). Failed emails must **not** be re-published back onto the Pub/Sub queue, and no Dead-Letter Queue (DLQ) will be created. Retry counts are governed by a configurable environment variable (`MAX_EMAIL_RETRIES`), initially set to `3`.

---

## 6. Success Metrics & Signals

- **Leading Indicator**: 100% of user authentication OTP and booking notification events produce an `email_outbox` row and publish `email_id` to Pub/Sub without HTTP handler latency regressions.
- **Lagging Indicator**: 0 duplicate emails dispatched; end-to-end delivery latency (from trigger to worker completion) < 2 seconds.

---

## 7. Open Questions & Policy Concerns

- [x] **Pub/Sub Transport Pattern**: Uses Cloud Run Pub/Sub Push HTTP POST notifications delivered directly to the `email_worker` service endpoint (`POST /pubsub/email-events`).
- [x] **Production Email Provider**: Credentials (SMTP host, port, username, password) are loaded from environment variables populated via GitHub Secrets (using standard SMTP / `lettre` provider integration).
- [x] **Retry & DLQ Strategy**: Retries are handled within the application (`email_worker`) governed by `MAX_EMAIL_RETRIES` (default `3`). Failed emails are not re-published back to Pub/Sub, and no DLQ will be used.



