# Spec 67: Core Actuator Functionality

## Overview

In production microservice environments—specifically **GCP Cloud Run scale-to-zero serverless architectures**—operational visibility, telemetry, health inspection, container lifecycle orchestration, and runtime configuration management are paramount. Spring Boot Actuator is an industry standard for providing built-in operational HTTP endpoints (`/metrics`, `/health`, `/info`, `/loggers`).

This specification defines the design and implementation of **Core Actuator Functionality** for the Our Places Rust microservices platform (`app_api/api_core`, `listing_api`, `booking_api`, `user_api`, and `image_worker`). It delivers:
1. **GCP Cloud Run Container Lifecycle Probes (`/health/startup`, `/health/liveness`, `/health/readiness`) & Composite Health (`/health`)**: Native alignment with Google Cloud Run's container lifecycle hooks (Startup Probes, Liveness Probes, Readiness Probes) and composite database readiness (`SELECT 1`), ensuring graceful scale-to-zero cold-starts (<300ms p50) and robust traffic routing.
2. **System & Application Metrics (`/metrics`) with Google Cloud Observability**: Prometheus exposition endpoint powered by `metrics` and `metrics-exporter-prometheus` compatible with Google Cloud Managed Service for Prometheus (GMP), capturing HTTP request latency histograms, status counters, and PostgreSQL connection pool gauges.
3. **Google Cloud Trace & Logging Correlation**: Seamless integration with Cloud Run's request logging, tracing propagation (`X-Cloud-Trace-Context`), and structured JSON event routing.
4. **Build & Release Metadata (`/info`)**: Compile-time embedded build, Git commit SHA, branch, and runtime metadata returned as structured JSON.
5. **Dynamic Log Level Management (`/loggers`, `/loggers/{name}`)**: Inspectable and reloadable `tracing_subscriber` filter layers allowing operators to change logging levels (e.g., from `INFO` to `DEBUG` or `TRACE`) on live instances without restarting processes, secured with dual-mode authentication (Admin JWT or `X-Actuator-Token`).
6. **Legacy Health Route Supersession & Removal**: Fully deprecate, delete, and supersede the existing legacy `/health_check` endpoint (`app_api/api_core/src/health.rs`) and all service-prefixed routes (`/api/v1/listings/health_check`, `/api/v1/bookings/health_check`, `/api/v1/users/health_check`, `/api/v1/images/health_check`), updating `docker-compose.yml` and OpenAPI contracts to point to the new unified `/health` endpoints.

All actuator endpoints are mounted at root HTTP paths (`/health`, `/metrics`, `/info`, `/loggers`) and automatically registered across all backend microservices via `api_core::startup::run`.

---

## 1. Architecture Review & System Design

### 1.1. Data Flow Diagram

```
+---------------------------------------------------------------------------------------------------+
|                           GCP CLOUD RUN / INBOUND CLIENT TRAFFIC                                  |
+---------------------------------------------------------------------------------------------------+
                                                  |
                                                  v
                     +--------------------------------------------------------+
                     | Actix HttpServer (api_core::startup::run)             |
                     | - TracingLogger (X-Cloud-Trace-Context extraction)     |
                     | - ActuatorMetricsMiddleware (timing, path normalizer)   |
                     +--------------------------------------------------------+
                                                  |
           +--------------------+-----------------+--------------------+--------------------+
           |                    |                                      |                    |
           v                    v                                      v                    v
+---------------------+ +----------------------+            +--------------------+ +-------------------+
|  /health Probes     | |     GET /metrics     |            |     GET /info      | | /loggers (GET/POST) |
| - /health/startup   | |                      |            |                    | |                   |
| - /health/liveness  | |                      |            |                    | |                   |
| - /health/readiness | |                      |            |                    | |                   |
| - /health (composite) |                      |            |                    | |                   |
+---------------------+ +----------------------+            +--------------------+ +-------------------+
           |                    |                                      |                    |
           v                    v                                      v                    v
+---------------------+ +----------------------+            +--------------------+ +-------------------+
|  Health Component   | |  Prometheus Exporter |            |  Build Metadata    | |  Tracing Reload   |
|  - SELECT 1 (PgPool)| |  - PrometheusHandle  |            |  - vergen / env!   | |  - reload::Handle |
|  - 2s Timeout       | |  - Pool connection   |            |  - Git SHA/branch  | |  - Auth Guard     |
|  - UP/DOWN payload  | |    gauges            |            |  - Rustc/Arch JSON | |    (Admin JWT or  |
+---------------------+ +----------------------+            +--------------------+ |    Actuator Token)|
           |                    |                                      |           +-------------------+
           v                    v                                      v                    |
     PostgreSQL DB         Google Cloud GMP                     Ops / Release UI            v
 (Health verification)   (Managed Prometheus)                 (JSON Build Metadata)    Hot-reloads
                                                                                       active EnvFilter
```

