# Build the Relic Android APK end to end:
#   1. cargo-ndk cross-compiles relic-ffi to librelic_ffi.so (arm64 + x86_64)
#   2. UniFFI regenerates the Kotlin bindings
#   3. Gradle assembles the APK
# Usage: pwsh -File tools/android/build-apk.ps1 [-Release] [-GradleBin <path>]
param(
    [switch]$Release,
    # Falls back to the wrapper once committed; a plain gradle install works too.
    [string]$GradleBin = ''
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
Push-Location $repo
try {
    if (-not $env:ANDROID_HOME) { $env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk" }
    $ndk = Get-ChildItem "$env:ANDROID_HOME\ndk" | Sort-Object Name -Descending | Select-Object -First 1
    if (-not $ndk) { throw "no NDK under $env:ANDROID_HOME\ndk" }
    $env:ANDROID_NDK_HOME = $ndk.FullName

    $jni = 'apps/android/app/src/main/jniLibs'
    $profileArgs = if ($Release) { @('--release') } else { @() }
    cargo ndk -t arm64-v8a -t x86_64 -o $jni build -p relic-ffi @profileArgs
    if ($LASTEXITCODE) { throw 'cargo-ndk build failed' }

    pwsh -File ffi/uniffi/generate-kotlin.ps1
    if ($LASTEXITCODE) { throw 'binding generation failed' }

    if (-not $GradleBin) {
        $wrapper = 'apps/android/gradlew.bat'
        $GradleBin = if (Test-Path $wrapper) { (Resolve-Path $wrapper).Path } else { 'gradle' }
    }
    Push-Location apps/android
    try {
        $task = if ($Release) { 'assembleRelease' } else { 'assembleDebug' }
        & $GradleBin $task --no-daemon
        if ($LASTEXITCODE) { throw "gradle $task failed" }
    }
    finally { Pop-Location }

    $apk = Get-ChildItem apps/android/app/build/outputs/apk -Recurse -Filter *.apk |
        Select-Object -First 1
    Write-Host "APK: $($apk.FullName)" -ForegroundColor Green
    Write-Host "Install: adb install -r `"$($apk.FullName)`""
}
finally { Pop-Location }
