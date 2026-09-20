gcloud dns --project=our-places-dev managed-zones create ourplaces-dev-api-zone --description="" --dns-name="api.dev.ourplaces.io." --visibility="private" --networks="default"

# Generate certificate for API's
gcloud compute ssl-certificates create ourplaces-apicertdev \
    --description="Certificate for dev apis" \
    --domains=dev.api.ourplaces.io \
    --global

# TF version
resource "google_compute_managed_ssl_certificate" "lb_default" {
  provider = google-beta
  name     = "ourplaces-apicertdev"

  managed {
    domains = [dev.api.ourplaces.io]
  }
}


# List certs
gcloud compute ssl-certificates list \
   --global


# Our Places (`our_places_rs`)

High-performance, full-stack short-term property rental platform for luxury villas and apartments, structured as an Isomorphic Rust Monorepo targeting GCP Cloud Run scale-to-zero workloads.

---

## 🔄 AI-Native SDLC & Agent Skills Reference

The engineering workflow operates across 6 artifact-driven stages in an AI-Native Software Development Lifecycle (SDLC):

| SDLC Stage | Primary Artifact | Associated Skills | Execution Role & Trigger |
| :--- | :--- | :--- | :--- |
| **Stage 1: Plan** | `intent.md` | [`draft-intent`](.agents/skills/draft-intent/SKILL.md)<br>[`router`](.agents/skills/router/SKILL.md)<br>[`grill-me`](.agents/skills/grill-me/skill.md)<br>[`create-worktree`](.agents/skills/create-worktree/SKILL.md) | **Ideation & Scoping**: Brainstorm raw ideas, scope MVP boundaries, capture non-negotiable invariants, and isolate git worktrees. |
| **Stage 2: Design** | `spec.md` | [`generate-spec`](.agents/skills/generate-spec/SKILL.md)<br>[`grill-me`](.agents/skills/grill-me/skill.md)<br>[`assumption-review`](.agents/skills/assumption-review/SKILL.md)<br>[`edge-case-analysis`](.agents/skills/edge-case-analysis/SKILL.md)<br>[`failure-scenario-analysis`](.agents/skills/failure-scenario-analysis/SKILL.md)<br>[`resilience-exploration`](.agents/skills/resilience-exploration/SKILL.md)<br>[`security-review`](.agents/skills/security-review/SKILL.md)<br>[`security-posture-assessment`](.agents/skills/security-posture-assessment/SKILL.md)<br>[`risk-assessment`](.agents/skills/risk-assessment/SKILL.md)<br>[`vulnerability-analysis`](.agents/skills/vulnerability-analysis/SKILL.md) | **Specification & Policy Review**: Compress requirements and technical design into a single session; stress-test edge cases, failure blast radiuses, OWASP threats, and lock data models. |
| **Stage 3: Build** | `plan.md`<br>Source Code / Diff | [`rust-core`](.agents/skills/rust-core/SKILL.md)<br>[`monad-design`](.agents/skills/monad-design/SKILL.md)<br>[`topcoat`](.agents/skills/topcoat/SKILL.md)<br>[`daisyui`](.agents/skills/daisyui/SKILL.md)<br>[`lint-hunter`](.agents/skills/lint-hunter/SKILL.md)<br>[`general-debug`](.agents/skills/general-debug/SKILL.md)<br>[`write-new-skill`](.agents/skills/write-new-skill/skill.md)<br>[`handoff`](.agents/skills/handoff/skill.md) | **Implementation**: Execute code implementation starting from `plan.md`; enforce panic-free monadic Rust (`Option`/`Result`), tri-currency math, and SSR/UI components. |
| **Stage 4: Test** | Test Runs<br>`evals_results.json` | [`general-debug`](.agents/skills/general-debug/SKILL.md)<br>[`lint-hunter`](.agents/skills/lint-hunter/SKILL.md)<br>`chrome-devtools`<br>`a11y-debugging` | **Verification & Evals**: Run local test suites and compile checks; execute synthetic benchmark eval suites against agent skills to prevent prompt regressions; audit browser accessibility. |
| **Stage 5: Deploy** | `REVIEW.md`<br>Pull Request | [`pr-analyzer`](.agents/skills/pr-analyzer/SKILL.md)<br>[`pr-remediation`](.agents/skills/pr-remediation/SKILL.md) | **PR Review & Remediation**: Run 8-point automated code quality review, autonomously sweep and fix review comments/broken CI checks, and sync documentation post-ship. |
| **Stage 6: Maintain** | Incident `intent.md` | [`auto-triage-incident`](.agents/skills/auto-triage-incident/SKILL.md) | **Closed-Loop Incident Triage**: Ingest telemetry/log anomalies and metric breaches, isolate root causes, and write an `intent.md` proto-spec to restart Stage 1. |

