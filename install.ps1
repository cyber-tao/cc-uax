#
# cc-uax one-line installer for Windows (PowerShell).
#
#   irm https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.ps1 | iex
#
# Uninstall (remove the binary, PATH entry, and skills):
#   .\install.ps1 -Uninstall
#   $env:UNINSTALL='1'; irm https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.ps1 | iex
#
# What it does:
#   1. Resolves the latest release from GitHub
#   2. Downloads the x86_64 Windows archive (also runs on Windows 11 ARM via x64 emulation)
#   3. Installs cc-uax.exe (default: $env:LOCALAPPDATA\Programs\cc-uax, override with $env:INSTALL_DIR)
#   4. Adds the install dir to the user PATH (idempotent)
#   5. Installs the cc-uax skill into Claude Code (~\.claude\skills) and Codex (~\.agents\skills)
#
# Environment overrides (set before invoking):
#   $env:INSTALL_DIR   binary install location   (default: ~\AppData\Local\Programs\cc-uax)
#   $env:VERSION       specific release tag      (default: latest)
#   $env:NO_SKILL='1'  skip skill configuration
#   $env:UNINSTALL='1' remove cc-uax instead of installing
#
param([switch]$Uninstall)
$ErrorActionPreference = 'Stop'
# Invoke-WebRequest's progress bar drastically throttles downloads on Windows PowerShell 5.1.
$ProgressPreference = 'SilentlyContinue'

$Repo = 'cyber-tao/cc-uax'
$InstallDir = if ($env:INSTALL_DIR) { $env:INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\cc-uax' }
$NoSkill = ($env:NO_SKILL -eq '1')
# $Uninstall binds for `.\install.ps1 -Uninstall`; the env var covers the piped `irm | iex` path.
$DoUninstall = $Uninstall -or ($env:UNINSTALL -eq '1')

function Write-Step($n, $msg) { Write-Host "`n[$n/5] $msg" -ForegroundColor Cyan }
function Write-Ok($msg)      { Write-Host "[OK] $msg" -ForegroundColor Green }
function Write-Info($msg)    { Write-Host ">> $msg" -ForegroundColor DarkGray }
function Write-WarnMsg($msg) { Write-Host "!! $msg" -ForegroundColor Yellow }
function Die($msg)           { Write-Host "[X] $msg" -ForegroundColor Red; exit 1 }

# ── uninstall ───────────────────────────────────────────────────────────────
if ($DoUninstall) {
    Write-Host "`ncc-uax uninstall" -ForegroundColor Cyan
    $removed = $false

    $bin = Join-Path $InstallDir 'cc-uax.exe'
    if (Test-Path $bin) {
        Remove-Item $bin -Force
        Write-Ok "removed $bin"
        $removed = $true
        # Drop the install dir only if it is now empty.
        if ((Test-Path $InstallDir) -and -not (Get-ChildItem -Force $InstallDir)) {
            Remove-Item $InstallDir -Force
            Write-Ok "removed empty dir $InstallDir"
        }
    } else {
        Write-WarnMsg "binary not found: $bin"
    }

    # Reverse the install-time user PATH edit, but only when our dir is actually
    # present — and keep unrelated (including empty) segments untouched.
    $userPath = [System.Environment]::GetEnvironmentVariable('PATH', 'User')
    if ($userPath -and ($userPath.Split(';') -contains $InstallDir)) {
        $kept = $userPath.Split(';') | Where-Object { $_ -ne $InstallDir }
        [System.Environment]::SetEnvironmentVariable('PATH', ($kept -join ';'), 'User')
        Write-Ok "removed $InstallDir from user PATH"
        $removed = $true
    }

    if ($NoSkill) {
        Write-WarnMsg 'NO_SKILL=1 — leaving skills in place'
    } else {
        foreach ($dir in @(
                (Join-Path $env:USERPROFILE '.claude\skills\cc-uax'),
                (Join-Path $env:USERPROFILE '.agents\skills\cc-uax')
            )) {
            if (Test-Path $dir) {
                Remove-Item -Recurse -Force $dir
                Write-Ok "removed $dir"
                $removed = $true
            }
        }
    }

    Write-Host ''
    if ($removed) { Write-Host 'cc-uax uninstalled.' -ForegroundColor Green }
    else { Write-Host 'nothing to uninstall.' -ForegroundColor Yellow }
    Write-Host ''
    exit 0
}

# ── [1/5] detect platform ───────────────────────────────────────────────────
Write-Step 1 'Detecting platform'
# Windows release is x86_64-pc-windows-msvc; Windows 11 ARM runs it via x64 emulation.
$Target = 'x86_64-pc-windows-msvc'
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -eq 'ARM64') {
    Write-WarnMsg "ARM64 Windows detected — using x86_64 build via emulation."
} elseif ($arch -ne 'AMD64') {
    Die "unsupported arch: $arch (expected AMD64 or ARM64)"
}
Write-Ok "target=$Target  arch=$arch"

