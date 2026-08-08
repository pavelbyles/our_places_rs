---
description: Extracts requirements from a GitHub issue, cross-references with ARCHITECTURE.md, conducts security and performance reviews, and generates a detailed technical specification and test plan using the grill-me skill.
---

# /create-spec-from-hl-req — Create spec from HL reqs

# Goal
Convert a high-level GitHub issue into a highly detailed, architecture-compliant, secure, and performant technical specification with a comprehensive test plan.

# Instructions
1. Use your tools to fetch the contents of the provided GitHub issue.
2. Invoke the `grill-me` skill against the issue content to generate a comprehensive draft technical specification.
3. Read the `ARCHITECTURE.md` file in the root of the workspace.
4. **Performance & Scalability Check:** Review the draft specification specifically for Big-O inefficiencies, N+1 query problems, and memory bottlenecks. Ensure the proposed design strictly meets the system constraints and scaling requirements outlined in `ARCHITECTURE.md`. Revise the draft to resolve any identified bottlenecks.
5. **Threat Modeling & Security Review:** Act as a security engineer and analyze the revised draft specification. Look for injection vulnerabilities, broken access control, data exposure risks, and other OWASP Top 10 vulnerabilities specific to the proposed changes. Document required security mitigations and integrate them into the specification.
6. Generate a comprehensive test plan (unit, integration, and E2E) based on the finalized, secure, and performant specification. Ensure the test plan includes coverage for the identified security edge cases and performance benchmarks.
7. Write the final document to `docs/specs/spec-<ISSUE_NUMBER>-<ISSUE DESCRIPTION>.md`.

# Constraints
- Ensure the output strictly adheres to the architectural guidelines in `ARCHITECTURE.md`.
- The final document must be highly structured and contain distinct sections for "Technical Specification", "Performance Considerations", "Security Mitigations", and the "Test Plan".