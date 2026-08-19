#!/usr/bin/env bash
# Render a human-readable CHANGELOG.md from the structured changelog payload.
#
# Reads the payload from stdin if given, otherwise generates it by calling
# changelog-items.sh. Writes to $1 (default CHANGELOG.md) and echoes the path.
#
# Split out of upload-changelog.sh so the markdown is produced even when the
# website upload is skipped (no CHANGELOG_API_KEY) - the GitHub Release body
# needs it regardless of whether gitwyrm.com got the structured copy.
#
# Within each section, items are grouped by platform. The platform comes from
# the OS tags the generator normalized (windows / linux / macos); anything with
# no OS tag is shared and lands under "All platforms". A platform sub-heading is
# only printed when the release actually mixes platforms - a Windows-only or
# all-shared release reads as a plain list, the way it always has.
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
  # An item's OS tags are dropped from its printed tag list: they are already
  # said by the sub-heading it sits under, so repeating them is noise.
  MIXED="$(jq -r '
    [.items[] | ([.tags[] | select(. == "windows" or . == "linux" or . == "macos")][0] // "all")]
    | unique | length > 1' <<<"$PAYLOAD")"

  for section in breaking feature fix change docs; do
    label="$(jq -r --arg s "$section" '
      {breaking:"### Breaking Changes",feature:"### New",fix:"### Fixed",
       change:"### Improved",docs:"### Documentation"}[$s]' <<<'{}')"

    # Nothing in this section? skip it entirely.
    [ "$(jq --arg s "$section" '[.items[]|select(.section==$s)]|length' <<<"$PAYLOAD")" = 0 ] && continue

    echo "$label"
    echo

    for platform in all windows linux macos; do
      rows="$(jq -r --arg s "$section" --arg p "$platform" '
        .items[]
        | select(.section == $s)
        | ([.tags[] | select(. == "windows" or . == "linux" or . == "macos")][0] // "all") as $itemp
        | select($itemp == $p)
        | ([.tags[] | select(. != "windows" and . != "linux" and . != "macos")]) as $rest
        | "- " + .text + (if ($rest|length) > 0 then " (" + ($rest|join(", ")) + ")" else "" end)
      ' <<<"$PAYLOAD")"
      [ -n "$rows" ] || continue

      if [ "$MIXED" = true ]; then
        heading="$(jq -r --arg p "$platform" '
          {all:"All platforms",windows:"Windows",linux:"Linux",macos:"macOS"}[$p]' <<<'{}')"
        echo "#### $heading"
        echo
      fi
      echo "$rows"
      echo
    done
  done
} >"$OUT"

echo "$OUT"
