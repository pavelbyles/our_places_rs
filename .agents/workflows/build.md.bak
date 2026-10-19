---
description: Implement tasks incrementally — build, test, verify, commit. Add "auto" to execute the entire plan autonomously under a single approval checkpoint.
---

# /build — Incremental & Autonomous Implementation Pipeline

## Overview

Execute implementation tasks from `tasks/plan.md` or approved specifications using strict Test-Driven Development ([test-driven-development](file:///home/pav/code/our_places_rs-update-agent-md/.agents/skills/test-driven-development/SKILL.md)) and vertical slicing ([incremental-implementation](file:///home/pav/code/our_places_rs-update-agent-md/.agents/skills/incremental-implementation/SKILL.md)).

---

## Modes

- **`/build`** *(Default: Single Slice)*: Implements the single next pending task, verifies tests, commits atomically, and halts for review.
- **`/build auto`** *(Autonomous)*: Verifies spec, establishes a clean git baseline, generates/confirms the plan, secures a single human approval checkpoint, and executes all tasks in dependency order without stopping.

---

## Mode 1: Default `/build` (Single Increment)

Pick the next pending task (`- [ ]`) from `tasks/plan.md`:

1. **Review Task**: Read acceptance criteria, target files, and dependencies.
2. **RED (Test First)**: Author an automated test demonstrating the required behavior or reproducing a defect. Verify the test fails for the expected reason.
3. **GREEN (Minimal Code)**: Implement the minimal code required to pass the test. Follow [CONSTRAINTS.md](file:///home/pav/code/our_places_rs-update-agent-md/CONSTRAINTS.md) (no `.unwrap()`, no float money, etc.).
4. **VERIFICATION**:
   - Run the focused test: `cargo test -p <crate> --test <test_name>`
   - Run the workspace test suite: `cargo test --workspace`
   - Run compilation & linting: `cargo clippy --workspace`
5. **ATOMIC COMMIT**:
   - Stage only modified files and the task checklist update in `tasks/plan.md`.
   - Commit with an imperative commit message (e.g. `feat(booking): implement 15-minute hold timeout`).
6. **HALT**: Mark task completed (`- [x]`) and stop.

---

## Mode 2: Autonomous Pipeline (`/build auto`)

Execute an entire feature plan autonomously in a single approved run:

```
┌────────────────────────────────────────────────────────┐
│ 1. Validate Specification (docs/specs/spec-*.md)       │
│                        │                               │
│                        ▼                               │
│ 2. Verify Clean Git Baseline (git status --porcelain)  │
│                        │                               │
│                        ▼                               │
│ 3. Generate / Validate Plan (tasks/plan.md)            │
│                        │                               │
│                        ▼                               │
│ 4. Single Approval Checkpoint (User confirmation)     │
│                        │                               │
│                        ▼                               │
│ 5. Autonomous Execution Loop (RED → GREEN → Commit)   │
│                        │                               │
│                        ▼                               │
│ 6. Definition of Done & Workspace Sanity Gate          │
│                        │                               │
│                        ▼                               │
│ 7. Handoff to /sanity-check-workflow & /ship-pr        │
└────────────────────────────────────────────────────────┘
```

### Step 1: Validate Specification
Look for the feature spec under `docs/specs/spec-*.md` or `SPEC.md`. If no specification exists, **STOP immediately** and instruct the user to run `/create-spec-from-hl-req`. Never invent requirements autonomously.

### Step 2: Verify Clean Git Baseline
Run `git status --porcelain`. If uncommitted changes exist outside planning documents, stop and request the user commit or stash them before beginning. A clean baseline is required for atomic rollback guarantees.

### Step 3: Establish Implementation Plan
If `tasks/plan.md` does not exist, invoke [planning-and-task-breakdown](file:///home/pav/code/our_places_rs-update-agent-md/.agents/skills/planning-and-task-breakdown/SKILL.md) to generate a dependency-ordered list of vertical slices.

### Step 4: Single Human Approval Gate
Present the planned tasks, dependency sequence, and verification commands. Ask the user for explicit approval:
> *"Tasks 1 through N are sequenced. Reply 'proceed' or 'go' to execute the plan autonomously."*

Wait for unambiguous confirmation. Treat hedged responses as non-approval. Once approved, commit `tasks/plan.md` as a preparatory commit.

### Step 5: Autonomous Execution Loop
Iterate through tasks in dependency order:
1. Execute the full RED $\rightarrow$ GREEN $\rightarrow$ VERIFICATION cycle for the task.
2. Stage **only** the files touched by the task plus the updated `tasks/plan.md`.
3. Create an independent commit per task (`git commit -m "feat(...): ..."`).
4. Proceed immediately to the next task.

#### Stop-the-Line Triggers (Pause Autonomous Mode)
Halt execution and alert the user immediately if:
- A test fails or build breaks and cannot be resolved cleanly via [general-debug](file:///home/pav/code/our_places_rs-update-agent-md/.agents/skills/general-debug/SKILL.md).
- Ambiguous or contradictory requirements are discovered in the spec.
- An irreversible or high-risk action is required (e.g. dropping production data tables, modifying live secrets).

### Step 6: Definition of Done Gate
After all tasks complete:
1. Verify compliance with [CONSTRAINTS.md](file:///home/pav/code/our_places_rs-update-agent-md/CONSTRAINTS.md).
2. Execute full workspace checks:
   ```bash
   cargo test --workspace
   cargo clippy --workspace --all-targets -- -D warnings
   ```

### Step 7: Final Summary & Handoff
Present an execution summary table:
- Tasks completed and commit hashes.
- New unit/integration tests added.
- Recommended next command: `/sanity-check-workflow` $\rightarrow$ `/ship-pr`.
