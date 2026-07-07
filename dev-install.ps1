#
# cc-uax dev installer (Windows / PowerShell) — rebuild from source and refresh local skills.
#
# Usage:
#   .\dev-install.ps1               build + install, refresh skills
#   .\dev-install.ps1 -Uninstall    cargo-uninstall cc-uax and remove local skills
#
# What it does:
#   1. cargo install --path . --force  →  builds and installs `cc-uax.exe` into ~\.cargo\bin
#   2. Copies skills\cc-uax\SKILL.md into Claude Code (~\.claude\skills\cc-uax),
#      Codex (~\.codex\skills\cc-uax), and legacy Agents (~\.agents\skills\cc-uax),
#      overwriting any existing copy.
#
# This is a local development helper. For the end-user release installer, see install.ps1.
#
param([switch]$Uninstall)
$ErrorActionPreference = 'Stop'

# Run from the script's own directory so cargo operates on the repo root.
if ($PSScriptRoot) { Set-Location $PSScriptRoot }

function Write-Step($n, $msg) { Write-Host "`n[$n/2] $msg" -ForegroundColor Cyan }
function Write-Ok($msg)      { Write-Host "[OK] $msg" -ForegroundColor Green }
function Write-Info($msg)    { Write-Host ">> $msg" -ForegroundColor DarkGray }
function Write-WarnMsg($msg) { Write-Host "!! $msg" -ForegroundColor Yellow }
function Die($msg)           { Write-Host "[X] $msg" -ForegroundColor Red; exit 1 }

# ── uninstall ───────────────────────────────────────────────────────────────
if ($Uninstall -or ($env:UNINSTALL -eq '1')) {
    Write-Host "`ncc-uax dev uninstall" -ForegroundColor Cyan
    $removed = $false
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        cargo uninstall cc-uax 2>$null
        if ($LASTEXITCODE -eq 0) {
            Write-Ok 'cargo uninstall cc-uax'
            $removed = $true
        } else {
            Write-WarnMsg 'cc-uax was not installed via cargo'
        }
    } else {
        Write-WarnMsg 'cargo not found - skipping binary removal'
    }
    foreach ($dir in @(
            (Join-Path $env:USERPROFILE '.claude\skills\cc-uax'),
            (Join-Path $env:USERPROFILE '.codex\skills\cc-uax'),
            (Join-Path $env:USERPROFILE '.agents\skills\cc-uax')
        )) {
        if (Test-Path $dir) {
            Remove-Item -Recurse -Force $dir
            Write-Ok "removed $dir"
            $removed = $true
        }
    }
    Write-Host ''
    if ($removed) { Write-Host 'cc-uax dev uninstall complete.' -ForegroundColor Green }
    else { Write-Host 'nothing to uninstall.' -ForegroundColor Yellow }
    Write-Host ''
    exit 0
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Die 'cargo not found on PATH — install Rust first'
}

$CargoBin = if ($env:CARGO_HOME) { Join-Path $env:CARGO_HOME 'bin' } else { Join-Path $env:USERPROFILE '.cargo\bin' }
$SkillSrc = Join-Path $PSScriptRoot 'skills\cc-uax\SKILL.md'
if (-not (Test-Path $SkillSrc)) { Die "skill source not found: $SkillSrc" }

# ── [1/2] build + install binary ─────────────────────────────────────────────
Write-Step 1 'Build and install cc-uax'
Write-Info 'cargo install --path . --force'
# $ErrorActionPreference = 'Stop' does not cover native-exe exit codes — check explicitly.
cargo install --path . --force
if ($LASTEXITCODE -ne 0) { Die "cargo install failed (exit $LASTEXITCODE)" }
Write-Ok "cc-uax -> $CargoBin\cc-uax.exe"

# ── [2/2] refresh skills (overwrite) ─────────────────────────────────────────
Write-Step 2 'Refresh agent skills'
foreach ($dir in @(
        (Join-Path $env:USERPROFILE '.claude\skills\cc-uax'),
        (Join-Path $env:USERPROFILE '.codex\skills\cc-uax'),
        (Join-Path $env:USERPROFILE '.agents\skills\cc-uax')
    )) {
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    Copy-Item $SkillSrc (Join-Path $dir 'SKILL.md') -Force
    Write-Ok "skill -> $dir\SKILL.md"
}

# ── summary ──────────────────────────────────────────────────────────────────
Write-Host ''
Write-Host 'cc-uax dev install complete.' -ForegroundColor Green
Write-Host 'Verify:  cc-uax --version' -ForegroundColor DarkGray
Write-Host ''
