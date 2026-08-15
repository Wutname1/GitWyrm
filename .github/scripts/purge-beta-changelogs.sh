#!/usr/bin/env bash
# Delete every beta changelog entry for the product from gitwyrm.com.
#
# Run after a stable release uploads its own entry. Beta entries exist so a
# tester can read what changed before the release ships; once it has, the
# release entry covers the same commits -- its range runs from the previous
# STABLE tag, so nothing a beta described is lost -- and leaving the betas would
# stack up rows nobody can install.
#
# Ordered after the stable upload deliberately: purging first would leave a
# window with no notes at all if the upload then failed.
#
# Env:
#   CHANGELOG_API_KEY  Bearer token; must match the website binding (required)
#   CHANGELOG_API_URL  Base changelogs endpoint (default gitwyrm.com)
#   PRODUCT            product key (default GitWyrm)
set -euo pipefail

API_URL="${CHANGELOG_API_URL:-https://gitwyrm.com/api/v1/changelogs}"
PRODUCT="${PRODUCT:-GitWyrm}"

if [ -z "${CHANGELOG_API_KEY:-}" ]; then
  echo "SKIP: CHANGELOG_API_KEY not set" >&2
  exit 0
fi

# `channel=beta` is the API's own prerelease filter, so the definition of "a
# beta" lives in one place rather than being re-implemented here with a
# different pattern that could drift out of step.
# limit=100 is the endpoint's ceiling (values above it are clamped). Betas
# between two releases never approach that, and anything beyond is picked up by
# the next release's purge rather than being silently missed.
LIST="$(curl -sS --connect-timeout 10 --max-time 30 \
  "${API_URL}?product=${PRODUCT}&channel=beta&limit=100")" || {
  echo "curl failed to list beta changelogs" >&2
  exit 1
}

IDS="$(jq -r '.entries[]?.id // empty' <<<"$LIST")"

if [ -z "$IDS" ]; then
  echo "No beta changelog entries to purge"
  exit 0
fi

FAILED=0
while IFS= read -r id; do
  [ -z "$id" ] && continue
  version="$(jq -r --arg id "$id" '.entries[] | select(.id == $id) | .version' <<<"$LIST")"

  CODE="$(curl -sS -o /tmp/purge-resp.json -w '%{http_code}' \
    --connect-timeout 10 --max-time 30 -X DELETE "${API_URL}/${id}" \
    -H "Authorization: Bearer $CHANGELOG_API_KEY")" || {
    echo "curl failed deleting $version ($id)" >&2
    FAILED=1
    continue
  }

  # 404 means it is already gone, which is the state we wanted anyway.
  if [ "$CODE" = "200" ] || [ "$CODE" = "204" ] || [ "$CODE" = "404" ]; then
    echo "Purged $version ($CODE)"
  else
    echo "ERROR purging $version ($CODE): $(cat /tmp/purge-resp.json)" >&2
    FAILED=1
  fi
done <<<"$IDS"

# A failed purge must not fail the release: the stable entry is already live and
# the worst case is a stale beta row that the next release clears.
if [ "$FAILED" = "1" ]; then
  echo "::warning::Some beta changelog entries could not be purged" >&2
fi

exit 0
