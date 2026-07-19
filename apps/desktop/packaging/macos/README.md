# Relic Desktop — macOS .dmg packaging

Assembles `relic-desktop.app` from `target/release/relic-desktop` and the
`Info.plist` in this folder, then produces a compressed (UDZO) `.dmg`
with a drag-to-Applications layout via `hdiutil`.

Bundle id: `org.relic.desktop`. Version (`CFBundleShortVersionString` and
`CFBundleVersion`) tracks `workspace.package.version` in the root
`Cargo.toml` (currently `0.1.0`); bump both together when the workspace
version moves.

## Prerequisites

- macOS (uses `hdiutil`).
- Rust stable.

## Build

From repo root:

```sh
bash apps/desktop/packaging/macos/build-dmg.sh
```

Override the version label (defaults to `0.1.0`):

```sh
RELIC_VERSION=0.1.0 bash apps/desktop/packaging/macos/build-dmg.sh
```

Output: `target/dmg/relic-desktop-0.1.0.dmg`.

## Notarization

Not included in `build-dmg.sh` — Relic has no Apple Developer ID in this
repo. For a signed + notarized build, after `hdiutil create` run
`codesign --deep --options runtime --sign "Developer ID Application: …"`
on the `.app` inside the staging dir, then `xcrun notarytool submit …`
against the `.dmg`, then `xcrun stapler staple`. That requires credentials
not present in this repo and is intentionally left to the release owner.
