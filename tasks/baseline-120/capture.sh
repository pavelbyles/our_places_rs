#!/usr/bin/env bash
# Spec #120 T2/T15: response headers + cookie attributes for the portals.
# Run inside devenv from the repo root with both portals and the APIs up:
#   devenv shell -- tasks/baseline-120/capture.sh <guest|admin> <out_file>
set -u
kind=$1; out=$2
if [ "$kind" = guest ]; then base=http://localhost:3010; email=guest.tester@example.com; pw='guestPassword123!'; authed=/bookings
else base=http://localhost:3012; email=admin@ourplaces.io; pw=admin_changeme_2026; authed=/admin; fi
jar=$(mktemp); trap 'rm -f "$jar"' EXIT
norm() { tr -d '\r' | grep -viE '^(date|x-request-id|traceparent|content-length|etag|last-modified):' \
  | sed -E -e 's/^(set-cookie: *[^=]+=)[^;]*/\1<redacted>/I' -e 's/Expires=[^;]*/Expires=<date>/'; }
step() { local label=$1; shift; echo "### $label"; curl -s -o /dev/null -D - -b "$jar" -c "$jar" "$@" | norm; echo; }
{
  echo "# $kind portal response headers ($base) — baseline capture, topcoat $(grep -oE '^topcoat = \{ version = "[^"]+"' Cargo.toml | grep -oE '[0-9]+\.[0-9]+\.[0-9]+')"
  echo "# captured $(date -u +%FT%TZ); cookie values redacted; Date/ETag/request-id/Content-Length dropped"
  echo
  step "GET / (anonymous)" "$base/"
  step "GET /login (anonymous)" "$base/login"
  step "GET $authed (anonymous)" "$base$authed"
  step "POST /login" -X POST -H "Origin: $base" -H 'Content-Type: application/x-www-form-urlencoded' \
       --data-urlencode "email=$email" --data-urlencode "password=$pw" --data-urlencode "redirect=$authed" "$base/login"
  step "GET $authed (authenticated)" "$base$authed"
  step "GET $authed (authenticated, HX-Request partial)" -H 'HX-Request: true' "$base$authed"
  if [ "$kind" = guest ]; then step "GET /currency/EUR (currency cookie)" -H "Referer: $base/" "$base/currency/EUR"; fi
  step "POST /logout" -X POST -H "Origin: $base" "$base/logout"
  step "GET $authed (after logout)" "$base$authed"
} > "$out"
