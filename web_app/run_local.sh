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
      ROOT_DIR=$(git rev-parse --show-toplevel 2>/dev/null || echo "$PWD/..")
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

# Ensure node_modules exist
if [ ! -d "node_modules" ]; then
  echo "node_modules not found. Installing dependencies..."
  npm install
fi

# Build Tailwind CSS to ensure output.css is generated
npm run build:css

# Run Leptos development server
LISTING_API_URL="${LISTING_API_URL:-http://localhost:8082}" \
USER_API_URL="${USER_API_URL:-http://localhost:8083}" \
BOOKING_API_URL="${BOOKING_API_URL:-http://localhost:8081}" \
DATABASE_URL="${DATABASE_URL:-postgres://postgres:password@localhost:5432/our_places}" \
LEPTOS_SITE_ADDR="${LEPTOS_SITE_ADDR:-127.0.0.1:3000}" \
LEPTOS_RELOAD_PORT="${LEPTOS_RELOAD_PORT:-3001}" \
cargo leptos watch