---

## 🏛️ Architecture Documentation

* **System Architecture**: See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for monorepo crate taxonomies, database locking, and tri-currency pricing flows.
* **Agent Rules & Invariants**: See [`.agents/AGENTS.md`](.agents/AGENTS.md) for hard monorepo rules, performance budgets (<300ms), and token efficiency directives.

---

## 🛣️ Module Route Reference

### 1. `app_api/api_core` (Shared Actuator & Observability)

Configured across all Actix-web microservices via `api_core::actuator::configure_actuator` or `api_core::startup::run`:

| Method | Path | Description |
| :--- | :--- | :--- |
| `GET` | `/health` | Primary health check returning overall status and server uptime |
| `GET` | `/health/startup` | Kubernetes / Cloud Run startup probe |
| `GET` | `/health/liveness` | Container liveness probe |
| `GET` | `/health/readiness` | Database connection pool readiness probe |
| `GET` | `/metrics` | Prometheus metrics scrape endpoint (`http_requests_total`, `http_request_duration_seconds`) |
| `GET` | `/info` | Application build metadata (name, version, git commit hash, build timestamp) |
| `GET` | `/loggers` | List all active loggers and current log levels |
| `POST` | `/loggers` | Dynamically update the root logging level |
| `GET` | `/loggers/{name}` | Inspect the log level of a specific logger target |
| `POST` | `/loggers/{name}` | Dynamically update the log level of a specific logger target |

#### Dynamic Log Level Management & Secret Generation

Loggers target either the root (`ROOT`), a monorepo crate (`booking_api`, `listing_api`, `user_api`, `db_core`, `api_core`), third-party libraries (`sqlx`, `actix_web`), or granular sub-modules (`booking_api::apis`, `db_core::booking`).

Mutating log levels requires authentication via either **Method 1 (Pre-Shared Token)** or **Method 2 (Admin JWT)**:

##### Method 1: Pre-Shared Token (`X-Actuator-Token`) — Recommended
1. **Generate a Secret Token**:
   ```bash
   openssl rand -hex 32
   ```
2. **Configure Environment**:
   Add to `.env` (or Cloud Run environment):
   ```bash
   ACTUATOR_SECRET="<your-generated-token>"
   ```
   *(Note: Defaults to `"secret"` in local development if omitted).*
3. **Execute POST Request**:
   ```bash
   curl -X POST http://localhost:8081/loggers/booking_api::apis \
     -H "X-Actuator-Token: <your-generated-token>" \
     -H "Content-Type: application/json" \
     -d '{"configuredLevel": "TRACE"}'
   ```

##### Method 2: Admin JWT Bearer Token (`Authorization: Bearer <token>`)
1. **Obtain / Generate an Admin JWT** (signed with `JWT_SECRET` from `.env`):
   ```bash
   python3 -c "
   import jwt, time, os, uuid
   secret = os.getenv('JWT_SECRET', 'secret')
   payload = {'sub': str(uuid.uuid4()), 'role': 'admin', 'exp': int(time.time()) + 3600}
   print(jwt.encode(payload, secret, algorithm='HS256'))
   "
   ```
