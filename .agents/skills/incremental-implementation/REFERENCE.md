# Incremental Implementation Reference Guide

## 1. Slicing Strategy Walkthroughs

### Strategy A: Vertical Slicing (CRUD Example)
Build one end-to-end path through all architectural layers before starting the next operation:

```
┌────────────────────────────────────────────────────────┐
│ Slice 1: Create Item                                   │
│   1. DB schema / migration + model insert query        │
│   2. API route / request handler + validation          │
│   3. Basic UI form / entry point                       │
│   → Verified: User creates item; row stored in DB.     │
├────────────────────────────────────────────────────────┤
│ Slice 2: Read / List Items                             │
│   1. DB query for retrieval with pagination/filtering  │
│   2. API endpoint returning typed collection           │
│   3. UI display / table list view                      │
│   → Verified: User views items created in Slice 1.     │
├────────────────────────────────────────────────────────┤
│ Slice 3: Update Item                                   │
│   1. DB update query + optimistic lock check           │
│   2. API PUT/PATCH endpoint + validation               │
│   3. UI edit modal or form prefill                     │
│   → Verified: User edits item; changes persist.        │
├────────────────────────────────────────────────────────┤
│ Slice 4: Delete Item                                   │
│   1. DB soft-delete or remove query                    │
│   2. API DELETE endpoint + auth check                  │
│   3. UI confirmation dialog + row removal              │
│   → Verified: Full CRUD complete and independently testable.│
└────────────────────────────────────────────────────────┘
```

---

### Strategy B: Contract-First Slicing
Used when frontend and backend develop in parallel, or when integrating external services:

```
Slice 0: Contract Definition
  - Define protobuf, OpenAPI/Swagger schema, or shared type definitions.
  - Establish mock responses matching the exact contract.
  - Verified: Type definitions compile in both client and server crates/packages.

Slice 1a: Server Implementation
  - Implement server route against schema contracts.
  - Write integration tests asserting response structure and error codes.
  - Verified: Server tests pass against contract assertions.

Slice 1b: Client Implementation
  - Build UI / client calls consuming contract mock fixtures.
  - Write client component tests validating presentation states.
  - Verified: UI renders correctly with mocked responses.

Slice 2: End-to-End Integration
  - Swap client target from mocks to live server.
  - Execute end-to-end integration test suite.
  - Verified: Live end-to-end request/response cycle succeeds.
```

---

### Strategy C: Risk-First Slicing
Used when technical feasibility, performance limits, or third-party integrations carry high uncertainty:

```
Slice 1: The Feasibility Spike (Highest Risk)
  - Build minimal prototype isolating the risky mechanism (e.g., raw WebSocket connection,
    hardware device driver, complex concurrency lock).
  - Benchmark or test under boundary conditions.
  - Verified: Risk retired or alternative design selected BEFORE building surrounding features.

Slice 2: Core Domain Logic
  - Build state machines, domain models, and business rules around the validated mechanism.
  - Verified: Unit tests pass for all domain invariants.

Slice 3: Peripherals & Hardening
  - Add retry policies, reconnection logic, logging, and user interfaces.
  - Verified: Production-grade system withstands failure injection.
```

---

## 2. Feature Flag Patterns by Ecosystem

When increments must merge to the main branch before end-to-end functionality is ready for release, isolate WIP code behind feature flags.

### Rust
```rust
// Environment / Config Flag
pub struct AppConfig {
    pub enable_new_payment_flow: bool,
}

// In Handler / Service
if config.enable_new_payment_flow {
    execute_v2_payment(payload).await
} else {
    execute_v1_payment(payload).await
}

// Or Compile-Time Cargo Feature (for experimental crates/modules)
#[cfg(feature = "experimental_search")]
pub mod search_v2;
```

### TypeScript / Node.js
```typescript
// Environment or Runtime Config Flag
const ENABLE_NEW_CHECKOUT = process.env.FEATURE_NEW_CHECKOUT === 'true';

export function handleCheckout(req: Request, res: Response) {
  if (ENABLE_NEW_CHECKOUT) {
    return handleCheckoutV2(req, res);
  }
  return handleCheckoutV1(req, res);
}
```

### Python
```python
import os

FEATURE_ASYNC_PROCESSING = os.getenv("FEATURE_ASYNC_PROCESSING", "false").lower() == "true"

def process_order(order):
    if FEATURE_ASYNC_PROCESSING:
        return enqueue_order_async(order)
    return process_order_sync(order)
```

### Go
```go
type FeatureFlags struct {
    EnableFastPath bool
}

func (s *Service) HandleRequest(ctx context.Context, req Request) Response {
    if s.flags.EnableFastPath {
        return s.handleFastPath(ctx, req)
    }
    return s.handleStandardPath(ctx, req)
}
```

---

## 3. Definition of Done (DoD)

The Definition of Done is the standing quality contract that every increment and feature must satisfy before completion.

### Correctness
- [ ] All specified acceptance criteria are met.
- [ ] Code runs and functions as intended in runtime execution, not just compiling.
- [ ] New functionality is covered by automated tests that fail without the change and pass with it.
- [ ] Full regression test suite passes cleanly.
- [ ] Error paths, invalid inputs, and edge cases are handled gracefully.

### Quality & Hygiene
- [ ] Code intent is self-evident from naming and structure; no comments explaining *what* basic code does.
- [ ] Zero duplicated business logic.
- [ ] Zero dead code, temporary debug statements, or leftover print statements.
- [ ] Strict scope discipline maintained: no unrelated refactoring or adjacent file cleanup included.
- [ ] Formatting and linting checks pass without new suppressions.

### Integration & Safety
- [ ] Database migrations are additive, tested, and backward-compatible.
- [ ] Feature flags protect incomplete or high-risk paths.
- [ ] Interfaces and API contracts remain backward-compatible where required.

### Documentation
- [ ] Public API methods, structs, and configuration flags have descriptive doc comments.
- [ ] Architectural decisions are noted in documentation or ADRs when applicable.

---

## 4. Atomic Commit & Rollback Discipline

### Commit Granularity
Each commit should represent one verifiable step:
- `feat(billing): add tax calculation formula and unit tests`
- `feat(billing): expose tax calculation in checkout handler`
- `feat(billing): display breakdown in checkout UI behind flag`

### Revert Recipe
If an increment introduces unexpected regressions:
1. Identify the atomic commit hash:
   ```bash
   git log --oneline -n 5
   ```
2. Revert the specific commit without affecting neighboring slices:
   ```bash
   git revert <commit-hash> --no-edit
   ```
3. Run verification suite to confirm stability restored:
   ```bash
   cargo test # or stack equivalent
   ```
