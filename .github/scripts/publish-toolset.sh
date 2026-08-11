#!/usr/bin/env bash
#
# Packs the bundled git and gpg into a single archive and publishes it to the
# CDN, so the app can fetch them independently of an app release.
#
# Why they are not in the installer any more: the NSIS resources loop deletes
# and re-extracts every bundled file on *every* app update - 408 files, 103 MB -
# while Git for Windows only ships every month or so. Serving them separately
# means an app update stops touching them at all unless the version moved.
#
# Only the latest toolset is kept. There is deliberately no version history on
# the CDN: nothing references an older toolset, so keeping them would just
# accumulate.
#
# Layout published, per arch:
#   tools/<arch>/toolset.tar.xz   the tree, rooted at git/ and gpg/
#   tools/<arch>/toolset.json     version, size, sha256 of the archive above
#
# Usage: publish-toolset.sh <arch> <src> [--dry-run]
#   arch: x86_64 | aarch64
#   src:  directory holding git/ and gpg/ (what fetch-bundled-tools.sh filled)

set -euo pipefail

ARCH="${1:?usage: publish-toolset.sh <x86_64|aarch64> <src> [--dry-run]}"
SRC="${2:?usage: publish-toolset.sh <x86_64|aarch64> <src> [--dry-run]}"
DRY_RUN="${3:-}"

case "$ARCH" in
  x86_64|aarch64) ;;
  *) echo "::error::Unknown arch '$ARCH' (expected x86_64 or aarch64)"; exit 1 ;;
esac

for required in "$SRC/git/cmd/git.exe" "$SRC/gpg/gpg.exe" "$SRC/BUNDLED-VERSION.txt"; do
  if [ ! -f "$required" ]; then
    echo "::error::Expected $required before publishing - run fetch-bundled-tools.sh first"
    exit 1
  fi
done

# The upstream tag fetch-bundled-tools.sh recorded, e.g. "v2.55.0.windows.3".
# This is the version the app compares against to decide whether to re-download,
# so it must identify the *tools*, never the GitWyrm release they shipped beside.
VERSION="$(sed -n 's|.*/releases/tag/\(.*\)$|\1|p' "$SRC/BUNDLED-VERSION.txt" | tr -d '\r')"
if [ -z "$VERSION" ]; then
  echo "::error::Could not read a tag out of $SRC/BUNDLED-VERSION.txt"
  exit 1
fi
echo "Toolset version: $VERSION ($ARCH)"

BASE="s3://gitwyrm-cdn/tools/$ARCH"
PUBLIC="https://cdn.gitwyrm.com/tools/$ARCH"

# ------------------------------------------------------------ skip if unchanged
# The point of the exercise: a GitWyrm release whose tools did not move must not
# republish 29 MB, and more importantly must not change the hash clients have
# already cached. Compared by version rather than by hashing the tree, because
# the archive is not reproducible byte-for-byte (timestamps).
if [ "$DRY_RUN" != "--dry-run" ]; then
  # Cache-busted deliberately. The manifest is served no-store, but an edge or a
  # runner-side proxy can still hold a copy, and a stale read here is the one
  # that hurts: it would republish an unchanged toolset, changing the archive
  # bytes and its hash while clients are mid-download against the old one.
  PUBLISHED="$(curl -fsSL -H 'Cache-Control: no-cache' -H 'Pragma: no-cache' \
    "$PUBLIC/toolset.json?t=$(date +%s)" 2>/dev/null \
    | python3 -c 'import json,sys; print(json.load(sys.stdin).get("version",""))' 2>/dev/null || true)"

  # An unreadable manifest (404, malformed, offline) falls through to
  # publishing. That is the safe direction: shipping a toolset nobody needed
  # costs bandwidth, whereas skipping one that was actually missing leaves a
  # clean machine with no git.
  if [ "$PUBLISHED" = "$VERSION" ]; then
    echo "CDN already serves $VERSION for $ARCH - nothing to publish."
    exit 0
  fi
  echo "CDN has '${PUBLISHED:-nothing}', publishing $VERSION"
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Self-check before touching the CDN. Cheap, and the properties it guards
# (skip-before-pack, uncached version read, per-arch paths, archive-before-
# manifest) all fail silently in ways a green build would hide.
if [ -x "$(dirname "$0")/test-publish-toolset.sh" ]; then
  "$(dirname "$0")/test-publish-toolset.sh" >/dev/null || {
    echo "::error::publish-toolset self-check failed; refusing to publish"
    exit 1
  }
