# Security Posture Assessment Reference

## 1. Domain Control Checklist

### 1. Identity & Auth (IAM)
- [ ] Centralized JWT authentication with asymmetric or high-entropy secrets.
- [ ] No hardcoded tokens, passwords, or test credentials in repository history.
- [ ] Role-based access control (RBAC) enforced via compile-time or middleware extractors.

### 2. Cloud Infrastructure & Scale-to-Zero (GCP)
- [ ] Cloud Run service accounts follow least-privilege IAM roles.
- [ ] GCS image bucket blocks all direct public uploads (enforces V4 Signed URLs).
- [ ] Pub/Sub topics require authenticated push subscriptions.

### 3. Data Integrity & Storage
- [ ] PostgreSQL connections enforce TLS (`sslmode=require`).
- [ ] All schema migrations version-controlled and immutable (`db_core/migrations`).
- [ ] Financial calculations isolate Decimal types to prevent floating-point drift.

---

## 2. Maturity Level Rubric

| Level | Definition | Characteristics |
| :--- | :--- | :--- |
| **1: Ad-Hoc** | Reactive, informal | No automated linting, manual secrets distribution, informal code review. |
| **2: Defined** | Standardized, documented | CI-gated tests, automated `cargo audit`, standardized AppError models. |
| **3: Managed** | Quantified, monitored | Structured `tracing`, DORA metrics, Cloud Run latency budgets (<300ms). |
| **4: Optimized** | Continuous, automated | Automated PR remediation, closed-loop incident triage to `intent.md`. |
