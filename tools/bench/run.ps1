<#
.SYNOPSIS
  Performance budget benchmark for the relic CLI.
.DESCRIPTION
  Runs relic-cli against a synthetic library of a given size, measures execution times
  for key CLI operations, checks them against budgeted performance thresholds, and reports a table.
  Can be run locally and in CI on Windows, Linux, and macOS.
.PARAMETER Files
  The target number of files to generate in the synthetic library. Defaults to 10000.
.PARAMETER KeepArtifacts
  If set, does not clean up the generated synthetic ROM files and test databases.
#>
param(
    [int]$Files = 10000,
    [switch]$KeepArtifacts
)

$ErrorActionPreference = "Stop"

# Detect OS
if ($null -eq $IsWindows) {
    $IsWindows = $env:OS -like "*Windows*" -or $env:OS -eq "Windows_NT"
}

# Resolve paths
$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".." ".."))
$FixgenPath = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".." "fixgen" "generate.ps1"))

# Locate binary
$BinDir = "release"
$BinName = if ($IsWindows) { "relic.exe" } else { "relic" }
$RelicBin = [System.IO.Path]::GetFullPath((Join-Path $RepoRoot "target" $BinDir $BinName))

# 1. Build CLI in release mode
Write-Host "Building relic-cli in release mode..." -ForegroundColor Cyan
Push-Location $RepoRoot
try {
    cargo build -p relic-cli --release
    if ($LastExitCode -ne 0) {
        Write-Host "Error: Cargo release build failed!" -ForegroundColor Red
        exit 1
    }
}
catch {
    Write-Host "Error: An exception occurred during Cargo build: $_" -ForegroundColor Red
    exit 1
}
finally {
    Pop-Location
}

if (-not (Test-Path $RelicBin)) {
    Write-Host "Error: Could not find built binary at $RelicBin" -ForegroundColor Red
    exit 1
}

# Create temp directory
$TempDir = [System.IO.Path]::GetTempPath()
$RandomSuffix = [System.IO.Path]::GetRandomFileName()
$ScratchDir = Join-Path $TempDir "relic_bench_$RandomSuffix"
$null = New-Item -ItemType Directory -Path $ScratchDir -Force

$LibraryRoot = Join-Path $ScratchDir "roms"
$null = New-Item -ItemType Directory -Path $LibraryRoot -Force

# Helper to create unique DB path
function Get-FreshDbPath {
    $RandomName = [System.IO.Path]::GetRandomFileName()
    return Join-Path $ScratchDir "bench_$RandomName.db"
}

