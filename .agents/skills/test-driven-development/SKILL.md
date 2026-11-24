---
name: test-driven-development
aliases:
  - tdd
  - red-green-refactor
  - prove-it
description: Drives development with tests using the red-green-refactor loop. Use when implementing any logic, fixing any bug, or changing any behavior. Use when you need to prove that code works, when a bug report arrives, or when you're about to modify existing functionality.
version: 2.0.0
---

# Test-Driven Development (TDD)

## Overview

Write a failing test before writing the code that makes it pass. For bug fixes, reproduce the bug with a test before attempting a fix. Tests are proof — "seems right" is not done.

A codebase with comprehensive automated tests allows fearless refactoring and reliable agent autonomy. A codebase without tests is technical liability where every edit risks silent regression.

---

## When to Use

- Implementing new domain logic, calculation rules, or state machines.
- Fixing any bug or runtime fault (The Prove-It Pattern).
- Modifying existing functionality or refactoring public interfaces.
- Handling boundary conditions and edge cases discovered during design.
- Any code change where behavioral regression is possible.

### When NOT to Use
- Pure documentation, comment updates, or asset modifications without behavioral changes.
- Throwaway spikes or temporary scripts explicitly labeled as exploratory.

---

## The TDD Cycle

```
    RED                GREEN              REFACTOR
 Write a test    Write minimal code    Clean up the
 that fails  ──→  to make it pass  ──→  implementation  ──→  (repeat)
      │                  │                    │
      ▼                  ▼                    ▼
   Test FAILS        Test PASSES         Tests still PASS
```

### 1. RED — Write a Failing Test
Write the test first. It must fail for the right reason (assertion failure or missing symbol, not syntax error). A test that passes before code is written proves nothing.

### 2. GREEN — Make It Pass
Write the minimal implementation required to make the test pass. Avoid premature optimization or implementing unrequested secondary features.

### 3. REFACTOR — Clean Up Under Green
With tests passing, clean up the implementation without changing behavior:
- Eliminate duplication, improve variable and function naming.
- Extract domain models, simplify combinators, and enforce invariants.
- Run tests after every refactoring step to verify zero behavioral divergence.

---

## The Prove-It Pattern (Bug Fixes)

When a bug report or runtime panic occurs, **never start by guessing or modifying the implementation**:

```
Bug Report / Incident
         │
         ▼
Write an automated reproduction test
         │
         ▼
Test FAILS (confirms bug exists & isolates the fault)
         │
         ▼
Implement the minimal root-cause fix
         │
         ▼
Test PASSES (proves the fix works)
         │
         ▼
Run full test suite (guarantees zero regressions)
```

If you cannot write a test that fails, you do not yet understand the bug. Return to `general-debug` to reproduce it first.

---

## The Test Pyramid & Sizing

Invest testing effort according to the pyramid. Most tests must be small, fast, and isolated:

```
          ╱╲
         ╱  ╲         E2E Tests (~5%)
        ╱    ╲        Critical user journeys, live browser/network
       ╱──────╲
      ╱        ╲      Integration Tests (~15%)
     ╱          ╲     Component boundaries, DB transactions, API contracts
    ╱────────────╲
   ╱              ╲   Unit Tests (~80%)
  ╱                ╲  Pure functions, domain math, isolated state, milliseconds
 ╱──────────────────╲
```

### The Beyonce Rule
> *"If you liked it, you should have put a test on it."*

Infrastructure migrations, language edition upgrades, dependency bumps, and agent refactors are not responsible for catching your bugs — your tests are. If an unverified behavior breaks during a refactor, the fault lies in the missing test.

### Resource Sizing Model

| Size | Constraints | Latency Target | Examples |
| :--- | :--- | :--- | :--- |
| **Small (Unit)** | Single process, zero disk I/O, zero network, zero external DB | Milliseconds | Domain logic, pricing math, serialization, validators |
| **Medium (Integration)** | Localhost only, test database transactions, mock external services | Seconds | API route handlers, database repositories, middleware extractors |
| **Large (E2E)** | Full system assembly, live browsers, external integrations | Minutes | End-to-end checkout flows, browser automation suites |

---

## Writing High-Confidence Tests

### 1. Test State and Outcomes, Not Internal Interactions
Assert on the **state result or returned output** of an operation, not on internal private methods called along the way.
- **Good (State-based)**: Verifies that after `complete_order()`, status is `Confirmed` and balance is updated.
- **Bad (Interaction-based)**: Verifies that `complete_order()` invoked `db.execute()` with an exact internal string.

### 2. DAMP Over DRY in Tests
While production code prioritizes DRY (Don't Repeat Yourself), test code favors **DAMP (Descriptive And Meaningful Phrases)**:
- Tests must be readable top-to-bottom as a self-contained behavioral specification.
- Tolerating modest setup repetition is better than creating dense, brittle test-helper abstractions that obscure test intent.

### 3. Prefer Real Implementations Over Mocks
Use the simplest test double that provides determinism:
```
Real Implementation  ──→ Highest confidence, catches true integration bugs
Fake (In-Memory)     ──→ High confidence, fast (e.g., in-memory repository)
Stub                 ──→ Returns canned data for slow third-party services
Mock                 ──→ Verifies call sequences; use sparingly for side-effects
```
Over-mocking creates test suites that pass cleanly while production fails.

### 4. Arrange-Act-Assert (AAA)
Structure every test into three distinct blocks:
1. **Arrange**: Set up inputs, fixtures, and preconditions.
2. **Act**: Execute the single function or operation being tested.
3. **Assert**: Verify the expected outcome and invariants.

### 5. Name Tests Descriptively
Name tests as behavioral specifications:
- `should_reject_reservation_when_requested_dates_overlap_existing_hold`
- `should_apply_statutory_tax_rate_to_booking_subtotal`
- Avoid meaningless names like `test_1`, `test_booking`, or `works`.

---

## Anti-Rationalization Table

| Rationalization | Reality |
| :--- | :--- |
| *"I'll write the tests after the code works."* | Tests written after implementation anchor to what the code *does*, not what it *should do*. Write RED first. |
| *"This change is too trivial to need a test."* | Trivial code breaks when edge cases appear. If the behavior matters, document it with a test. |
| *"Tests slow down my implementation velocity."* | Tests slow down the first 10 minutes; they save days of debugging and regression hunting later. |
| *"I verified it manually in the UI/terminal."* | Manual testing evaporates the second your session ends. Automated tests persist forever. |
| *"Let me re-run the tests again without changes just to be sure."* | Re-running tests against unchanged code adds zero information. Run tests only when code or fixtures change. |

---

## Verification Checklist

- [ ] Repository test runner and conventions detected before running tests
- [ ] Failing test written and executed first (confirmed RED)
- [ ] Minimal code implemented to pass the test (confirmed GREEN)
- [ ] Code refactored without altering test behavior
- [ ] Bug fixes include a reproduction test that failed before the fix
- [ ] Tests assert on outcomes/state rather than private implementation details
- [ ] Tests follow Arrange-Act-Assert (AAA) structure with descriptive names
- [ ] Full test suite passes clean before committing
