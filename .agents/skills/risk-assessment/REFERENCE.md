# Risk Assessment Reference

## 1. $5 \times 5$ Risk Scoring Matrix

$$\text{Risk Score} = \text{Likelihood (1–5)} \times \text{Impact (1–5)}$$

| Likelihood \ Impact | 1: Negligible | 2: Minor | 3: Moderate | 4: Major | 5: Catastrophic |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **5: Frequent / Trivial** | Medium (5) | High (10) | High (15) | Critical (20) | Critical (25) |
| **4: Probable / Easy** | Low (4) | Medium (8) | High (12) | High (16) | Critical (20) |
| **3: Occasional / Moderate** | Low (3) | Medium (6) | Medium (9) | High (12) | High (15) |
| **2: Remote / Complex** | Low (2) | Low (4) | Medium (6) | Medium (8) | High (10) |
| **1: Improbable / Impractical** | Low (1) | Low (2) | Low (3) | Low (4) | Medium (5) |

---

## 2. Risk Registry Entry Template

```markdown
### Risk: [Risk Title]
* **Category**: Security / Data Integrity / Operational / Financial
* **Likelihood**: [1–5] - [Justification]
* **Impact**: [1–5] - [Justification]
* **Overall Rating**: Critical (17-25) | High (10-16) | Medium (5-9) | Low (1-4)
* **Preconditions**: [What circumstances trigger this risk]
* **Mitigation Strategy**: [Architectural control or invariant]
* **Residual Risk**: [Risk remaining after mitigation]
```
