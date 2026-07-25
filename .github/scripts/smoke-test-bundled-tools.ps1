# Proves the bundled git and gpg can actually sign a commit.
#
# fetch-bundled-tools.sh pins an explicit list of the MSYS libraries GnuPG
# links against, rather than copying all 73 in usr/bin (35 MB, mostly
# Subversion and Perl). A pinned list is only safe if something checks it: a
# missing library does not announce itself, gpg just dies partway through with
# an error that points nowhere useful. This runs the real path end to end so
# an incomplete bundle fails the build instead of a user's first commit.
#
# Deliberately PowerShell, not bash. Git Bash's MSYS runtime rewrites GNUPGHOME
# on the way to a child process, so a test run there passes for reasons that do
# not apply to the shipped app - which spawns gpg directly from Rust. This
# mirrors the real conditions.
#
# Usage: smoke-test-bundled-tools.ps1 <resources-dir>

param(
  [Parameter(Mandatory = $true)][string]$ResourcesDir
)

$ErrorActionPreference = 'Stop'

$gitExe = Join-Path $ResourcesDir 'git\cmd\git.exe'
$gpgExe = Join-Path $ResourcesDir 'gpg\gpg.exe'
$gpgAgent = Join-Path $ResourcesDir 'gpg\gpg-agent.exe'

foreach ($required in @($gitExe, $gpgExe, $gpgAgent)) {
  if (-not (Test-Path $required)) {
    Write-Host "::error::Missing $required"
    exit 1
  }
}

# Short path on purpose: gpg-agent opens sockets inside GNUPGHOME and refuses to
# start when the resulting path gets too long, reporting only "No agent running".
$smoke = 'C:\gwsmoke'
Remove-Item -Recurse -Force $smoke -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path "$smoke\home", "$smoke\repo" | Out-Null

$gpgDir = (Resolve-Path (Join-Path $ResourcesDir 'gpg')).Path

# gpg.exe hardcodes /usr/bin/gpg-agent, an MSYS path that does not exist once
# the binaries are copied out of a Git for Windows install. Without this every
# operation needing the agent fails with "No agent running".
"agent-program $($gpgAgent -replace '\\', '/')" | Set-Content "$smoke\home\gpg.conf"

# GnuPG is an MSYS build and resolves paths through a POSIX layer, so GNUPGHOME
# must be /cygdrive/<letter>/... - a native "C:\..." makes it prefix its own
# root and look somewhere else entirely. Matches to_cygdrive() in
# src-tauri/src/git/signing.rs; if you change one, change the other.
$env:GNUPGHOME = '/cygdrive/c/gwsmoke/home'

Write-Host 'Generating a signing key with the bundled gpg...'
& $gpgExe --batch --passphrase '' --quick-generate-key `
  'Smoke Test <smoke@gitwyrm.invalid>' ed25519 sign never 2>&1 | Out-String | Write-Host

$secLine = & $gpgExe --list-secret-keys --with-colons 2>$null | Select-String '^sec:' | Select-Object -First 1
if (-not $secLine) {
  Write-Host '::error::Bundled gpg generated no usable key - the pinned DLL list is probably incomplete'
  exit 1
}
$key = $secLine.ToString().Split(':')[4]
Write-Host "Key: $key"

# An isolated global config. A stray or blank gpg.format on the runner makes git
# refuse every commit, which would read as a bundling failure rather than a
# machine-local config problem.
$env:GIT_CONFIG_GLOBAL = "$smoke\gitconfig"
New-Item -ItemType File -Force -Path $env:GIT_CONFIG_GLOBAL | Out-Null

Push-Location "$smoke\repo"
try {
  & $gitExe init -q .
  & $gitExe config user.name 'Smoke Test'
  & $gitExe config user.email 'smoke@gitwyrm.invalid'
  & $gitExe config gpg.program ($gpgExe -replace '\\', '/')
  & $gitExe config user.signingkey $key
  & $gitExe config commit.gpgsign true

  'smoke' | Set-Content file.txt
  & $gitExe add file.txt
  & $gitExe commit -q -m 'smoke: signed by the bundled tools'

  $verify = & $gitExe log --show-signature -1 2>&1 | Out-String
  if ($verify -notmatch 'Good signature') {
    Write-Host '::error::The bundled tools produced a commit without a good signature'
    Write-Host $verify
    exit 1
  }
  Write-Host 'Verified: the bundled git and gpg produced a good signature.'
}
finally {
  Pop-Location
  # Stop the agent before deleting its home, or it holds the directory open.
  # Cleanup must not decide the script's exit status: gpgconf returning
  # non-zero because no agent was running would otherwise fail a passing test.
  try { & (Join-Path $gpgDir 'gpgconf.exe') --kill all 2>&1 | Out-Null } catch {}
  $global:LASTEXITCODE = 0
  Remove-Item -Recurse -Force $smoke -ErrorAction SilentlyContinue
  Remove-Item Env:\GNUPGHOME -ErrorAction SilentlyContinue
  Remove-Item Env:\GIT_CONFIG_GLOBAL -ErrorAction SilentlyContinue
}

exit 0
