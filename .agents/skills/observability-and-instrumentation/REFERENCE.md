# Observability & Instrumentation Reference Guide

## 1. Multi-Stack Implementation Patterns

### Rust (Actix-Web / Tokio / Tracing)

#### Structured Logging with `tracing` & Spans
```rust
use tracing::{info, warn, error, instrument};
use uuid::Uuid;

#[instrument(
    skip(payload),
    fields(
        request_id = %Uuid::new_v4(),
        listing_id = %payload.listing_id,
        user_id = %payload.user_id
    )
)]
pub async fn handle_create_booking(
    payload: web::Json<BookingRequest>,
) -> Result<HttpResponse, AppError> {
    info!(event = "booking_attempt_started", "Processing new booking hold");

    match execute_booking_hold(&payload).await {
        Ok(reservation) => {
            info!(
                event = "booking_hold_created",
                reservation_id = %reservation.id,
                total_cents = %reservation.total_amount,
                "Reservation hold successfully locked"
            );
            Ok(HttpResponse::Created().json(reservation))
        }
        Err(err) => {
            warn!(
                event = "booking_hold_rejected",
                error_code = %err.code(),
                reason = %err.to_string(),
                "Unable to create reservation hold"
            );
            Err(err)
        }
    }
}
```

#### Metrics Instrumentation with `metrics` Crate
```rust
use metrics::{counter, histogram};
use std::time::Instant;

pub async fn execute_external_payment(order_id: &str, amount: Decimal) -> Result<(), PaymentError> {
    counter!("payment_attempts_total", "provider" => "stripe").increment(1);
    let start = Instant::now();

    let result = call_gateway(order_id, amount).await;
    let duration_sec = start.elapsed().as_secs_f64();

    histogram!("payment_duration_seconds", "provider" => "stripe").record(duration_sec);

    if let Err(ref e) = result {
        counter!(
            "payment_failures_total",
            "provider" => "stripe",
            "failure_reason" => e.error_kind()
        ).increment(1);
    }

    result
}
```

---

### TypeScript / Node.js (Pino / Prom-Client / OpenTelemetry)

#### Structured Child Logging with Correlation ID
```typescript
import pino from 'pino';
import { Request, Response, NextFunction } from 'express';
import crypto from 'node:crypto';

export const logger = pino({
  level: process.env.LOG_LEVEL || 'info',
  formatters: {
    level: (label) => ({ level: label }),
  },
});

export function correlationMiddleware(req: Request, res: Response, next: NextFunction) {
  const correlationId = (req.headers['x-request-id'] as string) || crypto.randomUUID();
  req.id = correlationId;
  res.setHeader('x-request-id', correlationId);

  req.log = logger.child({
    requestId: correlationId,
    method: req.method,
    route: req.route?.path || req.path,
  });

  next();
}
```

#### RED Metrics with `prom-client`
```typescript
import { Counter, Histogram } from 'prom-client';

export const httpRequestDuration = new Histogram({
  name: 'http_request_duration_seconds',
  help: 'HTTP request duration in seconds',
  labelNames: ['method', 'route', 'status_class'],
  buckets: [0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0],
});

export const httpRequestsTotal = new Counter({
  name: 'http_requests_total',
  help: 'Total number of HTTP requests handled',
  labelNames: ['method', 'route', 'status_class'],
});
```

---

### Python (Structlog / Prometheus Client)

```python
import time
import structlog
from prometheus_client import Histogram, Counter

logger = structlog.get_logger()

REQUEST_DURATION = Histogram(
    "http_request_duration_seconds",
    "Request latency",
    ["method", "endpoint", "status_class"],
    buckets=[0.05, 0.1, 0.25, 0.5, 1.0, 2.5]
)

def handle_request(method: str, endpoint: str, correlation_id: str):
    log = logger.bind(request_id=correlation_id, endpoint=endpoint)
    log.info("request_started", event="request_received")
    
    start_time = time.perf_counter()
    status_class = "2xx"
    try:
        # Business logic
        pass
    except Exception as exc:
        status_class = "5xx"
        log.error("request_failed", exc_info=exc, event="internal_error")
        raise
    finally:
        duration = time.perf_counter() - start_time
        REQUEST_DURATION.labels(method=method, endpoint=endpoint, status_class=status_class).observe(duration)
        log.info("request_finished", duration_seconds=duration, event="request_completed")
```