### 1.2. Component Boundaries & Interfaces

```
                                  +------------------------------+
                                  |         Cargo.toml           |
                                  | - metrics                    |
                                  | - metrics-exporter-prometheus|
                                  +------------------------------+
                                                 |
                                                 v
+------------------------------------------------------------------------------------------------+
| app_api/api_core                                                                               |
|                                                                                                |
|  [actuator::health]   --> Cloud Run Probes: startup, liveness, readiness, composite            |
|  [actuator::metrics]  --> Installs global recorder; exposes PrometheusHandle; wraps requests   |
|  [actuator::info]     --> Collects compile-time vergen constants into AppInfoResponse          |
|  [actuator::loggers]  --> Inspects & updates tracing_subscriber::reload::Handle<EnvFilter>     |
|  [actuator::models]   --> Utoipa-annotated DTOs and schema definitions                         |
|  [startup::run]       --> Automatically registers Actuator scope & middleware on server boot   |
+------------------------------------------------------------------------------------------------+
                                                 |
                 +-------------------------------+-------------------------------+
                 |                               |                               |
                 v                               v                               v
    +--------------------------+   +--------------------------+   +--------------------------+
    |   app_api/listing_api    |   |   app_api/booking_api    |   |     app_api/user_api     |
    | - Removed legacy health  |   | - Removed legacy health  |   | - Removed legacy health  |
    | - Inherits root actuator |   | - Inherits root actuator |   | - Inherits root actuator |
    +--------------------------+   +--------------------------+   +--------------------------+
```

### 1.3. State & Error Paths

| Subsystem | State / Event | Error Condition | Handling Strategy |
|---|---|---|---|
| `/health/startup` | Cold-start boot | Initialization or migrations incomplete | Returns HTTP `503 Service Unavailable`; Cloud Run delays traffic until probe succeeds. |
| `/health/liveness` | Container runtime | Process hung or deadlocked | Returns HTTP `500`/no-response; Cloud Run restarts container instance after failure threshold. |
| `/health/readiness` | Dependency readiness | Database unreachable or connection pool exhausted | Returns HTTP `503 Service Unavailable`; Cloud Run stops routing traffic to instance. |
| `/health` | Composite check | Database query timeout (>2s) | Returns HTTP `503 Service Unavailable`, payload `{"status":"DOWN", "components":{"db":{"status":"DOWN","details":{"error":"..."}}}}`. |
| `/metrics` | Metric scrape | Concurrency / repeated scrape | Lock-free atomics in `metrics` crate; zero allocation overhead per scrape pass. |
| `/metrics` | Middleware path observation | Dynamic path parameter (`/api/v1/listings/{uuid}`) | Match pattern normalization (`req.match_pattern()`) prevents time-series explosion. |
| `/loggers` | GET /loggers | Unrecognized logger filter | Defaults to current root filter level. |
| `/loggers/{name}` | POST level mutation | Invalid level string (e.g. `"VERBOSE"`) | Returns HTTP `400 Bad Request` with valid levels list. |
| `/loggers/{name}` | POST level mutation | Missing or invalid auth header | Returns HTTP `401 Unauthorized`. |
| `/loggers/{name}` | POST level mutation | Dynamic reload failure | Logs error; rolls back filter; returns HTTP `500 Internal Server Error`. |

---

## 2. Requirements & Functional Contracts

