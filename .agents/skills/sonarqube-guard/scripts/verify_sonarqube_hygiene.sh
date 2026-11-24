#!/usr/bin/env bash
# verify_sonarqube_hygiene.sh — Static Analysis & SonarQube / DeepSource Hygiene Checker
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$REPO_ROOT"

MODE_WORKING="working"
INDENT_SED='s/^/      /'

MODE="staged"
if [[ "${1:-}" == "--all" ]]; then
  MODE="all"
elif [[ "${1:-}" == "--staged" ]]; then
  MODE="staged"
elif [[ -z "$(git diff --cached --name-only 2>/dev/null)" ]]; then
  MODE="$MODE_WORKING"
fi

echo "==> Running SonarQube & DeepSource Hygiene Audit [Mode: $MODE]..."
ERRORS_FOUND=0

# Helper to filter test files
is_test_file() {
  local f="$1"
  if [[ "$f" =~ /tests/ || "$f" =~ tests\.rs$ || "$f" =~ test_.*\.rs$ || "$f" =~ .*_test\.rs$ || "$f" =~ .*_tests\.rs$ ]]; then
    return 0
  fi
  return 1
}

# 1. Check for hardcoded "/tmp" paths in non-test Rust files (DeepSource RS-S1003)
echo -n "  [1/6] Checking for hardcoded /tmp in Rust source files... "
if [[ "$MODE" == "all" ]]; then
  TMP_MATCHES=$(git grep -n -E '"/tmp(/[^"]*)?"' -- '*.rs' ':!*/tests/*' ':!*/tests.rs' ':!*test*.rs' 2>/dev/null || true)
else
  DIFF_CMD="git diff --cached -U0 -- '*.rs'"
  [[ "$MODE" == "$MODE_WORKING" ]] && DIFF_CMD="git diff -U0 -- '*.rs'"
  TMP_MATCHES=$(eval "$DIFF_CMD" 2>/dev/null | grep '^+[^+]' | grep -E '"/tmp(/[^"]*)?"' || true)
fi

if [[ -n "$TMP_MATCHES" ]]; then
  echo "FAIL"
  echo "    Found hardcoded /tmp path (DeepSource RS-S1003). Use std::env::temp_dir() instead:"
  echo "$TMP_MATCHES" | sed "$INDENT_SED"
  ERRORS_FOUND=$((ERRORS_FOUND + 1))
else
  echo "PASS"
fi

# 2. Check for raw string literals in env::var calls (DeepSource RS-W1015)
echo -n "  [2/6] Checking for string literals in env::var calls... "
if [[ "$MODE" == "all" ]]; then
  ENV_LITERALS=$(git grep -n -E '(std::)?env::var\("[^"]+"\)' -- '*.rs' ':!*/tests/*' ':!*/tests.rs' ':!*test*.rs' ':!*build.rs' 2>/dev/null || true)
else
  DIFF_CMD="git diff --cached -U0 -- '*.rs'"
  [[ "$MODE" == "$MODE_WORKING" ]] && DIFF_CMD="git diff -U0 -- '*.rs'"
  ENV_LITERALS=$(eval "$DIFF_CMD" 2>/dev/null | grep '^+[^+]' | grep -E '(std::)?env::var\("[^"]+"\)' || true)
fi

if [[ -n "$ENV_LITERALS" ]]; then
  echo "FAIL"
  echo "    Found string literal in env::var (DeepSource RS-W1015). Use static CONST_ENV: &str = \"...\":"
  echo "$ENV_LITERALS" | sed "$INDENT_SED"
  ERRORS_FOUND=$((ERRORS_FOUND + 1))
else
  echo "PASS"
fi

# 3. Check HTML email templates for required structure (SonarCloud S5254, S5148)
echo -n "  [3/6] Checking HTML email templates for <html lang>, <head>, <meta charset>, and <title>... "
TEMPLATE_FAILURES=""
while IFS= read -r template_file; do
  [[ -z "$template_file" ]] && continue
  if ! grep -q '<html[^>]*lang=' "$template_file"; then
    TEMPLATE_FAILURES="$TEMPLATE_FAILURES"$'\n'"$template_file: missing <html lang=\"en\">"
  fi
  if ! grep -q '<head>' "$template_file"; then
    TEMPLATE_FAILURES="$TEMPLATE_FAILURES"$'\n'"$template_file: missing <head>"
  fi
  if ! grep -qi '<meta[^>]*charset=' "$template_file"; then
    TEMPLATE_FAILURES="$TEMPLATE_FAILURES"$'\n'"$template_file: missing <meta charset=\"utf-8\">"
  fi
  if ! grep -q '<title>' "$template_file"; then
    TEMPLATE_FAILURES="$TEMPLATE_FAILURES"$'\n'"$template_file: missing <title>"
  fi
