#!/usr/bin/env bash
set -e

# Navigate to script directory (app_api/user_api)
cd "$(dirname "$0")"

# Run User API service
EA__SERVER__PORT="${EA__SERVER__PORT:-8083}" \
EA__DATABASE__HOST="${EA__DATABASE__HOST:-localhost}" \
cargo run