### 2.1. Architectural Principles & Monorepo Boundaries
- **Crate Placement**: All actuator domain logic, handlers, middleware, schemas, and reloadable subscriber infrastructure live inside `app_api/api_core`.
- **Automatic Wiring**: Any service calling `api_core::startup::run(...)` automatically gains actuator endpoints and metric middleware without boilerplate duplication.
- **Standalone Opt-In**: A public helper `api_core::actuator::configure_actuator(&mut web::ServiceConfig)` is also exported for custom or isolated route setups (such as integration test harnesses).
- **Scale-to-Zero Efficiency**: Memory footprint overhead must remain negligible (< 5MB) and cold-start impact under 10ms to stay within the Cloud Run p50 < 300ms budget.
- **Zero Panic Policy**: No `.unwrap()` or `.expect()` calls in production handlers or middleware. Strict `Result<T, E>` error propagation and monadic chaining.
- **Complete Legacy Removal**: No legacy `/health_check` code or prefixed routes remain in `api_core` or downstream microservice crates.

---

### 2.2. GCP Cloud Run Lifecycle Probes & Composite Health

Google Cloud Run utilizes three distinct container lifecycle probes:
1. **Startup Probe (`GET /health/startup`)**:
   - **Cloud Run Role**: Checks whether container application has initialized. Cloud Run withholds all traffic and suppresses liveness/readiness probes until the startup probe succeeds.
   - **Check Behavior**: Confirms Actix HTTP listener is accepting connections, initial DB connection pool is established, and tracing is active.
   - **Response**: Returns HTTP `200 OK` `{"status": "UP"}` on completion, or HTTP `503 Service Unavailable` `{"status": "DOWN"}` during initialization failure.
2. **Liveness Probe (`GET /health/liveness`)**:
   - **Cloud Run Role**: Checks if the container process is alive and responsive. If failed `failureThreshold` times, Cloud Run restarts the container instance.
   - **Check Behavior**: Fast, non-blocking synchronous check confirming the Actix event loop is processing requests. Never performs external network/DB calls to prevent restart loops during transient DB outages.
   - **Response**: Returns HTTP `200 OK` `{"status": "UP"}` unconditionally.
3. **Readiness Probe (`GET /health/readiness`)**:
   - **Cloud Run Role**: Determines if the container instance is ready to receive user requests. If readiness fails, Cloud Run immediately stops routing traffic to that instance without restarting it.
   - **Check Behavior**: Verifies critical dependencies: executes `SELECT 1` on PostgreSQL `PgPool` with a 2-second timeout.
   - **Response**: Returns HTTP `200 OK` `{"status": "UP"}` when DB is reachable, or HTTP `503 Service Unavailable` `{"status": "DOWN"}` when DB query fails or times out.
4. **Composite Health (`GET /health`)**:
   - Aggregates all component states into a standard JSON payload:
     ```json
     {
       "status": "UP",
       "components": {
         "db": {
           "status": "UP",
           "details": {
             "database": "PostgreSQL",
             "latency_ms": 1.25
           }
         }
       }
     }
     ```
   - HTTP Status: `200 OK` when all components are `UP`; `503 Service Unavailable` when any component is `DOWN`.

#### Cloud Run Configuration Alignment
Services deployed to Cloud Run can be configured via service manifest or Terraform with:
```yaml
startupProbe:
  httpGet:
    path: /health/startup
    port: 8080
  initialDelaySeconds: 0
  periodSeconds: 2
  failureThreshold: 15
  timeoutSeconds: 2
livenessProbe:
  httpGet:
    path: /health/liveness
    port: 8080
  periodSeconds: 10
  failureThreshold: 3
  timeoutSeconds: 2
readinessProbe:
  httpGet:
    path: /health/readiness
    port: 8080
  periodSeconds: 5
  failureThreshold: 2
  timeoutSeconds: 2
```

---

### 2.3. Google Cloud Observability & Metrics (`GET /metrics`)

- Uses `metrics-exporter-prometheus` integrated with the Rust `metrics` facade.
- Single global recorder initialized during application bootstrap.
- Output adheres to Prometheus exposition text format (`text/plain; version=0.0.4; charset=utf-8`), natively scrapeable by **Google Cloud Managed Service for Prometheus (GMP)** or OpenTelemetry collectors.
- **Captured Metrics**:
  - `http_requests_total{method="GET|POST|...", path="...", status="200|400|500|..."}`: Counter of all inbound HTTP requests.
  - `http_request_duration_seconds{method="...", path="...", status="..."}`: Latency histogram for Cloud Monitoring SLO alerting.
  - `db_connections_active`: Active PostgreSQL connections (`PgPool::size() - PgPool::num_idle()`).
  - `db_connections_idle`: Idle connections (`PgPool::num_idle()`).
  - `db_connections_max`: Configured maximum pool size.
