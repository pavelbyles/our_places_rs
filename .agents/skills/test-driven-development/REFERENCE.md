# Test-Driven Development Reference & Patterns

## 1. Cross-Ecosystem Test Patterns

### Rust (Cargo Test)
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_calculate_tax_for_standard_rate() {
        // Arrange
        let amount = rust_decimal::Decimal::new(10000, 2); // 100.00
        let tax_rate = rust_decimal::Decimal::new(15, 2);  // 0.15

        // Act
        let tax = calculate_tax(amount, tax_rate);

        // Assert
        assert_eq!(tax, rust_decimal::Decimal::new(1500, 2)); // 15.00
    }
}
```

### TypeScript / JavaScript (Vitest / Jest)
```typescript
describe('PricingEngine', () => {
  it('should calculate tax for standard rate', () => {
    // Arrange
    const amount = 100.00;
    const taxRate = 0.15;

    // Act
    const tax = calculateTax(amount, taxRate);

    // Assert
    expect(tax).toBe(15.00);
  });
});
```

### Python (Pytest)
```python
def test_should_calculate_tax_for_standard_rate():
    # Arrange
    amount = Decimal('100.00')
    tax_rate = Decimal('0.15')

    # Act
    tax = calculate_tax(amount, tax_rate)

    # Assert
    assert tax == Decimal('15.00')
```

---

## 2. Test Double Selection Guide

Use the lowest-complexity double that provides reliable determinism:

```
┌─────────────────────────────────────────────────────────────┐
│ 1. REAL IMPLEMENTATION                                      │
│    Use whenever fast and side-effect free. Highest trust.   │
├─────────────────────────────────────────────────────────────┤
│ 2. IN-MEMORY FAKE                                           │
│    Fast, working implementation (e.g. HashMap repository).  │
├─────────────────────────────────────────────────────────────┤
│ 3. STATIC STUB                                              │
│    Returns predetermined responses for slow third-parties.  │
├─────────────────────────────────────────────────────────────┤
│ 4. INTERACTION MOCK                                         │
│    Verifies method invocation sequences. Use as last resort.│
└─────────────────────────────────────────────────────────────┘
```

---

## 3. Flaky Test Diagnostic Checklist

When a test passes intermittently:

1. **Shared State Leakage**:
   - Is a test modifying a static singleton, global variable, or shared database table without rolling back?
   - *Fix*: Run tests with isolated transactions or dedicated per-test namespaces.
2. **Timing & Asynchronous Race Windows**:
   - Does the test use arbitrary sleep statements (`sleep(100)`) waiting for background work?
   - *Fix*: Await explicit channels, condition variables, or polling with timeout bounds.
3. **Execution Order Dependency**:
   - Does Test B depend on data created by Test A?
   - *Fix*: Shuffle test execution order to expose implicit sequencing assumptions.
4. **Timezone & Locale Drift**:
   - Does the test assert on `DateTime::now()` or local string formatting?
   - *Fix*: Freeze time using mock clocks or inject fixed UTC timestamps into fixtures.
