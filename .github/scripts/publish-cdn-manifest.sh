#!/usr/bin/env bash
# Publish an updater manifest for one channel to the CDN.
#
# The app reads https://cdn.gitwyrm.com/updates/<channel>.json. That is a plain
# R2 object served straight through Cloudflare with no Worker in the request
# path, so an update check costs one cached GET and zero compute no matter how
# many people are running GitWyrm. Anything that made the endpoint dynamic --
# a Worker branching on a header, a redirect, a KV lookup -- would turn every
# client's 2-hourly poll into a billable invocation, which is exactly what this
# layout avoids.
#
# The manifest is derived from the one tauri-action already produced for the
# GitHub release, so the signatures are the real ones minisign generated at
# build time. Only the download URLs are rewritten, to point at the immutable
# per-version CDN copies the release job uploaded.
#
# Usage: publish-cdn-manifest.sh <channel> <tag>
#   channel  stable | beta
#   tag      the release tag the manifest describes (e.g. 0.6.1, 0.6.1-beta.2)
#
# Env: GH_REPO, GH_TOKEN, R2_ENDPOINT, AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY
set -euo pipefail

CHANNEL="${1:?usage: publish-cdn-manifest.sh <channel> <tag>}"
TAG="${2:?usage: publish-cdn-manifest.sh <channel> <tag>}"
REPO="${GH_REPO:?GH_REPO must be set}"
R2_ENDPOINT="${R2_ENDPOINT:?R2_ENDPOINT must be set}"
CDN_BASE="${CDN_BASE:-https://cdn.gitwyrm.com}"
BUCKET="${R2_BUCKET:-gitwyrm-cdn}"

case "$CHANNEL" in
  stable | beta) ;;
  *)
    echo "::error::Unknown channel '$CHANNEL' (expected stable or beta)"
    exit 1
    ;;
esac

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

# The signed manifest from the build. Signatures cannot be regenerated here --
# the private key is only in the build job -- so this is copied, never rebuilt.
gh release download "$TAG" --repo "$REPO" --pattern latest.json --dir "$workdir"

# Rewrite each platform's URL to the immutable CDN copy of that arch's installer.
# Python rather than jq because the platform keys carry an arch we have to map,
# and a silent mismatch here ships a manifest that downloads the wrong binary.
python3 - "$workdir/latest.json" "$CDN_BASE" "$TAG" <<'PY'
import json, sys

path, cdn_base, tag = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, encoding="utf-8") as fh:
    manifest = json.load(fh)

# tauri keys platforms as "<os>-<arch>", optionally with the bundle type
# appended: it emits BOTH "windows-x86_64" and "windows-x86_64-nsis" for the
# same installer, same signature. Older updater clients look up the bare key and
# newer ones prefer the qualified one, so both have to be rewritten -- dropping
# either would leave half the clients pointed at a GitHub asset URL.
#
# Windows keys are matched with an optional installer suffix; Linux keys map
# each package type explicitly. An unrecognised platform is still a hard error rather than a
# pass-through, because a key silently left on its GitHub URL is the 0.0.3 bug
# all over again.
ARTIFACTS = {
    "windows-x86_64": "GitWyrm-Setup.exe",
    "windows-aarch64": "GitWyrm-Setup-ARM64.exe",
    "linux-x86_64-appimage": "GitWyrm-x86_64.AppImage",
    "linux-x86_64-deb": "GitWyrm-amd64.deb",
    "linux-x86_64-rpm": "GitWyrm-x86_64.rpm",
    # Older updater clients and unqualified Linux installs use the AppImage.
    "linux-x86_64": "GitWyrm-x86_64.AppImage",
}
# Bundle types tauri may append. Anything else is unknown and must not be
# guessed at: a new bundle type could well need a different installer name.
BUNDLES = ("nsis", "msi")


def arch_of(key):
    """The artifact mapping key for this updater platform, if supported."""
    if key in ARTIFACTS:
        return key
    base, _, bundle = key.rpartition("-")
    if bundle in BUNDLES and base in ARTIFACTS:
        return base
    return None


platforms = manifest.get("platforms", {})
if not platforms:
    sys.exit("::error::manifest has no platforms")

for key in platforms:
    arch = arch_of(key)
    if arch is None:
        sys.exit(f"::error::unmapped platform '{key}' - add it to ARTIFACTS")
    url = f"{cdn_base}/releases/{tag}/{ARTIFACTS[arch]}"
    platforms[key]["url"] = url
    if not platforms[key].get("signature"):
        sys.exit(f"::error::platform '{key}' has no signature")

# Every arch we ship must be reachable under some key. A manifest that lost one
# entirely would publish and quietly strand that architecture on the old
# version, which no per-key check above can catch.
covered = {arch_of(k) for k in platforms}
required = {
    "windows-x86_64",
    "windows-aarch64",
    "linux-x86_64-appimage",
    "linux-x86_64-deb",
    "linux-x86_64-rpm",
}
missing = sorted(required - covered)
if missing:
    sys.exit(f"::error::manifest is missing {', '.join(missing)}")

# The updater compares this against the running build, so it must be the real
# version and not the tag with decoration. They are the same string today; this
# asserts it rather than assuming.
if manifest.get("version") != tag:
    print(f"::warning::manifest version {manifest.get('version')!r} != tag {tag!r}; using tag")
    manifest["version"] = tag

with open(path, "w", encoding="utf-8") as fh:
    json.dump(manifest, fh, indent=2)

print(f"Manifest for {tag}:")
for key, val in platforms.items():
    print(f"  {key} -> {val['url']}")
PY

# no-store: this object is the pointer clients poll. Cached at the edge it would
# keep handing out the previous version until the TTL lapsed, which on a hotfix
# is the difference between minutes and hours.
aws s3 cp "$workdir/latest.json" \
  "s3://${BUCKET}/updates/${CHANNEL}.json" \
  --endpoint-url "$R2_ENDPOINT" \
  --content-type "application/json" \
  --cache-control "no-cache, no-store, must-revalidate"

echo "Published ${CHANNEL}.json -> ${TAG}"

# A manifest whose installer 404s bricks the update path, and it is cheap to
# rule out here rather than discover from a user report.
# Deduplicated: tauri lists each installer under both a bare and a
# bundle-qualified key, so the raw list would check every URL twice.
for url in $(python3 -c "
import json,sys
m = json.load(open('$workdir/latest.json', encoding='utf-8'))
for u in sorted({p['url'] for p in m['platforms'].values()}):
    print(u)
"); do
  code=$(curl -s -o /dev/null -w '%{http_code}' -I -L "$url" || echo 000)
  if [ "$code" != "200" ]; then
    echo "::error::${url} returned HTTP ${code} - the ${CHANNEL} channel would fail to update."
    exit 1
  fi
  echo "  OK ${url}"
done
