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
#   3. Verifies the published SHA-256 checksum and installs cc-uax.exe
#   4. Adds the install dir to the user PATH (idempotent)
#   5. Installs the cc-uax skill into Claude Code (~\.claude\skills),
#      Codex (~\.codex\skills), and the legacy Agents path (~\.agents\skills)
#
# Environment overrides (set before invoking):
#   $env:INSTALL_DIR   binary install location   (default: ~\AppData\Local\Programs\cc-uax)
#   $env:VERSION       specific release tag      (default: latest)
#   $env:NO_SKILL='1'        skip skill configuration
#   $env:UNINSTALL='1'       remove cc-uax instead of installing
#   $env:KEEP_BOTH='1'       if a cargo/dev copy exists, keep it (no prompt)
#   $env:REPLACE_OTHER='1'   if a cargo/dev copy exists, remove it (no prompt)
#
param(
    [switch]$Uninstall,
    [switch]$KeepBoth,
    [switch]$ReplaceOther
)
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

function Test-SamePath($a, $b) {
    if (-not $a -or -not $b) { return $false }
    return [string]::Equals(
        [IO.Path]::GetFullPath($a),
        [IO.Path]::GetFullPath($b),
        [StringComparison]::OrdinalIgnoreCase
    )
}

function Get-DefaultCargoBinDir {
    if ($env:CC_UAX_DEV_BIN) { return $env:CC_UAX_DEV_BIN }
    if ($env:CARGO_HOME) { return (Join-Path $env:CARGO_HOME 'bin') }
    return (Join-Path $env:USERPROFILE '.cargo\bin')
}

