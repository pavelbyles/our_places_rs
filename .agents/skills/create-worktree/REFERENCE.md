# Create Worktree Reference

## 1. Branch Naming Examples

| Issue | Title | Sanitized Branch Name (< 63 chars) |
| :--- | :--- | :--- |
| **#46** | Add listing from existing listing | `feat/46-add-listing-from-existing-listing` |
| **#47** | Update check-in times | `feat/47-update-check-in-times` |
| **#54** | Add user profile features | `feat/54-add-user-profile-features` |
| **#56** | Refactor pricing into its own module | `56-refactor-pricing-into-its-own-module` |
| **#57** | Dynamic seasonal pricing calculation | `feat/57-dynamic-seasonal-pricing` |
| **#63** | Verified guest review system | `feat/63-verified-guest-review-system` |

---

## 2. Manual Execution Workflow (Without Script)

If running commands manually without `create_worktree.py`:

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
   - Copy `.env` from the active repository root to the new worktree directory if it exists:
   ```bash
   cp .env ../<repo_name>-<sanitized_branch>/.env
   ```

6. **Confirm to User**:
   - Output the newly created branch name and absolute directory path.
