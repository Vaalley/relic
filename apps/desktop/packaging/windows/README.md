# Relic Desktop — Windows MSI packaging

WiX Toolset v4 source for an MSI that installs `relic-desktop.exe` into
`%ProgramFiles%\Relic\bin\` and adds a Start Menu shortcut under
`Relic → Relic Desktop`.

## Prerequisites

- Rust stable + `cargo build --release -p relic-desktop` produces
  `target/release/relic-desktop.exe`.
- One of:
  - **WiX v4 CLI** (`wix.exe`) — `dotnet tool install --global wix`,
    or download from <https://wixtoolset.org/releases/>.
  - **cargo-wix** (wraps WiX v3) — `cargo install cargo-wix`.

## Build

Set `CARGO_TARGET_DIR` so the source's `$(env.CARGO_TARGET_DIR)\release\…`
reference resolves (defaults to the workspace `target/`):

```powershell
# From repo root:
cargo build --release -p relic-desktop

# Option A — WiX v4 CLI:
$env:CARGO_TARGET_DIR = "target"
wix build -o target/wix/relic-desktop-0.1.0.msi `
  apps/desktop/packaging/windows/main.wxs

# Option B — cargo-wix:
cargo wix -p relic-desktop `
  --output target/wix/relic-desktop-0.1.0.msi `
  --input apps/desktop/packaging/windows/main.wxs
```

The resulting `target/wix/relic-desktop-0.1.0.msi` is the installable
artifact. The version `0.1.0` matches `workspace.package.version` in the
root `Cargo.toml`; bump it there and in `main.wxs` together.
