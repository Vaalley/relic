# The full verification gate every change must pass (AGENTS.md, PLAN.md §8).
# Usage: .\.agents\verify.ps1   (exit code 0 = green)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
Push-Location $repo
try {
    cargo fmt --all --check; if ($LASTEXITCODE) { throw 'fmt' }
    cargo clippy --workspace --all-targets -- -D warnings; if ($LASTEXITCODE) { throw 'clippy' }
    cargo test --workspace; if ($LASTEXITCODE) { throw 'test' }
    cargo build -p relic-core --no-default-features; if ($LASTEXITCODE) { throw 'offline-only build' }
    Write-Host "verify: ALL GREEN" -ForegroundColor Green
}
catch {
    Write-Host "verify: FAILED at $_" -ForegroundColor Red
    exit 1
}
finally {
    Pop-Location
}
