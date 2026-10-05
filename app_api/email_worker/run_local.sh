#!/usr/bin/env bash
set -e

# Helper function to ensure database is running and ready
ensure_db_running() {
  is_db_ready() {
    (echo > /dev/tcp/localhost/5432) >/dev/null 2>&1 || pg_isready -h localhost -p 5432 >/dev/null 2>&1
  }

  if is_db_ready; then
    return 0
  fi

  local lockfile="/tmp/our_places_db_startup.lock"
  (
    flock -x 200
    if ! is_db_ready; then
      echo "Database is not running on localhost:5432. Starting database via db-start..."
      ROOT_DIR=$(git rev-parse --show-toplevel 2>/dev/null || echo "$PWD/../..")
      (cd "$ROOT_DIR" && (db-start 2>/dev/null || devenv shell db-start 2>/dev/null || docker start ourplaces_db 2>/dev/null || true))
    fi
  ) 200>"$lockfile"

  local retries=5
  until is_db_ready || [ $retries -eq 0 ]; do
    echo "Waiting for database to accept connections... (${retries}s remaining)"
    sleep 1
    retries=$((retries - 1))
  done
}

ensure_db_running

# Run Email Worker service
EA__SERVER__PORT="${EA__SERVER__PORT:-8080}" \
EA__DATABASE__HOST="${EA__DATABASE__HOST:-localhost}" \
exec cargo run
