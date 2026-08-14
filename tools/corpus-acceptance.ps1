<#
.SYNOPSIS
Real-corpus acceptance harness for cc-uax.

.DESCRIPTION
Scans real UE projects with a release build of cc-uax and records the numbers that
ordinary workspace tests cannot cover: exit code, report status, scan accounting,
unsupported-package counts, opaque-region totals, byte conservation, the
FileVersionUE5 distribution actually present, and rendered report size.

The first run writes a baseline. Later runs compare against it and fail on any
regression, so a decoder change that silently loses evidence or reclassifies
packages is caught against real assets.

This is deliberately not a Cargo workspace member (see CLAUDE.md): corpora live
outside the repository, so corpus paths are arguments and every generated report
and baseline is written to -OutputDirectory, which defaults to a directory under
the OS temp dir. Nothing corpus-specific is written into the repository.

.PARAMETER Project
One or more UE project roots, .uproject files, or Content directories.

.PARAMETER OutputDirectory
Where reports and the baseline are written. Defaults to <temp>/cc-uax-corpus.

.PARAMETER UpdateBaseline
Overwrite the baseline with this run's numbers instead of comparing against it.

.EXAMPLE
./tools/corpus-acceptance.ps1 -Project D:/Work/StackOBot, D:/Work/Vehicle_58

.EXAMPLE
./tools/corpus-acceptance.ps1 -Project D:/Work/StackOBot -UpdateBaseline
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string[]] $Project,

    [string] $OutputDirectory = (Join-Path ([System.IO.Path]::GetTempPath()) 'cc-uax-corpus'),

    [switch] $UpdateBaseline
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent $PSScriptRoot

function Get-ReleaseBinary {
    Push-Location $repoRoot
    try {
        cargo build --workspace --release --locked
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
        # Honour a redirected target directory (CARGO_TARGET_DIR, config.toml).
        $metadata = cargo metadata --format-version 1 --no-deps | ConvertFrom-Json
        $exe = if ($IsWindows -or $env:OS -eq 'Windows_NT') { 'cc-uax.exe' } else { 'cc-uax' }
        $path = Join-Path (Join-Path $metadata.target_directory 'release') $exe
        if (-not (Test-Path -LiteralPath $path)) { throw "release binary not found at $path" }
        return $path
    }
    finally { Pop-Location }
}

# FileVersionUE5 lives at a fixed header offset, and only when LegacyFileVersion is
# <= -8. UE4 packages stop before that field and report 0.
function Get-FileVersionDistribution {
    param([string] $Root)

    $distribution = @{}
    $files = Get-ChildItem -LiteralPath $Root -Recurse -File -Include *.uasset, *.umap -ErrorAction SilentlyContinue
    foreach ($file in $files) {
        $head = New-Object byte[] 20
        $stream = [System.IO.File]::OpenRead($file.FullName)
        try { $read = $stream.Read($head, 0, 20) } finally { $stream.Dispose() }
        if ($read -lt 20) { continue }
        $legacy = [System.BitConverter]::ToInt32($head, 4)
        $version = if ($legacy -le -8) { [System.BitConverter]::ToInt32($head, 16) } else { 0 }
        $key = [string] $version
        if ($distribution.ContainsKey($key)) { $distribution[$key] += 1 } else { $distribution[$key] = 1 }
    }
    return $distribution
}

function Measure-Project {
    param([string] $Binary, [string] $Target, [string] $ReportPath)

    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    & $Binary project $Target --no-cache --compact --output $ReportPath
    $exitCode = $LASTEXITCODE
    $stopwatch.Stop()

    if (-not (Test-Path -LiteralPath $ReportPath)) {
        throw "no report was written for $Target (exit code $exitCode)"
    }
    $report = Get-Content -LiteralPath $ReportPath -Raw | ConvertFrom-Json

    # Reports are sparse: a zero counter or empty collection is omitted, so an
    # absent property means the default rather than an error.
    $number = { param($object, $name) if ($object.PSObject.Properties[$name]) { $object.$name } else { 0 } }
    $count = { param($object, $name) if ($object.PSObject.Properties[$name]) { @($object.$name).Count } else { 0 } }
    $items = { param($object, $name) if ($object.PSObject.Properties[$name]) { @($object.$name) } else { @() } }

    $coverage = $report.analysis.coverage
    $groupRegions = 0
    $groupBytes = 0
    foreach ($asset in (& $items $report 'inventory')) {
        if (-not $asset.analysis.PSObject.Properties['known_opaque']) { continue }
        foreach ($group in (& $items $asset.analysis.known_opaque 'groups')) {
            $groupRegions += $group.regions
            $groupBytes += $group.bytes
        }
    }

    return [ordered] @{
        exit_code             = $exitCode
        status                = $report.status
        seconds               = [math]::Round($stopwatch.Elapsed.TotalSeconds, 1)
        report_bytes          = (Get-Item -LiteralPath $ReportPath).Length
        discovered            = $report.stats.discovered
        indexed               = $report.stats.indexed
        failed                = $report.stats.failed
        skipped               = $report.stats.skipped
        assets                = $report.analysis.assets
        complete_assets       = $report.analysis.complete_assets
        partial_assets        = $report.analysis.partial_assets
        unsupported_assets    = $report.analysis.unsupported_assets
        scan_failures         = $report.analysis.scan_failures
        failures              = & $count $report 'failures'
        diagnostics           = & $count $report 'diagnostics'
        export_bytes_total    = & $number $coverage 'export_bytes_total'
        opaque_bytes          = & $number $coverage 'opaque_bytes'
        known_opaque_regions  = & $number $coverage 'known_opaque_regions'
        unclassified_bytes    = & $number $coverage 'unclassified_bytes'
        grouped_regions       = $groupRegions
        grouped_bytes         = $groupBytes
    }
}