# ── [2/5] resolve version ───────────────────────────────────────────────────
Write-Step 2 'Resolving latest version'
if ($env:VERSION) {
    $Tag = $env:VERSION
} else {
    $apiUrl = "https://api.github.com/repos/$Repo/releases/latest"
    try {
        $release = Invoke-RestMethod -Uri $apiUrl -UseBasicParsing
        $Tag = $release.tag_name
    } catch {
        Die "cannot resolve latest release (network error or rate limited): $($_.Exception.Message)"
    }
}
if (-not $Tag) { Die 'empty release tag' }
$Version = $Tag.TrimStart('v')
Write-Ok "version=$Version ($Tag)"

# ── [3/5] download ──────────────────────────────────────────────────────────
Write-Step 3 'Downloading'
$Archive = "cc-uax-${Target}-${Version}.zip"
$Url = "https://github.com/$Repo/releases/download/$Tag/$Archive"
Write-Info $Url
$Tmp = New-Item -ItemType Directory -Path (Join-Path $env:TEMP "cc-uax-install-$(Get-Random)") -Force
$ArchivePath = Join-Path $Tmp.FullName $Archive
try {
    Invoke-WebRequest -Uri $Url -OutFile $ArchivePath -UseBasicParsing
} catch {
    Die "download failed: $($_.Exception.Message)"
}
if (-not (Test-Path $ArchivePath)) { Die "archive not downloaded: $Archive" }
Write-Ok "downloaded $Archive"

# ── [4/5] install binary ────────────────────────────────────────────────────
Write-Step 4 'Installing binary'
$Extract = Join-Path $Tmp.FullName 'extract'
Expand-Archive -Path $ArchivePath -DestinationPath $Extract -Force
$StagedExe = Join-Path $Extract "cc-uax-${Target}-${Version}\cc-uax.exe"
if (-not (Test-Path $StagedExe)) { Die "cc-uax.exe not found in archive" }

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item $StagedExe (Join-Path $InstallDir 'cc-uax.exe') -Force
Write-Ok "binary -> $InstallDir\cc-uax.exe"

# Add to user PATH (idempotent)
$userPath = [System.Environment]::GetEnvironmentVariable('PATH', 'User')
if ($userPath -and ($userPath.Split(';') -contains $InstallDir)) {
    Write-Ok "$InstallDir already on user PATH"
} else {
    $newPath = if ($userPath) { "$InstallDir;$userPath" } else { $InstallDir }
    # Guard against the historical 2048-char limit for the User env var.
    if ($newPath.Length -gt 2048) {
        Write-WarnMsg "User PATH is too long to auto-modify; add $InstallDir manually."
    } else {
        [System.Environment]::SetEnvironmentVariable('PATH', $newPath, 'User')
        # Reflect in the current process so `cc-uax` works in this session.
        $env:PATH = "$InstallDir;$env:PATH"
        Write-Ok "added $InstallDir to user PATH"
    }
}

# ── [5/5] configure skills ──────────────────────────────────────────────────
Write-Step 5 'Configuring agent skills'
if ($NoSkill) {
    Write-WarnMsg 'NO_SKILL=1 — skipping skill configuration'
} else {
    $SkillSrc = Join-Path $Extract "cc-uax-${Target}-${Version}\skills\cc-uax\SKILL.md"
    if (-not (Test-Path $SkillSrc)) { Die "SKILL.md missing in archive" }

    # Claude Code: ~\.claude\skills\cc-uax\
    $CcDir = Join-Path $env:USERPROFILE '.claude\skills\cc-uax'
    New-Item -ItemType Directory -Force -Path $CcDir | Out-Null
    Copy-Item $SkillSrc (Join-Path $CcDir 'SKILL.md') -Force
    Write-Ok "Claude Code skill -> $CcDir\SKILL.md"

    # Codex: ~\.agents\skills\cc-uax\
    $CodexDir = Join-Path $env:USERPROFILE '.agents\skills\cc-uax'
    New-Item -ItemType Directory -Force -Path $CodexDir | Out-Null
    Copy-Item $SkillSrc (Join-Path $CodexDir 'SKILL.md') -Force
    Write-Ok "Codex skill        -> $CodexDir\SKILL.md"
}

# ── summary ─────────────────────────────────────────────────────────────────
Remove-Item -Recurse -Force $Tmp.FullName -ErrorAction SilentlyContinue
Write-Host ""
Write-Host "cc-uax $Version installed." -ForegroundColor Green
Write-Host "Open a NEW terminal, then run:  cc-uax --version" -ForegroundColor DarkGray
Write-Host ""
