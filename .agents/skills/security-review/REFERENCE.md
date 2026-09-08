# Security Review Reference & Checklists

## 1. OWASP Top 10 Evaluation Checklist

| Category | Key Verification Checks |
| :--- | :--- |
| **A01: Broken Access Control** | Verify IDOR checks on entity IDs (e.g. `user_id`, `booking_id`). Ensure JWT claims match target resources. |
| **A02: Cryptographic Failures** | Ensure sensitive data at rest and in transit is encrypted. Verify no plaintext passwords or secrets in code/logs. |
| **A03: Injection** | Ensure all SQL queries use parameterized `sqlx::query!` macros. Validate regex and shell command inputs. |
| **A04: Insecure Design** | Check rate limiting, lockout policies, and availability hold expirations (e.g. 15-minute lock window). |
| **A05: Security Misconfiguration** | Check CORS configurations, Cloud Run scale-to-zero container permissions, and default ports. |
| **A06: Vulnerable Components** | Check `Cargo.lock` dependencies for known vulnerabilities (`cargo audit`). |
| **A07: Identification & Auth** | Verify JWT signature verification, bcrypt hashing work factor, and shadow user promotion. |
| **A08: Software/Data Integrity** | Verify GCS signed URLs have strict time-to-live and restricted HTTP verbs. |
| **A09: Logging & Monitoring** | Ensure no PII in `tracing` events; verify audit logging for all critical state changes. |
| **A10: SSRF** | Validate and restrict URLs before fetching external resources or geocoding APIs. |

---

## 2. Findings Report Template

```markdown
### Security Finding: [Short Title]
- **Severity**: Critical | High | Medium | Low
- **Category**: [e.g. A01: Broken Access Control]
- **Location**: `path/to/file.rs:L123`
- **Impact**: [What an attacker can achieve]
- **Vulnerability Explanation**: [Technical explanation]
- **Remediation**:
```rust
// Proposed code fix
```
```
