#!/usr/bin/env bash
# Emit a structured changelog for one release as JSON on stdout:
#   { product, version, released_at, download_url, items:[{section,text,tags[]}] }
#
# Each commit subject in the release range becomes one item. The section is
# derived from the commit-prefix convention (new:/fixes:/improved:). Tags carry
# ONLY what the author wrote explicitly as trailing [tag] / #tag markers - all
# content-based auto-tagging is done server-side on the website so the rules
# live in one place. The verb prefix and the trailing tag markers are stripped
# from the displayed text.
#
# Env:
#   PRODUCT          product key stored on the website (default GitWyrm)
#   RELEASE_VERSION  version string (preferred; the workflow computes it once so
#                    the built app and the changelog agree). Falls back to
#                    GITHUB_REF_NAME with a leading v stripped.
#   DOWNLOAD_URL     optional download link
#   GITHUB_REF_NAME  tag name on a tag push (e.g. 1.2.3)
set -euo pipefail

PRODUCT="${PRODUCT:-GitWyrm}"
REF_NAME="${GITHUB_REF_NAME:-$(git describe --tags --abbrev=0 2>/dev/null || echo '')}"
VERSION="${RELEASE_VERSION:-${REF_NAME#v}}"
DOWNLOAD_URL="${DOWNLOAD_URL:-}"
RELEASED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# --- Commit range: prev tag .. this tag; first-ever tag => whole history ------
# On the very first release there is no previous tag, so a bare ref walks all
# reachable commits - which is exactly the full-history changelog we want for
# the initial release. Every later tag has a predecessor and gets a scoped diff.
THIS_REF="${REF_NAME:-HEAD}"
PREV_TAG="$(git describe --tags --abbrev=0 "${THIS_REF}^" 2>/dev/null || echo '')"
if [ -n "$PREV_TAG" ]; then
  RANGE="${PREV_TAG}..${THIS_REF}"
else
  RANGE="$THIS_REF"
fi

# --- section: map a commit subject to a canonical section slug ----------------
categorize() {
  local m="$1" low
  low="$(printf '%s' "$m" | tr '[:upper:]' '[:lower:]')"
  case "$m" in
    breaking:* | BREAKING:*) echo breaking; return ;;
  esac
  case "$low" in
    docs:* | documentation:*) echo docs; return ;;
    feat:* | feature:* | new:* | enhancement:*) echo feature; return ;;
    fix:* | fixes:* | bug:* | bugfix:*) echo fix; return ;;
    chore:* | refactor:* | style:* | perf:* | improved:*) echo change; return ;;
  esac
  # natural-language leading verbs
  if [[ "$low" =~ ^(fix|fixes|fixed|fixing|silence|suppress|protect|protects|ensure|ensures|guard|guards)([[:space:]:-]) ]]; then echo fix; return; fi
  if [[ "$low" =~ ^(add|adds|added|adding|new|implement|implements|implemented|integrate|integrates|integrated|register|registers|monitor|track|respect)([[:space:]:-]) ]]; then echo feature; return; fi
  if [[ "$low" =~ ^(improve|improves|improved|enhance|enhances|update|updates|refactor|refactors|cleanup|reorganize|simplify|migrate|migrates|move|moves|moved|remove|removes|removed|delete|deletes|switch|replace|replaces|bundle)([[:space:]:-]) ]]; then echo change; return; fi
  echo other
}

