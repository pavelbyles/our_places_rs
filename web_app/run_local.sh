#!/usr/bin/env bash
set -e

# Navigate to script directory (web_app)
cd "$(dirname "$0")"

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
