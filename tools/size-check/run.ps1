<#
.SYNOPSIS
  Checks release build artifacts against the binary-size budgets in
  PLAN.md §8. Can be run locally and in CI on Windows, Linux, and macOS.
.DESCRIPTION
  Builds relic-capi and relic-desktop in release mode, then compares
  the resulting cdylib/binary sizes against their documented budgets
  (relic-capi < 4 MB, relic-desktop < 20 MB). Exits non-zero if any
  artifact is missing or over budget.
.PARAMETER SkipBuild
  If set, skips the cargo build and checks whatever is already in
  target/release (useful for re-running the check after a build you
  already did).
#>
param(
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

# Detect OS
if ($null -eq $IsWindows) {
    $IsWindows = $env:OS -like "*Windows*" -or $env:OS -eq "Windows_NT"
}
if ($null -eq $IsMacOS) {
    $IsMacOS = $false
}

$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$ReleaseDir = Join-Path $RepoRoot "target" "release"

if (-not $SkipBuild) {
    Write-Host "Building relic-capi and relic-desktop in release mode..." -ForegroundColor Cyan
    Push-Location $RepoRoot
    try {
        cargo build --release -p relic-capi -p relic-desktop
        if ($LastExitCode -ne 0) {
            Write-Host "Cargo build failed!" -ForegroundColor Red
            exit 1
        }
    }
    finally {
        Pop-Location
    }
}

function Get-CdylibPath {
    param([string]$CrateName)
    $stem = $CrateName -replace '-', '_'
    if ($IsWindows) { return Join-Path $ReleaseDir "$stem.dll" }
    if ($IsMacOS) { return Join-Path $ReleaseDir "lib$stem.dylib" }
    return Join-Path $ReleaseDir "lib$stem.so"
}

function Get-BinPath {
    param([string]$BinName)
    if ($IsWindows) { return Join-Path $ReleaseDir "$BinName.exe" }
    return Join-Path $ReleaseDir $BinName
}

# Path, budget (MB), and PLAN.md §8 row name for each checked artifact.
$Budgets = @(
    [pscustomobject]@{ Name = "relic-capi (core library)"; Path = (Get-CdylibPath "relic-capi"); BudgetMB = 4 },
    [pscustomobject]@{ Name = "relic-desktop (desktop binary)"; Path = (Get-BinPath "relic-desktop"); BudgetMB = 20 }
)

$Failed = $false
foreach ($b in $Budgets) {
    if (-not (Test-Path $b.Path)) {
        Write-Host ("MISSING  {0}: expected artifact not found at {1}" -f $b.Name, $b.Path) -ForegroundColor Red
        $Failed = $true
        continue
    }
    $sizeMB = (Get-Item $b.Path).Length / 1MB
    $overBudget = $sizeMB -gt $b.BudgetMB
    if ($overBudget) { $Failed = $true }
    $status = if ($overBudget) { "OVER" } else { "OK" }
    $color = if ($overBudget) { "Red" } else { "Green" }
    Write-Host ("{0,-6} {1,-32} {2,8:N2} MB  (budget {3} MB)" -f $status, $b.Name, $sizeMB, $b.BudgetMB) -ForegroundColor $color
}

if ($Failed) {
    Write-Host "`nSize budget check failed - see PLAN.md section 8." -ForegroundColor Red
    exit 1
}
Write-Host "`nAll size budgets met." -ForegroundColor Green
