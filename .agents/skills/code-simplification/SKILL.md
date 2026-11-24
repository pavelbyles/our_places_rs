---
name: code-simplification
aliases:
  - simplify-code
  - reduce-complexity
  - refactor-clarity
  - clean-code
description: Simplifies code for clarity and maintainability without changing behavior. Use when refactoring code after tests pass, reducing cognitive load or cyclomatic complexity, flattening deeply nested control flow, or removing dead code and over-engineered abstractions.
version: 2.0.0
---

# Code Simplification

## Overview

Simplify code by reducing complexity while preserving exact behavior. The primary goal is not fewer lines of code — it is code that is faster to read, understand, modify, and debug. 

Every simplification must satisfy the core question: **"Would a new engineer understand and modify this faster and more safely than the original?"**

---

## When to Use

- After a feature is working and tests pass, but the implementation feels heavier or more tangled than necessary.
- During code review when readability, cyclomatic complexity, or cognitive load issues are flagged.
- When encountering deeply nested logic (3+ levels), long functions (>50 lines), or cryptic identifiers.
- When consolidating duplicated logic or pruning confirmed dead code.
- After merging changes that introduced inconsistencies with existing project conventions.

### When NOT to Use
- Code is already clean and readable — never simplify merely for the sake of churn.
- You do not yet understand what the code does — comprehend before you modify (Chesterton's Fence).
- The code is performance-critical and the "simpler" expression would be measurably slower.
- You are about to rewrite the module entirely.

---

## The Five Principles of Simplification

### 1. Preserve Behavior Exactly
Do not change what the code does — only how it expresses it. All inputs, outputs, side effects, error states, and edge cases must remain identical.

```
BEHAVIORAL INVARIANCE CHECKLIST:
→ Does this produce the identical output for every valid and invalid input?
→ Does this maintain exact error types, error codes, and messages?
→ Does this preserve identical side-effect ordering (I/O, database queries, events)?
→ Do all existing automated tests pass without modification?
```

### 2. Follow Project Conventions & Contracts
Simplification must align code with existing codebase conventions and project constraints (such as `CONSTRAINTS.md` or `AGENTS.md`), not impose external personal preferences.
- Check project style, error handling patterns, and architecture rules.
- Respect project-specific conventions (e.g. monetary precision, panic prohibitions).

### 3. Prefer Clarity Over Cleverness
Explicit code that is instantly understood is superior to clever, dense expressions that require mental decoding.

```
CLARITY CHECK:
✗ Deeply nested ternary operators or compound boolean expressions
✓ Straightforward if/else chains, guard clauses, or lookup tables

✗ Complex functional reducer with inline mutations and spread operators
✓ Clear loop or explicit combinator pipeline with named intermediate steps
```

### 4. Maintain Balance (Avoid Over-Simplification)
Beware of these common over-simplification traps:
- **Inlining too aggressively**: Removing a helper function that gave an important domain concept a clear name makes call sites harder to read.
- **Combining unrelated logic**: Merging two distinct functions into one parameterized function increases coupling.
- **Stripping necessary abstractions**: Some interfaces exist for testability, isolation, or polymorphism.
- **Optimizing for line count**: Fewer lines is never the primary goal; lower cognitive load is.

### 5. Scope to What Changed
Default to simplifying recently modified code or specifically scoped modules. Avoid drive-by refactoring of unrelated files to keep PR diffs clean and prevent collateral regressions.

---

## The Simplification Process

```
┌────────────────────────────────────────────────────────┐
│ 1. Understand (Chesterton's Fence & Tests)             │
│                       │                                │
│                       ▼                                │
│ 2. Diagnose (Structural, Naming, Redundancy signals)   │
│                       │                                │
│                       ▼                                │
│ 3. Apply Incrementally (One change → Run tests)        │
│                       │                                │
│                       ▼                                │
│ 4. Verify (Diff review, test suite, style compliance)  │
└────────────────────────────────────────────────────────┘
```

### Step 1: Understand Before Touching (Chesterton's Fence)
Before modifying or removing any logic, establish why it exists:
- What is this code's exact responsibility?
- What are all its callers, and what dependencies does it call?
- What edge cases, error conditions, or platform idiosyncrasies does it accommodate?
- Review git history (`git log -p -S <symbol>`) to understand past context.

### Step 2: Identify Simplification Opportunities

#### A. Structural Complexity
| Pattern | Signal | Simplification |
|---|---|---|
| Deep Nesting (3+ levels) | Hard to trace control flow | Extract guard clauses or return early |
| Long Functions (>50 lines) | Multiple mixed responsibilities | Split into cohesive, well-named helper functions |
| Nested Ternaries / Switch ladders | Mental stack overflow | Replace with early exits, lookup maps, or pattern matching |
| Flag Arguments (`func(true, false)`) | Unclear call site intent | Replace with typed options structs or distinct functions |

#### B. Naming & Readability
| Pattern | Signal | Simplification |
|---|---|---|
| Generic Names (`data`, `temp`, `val`, `res`) | Obscures domain meaning | Rename to reveal content (`bookingRequest`, `validatedEmail`) |
| Cryptic Abbreviations (`usr`, `cfg`, `btn`) | Stumbles during reading | Expand to full words (`user`, `config`, `button`) unless universally standard (`id`, `url`) |
| "What" Comments | `// increment count` above `count += 1` | Delete redundant comments |
| "Why" Comments | `// Lock required to prevent concurrent race condition` | Preserve comments that explain business rationale or invariants |

#### C. Redundancy & Dead Code
| Pattern | Signal | Simplification |
|---|---|---|
| Duplicated Logic | Identical code repeated in multiple places | Extract to a shared utility or helper function |
| Dead Code | Unreachable branches, unused parameters, commented code | Safely delete after verifying zero references |
| Speculative Abstractions | Interfaces or wrappers with only one implementation | Inline wrapper; call target directly |

### Step 3: Apply Changes Incrementally
- Make one atomic simplification at a time.
- Run the test suite after every edit.
- If tests fail, revert immediately and re-evaluate.
- **The Rule of 500**: If a refactoring touches more than 500 lines, use automated transforms (AST codemods or scripts) rather than error-prone manual editing.

### Step 4: Verify the Result
- Run all unit, integration, and lint checks.
- Review the diff: Is it cleaner, easier to review, and strictly behavior-preserving?

*For language-specific before/after examples and detailed transformation patterns, see [REFERENCE.md](file:///home/pav/code/our_places_rs-update-agent-md/.agents/skills/code-simplification/REFERENCE.md).*

---

## Anti-Rationalization Table

| Rationalization | Reality | Enforcement |
|---|---|---|
| *"It works, so don't touch it."* | Fragile, convoluted code degrades over time and causes costly future bugs. Simplify while context is fresh. |
| *"Fewer lines is always cleaner."* | A dense, single-line chained expression is harder to maintain than 5 clean, explicit lines. |
| *"I'll clean up unrelated files while I'm in the neighborhood."* | Drive-by edits pollute diffs, inflate PR review burden, and cause unexpected regressions. Keep scope tight. |
| *"The types make comments and naming unnecessary."* | Types declare shape; naming and structure explain intent and domain rules. |
| *"I'll need this abstraction later."* | Speculative generality is technical debt. Build for today's requirements; refactor when new requirements arrive. |
| *"I can refactor and add this feature at the same time."* | Always split refactoring and feature additions into separate commits or PRs. |

---

## Simplification Checklist

- [ ] All existing tests pass without modification.
- [ ] No behavioral change, side-effect alteration, or error condition weakened.
- [ ] Cyclomatic complexity reduced (guard clauses applied, nesting flattened).
- [ ] Function and variable names reflect explicit domain intent.
- [ ] No dead code, obsolete comments, or temporary debug output left behind.
- [ ] Project-specific constraints and style guidelines respected.
- [ ] Changes committed atomically and cleanly.
