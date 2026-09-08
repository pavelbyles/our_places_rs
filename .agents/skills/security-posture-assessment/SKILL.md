---
name: security-posture-assessment
description: Assess overall security maturity, organizational controls, compliance readiness, and platform hardening.
---

# Security Posture Assessment

Evaluate the security maturity, compliance posture, and architectural controls across systems, services, and cloud infrastructure.

## Assessment Scope
Evaluate organizational and architectural security controls across:
- **Identity & Access Management (IAM)**: Roles, token lifecycles, and least-privilege policies.
- **Data Protection & Encryption**: Key rotation, data classification, and PII protection.
- **Platform & Infrastructure Hardening**: Cloud Run scale-to-zero configurations, network boundaries, and secrets isolation.

---

## 4-Step Assessment Process

1. **Baseline Discovery**: Inspect repo configurations (`.env.example`, Terraform/Pulumi, CI workflows, and crate boundaries).
2. **Control Evaluation**: Score technical controls across IAM, Data, Infrastructure, and Incident Response.
3. **Maturity Rating**: Assign maturity levels (Ad-Hoc, Defined, Managed, Optimized).
4. **Gap Analysis & Roadmap**: Document vulnerabilities, compliance gaps, and prioritized remediation milestones.

---

## Detailed Control Matrices & Scoring Rubrics
* For the CIS/NIST control evaluation checklist, scoring formulas, and assessment templates, see **[REFERENCE.md](REFERENCE.md)**.
