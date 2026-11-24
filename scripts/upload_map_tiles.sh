#!/usr/bin/env bash
# Publishes one version of the map tiles to its bucket (spec 103 §1.10).
#
# usage: scripts/upload_map_tiles.sh <bucket> <version> <artifact_dir>
#        DEV_BUCKET=<bucket> scripts/upload_map_tiles.sh <bucket> <version> --copy-from-dev
#
#   bucket        target bucket name, no gs:// (e.g. our-places-gcs-map-tiles-dev)
#   version       YYYYMMDD
#   artifact_dir  this run's output of build_map_tiles.sh
#   --copy-from-dev
#                 take gs://$DEV_BUCKET/v<version>/ (which must be _READY)
#                 instead of a build; it is verified the same way
#
# Exactly one run publishes a version: it takes v<version>/_LOCK with
# --if-generation-match=0 and releases it on exit. A run that loses waits
# for _READY for up to LOCK_WAIT_SECONDS; on timeout it warns and exits 0,
# so the setup job isn't blocked (the portals then run with maps off).
#
# Nothing from the artifact is executed or sourced. Only an allowlist of
# paths is uploaded, every file must match MANIFEST, and symlinks or
# anything else that isn't a regular file are refused. _READY is written
# last.
#
# Environment (for tests): PUBLIC_HOST (default https://storage.googleapis.com),
# LOCK_WAIT_SECONDS (default 600), LOCK_STALE_SECONDS (default 600),
# POLL_SECONDS (default 15).
set -euo pipefail

readonly PUBLIC_HOST="${PUBLIC_HOST:-https://storage.googleapis.com}"
readonly LOCK_WAIT_SECONDS="${LOCK_WAIT_SECONDS:-600}"
readonly LOCK_STALE_SECONDS="${LOCK_STALE_SECONDS:-600}"
readonly POLL_SECONDS="${POLL_SECONDS:-15}"
readonly CACHE_CONTROL="public, max-age=31536000, immutable"
readonly BUCKET_RE='^[a-z0-9][a-z0-9._-]{1,61}[a-z0-9]$'
# Every path a version folder may hold (relative to the folder).
readonly ALLOWED_RE='^(jamaica\.pmtiles|style-(light|dark)\.json|BUILD_INFO|MANIFEST|fonts/OFL\.txt|fonts/Noto Sans (Regular|Medium|Italic)/[0-9]{1,5}-[0-9]{1,5}\.pbf|sprites/v4/(light|dark)(@2x)?\.(json|png))$'

die() {
  echo "::error::upload_map_tiles: $*" >&2
  exit 1
}

usage() {
  echo "usage: $0 <bucket> <version> <artifact_dir|--copy-from-dev>" >&2
  exit 2
}

