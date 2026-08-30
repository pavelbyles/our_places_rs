# Architecture Documentation — Our Places (`our_places_rs`)

## 1. Executive Summary & Architectural Goals

**Our Places** is an Isomorphic Rust monorepo designed for high-performance, low-cost property rental management for luxury villas in Jamaica. 

### Key Architectural Drivers
- **Cloud Run Scale-to-Zero Efficiency**: Designed to execute within lightweight container limits ($0.25\text{ vCPU}$, $256\text{MB RAM}$) with cold start targets under $300\text{ms}$ (p50) and $1\text{s}$ (p95).
- **Isomorphic Code Execution**: Domain models, pricing mathematics, and business validators are written once in Rust (`common`) and compiled to both WebAssembly (`web_app` frontend) and native machine code (`app_api` backend).
- **Financial Precision**: Absolute avoidance of floating-point arithmetic (`f32`/`f64`) across multi-currency conversions and tax calculations using `rust_decimal::Decimal`.
- **Atomic Reservation Concurrency**: Elimination of double-bookings through PostgreSQL row-level locks (`SELECT ... FOR UPDATE`) and 15-minute self-expiring date holds.

---

## 2. High-Level Component Architecture

The following diagram illustrates the crate boundaries, data access layers, and external service integrations within the monorepo:

```mermaid
flowchart TB
    subgraph s1["WebAssembly Frontend - Leptos"]
        WA["web_app - Guest Portal"]
        WAA["web_app_admin - Admin Dashboard"]
        WAC["web_app_common - Shared UI & API Client"]
    end
    subgraph s2["Shared Isomorphic Crates"]
        COM["common - Isomorphic Pricing & Ref Data"]
        DBC["db_core - SQLx PostgreSQL Entities & Migrations"]
    end
    subgraph s3["Actix-web API Services"]
        LA["listing_api - Listings & Geocoding"]
        BA["booking_api - Booking State Machine"]
        UA["user_api - Auth & Profiles"]
        IW["image_worker - Background Image Processor"]
    end
    subgraph s4["Google Cloud Platform"]
        PG[("PostgreSQL Database")]
        GCS[("Google Cloud Storage")]
        PS["GCP Pub/Sub Event Bus"]
    end
    WA --> WAC
    WAA --> WAC
    WA -.-> COM
    WAA -.-> COM
    LA --> COM
    LA --> DBC
    BA --> COM
    BA --> DBC
    UA --> COM
    UA --> DBC
    IW --> DBC
    DBC --> PG
    LA -- V4 Signed URL --> GCS
    GCS -- Upload Notification --> PS
    PS -- Push Event --> IW
    IW -- Optimized WebP Assets --> GCS
```

---

## 3. Monorepo Crate Taxonomy

| Crate | Target | Description | Dependencies |
| :--- | :--- | :--- | :--- |
| **`common/`** | WASM & Native | Isomorphic domain DTOs, `rust_decimal` pricing engine, statutory tax rates, static reference data. | None |
| **`db_core/`** | Native | PostgreSQL pool management, compile-time verified `sqlx` queries, migration scripts. | `common` |
| **`app_api/listing_api`** | Native | Villa creation, OSM Nominatim reverse geocoding, GCS V4 Signed URL generation, spatial search. | `common`, `db_core` |
| **`app_api/booking_api`** | Native | Availability checking, `SELECT FOR UPDATE` locking, 15-min reservation holds, payment orchestration. | `common`, `db_core` |
| **`app_api/user_api`** | Native | JWT authentication, bcrypt password hashing, shadow user registration and promotion. | `common`, `db_core` |
| **`app_api/image_worker`** | Native | Event-driven background service processing GCS image upload events to multi-resolution WebP. | `db_core` |
| **`web_app/`** | WASM / SSR | Public guest-facing Leptos application (villas, search, booking checkout, user portal). | `common`, `web_app_common` |
| **`web_app_admin/`** | WASM / SSR | Internal administration dashboard (exchanges rates, listings, user promotion, audits). | `common`, `web_app_common` |
| **`web_app_common/`** | WASM | Shared Leptos UI components (VillaCard, responsive image picture tags, centralized HTTP client). | `common` |

---

## 4. Key Subsystems & Technical Workflows

### 4.1. Booking Engine & Concurrency Control

To ensure **zero double-bookings** without relying on volatile caching layers (e.g. Redis), availability verification and hold allocation are enforced directly in PostgreSQL.

```mermaid
sequenceDiagram
    autonumber
    actor Guest
    participant Client as Leptos WASM Client
    participant API as booking_api (Actix)
    participant DB as PostgreSQL (db_core)

    Guest->>Client: Select Check-in / Check-out Dates
    Client->>API: POST /bookings/hold (listing_id, dates, guest_info)
    Note over API,DB: Begin Serializable Transaction
    API->>DB: SELECT * FROM listings WHERE id = $1 FOR UPDATE
    API->>DB: Check overlapping confirmed or pending_payment holds
    alt Dates Available
        API->>DB: INSERT INTO bookings (status = 'pending_payment', expires_at = NOW() + 15m)
        API->>DB: INSERT INTO booking_status_history (immutable audit log)
        DB-->>API: Transaction Commit Success
        API-->>Client: 201 Created (booking_id, hold_expires_at)
    else Dates Overlapping
        DB-->>API: Row Locked / Conflict Detected
        API-->>Client: 409 Conflict ("DATE_UNAVAILABLE")
    end
```

