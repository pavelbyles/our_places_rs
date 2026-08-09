#!/usr/bin/env bash
set -e

# Navigate to script directory (app_api/listing_api)
cd "$(dirname "$0")"

# Determine safe branch name for GCS bucket
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "main")
SAFE_BRANCH=$(echo "$BRANCH" | tr '[:upper:]' '[:lower:]' | sed 's/[_\/]\/-/g' | sed 's/[^a-z0-9-]//g')

# Run Listing API service
EA__SERVER__PORT="${EA__SERVER__PORT:-8082}" \
EA__DATABASE__HOST="${EA__DATABASE__HOST:-localhost}" \
GCS_RAW_BUCKET="${GCS_RAW_BUCKET:-our-places-gcs-img-raw-${SAFE_BRANCH}}" \
GOOGLE_APPLICATION_CREDENTIALS="${GOOGLE_APPLICATION_CREDENTIALS:-/home/pav/Downloads/our-places-dev-sa-listing-api.json}" \
cargo run
