#!/usr/bin/env python3
"""
Create Git Worktree Script
Creates a new git worktree branching from an up-to-date source branch,
named according to GitHub issue number and title.
"""

import argparse
import json
import os
import re
import subprocess
import sys


def run_cmd(cmd, cwd=None, check=True):
    """Execute shell command and return stdout."""
    res = subprocess.run(cmd, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if check and res.returncode != 0:
        raise RuntimeError(f"Command failed ({' '.join(cmd)}):\n{res.stderr.strip() or res.stdout.strip()}")
    return res.stdout.strip()


def get_issue_title(issue_num):
    """Fetch GitHub issue title using gh CLI."""
    try:
        raw_out = run_cmd(["gh", "issue", "view", str(issue_num), "--json", "title"])
        # Match JSON object in output (skipping any tty progress characters)
        match = re.search(r"\{.*\}", raw_out, re.DOTALL)
        if match:
            data = json.loads(match.group(0))
            return data.get("title", "")
        # Fallback to direct string if no JSON brackets found
        return raw_out.strip()
    except Exception as e:
        print(f"Error fetching issue #{issue_num} via gh: {e}", file=sys.stderr)
        return None


def slugify(text):
    """Convert text to lowercase hyphenated slug."""
    text = text.lower()
    # Replace any non-alphanumeric character with hyphen
    text = re.sub(r"[^a-z0-9]+", "-", text)
    # Collapse multiple hyphens into single hyphen
    text = re.sub(r"-+", "-", text)
    return text.strip("-")


def format_branch_name(issue_num, title, prefix="feat", max_length=62):
    """
    Format branch name: <prefix>/<issue_num>-<slug> (or <issue_num>-<slug> if prefix is empty).
    Ensures branch name is strictly less than 63 characters (< 63).
    """
    slug = slugify(title)
    if prefix:
        base = f"{prefix}/{issue_num}-"
    else:
        base = f"{issue_num}-"

    max_slug_len = max_length - len(base)
    if max_slug_len <= 0:
        return base.rstrip("-/")

    if len(slug) > max_slug_len:
        slug = slug[:max_slug_len].rstrip("-")

    return f"{base}{slug}"


def get_repo_info():
    """Detect git repo root and base repo name."""
    toplevel = run_cmd(["git", "rev-parse", "--show-toplevel"])
    # Find common main repository name (e.g. our_places_rs)
    dir_name = os.path.basename(toplevel)
    # If currently inside a worktree like our_places_rs-feat-76-..., extract base name
    match = re.match(r"^([a-zA-Z0-9_]+)-", dir_name)
    if match:
        base_name = match.group(1)
    else:
        base_name = dir_name

    parent_dir = os.path.dirname(toplevel)
    return toplevel, parent_dir, base_name


def main():
    parser = argparse.ArgumentParser(description="Create a git worktree for a GitHub issue.")
    parser.add_argument("issue", type=int, help="GitHub issue number")
    parser.add_argument("source_branch", nargs="?", default="dev", help="Source branch to branch from (default: dev)")
    parser.add_argument("--prefix", default="feat", help="Branch prefix (e.g. 'feat', 'fix', or empty for none; default: 'feat')")
    parser.add_argument("--title", help="Override issue title instead of fetching from GitHub")
    parser.add_argument("--dry-run", action="store_true", help="Print what would be done without creating worktree")

    args = parser.parse_args()

    # 1. Obtain issue title
    title = args.title
    if not title:
        print(f"Fetching details for issue #{args.issue}...")
        title = get_issue_title(args.issue)
        if not title:
            print(f"Failed to retrieve title for issue #{args.issue}. Please provide --title manually.", file=sys.stderr)
            sys.exit(1)
        print(f"Issue #{args.issue} Title: {title}")

    # 2. Generate branch name
    prefix = args.prefix.strip("/") if args.prefix else ""
    branch_name = format_branch_name(args.issue, title, prefix=prefix, max_length=62)
    print(f"Target Branch Name: {branch_name} (Length: {len(branch_name)} chars)")

    # 3. Pull latest from source branch
    print(f"Fetching latest changes from origin/{args.source_branch}...")
    try:
        run_cmd(["git", "fetch", "origin", args.source_branch])
    except Exception as e:
        print(f"Warning/Error fetching origin/{args.source_branch}: {e}", file=sys.stderr)
        print(f"Attempting to proceed with local {args.source_branch} reference...")

    # 4. Determine worktree path
    toplevel, parent_dir, base_name = get_repo_info()
    sanitized_branch = branch_name.replace("/", "-")
    worktree_dir_name = f"{base_name}-{sanitized_branch}"
    worktree_path = os.path.join(parent_dir, worktree_dir_name)

    print(f"Target Worktree Path: {worktree_path}")

    if os.path.exists(worktree_path):
        print(f"Error: Target worktree directory already exists: {worktree_path}", file=sys.stderr)
        sys.exit(1)

    if args.dry_run:
        print("\n[Dry Run] Worktree creation skipped.")
        return

    # 5. Create worktree
    # Check if origin/<source_branch> exists, otherwise fallback to source_branch
    base_ref = f"origin/{args.source_branch}"
    has_remote_ref = subprocess.run(["git", "rev-parse", "--verify", base_ref], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0
    if not has_remote_ref:
        base_ref = args.source_branch

    print(f"Creating worktree based on '{base_ref}'...")
    run_cmd(["git", "worktree", "add", worktree_path, "-b", branch_name, base_ref])

    # 6. Copy .env if present in current workspace
    env_src = os.path.join(toplevel, ".env")
    env_dest = os.path.join(worktree_path, ".env")
    if os.path.isfile(env_src) and not os.path.exists(env_dest):
        try:
            import shutil
            shutil.copy2(env_src, env_dest)
            print("Copied .env to new worktree.")
        except Exception as e:
            print(f"Note: Could not copy .env: {e}")

    print("\n✅ Worktree successfully created!")
    print(f"  Branch:   {branch_name}")
    print(f"  Location: {worktree_path}")
    print(f"  Source:   {base_ref}")
    print(f"\nTo switch to the new worktree:\n  cd {worktree_path}")


if __name__ == "__main__":
    main()