[[ $# -eq 3 ]] || usage
bucket="$1"
version="$2"
source_arg="$3"

[[ "$bucket" =~ $BUCKET_RE ]] || die "bad bucket name: '$bucket'"
[[ "$version" =~ ^[0-9]{8}$ ]] || die "version must be 8 digits (YYYYMMDD), got '$version'"

readonly prefix="gs://$bucket/v$version"
readonly public_prefix="$PUBLIC_HOST/$bucket/v$version"
readonly lock_url="$prefix/_LOCK"
readonly ready_url="$prefix/_READY"

work="$(mktemp -d)"
lock_generation=""

cleanup() {
  if [[ -n "$lock_generation" ]]; then
    gcloud storage rm --quiet --if-generation-match="$lock_generation" "$lock_url" >/dev/null 2>&1 \
      || echo "::warning::could not release $lock_url (generation $lock_generation)"
  fi
  rm -rf "$work"
}
trap cleanup EXIT

ready_exists() {
  gcloud storage objects describe "$ready_url" --format='value(name)' >/dev/null 2>&1
}

# Prints "<generation> <creation epoch seconds>" for the lock, or nothing.
lock_info() {
  gcloud storage objects describe "$lock_url" --format=json 2>/dev/null \
    | jq -r '"\(.generation) \(.creation_time)"' \
    | while read -r gen created; do echo "$gen $(date -d "$created" +%s)"; done
}

# Tries to take the lock; on success sets lock_generation.
take_lock() {
  echo "$GITHUB_RUN_ID_OR_LOCAL" > "$work/_LOCK"
  if gcloud storage cp --quiet --if-generation-match=0 "$work/_LOCK" "$lock_url" >/dev/null 2>&1; then
    lock_generation="$(gcloud storage objects describe "$lock_url" --format='value(generation)')"
    echo "took $lock_url (generation $lock_generation)"
    return 0
  fi
  return 1
}

GITHUB_RUN_ID_OR_LOCAL="run ${GITHUB_RUN_ID:-local} attempt ${GITHUB_RUN_ATTEMPT:-0} at $(date -u +%FT%TZ)"

if ready_exists; then
  echo "$ready_url already exists; nothing to do."
  exit 0
fi

if ! take_lock; then
  deadline=$(( $(date +%s) + LOCK_WAIT_SECONDS ))
  while true; do
    if ready_exists; then
      echo "another run published v$version."
      exit 0
    fi
    gen=""
    created=""
    read -r gen created < <(lock_info) || true
    now=$(date +%s)
    if [[ -z "${gen:-}" ]]; then
      # The holder released without publishing (it failed); try again.
      take_lock && break
    elif (( now - created > LOCK_STALE_SECONDS )); then
      echo "clearing stale $lock_url (generation $gen, $(( now - created ))s old)"
      # Generation-matched, so a lock another run just took is never deleted.
      gcloud storage rm --quiet --if-generation-match="$gen" "$lock_url" >/dev/null 2>&1 || true
      take_lock && break
    fi
    if (( now >= deadline )); then
      echo "::warning::timed out after ${LOCK_WAIT_SECONDS}s waiting for $ready_url; maps stay off until a later run publishes v$version."
      exit 0
    fi
    sleep "$POLL_SECONDS"
  done
fi

# --- From here on this run holds the lock. ---

if [[ "$source_arg" == "--copy-from-dev" ]]; then
  [[ "${DEV_BUCKET:-}" =~ $BUCKET_RE ]] || die "DEV_BUCKET must name the dev bucket for --copy-from-dev"
  [[ "$DEV_BUCKET" != "$bucket" ]] || die "DEV_BUCKET is the target bucket"
  gcloud storage objects describe "gs://$DEV_BUCKET/v$version/_READY" --format='value(name)' >/dev/null 2>&1 \
    || die "gs://$DEV_BUCKET/v$version/_READY does not exist"
  echo "::group::download gs://$DEV_BUCKET/v$version"
  src="$work/src"
  mkdir "$src"
  gcloud storage cp --quiet -r "gs://$DEV_BUCKET/v$version/*" "$src/"
  rm -f "$src/_READY" "$src/_LOCK"
  echo "::endgroup::"
else
  src="$source_arg"
  [[ -d "$src" && ! -L "$src" ]] || die "artifact_dir is not a directory: '$src'"
fi

echo "::group::verify $src"
[[ -f "$src/MANIFEST" && ! -L "$src/MANIFEST" ]] || die "MANIFEST missing"
# Nothing but regular files and directories.
odd="$(find "$src" -mindepth 1 ! -type f ! -type d -print -quit)"
[[ -z "$odd" ]] || die "not a regular file: $odd"
# Every file is allowed and listed; every listed file is allowed and exists.
(cd "$src" && find . -type f -printf '%P\n') | LC_ALL=C sort > "$work/files"
sed -E 's/^[0-9a-f]{64}  //' "$src/MANIFEST" | LC_ALL=C sort > "$work/listed"
grep -Evq '^[0-9a-f]{64}  ' "$src/MANIFEST" && die "MANIFEST has a malformed line"
bad="$(grep -Ev "$ALLOWED_RE" "$work/files" || true)"
[[ -z "$bad" ]] || die "unexpected path(s): $(head -3 <<<"$bad" | tr '\n' ' ')"
bad="$(grep -Ev "$ALLOWED_RE" "$work/listed" || true)"
[[ -z "$bad" ]] || die "MANIFEST lists unexpected path(s): $(head -3 <<<"$bad" | tr '\n' ' ')"
# MANIFEST doesn't list itself.
grep -vx 'MANIFEST' "$work/files" > "$work/files_but_manifest" || true
cmp -s "$work/files_but_manifest" "$work/listed" || die "files and MANIFEST differ"
for required in jamaica.pmtiles style-light.json style-dark.json BUILD_INFO; do
  grep -qx "$required" "$work/listed" || die "MANIFEST lacks $required"
done
(cd "$src" && sha256sum --quiet --strict -c MANIFEST) || die "MANIFEST check failed"
jq -e --arg v "$version" '.version == $v' "$src/BUILD_INFO" >/dev/null || die "BUILD_INFO is not for v$version"
echo "verified $(wc -l < "$work/listed") files"
echo "::endgroup::"

echo "::group::upload to $prefix"
# One cp per top-level entry, so folders land under the version prefix.
# jamaica.pmtiles goes up as stored: no -z/-Z, so no Content-Encoding.
for entry in BUILD_INFO MANIFEST jamaica.pmtiles style-light.json style-dark.json fonts sprites; do
  gcloud storage cp --quiet -r --cache-control="$CACHE_CONTROL" "$src/$entry" "$prefix/"
done
echo "::endgroup::"

pmtiles_meta="$(gcloud storage objects describe "$prefix/jamaica.pmtiles" --format=json)"
[[ "$(jq -r '.content_encoding // ""' <<<"$pmtiles_meta")" == "" ]] || die "jamaica.pmtiles has a Content-Encoding"
crc32c="$(jq -r '.crc32c_hash' <<<"$pmtiles_meta")"
[[ -n "$crc32c" && "$crc32c" != "null" ]] || die "no crc32c for jamaica.pmtiles"

headers="$(curl -s -D - -o /dev/null -H 'Range: bytes=0-127' -H 'Cache-Control: no-cache' \
  "$public_prefix/jamaica.pmtiles?t=$(date +%s)")"
grep -qE '^HTTP/[0-9.]+ 206' <<<"$headers" || die "ranged GET did not return 206: $(head -1 <<<"$headers")"
if grep -qi '^content-encoding:' <<<"$headers"; then
  die "ranged GET returned a Content-Encoding"
fi
echo "ranged GET: 206, no Content-Encoding"

jq -n \
  --slurpfile build "$src/BUILD_INFO" \
  --arg manifest "$(sha256sum "$src/MANIFEST" | cut -d' ' -f1)" \
  --arg crc32c "$crc32c" \
  --arg published "$(date -u +%FT%TZ)" \
  --arg by "$GITHUB_RUN_ID_OR_LOCAL" \
  '{version: $build[0].version, source: $build[0].source, tools: $build[0].tools,
    manifest_sha256: $manifest, pmtiles_crc32c: $crc32c, published: $published, by: $by}' \
  > "$work/_READY"
gcloud storage cp --quiet --cache-control="$CACHE_CONTROL" --if-generation-match=0 "$work/_READY" "$ready_url"
echo "published v$version:"
cat "$work/_READY"