function Get-OtherInstallBins([string]$OurDir) {
    $found = @()
    $cargoBin = Get-DefaultCargoBinDir
    if (-not (Test-SamePath $cargoBin $OurDir)) {
        foreach ($name in @('cc-uax.exe', 'cc-uax')) {
            $p = Join-Path $cargoBin $name
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
# would otherwise terminate this one. Clear this script's INSTALL_DIR so
# the child uninstalls ~/.cargo/bin, not the release destination.
function Invoke-DevUninstall {
    if (-not $PSScriptRoot) {
        Write-WarnMsg 'cannot invoke dev-install.ps1 (not running from a checkout); leaving the other copy in place.'
        return $false
    }
    $script = Join-Path $PSScriptRoot 'dev-install.ps1'
    if (-not (Test-Path -LiteralPath $script)) {
        Write-WarnMsg 'cannot invoke dev-install.ps1 (not next to this script); leaving the other copy in place.'
        return $false
    }
    $saved = @{
        INSTALL_DIR   = $env:INSTALL_DIR
        CC_UAX_HOME   = $env:CC_UAX_HOME
        REPLACE_OTHER = $env:REPLACE_OTHER
        KEEP_BOTH     = $env:KEEP_BOTH
        UNINSTALL     = $env:UNINSTALL
    }
    if ($env:CC_UAX_DEV_BIN) {
        $env:INSTALL_DIR = $env:CC_UAX_DEV_BIN
        if (-not $env:CC_UAX_HOME) { $env:CC_UAX_HOME = $env:USERPROFILE }
    } else {
        Remove-Item Env:INSTALL_DIR -ErrorAction SilentlyContinue
        Remove-Item Env:CC_UAX_HOME -ErrorAction SilentlyContinue
    }
    Remove-Item Env:REPLACE_OTHER -ErrorAction SilentlyContinue
    Remove-Item Env:KEEP_BOTH -ErrorAction SilentlyContinue
    Remove-Item Env:UNINSTALL -ErrorAction SilentlyContinue
    try {
        $p = Start-Process -FilePath 'powershell.exe' -ArgumentList @(
            '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $script, '-Uninstall'
        ) -WorkingDirectory $PSScriptRoot -Wait -PassThru -NoNewWindow
        if ($p.ExitCode -ne 0) {
            Write-WarnMsg "dev-install.ps1 -Uninstall exited $($p.ExitCode)"
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

# == uninstall ===============================================================
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
    # present -- and keep unrelated (including empty) segments untouched.
    $userPath = [System.Environment]::GetEnvironmentVariable('PATH', 'User')
    if ($userPath -and ($userPath.Split(';') -contains $InstallDir)) {
        $kept = $userPath.Split(';') | Where-Object { $_ -ne $InstallDir }
        [System.Environment]::SetEnvironmentVariable('PATH', ($kept -join ';'), 'User')
        Write-Ok "removed $InstallDir from user PATH"
        $removed = $true
    }

    if ($NoSkill) {
        Write-WarnMsg 'NO_SKILL=1 -- leaving skills in place'
    } else {
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
    }

    Write-Host ''
    if ($removed) { Write-Host 'cc-uax uninstalled.' -ForegroundColor Green }
    else { Write-Host 'nothing to uninstall.' -ForegroundColor Yellow }
    Write-Host ''
    exit 0
}

$otherBins = @(Get-OtherInstallBins $InstallDir)
if ($otherBins.Count -gt 0) {
    $consequence = @"
This installer prepends $InstallDir to User PATH, so the new release
binary will run instead of the cargo/dev copy. Keeping both leaves an
unused binary in ~/.cargo/bin. Uninstalling the cargo/dev copy runs
dev-install.ps1 -Uninstall; this installer then refreshes skills.
"@
    if (Confirm-RemoveOther $otherBins $consequence) {
        if (-not (Invoke-DevUninstall)) {
            Write-WarnMsg 'keeping both -- could not run dev-install.ps1 -Uninstall.'
        }
    } else {
        Write-WarnMsg 'keeping both -- the release copy will win on PATH after install.'
    }
}

# == [1/5] detect platform ===================================================
Write-Step 1 'Detecting platform'
# Windows release is x86_64-pc-windows-msvc; Windows 11 ARM runs it via x64 emulation.
$Target = 'x86_64-pc-windows-msvc'
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -eq 'ARM64') {
    Write-WarnMsg "ARM64 Windows detected -- using x86_64 build via emulation."
} elseif ($arch -ne 'AMD64') {
    Die "unsupported arch: $arch (expected AMD64 or ARM64)"
}
Write-Ok "target=$Target  arch=$arch"

# == [2/5] resolve version ===================================================
Write-Step 2 'Resolving latest version'
if ($env:VERSION) {
    $Tag = if ($env:VERSION.StartsWith('v')) { $env:VERSION } else { "v$($env:VERSION)" }
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

# == [3/5] download ==========================================================
Write-Step 3 'Downloading'
$Archive = "cc-uax-${Target}-${Version}.zip"
$Url = "https://github.com/$Repo/releases/download/$Tag/$Archive"
$ChecksumUrl = "https://github.com/$Repo/releases/download/$Tag/SHA256SUMS"
Write-Info $Url
$Tmp = New-Item -ItemType Directory -Path (Join-Path $env:TEMP "cc-uax-install-$(Get-Random)") -Force
$ArchivePath = Join-Path $Tmp.FullName $Archive
$ChecksumPath = Join-Path $Tmp.FullName 'SHA256SUMS'
try {
    Invoke-WebRequest -Uri $Url -OutFile $ArchivePath -UseBasicParsing
    Invoke-WebRequest -Uri $ChecksumUrl -OutFile $ChecksumPath -UseBasicParsing
} catch {
    Die "download or checksum fetch failed: $($_.Exception.Message)"
}
if (-not (Test-Path $ArchivePath)) { Die "archive not downloaded: $Archive" }
$ChecksumText = Get-Content -Raw -LiteralPath $ChecksumPath
$ArchivePattern = [regex]::Escape($Archive)
$ChecksumMatch = [regex]::Match(
    [string]$ChecksumText,
    "(?m)^([0-9a-fA-F]{64})\s+\*?$ArchivePattern`r?$"
)
if (-not $ChecksumMatch.Success) { Die "checksum missing or invalid for $Archive" }
$ExpectedHash = $ChecksumMatch.Groups[1].Value.ToLowerInvariant()
$ActualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $ArchivePath).Hash.ToLowerInvariant()
if ($ActualHash -ne $ExpectedHash) { Die "checksum mismatch for $Archive" }
Write-Ok "downloaded and verified $Archive"

# == [4/5] install binary ====================================================
Write-Step 4 'Installing binary'
$Extract = Join-Path $Tmp.FullName 'extract'
Expand-Archive -Path $ArchivePath -DestinationPath $Extract -Force
$StagedExe = Join-Path $Extract "cc-uax-${Target}-${Version}\cc-uax.exe"
if (-not (Test-Path $StagedExe)) { Die "cc-uax.exe not found in archive" }
$StagedLicense = Join-Path $Extract "cc-uax-${Target}-${Version}\LICENSE"
if (-not (Test-Path $StagedLicense)) { Die "LICENSE missing in archive" }

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

# == [5/5] configure skills ==================================================
Write-Step 5 'Configuring agent skills'
if ($NoSkill) {
    Write-WarnMsg 'NO_SKILL=1 -- skipping skill configuration'
} else {
    $SkillSrc = Join-Path $Extract "cc-uax-${Target}-${Version}\skills\cc-uax"
    if (-not (Test-Path (Join-Path $SkillSrc 'SKILL.md'))) { Die "SKILL.md missing in archive" }

    foreach ($dir in @(
            (Join-Path $env:USERPROFILE '.claude\skills\cc-uax'),
            (Join-Path $env:USERPROFILE '.codex\skills\cc-uax'),
            (Join-Path $env:USERPROFILE '.agents\skills\cc-uax')
        )) {
        if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $dir) | Out-Null
        Copy-Item -LiteralPath $SkillSrc -Destination $dir -Recurse -Force
        Write-Ok "skill -> $dir"
    }
}

# == summary =================================================================
Remove-Item -Recurse -Force $Tmp.FullName -ErrorAction SilentlyContinue
Write-Host ""
Write-Host "cc-uax $Version installed." -ForegroundColor Green
Show-PathWinner
Write-Host "Open a NEW terminal, then run:  cc-uax --version" -ForegroundColor DarkGray
Write-Host ""
