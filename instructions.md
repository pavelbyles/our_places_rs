# Our Places: Villa Booking Platform

## 1. Project Overview
"Our Places" is a high-performance, full-stack short-term property rental platform for properties in my family's real estate portfolio. The platform is designed to handle international bookings, complex multi-currency logic, tax compliance (e.g., Jamaican GCT), and high-resolution image processing, all while maintaining lightning-fast frontend performance. The platform is intended to be used by both guests for booking, and hosts for managing their properties. Admins will use the platform to manage the properties and the bookings.

The project is built as an Isomorphic Rust Application, sharing core domain logic, mathematics, and data models seamlessly between the backend servers and the WebAssembly (WASM) frontend.

## 2. Technology Stack
- **Frontend**: Leptos (Rust-based, fine-grained reactivity) with TailwindCSS and DaisyUI.
- **Backend**: Actix-web (High-performance Rust async web framework).
- **Database**: PostgreSQL (Interfaced via sqlx for compile-time query verification) via SQLX.
- **Infrastructure**: Google Cloud Platform (GCP).
- **Compute**: Cloud Run (Scale-to-zero microservices).
- **Storage**: Google Cloud Storage (GCS) for images.
- **Events**: Pub/Sub for asynchronous background workers.
- **Architecture Pattern**: Domain-Driven Microservices (Listing API, Booking API, User API) with a shared common crate for isomorphic logic.

## 3. Core Domains & Architecture

### A. The Booking Engine (State Machine & Concurrency)
The booking system is designed to prevent race conditions (double-bookings) and maintain a cryptographically strict audit log for accounting.
- **Concurrency**: Uses PostgreSQL row-level locking (`SELECT ... FOR UPDATE` on the listing) rather than Redis to securely evaluate date overlaps.
- **Reservation Holds**: When a user initiates checkout, a `pending_payment` record is created with a 15-minute `expires_at` hold. This naturally releases dates if the checkout is abandoned, requiring zero background cleanup workers.
- **Shadow Users**: Unauthenticated users can initiate a checkout to secure the 15-minute hold. They are promoted to full users post-payment.
- **Immutable Audit Trail**: Every status change (pending_payment -> confirmed -> completed -> refunded) is logged in a `booking_status_history` table for Tax Administration Jamaica (TAJ) compliance and host dispute resolution.

### B. Isomorphic Pricing & "Tri-Currency" Logic
Handling money across borders requires strict precision to avoid floating-point rounding errors.
- **Shared Math**: The `common/src/pricing.rs` module uses `rust_decimal::Decimal`. This exact code runs in the browser (to show UI estimates instantly) and on the server (to enforce payment security).
- **Tri-Currency Architecture**:
  - **Base Currency**: What the host prices the villa in (e.g., USD).
  - **Display Currency**: What the user views the site in (Approximate).
  - **Payment Currency**: What the user explicitly checks out with (e.g., JMD, GBP, USD).
- **Flow**: The system always converts the Base nightly rate to the Payment currency before applying math (multiplying by nights and adding tax). This guarantees perfect accounting.
- **Decoupled Taxes**: A villa located in Jamaica applies a 15% GCT (stored statically in `common/src/reference.rs`), regardless of whether the listing is priced in USD and paid for in GBP.

### C. The High-Performance Image Pipeline
To keep bandwidth costs low and UI performance high, the application utilizes a multi-step, serverless image processing pipeline:
- **Direct Uploads**: The backend generates a V4 Signed URL, allowing the Leptos client to securely upload raw images directly to a GCS bucket (bypassing the Actix backend).
- **Event-Driven Processing**: The upload triggers a GCP Pub/Sub event, waking up a background Actix worker.
- **WebP Generation**: The worker resizes the image into preset resolutions (Mobile 640px, Tablet 1024px, Desktop 1920px) and converts them to the highly compressed WebP format.
- **Client-Side Resolution**: The API returns all URL resolutions to the frontend. The Leptos UI uses HTML `<picture>` and `srcset` tags to allow the user's browser to naturally download the optimal size, avoiding expensive backend filtering.

### D. Listings & Geographic Data
- **SEO & Routing**: Listings utilize automatically generated, collision-resistant URL slugs (`v-villa-name-1a2b3c`) for SEO-friendly routing.
- **Reverse Geocoding**: When a host inputs Latitude/Longitude, the backend automatically queries an external provider (OpenStreetMap/Nominatim) to extract and save the specific City/Locality for fast search filtering.
- **Dynamic Attributes**: Uses PostgreSQL JSONB columns coupled with GIN indexes to store and query highly dynamic villa attributes (amenities, pool details) without schema bloat.

## 4. Project Workspace Structure
The monorepo is divided into distinct crates to enforce boundaries:
- `db_core/`: PostgreSQL connection pooling, sqlx models, and migration scripts.
- `common/`: The Isomorphic crate. Contains strictly typed domain models, pricing mathematics, and static reference data (Regions, Taxes) compiled into both the frontend and backend.
- `app_api/`: The backend microservices.
  - `listing_api/`: Villa creation, search, geocoding, and image pre-signing.
  - `booking_api/`: State machine, availability locking, and payment orchestration.
  - `user_api/`: Authentication, host profiles, and shadow user promotion.
- `web_app/`: The public-facing Leptos WASM application.
- `web_app_admin/`: The internal dashboard for managing users, listings, and exchange rates.
- `web_app_common/`: Shared UI components (e.g., VillaCard, Optimized Images) and the centralized API client logic.

## 5. Development Guidelines
- **Migrations First**: Always utilize `sqlx migrate run` and ensure the `db_core` structs map exactly to the database schema.
- **Worktrees over Branches**: Use `git worktree` for handling concurrent feature development (e.g., `feat-46` and `feat-47`) to prevent database/schema collisions in your local environment.
- **Financial Precision**: NEVER use `f32` or `f64` for money or tax rates. Always use `rust_decimal::Decimal`.
- **Static Reference Data**: Non-volatile business rules (Country Names, Statutory Tax Rates) live in `common/src/reference.rs` to save DB round-trips. Volatile rules (Exchange Rates) live in the `currency_exchange_rates` DB table.