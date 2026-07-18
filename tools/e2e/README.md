# Relic CLI End-to-End Tests

This directory contains the end-to-end regression test suite for the Relic CLI (`relic`).

## What it Covers

The script runs a suite of assertions against a built `relic` binary using the synthetic `fixtures/mini` library:
1. **Initial Scan**: Checks that scan adds exactly 3 games from `fixtures/mini`.
2. **Rescan**: Checks that rescanning is idempotent (adds 0, unchanged 3).
3. **Import Gamelists**: Verifies metadata matching from `gamelist.xml` (matched=1 unmatched=0).
4. **Games Query**: Verifies canonical rename (Super Mario World) and the existence of other games.
5. **Empty Search**: Verifies searches with no match return empty output.
6. **Emulator Registration**: Adds a dummy emulator (`cmd` on Windows, `true` on other OSes).
7. **Profile Association**: Links the emulator to a system launch profile.
8. **Dry-Run Launch**: Verifies dry-run launch of a game generates the correct command-line.
9. **Validation of Placeholders**: Verifies that invalid template placeholders are rejected.
10. **Doctor Command**: Validates the health integrity check.

## Usage

You can run the tests locally with the following command from the repository root:

```powershell
pwsh -File tools/e2e/run.ps1
```

To run against the release target build:

```powershell
pwsh -File tools/e2e/run.ps1 -Release
```

## Continuous Integration (CI)

This end-to-end test suite is automatically executed on CI for every pull request and push.
*Note: Do not edit the `.github/` folder directly to modify its integration; it has been wired into CI workflows by the maintainer.*
