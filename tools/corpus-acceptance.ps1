<#
.SYNOPSIS
Real-corpus acceptance harness for cc-uax.

.DESCRIPTION
Scans real UE projects with a release build of cc-uax and records the numbers that
ordinary workspace tests cannot cover: exit code, report status, scan accounting,
unsupported-package counts, opaque-region totals and their attribution, byte
conservation, whether every partial asset explains itself, the mount count, the
FileVersionUE5 distribution the scan actually covered, and rendered report size.

The version distribution is read from the report rather than by walking Content,
so it reflects exactly what was scanned, including auto-mounted plugin roots.

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

# `$ErrorActionPreference = 'Stop'` turns anything a native command writes to
# stderr into a terminating error, and cargo reports normal build progress there.
# Run external commands through this so only a non-zero exit code is a failure.
function Invoke-Native {
    param([scriptblock] $Command)

    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { & $Command } finally { $ErrorActionPreference = $previous }
}

# Runs a native command for its console output and exit code only. Stringifying
# each line drops the ErrorRecord wrapper PowerShell puts around stderr, which is
# what otherwise leaves a NativeCommandError in `$Error` for ordinary progress.
#
# Deliberately not delegating to Invoke-Native: a scriptblock passed to it would
# bind `$Command` to that function's own parameter and recurse.
function Invoke-NativeLogged {
    param([scriptblock] $NativeCommand)

    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $NativeCommand 2>&1 | ForEach-Object { "$_" } | Out-Host
    }
    finally { $ErrorActionPreference = $previous }
}

# `$IsWindows` is PowerShell 7+ only, and `Set-StrictMode -Version Latest` makes
# reading an undefined variable fatal, so referencing it broke this script on the
# Windows PowerShell 5.1 that CI parses it with. `$env:OS` is set on both.
$script:OnWindows = $env:OS -eq 'Windows_NT'

function Get-ReleaseBinary {
    Push-Location $repoRoot
    try {
        Invoke-NativeLogged { cargo build --workspace --release --locked }
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
        # Honour a redirected target directory (CARGO_TARGET_DIR, config.toml).
        $metadata = Invoke-Native { cargo metadata --format-version 1 --no-deps } | ConvertFrom-Json
        $exe = if ($script:OnWindows) { 'cc-uax.exe' } else { 'cc-uax' }
        $path = Join-Path (Join-Path $metadata.target_directory 'release') $exe
        if (-not (Test-Path -LiteralPath $path)) { throw "release binary not found at $path" }
        return $path
    }
    finally { Pop-Location }
}

