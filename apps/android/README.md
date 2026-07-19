# Relic — Android shell (alpha)

Kotlin + Jetpack Compose over the UniFFI bindings (`ffi/uniffi`), running the
same Rust engine as the desktop CLI. Status: **sideload alpha** — browse,
game detail, favorites, search, and launch work; controller navigation is in
(d-pad moves focus, A confirms, B backs out) and the library rescans
incrementally on resume, and Relic can be set as the device's default Home
app. Launching is now data-driven from `core/data/intents/*.toml`
(`IntentLauncher.kt`, docs/android-intents.md) — RetroArch and the standalone
emulators in the built-in template set all launch through the same resolver,
trying candidates for a game's system in order and firing whichever package
is installed.

## Build

Prereqs: Android SDK (platform 36) + NDK, JDK 17+, Rust with the
`aarch64-linux-android` target, `cargo install cargo-ndk`.

```powershell
pwsh -File tools/android/build-apk.ps1          # debug APK
pwsh -File tools/android/build-apk.ps1 -Release
```

The script cross-compiles `relic-ffi` (arm64 + x86_64), regenerates the Kotlin
bindings, and runs Gradle. Output: `apps/android/app/build/outputs/apk/`.

## Install on a handheld (AYN Thor & friends)

1. Enable Developer options + USB debugging on the device.
2. `adb install -r app-debug.apk`
3. Put ROMs in per-system folders on the device, e.g. `/storage/emulated/0/ROMs/snes/…`
   (slugs listed in `core/data/systems/`).
4. Open Relic → grant "All files access" → Scan.
5. Launching tries each installed emulator template for the game's system in
   priority order (RetroArch first); RetroArch needs its cores downloaded, and
   Relic passes the system's default core from the registry.

## Alpha shortcuts (tracked, will change)

- **All-files access** instead of SAF content-URI translation (ES-DE precedent
  for sideloaded launchers). The SAF flow per docs/android-intents.md replaces
  this before any store distribution. Launch still works today because
  `IntentLauncher` mints a `FileProvider` content:// URI from the plain file
  path this access model already gives it — the ROM grant to the emulator is
  correctly scoped either way, only *Relic's own* library reads are broader
  than the eventual SAF model.
- **URI grant revocation isn't wired to session end** (docs/android-intents.md
  §5 step 10) — there's no session-lifecycle watchdog yet. The grant is
  scoped to one file and one package and drops when Relic's process dies.
- Touch-first UI; controller focus navigation is the next shell milestone.
