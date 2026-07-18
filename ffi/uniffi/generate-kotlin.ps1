# Generate Kotlin bindings from the compiled relic-ffi cdylib (library mode).
# Output lands in ffi/uniffi/out/kotlin — consumed by apps/android (Phase 3).
$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
Push-Location $repo
try {
    cargo build -p relic-ffi; if ($LASTEXITCODE) { throw 'build failed' }
    $lib = if ($IsWindows) { 'target/debug/relic_ffi.dll' }
    elseif ($IsMacOS) { 'target/debug/librelic_ffi.dylib' }
    else { 'target/debug/librelic_ffi.so' }
    cargo run -p relic-ffi --features cli --bin uniffi-bindgen -- `
        generate --library $lib --language kotlin --out-dir ffi/uniffi/out/kotlin
    if ($LASTEXITCODE) { throw 'bindgen failed' }
    Get-ChildItem -Recurse ffi/uniffi/out/kotlin -Filter *.kt |
        ForEach-Object { Write-Host "generated: $($_.FullName)" }
}
finally {
    Pop-Location
}
