# Constraint-Driven Development Reference

## 1. Diff-Scoped Floor Guard Implementation

The Floor Guard detects attempts to weaken quality checks or bypass tests in git diffs:

```bash
#!/usr/bin/env bash
# floor-guard.sh — Run diff-scoped check against merge-base
set -euo pipefail

BASE_REF="${1:-origin/main}"
MERGE_BASE="$(git merge-base "$BASE_REF" HEAD 2>/dev/null || echo "$BASE_REF")"

echo "Evaluating Floor Guard against merge-base: $MERGE_BASE"

# 1. Check for newly introduced suppression comments
SUPPRESSIONS_REGEX="#\[allow\(|@ts-ignore|@ts-nocheck|eslint-disable|biome-ignore|# *noqa|# *type: *ignore|nosemgrep|gitleaks:allow"
NEW_SUPPRESSIONS="$(git diff -U0 "$MERGE_BASE" HEAD | grep -E "^\+.*($SUPPRESSIONS_REGEX)" || true)"

if [ -n "$NEW_SUPPRESSIONS" ]; then
    echo "ERROR: New linter/compiler suppression comments detected:"
    echo "$NEW_SUPPRESSIONS"
    exit 1
fi

# 2. Check for newly added test skips
SKIPS_REGEX="#\[ignore\]|\.skip\(|\.todo\(|@pytest\.mark\.skip|t\.Skip\("
NEW_SKIPS="$(git diff -U0 "$MERGE_BASE" HEAD | grep -E "^\+.*($SKIPS_REGEX)" || true)"

if [ -n "$NEW_SKIPS" ]; then
    echo "ERROR: Newly skipped tests detected:"
    echo "$NEW_SKIPS"
    exit 1
fi

# 3. Check for unfinished stubs
STUBS_REGEX="todo!\(|unimplemented!\(|throw new Error\(.*[Nn]ot implemented"
NEW_STUBS="$(git diff -U0 "$MERGE_BASE" HEAD | grep -E "^\+.*($STUBS_REGEX)" || true)"

if [ -n "$NEW_STUBS" ]; then
    echo "ERROR: Unfinished implementation stubs detected:"
    echo "$NEW_STUBS"
    exit 1
fi

echo "Floor Guard: Clean. No evasion patterns detected."
```

---

## 2. Cross-Ecosystem Quality Commands

| Dimension | Rust Ecosystem | TypeScript / Node | Python Ecosystem | Go Ecosystem |
| :--- | :--- | :--- | :--- | :--- |
| **Compiler Check** | `cargo check --workspace --all-targets` | `tsc --noEmit` | `mypy .` | `go vet ./...` |
| **Strict Linter** | `cargo clippy --workspace -- -D warnings` | `eslint . --max-warnings=0` | `ruff check .` | `golangci-lint run` |
| **Code Formatting** | `cargo fmt --check` | `prettier --check .` / `biome format` | `ruff format --check .` | `gofmt -l .` |
| **Unit Test Suite** | `cargo test --workspace` | `vitest run` / `jest` | `pytest` | `go test ./...` |
| **Diff Coverage** | `cargo llvm-cov --summary-only` | `vitest run --coverage` | `pytest --cov` | `go test -coverprofile` |
| **Dependency CVEs** | `cargo audit` | `npm audit --audit-level=high` | `pip-audit` | `govulncheck ./...` |
| **Secret Scanning** | `gitleaks detect --redact` | `gitleaks detect --redact` | `gitleaks detect --redact` | `gitleaks detect --redact` |

---

## 3. Standard `CONSTRAINTS.md` Template

```markdown
# Quality Constraints Contract

Last updated: [YYYY-MM-DD]

## 1. The Floor (Non-Negotiable)
- No new suppression pragmas (`#[allow(...)]`, `@ts-ignore`, `eslint-disable`, `# noqa`)
- No unimplemented stubs (`todo!()`, `unimplemented!()`, empty catch blocks)
- No skipped tests (`#[ignore]`, `.skip`) without linked issue tracking the blocker
- Zero secrets committed to version control
- This document cannot be relaxed to make a failing PR pass

## 2. Enforced Dimensions
| Dimension | Requirement | Tool / Command | Enforcement Stage |
| :--- | :--- | :--- | :--- |
| Compilation | Zero compiler errors | `<command>` | Edit loop |
| Linter | Zero warnings | `<command>` | Edit loop |
| Tests | 100% pass rate | `<command>` | Task completion |
| Coverage | $\ge 80\%$ on changed lines | `<command>` | Task completion & CI |
| Security | Zero high/critical findings | `<command>` | Pre-merge & CI |

## 3. Ratchet Baselines (Directional)
| Metric | Baseline | Rule |
| :--- | :--- | :--- |
| Project Coverage | [XX.X]% | Must not decrease |
| Binary/Bundle Size | [XX.X] MB | Must not increase by >5% |

## 4. Exceptions Log
| ID | Rule | Target | Reason | Owner | Expiration |
| :--- | :--- | :--- | :--- | :--- | :--- |
| EX-01 | `<rule>` | `<path>` | `<reason>` | @<user> | YYYY-MM-DD |
```
