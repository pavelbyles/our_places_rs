# Resilience Exploration Reference

## 1. Failure Taxonomy & Invariant Checks

| Subsystem | Failure Scenario | Required Resilience Mechanism |
| :--- | :--- | :--- |
| **PostgreSQL** | Connection pool starvation | Timeout fail-fast, connection queue limits, read-only replica fallback. |
| **Exchange Rate API** | External rate API outage / timeout | Use cached rates from DB table with statutory fallback rather than blocking checkout. |
| **GCS Signed URLs** | Network timeout generating URL | Return explicit retryable error (`STORAGE_SERVICE_UNAVAILABLE`). |
| **Pub/Sub Worker** | Worker container crash during image resize | Message ack deadline retry, Dead-Letter Queue (DLQ) after 5 retries. |
| **Concurrency** | Concurrent booking attempts on same villa | PostgreSQL row lock (`SELECT ... FOR UPDATE`), 15-minute hold expiration. |

---

## 2. Idempotency Checklist

- [ ] All payment processing endpoints require an `Idempotency-Key` header.
- [ ] Status transitions check current state before updating (`WHERE status = 'pending_payment'`).
- [ ] Database migrations are strictly additive and backwards-compatible.
