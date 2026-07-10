---
description: Sanity check
---

Actions to use whenever significant code changes are done

> [!IMPORTANT]
> **CRITICAL RULE:** If any of the steps below fail you should apply a fix. Tou **MUST re-run that specific command** and apply fixes until it completes with 0 errors before moving on to the next step. Do not assume your fix worked.

1. Run `cargo update` to ensure all dependencies are at the latest
2. Check linting with cargo clippy
    execute: `cargo clippy --workspace --exclude protoproj --all-features --manifest-path Cargo.toml -- -D warnings`
    (Note: If this command fails due to a `DATABASE_URL` error, prepend the `DATABASE_URL` environment variable and run it again.)
3. Ensure code is properly formatted
    execute: `cargo fmt --check`
4. Keep sqlx files updated
    execute: `cargo sqlx prepare --workspace`
5. Run unit tests
    execute: `cargo test --workspace --exclude protoproj`