#!/usr/bin/env bash
# Render a human-readable CHANGELOG.md from the structured changelog payload.
#
# Reads the payload from stdin if given, otherwise generates it by calling
# changelog-items.sh. Writes to $1 (default CHANGELOG.md) and echoes the path.
#
# Split out of upload-changelog.sh so the markdown is produced even when the
# website upload is skipped (no CHANGELOG_API_KEY) - the GitHub Release body
# needs it regardless of whether gitwyrm.com got the structured copy.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="${1:-CHANGELOG.md}"

if [ -t 0 ]; then
  PAYLOAD="$("$SCRIPT_DIR/changelog-items.sh")"
else
  PAYLOAD="$(cat)"
  [ -n "$PAYLOAD" ] || PAYLOAD="$("$SCRIPT_DIR/changelog-items.sh")"
fi

# Same section order and labels as the website.
{
  for section in breaking feature fix change docs; do
    label="$(jq -r --arg s "$section" '
      {breaking:"### Breaking Changes",feature:"### New",fix:"### Fixed",
       change:"### Improved",docs:"### Documentation"}[$s]' <<<'{}')"
    rows="$(jq -r --arg s "$section" '.items[]|select(.section==$s)|
      "- " + .text + (if (.tags|length)>0 then " (" + (.tags|join(", ")) + ")" else "" end)' \
      <<<"$PAYLOAD")"
    [ -n "$rows" ] && {
      echo "$label"
      echo
      echo "$rows"
      echo
    }
  done
} >"$OUT"

echo "$OUT"
