---
description: Pre-merge readiness and PR creation — execute sanity checks, security posture review, unwrap/expect check, and generate PR with pr-analyzer.
---

# /ship-pr — Ship Pull Request & Pre-Flight Review

## Goal
Conduct a comprehensive pre-flight audit across tests, code formatting, lints, security posture, and monorepo rules before generating a pull request.

## When to Use
Run `/ship-pr` when feature or bug fix implementation is complete and ready for PR creation and review.

---

## Process

### Step 1: Run Sanity Checks
Execute all checks outlined in [`sanity-check-workflow.md`](file:///home/pav/code/our_places_rs-feat-76-update-agent-skills/.agents/workflows/sanity-check-workflow.md):
```bash
# 1. Code formatting
cargo fmt --check

# 2. Linting
cargo clippy --workspace --exclude protoproj --all-features -- -D warnings

# 3. Offline SQLx metadata sync
cargo sqlx prepare --workspace --check || cargo sqlx prepare --workspace

# 4. Workspace tests
cargo test --workspace --exclude protoproj
```

### Step 2: "Never Do" Compliance Scan
Run ripgrep checks for forbidden patterns in production code:
```bash
# 1. Check for unwrap() or expect() in HTTP endpoints / libraries
rg "\.(unwrap|expect)\(" app_api/ web_app/ common/ db_core/

# 2. Check for float usage in monetary paths
rg "f(32|64)" common/src/pricing.rs app_api/booking_api/
```

### Step 3: Security & Posture Review
Invoke the [`security-reviewer`](.agents/agents/security-reviewer/AGENT.md) agent to evaluate:
- Route authentication extractors (`web::ReqData<Claims>`)
- GCS signed URL expiry and authorization bounds
- Secret isolation (no hardcoded credentials or API keys)

### Step 4: Create PR & Run PR Analysis
Use the [`pr-analyzer`](.agents/skills/pr-analyzer/SKILL.md) skill to create the pull request and run the comprehensive 8-point analysis:
```bash
gh pr create --fill
```
Follow up with [`document-release.md`](.agents/workflows/document-release.md) once merged.