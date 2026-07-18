# Relic — Android shell (alpha)

Kotlin + Jetpack Compose over the UniFFI bindings (`ffi/uniffi`), running the
same Rust engine as the desktop CLI. Status: **sideload alpha** — browse and
launch works; HOME-launcher role, controller-first focus, and the data-driven
intent-template engine (docs/android-intents.md) are still ahead.

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
5. Launching needs RetroArch (`com.retroarch` or `com.retroarch.aarch64`)
   installed with its cores downloaded; Relic passes the system's default core
   from the registry.

## Alpha shortcuts (tracked, will change)

- **All-files access** instead of SAF content-URI translation (ES-DE precedent
  for sideloaded launchers). The SAF flow per docs/android-intents.md replaces
  this before any store distribution.
- **RetroArch launch is hardcoded** (`RetroArchLauncher.kt`) rather than driven
  by `core/data/intents/*.toml`; the template engine lands with Phase 3 proper.
- Touch-first UI; controller focus navigation is the next shell milestone.
