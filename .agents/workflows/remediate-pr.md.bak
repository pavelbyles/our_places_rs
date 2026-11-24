---
description: Automated PR remediation loop — sweep unresolved review comments and failing CI checks on an open PR, apply code fixes locally, run sanity checks, and push updates back to the PR branch.
---

# /remediate-pr — Automated PR Remediation Loop

## Goal
Automate the pull request iteration cycle by systematically gathering review comments and failing CI checks, applying code fixes, validating locally, and pushing updates back to the PR branch until all checks are green.

## When to Use
Run `/remediate-pr` when:
- Reviewers have left comments or change requests on an open PR.
- CI pipeline checks (lints, formatting, tests) have failed on a PR branch.
- You want the agent to babysit and resolve feedback on a PR until ready for merge.

---

## Remediation Process

### 1. Identify PR Context & Branch
- Run `gh pr view --json number,headRefName,title` to extract the current PR details.
- Ensure your local workspace is checked out on the PR branch (`git checkout <branch_name>`).
- Pull the latest commits: `git pull origin <branch_name>`.

### 2. Fetch Unresolved Comments & Failing CI Checks
- View status checks and review feedback:
  ```bash
  # Check status of CI runs
  gh pr checks
  
  # Fetch review comments
  gh pr view --comments
  ```

### 3. Diagnose & Apply Fixes
- Address every review item and CI error systematically:
  - **Linter / Formatting**: Fix style and warnings cleanly.
  - **Logical / Error Handling**: Replace any illegal `.unwrap()` / `.expect()` or float usage with proper `Result` propagation and `Decimal` math.
  - **Failing Tests**: Inspect failure logs, fix the root cause in the source code, and ensure tests pass.

### 4. Run Local Sanity Checks
Execute all checks from [`sanity-check-workflow.md`](file:///home/pav/code/our_places_rs-update-agent-md/.agents/workflows/sanity-check-workflow.md):
```bash
# 1. Format check
cargo fmt --check

# 2. Clippy linter
cargo clippy --workspace --exclude protoproj --all-features -- -D warnings

# 3. Unit & integration tests
cargo test --workspace --exclude protoproj
```

### 5. Commit, Push & Post Resolution Summary
- Stage and commit the fixes with a clear message:
  ```bash
  git commit -am "fix(<component>): address review feedback and CI check failures"
  git push origin <branch_name>
  ```
- Post a structured summary comment to the PR:
  ```bash
  gh pr comment <pr_number> --body "### 🛠️ PR Remediation Applied\n- Fixed review feedback items.\n- Verified all sanity checks and tests pass locally."
  ```
