---
name: start-feature
description: Feature lifecycle kickoff — fetch GitHub issue, create an isolated git worktree branching from source branch, establish a compiling baseline, and hand off to spec generation.
---

# /start-feature — Feature Lifecycle Kickoff

## Goal
Kick off development for a GitHub issue by fetching its requirements, spinning up an isolated git worktree from a designated source branch, verifying a clean compiling baseline, and transitioning seamlessly into technical specification.

## When to Use
Run `/start-feature <ISSUE_NUMBER> [SOURCE_BRANCH]` whenever picking up a new GitHub issue.

* **`<ISSUE_NUMBER>`** *(required)*: The GitHub issue ID (e.g. `76`).
* **`[SOURCE_BRANCH]`** *(optional)*: Base branch to branch from (default: `dev`). Use `main` for hotfixes or specify a feature branch for stacked work.

---

## Process

### Step 1: Fetch GitHub Issue Context & Move to In-Progress
Retrieve and inspect the issue details, assign yourself, and mark the issue as in-progress:
```bash
gh issue view <ISSUE_NUMBER> --json number,title,body,labels
gh issue edit <ISSUE_NUMBER> --add-assignee "@me" --add-label "in-progress"
```

### Step 2: Create Dedicated Git Worktree
Spawn an isolated worktree branching from the latest source branch:
```bash
python3 .agents/skills/create-worktree/scripts/create_worktree.py <ISSUE_NUMBER> [SOURCE_BRANCH]
```
* Defaults to branching from `origin/dev`.
* Creates a sibling directory (e.g. `our_places_rs-feat-<issue_number>-<slug>`) and copies `.env` if present.

### Step 3: Publish & Link Branch to GitHub Issue
In the newly created worktree, publish the feature branch to `origin` and link it to the GitHub issue under the Development section:
```bash
git push -u origin <BRANCH_NAME>
gh issue develop <ISSUE_NUMBER> --name <BRANCH_NAME>
```

### Step 4: Verify Clean Baseline
In the newly created worktree:
1. Verify offline SQLx cache or database connection is ready.
2. Ensure the workspace compiles cleanly without errors:
   ```bash
   cargo check --workspace
   ```

### Step 5: Handoff to Specification & Architecture Review
Now that the active branch context is established in the worktree, activate the [`generate-spec`](../generate-spec/SKILL.md) skill:
$$\longrightarrow \text{\textbf{generate-spec}}$$
This will interview on design decisions via [`grill-me`](../grill-me/SKILL.md), execute performance and threat-modeling reviews, and write `docs/specs/spec-<branch_name>.md`.
