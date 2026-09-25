<#
.SYNOPSIS
  Install the `wcl` CLI from a GitHub release.

.DESCRIPTION
  Downloads the Windows x86_64 `wcl` binary from a GitHub release, installs it
  into a bin directory, and adds that directory to the user PATH. The download
  is checked against the release's SHA256SUMS first; a missing or mismatched
  checksum aborts the install.

  WCL is pre-release only for now, so use -Pre (or -Version) — a plain run
  targets stable, which does not exist yet.

.EXAMPLE
  iwr https://wcl.dev/install.ps1 | iex

.EXAMPLE
  # To pass options, download then run (iex cannot forward arguments):
  iwr https://wcl.dev/install.ps1 -OutFile install.ps1; ./install.ps1 -Pre

.PARAMETER Version
  Install this version (e.g. 0.16.0-alpha).

.PARAMETER Pre
  Install the newest pre-release.

.PARAMETER InstallDir
  Install into this directory (default: %LOCALAPPDATA%\Programs\wcl).
#>
[CmdletBinding()]
param(
  [string]$Version,
  [switch]$Pre,
  [string]$InstallDir = "$env:LOCALAPPDATA\Programs\wcl"
)

$ErrorActionPreference = 'Stop'
$Repo = 'wiltaylor/wcl'
$SourceBuild = 'cargo install --git https://github.com/wiltaylor/wcl -p wcl --locked'
# Where release assets are fetched from. Only tests point it elsewhere.
$ReleasesUrl = if ($env:WCL_RELEASES_URL) { $env:WCL_RELEASES_URL } else { "https://github.com/$Repo/releases" }

# ── Detect platform ─────────────────────────────────────────────────────────
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -ne 'AMD64') {
  throw "no prebuilt Windows binary for $arch — build from source:`n  $SourceBuild"
}
$suffix = 'windows-x86_64.exe'

# ── Resolve version ─────────────────────────────────────────────────────────
$headers = @{ 'User-Agent' = 'wcl-install' }
if ($Version) {
  $tag = "v$($Version -replace '^v','')"
} elseif ($Pre) {
  $releases = Invoke-RestMethod -Headers $headers -Uri "https://api.github.com/repos/$Repo/releases"
  if (-not $releases) { throw "could not find any release for $Repo" }
  $tag = $releases[0].tag_name
} else {
  try {
    $latest = Invoke-RestMethod -Headers $headers -Uri "https://api.github.com/repos/$Repo/releases/latest"
    $tag = $latest.tag_name
  } catch {
    throw @"
no stable release published yet.
WCL is pre-release only for now — re-run with -Pre to get the newest pre-release:
  iwr https://wcl.dev/install.ps1 -OutFile install.ps1; ./install.ps1 -Pre
See https://github.com/$Repo/releases
"@
  }
}

$ver = $tag -replace '^v',''
$asset = "wcl-$ver-$suffix"
$base = "$ReleasesUrl/download/$tag"
$url = "$base/$asset"
$sumsUrl = "$base/SHA256SUMS"

# ── Download + verify + install ─────────────────────────────────────────────
Write-Host "Installing wcl $ver to $InstallDir"
$tmp = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
try {
  $download = Join-Path $tmp $asset
  $sums = Join-Path $tmp 'SHA256SUMS'
  try {
    Invoke-WebRequest -Headers $headers -Uri $url -OutFile $download
  } catch {
    throw "download failed: $url`nThe release may not exist or may lack a $suffix asset. See https://github.com/$Repo/releases"
  }
  try {
    Invoke-WebRequest -Headers $headers -Uri $sumsUrl -OutFile $sums
  } catch {
    throw "download failed: $sumsUrl`nReleases before checksums were published cannot be verified; build from source instead:`n  $SourceBuild"
  }

  # SHA256SUMS lines are `<hex>  <file>` (a `*` before the name marks binary mode).
  $expected = $null
  foreach ($line in Get-Content -LiteralPath $sums) {
    $fields = $line -split '\s+', 2
    if ($fields.Count -eq 2 -and $fields[1].TrimStart('*') -ceq $asset) {
      $expected = $fields[0].ToLowerInvariant()
      break
    }
  }
  if (-not $expected) { throw "SHA256SUMS has no entry for $asset - refusing to install" }
  $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $download).Hash.ToLowerInvariant()
  if ($actual -ne $expected) {
    throw "checksum mismatch for $asset - refusing to install`n  expected $expected`n  got      $actual"
  }

  New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
  $dest = Join-Path $InstallDir 'wcl.exe'
  Move-Item -Force -LiteralPath $download -Destination $dest
} finally {
  Remove-Item -Recurse -Force -LiteralPath $tmp -ErrorAction SilentlyContinue
}

Write-Host "Installed: $(& $dest --version)"

# ── Add to user PATH ────────────────────────────────────────────────────────
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$entries = ($userPath -split ';') | Where-Object { $_ -ne '' }
if ($entries -notcontains $InstallDir) {
  $newPath = if ($userPath) { "$userPath;$InstallDir" } else { $InstallDir }
  [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
  Write-Host "`nAdded $InstallDir to your user PATH. Open a new terminal for it to take effect."
}
