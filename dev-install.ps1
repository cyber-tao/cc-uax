#
# cc-uax dev installer (Windows / PowerShell) -- rebuild from source and refresh local skills.
#
# Usage:
#   .\dev-install.ps1               build + install, link skills into agent homes
#   .\dev-install.ps1 -Uninstall    remove the installed binary and skill links
#
# What it does:
#   1. cargo build -p cc-uax-cli --release --locked (incremental)
#   2. Copies target\release\cc-uax.exe into ~\.cargo\bin (override with INSTALL_DIR)
#   3. Junctions skills\cc-uax into Claude Code (~\.claude\skills\cc-uax),
#      Codex (~\.codex\skills\cc-uax), and legacy Agents (~\.agents\skills\cc-uax)
#
# Environment overrides (set before invoking):
#   $env:INSTALL_DIR         binary install location   (default: $CARGO_HOME\bin)
#   $env:CC_UAX_HOME         skill home root           (default: $env:USERPROFILE)
#   $env:UNINSTALL='1'       remove cc-uax instead of installing
#   $env:KEEP_BOTH='1'       if a release copy exists, keep it (no prompt)
#   $env:REPLACE_OTHER='1'   if a release copy exists, remove it (no prompt)
#
# This is a local development helper. For the end-user release installer, see install.ps1.
#
param(
    [switch]$Uninstall,
    [switch]$KeepBoth,
    [switch]$ReplaceOther
)
$ErrorActionPreference = 'Stop'

function Write-Step($n, $msg) { Write-Host "`n[$n/2] $msg" -ForegroundColor Cyan }
function Write-Ok($msg)      { Write-Host "[OK] $msg" -ForegroundColor Green }
function Write-Info($msg)    { Write-Host ">> $msg" -ForegroundColor DarkGray }
function Write-WarnMsg($msg) { Write-Host "!! $msg" -ForegroundColor Yellow }
function Die($msg)           { Write-Host "[X] $msg" -ForegroundColor Red; exit 1 }

function Test-SamePath($a, $b) {
    if (-not $a -or -not $b) { return $false }
    return [string]::Equals(
        [IO.Path]::GetFullPath($a),
        [IO.Path]::GetFullPath($b),
        [StringComparison]::OrdinalIgnoreCase
    )
}

function Get-DefaultReleaseDir {
    if ($env:CC_UAX_RELEASE_DIR) { return $env:CC_UAX_RELEASE_DIR }
    return (Join-Path $env:LOCALAPPDATA 'Programs\cc-uax')
}

function Get-OtherInstallBins([string]$OurDir) {
    $found = @()
    $releaseDir = Get-DefaultReleaseDir
    if (-not (Test-SamePath $releaseDir $OurDir)) {
        foreach ($name in @('cc-uax.exe', 'cc-uax')) {
            $p = Join-Path $releaseDir $name
            if (Test-Path -LiteralPath $p) { $found += (Resolve-Path -LiteralPath $p).Path }
        }
    }
    return $found | Select-Object -Unique
}

function Confirm-RemoveOther([string[]]$Others, [string]$Consequence) {
    $doReplace = $ReplaceOther -or ($env:REPLACE_OTHER -eq '1')
    $doKeep = $KeepBoth -or ($env:KEEP_BOTH -eq '1')
    if ($doReplace) { return $true }
    if ($doKeep) { return $false }
    Write-Host ''
    Write-WarnMsg 'Another cc-uax install is present:'
    foreach ($p in $Others) { Write-Host "    $p" }
    Write-Host $Consequence
    $redirected = $false
    try { $redirected = [Console]::IsInputRedirected } catch { $redirected = $true }
    if ($redirected) {
        Write-WarnMsg 'stdin is not a TTY; keeping both. Re-run with -ReplaceOther (or $env:REPLACE_OTHER=1) to remove the other copy.'
        return $false
    }
    $ans = Read-Host 'Uninstall the other copy? [y/N]'
    return ($ans -match '^[yY]([eE][sS])?$')
}

