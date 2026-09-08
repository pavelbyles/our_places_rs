# Auto-Triage Incident Examples

## Example 1: Database Lock Timeout (Backend Microservice)

### Input Incident Telemetry
* **Source**: Production Cloud Logging / Alert Manager
* **Alert**: Spike in 500 error responses on transactional checkout endpoint
* **Log Snippet**:
  ```text
  ERROR api::handlers::checkout: failed to acquire row lock for entity 4f5a9b81...: db error: canceling statement due to statement timeout
  ```

### Generated Artifact (`docs/intent/intent-INC-20260907-checkout-lock-timeout.md`)

```markdown
# Intent: Transactional Checkout Lock Timeout Under Concurrent Requests

## Incident Metadata
- **Incident ID**: INC-20260907-checkout-lock-timeout
- **Timestamp**: 2026-09-07T22:15:00Z
- **Severity**: P0 - Critical
- **Affected Subsystems**: services/checkout, core/database
- **Origin**: Cloud Logging / 5xx Alert Spike

## Problem & Symptoms
Concurrent transactional checkout requests encountered database row-level lock statement timeouts, returning HTTP 500 errors to end users and leaving pending reservations unfulfilled.

### Evidence & Trace Data
- **Error Code / Status**: HTTP 500 / DB_LOCK_TIMEOUT
- **Relevant Logs / Stack Trace**:
  ```text
  ERROR api::handlers::checkout: failed to acquire row lock for entity 4f5a9b81... statement timeout
  ```
- **Trigger Conditions**: High-concurrency checkout bursts on the same entity within sub-second intervals.

## Root Cause Hypothesis
The handler is performing external HTTP requests and long-running validations *inside* an open database transaction while holding an exclusive lock, blocking competing threads.

## Proposed Outcome & Invariants
- Move external validations and preliminary computations outside the exclusive row-level lock transaction.
- Maintain atomic state transition and prevent double-booking anomalies.
- Enforce lock acquisition timeouts (< 100ms) with graceful retry or conflict error response.

## Reproduction & Verification Plan
- **Unit/Integration Test Target**: `checkout::tests::test_concurrent_checkout_contention`
- **Regression Command**: `cargo test -p checkout_service` / `npm test`

## Open Questions & Policy Checks
- [ ] Should retry logic with exponential backoff be introduced at the client or gateway tier?
- [ ] Does any lock release leak when an upstream connection is dropped?
```

---

## Example 2: Worker Memory Spike / OOM (Queue Worker)

### Input Incident Telemetry
* **Source**: Background Worker Queue / Sentry
* **Alert**: Batch worker container restarted due to out-of-memory (OOM) condition.
* **Log Snippet**:
  ```text
  WARN worker::processor: memory consumption exceeded 90% threshold processing payload batch_id=8b19a0-..., container killed by SIGKILL
  ```

### Generated Artifact (`docs/intent/intent-INC-20260907-worker-oom.md`)

```markdown
# Intent: Batch Queue Worker Out-Of-Memory During Large Payload Ingestion

## Incident Metadata
- **Incident ID**: INC-20260907-worker-oom
- **Timestamp**: 2026-09-07T22:15:00Z
- **Severity**: P1 - Major
- **Affected Subsystems**: workers/batch_processor
- **Origin**: Queue DLQ / Sentry Event

## Problem & Symptoms
Processing large uncompressed batch payloads caused background worker containers to exceed their allocated memory limit (256MB), triggering container restarts and unacknowledged message retry storms.

### Evidence & Trace Data
- **Error Code / Status**: Container OOMKilled / Message routed to Dead Letter Queue
- **Trigger Conditions**: Batch sizes exceeding 5,000 items or raw payload > 20MB.

## Root Cause Hypothesis
The worker parses the entire batch payload into memory simultaneously as an in-memory collection instead of streaming or chunking processing.

## Proposed Outcome & Invariants
- Stream and process batch payloads in chunks of 500 items to bound maximum memory usage.
- Fit within container memory limits without dropping queue messages.

## Reproduction & Verification Plan
- **Unit Test Target**: `worker::tests::test_large_payload_streaming_memory_bound`
- **Regression Command**: `cargo test -p worker` / `pytest tests/test_worker.py`
```