done < <(find app_api/email_worker/templates -name "*.html" 2>/dev/null || true)

if [[ -n "$TEMPLATE_FAILURES" ]]; then
  echo "FAIL"
  echo "    HTML template standards violation (SonarCloud S5254/S5148):"
  echo "$TEMPLATE_FAILURES" | sed "$INDENT_SED"
  ERRORS_FOUND=$((ERRORS_FOUND + 1))
else
  echo "PASS"
fi

# 4. Check for Sonar and DeepSource migration exclusions
echo -n "  [4/6] Verifying static analysis tool migration exclusions... "
CONFIG_FAILURES=""
if [[ ! -f "sonar-project.properties" ]] || ! grep -q "sonar.exclusions=.*db_core/migrations" "sonar-project.properties"; then
  CONFIG_FAILURES="$CONFIG_FAILURES"$'\n'"sonar-project.properties: missing migration exclusion"
fi
if [[ ! -f ".sonarcloud.properties" ]] || ! grep -q "sonar.exclusions=.*db_core/migrations" ".sonarcloud.properties"; then
  CONFIG_FAILURES="$CONFIG_FAILURES"$'\n'".sonarcloud.properties: missing migration exclusion"
fi
if [[ ! -f ".deepsource.toml" ]] || ! grep -q "db_core/migrations" ".deepsource.toml"; then
  CONFIG_FAILURES="$CONFIG_FAILURES"$'\n'".deepsource.toml: missing migration exclusion in exclude_patterns"
fi

if [[ -n "$CONFIG_FAILURES" ]]; then
  echo "FAIL"
  echo "    Static analysis configuration missing migration exclusions:"
  echo "$CONFIG_FAILURES" | sed "$INDENT_SED"
  ERRORS_FOUND=$((ERRORS_FOUND + 1))
else
  echo "PASS"
fi

# 5. Check database migration immutability against parent
echo -n "  [5/6] Checking database migration immutability... "
MODIFIED_MIGRATIONS=$(git diff --diff-filter=M --name-only origin/main...HEAD -- 'db_core/migrations/*' 2>/dev/null || git diff --diff-filter=M --name-only HEAD~1 -- 'db_core/migrations/*' 2>/dev/null || true)
STAGED_MOD_MIGRATIONS=$(git diff --cached --diff-filter=M --name-only -- 'db_core/migrations/*' 2>/dev/null || true)

if [[ -n "$MODIFIED_MIGRATIONS" || -n "$STAGED_MOD_MIGRATIONS" ]]; then
  echo "FAIL"
  echo "    Attempted modification of previously applied migrations (Checksum invalidation risk):"
  for mf in $MODIFIED_MIGRATIONS $STAGED_MOD_MIGRATIONS; do
    echo "      $mf"
  done
  ERRORS_FOUND=$((ERRORS_FOUND + 1))
else
  echo "PASS"
fi

# 6. Check for unwrap/expect in production code paths
echo -n "  [6/6] Checking for .unwrap() and .expect() in non-test Rust source... "
if [[ "$MODE" == "all" ]]; then
  UNWRAP_MATCHES=$(git grep -n -E '\.(unwrap|expect)\(' -- '*.rs' ':!*/tests/*' ':!*/tests.rs' ':!*test*.rs' 2>/dev/null | grep -v 'unwrap_or' | grep -v 'unwrap_err' || true)
  if [[ -n "$UNWRAP_MATCHES" ]]; then
    UNWRAP_COUNT=$(echo "$UNWRAP_MATCHES" | wc -l)
    echo "WARN ($UNWRAP_COUNT instances found across repo)"
  else
    echo "PASS"
  fi
else
  DIFF_CMD="git diff --cached -U0 -- '*.rs'"
  [[ "$MODE" == "$MODE_WORKING" ]] && DIFF_CMD="git diff -U0 -- '*.rs'"
  UNWRAP_MATCHES=$(eval "$DIFF_CMD" 2>/dev/null | grep '^+[^+]' | grep -E '\.(unwrap|expect)\(' | grep -v 'unwrap_or' | grep -v 'unwrap_err' || true)
  if [[ -n "$UNWRAP_MATCHES" ]]; then
    echo "FAIL"
    echo "    Found .unwrap() or .expect() in changed production code (AGENTS.md / DeepSource RS-W1072):"
    echo "$UNWRAP_MATCHES" | sed "$INDENT_SED"
    ERRORS_FOUND=$((ERRORS_FOUND + 1))
  else
    echo "PASS"
  fi
fi

echo "=========================================================="
if [[ $ERRORS_FOUND -gt 0 ]]; then
  echo "❌ $ERRORS_FOUND SonarQube / DeepSource hygiene violations detected."
  exit 1
else
  echo "✅ All SonarQube & DeepSource hygiene guard checks passed!"
  exit 0
fi
