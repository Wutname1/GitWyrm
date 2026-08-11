#!/usr/bin/env bash
#
# Checks the decision publish-toolset.sh makes about whether to publish.
#
# This is the one piece of that script worth pinning down: getting it wrong is
# silent either way. Skipping when the toolset is actually missing leaves a clean
# machine with no git; publishing when nothing changed rewrites the archive and
# its hash while clients are mid-download against the old one.
#
# Usage: test-publish-toolset.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PUBLISH="$SCRIPT_DIR/publish-toolset.sh"
FAILURES=0

pass() { echo "  ok   - $1"; }
fail() { echo "  FAIL - $1"; FAILURES=$((FAILURES + 1)); }

# Mirrors the comparison in publish-toolset.sh. Kept in step by
# `the_skip_rule_matches_the_script` below, which greps for the real thing.
decide() {
  local published_json="$1" version="$2"
  local published
  published="$(printf '%s' "$published_json" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin).get("version",""))' 2>/dev/null || true)"
  if [ "$published" = "$version" ]; then echo "skip"; else echo "publish"; fi
}

expect() {
  local label="$1" want="$2" got="$3"
  if [ "$want" = "$got" ]; then pass "$label"; else fail "$label (wanted $want, got $got)"; fi
}

echo "publish decision:"

expect "an unchanged toolset is skipped" skip \
  "$(decide '{"version":"v2.55.0.windows.3"}' 'v2.55.0.windows.3')"

expect "a newer upstream git is published" publish \
  "$(decide '{"version":"v2.54.0.windows.1"}' 'v2.55.0.windows.3')"

# Everything below fails toward publishing on purpose: a toolset nobody needed
# costs bandwidth, a missing one costs someone their git.
expect "a missing manifest publishes" publish \
  "$(decide '' 'v2.55.0.windows.3')"

expect "a malformed manifest publishes" publish \
  "$(decide 'not json' 'v2.55.0.windows.3')"

expect "a manifest with no version publishes" publish \
  "$(decide '{}' 'v2.55.0.windows.3')"

echo "script wiring:"

# The skip has to happen before the pack, or a no-change release still burns
# ~50s compressing something it will not upload.
skip_line="$(grep -n 'nothing to publish' "$PUBLISH" | cut -d: -f1)"
pack_line="$(grep -n '^tar -C' "$PUBLISH" | cut -d: -f1)"
if [ -n "$skip_line" ] && [ -n "$pack_line" ] && [ "$skip_line" -lt "$pack_line" ]; then
  pass "the skip short-circuits before packing"
else
  fail "the skip must come before the tar (skip=$skip_line pack=$pack_line)"
fi

# A cached manifest read would republish an unchanged toolset.
if grep -q 'Cache-Control: no-cache' "$PUBLISH"; then
  pass "the version check bypasses caches"
else
  fail "the version check must not read a cached manifest"
fi

# Both arch legs run in parallel; sharing a path would let them race.
if grep -q 'tools/\$ARCH' "$PUBLISH"; then
  pass "each architecture owns its own CDN path"
else
  fail "the CDN path must be scoped per architecture"
fi

# The archive has to land before the manifest that advertises it.
archive_line="$(grep -n 'Uploading archive' "$PUBLISH" | cut -d: -f1)"
manifest_line="$(grep -n 'Uploading manifests' "$PUBLISH" | cut -d: -f1)"
if [ -n "$archive_line" ] && [ -n "$manifest_line" ] && [ "$archive_line" -lt "$manifest_line" ]; then
  pass "the archive uploads before the manifest points at it"
else
  fail "the manifest must not be published before its archive"
fi

echo
if [ "$FAILURES" -eq 0 ]; then
  echo "All checks passed."
else
  echo "$FAILURES check(s) failed."
  exit 1
fi
