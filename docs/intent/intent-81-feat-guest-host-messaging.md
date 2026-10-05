# Intent: Guest-Host Messaging Linked to Bookings

## Metadata
- **Author**: Originator (via GitHub Issue #81)
- **Date**: 2026-09-08
- **Status**: Approved
- **Target Area**: Fullstack (Database, Booking API, Guest Web App [web_app_tc], Admin Web App [web_app_admin_tc], Async Notifications)

---

## 1. Problem Statement & Motivation
Currently, guests who initiate or complete a booking have no in-platform mechanism to communicate with their host regarding stay details, check-in instructions, special accommodations, or travel updates. Similarly, hosts and platform admins lack a consolidated record of communications tied to specific reservations.

This forces communication off-platform (SMS, external email, phone calls), resulting in:
- Loss of context and audit trails for dispute resolution or reservation management.
- Operational friction for hosts and administrators managing multiple reservations across villas.
- Diminished guest experience due to absence of an integrated reservation communications hub.

Solving this creates a unified communication thread tied directly to each booking, accessible to guests in the guest portal (`web_app_tc`) and to hosts/admins in the admin dashboard (`web_app_admin_tc`).

---

## 2. Proposed Outcome (The Vision)
A secure, contextual messaging thread tied directly to individual reservations (`booking_id`):
1. **Guest Experience**: A guest can compose and send messages to the villa host from their booking details view (`web_app_tc`) across `BookingStatus::Pending` (awaiting host/admin approval) as well as confirmed/completed stages, and review host/admin replies in chronological order with unread status indicators.
2. **Host & Admin Experience**: Hosts and administrators viewing a reservation within the admin console (`web_app_admin_tc`) see the full chronological conversation history, unread badges, and can reply directly to the guest.
3. **Async Email Notifications**: Sending a message triggers an asynchronous notification event dispatching an email notification to the recipient (guest or host).
4. **Auditability & Traceability**: All messages are permanently persisted with timestamps, sender attribution, read receipts (`read_at`), and strict authorization checks.

---

## 3. Scope Boundaries

### In Scope (MVP)
- **Data Persistence (`db_core`)**:
  - PostgreSQL table `booking_message` linked to `booking(id)` and `user(id)` with sender role tracking (`guest`, `host`, `admin`), message text (up to 2,000 chars), `read_at TIMESTAMPTZ`, and audit timestamps (`created_at`).
  - Efficient indexing on `(booking_id, created_at ASC)` and `(booking_id, read_at)` for unread counts.
- **Booking State Eligibility**:
  - Messages can be exchanged on bookings in `BookingStatus::Pending` (awaiting host/admin approval), `BookingStatus::Confirmed`, and `BookingStatus::Completed` states.
- **API Endpoints (`app_api/booking_api`)**:
  - `GET /api/v1/bookings/{id}/messages`: Retrieve chronological messages for a booking, including read status.
  - `POST /api/v1/bookings/{id}/messages`: Post a new message (validated <= 2,000 chars).
  - `PATCH /api/v1/bookings/{id}/messages/read`: Mark incoming messages as read, updating `read_at`.
  - Strict authorization: Caller must be the booking's guest (`booking.guest_id`), the property host (`listing.user_id`), or a platform admin.
  - Admin Sender Attribution: Messages posted by platform administrators must be clearly identified and formatted with `[Name] (admin)` (e.g. `John (admin)`).
- **Async Notification Event / Email Dispatch**:
  - Fire an asynchronous notification event when a message is persisted.
  - Dispatch email notification to the opposite party (guest if host sent; host if guest sent).
- **Client Integration (`web_app_common_tc`)**:
  - Centralized API client methods for fetching messages, posting messages, and marking messages as read.
- **Guest UI (`web_app_tc`)**:
  - Chronological message history component (Topcoat SSR / HTMX) on the booking details / My Bookings view.
  - Unambiguous sender labels distinguishing Guest, Host, and Admin (with admin senders clearly formatted as `[Name] (admin)`).
  - Message submission form with 2,000-character counter, validation, and auto-scroll.
  - Unread message indicators and auto-mark as read upon viewing.
- **Host / Admin UI (`web_app_admin_tc`)**:
  - Embedded messaging section inside bookings management (`bookings_admin.rs`) with unread badges and reply form.
  - Admin replies clearly badged as `[Name] (admin)`.

### Out of Scope / Non-Goals
- Real-time push via WebSockets or Server-Sent Events (SSE) — MVP relies on HTTP polling or view refresh.
- Multimedia or file attachments (images, PDFs, documents) — plain text only for MVP.
- General user-to-user direct messaging unattached to a booking.

---

## 4. Affected Systems & Stakeholders

- **User Personas**:
  - **Guests**: Users with pending holds or confirmed reservations communicating with hosts.
  - **Hosts**: Property owners (`listing.user_id`) managing guest inquiries for their villas.
  - **Administrators**: Operational admins supporting bookings and mediating communications.
- **System Components**:
  - `db_core`: New migration `create_booking_message_table.sql` and query functions.
  - `common`: Shared DTOs (`BookingMessageResponse`, `CreateBookingMessageRequest`, `BookingMessagesWrapper`).
  - `app_api/booking_api`: Route handlers, input validation (2,000 char limit), access control logic, and async event dispatch.
  - `web_app_tc`: Topcoat SSR / HTMX component for guest message thread view, unread indicators, and response submission.
  - `web_app_admin_tc`: Topcoat SSR / HTMX component for host/admin viewing, unread indicators, and reply submission.
  - `web_app_common_tc`: Shared HTTP client functions for booking message routes.
  - Notification Worker / Email Service: Event handler consuming message events and sending recipient notification emails.

---

## 5. Constraints & Non-Negotiable Invariants

- **Security & Authorization**:
  - Multi-tenant isolation: Unrelated guests or hosts of different properties must receive `403 Forbidden` if attempting to view or append messages to a booking.
  - Role-aware sender attribution: Senders cannot spoof roles; sender identity and role are derived from authenticated session claims. Messages authored by platform admins are strictly enforced to render as `[Name] (admin)` in both guest and admin views.
- **Data Integrity & Immutability**:
  - Messages are append-only audit records. No updating message text or deleting messages.
  - Text validation: Trim whitespace, enforce non-empty constraint, and enforce 2,000-character ceiling.
  - Input Sanitization & Safety:
    - Strip or reject malicious control characters (e.g. null bytes `\0`, unprintable ASCII control codes, unicode directional overrides).
    - Strict HTML entity escaping in Topcoat SSR templates (`web_app_tc` and `web_app_admin_tc`) to prevent XSS injection and broken layout formatting.
    - URL encoding: Enforce strict percent/URL-encoding for all dynamic route identifiers and query parameters across API endpoints and HTMX attributes to prevent parameter injection or malformed HTTP requests.
- **Performance & Cloud Run Invariants**:
  - Fast response time: Message fetch and post latency < 100ms (p95).
  - Async decoupling: Email dispatch must not block HTTP response latency (offloaded to background task or Pub/Sub).
  - Scale-to-zero compatibility: No background websocket connection pools or long-lived idle connections.
- **Coding Standards**:
  - Zero floating-point arithmetic.
  - Monadic error handling with no unwrap/expect in production code paths.

---

## 6. Success Metrics & Signals

- **Leading Indicators**:
  - 100% test pass rate for authorization guards (unauthorized access rejected with `403 Forbidden`).
  - 100% of persisted messages trigger an asynchronous notification event.
  - Guests and hosts can exchange messages and view updated conversation threads within 1 second of submission.
- **Lagging Indicators**:
  - Reduction in external support tickets regarding check-in inquiries and villa details.
  - High engagement rate on booking message threads across both pending holds and confirmed bookings.

---

## 7. Open Questions & Policy Concerns

- [x] **Booking State Eligibility**: Messages can be sent on bookings in `BookingStatus::Pending` (awaiting host/admin approval) as well as `BookingStatus::Confirmed` and `BookingStatus::Completed`.
- [x] **Read Status Tracking**: Schema and API will include `read_at TIMESTAMPTZ` and unread indicators in both guest and admin UIs.
- [x] **Character Length Limit**: Capped at 2,000 characters per message.
- [x] **Email Notifications**: Async notification event will be emitted upon message post to email the counterpart (host or guest).
