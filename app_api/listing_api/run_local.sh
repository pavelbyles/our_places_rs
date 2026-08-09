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

  # Prevent race conditions when tmuxinator launches scripts concurrently
  local lockfile="/tmp/our_places_db_startup.lock"
  (
    flock -x 200
    if ! is_db_ready; then
      echo "Database is not running. Starting Docker container 'ourplaces_db'..."
      ROOT_DIR=$(git rev-parse --show-toplevel 2>/dev/null || echo "$PWD/../..")
      docker start ourplaces_db 2>/dev/null || (cd "$ROOT_DIR" && docker compose up -d db)
    fi
  ) 200>"$lockfile"

  # Wait up to 5 seconds for DB readiness
  local retries=5
  until is_db_ready || [ $retries -eq 0 ]; do
    echo "Waiting for database to accept connections... (${retries}s remaining)"
    sleep 1
    retries=$((retries - 1))
  done
}

ensure_db_running

# Determine safe branch name for GCS bucket
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "main")
SAFE_BRANCH=$(echo "$BRANCH" | tr '[:upper:]' '[:lower:]' | sed 's/[_\/]\/-/g' | sed 's/[^a-z0-9-]//g')

# Run Listing API service
EA__SERVER__PORT="${EA__SERVER__PORT:-8082}" \
EA__DATABASE__HOST="${EA__DATABASE__HOST:-localhost}" \
GCS_RAW_BUCKET="${GCS_RAW_BUCKET:-our-places-gcs-img-raw-${SAFE_BRANCH}}" \
GOOGLE_APPLICATION_CREDENTIALS="${GOOGLE_APPLICATION_CREDENTIALS:-/home/pav/Downloads/our-places-dev-sa-listing-api.json}" \
cargo run
