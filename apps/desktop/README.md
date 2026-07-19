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
