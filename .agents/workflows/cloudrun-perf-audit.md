---
description: Cloud Run scale-to-zero and performance audit — check cold start latency budgets (<300ms p50), async I/O blocking, direct GCS signed URL pipelines, and dependency bloat.
---

# /cloudrun-perf-audit — Cloud Run & Performance Audit

## Goal
Verify that all microservices and shared crates strictly adhere to Google Cloud Run scale-to-zero requirements, performance budgets, and asynchronous safety standards.

## When to Use
Run `/cloudrun-perf-audit` before launching new endpoints, introducing new crate dependencies, or refactoring media/file handling pipelines.

---

## Audit Checklist

### 1. Cold Start Budget & Dependency Bloat
* **Budget Targets**: $< 300\text{ms}$ (p50), $< 1\text{s}$ (p95) on Cloud Run ($0.25\text{ vCPU}$, $256\text{MB RAM}$).
* Review newly added dependencies in `Cargo.toml`:
  - Avoid heavy proc-macros or bloated runtime dependencies where lightweight alternatives exist.
  - Verify lazy database connection pool initialization so server startup is not blocked if Postgres is cold.

### 2. Async Runtime Health & Blocking I/O
> [!IMPORTANT]
> **NEVER run CPU-intensive or blocking synchronous I/O directly on Tokio async worker threads.**

- Verify that all blocking calls (e.g. image decoding/encoding, heavy cryptographic operations, synchronous disk reads) are wrapped in `tokio::task::spawn_blocking`:
  ```rust
  tokio::task::spawn_blocking(move || {
      // Synchronous/CPU-heavy task
  }).await??;
  ```
- Ensure connection pool acquisitions have reasonable acquisition timeouts to avoid exhausting Tokio worker pools.

### 3. Media Pipeline (Zero Raw Streaming)
> [!WARNING]
> **NEVER stream raw image upload bytes through Actix HTTP endpoints.**

- Verify that image uploads exclusively issue GCP V4 Signed URLs via `listing_api`.
- Confirm that WebP resizing is completely offloaded to the Pub/Sub-triggered `image_worker` background service.
- Verify frontend uses `<picture>` and `srcset` tags for 640px (mobile), 1024px (tablet), and 1920px (desktop) delivery.

### 4. Distributed Tracing
- Ensure all new public async handlers and operations are annotated with `#[tracing::instrument]`.
- Verify errors are logged using `tracing::error!` or `tracing::warn!` with structured contextual fields before returning JSON error responses.