# --- clean: strip leading verb/prefix + trailing tag markers ------------------
clean_subject() {
  local m="$1"
  m="$(printf '%s' "$m" | sed -E 's/^(feat|feature|fix|fixes|bug|bugfix|chore|refactor|style|perf|docs|documentation|breaking|new|enhancement|improved):[[:space:]]+//I')"
  m="$(printf '%s' "$m" | sed -E 's/^(Fixes|Fixed|Fixing|Fix|Adds|Added|Adding|Add|Implements|Implemented|Implement|Improves|Improved|Improving|Improve|Enhances|Enhanced|Enhance|Updates|Updated|Update|Refactors|Refactored|Refactor|Cleanup|Removes|Removed|Remove|Deletes|Deleted|Delete|Integrates|Integrated|Integrate|Registers|Registered|Register|Silence|Suppress|Protects|Protect|Ensures|Ensure|Guards|Guard|Switches|Switch|Replaces|Replace|Moves|Moved|Move|Migrates|Migrated|Migrate)[[:space:]:-]+//I')"
  # Drop trailing explicit tag markers from the display text. A marker must
  # contain a letter: bare numbers are prose or issue refs ("Rule #2",
  # "closes #481"), not tag slugs, and stripping them mangles the sentence.
  m="$(printf '%s' "$m" | sed -E 's/[[:space:]]*(\[[a-zA-Z0-9_/-]*[a-zA-Z][a-zA-Z0-9_/-]*\]|#[a-zA-Z0-9_/-]*[a-zA-Z][a-zA-Z0-9_/-]*)+[[:space:]]*$//')"
  m="$(printf '%s' "$m" | sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//')"
  # capitalize the first letter
  printf '%s' "$m" | sed -E 's/^(.)/\U\1/'
}

# --- tags: EXPLICIT markers only ([tag] / #tag); one slug per line -------------
# Content-based tagging is the website's job. Here we only pass through what the
# author deliberately marked.
# A marker must contain at least one letter, so issue refs and prose numbers
# ("Rule #2", "closes #481") never become tags.
extract_tags() {
  local m="$1"
  printf '%s\n' "$m" \
    | grep -oE '(\[[a-zA-Z0-9_/-]*[a-zA-Z][a-zA-Z0-9_/-]*\]|#[a-zA-Z0-9_/-]*[a-zA-Z][a-zA-Z0-9_/-]*)' \
    | tr -d '[]#' \
    | tr '[:upper:]' '[:lower:]' \
    | sort -u | grep -v '^$' || true
}

# --- build the items JSON array -----------------------------------------------
ITEMS='[]'
# `|| [ -n "$subject" ]` keeps the final line even if it arrives unterminated,
# so a future change back to a `format:`-style pretty can't silently re-drop it.
while IFS= read -r subject || [ -n "$subject" ]; do
  [ -z "$subject" ] && continue
  section="$(categorize "$subject")"
  # Only commits that match a known prefix/verb get a changelog entry. Anything
  # that falls through to "other" (e.g. "Script update", "CICD Updates",
  # "Dependency bump") is intentionally skipped so it doesn't pollute the log.
  [ "$section" = other ] && continue
  text="$(clean_subject "$subject")"
  [ -z "$text" ] && continue
  tags_json="$(extract_tags "$subject" | jq -R . | jq -sc .)"
  ITEMS="$(jq -c \
    --arg section "$section" --arg text "$text" --argjson tags "$tags_json" \
    '. += [{section:$section, text:$text, tags:$tags}]' <<<"$ITEMS")"
# NOTE: --pretty='%s', NOT --pretty=format:'%s'. The `format:` variant omits the
# trailing newline on the last line, and `read` returns non-zero on an
# unterminated final line - so bash silently drops the OLDEST commit of every
# range. With a single-commit release that means zero items, which the API
# rejects ("Missing required fields: ... items[]"); with more it just quietly
# lost one entry per release. `tformat:` (the --pretty= default) terminates
# every line.
done < <(git log "$RANGE" --no-merges --pretty='%s')

jq -n \
  --arg product "$PRODUCT" \
  --arg version "$VERSION" \
  --arg released_at "$RELEASED_AT" \
  --arg download_url "$DOWNLOAD_URL" \
  --argjson items "$ITEMS" \
  '{product:$product, version:$version, released_at:$released_at,
    download_url:(if $download_url=="" then null else $download_url end),
    items:$items}'
