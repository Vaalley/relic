<#
.SYNOPSIS
  PowerShell 7 end-to-end regression test for the relic CLI.
  Can be run locally and in CI on Windows, Linux, and macOS.
.DESCRIPTION
  Builds relic-cli, creates a temporary database, runs a sequence
  of CLI commands against the synthetic fixtures/mini library,
  asserts behavior/outputs, and cleans up.
.PARAMETER Release
  If set, builds and tests the release target instead of debug.
#>
param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"

# Detect OS
if ($null -eq $IsWindows) {
    $IsWindows = $env:OS -like "*Windows*" -or $env:OS -eq "Windows_NT"
}

# Resolve paths
$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$FixturesMini = [System.IO.Path]::GetFullPath((Join-Path $RepoRoot "fixtures" "mini"))

# Locate binary
$BinDir = if ($Release) { "release" } else { "debug" }
$BinName = if ($IsWindows) { "relic.exe" } else { "relic" }
$RelicBin = [System.IO.Path]::GetFullPath((Join-Path $RepoRoot "target" $BinDir $BinName))

# Build CLI
Write-Host "Building relic-cli in $BinDir mode..." -ForegroundColor Cyan
Push-Location $RepoRoot
try {
    if ($Release) {
        cargo build -p relic-cli --release
    } else {
        cargo build -p relic-cli
    }
    if ($LastExitCode -ne 0) {
        Write-Host "Cargo build failed!" -ForegroundColor Red
        exit 1
    }
}
finally {
    Pop-Location
}

if (-not (Test-Path $RelicBin)) {
    Write-Host "Could not find built binary at $RelicBin" -ForegroundColor Red
    exit 1
}

# Create scratch directory
$TempDir = [System.IO.Path]::GetTempPath()
$RandomSuffix = [System.IO.Path]::GetRandomFileName()
$ScratchDir = Join-Path $TempDir "relic_e2e_$RandomSuffix"
$null = New-Item -ItemType Directory -Path $ScratchDir -Force
$DbPath = Join-Path $ScratchDir "test.db"

Write-Host "Scratch directory: $ScratchDir" -ForegroundColor Gray
Write-Host "Database path: $DbPath" -ForegroundColor Gray
Write-Host ""

$script:FailureCount = 0
$script:PassedCount = 0

function Run-Step {
    param (
        [string]$Name,
        [string[]]$ArgsList,
        [int]$ExpectedExitCode = 0,
        [string[]]$Contains = @(),
        [string[]]$NotContains = @(),
        [switch]$ExpectEmptyOutput
    )

    $stdoutFile = [System.IO.Path]::GetTempFileName()
    $stderrFile = [System.IO.Path]::GetTempFileName()

    try {
        # Execute binary and redirect output to files
        & $RelicBin @ArgsList > $stdoutFile 2> $stderrFile
        $exitCode = $LastExitCode

        $stdout = [System.IO.File]::ReadAllText($stdoutFile)
        $stderr = [System.IO.File]::ReadAllText($stderrFile)

        $failed = $false
        $reasons = [System.Collections.Generic.List[string]]::new()

        # Check exit code
        if ($ExpectedExitCode -eq -1) {
            if ($exitCode -eq 0) {
                $failed = $true
                $reasons.Add("Expected non-zero exit code, but got 0")
            }
        } else {
            if ($exitCode -ne $ExpectedExitCode) {
                $failed = $true
                $reasons.Add("Expected exit code $ExpectedExitCode, but got $exitCode")
            }
        }

        # Check contains
        foreach ($item in $Contains) {
            if (-not $stdout.Contains($item)) {
                $failed = $true
                $reasons.Add("Expected stdout to contain '$item'")
            }
        }

        # Check not contains
        foreach ($item in $NotContains) {
            if ($stdout.Contains($item)) {
                $failed = $true
                $reasons.Add("Expected stdout NOT to contain '$item'")
            }
        }

        # Check empty output
        if ($ExpectEmptyOutput -and $stdout.Trim() -ne "") {
            $failed = $true
            $reasons.Add("Expected stdout to be empty, but got non-empty output")
        }

        if ($failed) {
            $script:FailureCount++
            Write-Host "FAIL: $Name" -ForegroundColor Red
            Write-Host "Command: relic $($ArgsList -join ' ')" -ForegroundColor Yellow
            Write-Host "Exit Code: $exitCode" -ForegroundColor Yellow
            if ($stdout.Trim() -ne "") {
                Write-Host "--- STDOUT ---" -ForegroundColor Cyan
                Write-Host $stdout
            }
            if ($stderr.Trim() -ne "") {
                Write-Host "--- STDERR ---" -ForegroundColor DarkRed
                Write-Host $stderr
            }
            foreach ($reason in $reasons) {
                Write-Host "  Reason: $reason" -ForegroundColor Red
            }
            Write-Host ""
        } else {
            $script:PassedCount++
            Write-Host "PASS: $Name" -ForegroundColor Green
        }
    }
    catch {
        $script:FailureCount++
        Write-Host "FAIL: $Name" -ForegroundColor Red
        Write-Host "An exception occurred: $_" -ForegroundColor Red
        Write-Host ""
    }
    finally {
        if (Test-Path $stdoutFile) { Remove-Item $stdoutFile -ErrorAction Ignore }
        if (Test-Path $stderrFile) { Remove-Item $stderrFile -ErrorAction Ignore }
    }
}

