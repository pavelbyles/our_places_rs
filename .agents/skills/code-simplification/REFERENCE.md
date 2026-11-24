# Code Simplification Reference Guide

## 1. Concrete Transformation Patterns Across Stacks

### Pattern 1: Guard Clauses & Early Returns (Flattening Nesting)

#### Rust
```rust
// BEFORE: Deeply nested pattern matching
pub fn process_reservation(req: &ReservationRequest) -> Result<Reservation, AppError> {
    if req.is_valid() {
        if let Some(user) = find_user(&req.user_id)? {
            if user.is_active {
                if let Some(listing) = find_listing(&req.listing_id)? {
                    create_booking(user, listing, req)
                } else {
                    Err(AppError::NotFound("Listing not found".into()))
                }
            } else {
                Err(AppError::Forbidden("User inactive".into()))
            }
        } else {
            Err(AppError::NotFound("User not found".into()))
        }
    } else {
        Err(AppError::Validation("Invalid reservation request".into()))
    }
}

// AFTER: Linear guard clauses with idiomatic Result propagation
pub fn process_reservation(req: &ReservationRequest) -> Result<Reservation, AppError> {
    if !req.is_valid() {
        return Err(AppError::Validation("Invalid reservation request".into()));
    }

    let user = find_user(&req.user_id)?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    if !user.is_active {
        return Err(AppError::Forbidden("User inactive".into()));
    }

    let listing = find_listing(&req.listing_id)?
        .ok_or_else(|| AppError::NotFound("Listing not found".into()))?;

    create_booking(user, listing, req)
}
```

#### TypeScript / JavaScript
```typescript
// BEFORE: Nested pyramids of conditions
function calculateDiscount(user: User, cart: Cart): number {
  let discount = 0;
  if (user != null) {
    if (user.isMember) {
      if (cart.total > 100) {
        if (!cart.hasCoupon) {
          discount = cart.total * 0.15;
        } else {
          discount = cart.total * 0.20;
        }
      } else {
        discount = cart.total * 0.05;
      }
    }
  }
  return discount;
}

// AFTER: Clear guard clauses and early exits
function calculateDiscount(user: User | null, cart: Cart): number {
  if (!user || !user.isMember) return 0;
  if (cart.total <= 100) return cart.total * 0.05;
  return cart.hasCoupon ? cart.total * 0.20 : cart.total * 0.15;
}
```

#### Python
```python
# BEFORE: Cascading indentation
def fetch_report(user, report_id):
    if user is not None:
        if user.is_authenticated():
            if user.has_permission("reports:read"):
                report = db.get_report(report_id)
                if report is not None:
                    return format_report(report)
                else:
                    raise NotFoundError("Report not found")
            else:
                raise PermissionDeniedError()
        else:
            raise AuthenticationError()
    else:
        raise ValueError("User must not be None")

# AFTER: Guard clauses with early exits
def fetch_report(user, report_id):
    if user is None:
        raise ValueError("User must not be None")
    if not user.is_authenticated():
        raise AuthenticationError()
    if not user.has_permission("reports:read"):
        raise PermissionDeniedError()

    report = db.get_report(report_id)
    if report is None:
        raise NotFoundError("Report not found")

    return format_report(report)
```

---

### Pattern 2: Lookup Maps vs. Dense Conditionals

#### TypeScript
```typescript
// BEFORE: Nested ternary chain
const badgeColor = status === 'active' 
  ? 'green' 
  : status === 'pending' 
  ? 'yellow' 
  : status === 'suspended' 
  ? 'red' 
  : 'gray';

// AFTER: Explicit dictionary lookup with fallback
const STATUS_COLORS: Record<Status, string> = {
  active: 'green',
  pending: 'yellow',
  suspended: 'red',
  archived: 'gray',
};
const badgeColor = STATUS_COLORS[status] ?? 'gray';
```

#### Rust
```rust
// BEFORE: Repetitive if/else ladder
pub fn status_code_to_str(status: &BookingStatus) -> &'static str {
    if *status == BookingStatus::Pending {
        "pending"
    } else if *status == BookingStatus::Confirmed {
        "confirmed"
    } else if *status == BookingStatus::Cancelled {
        "cancelled"
    } else {
        "unknown"
    }
}

// AFTER: Exhaustive pattern match
pub fn status_code_to_str(status: &BookingStatus) -> &'static str {
    match status {
        BookingStatus::Pending => "pending",
        BookingStatus::Confirmed => "confirmed",
        BookingStatus::Cancelled => "cancelled",
        BookingStatus::Unknown => "unknown",
    }
}
```

---

### Pattern 3: Declarative Transformations vs. Imperative Accumulators

#### Rust
```rust
// BEFORE: Manual vector allocation and mutable loop
let mut active_user_ids = Vec::new();
for user in &users {
    if user.is_active && user.email_verified {
        active_user_ids.push(user.id);
    }
}

// AFTER: Clear iterator pipeline
let active_user_ids: Vec<_> = users
    .iter()
    .filter(|u| u.is_active && u.email_verified)
    .map(|u| u.id)
    .collect();
```

#### Python
```python
# BEFORE: Imperative dictionary population
user_map = {}
for user in users:
    if user.is_active:
        user_map[user.id] = user.email

# AFTER: Dictionary comprehension
user_map = {user.id: user.email for user in users if user.is_active}
```

---

## 2. Anti-Refactoring Traps to Avoid

| Anti-Pattern | Description | Remediation |
|---|---|---|
| **The Micro-Function Maze** | Breaking a simple 20-line linear sequence into five 4-line helper functions scattered across a file. | Keep code together if it is only called in one place and reads linearly top-to-bottom. |
| **Error Flattening** | Replacing distinct errors (`NotFound`, `Unauthorized`, `InvalidInput`) with a single generic `Err("failed")` to "clean up" the signature. | Never destroy domain error fidelity or HTTP response accuracy. |
| **Premature Generalization** | Adding generic type parameters `T`, trait bounds, or plugin hooks when only one concrete type exists. | Keep concrete types until a second or third variation genuinely requires abstraction. |
| **Clever One-Liners** | Compressing 6 readable lines of arithmetic and validation into an obscure single-line bitwise or regex trick. | Favor readable multi-step code over clever compaction. |

---

## 3. Safe Refactoring Workflow (The Prove-and-Prune Loop)

1. **Verify Test Harness**: Ensure test suite covers existing behavior.
2. **Apply Single Transform**: Apply one pattern (e.g., flatten guard clauses).
3. **Execute Test Suite**: Run `cargo test` / `npm test` / `pytest`.
4. **Inspect Diff**: Review `git diff` to ensure zero unintended changes or formatting creep.
5. **Commit Atomically**: Record the refactoring separately from feature commits (`refactor: flatten reservation validation with guard clauses`).