- **Cloud Trace Correlation**:
  - Middleware extracts Google Cloud Trace context from the `X-Cloud-Trace-Context` header (`TRACE_ID/SPAN_ID;o=TRACE_TRUE`) or W3C `traceparent`.
  - Attaches `logging.googleapis.com/trace: projects/${GCP_PROJECT}/traces/${TRACE_ID}` and `logging.googleapis.com/spanId` to structured tracing spans so application logs and Actix access logs correlate in Cloud Logging.

---

### 2.4. Application & Build Info (`GET /info`)

- Returns structured compile-time metadata embedded via `build.rs` using `vergen` / environment variables:
  ```json
  {
    "app": {
      "name": "our_places_api",
      "version": "0.1.0",
      "description": "Short-term luxury property rental backend microservice"
    },
    "git": {
      "branch": "feat/67-feat-implement-core-actuator-functionality",
      "commit_hash": "c3f81e8095b9d3b76a0c00010000000000000000",
      "commit_short_hash": "c3f81e8",
      "commit_timestamp": "2026-09-19T20:15:00Z"
    },
    "build": {
      "timestamp": "2026-09-19T20:20:00Z",
      "rustc_version": "rustc 1.85.0",
      "target_triple": "x86_64-unknown-linux-gnu",
      "profile": "release"
    }
  }
  ```
- Graceful fallbacks provided when Git metadata is absent (e.g., bare source tarballs without `.git`).

---

### 2.5. Logging Configuration & Dynamic Modification (`/loggers`)

- **List All Loggers (`GET /loggers`)**:
  - Returns supported log levels (`["OFF", "ERROR", "WARN", "INFO", "DEBUG", "TRACE"]`) and the current configured level:
    ```json
    {
      "levels": ["OFF", "ERROR", "WARN", "INFO", "DEBUG", "TRACE"],
      "loggers": {
        "ROOT": {
          "configuredLevel": "INFO",
          "effectiveLevel": "INFO"
        }
      }
    }
    ```
- **Get Specific Logger Level (`GET /loggers/{name}`)**:
  - Returns level for target logger (e.g. `ROOT`, `api_core`, `db_core`, `sqlx`).
- **Update Log Level at Runtime (`POST /loggers/{name}` or `POST /loggers`)**:
  - Accepts JSON payload:
    ```json
    {
      "configuredLevel": "DEBUG"
    }
    ```
  - Dynamically hot-reloads the global `tracing_subscriber::reload::Handle<EnvFilter>` without restarting the application.
  - Logs an audit event via `tracing::warn!("Log level dynamically changed to {} by authorized principal", new_level)`.
  - Returns HTTP `200 OK` with updated status or `204 No Content`.
- **Security & Access Control for `/loggers` Mutation**:
  - `GET /loggers` and `GET /loggers/{name}` are publicly queryable or queryable by internal mesh.
  - `POST /loggers/{name}` mutation requires authorization via either:
    1. **Internal Secret Token**: Header `X-Actuator-Token: <secret>` matching server environment variable `ACTUATOR_SECRET`.
    2. **Admin JWT**: Header `Authorization: Bearer <jwt>` containing valid token with claims `role: "admin"` (or `is_admin: true`).
  - If neither credential is valid or present, returns HTTP `401 Unauthorized` (`{ "code": "UNAUTHORIZED", "message": "Missing or invalid actuator credentials" }`).

---

### 2.6. Legacy Health Check Supersession & Deletion

- **Delete Legacy Module**: Remove `app_api/api_core/src/health.rs` and its declaration `pub mod health;` from `app_api/api_core/src/lib.rs`.
- **Remove Sub-Service Legacy Routes**:
  - Remove route `.route("/health_check", web::get().to(api_core::health::health_check))` and `api_core::health::health_check` from OpenAPI doc in:
    - `app_api/listing_api/src/apis.rs`
    - `app_api/booking_api/src/apis.rs`
    - `app_api/user_api/src/apis.rs`
    - `app_api/image_worker/src/apis.rs`
