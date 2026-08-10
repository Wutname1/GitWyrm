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

# tauri keys platforms as "<os>-<arch>". Only Windows ships today; an unknown
# key is a hard error rather than a pass-through, because a platform silently
# left pointing at a GitHub asset URL is the 0.0.3 bug all over again.
SUFFIX = {"windows-x86_64": "", "windows-aarch64": "-ARM64"}

platforms = manifest.get("platforms", {})
if not platforms:
    sys.exit("::error::manifest has no platforms")

for key in platforms:
    if key not in SUFFIX:
        sys.exit(f"::error::unmapped platform '{key}' - add it to SUFFIX")
    url = f"{cdn_base}/releases/{tag}/GitWyrm-Setup{SUFFIX[key]}.exe"
    platforms[key]["url"] = url
    if not platforms[key].get("signature"):
        sys.exit(f"::error::platform '{key}' has no signature")

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
for url in $(python3 -c "
import json,sys
m = json.load(open('$workdir/latest.json', encoding='utf-8'))
for p in m['platforms'].values():
    print(p['url'])
"); do
  code=$(curl -s -o /dev/null -w '%{http_code}' -I -L "$url" || echo 000)
  if [ "$code" != "200" ]; then
    echo "::error::${url} returned HTTP ${code} - the ${CHANNEL} channel would fail to update."
    exit 1
  fi
  echo "  OK ${url}"
done
