# Relic Binary Size Budget Check

Checks release build artifacts against the binary-size budgets in
[PLAN.md](../../PLAN.md#8-performance-size--reliability-budgets):

| Artifact | Budget |
|---|---|
| `relic-capi` cdylib (core library embedded by shells/third parties) | < 4 MB |
| `relic-desktop` binary | < 20 MB |

The Android APK budget (< 15 MB) isn't checked here — it needs the Android
SDK/NDK toolchain from `tools/android/build-apk.ps1` and is tracked separately.

## Usage

```powershell
pwsh -File tools/size-check/run.ps1              # builds release, then checks
pwsh -File tools/size-check/run.ps1 -SkipBuild    # checks target/release as-is
```

Exits non-zero (and prints which artifact) if anything is missing or over
budget. Runs in CI on every push/PR (`.github/workflows/ci.yml`, `size-budgets`
job).