- **Update Container Health Checks**:
  - Modify `docker-compose.yml` healthcheck tests to use root `/health`:
    - `curl -f http://0.0.0.0:8080/health` (replacing `/api/v1/{service}/health_check`).
- **Update OpenAPI Specs**:
  - Remove `/api/v1/users/health_check`, `/api/v1/listings/health_check`, and `/api/v1/bookings/health_check` paths from `openapi.yaml` and `openapi-prod.yaml`.
  - Document unified root `/health`, `/health/startup`, `/health/liveness`, and `/health/readiness` endpoints.

---

## 3. Edge Case & Scale Analysis

1. **Database Unavailability During Health Check**:
   - If PostgreSQL pool is exhausted or DB is down, `health_check` and `readiness_check` must not hang indefinitely. They enforce a strict `tokio::time::timeout` (2s) and return HTTP `503 Service Unavailable` with `status: DOWN`.
2. **Liveness Independence from External Backends**:
   - `liveness_check` never calls the database. A database degradation should cause `readiness_check` to stop traffic, not cause Cloud Run to violently restart the container in an endless cascade.
3. **Prometheus High Cardinality Protection**:
   - HTTP route matching normalizes dynamic path parameters (e.g., `/api/v1/listings/018f2e23-74b5-7798-8f83-e18e80123456` becomes `/api/v1/listings/{id}`) to prevent memory exhaustion and metric explosion.
4. **Concurrent Metric Scrapes (1000x Normal Load)**:
   - Prometheus scrapers query `/metrics` at high frequency: `PrometheusHandle::render()` uses lock-free atomics and reads memory-mapped values without allocating per scrape.
5. **Invalid Log Level String in Mutation**:
   - If client submits an invalid level (e.g. `{"configuredLevel": "VERBOSE"}`), return HTTP `400 Bad Request` with `{ "code": "INVALID_LOG_LEVEL", "message": "Supported levels are: OFF, ERROR, WARN, INFO, DEBUG, TRACE" }`.
6. **Multiple Calls to Recorder Installation**:
   - Prometheus global recorder installation is guarded by `OnceCell` so concurrent calls in test suites will not panic.
7. **Stripped Container Environments (No Git)**:
   - If `.git` is omitted during minimal CI Docker builds, `build.rs` falls back to `option_env!` and Cargo package version without failing compilation.

---

## 4. Security Review

- **Authentication & Authorization**:
  - `/health`, `/health/startup`, `/health/liveness`, `/health/readiness`, `/metrics`, and `/info` are read-only operational endpoints.
  - `/loggers` mutating endpoint (`POST`) is strictly guarded: requires either `X-Actuator-Token` matching `ACTUATOR_SECRET` or a verified JWT with `role: "admin"`.
- **Input Validation**:
  - Request body for logger modification is deserialized into a strict enum/validated struct; invalid log levels are rejected before modifying runtime filters.
- **Data Exposure Risks**:
  - `/info` only reveals compile-time build version, git commit hash, and rustc version. No environment variables, secrets, or internal database credentials are exposed.
  - `/health` error messages in production return high-level category strings rather than full internal database stack traces or connection strings.
- **Dependency Vulnerabilities**:
  - Standard, audited crates from Tokio/metrics ecosystem (`metrics`, `metrics-exporter-prometheus`, `vergen`).

---

## 5. Technical Implementation

### 5.1. Workspace & Dependency Setup

#### [Cargo.toml](file:///home/pav/code/our_places_rs-feat-67-feat-implement-core-actuator-functionality/Cargo.toml)
Add actuator dependencies to `[workspace.dependencies]`:
```toml
metrics = "0.24.1"
metrics-exporter-prometheus = { version = "0.16.2", default-features = false, features = ["http-listener"] }
```

#### [app_api/api_core/Cargo.toml](file:///home/pav/code/our_places_rs-feat-67-feat-implement-core-actuator-functionality/app_api/api_core/Cargo.toml)
Include new dependencies and build dependencies:
```toml
[dependencies]
metrics.workspace = true
metrics-exporter-prometheus.workspace = true
parking_lot = "0.12.3"
once_cell = "1.20.2"

[build-dependencies]
vergen = { version = "8.3.2", features = ["build", "cargo", "git", "gitcl", "rustc"] }
```

