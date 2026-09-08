---
name: failure-scenario-analysis
description: Analyze component failures, dependency outages, error propagation, and blast radiuses across architectures.
---

# Failure Scenario Analysis

Analyze how distributed microservices, databases, and third-party integrations behave under partial or total failure.

## Analysis Workflow

```mermaid
graph LR
    Component[1. Select Component] --> Inject[2. Model Failure Modes]
    Inject --> Trace[3. Trace Error Propagation]
    Trace --> Blast[4. Measure Blast Radius]
    Blast --> Fortify[5. Propose Safeguards]
```

1. **Select Component**: Identify target service, background worker, or external API dependency.
2. **Model Failure Modes**: Simulate crash failures, hang/timeouts, data corruption, and dropped connections.
3. **Trace Error Propagation**: Verify error propagation paths using Rust `Result<T, AppError>` combinators (no unhandled panics).
4. **Measure Blast Radius**: Determine which upstream user workflows and dependent services are impacted.
5. **Propose Safeguards**: Add timeouts, circuit breakers, fallback responses, and alerting triggers.

---

## Failure Catalogs & Blast Radius Templates
* For outage simulation matrices and cascading failure checklists, see **[REFERENCE.md](REFERENCE.md)**.