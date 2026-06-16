---
description: Sanity check
---

Actions to use whenever significant code changes are done

> [!IMPORTANT]
> **CRITICAL RULE:** If any of the steps below fail and you apply a fix, you **MUST re-run that specific command** until it completes with 0 errors before moving on to the next step. Do not assume your fix worked.

1. Check linting with cargo clippy
    execute: `cargo clippy --workspace --exclude protoproj --all-features --manifest-path Cargo.toml -- -D warnings`
    (Note: If this command fails due to a `DATABASE_URL` error, prepend the `DATABASE_URL` environment variable and run it again.)
2. Ensure code is properly formatted
    execute: `cargo fmt --check`
3. Keep sqlx files updated
    execute: `cargo sqlx prepare --workspace`