---
name: incremental-implementation
aliases:
  - vertical-slice
  - slice-implementation
  - step-by-step-build
description: Delivers changes incrementally in thin, verifiable slices. Use when implementing any feature or change that touches more than one file, or when picking up the next task from a plan. Use when rolling a change out behind a feature flag, when you're about to write a large amount of code at once, or when a task feels too big to land in one step.
version: 2.0.0
---

# Incremental Implementation

## Overview

Build in thin vertical slices — implement one piece, test it, verify it, then expand. Never implement an entire multi-component feature in a single unverified pass. Each increment must leave the system in a compiling, fully testable, and commit-ready state.

This skill governs execution discipline: keeping changes small, verified, rollback-friendly, and strictly within scope.

---

## When to Use

- Implementing any multi-file or multi-layer change (database, domain, API, UI).
- Executing tasks from a task breakdown, specification, or engineering plan.
- Refactoring complex subsystems without breaking existing contracts.
- Any time you are tempted to write more than ~100 lines of code before testing.
- Rolling out work-in-progress behind feature flags.

### When NOT to Use
- Single-line bug fixes or typo corrections.
- Single-file, single-function changes where the scope is already atomic and minimal.

---

## The Increment Cycle

```
┌────────────────────────────────────────────────────────┐
│                                                        │
│   1. Implement ──→ 2. Test ──→ 3. Verify ──→ 4. Commit │
│         ▲                                       │      │
│         │                                       ▼      │
│         └────────── 5. Next Slice ──────────────┘      │
│                                                        │
└────────────────────────────────────────────────────────┘
```

1. **Implement**: Code the smallest complete piece of functionality (one model, one endpoint, or one component).
2. **Test**: Run relevant automated tests (unit/integration) or write new tests under TDD.
3. **Verify**: Confirm build compiles, lints pass, and type checks succeed.
4. **Commit**: Save progress with an atomic, descriptive commit message.
5. **Next Slice**: Proceed to the next increment without carrying uncommitted or broken state.

---

## Slicing Strategies

| Strategy | When to Choose | Structure |
|---|---|---|
| **Vertical Slicing (Preferred)** | Standard user-facing features | One full slice across all layers (DB $\rightarrow$ API $\rightarrow$ UI) per capability |
| **Contract-First Slicing** | Parallel backend/frontend teams or external APIs | Slice 0: Types & Schemas $\rightarrow$ Slice 1: Backend + Mocks $\rightarrow$ Slice 2: Integration |
| **Risk-First Slicing** | High technical uncertainty or external dependencies | Slice 1: High-risk proof-of-concept $\rightarrow$ Slice 2: Core logic $\rightarrow$ Slice 3: Peripherals |

*Detailed examples of each strategy are documented in [REFERENCE.md](file:///home/pav/code/our_places_rs-update-agent-md/.agents/skills/incremental-implementation/REFERENCE.md).*

---

## Non-Negotiable Implementation Rules

### Rule 0: Simplicity First
Before writing code, ask: **"What is the simplest thing that could possibly work?"**

- Prefer standard language primitives over generic frameworks or abstract patterns.
- Three duplicate lines of straightforward code are better than a premature abstraction.
- Build for current requirements, not hypothetical future variations.

```
SIMPLICITY CHECK:
✗ Generic EventBus and abstract event handlers for a single notification
✓ Simple direct function call

✗ Abstract factory pattern for two related components
✓ Two explicit functions sharing a common helper

✗ Dynamic schema-driven form generator for three forms
✓ Three static, type-safe form components
```

### Rule 0.5: Scope Discipline
**Touch only what the immediate task requires.**

- **DO NOT** reformat or "clean up" adjacent code in files you edit.
- **DO NOT** update imports or dependency versions unrelated to the task.
- **DO NOT** delete comments or modify code you do not fully understand.
- **DO NOT** sneak in unrequested features because they "seem useful".

When noting improvements outside scope, record them without modifying code:
```
NOTICED BUT OUT OF SCOPE:
- src/auth/token.rs: Unused import detected (unrelated to current task).
- src/billing/tax.rs: Error message could be more descriptive.
→ Logged for follow-up; not touched in this increment.
```

### Rule 1: One Thing at a Time
Every increment modifies one logical concern. Never combine feature additions, architectural refactors, and dependency upgrades in the same change.

### Rule 2: Keep It Compilable
After every increment, the codebase must compile cleanly and pass existing automated tests. Never leave the branch broken between slices.

### Rule 3: Feature Flags for Incomplete Features
If code must merge before the end-to-end user experience is complete, protect incomplete entry points behind configuration or feature flags. Never expose half-finished workflows to production users.

### Rule 4: Safe Defaults
New features and options must default to conservative, safe behavior (e.g., opt-in flags, disabled by default, strict validation).

### Rule 5: Rollback-Friendly
Every commit must be independently revertable:
- Schema migrations must have forward/backward clarity or non-destructive additive steps.
- Additive changes (new routes, new files) precede deprecation of legacy paths.
- Avoid replacing or deleting an existing implementation in the same commit where a new one is introduced.

---

## Increment Execution Checklist

Execute repository verification commands after each slice:

- [ ] Change accomplishes exactly one task or slice.
- [ ] No out-of-scope files or unrelated formatting changes included.
- [ ] Code compiles without warnings/errors (`cargo check`, `tsc --noEmit`, etc.).
- [ ] Automated tests pass (`cargo test`, `pytest`, `npm test`, etc.).
- [ ] Linter passes cleanly (`cargo clippy`, `eslint`, `flake8`, etc.).
- [ ] Feature flagged if incomplete or user-facing.
- [ ] Committed atomically with a clear, imperative commit message.

---

## Anti-Rationalization Table

| Rationalization | Reality | Enforcement |
|---|---|---|
| *"I'll test everything once at the very end."* | Errors compound geometrically across slices. | Run tests after every single slice. |
| *"It's much faster to write all files together."* | Debugging 500 lines across 10 files takes 5x longer than writing 5 clean slices. | Cap unverified changes at ~100 lines. |
| *"This small refactor is right here, might as well do it."* | Pollutes PR diffs and hides regression culprits. | Note out-of-scope items; do not edit. |
| *"I don't need a feature flag, it's not hooked up to UI."* | Unprotected endpoints or jobs can be triggered accidentally. | Gate incomplete paths with feature flags. |
| *"Let me rerun the test again to be sure."* | Running identical checks on unchanged code produces zero new data. | Run checks only when code changes. |

---

## Verification & Completion Gate

Before marking a feature or milestone complete:
1. Verify each increment cleared the **Definition of Done** in [REFERENCE.md](file:///home/pav/code/our_places_rs-update-agent-md/.agents/skills/incremental-implementation/REFERENCE.md).
2. Ensure the full test suite runs clean across the workspace.
3. Verify zero uncommitted scratch files or temporary debug statements remain.