---

### 5.2. Module Breakdown in `app_api/api_core`

Create a dedicated `actuator` module within `app_api/api_core` and delete the legacy `health.rs`:
```
app_api/api_core/src/
├── actuator/
│   ├── mod.rs             # Module re-exports, OpenAPI definitions, route configuration
│   ├── health.rs          # Startup, liveness, readiness, composite /health handlers
│   ├── metrics.rs         # Prometheus handle, metrics middleware, /metrics handler
│   ├── info.rs            # Compile-time build and git metadata handler
│   ├── loggers.rs         # Tracing reload handle, GET/POST handlers, auth guards
│   └── models.rs          # DTOs and ToSchema structs for Actuator endpoints
├── startup.rs             # Integrate actuator routes and metrics middleware automatically
├── tracing_utils.rs       # Enhance with reloadable subscriber layer & Cloud Trace support
├── lib.rs                 # Remove `pub mod health;`, add `pub mod actuator;`
└── health.rs              # DELETED (superseded by actuator/health.rs)
```

---

### 5.3. Detailed Component Implementation

#### 5.3.1. Reloadable Logging & Cloud Trace (`app_api/api_core/src/tracing_utils.rs` & `loggers.rs`)
1. In `tracing_utils.rs`:
   - Wrap the `EnvFilter` layer in `tracing_subscriber::reload::Layer`.
   - Store the `reload::Handle<EnvFilter, Registry>` in a global `OnceCell<tracing_subscriber::reload::Handle<EnvFilter, Registry>>`.
   - Update `CustomRootSpanBuilder` to parse `X-Cloud-Trace-Context` header and attach Google Cloud Trace identifiers for Cloud Logging correlation.
   - Expose public helper `pub fn reload_filter(filter_str: &str) -> Result<(), AppError>` to apply new directives dynamically.
2. In `loggers.rs`:
   - `GET /loggers`: Return active level and available levels.
   - `GET /loggers/{name}`: Return level for specific target.
   - `POST /loggers/{name}`: Verify either `X-Actuator-Token` or Admin JWT (`Authorization`). Parse new level, invoke `reload_filter`, and return updated status.

#### 5.3.2. Prometheus Metrics & Middleware (`app_api/api_core/src/actuator/metrics.rs`)
1. Create `ActuatorMetrics` holding the global `PrometheusHandle`.
2. Implement Actix middleware `MetricsMiddleware`:
   - Intercepts requests on start, records timestamp.
   - On response finish:
     - Extracts normalized path pattern (using `req.match_pattern()` or falling back to raw path for static routes) to prevent cardinality explosions.
     - Increments `http_requests_total` counter.
     - Observes elapsed duration in `http_request_duration_seconds` histogram.
3. Expose `GET /metrics` handler:
   - Queries `PgPool` active and idle connection counts and updates gauges.
   - Calls `handle.render()` and returns response with `text/plain; version=0.0.4; charset=utf-8`.

#### 5.3.3. Cloud Run Lifecycle Probes (`app_api/api_core/src/actuator/health.rs`)
1. `startup_check(pool: web::Data<PgPool>) -> HttpResponse`:
   - Verifies server readiness and database pool availability; returns 200 `{"status": "UP"}` or 503 `{"status": "DOWN"}`.
2. `liveness_check() -> HttpResponse`:
   - Lightweight, synchronous process check; returns HTTP 200 `{"status": "UP"}` unconditionally.
3. `readiness_check(pool: web::Data<PgPool>) -> HttpResponse`:
   - Runs `tokio::time::timeout(Duration::from_secs(2), sqlx::query("SELECT 1").execute(pool.get_ref()))`.
   - Returns 200 `{"status": "UP"}` when DB is reachable, or 503 `{"status": "DOWN"}` on failure.
4. `health_check(pool: web::Data<PgPool>) -> HttpResponse`:
   - Composite check returning detailed JSON with DB status and query latency.

