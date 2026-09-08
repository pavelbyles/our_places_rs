---
description: Sanity check before running CI/CD or pushing branches
---

Actions to execute whenever significant code changes are done, before pushing or opening a PR to ensure GitHub Actions CI/CD passes cleanly.

> [!IMPORTANT]
> **CRITICAL RULE:** If any step below fails, apply a fix and **re-run that specific command** until it completes with 0 errors before moving to the next step. Do not assume your fix worked.

### 1. Ensure Frontend Dependencies are Installed
Topcoat crates (`web_app_common_tc`, `web_app_tc`, `web_app_admin_tc`) compile Tailwind CSS v4 in `build.rs` and require `daisyui` in `node_modules`.
```bash
npm ci
```

### 2. Verify Git Branch & Upstream Tracking Target
Ensure your local branch is tracking its own feature branch on the remote, not `origin/dev`:
```bash
git branch -vv
```
If your branch is tracking `origin/dev`, set it to track its own remote branch:
```bash
git push -u origin <branch-name>
```

### 3. Dependency Updates & Vulnerability Auditing
Ensure dependencies are up to date and check for security vulnerabilities:
```bash
cargo update
cargo audit
```
*(Note: Ignore RUSTSEC-2024-0436 and RUSTSEC-2023-0071 if unpatched upstream).*

### 4. Code Formatting
Ensure all crates adhere to the standard format:
```bash
cargo fmt --check
```

### 5. Linter Check (Clippy)
Run clippy across all workspace members with warnings treated as errors:
```bash
SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features --manifest-path Cargo.toml -- -D warnings
```
*(Note: If this command fails due to a `DATABASE_URL` error, prepend `RUN_ENV=Development DATABASE_URL=postgres://postgres:password@localhost:5432/our_places` and run it again).*

### 6. SQLx Query Cache Verification (Offline Compilation)
Ensure offline query data is prepared and verify all crates and test targets compile cleanly under `SQLX_OFFLINE=true`:
1. If DB queries changed, update query cache (with local Postgres running):
   ```bash
   cargo sqlx prepare --workspace -- --all-targets
   ```
2. Verify offline compilation succeeds across all library and test targets:
   ```bash
   SQLX_OFFLINE=true cargo check --workspace --exclude protoproj --all-features --tests
   ```
> [!WARNING]
> Test setup queries in `#[cfg(test)]` should avoid `sqlx::query!` compile-time macros unless prepared; use runtime `sqlx::query(...).bind(...)` instead to prevent `SQLX_OFFLINE=true` query missing errors.

### 7. Run Test Suites Mirroring CI Jobs
Run the test suites in the same isolated configurations as CI to catch environment-specific issues (such as missing daemons or unmocked localhost connections):

1. **Backend & Shared Domain Tests (mirrors `test-api` job)**:
   ```bash
   cargo test --verbose --workspace --exclude web_app --exclude web_app_admin --exclude web_app_common --exclude web_app_tc --exclude web_app_admin_tc --exclude web_app_common_tc --exclude protoproj
   ```
2. **Topcoat Web & Common Tests (mirrors `test-web-tc` job)**:
   ```bash
   cargo test -p web_app_tc -p web_app_common_tc --verbose
   ```
3. **Topcoat Admin Tests (mirrors `test-web-admin-tc` job)**:
   ```bash
   cargo test -p web_app_admin_tc --verbose
   ```
> [!NOTE]
> Frontend unit tests must not require a live backend API server running on `localhost:8082/8081/8083`. If a test exercises client calls, ensure it includes offline fallback handling.