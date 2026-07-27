#!/usr/bin/env bash
#
# Fetches the git and gpg that ship inside GitWyrm.
#
# GitWyrm bundles its own copies so someone on a clean Windows machine can
# clone, commit, sign, and push without installing anything first. At runtime a
# system install always wins (see src-tauri/src/git/bundled.rs); these are the
# fallback.
#
# Both come from the same upstream release, so the bundled git and gpg are
# always a version pair that Git for Windows itself ships together.
#
#   git -> MinGit-<ver>-<arch>.zip, the tree Git for Windows publishes for
#          redistribution. Includes Git Credential Manager, so HTTPS auth works.
#   gpg -> carved out of Git-<ver>-<arch>.tar.bz2, the portable tree. MinGit
#          trims usr/bin down and drops GnuPG, so it has to come from here.
#
# Usage: fetch-bundled-tools.sh <arch> <dest>
#   arch: x86_64 | aarch64
#   dest: directory to populate (src-tauri/resources)
#
# Layout produced, matching the paths bundled.rs looks for:
#   <dest>/git/cmd/git.exe
#   <dest>/gpg/gpg.exe

set -euo pipefail

ARCH="${1:?usage: fetch-bundled-tools.sh <x86_64|aarch64> <dest>}"
DEST="${2:?usage: fetch-bundled-tools.sh <x86_64|aarch64> <dest>}"

case "$ARCH" in
  x86_64)  MINGIT_PATTERN="64-bit"; PORTABLE_PATTERN="64-bit" ;;
  aarch64) MINGIT_PATTERN="arm64";  PORTABLE_PATTERN="arm64"  ;;
  *) echo "::error::Unknown arch '$ARCH' (expected x86_64 or aarch64)"; exit 1 ;;
esac

API="https://api.github.com/repos/git-for-windows/git/releases/latest"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "Resolving the latest Git for Windows release..."
# GH_TOKEN lifts the 60/hr anonymous rate limit that otherwise makes this step
# flaky on busy runners.
AUTH=()
if [ -n "${GH_TOKEN:-}" ]; then
  AUTH=(-H "Authorization: Bearer $GH_TOKEN")
fi
curl -fsSL "${AUTH[@]}" "$API" > "$WORK/release.json"

TAG="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["tag_name"])' "$WORK/release.json")"
echo "Latest release: $TAG"

# Pick an asset by substring, failing loudly rather than shipping an installer
# with no git in it. A silent miss here is far worse than a failed build.
pick_asset() {
  python3 - "$WORK/release.json" "$1" "$2" <<'PY'
import json, sys
data, prefix, pattern = sys.argv[1], sys.argv[2], sys.argv[3]
release = json.load(open(data))
for asset in release["assets"]:
    name = asset["name"]
    if name.startswith(prefix) and pattern in name and name.endswith(prefix_ext := ".zip" if prefix == "MinGit" else ".tar.bz2"):
        print(asset["browser_download_url"])
        break
PY
}

MINGIT_URL="$(pick_asset MinGit "$MINGIT_PATTERN")"
PORTABLE_URL="$(pick_asset Git "$PORTABLE_PATTERN")"

if [ -z "$MINGIT_URL" ]; then
  echo "::error::No MinGit asset matching '$MINGIT_PATTERN' in $TAG"
  exit 1
fi
if [ -z "$PORTABLE_URL" ]; then
  echo "::error::No portable Git asset matching '$PORTABLE_PATTERN' in $TAG"
  exit 1
fi

mkdir -p "$DEST/git" "$DEST/gpg"

# ---------------------------------------------------------------- git (MinGit)
echo "Downloading $(basename "$MINGIT_URL")"
curl -fsSL -o "$WORK/mingit.zip" "$MINGIT_URL"
echo "Unpacking git..."
# Unpacked rather than shipped as a zip on purpose: the NSIS bundler compresses
# its payload with solid LZMA, which gets the tree far smaller than passing
# through an already-compressed zip (measured 21.9 MB vs 37 MB). git also cannot
# run from inside an archive - it needs its real siblings on disk.
unzip -q "$WORK/mingit.zip" -d "$DEST/git"

# ------------------------------------------------------------------ gpg (GnuPG)
echo "Downloading $(basename "$PORTABLE_URL")"
curl -fsSL -o "$WORK/git-portable.tar.bz2" "$PORTABLE_URL"
echo "Extracting gpg..."

