#!/usr/bin/env bash
# Builds one version of the map tiles and assets (spec 103 §1.10).
#
# usage: scripts/build_map_tiles.sh [--dry-run] <version> <base_url> <out_dir>
#
#   version   Protomaps daily build date, YYYYMMDD
#   base_url  where the folder will be served, ending in /v<version>
#   out_dir   an existing, empty directory
#
# Writes jamaica.pmtiles, style-light.json, style-dark.json, fonts/, sprites/,
# BUILD_INFO and MANIFEST (sha256 of every other file) into out_dir.
#
# Runs in a CI job with no secrets: it downloads a CLI, npm packages and
# upstream data. Every download is pinned (checksum, lockfile or commit).
set -euo pipefail

readonly PMTILES_VERSION="1.31.2"
readonly PMTILES_TARBALL="go-pmtiles_${PMTILES_VERSION}_Linux_x86_64.tar.gz"
readonly PMTILES_SHA256="3ed7dbf4ec2e6dfe5e25b6f70d1ffc932729f93c86db353bf514dd71010a312f"
readonly BASEMAPS_ASSETS_REPO="https://github.com/protomaps/basemaps-assets.git"
readonly BASEMAPS_ASSETS_SHA="028c18f713baecad011301ff7a69acc39bcc2ae7"
readonly BBOX="-78.60,17.55,-75.95,18.70"
readonly MAXZOOM="15"
readonly FONTSTACKS=("Noto Sans Regular" "Noto Sans Medium" "Noto Sans Italic")
readonly FLAVORS=("light" "dark")
readonly BASE_URL_RE='^https://storage\.googleapis\.com/[a-z0-9][a-z0-9._-]{1,61}[a-z0-9]/v[0-9]{8}$'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR

die() {
  echo "build_map_tiles: $*" >&2
  exit 1
}

usage() {
  echo "usage: $0 [--dry-run] <version> <base_url> <out_dir>" >&2
  exit 2
}

dry_run=false
if [[ "${1:-}" == "--dry-run" ]]; then
  dry_run=true
  shift
fi
[[ $# -eq 3 ]] || usage
version="$1"
base_url="$2"
out_dir="$3"

[[ "$version" =~ ^[0-9]{8}$ ]] || die "version must be 8 digits (YYYYMMDD), got '$version'"
[[ "$base_url" =~ $BASE_URL_RE ]] || die "base_url is not a tile folder URL: '$base_url'"
[[ "$base_url" == */v"$version" ]] || die "base_url must end in /v$version"
[[ -d "$out_dir" ]] || die "out_dir does not exist: '$out_dir'"
[[ -z "$(ls -A "$out_dir")" ]] || die "out_dir is not empty: '$out_dir'"

source_url="https://build.protomaps.com/${version}.pmtiles"

if $dry_run; then
  echo "dry run: would build v$version"
  echo "  source:   $source_url (bbox $BBOX, maxzoom $MAXZOOM)"
  echo "  pmtiles:  $PMTILES_VERSION ($PMTILES_SHA256)"
  echo "  assets:   $BASEMAPS_ASSETS_REPO @ $BASEMAPS_ASSETS_SHA"
  echo "  base_url: $base_url"
  echo "  out_dir:  $out_dir"
  exit 0
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "::group::pmtiles CLI $PMTILES_VERSION"
curl -fsSL --retry 3 -o "$work/$PMTILES_TARBALL" \
  "https://github.com/protomaps/go-pmtiles/releases/download/v${PMTILES_VERSION}/${PMTILES_TARBALL}"
echo "$PMTILES_SHA256  $work/$PMTILES_TARBALL" | sha256sum -c -
mkdir "$work/bin"
tar -xzf "$work/$PMTILES_TARBALL" -C "$work/bin" pmtiles
pmtiles="$work/bin/pmtiles"
"$pmtiles" version
echo "::endgroup::"

echo "::group::npm ci (scripts/map-tiles)"
npm ci --prefix "$SCRIPT_DIR/map-tiles" --ignore-scripts --no-audit --no-fund
echo "::endgroup::"

echo "::group::basemaps-assets @ $BASEMAPS_ASSETS_SHA"
git init -q "$work/assets"
git -C "$work/assets" fetch -q --depth 1 "$BASEMAPS_ASSETS_REPO" "$BASEMAPS_ASSETS_SHA"
git -C "$work/assets" checkout -q FETCH_HEAD
[[ "$(git -C "$work/assets" rev-parse HEAD)" == "$BASEMAPS_ASSETS_SHA" ]] || die "basemaps-assets checkout is not $BASEMAPS_ASSETS_SHA"
echo "::endgroup::"

echo "::group::extract Jamaica from $source_url"
"$pmtiles" extract "$source_url" "$out_dir/jamaica.pmtiles" --bbox="$BBOX" --maxzoom="$MAXZOOM"
"$pmtiles" verify "$out_dir/jamaica.pmtiles"
echo "::endgroup::"

echo "::group::fonts and sprites"
mkdir -p "$out_dir/fonts" "$out_dir/sprites/v4"
for stack in "${FONTSTACKS[@]}"; do
  [[ -d "$work/assets/fonts/$stack" ]] || die "font stack missing upstream: $stack"
  cp -R "$work/assets/fonts/$stack" "$out_dir/fonts/"
done
cp "$work/assets/fonts/OFL.txt" "$out_dir/fonts/"
for flavor in "${FLAVORS[@]}"; do
  for suffix in "" "@2x"; do
    for ext in json png; do
      cp "$work/assets/sprites/v4/${flavor}${suffix}.${ext}" "$out_dir/sprites/v4/"
    done
  done
done
echo "::endgroup::"

echo "::group::styles"
node "$SCRIPT_DIR/map-tiles/make_styles.mjs" "$base_url" "$out_dir" "${FONTSTACKS[@]}"
if grep -q '"attribution"' "$out_dir"/style-*.json; then
  die "a style carries an attribution key"
fi
echo "::endgroup::"

basemaps_version="$(node -p "require('$SCRIPT_DIR/map-tiles/node_modules/@protomaps/basemaps/package.json').version")"
jq -n \
  --arg version "$version" \
  --arg source "$source_url" \
  --arg pmtiles "$PMTILES_VERSION" \
  --arg basemaps "$basemaps_version" \
  --arg assets "$BASEMAPS_ASSETS_SHA" \
  --arg bbox "$BBOX" \
  --arg maxzoom "$MAXZOOM" \
  '{version: $version, source: $source, bbox: $bbox, maxzoom: ($maxzoom | tonumber),
    tools: {pmtiles: $pmtiles, "protomaps-basemaps": $basemaps, "basemaps-assets": $assets}}' \
  > "$out_dir/BUILD_INFO"

# MANIFEST is in `sha256sum` format with paths relative to out_dir, so
# `sha256sum -c MANIFEST` run there verifies the folder.
(
  cd "$out_dir"
  find . -type f -printf '%P\0' | LC_ALL=C sort -z | xargs -0 sha256sum > "$work/MANIFEST"
  mv "$work/MANIFEST" MANIFEST
  sha256sum --quiet -c MANIFEST
)

echo "built v$version in $out_dir: $(wc -l < "$out_dir/MANIFEST") files, $(du -sh "$out_dir" | cut -f1)"