#### 5.3.4. Build Information (`app_api/api_core/src/actuator/info.rs` & `build.rs`)
1. Add `build.rs` to `app_api/api_core`:
   - Uses `vergen` to emit `VERGEN_GIT_SHA`, `VERGEN_GIT_BRANCH`, `VERGEN_GIT_COMMIT_TIMESTAMP`, `VERGEN_BUILD_TIMESTAMP`, and `VERGEN_RUSTC_SEMVER`.
2. In `info.rs`:
   - Reads constants via `env!` / `option_env!`.
   - Returns structured `AppInfoResponse` JSON payload.

#### 5.3.5. Web Server Integration (`app_api/api_core/src/startup.rs`)
Update `startup::run`:
```rust
pub fn run<F>(
    listener: TcpListener,
    db_pool: PgPool,
    config_fn: F,
    settings: Settings,
) -> Result<Server, std::io::Error>
where
    F: FnOnce(&mut web::ServiceConfig) + Clone + Send + 'static,
{
    // Initialize Prometheus recorder once
    let prometheus_handle = crate::actuator::metrics::init_metrics_recorder();

    let db_pool = web::Data::new(db_pool);
    let settings = web::Data::new(settings);
    let metrics_handle = web::Data::new(prometheus_handle);

    let server = HttpServer::new(move || {
        App::new()
            .wrap(Cors::permissive())
            .wrap(crate::actuator::metrics::ActuatorMetricsMiddleware)
            .wrap(TracingLogger::<crate::tracing_utils::CustomRootSpanBuilder>::new())
            .app_data(db_pool.clone())
            .app_data(settings.clone())
            .app_data(metrics_handle.clone())
            .configure(crate::actuator::configure_actuator)
            .configure(config_fn.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}
```

#### 5.3.6. Cleanup of Downstream Services & Configuration
1. **`app_api/listing_api/src/apis.rs`**:
   - Remove `api_core::health::health_check` from `#[openapi(paths(...))]`.
   - Remove `.route("/health_check", web::get().to(api_core::health::health_check))` from `configure_routes`.
2. **`app_api/booking_api/src/apis.rs`**:
   - Remove `api_core::health::health_check` from `#[openapi(paths(...))]`.
   - Remove `.route("/health_check", web::get().to(api_core::health::health_check))` from `configure_routes`.
3. **`app_api/user_api/src/apis.rs`**:
   - Remove `api_core::health::health_check` from `#[openapi(paths(...))]`.
   - Remove `.route("/health_check", web::get().to(api_core::health::health_check))` from `configure_routes`.
4. **`app_api/image_worker/src/apis.rs`**:
   - Remove `api_core::health::health_check` from `#[openapi(paths(...))]` and routes.
5. **`docker-compose.yml`**:
   - Line 44: `test: [ "CMD", "curl", "-f", "http://0.0.0.0:8080/health" ]`
   - Line 75: `test: [ "CMD", "curl", "-f", "http://0.0.0.0:8080/health" ]`
   - Line 102: `test: [ "CMD", "curl", "-f", "http://0.0.0.0:8080/health" ]`
6. **`openapi.yaml` & `openapi-prod.yaml`**:
   - Delete obsolete `/api/v1/{service}/health_check` entries and add `/health` definitions.

---

## 6. Unit Test Cases

### 1. Cloud Run Health Probe Tests (`app_api/api_core/src/actuator/health_test.rs`)
- `test_startup_probe_healthy`: Verify `GET /health/startup` returns 200 OK and `{"status": "UP"}` after server initialization.
- `test_liveness_returns_up_200`: Verify `GET /health/liveness` returns 200 OK and `{"status": "UP"}` immediately without touching database.
- `test_readiness_with_healthy_db`: Verify `GET /health/readiness` returns 200 OK when database pool is healthy.
- `test_readiness_with_db_failure`: Verify `GET /health/readiness` returns 503 Service Unavailable when database pool is down.
- `test_composite_health_healthy`: Verify `GET /health` returns 200 OK with `status: "UP"`, component details for `db`, and query latency.
- `test_composite_health_db_failure`: Mock or inject closed connection pool and verify `GET /health` returns 503 Service Unavailable with `status: "DOWN"`.
- `test_legacy_health_check_removed`: Verify legacy `GET /health_check` endpoint returns 404 Not Found.