# Only the GnuPG pieces, not the whole 116 MB tree. gpg.exe and its agent live
# in usr/bin alongside the msys crypto DLLs they link against; the helpers gpg
# spawns itself live in usr/lib/gnupg.
tar -xjf "$WORK/git-portable.tar.bz2" -C "$WORK" \
  --wildcards \
  'usr/bin/gpg*.exe' \
  'usr/bin/pinentry*.exe' \
  'usr/bin/ssh-keygen.exe' \
  'usr/bin/msys-*.dll' \
  'usr/lib/gnupg/*'

# Flatten into one directory: gpg locates its helpers relative to its own
# location, and a flat layout keeps the paths short (gpg-agent refuses to start
# when its socket path grows too long).
cp "$WORK"/usr/bin/gpg*.exe "$DEST/gpg/"
cp "$WORK"/usr/bin/pinentry*.exe "$DEST/gpg/"

# ssh-keygen is what signs commits when the user picks SSH over GPG, and it is
# the one ssh tool MinGit leaves out (it ships ssh, ssh-add and ssh-agent, but
# not keygen). Lives beside gpg so both signing backends resolve the same way.
cp "$WORK"/usr/bin/ssh-keygen.exe "$DEST/gpg/"
mkdir -p "$DEST/gpg/lib/gnupg"
cp "$WORK"/usr/lib/gnupg/* "$DEST/gpg/lib/gnupg/" 2>/dev/null || true

# Only the libraries GnuPG actually links against. usr/bin carries 73 msys DLLs
# (35 MB) covering Subversion, Perl, Kerberos and GnuTLS - none of which gpg
# loads. Copying the lot cost ~30 MB of installer for nothing, so the set is
# pinned here and the smoke test below is what proves it is complete.
GPG_DLLS=(
  msys-2.0.dll          # the MSYS runtime everything links against
  msys-gcrypt-20.dll    # libgcrypt, the actual crypto
  msys-gpg-error-0.dll  # error codes, used by every GnuPG library
  msys-assuan-9.dll     # gpg <-> gpg-agent IPC
  msys-npth-0.dll       # the agent's threading library
  msys-ksba-8.dll       # X.509/CMS, needed by gpgsm
  msys-iconv-2.dll      # character set conversion
  msys-intl-8.dll       # gettext, for translated messages
  msys-z.dll            # compression of packets
  msys-bz2-1.dll        # the other supported compression algorithm
  msys-readline8.dll    # line editing in --edit-key
  msys-ncursesw6.dll    # readline's terminal backend
  msys-sqlite3-0.dll    # keyboxd's database
  msys-gcc_s-seh-1.dll  # unwinder the above are built against
)
for dll in "${GPG_DLLS[@]}"; do
  if [ -f "$WORK/usr/bin/$dll" ]; then
    cp "$WORK/usr/bin/$dll" "$DEST/gpg/"
  else
    echo "::warning::Expected $dll in the portable tree but it was absent"
  fi
done

# GPL compliance: both trees are GPL'd, so the licence ships with them.
if [ -f "$DEST/git/LICENSE.txt" ]; then
  cp "$DEST/git/LICENSE.txt" "$DEST/gpg/LICENSE.txt"
fi
printf 'Git for Windows %s\nhttps://github.com/git-for-windows/git/releases/tag/%s\n' \
  "$TAG" "$TAG" > "$DEST/BUNDLED-VERSION.txt"

# ------------------------------------------------------------------- verify
# A bundle that unpacked but does not run is the failure mode worth catching
# here rather than in a user's install.
for required in "$DEST/git/cmd/git.exe" "$DEST/gpg/gpg.exe" "$DEST/gpg/gpg-agent.exe"; do
  if [ ! -f "$required" ]; then
    echo "::error::Expected $required after unpacking, but it is missing"
    exit 1
  fi
done

echo "Bundled git:  $("$DEST/git/cmd/git.exe" --version)"
echo "Bundled gpg:  $("$DEST/gpg/gpg.exe" --version | head -1)"

echo "Bundled tools ready in $DEST ($(du -sh "$DEST" | cut -f1))"
echo "Run smoke-test-bundled-tools.ps1 to verify they can sign a commit."
