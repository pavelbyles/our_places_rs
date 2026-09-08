# PR Remediation Reference

## 1. GitHub CLI & API Cheat Sheet

### Fetching PR Details
```bash
# Get PR status, branch name, and CI check statuses
gh pr view <pr_number> --json number,title,headRefName,state,statusCheckRollup,reviews

# Fetch inline diff comments
gh api repos/{owner}/{repo}/pulls/<pr_number>/comments --jq '.[] | {id: .id, path: .path, line: .line, body: .body, user: .user.login}'

# Fetch top-level PR issue comments
gh pr view <pr_number> --comments
```

### Posting Comments & Resolving Threads
```bash
# Reply to an inline review comment thread
gh api repos/{owner}/{repo}/pulls/<pr_number>/comments/<comment_id>/replies -f body="Fixed in commit <sha>: updated error propagation."

# Post a top-level remediation summary on the PR
gh pr comment <pr_number> --body "### PR Remediation Applied\n- Fixed lock statement timeout.\n- Addressed clippy warning.\n- Verified with sanity checks."
```

---

## 2. Review Finding Triage Matrix

| Feedback Type | Action Required | Verification Step |
| :--- | :--- | :--- |
| **Failing CI Test / Lint** | Reproduce failure locally, apply fix | Run specific test or linter target |
| **Logic / Edge-Case Bug** | Write/update unit test, implement fix | Run unit test suite |
| **Forbidden Pattern (e.g. `.unwrap()`)** | Refactor into `Result`/`Option` combinators | Scan with `rg "\.unwrap\("` |
| **Out-of-Scope Feature Request** | Explain scope constraint against `intent.md` / `spec.md` | Post polite explanation, do not write code |

---

## 3. Structured PR Resolution Summary Template

When posting an update to the PR thread after applying fixes, format using this schema:

```markdown
### 🛠️ Automated PR Remediation Summary

The following changes have been applied to address review feedback and CI checks:

- **[Component / File]**: [Description of fix, e.g. "Replaced `.unwrap()` with `AppError` propagation in `checkout.rs`"].
- **[Component / File]**: [Description of fix, e.g. "Added unit test for partial charge error rollback"].

#### Verification Results
- [x] Code formatted (`cargo fmt` / linter)
- [x] Static analysis passed with 0 warnings (`cargo clippy`)
- [x] All unit and integration tests passing (`cargo test`)
```
