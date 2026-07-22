#!/usr/bin/env bash
# Points the updater manifest at real installer downloads, and checks that it does.
#
# tauri-action generates latest.json with api.github.com/repos/.../releases/assets/<id>
# URLs. Fetched without an Accept: application/octet-stream header, that endpoint
# returns asset metadata as JSON rather than the installer -- so the updater hands
# NSIS a JSON blob, which tears down the existing install and leaves nothing behind
# (shipped as 0.0.3). The public releases/download/<tag>/<file> URL serves the bytes.
#
# Runs in two modes because a draft release does not serve its assets from the
# public download path -- that path 404s until the release is published:
#
#   rewrite <tag>   Rewrite the URLs and re-upload. Run BEFORE publishing, so the
#                   manifest is already correct the moment the release goes live.
#   verify  <tag>   HEAD every URL and fail if any is not a plausible installer.
#                   Run AFTER publishing, when the URLs actually resolve.
#
# Requires: gh, python3.
set -euo pipefail

MODE="${1:?usage: resolve-updater-urls.sh <rewrite|verify> <tag>}"
TAG="${2:?usage: resolve-updater-urls.sh <rewrite|verify> <tag>}"
REPO="${GH_REPO:?GH_REPO must be set}"

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT
manifest="$workdir/latest.json"

# --clobber on upload needs the local name to match, so keep it as latest.json.
gh release download "$TAG" --repo "$REPO" --pattern latest.json --dir "$workdir"

case "$MODE" in
  rewrite)
    # Done in Python rather than jq: the filename is extracted from the signature
    # blob, which uses CRLF internally, and a stray \r in a URL yields a 404 that
    # still looks perfectly correct in logs.
    python3 - "$manifest" "$REPO" "$TAG" <<'PY'
import base64, json, re, sys

manifest_path, repo, tag = sys.argv[1], sys.argv[2], sys.argv[3]
base = f"https://github.com/{repo}/releases/download/{tag}"

with open(manifest_path) as fh:
    manifest = json.load(fh)

platforms = manifest.get("platforms") or {}
if not platforms:
    sys.exit("::error::Manifest has no platforms; nothing to rewrite.")

# The filename lives in the minisign signature's trusted comment ("file:NAME"),
# which is authoritative -- it is what was actually signed. Deriving the name from
# the platform key would guess at the arch label and could produce a URL pointing
# at a different file than the signature covers.
for name, entry in sorted(platforms.items()):
    sig = base64.b64decode(entry["signature"]).decode()
    match = re.search(r"file:(\S+)", sig)
    if not match:
        sys.exit(f"::error::No 'file:' in the signature for {name}.")
    entry["url"] = f"{base}/{match.group(1)}"
    print(f"  {name:<22} -> {match.group(1)}")

with open(manifest_path, "w") as fh:
    json.dump(manifest, fh, indent=2)
PY

    gh release upload "$TAG" "$manifest" --repo "$REPO" --clobber
    echo "Updater manifest for $TAG now points at public download URLs."
    ;;

  verify)
    python3 - "$manifest" <<'PY'
import json, sys, urllib.request

with open(sys.argv[1]) as fh:
    manifest = json.load(fh)

platforms = manifest.get("platforms") or {}
if not platforms:
    sys.exit("::error::Manifest has no platforms to verify.")

print("Verifying download URLs:")
failed = False
for name, entry in sorted(platforms.items()):
    url = entry["url"]
    try:
        # HEAD follows redirects to the asset host and avoids pulling ~9MB each.
        req = urllib.request.Request(url, method="HEAD")
        with urllib.request.urlopen(req, timeout=30) as resp:
            code = resp.status
            ctype = (resp.headers.get("Content-Type") or "").split(";")[0]
            length = int(resp.headers.get("Content-Length") or 0)
    except Exception as exc:
        print(f"  {name:<22} FAIL unreachable: {exc}")
        failed = True
        continue

    if code != 200:
        status = f"FAIL http={code}"
    elif ctype == "application/json":
        # The exact failure that shipped in 0.0.3.
        status = "FAIL serves JSON metadata, not an installer"
    elif length < 1_000_000:
        status = f"FAIL implausibly small ({length} bytes)"
    else:
        status = "ok"

    print(f"  {name:<22} {length:>10}  {ctype:<26} {status}")
    if status != "ok":
        failed = True

if failed:
    sys.exit("::error::Updater manifest has unusable download URLs.")
PY
    echo "Updater manifest for $TAG verified."
    ;;

  *)
    echo "::error::Unknown mode '$MODE' (expected 'rewrite' or 'verify')." >&2
    exit 2
    ;;
esac