function Measure-Project {
    param([string] $Binary, [string] $Target, [string] $ReportPath)

    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    # Exit code 2 is an expected outcome (a hard scan failure still writes a
    # report), so the run must not be treated as a terminating error.
    Invoke-NativeLogged { & $Binary project $Target --no-cache --compact --output $ReportPath }
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
    # A partial asset has to say why. The report is the only place a consumer can
    # read that, so an unexplained partial is an acceptance failure even when
    # every count reconciles.
    $unexplainedPartials = 0
    foreach ($asset in (& $items $report 'inventory')) {
        $analysis = $asset.analysis
        if ($analysis.PSObject.Properties['known_opaque']) {
            foreach ($group in (& $items $analysis.known_opaque 'groups')) {
                $groupRegions += $group.regions
                $groupBytes += $group.bytes
            }
        }
        if ($analysis.status -ne 'partial') { continue }
        $hasCodes = $analysis.PSObject.Properties['diagnostics'] -and
                    $analysis.diagnostics.PSObject.Properties['codes']
        $hasDetail = $false
        foreach ($capability in (& $items $analysis 'capabilities')) {
            if ($capability.PSObject.Properties['detail']) { $hasDetail = $true; break }
        }
        if (-not $hasCodes -and -not $hasDetail) { $unexplainedPartials += 1 }
    }

    # The version distribution comes from the report, so it covers exactly what was
    # scanned -- including auto-mounted plugin content, which a Content-directory
    # walk would miss.
    $versions = [ordered] @{}
    if ($report.analysis.PSObject.Properties['file_versions']) {
        foreach ($property in $report.analysis.file_versions.PSObject.Properties) {
            $versions[$property.Name] = $property.Value
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
        mounts                = & $count $report 'mounts'
        export_bytes_total    = & $number $coverage 'export_bytes_total'
        opaque_bytes          = & $number $coverage 'opaque_bytes'
        class_payload_bytes   = & $number $coverage 'class_payload_bytes'
        unattributed_tail_bytes = & $number $coverage 'unattributed_tail_bytes'
        known_opaque_regions  = & $number $coverage 'known_opaque_regions'
        unclassified_bytes    = & $number $coverage 'unclassified_bytes'
        grouped_regions       = $groupRegions
        grouped_bytes         = $groupBytes
        unexplained_partials  = $unexplainedPartials
        file_version_ue5      = $versions
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
    # The two tail buckets are subsets of opaque_bytes covering export tails only,
    # so they can never exceed it; if they do, one of them is double-counting.
    $tailBytes = $Result.class_payload_bytes + $Result.unattributed_tail_bytes
    if ($tailBytes -gt $Result.opaque_bytes) {
        $problems += "tail byte split $tailBytes exceeds coverage.opaque_bytes $($Result.opaque_bytes)"
    }
    if ($Result.unexplained_partials -ne 0) {
        $problems += "$($Result.unexplained_partials) partial asset(s) carry neither a diagnostic code nor a capability detail; a partial must say what is missing"
    }
    if ($Result.file_version_ue5.Keys.Count -eq 0 -and $Result.assets -gt 0) {
        $problems += "no FileVersionUE5 distribution reported; the scan cannot state which version gates it exercised"
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
    # Losing a mount means losing whole content roots from the scan.
    if ($Result.mounts -lt $Baseline.mounts) {
        $problems += "mounts fell: $($Baseline.mounts) -> $($Result.mounts)"
    }
    # Version coverage is the only statement of which gates real assets exercise,
    # so a version disappearing from the distribution is lost coverage even if
    # every other count improves.
    if ($Baseline.PSObject.Properties['file_version_ue5']) {
        foreach ($property in $Baseline.file_version_ue5.PSObject.Properties) {
            $now = if ($Result.file_version_ue5.Contains($property.Name)) {
                $Result.file_version_ue5[$property.Name]
            } else { 0 }
            if ($now -lt $property.Value) {
                $problems += "FileVersionUE5 $($property.Name) coverage fell: $($property.Value) -> $now package(s)"
            }
        }
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

# Names key the baseline and the report files, so a leaf like `trunk` shared by two
# corpora must not silently overwrite the other. Qualify with parent directories
# until the name is unique.
function Get-CorpusName {
    param([string] $Target, [System.Collections.Specialized.OrderedDictionary] $Taken)

    $parts = [System.Collections.Generic.List[string]]::new()
    $parts.Add([System.IO.Path]::GetFileNameWithoutExtension((Split-Path -Leaf $Target)))
    $parent = Split-Path -Parent $Target
    while ($Taken.Contains(($parts.ToArray() -join '-')) -and $parent) {
        $leaf = Split-Path -Leaf $parent
        if ([string]::IsNullOrEmpty($leaf)) { break }
        $parts.Insert(0, $leaf)
        $parent = Split-Path -Parent $parent
    }
    $name = $parts.ToArray() -join '-'
    if ($Taken.Contains($name)) { throw "cannot derive a unique name for $Target" }
    return $name
}

$results = [ordered] @{}
foreach ($target in $Project) {
    if (-not (Test-Path -LiteralPath $target)) { throw "corpus not found: $target" }
    $name = Get-CorpusName -Target $target -Taken $results
    $reportPath = Join-Path $OutputDirectory "$name.json"
    Write-Host "scanning $name ..."
    $result = Measure-Project -Binary $binary -Target $target -ReportPath $reportPath
    $results[$name] = $result

    Write-Host ("  exit={0} status={1} indexed={2} unsupported={3} failed={4} opaque_regions={5} report={6} bytes  ({7}s)" -f `
        $result.exit_code, $result.status, $result.indexed, $result.unsupported_assets, $result.failed, `
        $result.known_opaque_regions, $result.report_bytes, $result.seconds)
    # Version coverage is corpus evidence in its own right: it records which gates
    # this run exercised, and by omission which remain untested by any real asset.
    $versions = ($result.file_version_ue5.Keys | Sort-Object { [int] $_ } | ForEach-Object {
        "$_=$($result.file_version_ue5[$_])"
    }) -join ' '
    Write-Host "  mounts=$($result.mounts) FileVersionUE5: $versions"
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
