#!/usr/bin/env bash
set -euo pipefail

INTERVAL="${1:-${CALSYNC_INTERVAL:-60}}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

cleanup() {
  exit 0
}
trap cleanup SIGINT SIGTERM

echo "[$(date -u +'%Y-%m-%dT%H:%M:%SZ')] Calendar sync loop started (interval: ${INTERVAL}s)"
while true; do
  (
    cd "$SCRIPT_DIR" && \
    ./run_local.sh --quiet
  ) || true
  sleep "$INTERVAL" &
  wait $!
done
