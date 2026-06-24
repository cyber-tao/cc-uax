#
# cc-uax dev installer (Windows / PowerShell) — rebuild from source and refresh local skills.
#
# Usage:
#   .\dev-install.ps1
#
# What it does:
#   1. cargo install --path . --force  →  builds and installs `cc-uax.exe` into ~\.cargo\bin
#   2. Copies skills\cc-uax\SKILL.md into Claude Code (~\.claude\skills\cc-uax)
#      and Codex (~\.agents\skills\cc-uax), overwriting any existing copy.
#
# This is a local development helper. For the end-user release installer, see install.ps1.
#
$ErrorActionPreference = 'Stop'

# Run from the script's own directory so cargo operates on the repo root.
if ($PSScriptRoot) { Set-Location $PSScriptRoot }

function Write-Step($n, $msg) { Write-Host "`n[$n/2] $msg" -ForegroundColor Cyan }
function Write-Ok($msg)      { Write-Host "[OK] $msg" -ForegroundColor Green }
function Write-Info($msg)    { Write-Host ">> $msg" -ForegroundColor DarkGray }
function Die($msg)           { Write-Host "[X] $msg" -ForegroundColor Red; exit 1 }

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
