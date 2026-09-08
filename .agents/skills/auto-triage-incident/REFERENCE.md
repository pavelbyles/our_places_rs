# Auto-Triage Incident Reference

## 1. Severity Classification Matrix

| Level | Definition | Impact Indicators | Typical SLA |
| :--- | :--- | :--- | :--- |
| **P0 - Critical** | Catastrophic failure, security compromise, data loss/corruption, or revenue/financial fault | Service-wide outage, authentication bypass, data integrity breach, unrecoverable data loss | Immediate hotfix |
| **P1 - Major** | Core feature broken or heavily degraded; significant performance/SLA regression | Elevated 5xx rate on critical endpoints, latency budget breaches (e.g. p95 > threshold), batch worker failures | Same-day triage |
| **P2 - Minor** | Low-frequency edge case, visual/formatting defect, or non-blocking administrative issue | Cosmetic UI issue, non-critical telemetry drop, localized admin error with workaround | Next sprint cycle |

---

## 2. Dynamic Project Invariant Discovery

When evaluating project guardrails during triage, check the following sources dynamically rather than assuming specific technologies:

1. **Workspace Agent Rules**:
   - Inspect `AGENTS.md`, `CLAUDE.md`, or `.agents/rules/` for repository-specific directives (e.g., zero unwrap policies, precision types, database concurrency rules, latency limits).
2. **Architecture & Design Docs**:
   - Check `docs/ARCHITECTURE.md`, `docs/specs/`, or OpenAPI schemas (`openapi.yaml`) to verify intended system behavior.
3. **Workspace Manifests**:
   - Inspect root build definitions (`Cargo.toml`, `package.json`, `go.mod`, `pyproject.toml`) to map workspace packages, crates, and boundaries.

---

## 3. Standard `intent.md` Schema Template

When generating an intent artifact (e.g., in `docs/intent/intent-<id>.md` or `intent/<id>.md`), adhere to this universal schema:

```markdown
# Intent: [Short Incident Title]

## Incident Metadata
- **Incident ID**: INC-[YYYYMMDD]-[SHORT-SLUG]
- **Timestamp**: [ISO 8601 UTC]
- **Severity**: [P0 - Critical | P1 - Major | P2 - Minor]
- **Affected Subsystems**: [List affected packages/services/modules]
- **Origin**: [Alert / Cloud Logging / Metric Breach / Bug Report / Sentry]

## Problem & Symptoms
[Concise description of the anomaly, who or what was impacted, and observable behavior.]

### Evidence & Trace Data
- **Error Code / Status**: [e.g., HTTP 500 / Timeout / Panic / Exception]
- **Relevant Logs / Stack Trace**:
  \`\`\`text
  [Paste sanitized log snippet, stack trace, or error output here]
  \`\`\`
- **Trigger Conditions**: [Specific request payload, concurrency level, edge case input, or environment trigger]

## Root Cause Hypothesis
[Technical hypothesis explaining why the error occurred, referencing suspected functions/files.]

## Proposed Outcome & Invariants
- **Desired Behavior**: [What should happen instead]
- **Architectural Guardrails**:
  - [List relevant constraints discovered from AGENTS.md / architecture docs]
  - [e.g., concurrency safety, memory budgets, precision requirements, backward compatibility]

## Reproduction & Verification Plan
- **Unit/Integration Test Target**: [Module and test scenario to reproduce the bug before fixing]
- **Regression Command**: [Test execution command, e.g., cargo test / npm test / pytest]

## Open Questions & Policy Checks
- [ ] Does this require a database schema migration?
- [ ] Does this alter public API contracts?
- [ ] Are third-party dependencies or external cloud services involved?
```
