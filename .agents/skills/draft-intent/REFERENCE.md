# Draft Intent (Proto-Spec) Reference

## 1. Universal `intent.md` Schema Template

When drafting an intent proto-spec in `docs/intent/intent-<slug>.md` or `intent/<slug>.md`, use this standardized schema:

```markdown
# Intent: [Short Feature or Problem Title]

## Metadata
- **Author**: [Originator Name / Role]
- **Date**: [YYYY-MM-DD]
- **Status**: [Draft | Under Review | Approved]
- **Target Area**: [Frontend / Backend / Infrastructure / Data]

## 1. Problem Statement & Motivation
[Describe the pain point or opportunity from the user's or business's perspective. Avoid technical jargon when describing user pain. Why solve this now?]

## 2. Proposed Outcome (The Vision)
[Describe the desired end state. What capability becomes available once this is implemented?]

## 3. Scope Boundaries
### In Scope (MVP)
- [Explicit capability 1]
- [Explicit capability 2]

### Out of Scope / Non-Goals
- [Explicitly excluded feature or deferred enhancement]

## 4. Affected Systems & Stakeholders
- **User Personas**: [e.g., Guests, Hosts, Administrators]
- **System Components**: [e.g., API services, Database schemas, UI pages, Event queues]

## 5. Constraints & Non-Negotiable Invariants
- **Domain Invariants**: [e.g., zero float currency math, atomic transaction boundaries]
- **Performance Budgets**: [e.g., p95 latency < 300ms, scale-to-zero memory < 256MB]
- **Security & Compliance**: [e.g., role-based auth, PII protection, audit logging]

## 6. Success Metrics & Signals
- **Leading Indicator**: [e.g., 100% of checkout holds completed without lock contention]
- **Lagging Indicator**: [e.g., 20% increase in converted bookings]

## 7. Open Questions & Policy Concerns
- [ ] [Unresolved policy question to be addressed during Stage 2 Design]
- [ ] [Architecture or vendor choice requiring technical lead decision]
```

---

## 2. Socratic Originator Interview Guide

When interviewing non-technical or technical originators, use these prompts to extract implicit assumptions:

1. **Clarifying the "Why"**:
   - *"What specific scenario happened recently that made this urgent?"*
   - *"If we do nothing, what breaks or who is blocked?"*

2. **Scoping the MVP**:
   - *"What is the absolute minimum version that solves 80% of this pain?"*
   - *"What related features should we explicitly say NO to in this first pass?"*

3. **Discovering Constraints**:
   - *"Are there statutory rules, compliance mandates, or third-party service limits we must obey?"*
   - *"Does this touch existing user data or requiring migrating existing states?"*

---

## 3. Cross-LLM Compatibility Principles

To ensure any LLM (Gemini, Claude, GPT, DeepSeek, Llama) can reliably read, evaluate, and generate `intent.md` artifacts:

* **Declarative Markdown**: Use clean headings, bullet lists, and standard task checkboxes (`- [ ]`). Avoid proprietary XML or platform-specific macro tags.
* **Deterministic Field Names**: Retain exact section headings (`Problem Statement`, `Proposed Outcome`, `Constraints`, `Open Questions`) so downstream agents can parse sections deterministically.
* **Self-Contained Context**: Ensure the proto-spec contains enough domain context that a downstream agent can proceed without access to prior conversation transcripts.
