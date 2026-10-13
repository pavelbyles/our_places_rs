---
name: planning-and-task-breakdown
aliases:
  - task-breakdown
  - plan-tasks
  - decompose-work
description: Breaks specifications and features into ordered, dependency-mapped vertical tasks. Use when you have a spec or clear requirements and need an executable implementation plan. Ensures tasks are small (XS, S, M), testable, and have explicit verification gates.
version: 2.0.0
---

# Planning and Task Breakdown

## Overview

Decompose approved specifications and high-level requirements into small, verifiable, dependency-ordered tasks with explicit acceptance criteria.

Good task breakdown is the foundation of reliable autonomous execution: every task must be small enough to implement, test, and verify within a single focused session without touching unrelated systems.

---

## When to Use

- You have an approved specification (`docs/specs/spec-*.md` or `SPEC.md`) and need an actionable implementation plan.
- A feature feels too large or complex to begin coding directly.
- Work needs to be sequenced in dependency order across domain layers.
- Invoked by `/build auto` to establish execution order.

### When NOT to Use
- Single-line bug fixes or typo corrections.
- Single-file changes where scope is already atomic and self-contained.

---

## The Planning Process

```
┌────────────────────────────────────────────────────────┐
│ 1. Enter Plan Mode (Read-only codebase exploration)    │
│                        │                               │
│                        ▼                               │
│ 2. Map Dependency Graph (DAG)                          │
│                        │                               │
│                        ▼                               │
│ 3. Slice Vertically (End-to-end paths, not horizontal) │
│                        │                               │
│                        ▼                               │
│ 4. Specify Tasks (Acceptance criteria & test commands) │
│                        │                               │
│                        ▼                               │
│ 5. Write to `tasks/plan.md` (Never overwrite in-flight)│
└────────────────────────────────────────────────────────┘
```

### Step 1: Read-Only Plan Mode
Operate in read-only mode — **never write implementation code during planning**:
- Read the specification, relevant domain types, database schemas, and API contracts.
- Identify existing codebase conventions and constraints (`CONSTRAINTS.md`).
- Map out external dependencies, risks, and unknowns.

### Step 2: Map the Dependency Graph (DAG)
Identify foundational components before dependent features:

```
Database Schema / Entity
    │
    ├── Domain Models & Pricing Math
    │       │
    │       ├── API Route Handler & Validation
    │       │       │
    │       │       └── UI Components / SSR Templates
    │       │
    │       └── Event Publishing / Background Worker
```

### Step 3: Vertical Slicing
Decompose features into thin vertical slices delivering end-to-end capability, rather than horizontal layer-by-layer bulk builds:
- **Avoid Horizontal Slices**: *"Task 1: Build all DB tables; Task 2: Build all 10 API endpoints; Task 3: Build all UI pages."*
- **Enforce Vertical Slices**: *"Task 1: Create reservation hold (DB + Handler + Validation); Task 2: Query availability; Task 3: Confirm payment."*

---

## Task Sizing Guidelines

| Size | Files Touched | Scope | Example |
|:---:|:---:|---|---|
| **XS** | 1 | Single utility function, schema field, or validation rule | Add tax rate validation check |
| **S** | 1–2 | Single domain entity, calculation, or API handler | Implement pricing calculation function |
| **M** | 3–5 | Complete vertical feature slice | Implement hold expiration worker and status transition |
| **L** | 5–8 | Multi-component subsystem | **Break down into S and M tasks before proceeding** |
| **XL** | 8+ | Architectural change | **Must be split into distinct phases** |

### Split Triggers
Break a task down if:
1. You find yourself writing the word "and" in the task title.
2. It touches more than 5 distinct files across different crate/package boundaries.
3. It cannot be verified with a single focused automated test suite.

---

## Output Standard: `tasks/plan.md`

Save the execution plan in `tasks/plan.md`. Never overwrite a plan if it already contains in-flight or unchecked tasks without explicit user instruction.

### Task Format Template

```markdown
# Implementation Plan: [Feature Name]

**Spec Reference:** `docs/specs/spec-[name].md`
**Baseline Commit:** `[commit-hash]`

## Tasks

- [ ] **Task 1: [Short Descriptive Title]**
  - **Scope:** [XS / S / M]
  - **Description:** Clear summary of the exact change.
  - **Files Touched:**
    - `path/to/file.rs`
    - `tests/path/to/test.rs`
  - **Acceptance Criteria:**
    - [ ] Condition 1
    - [ ] Condition 2
  - **Verification Command:** `cargo test -p crate_name --test test_name`
  - **Dependencies:** None

- [ ] **Task 2: [Next Vertical Slice]**
  - **Scope:** [S / M]
  - **Dependencies:** Task 1
  ...
```

---

## Verification & Handoff

Before handing off the plan to `/build` or `/build auto`:
- [ ] Every task has explicit test verification commands.
- [ ] Dependencies between tasks form an acyclic graph (no circular blocks).
- [ ] Foundation tasks are sequenced before integration tasks.
- [ ] High-risk or uncertain technical pieces are scheduled early (Risk-First).