try {
    # (1) 'relic scan --db DB fixtures/mini' exits 0 and stdout contains 'added=3'
    Run-Step -Name "(1) Scan library" -ArgsList @("scan", "--db", $DbPath, $FixturesMini) -Contains @("added=3")

    # (2) rescan exits 0 and contains 'added=0' and 'unchanged=3'
    Run-Step -Name "(2) Rescan library" -ArgsList @("scan", "--db", $DbPath, $FixturesMini) -Contains @("added=0", "unchanged=3")

    # (3) 'relic import-gamelists --db DB fixtures/mini' contains 'matched=1 unmatched=0'
    Run-Step -Name "(3) Import gamelists" -ArgsList @("import-gamelists", "--db", $DbPath, $FixturesMini) -Contains @("matched=1 unmatched=0")

    # (4) 'relic games --db DB' contains 'Super Mario World' and does NOT contain 'Super Mario World (USA)' and contains 'Contra (USA)' and 'Tetris (World)'
    Run-Step -Name "(4) List games" -ArgsList @("games", "--db", $DbPath) -Contains @("Super Mario World", "Contra (USA)", "Tetris (World)") -NotContains @("Super Mario World (USA)")

    # (5) 'relic games --db DB --search zelda' outputs nothing (no match)
    Run-Step -Name "(5) Search for zelda" -ArgsList @("games", "--db", $DbPath, "--search", "zelda") -ExpectEmptyOutput

    # (6) 'relic emulator-add --db DB noop SOMEEXEC' exits 0 where SOMEEXEC is cmd on Windows else true
    $SomeExec = if ($IsWindows) { "cmd" } else { "true" }
    Run-Step -Name "(6) Register emulator" -ArgsList @("emulator-add", "--db", $DbPath, "noop", $SomeExec)

    # (7) 'relic profile-add --db DB noop snes TEMPLATE' with TEMPLATE '/C echo {rom}' on Windows else '{rom}' exits 0
    $Template = if ($IsWindows) { "/C echo {rom}" } else { "{rom}" }
    Run-Step -Name "(7) Attach launch profile" -ArgsList @("profile-add", "--db", $DbPath, "noop", "snes", $Template)

    # (8) 'relic launch --db DB 2 --dry-run' exits 0 and stdout contains 'Super Mario World (USA).sfc'
    Run-Step -Name "(8) Dry-run launch game 2" -ArgsList @("launch", "--db", $DbPath, "2", "--dry-run") -Contains @("Super Mario World (USA).sfc")

    # (9) 'relic profile-add --db DB noop snes {bogus}' exits NONZERO (bad placeholder rejected)
    Run-Step -Name "(9) Bad placeholder rejected" -ArgsList @("profile-add", "--db", $DbPath, "noop", "snes", "{bogus}") -ExpectedExitCode -1

    # (10) 'relic doctor --db DB' exits 0 and contains OK
    Run-Step -Name "(10) Run doctor check" -ArgsList @("doctor", "--db", $DbPath) -Contains @("OK")

    # (11) 'relic intents' lists the built-in Android intent templates
    Run-Step -Name "(11) List intent templates" -ArgsList @("intents") -Contains @("retroarch", "duckstation")

    # (12) 'relic intent-validate' validates every shipped template clean
    Run-Step -Name "(12) Validate intent templates" -ArgsList @("intent-validate") -Contains @("OK    retroarch") -NotContains @("FAIL")

    # (13) 'relic export-gamelists' writes gamelist.xml back out. Runs against
    # a scratch copy of fixtures/mini, never the checked-in fixture itself —
    # export overwrites <root>/<system>/gamelist.xml in place.
    $ExportLib = Join-Path $ScratchDir "export-lib"
    Copy-Item -Recurse $FixturesMini $ExportLib
    $ExportDb = Join-Path $ScratchDir "export.db"
    & $RelicBin scan --db $ExportDb $ExportLib | Out-Null
    & $RelicBin import-gamelists --db $ExportDb $ExportLib | Out-Null
    Run-Step -Name "(13) Export gamelists" -ArgsList @("export-gamelists", "--db", $ExportDb, $ExportLib) -Contains @("wrote 3 gamelist.xml")
    $ExportedXml = Get-Content (Join-Path $ExportLib "snes" "gamelist.xml") -Raw
    if ($ExportedXml -notmatch "Super Mario World" -or $ExportedXml -notmatch "<genre>Platform</genre>") {
        $script:FailureCount++
        Write-Host "FAIL: (13b) Exported snes/gamelist.xml content" -ForegroundColor Red
    } else {
        $script:PassedCount++
        Write-Host "PASS: (13b) Exported snes/gamelist.xml content" -ForegroundColor Green
    }
}
finally {
    if (Test-Path $ScratchDir) {
        Remove-Item -Recurse -Force $ScratchDir -ErrorAction Ignore
    }
}

# Print summary
$Total = $script:PassedCount + $script:FailureCount
Write-Host "e2e: $script:PassedCount/$Total passed"

if ($script:FailureCount -gt 0) {
    exit 1
} else {
    exit 0
}