### 2. Metrics Tests (`app_api/api_core/src/actuator/metrics_test.rs`)
- `test_metrics_endpoint_render`: Verify `GET /metrics` returns 200 OK with `Content-Type: text/plain` containing Prometheus headers.
- `test_metrics_middleware_increments_request_counter`: Call a test route through Actix service and assert `http_requests_total` contains method, route pattern, and status code.
- `test_metrics_duration_histogram_recorded`: Assert `http_request_duration_seconds_bucket` is present in rendered output.
- `test_db_connection_pool_metrics_reported`: Assert `db_connections_active` and `db_connections_idle` are present and reflect pool state.

### 3. Info Tests (`app_api/api_core/src/actuator/info_test.rs`)
- `test_info_endpoint_returns_json`: Verify `GET /info` returns 200 OK with application name, semver version, git metadata, and build timestamp.
- `test_info_schema_validation`: Deserialize response into `AppInfoResponse` and ensure all fields are properly formed.

### 4. Logger Tests (`app_api/api_core/src/actuator/loggers_test.rs`)
- `test_get_loggers_returns_current_level`: Verify `GET /loggers` returns list of valid levels and current root filter level.
- `test_post_loggers_unauthorized_without_credentials`: Verify `POST /loggers/ROOT` with `{"configuredLevel": "DEBUG"}` without auth header returns 401 Unauthorized.
- `test_post_loggers_authorized_with_secret_token`: Verify `POST /loggers/ROOT` with `X-Actuator-Token: valid-secret` returns 200 OK and updates the active level.
- `test_post_loggers_authorized_with_admin_jwt`: Verify `POST /loggers/ROOT` with valid admin Bearer token returns 200 OK and reloads subscriber.
- `test_post_loggers_invalid_level_rejected`: Verify `POST /loggers/ROOT` with `{"configuredLevel": "BOGUS"}` returns 400 Bad Request.

---

## 7. Acceptance Criteria

- [ ] Legacy `app_api/api_core/src/health.rs` file and all `/api/v1/{service}/health_check` routes are deleted from `api_core`, `listing_api`, `booking_api`, `user_api`, and `image_worker`.
- [ ] `docker-compose.yml` healthcheck commands for `bookings`, `listing_api`, and `user_api` are updated to point to `http://0.0.0.0:8080/health`.
- [ ] Cloud Run container lifecycle probes are implemented and tested:
  - `GET /health/startup`: returns 200 OK when application initialization completes.
  - `GET /health/liveness`: returns 200 OK unconditionally as a fast process liveness probe.
  - `GET /health/readiness`: returns 200 OK when database is available, 503 Service Unavailable when database is unreachable.
- [ ] `GET /health` runs `SELECT 1` against PostgreSQL with a 2-second timeout, returning 200 OK `UP` with latency metadata when healthy and 503 Service Unavailable `DOWN` on failure.
- [ ] `GET /metrics` renders Prometheus exposition text scraping format compatible with Google Cloud Managed Service for Prometheus (GMP), containing `http_requests_total`, `http_request_duration_seconds`, and DB pool connection statistics.
- [ ] Dynamic path parameters are normalized in Prometheus metrics to prevent metric cardinality explosion.
- [ ] Tracing root span extracts `X-Cloud-Trace-Context` for Cloud Run and Google Cloud Logging trace correlation.
- [ ] `GET /info` returns structured JSON with application name, package version, Git commit SHA, branch, and build timestamp.
- [ ] `GET /loggers` returns current log levels and available level choices.
- [ ] `POST /loggers/{name}` dynamically updates the runtime log filter via reloadable `tracing_subscriber` layer.
- [ ] `POST /loggers/{name}` enforces authentication: allows requests with either a valid `X-Actuator-Token` header matching `ACTUATOR_SECRET` or an Admin JWT Bearer token, and rejects unauthorized requests with 401.
- [ ] All actuator endpoints are documented in OpenAPI / Utoipa specs where applicable, and obsolete healthcheck definitions are removed.
- [ ] Zero `.unwrap()` or `.expect()` calls in production actuator paths.
- [ ] All unit and integration tests for actuator endpoints pass via `cargo test -p api_core`.
