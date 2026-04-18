---
description: Sanity check
---

Actions to use whenever significant code changes are done

1. Check linting with cargo clippy
    execute: cargo clippy --workspace --exclude protoproj --all-features --manifest-path Cargo.toml -- -D warnings
2. Ensure code is properly formatted
    execute: cargo fmt --check
3. Keep sqlx files updated
    execute: cargo sqlx prepare --workspace