# Invariants that must hold for every corpus regardless of the baseline.
function Test-Invariants {
    param([string] $Name, $Result)

    $problems = @()
    if ($Result.unclassified_bytes -ne 0) {
        $problems += "unclassified_bytes is $($Result.unclassified_bytes); every export byte must be decoded or classified opaque"
    }
    if ($Result.discovered -ne ($Result.indexed + $Result.failed + $Result.skipped)) {
        $problems += "scan accounting broken: discovered $($Result.discovered) != indexed $($Result.indexed) + failed $($Result.failed) + skipped $($Result.skipped)"
    }
    if ($Result.assets -ne ($Result.complete_assets + $Result.partial_assets + $Result.unsupported_assets)) {
        $problems += "asset status accounting broken for $($Result.assets) assets"
    }
    if ($Result.grouped_regions -ne $Result.known_opaque_regions) {
        $problems += "grouped opaque regions $($Result.grouped_regions) != coverage.known_opaque_regions $($Result.known_opaque_regions)"
    }
    if ($Result.grouped_bytes -ne $Result.opaque_bytes) {
        $problems += "grouped opaque bytes $($Result.grouped_bytes) != coverage.opaque_bytes $($Result.opaque_bytes)"
    }
    if ($Result.exit_code -ne 0 -and $Result.exit_code -ne 2) {
        $problems += "unexpected exit code $($Result.exit_code); a scan should exit 0 or 2"
    }
    foreach ($problem in $problems) { Write-Host "  INVARIANT  $Name : $problem" -ForegroundColor Red }
    return $problems.Count -eq 0
}

# Regressions are one-directional: evidence may improve, never degrade.
function Test-AgainstBaseline {
    param([string] $Name, $Result, $Baseline)

    $problems = @()
    foreach ($key in 'exit_code', 'status', 'discovered', 'indexed', 'assets', 'unsupported_assets') {
        if ($Result.$key -ne $Baseline.$key) {
            $problems += "$key changed: $($Baseline.$key) -> $($Result.$key)"
        }
    }
    if ($Result.failed -gt $Baseline.failed) {
        $problems += "failed assets rose: $($Baseline.failed) -> $($Result.failed)"
    }
    if ($Result.complete_assets -lt $Baseline.complete_assets) {
        $problems += "complete assets fell: $($Baseline.complete_assets) -> $($Result.complete_assets)"
    }
    if ($Result.known_opaque_regions -gt $Baseline.known_opaque_regions) {
        $problems += "opaque regions rose: $($Baseline.known_opaque_regions) -> $($Result.known_opaque_regions)"
    }
    if ($Result.opaque_bytes -gt $Baseline.opaque_bytes) {
        $problems += "opaque bytes rose: $($Baseline.opaque_bytes) -> $($Result.opaque_bytes)"
    }
    foreach ($problem in $problems) { Write-Host "  REGRESSION $Name : $problem" -ForegroundColor Red }
    return $problems.Count -eq 0
}

$binary = Get-ReleaseBinary
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$baselinePath = Join-Path $OutputDirectory 'baseline.json'
Write-Host "cc-uax: $binary"
Write-Host "output: $OutputDirectory"
Write-Host ""

$results = [ordered] @{}
foreach ($target in $Project) {
    if (-not (Test-Path -LiteralPath $target)) { throw "corpus not found: $target" }
    $name = [System.IO.Path]::GetFileNameWithoutExtension((Split-Path -Leaf $target))
    $reportPath = Join-Path $OutputDirectory "$name.json"
    Write-Host "scanning $name ..."
    $result = Measure-Project -Binary $binary -Target $target -ReportPath $reportPath

    # Version coverage is corpus evidence in its own right: it records which gates
    # this run actually exercised, and which remain untested by any real asset.
    $contentRoot = if (Test-Path -LiteralPath (Join-Path $target 'Content')) { Join-Path $target 'Content' } else { Split-Path -Parent $target }
    if (Test-Path -LiteralPath $contentRoot) {
        $result['file_version_ue5'] = Get-FileVersionDistribution -Root $contentRoot
    }
    $results[$name] = $result

    Write-Host ("  exit={0} status={1} indexed={2} unsupported={3} failed={4} opaque_regions={5} report={6} bytes  ({7}s)" -f `
        $result.exit_code, $result.status, $result.indexed, $result.unsupported_assets, $result.failed, `
        $result.known_opaque_regions, $result.report_bytes, $result.seconds)
}

Write-Host ""
$ok = $true
foreach ($name in $results.Keys) {
    if (-not (Test-Invariants -Name $name -Result $results[$name])) { $ok = $false }
}

if ($UpdateBaseline -or -not (Test-Path -LiteralPath $baselinePath)) {
    if (-not $ok) { throw 'refusing to record a baseline that violates an invariant' }
    $results | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $baselinePath -Encoding utf8
    Write-Host "baseline recorded at $baselinePath" -ForegroundColor Green
    exit 0
}

$baseline = Get-Content -LiteralPath $baselinePath -Raw | ConvertFrom-Json
foreach ($name in $results.Keys) {
    if (-not $baseline.PSObject.Properties[$name]) {
        Write-Host "  NEW        $name : not in the baseline; rerun with -UpdateBaseline to record it" -ForegroundColor Yellow
        continue
    }
    if (-not (Test-AgainstBaseline -Name $name -Result $results[$name] -Baseline $baseline.$name)) { $ok = $false }
}

if (-not $ok) {
    Write-Host ""
    Write-Host 'corpus acceptance FAILED' -ForegroundColor Red
    exit 1
}
Write-Host 'corpus acceptance passed' -ForegroundColor Green
