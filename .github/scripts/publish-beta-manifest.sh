#!/usr/bin/env bash
# Point the beta channel at a freshly built beta.
#
# Betas are not GitHub releases, so unlike the stable path there is no manifest
# to download and rewrite -- this builds one from the .sig files the build job
# uploaded alongside each installer.
#
# The manifest lives at https://cdn.gitwyrm.com/updates/beta.json: a plain R2
# object served through Cloudflare with no Worker in the request path, so the
# 2-hourly poll from every beta client is a cached GET costing no compute.
#
# Also refreshes the fixed-name installer testers use for a first install, so
# one link always fetches the current beta.
#
# Usage: publish-beta-manifest.sh <version>
# Env:   R2_ENDPOINT, AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY
set -euo pipefail

VERSION="${1:?usage: publish-beta-manifest.sh <version>}"
R2_ENDPOINT="${R2_ENDPOINT:?R2_ENDPOINT must be set}"
CDN_BASE="${CDN_BASE:-https://cdn.gitwyrm.com}"
BUCKET="${R2_BUCKET:-gitwyrm-cdn}"

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

# tauri platform key -> the filename suffix the build job used.
declare -A PLATFORMS=(
  ["windows-x86_64"]=""
  ["windows-aarch64"]="-ARM64"
)

# Every arch must be present before the pointer moves. Publishing a manifest
# that names an installer which is not there turns an update into a dead end for
# that architecture, and the updater has already uninstalled by then.
platforms_json="{}"
for key in "${!PLATFORMS[@]}"; do
  suffix="${PLATFORMS[$key]}"
  exe_key="betas/${VERSION}/GitWyrm-Setup${suffix}.exe"

  if ! aws s3 ls "s3://${BUCKET}/${exe_key}" --endpoint-url "$R2_ENDPOINT" >/dev/null 2>&1; then
    echo "::error::Missing ${exe_key} - refusing to move the beta pointer."
    exit 1
  fi


  # The signature is the whole trust story: the updater verifies the downloaded
  # installer against it with the public key compiled into the app, so a beta
  # without one would simply be rejected on every client.
  aws s3 cp "s3://${BUCKET}/${exe_key}.sig" "$workdir/sig${suffix}" \
    --endpoint-url "$R2_ENDPOINT" >/dev/null
  signature=$(tr -d '\r\n' < "$workdir/sig${suffix}")
  if [ -z "$signature" ]; then
    echo "::error::Empty signature for ${exe_key}"
    exit 1
  fi

  platforms_json=$(jq -c \
    --arg key "$key" \
    --arg sig "$signature" \
    --arg url "${CDN_BASE}/${exe_key}" \
    '. + {($key): {signature: $sig, url: $url}}' <<<"$platforms_json")
done

# Three keys, one per package format. The updater asks for
# `{os}-{arch}-{installer}` and only then falls back to `{os}-{arch}`, so a lone
# linux-x86_64 entry would hand the AppImage to someone running the .deb - and
# its install path shells out to dpkg, which cannot read an AppImage. The plain
# key is published too, as the fallback for any installer not named here.
#
# Filenames match the release workflow's "Upload versioned Linux packages" step.
declare -A LINUX_ARTIFACTS=(
  ["linux-x86_64-deb"]="GitWyrm-amd64.deb"
  ["linux-x86_64-appimage"]="GitWyrm-x86_64.AppImage"
  ["linux-x86_64"]="GitWyrm-x86_64.AppImage"
)

for key in "${!LINUX_ARTIFACTS[@]}"; do
  artifact="betas/${VERSION}/${LINUX_ARTIFACTS[$key]}"

  if ! aws s3 ls "s3://${BUCKET}/${artifact}" --endpoint-url "$R2_ENDPOINT" >/dev/null 2>&1; then
    echo "::error::Missing ${artifact} - refusing to move the beta pointer."
    exit 1
  fi

  if ! aws s3 cp "s3://${BUCKET}/${artifact}.sig" "$workdir/sig-${key}" \
       --endpoint-url "$R2_ENDPOINT" >/dev/null 2>&1; then
    echo "::error::Missing signature for ${artifact} - refusing to move the beta pointer."
    exit 1
  fi

  linux_sig=$(tr -d '\r\n' < "$workdir/sig-${key}")
  if [ -z "$linux_sig" ]; then
    echo "::error::Empty signature for ${artifact}"
    exit 1
  fi

  platforms_json=$(jq -c \
    --arg key "$key" \
    --arg sig "$linux_sig" \
    --arg url "${CDN_BASE}/${artifact}" \
    '. + {($key): {signature: $sig, url: $url}}' <<<"$platforms_json")
done

jq -n \
  --arg version "$VERSION" \
  --arg pub_date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --argjson platforms "$platforms_json" \
  '{version: $version, pub_date: $pub_date,
    notes: "Beta build. Report anything odd at https://github.com/Wutname1/GitWyrm/issues",
    platforms: $platforms}' > "$workdir/beta.json"

cat "$workdir/beta.json"

# no-store: this is the pointer clients poll. Cached at the edge it would keep
# serving the previous beta until the TTL lapsed.
aws s3 cp "$workdir/beta.json" \
  "s3://${BUCKET}/updates/beta.json" \
  --endpoint-url "$R2_ENDPOINT" \
  --content-type "application/json" \
  --cache-control "no-cache, no-store, must-revalidate"

# Fixed-name copy for a first install: the updater takes over afterwards, but a
# new tester needs one link that is always the current beta. Server-side copy,
# so the bytes are provably the ones just verified above.
for suffix in "" "-ARM64"; do
  aws s3 cp \
    "s3://${BUCKET}/betas/${VERSION}/GitWyrm-Setup${suffix}.exe" \
    "s3://${BUCKET}/betas/GitWyrm-Setup-Beta${suffix}.exe" \
    --endpoint-url "$R2_ENDPOINT" \
    --cache-control "no-cache, no-store"
done

for file in GitWyrm-x86_64.AppImage GitWyrm-amd64.deb; do
  aws s3 cp \
    "s3://${BUCKET}/betas/${VERSION}/${file}" \
    "s3://${BUCKET}/betas/${file}" \
    --endpoint-url "$R2_ENDPOINT" \
    --cache-control "no-cache, no-store"
done

echo "Beta channel now points at ${VERSION}"