# Run the sibling installer in a child process -- `exit` in that script
# would otherwise terminate this one. Override INSTALL_DIR so the child
# uninstalls the release location, not this script's cargo-bin dest.
function Invoke-ReleaseUninstall {
    if (-not $PSScriptRoot) {
        Write-WarnMsg 'cannot invoke install.ps1 (not running from a checkout); leaving the other copy in place.'
        return $false
    }
    $script = Join-Path $PSScriptRoot 'install.ps1'
    if (-not (Test-Path -LiteralPath $script)) {
        Write-WarnMsg 'cannot invoke install.ps1 (not next to this script); leaving the other copy in place.'
        return $false
    }
    $saved = @{
        INSTALL_DIR    = $env:INSTALL_DIR
        NO_SKILL       = $env:NO_SKILL
        REPLACE_OTHER  = $env:REPLACE_OTHER
        KEEP_BOTH      = $env:KEEP_BOTH
        UNINSTALL      = $env:UNINSTALL
    }
    $env:INSTALL_DIR = Get-DefaultReleaseDir
    $env:NO_SKILL = '1'
    Remove-Item Env:REPLACE_OTHER -ErrorAction SilentlyContinue
    Remove-Item Env:KEEP_BOTH -ErrorAction SilentlyContinue
    Remove-Item Env:UNINSTALL -ErrorAction SilentlyContinue
    try {
        $p = Start-Process -FilePath 'powershell.exe' -ArgumentList @(
            '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $script, '-Uninstall'
        ) -WorkingDirectory $PSScriptRoot -Wait -PassThru -NoNewWindow
        if ($p.ExitCode -ne 0) {
            Write-WarnMsg "install.ps1 -Uninstall exited $($p.ExitCode)"
            return $false
        }
        return $true
    } finally {
        foreach ($k in $saved.Keys) {
            [Environment]::SetEnvironmentVariable($k, $saved[$k], 'Process')
        }
    }
}

function Show-PathWinner {
    $cmd = Get-Command cc-uax -ErrorAction SilentlyContinue
    if ($cmd -and $cmd.Source) {
        Write-Host "PATH will run: $($cmd.Source)" -ForegroundColor DarkGray
    }
}

function Get-SkillHome {
    if ($env:CC_UAX_HOME) { return $env:CC_UAX_HOME }
    return $env:USERPROFILE
}

function Get-BinDir {
    if ($env:INSTALL_DIR) { return $env:INSTALL_DIR }
    if ($env:CARGO_HOME) { return (Join-Path $env:CARGO_HOME 'bin') }
    return (Join-Path $env:USERPROFILE '.cargo\bin')
}

function Get-DefaultCargoBin {
    if ($env:CARGO_HOME) { return (Join-Path $env:CARGO_HOME 'bin') }
    return (Join-Path $env:USERPROFILE '.cargo\bin')
}

function Get-SkillDests([string]$SkillHome) {
    @(
        (Join-Path $SkillHome '.claude\skills\cc-uax'),
        (Join-Path $SkillHome '.codex\skills\cc-uax'),
        (Join-Path $SkillHome '.agents\skills\cc-uax')
    )
}

function Get-CargoTargetDir {
    if ($env:CARGO_TARGET_DIR) { return $env:CARGO_TARGET_DIR }
    $meta = cargo metadata --format-version 1 --no-deps --offline --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml') | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or -not $meta.target_directory) {
        return (Join-Path $PSScriptRoot 'target')
    }
    return $meta.target_directory
}

# Delete a skill destination without following a junction/symlink into the repo.
function Remove-SkillDest([string]$Dest) {
    if (-not (Test-Path -LiteralPath $Dest)) { return }
    $item = Get-Item -LiteralPath $Dest -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        [System.IO.Directory]::Delete($Dest)
        return
    }
    Remove-Item -LiteralPath $Dest -Recurse -Force
}

function Install-SkillLink([string]$Dest, [string]$Src) {
    $parent = Split-Path -Parent $Dest
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    if (Test-Path -LiteralPath $Dest) { Remove-SkillDest $Dest }
    New-Item -ItemType Junction -Path $Dest -Target $Src | Out-Null
}

