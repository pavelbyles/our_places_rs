---
name: build
description: Implement tasks incrementally — build, test, verify, commit. Add "auto" to execute the entire plan autonomously under a single approval checkpoint.
model: sonnet
---

# /build — Incremental & Autonomous Implementation

Implement tasks from `tasks/plan.md` test-first, one atomic commit per task. Follow `CONSTRAINTS.md` (no `.unwrap()`/`.expect()` in production code, `Decimal` for money, documented public items).

## Modes

- **`/build`**: implement the next pending task (`- [ ]`), commit, stop.
- **`/build auto`**: one preflight, one approval, then every task in dependency order.

## Per-task cycle (both modes)

1. **Read** the task's acceptance criteria, files and dependencies. Read only the code you need.
2. **Test first**: write a test for the behavior; confirm it fails for the expected reason.
3. **Implement** the minimal code that passes it.
4. **Verify only what the task touched** (run with `set -o pipefail`; never pipe a gate into `grep`/`tail` without it):
   - the focused test, then `cargo test -p <touched crates>`;
   - `SQLX_OFFLINE=true cargo clippy -p <touched crates> --all-targets -- -D warnings`;
   - `cargo fmt --check -p <touched crates>`;
   - the task's own verification command (e.g. a single Playwright spec).
   Do not run workspace-wide tests or clippy per task.
5. **Commit**: stage only the task's files plus `tasks/plan.md` (task marked `- [x]`), imperative message.

If a check fails, find the root cause before changing code; don't retry blindly.

## Auto mode

1. **Preflight** (one step): a spec exists in `docs/specs/` (if not, stop and suggest `/generate-spec`); `git status --porcelain` is clean apart from planning docs (if not, stop); `tasks/plan.md` exists (if not, run `/planning-and-task-breakdown`).
2. **Approval**: list the remaining tasks and ask once for scope. Treat hedged answers as no. Commit `tasks/plan.md` if it changed.
3. **Loop**: run the per-task cycle for each task in order. Report progress in one line per task.
4. **Stop and ask** if a failure can't be fixed cleanly, the spec is ambiguous or contradictory, or a step is irreversible (production data, live secrets, `terraform apply`).
5. **Done**: run the full gates once:
   ```bash
   set -o pipefail
   cargo test --workspace
   SQLX_OFFLINE=true cargo clippy --workspace --all-targets -- -D warnings
   ```
6. **Summary**: a compact table of tasks and commit hashes, the new tests, and anything deferred. Next: `/sanity-check-workflow` → `/ship-pr`.
