# Failure Scenario Analysis Reference

## 1. Outage Simulation Matrix

| Dependent System | Simulated Outage Mode | Expected System Behavior | Bad Failure Pattern (Prevent!) |
| :--- | :--- | :--- | :--- |
| **PostgreSQL Pool** | Exhausted connections | Return HTTP 503 `DB_BUSY` with backoff header. | Worker threads hang indefinitely until gateway timeout. |
| **External FX API** | 500 Internal Error / Slow (10s) | Serve cached exchange rate; log `tracing::warn!`. | Checkout flow fails completely with unhandled error. |
| **GCS Image Bucket** | Auth failure / 403 Forbidden | Return HTTP 502 `STORAGE_ERROR`; alert ops. | Panic on unwrap of signed URL response. |
| **Nominatim Geocoding** | Rate limit 429 Too Many Requests | Queue geocode task for retry; fallback to coords. | Listing creation blocks and rejects valid submission. |

---

## 2. Blast Radius Assessment Guide

When auditing a potential component failure:
1. **Direct Impact**: Which specific HTTP route or RPC handler fails?
2. **Cascading Effects**: Do other microservices sharing the database or message queue experience resource exhaustion?
3. **Data Contamination**: Can a partial write leave database records in an inconsistent state?
4. **Recovery Time**: Does the system recover automatically when the dependency returns, or does it require a restart?
