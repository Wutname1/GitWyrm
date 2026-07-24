<#
.SYNOPSIS
  Authenticode-signs a Windows binary with the Certum Open Source (SimplySign
  cloud) certificate. Called once per file.

.DESCRIPTION
  Two callers share this script:
    - tauri-action, via bundle.windows.signCommand in tauri.conf.json, which
      invokes it for the NSIS installer, the app exe, and the uninstaller.
    - bootstrapper.yml, as an explicit step for each bootstrapper exe.

  Certum's cloud key is a virtual smartcard exposed through a PKCS#11 provider,
  so signing goes through signtool's /csp + /kc path (a CNG/KSP provider works
  the same way with /csp). The signing material never lands on disk in the clear.

  DESIGNED TO NO-OP WHEN UNCONFIGURED. Until the certificate is bought and the
  CERTUM_* secrets are populated, this exits 0 without signing, so the release
  pipeline keeps producing (unsigned) builds exactly as it does today. The moment
  the secrets exist, every build signs with no workflow edit.

  Environment (all provided as GitHub secrets, empty until the cert is active):
    CERTUM_PKCS11_MODULE   Path to the SimplySign PKCS#11 provider DLL on the
                           runner (installed by an earlier setup step once known).
    CERTUM_KEY_CONTAINER   Key container / certificate name on the virtual card.
    CERTUM_CERT_SHA1       SHA1 thumbprint of the signing cert (selects it when
                           the card exposes more than one).
    CERTUM_PASSWORD        SimplySign credential unlocking the cloud key.
    CERTUM_TIMESTAMP_URL   RFC3161 timestamp URL. Defaults to Certum's.

.PARAMETER FilePath
  The binary to sign. tauri-action passes this as the trailing argument.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true, Position = 0)]
  [string]$FilePath
)

$ErrorActionPreference = 'Stop'

# Not configured yet -> succeed without signing. This is what keeps releases
# green between now and the cert being activated. Gate on the secrets that have
# no safe default; the timestamp URL has one, so it is not part of the check.
$required = @('CERTUM_PKCS11_MODULE', 'CERTUM_KEY_CONTAINER', 'CERTUM_CERT_SHA1', 'CERTUM_PASSWORD')
$missing = $required | Where-Object { [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($_)) }
if ($missing.Count -gt 0) {
  Write-Host "[sign-windows] Certum signing not configured (missing: $($missing -join ', ')). Skipping - '$FilePath' ships unsigned."
  exit 0
}

if (-not (Test-Path -LiteralPath $FilePath)) {
  Write-Error "[sign-windows] File to sign not found: $FilePath"
  exit 1
}

$timestampUrl = $env:CERTUM_TIMESTAMP_URL
if ([string]::IsNullOrWhiteSpace($timestampUrl)) {
  $timestampUrl = 'http://time.certum.pl'
}

# signtool ships with the Windows SDK, which is present on windows-latest runners
# but not always on PATH. Resolve it explicitly, newest SDK first.
function Resolve-SignTool {
  $cmd = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }

  $roots = @(
    "${env:ProgramFiles(x86)}\Windows Kits\10\bin",
    "${env:ProgramFiles}\Windows Kits\10\bin"
  ) | Where-Object { $_ -and (Test-Path $_) }

  $found = $roots |
    ForEach-Object { Get-ChildItem -Path $_ -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue } |
    Where-Object { $_.FullName -match '\\x64\\' } |
    Sort-Object FullName -Descending |
    Select-Object -First 1
  if ($found) { return $found.FullName }

  throw "signtool.exe not found. Install the Windows SDK on the runner."
}

$signtool = Resolve-SignTool
Write-Host "[sign-windows] Using signtool: $signtool"
Write-Host "[sign-windows] Signing: $FilePath"

# /fd sha256 + /td sha256: SHA-256 file digest and SHA-256 timestamp, required
# since SHA-1 signing is no longer trusted. /csp + /kc drive the SimplySign
# provider; /sha1 pins the exact cert on the virtual card. The password is passed
# via env, never as an argument, so it stays out of the process table and logs.
$signArgs = @(
  'sign',
  '/fd', 'sha256',
  '/tr', $timestampUrl,
  '/td', 'sha256',
  '/csp', 'Certum SimplySign CSP',
  '/kc', $env:CERTUM_KEY_CONTAINER,
  '/sha1', $env:CERTUM_CERT_SHA1,
  '/v',
  $FilePath
)

# Retry: timestamping reaches out to a network service that occasionally blips,
# and an unsigned-but-timestamped-later binary is not a thing - the whole sign
# has to redo. Three attempts with backoff.
$maxAttempts = 3
for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
  & $signtool @signArgs
  if ($LASTEXITCODE -eq 0) {
    Write-Host "[sign-windows] Signed and timestamped: $FilePath"
    # Prove it took, rather than trusting the exit code alone.
    & $signtool verify /pa /v $FilePath
    if ($LASTEXITCODE -ne 0) {
      Write-Error "[sign-windows] Signature verification failed for $FilePath"
      exit 1
    }
    exit 0
  }

  if ($attempt -lt $maxAttempts) {
    $delay = $attempt * 10
    Write-Warning "[sign-windows] Attempt $attempt failed (exit $LASTEXITCODE). Retrying in ${delay}s."
    Start-Sleep -Seconds $delay
  }
}

Write-Error "[sign-windows] signtool failed after $maxAttempts attempts for $FilePath"
exit 1