2. **Execute POST Request**:
   ```bash
   curl -X POST http://localhost:8081/loggers/booking_api::apis \
     -H "Authorization: Bearer <TOKEN>" \
     -H "Content-Type: application/json" \
     -d '{"configuredLevel": "TRACE"}'
   ```

##### Inspecting & Resetting Loggers
- **Inspect current level**:
  ```bash
  curl -X GET http://localhost:8081/loggers/booking_api::apis
  ```
- **Reset to standard level**:
  ```bash
  curl -X POST http://localhost:8081/loggers/booking_api::apis \
     -H "X-Actuator-Token: <your-generated-token>" \
     -H "Content-Type: application/json" \
     -d '{"configuredLevel": "INFO"}'
  ```

---

### 2. `app_api/booking_api` (Booking Engine & Messaging)

Core reservation state machine, availability locking, and guest messaging:

| Method | Path | Description |
| :--- | :--- | :--- |
| `GET` | `/api/docs/swagger-ui/{_:.*}` | Interactive Swagger UI API documentation |
| `GET` | `/api-docs/openapi.json` | OpenAPI 3.0 specification JSON |
| `GET` | `/api/v1/bookings/availability` | Check date availability and compute tri-currency pricing quotes with statutory taxes |
| `GET` | `/api/v1/bookings` | List all bookings with pagination and status filters |
| `POST` | `/api/v1/bookings` | Create a new reservation hold (`pending_payment` with 15-minute lock) |
| `GET` | `/api/v1/bookings/{id}` | Fetch booking details by booking UUID |
| `PATCH` | `/api/v1/bookings/{id}` | Update booking details or transition status |
| `DELETE` | `/api/v1/bookings/{id}` | Cancel or delete a reservation |
| `GET` | `/api/v1/bookings/user/{id}` | List all reservations for a specific user |
| `GET` | `/api/v1/bookings/listing/{id}` | List all reservations for a specific property listing |
| `POST` | `/api/v1/bookings/{id}/transfer` | Transfer booking ownership from a shadow user to a registered user |
| `GET` | `/api/v1/bookings/{id}/messages` | Retrieve message history for a booking |
| `POST` | `/api/v1/bookings/{id}/messages` | Send a new message on a booking thread |
| `PATCH` | `/api/v1/bookings/{id}/messages/read` | Mark booking messages as read |

---

### 3. `app_api/listing_api` (Listings, Pricing Overrides & Reviews)

Property catalog management, GCS V4 signed upload URLs, seasonal pricing rules, and guest reviews:

| Method | Path | Description |
| :--- | :--- | :--- |
| `GET` | `/api/docs/swagger-ui/{_:.*}` | Interactive Swagger UI API documentation |
| `GET` | `/api-docs/openapi.json` | OpenAPI 3.0 specification JSON |
| `GET` | `/api/v1/listings` | Search and filter villa listings with pagination |
| `POST` | `/api/v1/listings` | Create a new villa or apartment listing |
| `GET` | `/api/v1/listings/{id}` | Retrieve comprehensive listing details by UUID |
| `PATCH` | `/api/v1/listings/{id}` | Update listing details, amenities, or configuration |
| `DELETE` | `/api/v1/listings/{id}` | Delete a listing record |
| `POST` | `/api/v1/listings/{id}/images/presign` | Generate Google Cloud Storage (GCS) V4 Signed URLs for direct client uploads |
| `GET` | `/api/v1/listings/{id}/reviews` | Fetch paginated guest reviews and ratings for a listing |
| `GET` | `/api/v1/listings/{id}/price-overrides` | List seasonal and custom date-range price override rules |
| `POST` | `/api/v1/listings/{id}/price-overrides` | Create a seasonal or promotional price override rule |
| `PUT` | `/api/v1/listings/{id}/price-overrides/{override_id}` | Update an existing price override rule |
| `DELETE` | `/api/v1/listings/{id}/price-overrides/{override_id}` | Delete a price override rule |
| `GET` | `/api/v1/reviews/token/{token}` | Validate a review invitation token and fetch booking details |
| `POST` | `/api/v1/reviews/token/{token}` | Submit a verified guest review and ratings |
| `GET` | `/api/v1/reviews/booking/{booking_id}/token` | Fetch or generate a review invitation token for a completed stay |
| `POST` | `/api/v1/reviews/{id}/reply` | Submit a host reply to a guest review |

