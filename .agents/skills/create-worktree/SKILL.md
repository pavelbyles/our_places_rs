---
name: create-worktree
description: Create isolated git worktrees and branches for GitHub issues following workspace naming conventions.
---

# Create Worktree

Create an isolated Git worktree for concurrent feature development based on a GitHub issue number and source branch.

## Core Rules & Constraints

1. **Branch Naming**:
   - Standard format: `feat/<issue_number>-<slug>` (or `<issue_number>-<slug>`).
   - Slug: Derived from the GitHub issue title (lowercase, alphanumeric, collapsed hyphens).
   - **Length Limit**: Total branch name MUST be strictly `< 63` characters (max 62 chars). Truncate cleanly without trailing hyphens.

2. **Directory Naming**:
   - Format: `../<base_repo_name>-<sanitized_branch_name>` (slashes `/` replaced with hyphens `-`).
   - Example: `../our_places_rs-feat-46-add-listing-from-existing-listing`.

3. **Remote Synchronization**:
   - Always fetch the latest commit from remote `origin/<source_branch>` before creation.

---

## Execution Workflow

### Automated Script (Recommended)
Run the bundled Python helper script:
```bash
# Basic usage: <issue_number> [source_branch]
python3 .agents/skills/create-worktree/scripts/create_worktree.py <issue_number> <source_branch>

# Example: Issue 46 branching from dev
python3 .agents/skills/create-worktree/scripts/create_worktree.py 46 dev

# Without 'feat/' prefix:
python3 .agents/skills/create-worktree/scripts/create_worktree.py 56 dev --prefix ""
```

---

## Reference
* For manual step-by-step git commands, edge cases, and branch name examples, see **[REFERENCE.md](REFERENCE.md)**.
