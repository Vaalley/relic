# Relic — desktop shell (early Phase 2)

Rust + Slint over the same `relic-core::api::Engine` the CLI uses (ADR-002,
`docs/adr/0002-desktop-ui-stack.md`). Status: a real, functional browser, not
yet the polished Phase 2 target — pick a system, search/browse its games,
toggle favorites, add a library folder, configure an emulator + launch
profile per system, and double-click a game to play it. What's still ahead:
a detail page, gamepad/keyboard navigation, design-token theming, a proper
first-run wizard, and packaging (MSI/dmg/flatpak/AppImage).

## Run

```powershell
cargo run -p relic-desktop
```

The library database is a real file under the OS data dir
(`dirs::data_dir()/relic/library.db` — e.g. `%APPDATA%\relic\library.db` on
Windows), so libraries added via "Add Folder…" persist across runs. Debug
builds seed `fixtures/mini` once if that database has no games yet, so the
window isn't empty before you've pointed it at a real library.

## Alpha shortcuts (tracked, will change)

- **No detail page, no gamepad input, no theming.** Everything so far is
  functional-but-plain: default Slint widget styling, mouse/keyboard only.
- **One emulator/profile per system at a time** — the config form always
  overwrites priority 0; there's no UI yet for multiple profiles or picking
  between them.
- **Launch is synchronous** (`Engine::launch` blocks the UI thread until the
  emulator exits) — acceptable for now since Relic itself isn't meant to be
  interacted with while a game is running, but worth revisiting for
  responsiveness (e.g. a "Relic is paused" state) before 1.0.

## Packaging

PLAN.md §9 Phase 2 exit criteria calls for MSI (Windows), dmg (macOS),
flatpak and AppImage (Linux). Sources and per-platform build scripts live
under `apps/desktop/packaging/`; the CI workflow
`.github/workflows/release.yml` builds all four on `workflow_dispatch`
only (manually triggered, never automatic — no git tags, no GitHub
Release, no registry publish; artifacts stay as workflow-run artifacts).

Each platform assumes `cargo build --release -p relic-desktop` has
produced `target/release/relic-desktop` (or `.exe` on Windows) first.

### Windows — MSI

WiX Toolset v4 source + Start Menu shortcut. See
`packaging/windows/README.md` for prerequisites and the exact `wix build`
/ `cargo wix` invocation.

### macOS — dmg

`packaging/macos/build-dmg.sh` builds the release binary, assembles
`relic-desktop.app` (bundle id `org.relic.desktop`, version from
`Info.plist`), and runs `hdiutil` to produce a compressed dmg with a
drag-to-Applications layout. See `packaging/macos/README.md`. Notarization
is intentionally left to the release owner (no Apple Developer ID in
repo).

### Linux — flatpak

`packaging/linux/flatpak/org.relic.Relic.yml` is a flatpak-builder
manifest against `org.freedesktop.Platform//24.08` that builds
`relic-desktop` via cargo inside the sandbox and ships the
`org.relic.Relic.desktop` + `org.relic.Relic.metainfo.xml` (AppStream)
files alongside it. Per PLAN.md §1 hard rule #1, the manifest requests
**no** `--share=network` finish-arg — the desktop shell is fully
functional offline.

### Linux — AppImage

`packaging/linux/appimage/build-appimage.sh` stages an `AppDir` (binary,
`.desktop`, `AppRun` shim) and runs `appimagetool` to produce
`target/appimage/relic-desktop-<version>-x86_64.AppImage`. Requires
`appimagetool` on `$PATH` (or `APPIMAGETOOL=/path/to/it`). See the script
header for details.