---

### 4. `app_api/user_api` (Authentication, Users & Sessions)

Identity management, JWT authentication, user lifecycle, and session tracking:

| Method | Path | Description |
| :--- | :--- | :--- |
| `GET` | `/api/docs/swagger-ui/{_:.*}` | Interactive Swagger UI API documentation |
| `GET` | `/api-docs/openapi.json` | OpenAPI 3.0 specification JSON |
| `POST` | `/api/v1/sessions` | Create a user session |
| `GET` | `/api/v1/sessions/{token_hash}` | Retrieve session details by token hash |
| `DELETE` | `/api/v1/sessions/{token_hash}` | Invalidate/revoke a session by token hash |
| `GET` | `/api/v1/users` | List all registered users with pagination and filtering |
| `POST` | `/api/v1/users` | Register a new user account |
| `GET` | `/api/v1/users/user/{email}` | Retrieve user profile by email address |
| `PATCH` | `/api/v1/users/user/{id}` | Update user profile information |
| `DELETE` | `/api/v1/users/user/{id}` | Soft-delete a user account |
| `DELETE` | `/api/v1/users/user/{id}/sessions` | Revoke all active sessions for a user |
| `POST` | `/api/v1/users/user/{id}/restore` | Restore a soft-deleted user account |
| `DELETE` | `/api/v1/users/user/{id}/hard` | Permanently purge a user record from the database |
| `GET` | `/api/v1/users/user/{email}/bookings` | Retrieve all bookings associated with a user email |
| `GET` | `/api/v1/users/user/{email}/listings` | Retrieve all listings owned by a user email |
| `POST` | `/api/v1/users/login` | Authenticate credentials and issue JWT / session token |
| `POST` | `/api/v1/users/verify` | Verify email address via verification token |
| `POST` | `/api/v1/users/resend-verification` | Resend verification email |
| `POST` | `/api/v1/users/profile/password/request` | Request password reset email |
| `POST` | `/api/v1/users/profile/password/confirm` | Reset password using confirmation token |
| `POST` | `/api/v1/users/profile/email` | Update user email address |
| `POST` | `/api/v1/users/profile/deactivate` | Deactivate user account |

---

### 5. `app_api/image_worker` (Image Processing Worker)

Event-driven background worker handling image optimization and thumbnail generation:

| Method | Path | Description |
| :--- | :--- | :--- |
| `GET` | `/api/docs/swagger-ui/{_:.*}` | Interactive Swagger UI API documentation |
| `GET` | `/api-docs/openapi.json` | OpenAPI 3.0 specification JSON |
| `POST` | `/api/v1/internal/image/process_image` | GCP Pub/Sub push subscription endpoint: resizes uploaded images to WebP (640px, 1024px, 1920px) and persists metadata |

---

### 6. `web_app_tc` (Guest-Facing Web App)

Public-facing Topcoat SSR & HTMX frontend (default port `3000`):

