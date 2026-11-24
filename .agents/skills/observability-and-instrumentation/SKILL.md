---
name: observability-and-instrumentation
aliases:
  - telemetry
  - structured-logging
  - tracing
  - metrics-instrumentation
description: Instruments code so production behavior is visible and diagnosable. Use when adding logging, metrics, tracing, or alerting. Use when shipping any feature that runs in production and you need evidence it works. Use when production issues are reported but you can't tell what happened from the available data.
version: 2.0.0
---

# Observability and Instrumentation

## Overview

Code you cannot observe is code you cannot reliably operate. Observability is the capacity to answer **"what is the system doing and why?"** from the outside, using telemetry emitted by the running software.

Instrumentation is not an afterthought added post-launch — it is authored alongside domain code and tests. Telemetry without a concrete question is noise; metrics tell you *that* something is wrong, traces show *where*, and structured logs explain *why*.

---

## When to Use

- Building any feature, endpoint, or background worker that will run in production.
- Introducing external dependencies (third-party APIs, message brokers, databases, cache layers).
- Reviewing PRs adding asynchronous queues, retry loops, or cross-service network calls.
- Defining production alerting rules, SLOs, or diagnostic runbooks.
- Post-incident follow-ups where root cause diagnosis was delayed by missing telemetry.

### When NOT to Use
- Active incident triage happening right now (use `/investigate` or `general-debug`).
- Profiling memory or CPU bottlenecks on local machines (use dedicated profiling tools).
- Deploying or running rollback scripts (use deployment workflows).

---

## The 6-Step Instrumentation Process

```
┌────────────────────────────────────────────────────────┐
│ 1. Define On-Call Questions                            │
│                        │                               │
│                        ▼                               │
│ 2. Select Signal Type (Logs vs. Metrics vs. Traces)    │
│                        │                               │
│                        ▼                               │
│ 3. Implement Structured Logging (Correlation ID + JSON)│
│                        │                               │
│                        ▼                               │
│ 4. Instrument RED / USE Metrics (Bounded Cardinality)  │
│                        │                               │
│                        ▼                               │
│ 5. Propagate Distributed Tracing Spans                 │
│                        │                               │
│                        ▼                               │
│ 6. Define Symptom-Based Alerts & Runbooks              │
└────────────────────────────────────────────────────────┘
```

---

### Step 1: Define On-Call Questions Before Instrumenting
Telemetry without a hypothesis produces log bloat. Before writing instrumentation code, establish 2–4 concrete questions an on-call engineer will ask during a degradation:

```
FEATURE: Payment checkout retry mechanism
ON-CALL QUESTIONS:
1. What percentage of payments succeed on attempt 1 vs attempt 2+?
2. When payment fails permanently, what is the exact error code distribution?
3. Is external payment gateway latency degrading across regions?
→ Every metric, span, and log statement must directly answer one of these questions.
```

---

### Step 2: Select the Right Signal

| Signal | Core Question | Data Profile | Example |
|---|---|---|---|
| **Metrics** | *"How often / how fast in aggregate?"* | Fixed cost per time-series; aggregated counters & histograms | `http_request_duration_seconds` (p95, p99) |
| **Traces** | *"Where did time go across the call graph?"* | Per-request spans with parent-child hierarchy; sampled | Trace showing 450ms spent in DB query |
| **Logs** | *"What exact contextual state caused this event?"* | Event-driven JSON records; detailed key-value metadata | `payment_declined` with gateway reason code |

---

### Step 3: Structured Logging Discipline

1. **Log Events, Not Free-Form Prose**:
   - Emit machine-readable key-value pairs (JSON format).
   - Use stable, searchable event identifiers (`event = "order_placed"`, `event = "user_auth_failed"`).

2. **Mandatory Correlation IDs**:
   - Accept or generate a unique request ID (`x-request-id`, `traceparent`) at the system boundary.
   - Attach the correlation ID to every log line, span, and outbound network request.

3. **Entry Point Attribution**:
   - If a background job or handler is invoked via multiple triggers (HTTP, cron scheduler, message queue, manual CLI), tag the initiating boundary explicitly (`entry_point = "scheduler"` vs `entry_point = "http_api"`).

4. **Consistent Log Levels**:
   - `ERROR`: Invariant broken or user operation failed; requires human triage.
   - `WARN`: Degraded execution that was handled (e.g., retry succeeded, fallback used).
   - `INFO`: Significant lifecycle or business milestone (service started, payment completed).
   - `DEBUG`: Fine-grained operational details (disabled in production environments).

5. **Strict PII & Secret Redaction**:
   - Never log passwords, API keys, JWTs, card numbers, or full unredacted customer PII.
   - Allowlist logged attributes; never dump raw HTTP request or response bodies.

