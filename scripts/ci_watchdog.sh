#!/usr/bin/env bash
# ci_watchdog.sh — Continuous GHA & PR Status Monitor with Failure Triage
set -euo pipefail

POLL_INTERVAL=15
WATCH_MODE=false
PR_NUM=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --watch)
      WATCH_MODE=true
      shift
      ;;
    --interval)
      POLL_INTERVAL="$2"
      shift 2
      ;;
    --pr)
      PR_NUM="$2"
      shift 2
      ;;
    *)
      echo "Unknown flag: $1" >&2
      exit 1
      ;;
  esac
done

# Resolve active branch
CURRENT_BRANCH=$(git branch --show-current)

# Resolve PR if not provided
if [[ -z "$PR_NUM" ]]; then
  PR_JSON=$(gh pr view --json number,headRefName 2>/dev/null || true)
  if [[ -n "$PR_JSON" ]]; then
    PR_NUM=$(echo "$PR_JSON" | jq -r '.number // empty')
  fi
fi

echo "==> CI Watchdog active"
echo "    Branch: $CURRENT_BRANCH"
if [[ -n "$PR_NUM" ]]; then
  echo "    PR:     #$PR_NUM"
fi

check_once() {
  # 1. Check latest GHA workflow run for the branch
  LATEST_RUN_JSON=$(gh run list --branch "$CURRENT_BRANCH" -L 1 --json databaseId,status,conclusion,displayTitle,headSha 2>/dev/null || echo "[]")
  RUN_ID=$(echo "$LATEST_RUN_JSON" | jq -r '.[0].databaseId // empty')
  RUN_STATUS=$(echo "$LATEST_RUN_JSON" | jq -r '.[0].status // empty')
  RUN_CONCLUSION=$(echo "$LATEST_RUN_JSON" | jq -r '.[0].conclusion // empty')
  RUN_TITLE=$(echo "$LATEST_RUN_JSON" | jq -r '.[0].displayTitle // empty')

  # 2. Check PR checks rollup if PR exists
  CHECKS_FAILING=false
  FAIL_SUMMARY=""

  if [[ -n "$RUN_ID" ]]; then
    if [[ "$RUN_STATUS" != "completed" ]]; then
      echo "⏳ GHA Run $RUN_ID is $RUN_STATUS ($RUN_TITLE)"
      return 2
    fi

    if [[ "$RUN_CONCLUSION" == "failure" || "$RUN_CONCLUSION" == "cancelled" ]]; then
      CHECKS_FAILING=true
      FAIL_SUMMARY="GHA Run $RUN_ID concluded with: $RUN_CONCLUSION"
    fi
  fi

  # Check PR status checks (including DeepSource / SonarCloud)
  if [[ -n "$PR_NUM" ]]; then
    ROLLUP_JSON=$(gh pr view "$PR_NUM" --json statusCheckRollup 2>/dev/null || echo "{}")
    FAILED_CHECKS=$(echo "$ROLLUP_JSON" | jq -r '.statusCheckRollup[]? | select(.conclusion == "FAILURE" or .state == "FAILURE") | "\(.name // .context)"')
    PENDING_CHECKS=$(echo "$ROLLUP_JSON" | jq -r '.statusCheckRollup[]? | select(.status == "IN_PROGRESS" or .status == "QUEUED" or .state == "PENDING") | "\(.name // .context)"')

    if [[ -n "$PENDING_CHECKS" ]]; then
      echo "⏳ Pending checks on PR #$PR_NUM:"
      echo "$PENDING_CHECKS" | sed 's/^/   - /'
      return 2
    fi

    if [[ -n "$FAILED_CHECKS" ]]; then
      CHECKS_FAILING=true
      FAIL_SUMMARY="$FAIL_SUMMARY"$'\n'"Failed PR Checks:"$'\n'"$FAILED_CHECKS"
    fi
  fi

  if [[ "$CHECKS_FAILING" == true ]]; then
    echo "❌ Failures detected in CI/CD pipeline:"
    echo "$FAIL_SUMMARY"
    
    if [[ -n "$RUN_ID" && "$RUN_CONCLUSION" == "failure" ]]; then
      echo "--- Extracting failed GHA logs ---"
      gh run view "$RUN_ID" --log-failed | tail -n 100 || true
    fi
    return 1
  fi

  echo "✅ All CI/CD build pipelines and status checks are GREEN."
  return 0
}

if [[ "$WATCH_MODE" == false ]]; then
  check_once
  exit $?
fi

while true; do
  set +e
  check_once
  STATUS=$?
  set -e

  if [[ $STATUS -eq 0 ]]; then
    echo "Watchdog: Build verified successful."
    exit 0
  elif [[ $STATUS -eq 1 ]]; then
    echo "Watchdog: Build failure identified. Halting watch for agent remediation."
    exit 1
  fi

  sleep "$POLL_INTERVAL"
done