| Method | Path | Type | Description |
| :--- | :--- | :--- | :--- |
| `GET` | `/` | Page | Homepage with hero section, search form, and featured luxury villas |
| `GET` | `/htmx/welcome` | HTMX | Dynamic welcome partial |
| `GET` | `/about` | Page | About Our Places platform and team |
| `GET` | `/listings` | Page | Villa listings catalog with filter controls |
| `GET` | `/listings/filter` | HTMX | Filtered villa cards partial |
| `GET` | `/listings/{slug}` | Page | Detailed villa view with gallery, amenities, and booking widget |
| `GET` | `/listings/{slug}/quote` | HTMX | Dynamic pricing quote calculation for selected dates and currency |
| `GET` | `/checkout/{slug}` | Page | Reservation checkout flow for selected property |
| `GET` | `/checkout-jmd/{id}` | Page | Checkout flow with Jamaican Dollar (JMD) currency preset |
| `GET` | `/checkout-eur/{id}` | Page | Checkout flow with Euro (EUR) currency preset |
| `GET` | `/checkout-gbp/{id}` | Page | Checkout flow with British Pound (GBP) currency preset |
| `GET` | `/checkout-cad/{id}` | Page | Checkout flow with Canadian Dollar (CAD) currency preset |
| `GET` | `/bookings` | Page | Guest booking history and reservation status overview |
| `PATCH` | `/api/bookings/{id}` | Route | Modify or cancel a reservation |
| `GET` | `/bookings/{code}/messages` | Page | Guest messaging thread with host for a reservation |
| `POST` | `/bookings/{code}/messages` | Page | Submit a message on the guest reservation thread |
| `GET` | `/login` | Page | Guest login portal |
| `POST` | `/api/auth/login` | Route | Guest authentication handler |
| `GET` | `/logout` | Page | User logout and session termination |
| `GET` | `/register` | Page | Guest account registration |
| `GET` | `/verify` | Page | Account email verification confirmation |
| `GET` | `/profile` | Page | Guest profile details and settings |
| `GET` | `/reviews/submit` | Page | Guest review submission form via invitation token |
| `GET` | `/reviews/success` | Page | Review submission confirmation |

---

### 7. `web_app_admin_tc` (Internal Admin Dashboard)

Administrative Topcoat SSR & HTMX portal (default port `3002`):

| Method | Path | Type | Description |
| :--- | :--- | :--- | :--- |
| `GET` | `/admin`, `/` | Page | Admin dashboard with KPI metrics and recent booking activities |
| `GET` | `/admin/htmx/stats` | HTMX | Real-time analytics stats partial |
| `GET` | `/admin/listings`, `/listings` | Page | Property listings management table |
| `GET` | `/admin/listings/new` | Page | Create property form |
| `POST` | `/admin/listings`, `/api/admin/listings/create` | Route | Create property form submission handler |
| `GET` | `/admin/listings/{id}`, `/listings/{id}` | Page | Admin property details view |
| `GET` | `/admin/listings/{id}/edit`, `/listings/{id}/edit` | Page | Property edit form |
| `GET` | `/admin/listings/clone/{slug}` | Page | Clone property template form |
| `GET` | `/admin/listings/{id}/pricing`, `/listings/{id}/pricing` | Page | Seasonal rates and price overrides management |
| `GET` | `/admin/listings/{id}/pricing/add`, `/listings/{id}/pricing/add` | Page | Add seasonal price override rule |
| `GET` | `/admin/listings/{id}/pricing/remove` | Page | Remove price override rule |
| `GET` | `/admin/bookings`, `/bookings` | Page | Centralized bookings management pipeline |
| `PATCH` | `/api/bookings/{id}` | Route | Admin booking status or details update |
| `GET` | `/api/bookings/{id}/messages` | Route | Fetch booking message history |
| `POST` | `/api/bookings/{id}/messages` | Route | Send admin response message to guest |
| `PATCH` | `/api/bookings/{id}/messages/read` | Route | Mark booking messages as read |
| `GET` | `/admin/bookings/{code}/messages` | Page | Dedicated admin messaging thread |
| `POST` | `/admin/bookings/{code}/messages` | Page | Post message to guest on booking thread |
| `GET` | `/admin/users`, `/users` | Page | User management directory |
| `GET` | `/admin/users/new` | Page | Create user form |
| `POST` | `/api/admin/users/create` | Route | User creation API handler |
| `POST` | `/api/admin/users/update` | Route | Update user roles or account details |
| `GET` | `/admin/exchange-rates` | Page | Foreign currency exchange rate management & sync |
| `GET` | `/login` | Page | Admin login portal |
| `POST` | `/api/auth/login` | Route | Admin authentication handler |
| `GET` | `/logout` | Page | Admin logout and session termination |