$DoUninstall = $Uninstall -or ($env:UNINSTALL -eq '1')
$SkillHome = Get-SkillHome
$BinDir = Get-BinDir
$SkillSrc = Join-Path $PSScriptRoot 'skills\cc-uax'
$CliDir = Join-Path $PSScriptRoot 'crates\cc-uax-cli'

# == uninstall ===============================================================
if ($DoUninstall) {
    Write-Host "`ncc-uax dev uninstall" -ForegroundColor Cyan
    $removed = $false
    $sandbox = [bool]$env:INSTALL_DIR -or [bool]$env:CC_UAX_HOME
    if (-not $sandbox -and ($BinDir -eq (Get-DefaultCargoBin)) -and (Get-Command cargo -ErrorAction SilentlyContinue)) {
        cargo uninstall cc-uax-cli 2>$null
        if ($LASTEXITCODE -eq 0) {
            Write-Ok 'cargo uninstall cc-uax-cli'
            $removed = $true
        }
    }
    foreach ($name in @('cc-uax.exe', 'cc-uax')) {
        $bin = Join-Path $BinDir $name
        if (Test-Path -LiteralPath $bin) {
            Remove-Item -LiteralPath $bin -Force
            Write-Ok "removed $bin"
            $removed = $true
        }
    }
    foreach ($dir in (Get-SkillDests $SkillHome)) {
        if (Test-Path -LiteralPath $dir) {
            Remove-SkillDest $dir
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
    Die 'cargo not found on PATH -- install Rust first'
}
if (-not (Test-Path -LiteralPath (Join-Path $SkillSrc 'SKILL.md'))) {
    Die "skill source not found: $SkillSrc"
}
if (-not (Test-Path -LiteralPath (Join-Path $CliDir 'Cargo.toml'))) {
    Die "CLI package not found: $CliDir\Cargo.toml"
}

$SkillSrc = (Resolve-Path -LiteralPath $SkillSrc).Path

$otherBins = @(Get-OtherInstallBins $BinDir)
if ($otherBins.Count -gt 0) {
    $consequence = @"
The release installer places cc-uax earlier on PATH than ~/.cargo/bin.
Keeping both means ``cc-uax`` will still run the release binary, not the
checkout build this script is about to install into $BinDir.
Uninstalling the release copy runs install.ps1 -Uninstall (binary and PATH;
skills are left in place and re-linked to this repository).
"@
    if (Confirm-RemoveOther $otherBins $consequence) {
        if (-not (Invoke-ReleaseUninstall)) {
            Write-WarnMsg 'keeping both -- could not run install.ps1 -Uninstall.'
        }
    } else {
        Write-WarnMsg "keeping both -- ``cc-uax`` will still run the release copy."
    }
}

# == [1/2] build + install binary =============================================
Write-Step 1 'Build and install cc-uax'
Write-Info "cargo build -p cc-uax-cli --release --locked"
# $ErrorActionPreference = 'Stop' does not cover native-exe exit codes -- check explicitly.
cargo build -p cc-uax-cli --release --locked --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { Die "cargo build failed (exit $LASTEXITCODE)" }
$built = Join-Path (Get-CargoTargetDir) 'release\cc-uax.exe'
if (-not (Test-Path -LiteralPath $built)) { Die "built binary not found: $built" }
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
Copy-Item -LiteralPath $built -Destination (Join-Path $BinDir 'cc-uax.exe') -Force
Write-Ok "cc-uax -> $(Join-Path $BinDir 'cc-uax.exe')"

# == [2/2] link skills ========================================================
Write-Step 2 'Link agent skills'
foreach ($dir in (Get-SkillDests $SkillHome)) {
    Install-SkillLink $dir $SkillSrc
    Write-Ok "skill -> $dir"
}

# == summary ==================================================================
Write-Host ''
Write-Host 'cc-uax dev install complete.' -ForegroundColor Green
Show-PathWinner
Write-Host 'Verify:  cc-uax --version' -ForegroundColor DarkGray
Write-Host ''
