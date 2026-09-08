# Draft Intent (Proto-Spec) Examples

## Example 1: Multi-Currency Host Payouts

### Originator Input (Raw Idea)
> *"Our property owners in Jamaica want to receive their earnings in either USD or JMD directly to their local Jamaican bank accounts. Right now they have to accept manual wire transfers and they keep complaining about exchange rate confusion."*

### Generated Intent Artifact (`docs/intent/intent-host-multi-currency-payouts.md`)

```markdown
# Intent: Multi-Currency Automated Host Payouts (USD & JMD)

## Metadata
- **Author**: Operations & Host Partnerships Team
- **Date**: 2026-09-07
- **Status**: Approved
- **Target Area**: Backend Services & Host Admin UI

## 1. Problem Statement & Motivation
Property hosts currently receive rental disbursements via manual wire transfers, leading to administrative delays, manual reconciliation overhead, and host dissatisfaction regarding volatile foreign exchange conversions.

## 2. Proposed Outcome (The Vision)
Hosts can configure their preferred payout currency (USD or JMD) and Jamaican bank details in the host portal, and receive automated payout disbursements following completed guest stays.

## 3. Scope Boundaries
### In Scope (MVP)
- Host profile settings to select payout currency (USD or JMD) and validate bank account details.
- Automated payout calculation converting booking total into host's selected currency using verified exchange rates.
- Payout transaction history log in Host Admin Dashboard.

### Out of Scope / Non-Goals
- Real-time instant bank disbursements (daily batch settlement is acceptable).
- Support for currencies other than USD and JMD in this iteration.

## 4. Affected Systems & Stakeholders
- **User Personas**: Jamaican Villa Hosts, Platform Finance Admins.
- **System Components**: `user_api` (host profiles), `booking_api` (payout calculation), `db_core` (payout ledger schema), `web_app_admin` (payout management UI).

## 5. Constraints & Non-Negotiable Invariants
- **Financial Precision**: All currency math must strictly use exact Decimal arithmetic; floating-point math is forbidden.
- **Exchange Rates**: Exchange rates must be locked at the time of payout batch generation to prevent calculation drift.
- **Auditability**: Every payout record must immutably link to its source booking reservation ID.

## 6. Success Metrics & Signals
- **Leading Indicator**: Zero manual bank wires required for completed bookings.
- **Lagging Indicator**: Payout reconciliation time reduced from 3 business days to under 2 hours.

## 7. Open Questions & Policy Concerns
- [ ] Which Jamaican banking API or ACH clearing partner will handle JMD automated clearing?
- [ ] How should banking transaction fees be allocated between the platform and host?
```

---

## Example 2: Scale-to-Zero Database Connection Pool Optimization

### Originator Input (Infrastructure Idea)
> *"Our Cloud Run services occasionally throw 500s on cold starts because all 5 instances wake up at the same time and exhaust our PostgreSQL connection pool. We need to handle burst wake-ups smoothly."*

### Generated Intent Artifact (`docs/intent/intent-cloudrun-db-pool-tuning.md`)

```markdown
# Intent: PostgreSQL Connection Pool Resiliency During Cloud Run Cold Starts

## Metadata
- **Author**: Platform Engineering
- **Date**: 2026-09-07
- **Status**: Draft
- **Target Area**: Infrastructure & Database Core

## 1. Problem Statement & Motivation
When traffic spikes trigger simultaneous scale-to-zero wake-ups across Cloud Run container instances, multiple services open maximum connection pools concurrently, exhausting PostgreSQL `max_connections` and causing intermittent 500 errors.

## 2. Proposed Outcome (The Vision)
Cloud Run container instances initialize with bounded, dynamic connection pool sizing and exponential backoff retry policies, ensuring zero connection pool exhaustion errors during traffic surges.

## 3. Scope Boundaries
### In Scope (MVP)
- Tune `sqlx::postgres::PgPoolOptions` across all backend microservices (`max_connections`, `min_connections`, `acquire_timeout`).
- Implement connection retry with jitter on cold start pool initialization.

### Out of Scope / Non-Goals
- Deploying a separate PgBouncer proxy layer (solve via client configuration first).

## 4. Constraints & Non-Negotiable Invariants
- **Cold Start Latency**: Cold start initialization must remain under the 300ms p50 budget.
- **No Swallowed Errors**: Connection acquisition failures must be logged with structured `tracing::error!` spans.

## 5. Open Questions & Policy Concerns
- [ ] What is the optimal `max_connections` allocation per microservice container under 10x concurrency?
```
