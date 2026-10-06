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
    # The assets actually attached to the release, as {api url: file name}.
    # macOS needs this: tauri-action renames the updater archive per
    # architecture (GitWyrm_aarch64.app.tar.gz) after it was signed under the
    # bundler's own name (GitWyrm.app.tar.gz), so the signature's file name
    # does not exist on the release for those entries.
    gh release view "$TAG" --repo "$REPO" --json assets --jq '[.assets[] | {name, apiUrl}]' > "$workdir/assets.json"

    python3 - "$manifest" "$REPO" "$TAG" "$workdir/assets.json" <<'PY'
import base64, json, re, sys

manifest_path, repo, tag, assets_path = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
with open(assets_path) as fh:
    assets = json.load(fh)
asset_names = {a["name"] for a in assets}
name_by_api_url = {a["apiUrl"]: a["name"] for a in assets}
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
    filename = match.group(1)
    if filename not in asset_names:
        # Signed under a different name than the one uploaded. Trust the
        # asset the manifest already points at, found by its API URL.
        filename = name_by_api_url.get(entry.get("url", ""))
        if not filename:
            sys.exit(
                f"::error::{name}: signed as {match.group(1)}, which is not a release "
                f"asset, and its URL matches no asset either."
            )
    entry["url"] = f"{base}/{filename}"
    print(f"  {name:<22} -> {filename}")

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

# A platform missing entirely is the quiet failure: if a build leg produces no
# updater artifact -- signing turned off, a bundler that did not run -- tauri-action
# simply leaves that key out. The installer still uploads and every name-based
# asset check passes, so the release ships with those users pinned to their
# current version forever, with nothing in the logs saying so.
EXPECTED = ["windows-x86_64", "windows-aarch64", "linux-x86_64", "darwin-aarch64", "darwin-x86_64"]
absent = [key for key in EXPECTED if key not in platforms]
if absent:
    sys.exit(
        "::error::Updater manifest is missing platform(s): "
        + ", ".join(absent)
        + ". Present: "
        + ", ".join(sorted(platforms))
    )

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
