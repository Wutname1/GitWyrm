#!/usr/bin/env bash
# Emit a structured changelog for one release as JSON on stdout:
#   { product, version, released_at, download_url, items:[{section,text,tags[]}] }
#
# Each commit subject in the release range becomes one item. The section is
# derived from the commit-prefix convention (new:/fixes:/improved:). Tags carry
# ONLY what the author wrote explicitly as [tag] / #tag markers - all
# content-based auto-tagging is done server-side on the website so the rules
# live in one place. Markers are read from BOTH the subject and the body, so a
# user-facing subject can stay clean while the body carries the tags. The verb
# prefix and any trailing subject markers are stripped from the displayed text.
#
# Platform tags (windows/linux/macos and their aliases) are normalized to a
# canonical slug and kept in the same tags array - they are what lets a reader
# tell an OS-specific change from a shared one. render-changelog.sh reads them
# back out to group each section into Windows / Linux / macOS / All platforms.
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

# Betas are built from untagged commits on main, so there is no tag to describe
# from and THIS_REF would be a branch name. Their range runs from the newest
# STABLE tag to HEAD, which is exactly the set of commits the eventual release
# will also cover -- a beta tester sees the same notes early, and the release
# entry that replaces them is not missing anything.
#
# `--list '[0-9]*'` plus the anchored grep keeps this to real release tags even
# if beta tags are ever introduced; picking a beta tag as the floor would make
# the following release's changelog nearly empty.
if [ -n "${BETA_VERSION:-}" ]; then
  VERSION="$BETA_VERSION"
  PREV_TAG="$(git tag --list '[0-9]*.[0-9]*.[0-9]*' \
    | grep -E '^[0-9]+\.[0-9]+\.[0-9]+$' \
    | sort -V | tail -n1 || true)"
  THIS_REF="HEAD"
else
  PREV_TAG="$(git describe --tags --abbrev=0 "${THIS_REF}^" 2>/dev/null || echo '')"
fi

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

# --- platform: canonical slug for an OS tag, empty for anything else ----------
# Authors write whichever spelling is natural (#win, #macOS, #linux); the
# changelog needs one slug per OS so readers can tell an OS-specific change from
# a shared one, and so render-changelog.sh can group by it.
canonical_platform() {
  case "$1" in
    windows | win | win32 | win64 | msi | nsis) echo windows ;;
    linux | deb | rpm | appimage | apt | flatpak) echo linux ;;
    macos | mac | osx | darwin | dmg) echo macos ;;
    *) echo '' ;;
  esac
}

# --- tags: EXPLICIT markers only ([tag] / #tag); one slug per line -------------
# Content-based tagging is the website's job. Here we only pass through what the
# author deliberately marked, in either the subject or the body.
#
# Body markers exist so the subject can stay clean user-facing prose while the
# commit still carries its tags - that is the documented convention in AGENTS.md.
# A whole line of nothing but tags is the normal way to write them, so lines are
# never filtered wholesale; the marker pattern itself does the work:
#   - a marker must contain a letter, so issue refs and prose numbers
#     ("Rule #2", "closes #481") never become tags
#   - a `#tag` must start a word, so `#[cfg(...)]` and mid-word hashes are out
#   - lines opening a comment are skipped, so a pasted `// #foo`, `-- #foo` or
#     `# note #foo` snippet does not mint a tag. A line that STARTS with a tag
#     (`#linux`, no space after the hash) is not a comment and is kept - that
#     is the normal way to write a tag-only line.
extract_tags() {
  local m="$1" raw plat
  printf '%s\n' "$m" \
    | grep -vE '^[[:space:]]*(//|--|#[[:space:]]|#$)' \
    | grep -oE '(\[[a-zA-Z0-9_/-]*[a-zA-Z][a-zA-Z0-9_/-]*\]|(^|[[:space:]])#[a-zA-Z0-9_/-]*[a-zA-Z][a-zA-Z0-9_/-]*)' \
    | tr -d '[]#' \
    | tr '[:upper:]' '[:lower:]' \
    | sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//' \
    | while IFS= read -r raw; do
        [ -z "$raw" ] && continue
        plat="$(canonical_platform "$raw")"
        printf '%s\n' "${plat:-$raw}"
      done \
    | sort -u | grep -v '^$' || true
}

# --- build the items JSON array -----------------------------------------------
ITEMS='[]'
# Each commit is read as a NUL-terminated record of "subject\nbody", so a body
# containing blank lines cannot be mistaken for a record boundary. Tags are
# scanned over the whole record; only the subject becomes display text.
while IFS= read -r -d '' record; do
  subject="${record%%$'\n'*}"
  [ -z "$subject" ] && continue
  section="$(categorize "$subject")"
  # Only commits that match a known prefix/verb get a changelog entry. Anything
  # that falls through to "other" (e.g. "Script update", "CICD Updates",
  # "Dependency bump") is intentionally skipped so it doesn't pollute the log.
  [ "$section" = other ] && continue
  text="$(clean_subject "$subject")"
  [ -z "$text" ] && continue
  tags_json="$(extract_tags "$record" | jq -R . | jq -sc .)"
  ITEMS="$(jq -c \
    --arg section "$section" --arg text "$text" --argjson tags "$tags_json" \
    '. += [{section:$section, text:$text, tags:$tags}]' <<<"$ITEMS")"
# NOTE: -z makes git NUL-terminate every record including the last, so no commit
# is dropped. (The earlier newline-delimited form had to avoid `--pretty=format:`
# for the same reason: it omits the final terminator and `read` then returns
# non-zero, silently losing the OLDEST commit of every range - which on a
# single-commit release meant zero items and an API rejection.)
done < <(git log "$RANGE" --no-merges -z --pretty='%s%n%b')

jq -n \
  --arg product "$PRODUCT" \
  --arg version "$VERSION" \
  --arg released_at "$RELEASED_AT" \
  --arg download_url "$DOWNLOAD_URL" \
  --argjson items "$ITEMS" \
  '{product:$product, version:$version, released_at:$released_at,
    download_url:(if $download_url=="" then null else $download_url end),
    items:$items}'