fi

# ---------------------------------------------------------------------- pack
# tar.xz, not zip: measured on the real tree, xz gets 103.5 MB down to 29.0 MB
# where zip manages only 46.7 MB. 7z solid is smaller still (21.9 MB) but needs
# an NSIS plugin the bundler does not ship and a heavier decoder in the app.
#
# Rooted so the archive expands to git/... and gpg/..., matching the layout
# src-tauri/src/git/bundled.rs looks for.
echo "Packing $SRC (git, gpg) ..."
tar -C "$SRC" -cf "$WORK/toolset.tar" git gpg
xz -9 -T0 -c "$WORK/toolset.tar" > "$WORK/toolset.tar.xz"

SIZE="$(stat -c %s "$WORK/toolset.tar.xz")"
SHA="$(sha256sum "$WORK/toolset.tar.xz" | cut -d' ' -f1)"
echo "Archive: $(( SIZE / 1024 / 1024 )) MB  sha256=$SHA"

# The manifest the app reads. `url` is absolute so a client needs to know only
# the manifest location, and `sha256` is what makes an untrusted download safe
# to unpack and execute.
python3 - "$WORK/toolset.json" "$VERSION" "$SIZE" "$SHA" "$PUBLIC/toolset.tar.xz" <<'PY'
import json, sys
out, version, size, sha, url = sys.argv[1:6]
json.dump(
    {"version": version, "size": int(size), "sha256": sha, "url": url},
    open(out, "w"),
    indent=2,
)
PY
cat "$WORK/toolset.json"

# The component list the bootstrapper reads. Separate from toolset.json because
# it answers a different question: that one describes *the* toolset, this one
# lists which tools exist at all.
#
# This is what keeps the bootstrapper stable. It is published under a filename
# that never changes and so is rebuilt far less often than the app; it knows
# only "fetch everything this manifest lists". Adding or removing a tool means
# editing the list below, and every bootstrapper already installed picks it up
# with no new binary.
#
# Only `name` and `url` are required. The optional fields, and what they let a
# future entry express without a new bootstrapper:
#
#   version  absent -> always reinstall (nothing to compare against)
#   sha256   absent -> installed unverified, and logged loudly
#   size     absent -> progress runs on Content-Length instead
#   kind     "archive" (default) unpacks a tar.xz; "installer" runs an exe
#   args     arguments for kind=installer, e.g. "/S" - the silent switch differs
#            per vendor, so it belongs here rather than in the binary
#   into     subdirectory to unpack an archive into; empty means already rooted
#
# An unknown kind is skipped rather than guessed at, so an older bootstrapper
# meeting a newer manifest leaves that component to the app instead of running
# something it does not understand.
python3 - "$WORK/components.json" "$VERSION" "$SIZE" "$SHA" "$PUBLIC/toolset.tar.xz" <<'PY'
import json, sys
out, version, size, sha, url = sys.argv[1:6]
# git and gpg travel in one archive today, so they are one component. Splitting
# them later is a change to this list, not to the bootstrapper.
json.dump(
    {
        "schema": 1,
        "components": [
            {
                "name": "toolset",
                "version": version,
                "url": url,
                "sha256": sha,
                "size": int(size),
            }
        ],
    },
    open(out, "w"),
    indent=2,
)
PY
cat "$WORK/components.json"

if [ "$DRY_RUN" = "--dry-run" ]; then
  echo "Dry run: not uploading."
  exit 0
fi

# -------------------------------------------------------------------- publish
# Archive first, manifest second, and never the other way round: the manifest is
# what points clients at the archive, so publishing it first would advertise
# bytes that are not there yet.
#
# The archive is immutable-cached but the manifest is not, because the manifest
# is the thing that has to change the moment a new toolset lands. The archive
# path is fixed rather than versioned (latest-only, by design), so it carries a
# short max-age instead of `immutable` - the URL's contents genuinely do change.
echo "Uploading archive..."
aws s3 cp "$WORK/toolset.tar.xz" "$BASE/toolset.tar.xz" \
  --endpoint-url "$R2_ENDPOINT" \
  --cache-control "public, max-age=300"

echo "Uploading manifests..."
for manifest in toolset.json components.json; do
  aws s3 cp "$WORK/$manifest" "$BASE/$manifest" \
    --endpoint-url "$R2_ENDPOINT" \
    --content-type "application/json" \
    --cache-control "no-cache, no-store"
done

echo "Published $VERSION to $PUBLIC/"
