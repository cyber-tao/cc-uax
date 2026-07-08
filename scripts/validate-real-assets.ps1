param(
    [string]$ContentDir = $(if ($env:CC_UAX_CONTENT_DIR) { $env:CC_UAX_CONTENT_DIR } else { 'D:/WorkDir/ClashOfPets/Content' }),
    [string]$EngineSourceDir = $(if ($env:CC_UAX_UE_SOURCE_DIR) { $env:CC_UAX_UE_SOURCE_DIR } else { 'E:/UnrealEngine_5.7' }),
    [string]$Exe = $env:CC_UAX_EXE,
    [int]$Limit = 0,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir '..')
if (-not $Exe) {
    $Exe = Join-Path $RepoRoot 'target/release/cc-uax.exe'
}

if (-not $SkipBuild -and -not (Test-Path $Exe)) {
    Push-Location $RepoRoot
    try {
        cargo build --release
    } finally {
        Pop-Location
    }
}

if (-not (Test-Path $Exe)) {
    throw "cc-uax executable not found: $Exe"
}
if (-not (Test-Path $ContentDir)) {
    throw "content directory not found: $ContentDir"
}

$sourceChecks = @(
    'Engine/Source/Runtime/CoreUObject/Private/UObject/PropertyTag.cpp',
    'Engine/Source/Runtime/Engine/Private/EdGraph/EdGraphPin.cpp'
)
foreach ($relative in $sourceChecks) {
    $path = Join-Path $EngineSourceDir $relative
    if (-not (Test-Path $path)) {
        Write-Warning "UE source reference missing: $path"
    }
}

$files = Get-ChildItem $ContentDir -Recurse -File -Include '*.uasset', '*.umap' |
    Sort-Object FullName
if ($Limit -gt 0) {
    $files = $files | Select-Object -First $Limit
}
$files = @($files)
if ($files.Count -eq 0) {
    throw "no .uasset/.umap files found under $ContentDir"
}

function Invoke-CcUaxJson {
    param([string[]]$CliArgs)
    $output = & $Exe @CliArgs 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "cc-uax failed ($LASTEXITCODE): $($CliArgs -join ' ')`n$($output -join "`n")"
    }
    $text = ($output | Where-Object { $_ -match '^\s*\{' } | Select-Object -Last 1)
    if (-not $text) {
        throw "cc-uax did not emit JSON: $($CliArgs -join ' ')`n$($output -join "`n")"
    }
    [pscustomobject]@{
        Text = $text
        Json = $text | ConvertFrom-Json
    }
}

function Test-Section {
    param([string]$Section)
    $failed = 0
    $diagnostics = 0
    $unparsed = 0
    $i = 0
    foreach ($file in $files) {
        $i++
        if ($i -eq 1 -or $i % 100 -eq 0 -or $i -eq $files.Count) {
            Write-Host "[$Section] $i/$($files.Count) $($file.FullName)"
        }
        try {
            $result = Invoke-CcUaxJson -CliArgs @('-S', $Section, '--compact', $file.FullName)
            $diagnostics += @($result.Json.diagnostics).Count
            if ($result.Text.Contains('"@unparsed"')) {
                $unparsed++
            }
        } catch {
            $failed++
            Write-Warning $_.Exception.Message
        }
    }
    [pscustomobject]@{
        Section = $Section
        Total = $files.Count
        Failed = $failed
        Diagnostics = $diagnostics
        UnparsedFiles = $unparsed
    }
}

$debug = Test-Section 'debug'
$all = Test-Section 'all'

$sample = $files[0].FullName
$refs = Invoke-CcUaxJson -CliArgs @(
    '-S', 'refs',
    '--scan-dir', (Resolve-Path $ContentDir).Path,
    '--no-cache',
    '--compact',
    $sample
)

$referencedBy = @($refs.Json.references.referenced_by).Count
Write-Host "Reverse reference sample: $sample -> $referencedBy referencer(s)"
$debug
$all

if ($debug.Failed -ne 0 -or $debug.Diagnostics -ne 0 -or $debug.UnparsedFiles -ne 0) {
    throw "debug validation failed"
}
if ($all.Failed -ne 0 -or $all.Diagnostics -ne 0 -or $all.UnparsedFiles -ne 0) {
    throw "all validation failed"
}

Write-Host "Real asset validation passed."
