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
- **Session-end detection is `onResume`, not a real result contract**
  (docs/android-intents.md §5 step 10): every shipped template sets
  `FLAG_ACTIVITY_NEW_TASK`, which breaks `startActivityForResult`'s callback,
  so `MainActivity.onResume` — firing when Relic regains focus — is what ends
  the play session and revokes the ROM's `FileProvider` URI grant instead.
  Good enough for a foreground return; doesn't yet cover the emulator crashing
  without ever returning focus.
- Every screen requests an initial focus target on entry (first grid tile,
  or a fallback control when its list is empty) so a gamepad-only user with
  no touch input isn't stuck with dead D-pad input, and the two root screens
  (setup wizard, library) swallow the back button instead of falling through
  to the Activity's default `finish()` — required since Relic can be the
  device's Home app and there's nothing "behind" it to return to. Grid
  traversal itself still relies on Compose's default 2D focus search
  (untested on a physical gamepad/handheld); if cross-row jumps misbehave on
  odd screen widths, that's the likely next rough edge.