---

### Step 4: Metrics & Cardinality Defense

#### RED Method (for Request-Driven Services & APIs)
- **Rate**: Number of requests served per second (`http_requests_total`).
- **Errors**: Number of failed requests per second (`http_requests_failed_total`).
- **Duration**: Distribution of request latencies tracked as histograms (p50, p95, p99). Never track latencies as plain averages.

#### USE Method (for Resources, Pools, and Queues)
- **Utilization**: Percentage of time the resource is busy (e.g. CPU, active DB connections).
- **Saturation**: Volume of queued work waiting for service (e.g. connection pool wait queue, message backlog).
- **Errors**: Error count emitted by the resource (e.g. pool checkout timeout, worker crash).

#### The Cardinality Law
Every unique combination of metric label values creates an independent time-series in memory.
- **Permitted Labels (Fixed, Low Cardinality)**: `route = "/api/v1/bookings"`, `method = "POST"`, `status_class = "5xx"`, `provider = "stripe"`.
- **Prohibited Labels (Unbounded, High Cardinality)**: User IDs, email addresses, order IDs, UUIDs, full URLs, error message strings.

---

### Step 5: Distributed Tracing & Span Propagation

- Propagate W3C Trace Context (`traceparent`, `tracestate`) across all process and network boundaries (HTTP headers, message queue envelopes).
- Create spans around meaningful internal work units (e.g. database transactions, external RPC calls, cryptographic operations).
- Attach contextual attributes to spans to facilitate filtering (e.g. `db.system = "postgresql"`, `http.status_code = 200`).
- Ensure spans survive async boundaries and thread pool handoffs.

---

### Step 6: Symptom-Based Alerting & Minimum Viable Runbooks

Alert on **symptoms users experience**, not root causes:
- **Symptom (Alert/Page)**: Checkout error rate $> 2\%$ for 5 minutes, or p99 latency $> 1.5\text{s}$.
- **Cause (Dashboard)**: Host CPU at $85\%$, or connection pool utilization at $80\%$.

Every alert must satisfy three requirements:
1. **Actionable**: If the standard response is "ignore it, it will clear", remove the alert.
2. **Severities Capped at Two**: `Page` (immediate user impact) and `Ticket` (investigate within normal workflow).
3. **Linked to a Runbook**: Every alert must link to an actionable runbook in `docs/runbooks/`.

```markdown
# Minimum Viable Runbook (MVR) Template
**Alert Name:** HighErrorRateOnCheckout
**User Impact:** Customers unable to complete bookings; transaction drop.
**Immediate Triages:**
1. Check gateway status dashboard: `https://status.example.com`
2. Run database connection query: `SELECT count(*) FROM pg_stat_activity;`
3. Inspect structured logs: `event: "checkout_failed" AND error_class: "5xx"`
**Mitigation:** Roll back latest deployment if error spike aligns with release timestamp.
**Escalation:** On-call engineering rotation / Payment platform lead.
```

---

## Anti-Rationalization Table

| Rationalization | Reality | Enforcement |
|---|---|---|
| *"I'll add logging once the feature is working."* | Code ships without telemetry; the first incident requires blind guesswork. | Write structured logs and spans alongside feature code. |
| *"More log lines means better observability."* | Unstructured, verbose text floods log aggregators and drowns out critical errors. | Emit structured JSON events with standardized keys. |
| *"Average latency is 120ms, so we are fine."* | Averages hide the 1% or 5% of users experiencing 5-second timeouts. | Always use histograms and monitor p95 / p99 percentiles. |
| *"Putting user ID in metric labels helps us debug."* | Blows up metric backend cardinality and causes monitoring outages. | Keep user IDs in logs and traces, never in metric labels. |
| *"We don't need a runbook for this simple alert."* | When paged at 3 AM under stress, engineers forget basic queries and escalation paths. | Every alert must link to an actionable runbook. |

---

## Verification & Telemetry Gate

Before marking instrumentation complete:
- [ ] On-call diagnostic questions documented.
- [ ] Logs emit structured JSON with correlation IDs (`x-request-id` / trace context).
- [ ] No passwords, API keys, or unredacted PII in log records.
- [ ] RED metrics implemented with strictly bounded labels (zero high-cardinality keys).
- [ ] Latency recorded as a histogram with sensible bucket distributions.
- [ ] Distributed trace context verified across async and network boundaries.
- [ ] Alerts are symptom-based and link to verified runbooks.

*For stack-specific code implementations (Rust `tracing`, TypeScript, Python, Go), see [REFERENCE.md](file:///home/pav/code/our_places_rs-update-agent-md/.agents/skills/observability-and-instrumentation/REFERENCE.md).*
