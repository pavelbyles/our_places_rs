---
name: create-worktree
description: Create a git worktree for a GitHub issue branching from an up-to-date source branch, following workspace branch and directory naming conventions. Use when creating a new worktree, setting up a branch for a GitHub issue, or when the user mentions creating a worktree.
---

# Create Worktree

Create an isolated Git worktree for concurrent feature development based on a GitHub issue number and source branch.

## Naming Conventions & Rules

1. **Branch Naming**:
   - Format: `feat/<issue_number>-<slug>` (default) or `<issue_number>-<slug>`
   - Slug: Derived from the GitHub issue title (lowercase, alphanumeric characters and hyphens only, collapsed dashes).
   - **Length Limit**: Total branch name MUST be strictly less than 63 characters (`< 63` characters, max 62 chars). Truncate cleanly without trailing hyphens.
   - Examples:
     - `feat/46-add-listing-from-existing-listing`
     - `feat/47-update-check-in-times`
     - `feat/54-add-user-profile-features`
     - `56-refactor-pricing-into-its-own-module`
     - `feat/57-dynamic-seasonal-pricing`
     - `feat/6-ff-hard-delete`
     - `feat/63-verified-guest-review-system`
     - `feat/7-ff--history-auditing`
     - `feat/9-implement-tracing-from-fe-to-be`

2. **Worktree Directory Naming**:
   - Format: `../<base_repo_name>-<sanitized_branch_name>` (where slashes `/` are replaced with hyphens `-`).
   - Example: For branch `feat/46-add-listing-from-existing-listing` in repo `our_places_rs`, directory is `../our_places_rs-feat-46-add-listing-from-existing-listing`.

3. **Latest Source Branch**:
   - Always fetch and use the latest commit from the remote source branch (`origin/<source_branch>`) before creating the worktree.

## Workflow

### Option A: Using the Automated Helper Script (Recommended)

Run the bundled Python script to fetch the issue, format the branch name, fetch remote changes, and create the worktree:

```bash
# Basic usage: <issue_number> [source_branch]
python3 .agents/skills/create-worktree/scripts/create_worktree.py <issue_number> <source_branch>

# Example: Issue 46 branching from dev
python3 .agents/skills/create-worktree/scripts/create_worktree.py 46 dev

# Without 'feat/' prefix (e.g. 56-refactor-pricing-into-its-own-module):
python3 .agents/skills/create-worktree/scripts/create_worktree.py 56 dev --prefix ""

# Dry run preview:
python3 .agents/skills/create-worktree/scripts/create_worktree.py 46 dev --dry-run
```

### Option B: Manual Execution Steps

1. **Fetch Issue Information**:
   ```bash
   gh issue view <issue_number> --json title -q .title
   ```

2. **Format Branch Name**:
   - Convert title to lowercase hyphenated slug.
   - Assemble `feat/<issue_number>-<slug>`.
   - Ensure `length < 63` characters.

3. **Fetch Latest from Source Branch**:
   ```bash
   git fetch origin <source_branch>
   ```

4. **Create Worktree**:
   ```bash
   git worktree add ../<repo_name>-<sanitized_branch> -b <branch_name> origin/<source_branch>
   ```

5. **Copy Environment File**:
   - Copy `.env` from the current workspace to the new worktree if it exists.

6. **Confirm to User**:
   - Report the created branch name and the full path to the new worktree.