#### Reservation Hold Lifecycle
1. **Pending Hold Creation**: Unauthenticated or authenticated guests initiate checkout, creating a `pending_payment` status with a 15-minute `expires_at` window.
2. **Natural Expiry**: Expired holds (`expires_at < NOW()`) are automatically treated as available dates during subsequent `SELECT ... FOR UPDATE` checks, eliminating mandatory cleanup cron jobs.
3. **Shadow User Promotion**: Unauthenticated checkouts create a temporary guest identity ("shadow user"). Upon post-payment account creation, the identity is promoted seamlessly without invalidating date holds.

---

### 4.2. Isomorphic "Tri-Currency" Pricing Mathematics

Handling international transactions across multi-currency listings requires strict precision.

```
+------------------+      Exchange Rate      +---------------------+      Static Tax Math      +-------------------+
|  Base Currency   | ----------------------> |  Payment Currency   | ------------------------> |   Final Receipt   |
| (Villa Nightly)  |                         |  (Guest Checkout)   |                           | (Jamaican GCT 15%)|
+------------------+                         +---------------------+                           +-------------------+
```

- **Calculation Flow**:
  1. Convert Base Nightly Price (e.g. $350 USD) $\rightarrow$ Guest Payment Currency (e.g. $54,250 JMD) using active `currency_exchange_rates`.
  2. Compute base stay total: $\text{Payment Nightly} \times \text{Nights}$.
  3. Apply statutory tax rates (stored statically in `common/src/reference.rs`, e.g., Jamaican GCT 15%).
- **Precision Guarantee**: All monetary calculations use `rust_decimal::Decimal`. Floating point types (`f32`/`f64`) are prohibited in pricing paths.

---

### 4.3. High-Performance Asynchronous Image Pipeline

To minimize bandwidth usage on Cloud Run instances, raw image payloads bypass backend services entirely:

```mermaid
flowchart LR
    A[Leptos Client] -- 1. Request Signed Upload URL --> B[listing_api]
    B -- 2. Return V4 Signed URL --> A
    A -- 3. Upload Raw Image Direct --> C[Google Cloud Storage]
    C -- 4. Object Finalized Event --> D[GCP Pub/Sub]
    D -- 5. Push Notification --> E[image_worker]
    E -- 6. Generate 640px, 1024px, 1920px WebP --> C
    A -- 7. Render HTML srcset --> C
```

1. **Direct Signed Upload**: `listing_api` issues a short-lived GCP V4 Signed Upload URL.
2. **Client-Direct Push**: Client uploads high-resolution images directly to GCS.
3. **Pub/Sub Worker**: GCS upload events trigger `image_worker` to resize assets to preset resolutions (640px, 1024px, 1920px) and compress them into WebP format.
4. **Responsive Delivery**: Frontend renders standard `<picture>` and `srcset` tags allowing native browser resolution negotiation.

---

### 4.4. Verified Guest Review & Token Lifecycle State Machine

Authentic reviews are gated by single-use, time-bound verification tokens issued upon stay completion.

```mermaid
stateDiagram-v2
    [*] --> StayActive: Guest Booking Confirmed
    StayActive --> ReviewEligible: Stay Concluded (today >= date_to)
    ReviewEligible --> TokenActive: Token Issued (expires date_to + 15d)
    TokenActive --> TokenExpired: 15-Day Window Expired (410 Gone)
    TokenActive --> ReviewSubmitted: Review Posted (used_at = NOW())
    ReviewSubmitted --> HostReplied: Host Response Posted
    TokenExpired --> [*]
    HostReplied --> [*]
```

- **Eligibility Window**: Opens immediately post-checkout (`today >= date_to`) and strictly expires 15 days after checkout (`date_to + 15 days` at 23:59:59 UTC).
- **Single-Use Invalidation**: Review tokens are consumed atomically inside a PostgreSQL transaction (`UPDATE review_token SET used_at = NOW() WHERE token = $1 AND used_at IS NULL ... RETURNING ...`).
- **Rating Recalculation & Row Locking**: Review submission acquires a row lock (`SELECT id FROM listing WHERE id = $1 FOR UPDATE`) to recalculate property aggregate ratings (`overall_rating` and `review_count`) atomically without race conditions.

---


## 5. Security & Cross-Cutting Concerns

- **Authentication**: JWT tokens signed using `jsonwebtoken` algorithm (HMAC-SHA256). Extractors in Actix validate claims per-request.
- **Password Hashing**: User credentials hashed using `bcrypt` with work factor 12.
- **Database Safety**: SQL queries are verified at compile-time via `sqlx::query!`. Schema modifications require immutable migrations in `db_core/migrations/`.
- **Observability**: Distributed request tracing implemented via `tracing` crate with `#[instrument]` annotations on async service handlers.

---

## 6. Environment Configuration & Operational Commands

### Environment Variables (`.env`)
```bash
DATABASE_URL=postgres://postgres:postgres@localhost:5432/our_places
JWT_SECRET=super_secret_jwt_signing_key_32_chars
GCS_BUCKET_NAME=our-places-assets-dev
PUBSUB_TOPIC_ID=image-processing-topic
EXCHANGE_RATE_API_KEY=sync_api_key
```

### Local Development Orchestration
```bash
# Start infrastructure dependencies
docker compose up -d

# Run workspace compilation
cargo build --workspace

# Execute workspace unit & integration test suites
cargo test --workspace

# Database schema migrations
sqlx migrate run
```