---

### Go (`log/slog` & Prometheus)

```go
package telemetry

import (
	"context"
	"log/slog"
	"net/http"
	"time"

	"github.com/prometheus/client_golang/prometheus"
)

var RequestLatency = prometheus.NewHistogramVec(
	prometheus.HistogramOpts{
		Name:    "http_request_duration_seconds",
		Help:    "Latency of HTTP requests in seconds",
		Buckets: []float64{0.05, 0.1, 0.25, 0.5, 1.0, 2.5},
	},
	[]string{"method", "path", "status_class"},
)

func InstrumentHandler(logger *slog.Logger, path string, next http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		reqID := r.Header.Get("X-Request-ID")
		if reqID == "" {
			reqID = "gen-" + r.RemoteAddr
		}
		ctx := context.WithValue(r.Context(), "request_id", reqID)
		reqLogger := logger.With("request_id", reqID, "path", path)

		start := time.Now()
		reqLogger.Info("request_started", "event", "http_request_inbound")

		next(w, r.WithContext(ctx))

		duration := time.Since(start).Seconds()
		RequestLatency.WithLabelValues(r.Method, path, "2xx").Observe(duration)
		reqLogger.Info("request_completed", "duration_sec", duration, "event", "http_request_outbound")
	}
}
```

---

## 2. Cardinality Safety Matrix

| Key / Dimension | Metric Label Safe? | Correct Storage Target | Rationale |
|---|---|---|---|
| `route` (e.g., `/api/v1/listings/{id}`) | **YES** | Metric label | Parameterized template; small finite cardinality. |
| `raw_url` (e.g., `/api/v1/listings/123-abc`) | **NO** | Tracing span or log event | Unbounded permutations explode memory. |
| `status_class` (e.g., `2xx`, `4xx`, `5xx`) | **YES** | Metric label | Capped at 5 fixed buckets. |
| `http_status` (e.g., `200`, `404`, `500`) | **CAUTION** | Metric label | Acceptable if restricted to standard codes; avoid arbitrary codes. |
| `user_id` / `account_id` | **NO** | Structured log / Trace attribute | High cardinality; creates millions of timeseries. |
| `error_code` (e.g., `ERR_INSUFFICIENT_FUNDS`) | **YES** | Metric label | Bounded domain enum. |
| `error_message` (e.g., `"failed at row 42: ..."`)| **NO** | Structured log | Contains free-form text and stack data. |

---

## 3. Pre-Launch Observability Checklist

- [ ] **On-Call Questions Defined**: 2–4 diagnostic questions written before deploying.
- [ ] **JSON Structured Logs**: All logs emit structured JSON with standardized `event`, `level`, and timestamp fields.
- [ ] **Correlation ID Propagation**: Correlation ID (`x-request-id` or W3C `traceparent`) propagated across all incoming, outgoing, and queue operations.
- [ ] **Zero Secret Leaks**: Passwords, API tokens, card details, and PII excluded from logs and traces.
- [ ] **RED Metrics Live**: Request Rate, Error Rate, and Duration Histograms visible for all service endpoints.
- [ ] **Bounded Cardinality**: Verified zero unbounded identifiers used as metric labels.
- [ ] **Distributed Traces Verified**: End-to-end trace view demonstrates complete parent-child hierarchy without orphaned spans.
- [ ] **Actionable Alerts with Runbooks**: Alerts fire on symptoms (SLO breach, error spike), not internal resource causes, and contain functional links to documentation.
