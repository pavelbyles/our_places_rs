# PR Remediation Examples

## Example 1: Remediating Review Comments & Clippy Lint Failures

### Input PR Context
* **PR Number**: `#84` (`feat-84-split-payments`)
* **Review Comments**:
  1. *"Reviewer @alice: Please avoid `.expect("failed to parse UUID")` here in `checkout.rs:142`, map it to `AppError::InvalidInput` instead."*
  2. *"Reviewer @bob: Did you verify that partial charges roll back if card 2 fails?"*
* **Failing CI Check**:
  `clippy: error: unneeded \`return\` statement in checkout.rs:210`

### Remediation Steps
1. Checkout branch: `git checkout feat-84-split-payments`
2. Apply code fixes:
   - In `checkout.rs:142`, replaced `.expect(...)` with `.map_err(|e| AppError::InvalidInput(e.to_string()))?`.
   - Added unit test in `checkout_tests.rs` asserting atomic rollback when second charge fails.
   - Removed unneeded return in `checkout.rs:210`.
3. Verify locally:
   - `cargo fmt --check`
   - `cargo clippy --workspace --all-features -- -D warnings`
   - `cargo test -p booking_api`
4. Commit & push:
   ```bash
   git commit -am "fix(booking_api): address review feedback on error mapping and rollback test"
   git push origin feat-84-split-payments
   ```
5. Reply to PR thread:
   ```bash
   gh pr comment 84 --body "### 🛠️ PR Remediation Applied\n- Replaced `.expect()` with `AppError::InvalidInput` mapping in `checkout.rs`.\n- Added `test_split_payment_second_card_failure_rollback`.\n- Fixed Clippy lint in `checkout.rs`.\n- All CI checks verified locally."
   ```