try {
    # 2. Compute PerSystem and generate synthetic library (default has 6 systems)
    $PerSystem = [int][Math]::Ceiling($Files / 6)
    Write-Host "Generating synthetic library with $PerSystem games per system (6 systems, total ~$($PerSystem * 6) files)..." -ForegroundColor Cyan
    
    & $FixgenPath -Root $LibraryRoot -PerSystem $PerSystem
    if ($LastExitCode -ne 0) {
        Write-Host "Error: fixgen library generation failed!" -ForegroundColor Red
        exit 1
    }

    # Budgets calculation
    $ColdBudgetSec = 30.0 * ($Files / 10000.0)
    $IncrBudgetSec = 2.0 * ($Files / 10000.0)
    
    # Query budget distinction: The PLAN.md budget of 30ms is for the in-process engine.
    # Spawning the CLI process adds startup overhead (process startup, DB connection initialization, etc.),
    # so we use a 100ms threshold for the CLI-level process benchmarks.
    $QueryBudgetMs = 100.0
    
    # 3. Time operations
    
    # (a) cold initial scan
    Write-Host "Running (a) cold initial scan..." -ForegroundColor Cyan
    $ColdDb = Get-FreshDbPath
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    & $RelicBin scan --db $ColdDb $LibraryRoot > $null
    $sw.Stop()
    if ($LastExitCode -ne 0) {
        Write-Host "Error: Cold initial scan failed with exit code $LastExitCode" -ForegroundColor Red
        exit 1
    }
    $ColdMeasuredMs = $sw.Elapsed.TotalMilliseconds
    $ColdMeasuredSec = $ColdMeasuredMs / 1000.0

    # (b) incremental rescan (no changes)
    Write-Host "Running (b) incremental rescan (3 runs)..." -ForegroundColor Cyan
    $IncrDb = Get-FreshDbPath
    & $RelicBin scan --db $IncrDb $LibraryRoot > $null
    if ($LastExitCode -ne 0) {
        Write-Host "Error: Incremental scan setup failed with exit code $LastExitCode" -ForegroundColor Red
        exit 1
    }
    $IncrTimes = @()
    for ($i = 0; $i -lt 3; $i++) {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        & $RelicBin scan --db $IncrDb $LibraryRoot > $null
        $sw.Stop()
        if ($LastExitCode -ne 0) {
            Write-Host "Error: Incremental scan run $i failed with exit code $LastExitCode" -ForegroundColor Red
            exit 1
        }
        $IncrTimes += $sw.Elapsed.TotalMilliseconds
    }
    $IncrMedianMs = ($IncrTimes | Sort-Object)[1]
    $IncrMedianSec = $IncrMedianMs / 1000.0

    # (c) 'games' query listing one system's games
    Write-Host "Running (c) 'games' query for single system (3 runs)..." -ForegroundColor Cyan
    $GamesDb = Get-FreshDbPath
    & $RelicBin scan --db $GamesDb $LibraryRoot > $null
    if ($LastExitCode -ne 0) {
        Write-Host "Error: Games query setup failed with exit code $LastExitCode" -ForegroundColor Red
        exit 1
    }
    $GamesTimes = @()
    for ($i = 0; $i -lt 3; $i++) {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        & $RelicBin games --db $GamesDb --system "nes" > $null
        $sw.Stop()
        if ($LastExitCode -ne 0) {
            Write-Host "Error: Games query run $i failed with exit code $LastExitCode" -ForegroundColor Red
            exit 1
        }
        $GamesTimes += $sw.Elapsed.TotalMilliseconds
    }
    $GamesMedianMs = ($GamesTimes | Sort-Object)[1]

    # (d) 'games --search' FTS query
    Write-Host "Running (d) 'games --search' query (3 runs)..." -ForegroundColor Cyan
    $SearchDb = Get-FreshDbPath
    & $RelicBin scan --db $SearchDb $LibraryRoot > $null
    if ($LastExitCode -ne 0) {
        Write-Host "Error: Search query setup failed with exit code $LastExitCode" -ForegroundColor Red
        exit 1
    }
    $SearchTimes = @()
    for ($i = 0; $i -lt 3; $i++) {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        & $RelicBin games --db $SearchDb --search "Super" > $null
        $sw.Stop()
        if ($LastExitCode -ne 0) {
            Write-Host "Error: Search query run $i failed with exit code $LastExitCode" -ForegroundColor Red
            exit 1
        }
        $SearchTimes += $sw.Elapsed.TotalMilliseconds
    }
    $SearchMedianMs = ($SearchTimes | Sort-Object)[1]

    # Evaluate PASS/FAIL
    $ColdPass = $ColdMeasuredSec -le $ColdBudgetSec
    $IncrPass = $IncrMedianSec -le $IncrBudgetSec
    $GamesPass = $GamesMedianMs -le $QueryBudgetMs
    $SearchPass = $SearchMedianMs -le $QueryBudgetMs

    $TotalBudgeted = 4
    $PassedBudgeted = 0
    if ($ColdPass) { $PassedBudgeted++ }
    if ($IncrPass) { $PassedBudgeted++ }
    if ($GamesPass) { $PassedBudgeted++ }
    if ($SearchPass) { $PassedBudgeted++ }

    # Format output strings
    $ColdMeasuredStr = "{0:F2}s" -f $ColdMeasuredSec
    $ColdBudgetStr = "{0:F2}s" -f $ColdBudgetSec
    $IncrMedianStr = "{0:F2}s" -f $IncrMedianSec
    $IncrBudgetStr = "{0:F2}s" -f $IncrBudgetSec
    $GamesMedianStr = "{0:F1}ms" -f $GamesMedianMs
    $GamesBudgetStr = "100ms"
    $SearchMedianStr = "{0:F1}ms" -f $SearchMedianMs
    $SearchBudgetStr = "100ms"

    # Report results table
    Write-Host ""
    Write-Host "operation | measured | budget | PASS/FAIL"
    Write-Host "---|---|---|---"
    Write-Host "cold initial scan | $ColdMeasuredStr | $ColdBudgetStr | $(if ($ColdPass) { 'PASS' } else { 'FAIL' })"
    Write-Host "incremental rescan | $IncrMedianStr | $IncrBudgetStr | $(if ($IncrPass) { 'PASS' } else { 'FAIL' })"
    Write-Host "games query | $GamesMedianStr | $GamesBudgetStr | $(if ($GamesPass) { 'PASS' } else { 'FAIL' })"
    Write-Host "search query | $SearchMedianStr | $SearchBudgetStr | $(if ($SearchPass) { 'PASS' } else { 'FAIL' })"
    Write-Host ""

    Write-Host "bench: $PassedBudgeted/$TotalBudgeted within budget"
    
    if ($PassedBudgeted -lt $TotalBudgeted) {
        exit 1
    } else {
        exit 0
    }
}
finally {
    if (-not $KeepArtifacts) {
        if (Test-Path $ScratchDir) {
            Write-Host "Cleaning up temporary artifacts..." -ForegroundColor Gray
            Remove-Item -Recurse -Force $ScratchDir -ErrorAction Ignore
        }
    } else {
        Write-Host "Keeping temporary artifacts at: $ScratchDir" -ForegroundColor Gray
    }
}
