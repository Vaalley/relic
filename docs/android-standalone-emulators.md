# Relic — Android Standalone Emulator Launch Intents

> Companion to `docs/android-intents.md` and `PLAN.md` §4.5. A research
> digest of the **external-launch intent interfaces** exposed by the major
> standalone (non-libretro) Android emulators, so the data-driven template
> engine under `core/data/intents/*.toml` can be seeded from primary sources
> rather than guesswork.
>
> Status: **Research, Phase 3.** This document does not ship a template per
> emulator; it records what each emulator's intent interface actually is (or
> isn't), with a citation per claim. Where a fact could not be confirmed from
> a primary source — the emulator's own `AndroidManifest.xml`, source code on
> GitHub/GitLab, or the project's official docs — it is marked `[VERIFY]`,
> matching the convention in `docs/retroachievements-design.md`.
>
> The five templates already shipped under `core/data/intents/` (RetroArch,
> PPSSPP, Dolphin, melonDS, DuckStation) are cross-checked here; where this
> document and a shipped template disagree, the template is wrong until
> updated. Those corrections are flagged inline as **TEMPLATE FIX:**.

---

## 1. Scope and method

For each emulator this document records:

- **Package** — Android application ID, including paid/beta/nightly variants.
- **Launch component** — the activity class that accepts an external launch
  intent (the one a frontend should target with an explicit `ComponentName`).
- **Intent action** — `android.intent.action.MAIN` or `…VIEW` (or a vendor
  action), and why.
- **ROM transport** — how the ROM reaches the emulator: as `Intent.data`
  (a `content://` or `file://` URI), or via a named string extra, and whether
  the value is a URI or a bare filesystem path. This distinction is critical
  under scoped storage: a bare path is unreadable to apps targeting Android
  11+, only `content://` URIs (with `FLAG_GRANT_READ_URI_PERMISSION`) work.
- **Required extras** — core, config, save-state resume, etc.
- **Returns to caller** — whether the emulator activity finishes back to the
  launching activity on exit (so Relic can revoke the URI grant and record
  the `play_session` end), or whether Relic must rely on a session watchdog.
- **Source** — a link per claim, preferring the emulator's `AndroidManifest.xml`
  or source over blog posts, Reddit, or frontend-config repos.

Where an emulator has **no documented external-launch intent at all**, that
is stated explicitly — it is valuable information and means Relic cannot
support that emulator without an upstream change or a bridge app.

The TOML field names below (`package`, `activity`, `action`, `data_mode`,
`data_extra_name`, `[[extras]]`) are the ones defined in
`docs/android-intents.md` §4. The final section maps each emulator to those
fields and proposes a priority order for template authoring.

---

## 2. Per-emulator findings

### 2.1 RetroArch + RetroArch AArch64 (all libretro systems)

Already shipped as `core/data/intents/retroarch.toml`; documented here for
completeness and to capture the AArch64 variant and a recent regression.

| Field | Value |
|---|---|
| Package | `com.retroarch` (32-bit/legacy) **and** `com.retroarch.aarch64` (AArch64, the recommended build on 64-bit devices) |
| Activity | `com.retroarch.browser.retroactivity.RetroActivityFuture` (same class in both packages) |
| Action | `android.intent.action.MAIN` |
| ROM transport | `ROM` string extra — value is a ROM path or URI. `data_mode = "extra"`, `data_extra_name = "ROM"`. |
| Required extras | `LIBRETRO` (string, libretro core), `CONFIGFILE` (string, optional — empty string uses RetroArch default), `QUITFOCUS` (bool — see "Returns to caller"). |
| Returns to caller | **Yes**, when `QUITFOCUS=true`: RetroArch finishes its activity and returns focus to the launcher. This is the only emulator in this digest with an explicit, documented return-to-caller flag. |
| Source | [libretro/RetroArch#13551](https://github.com/libretro/RetroArch/issues/13551) (minimal viable launch config + QUITFOCUS behavior); [libretro/RetroArch#14578](https://github.com/libretro/RetroArch/pull/14578) (RetroActivityFuture intent extras contract); [libretro/RetroArch#17433](https://github.com/libretro/RetroArch/issues/17433) (LIBRETRO path regression); [libretro/RetroArch#18555](https://github.com/libretro/RetroArch/issues/18555) (Android 15 ANR caveat with MAME core). |

**Caveats:**
- As of the 2025-01-17 AArch64 nightly, the `LIBRETRO` extra must be the
  **full path** to the core `.so` (e.g.
  `/data/data/com.retroarch.aarch64/cores/mesen_libretro_android.so`), not
  just the filename stem. The shipped `retroarch.toml` uses `{core}` which
  resolves to the stem — this will break on nightlies ≥ 2025-01-17 and
  should be revisited. **TEMPLATE FIX:** the `{core}` placeholder
  resolution (or the template) needs to produce a full core path for
  affected RetroArch builds. `[VERIFY]` whether stable channel is also
  affected.
- `RetroActivityFuture` is `singleTask`-ish: a second launch with
  `--activity-clear-top` while content is already running does nothing
  (issue #13551). Relic's session watchdog must treat "RetroArch already
  foreground" as a no-op, not a new session.
- Android 15 + MAME core via external launch can ANR with "no focused
  window" (issue #18555); lighter cores are unaffected. Not a template
  issue, but worth noting for support.

### 2.2 Dolphin (GameCube / Wii)

| Field | Value |
|---|---|
| Package | `org.dolphinemu.dolphinemu` |
| Activity | `org.dolphinemu.dolphinemu.ui.main.MainActivity` |
| Action | `android.intent.action.MAIN` — **not** `VIEW`. `MainActivity` does not export a `VIEW` intent-filter; sending `VIEW` raises `ActivityNotFoundException`. |
| ROM transport | `AutoStartFile` string extra — value is a `content://` URI (post-scoped-storage) or a bare path (legacy). `data_mode = "extra"`, `data_extra_name = "AutoStartFile"`. |
| Required extras | None beyond `AutoStartFile`. |
| Returns to caller | `[VERIFY]` — not explicitly documented; Dolphin does not expose a `QUITFOCUS`-equivalent. Relic should rely on its session watchdog + `onActivityResult` rather than assume a clean return. |
| Source | [dolphin-emu/dolphin#10272](https://github.com/dolphin-emu/dolphin/pull/10272) (scoped-storage rework: content URIs via `AutoStartFile`); [pegasus-frontend#1096](https://github.com/mmatyas/pegasus-frontend/issues/1096) (`MainActivity` rejects `VIEW`; `AutoStartFile` extra is the launch mechanism); [Daijishou#640](https://github.com/TapiocaFox/Daijishou/issues/640) (working `am start` with `MAIN` + `AutoStartFile` content URI); [handhelds.wtf Dolphin shortcut guide](https://handhelds.wtf/guides/create-dolphin-game-shortcuts-on-android) (Shortcut Maker recipe using `AutoStartFile`). |

**TEMPLATE FIX:** the shipped `dolphin.toml` uses
`action = "android.intent.action.VIEW"` with `data_mode = "data"` and no
extras. Per the sources above, the working interface is `MAIN` +
`AutoStartFile` extra carrying the content URI. The template should be
rewritten as:
```toml
action = "android.intent.action.MAIN"
data_mode = "extra"
data_extra_name = "AutoStartFile"
[[extras]]
name = "AutoStartFile"
type = "string"
value = "{rom_uri}"
```
The existing `# UNVERIFIED` markers on `activity` and `action` are
resolved by this research: `MainActivity` + `MAIN` is correct, `VIEW` is
not.

### 2.3 Dolphin MMJR2 (GameCube / Wii)

A performance-focused Dolphin fork (originally Weihuoya's MMJ, then MMJR,
now MMJR2; the active line is Medard22's VBI build).

| Field | Value |
|---|---|
| Package | `org.dolphinemu.mmjr` (note: the application ID is `.mmjr`, **not** `.mmjr2`, despite the fork's marketing name). Co-installs with official Dolphin. |
| Activity | `org.dolphinemu.dolphinemu.ui.main.MainActivity` `[VERIFY]` — the fork keeps Dolphin's package-internal class names; EmuDeck's setup script launches the same `MainActivity`. |
| Action | `android.intent.action.MAIN` `[VERIFY]` — inherited from Dolphin. |
| ROM transport | `AutoStartFile` string extra `[VERIFY]` — inherited from Dolphin. |
| Required extras | None beyond `AutoStartFile`. |
| Returns to caller | `[VERIFY]` — same caveat as Dolphin. |
| Source | [Medard22/Dolphin-MMJR2-VBI README](https://github.com/Medard22/Dolphin-MMJR2-VBI/) (package ID `org.dolphinemu.mmjr`); [EmuDeck Android_Dolphin.sh](https://github.com/dragoonDorise/EmuDeck/blob/02f6e613/android/functions/EmuScripts/Android_Dolphin.sh) (`am start -n org.dolphinemu.mmjr/org.dolphinemu.dolphinemu.ui.main.MainActivity`); [ApkPure listing](https://apkpure.com/dolphin-mmjr2/org.dolphinemu.mmjr) (confirms package). |

**Note:** MMJR2 is a fork that periodically rebases on Dolphin dev builds;
its intent interface tracks Dolphin's. If Dolphin changes `AutoStartFile`,
MMJR2 will follow within a release or two. Relic should ship a separate
`dolphin_mmjr.toml` (different `package`) rather than a `per_system`
override, since both emulators target the same systems and the user picks
one.

### 2.4 DuckStation (PS1)

| Field | Value |
|---|---|
| Package | `com.github.stenzek.duckstation` |
| Activity | `com.github.stenzek.duckstation.EmulationActivity` — **not** `MainActivity`. `MainActivity` is the game-list UI; `EmulationActivity` is the exported launch target. |
| Action | `android.intent.action.MAIN` |
| ROM transport | `bootPath` string extra — value is a ROM path or URI. `data_mode = "extra"`, `data_extra_name = "bootPath"`. |
| Required extras | `resumeState` (bool) — `true` to boot from last save state, `false` for cold boot. Optional but recommended. |
| Returns to caller | `[VERIFY]` — autosave/resume-state does **not** work when the game is loaded externally (per Daijishou #612, the feature was removed for external launches). Relic should treat `resumeState` as `false` and not promise save-state continuity. |
| Source | [pegasus-frontend#788](https://github.com/mmatyas/pegasus-frontend/issues/788) (canonical launch command: `EmulationActivity` + `bootPath` + `resumeState`); [Daijishou#612](https://github.com/TapiocaFox/Daijishou/issues/612) (working config with `{file.uri}` in `bootPath`; resumeState broken on external launch); [stenzek/duckstation commit 6e49adb](https://github.com/stenzek/duckstation/commit/6e49adb508c009c5f7998a38d47e1d0d174990d0) (Android app source tree, now removed from public repo — the app source is no longer in the GitHub repo, only desktop). |

**TEMPLATE FIX:** the shipped `duckstation.toml` targets
`com.github.stenzek.duckstation/.MainActivity` with `VIEW` + `data_mode =
"data"`. Per pegasus #788 and Daijishou #612, the working interface is
`EmulationActivity` + `MAIN` + `bootPath` extra. The template should be
rewritten as:
```toml
activity = "com.github.stenzek.duckstation.EmulationActivity"
action = "android.intent.action.MAIN"
data_mode = "extra"
data_extra_name = "bootPath"
[[extras]]
name = "bootPath"
type = "string"
value = "{rom_uri}"
[[extras]]
name = "resumeState"
type = "bool"
value = "false"
```

**Note on source availability:** DuckStation's Android app source was
removed from the public GitHub repo (commit 6e49adb). The intent interface
above is corroborated by multiple independent frontend configs (Pegasus,
Daijishou, ES-DE) rather than a current manifest read; treat the activity
class name as stable but verify on a device before relying on it.

### 2.5 PPSSPP & PPSSPP Gold (PSP)

Already shipped as `ppsspp.toml`; documented here for the Gold/Legacy
variants and to confirm the interface from primary sources.

| Field | Value |
|---|---|
| Package | `org.ppsspp.ppsspp` (free) **or** `org.ppsspp.ppssppgold` (Gold, paid) **or** `org.ppsspp.ppsspplegacy` (legacy build, pre-scoped-storage). |
| Activity | `org.ppsspp.ppsspp.PpssppActivity` (same class name in all three packages). |
| Action | `android.intent.action.VIEW` |
| ROM transport | `Intent.data` — a `content://` URI (content scheme explicitly allowed in the manifest intent-filter as of [commit 1e2c3f7](https://github.com/hrydgard/ppsspp/commit/1e2c3f7f30df12fd1f116152a96a4ca293fffe37)). `data_mode = "data"`. |
| Required extras | None. PPSSPP reads the ROM entirely from `Intent.data`. |
| Returns to caller | `[VERIFY]` — not explicitly documented. PPSSPP's pause-menu "exit" returns to the launcher activity stack; Relic should rely on `onActivityResult`. |
| Source | [PPSSPP front-end integration docs](https://www.ppsspp.org/docs/reference/front-end-integration/) (canonical: activity names for free/Gold/legacy, `VIEW` action); [hrydgard/ppsspp commit 1e2c3f7](https://github.com/hrydgard/ppsspp/commit/1e2c3f7f30df12fd1f116152a96a4ca293fffe37) (manifest adds `content` scheme to the `VIEW` intent-filter); [pegasus specials.txt](https://github.com/mmatyas/pegasus-android-appdb/blob/master/specials.txt) (confirms `VIEW` + `{file.documenturi}`). |

**Note:** the shipped `ppsspp.toml` is correct for the free package. To
support Gold, Relic should ship a second `ppsspp_gold.toml` (or use a
launch-profile priority list across the three package IDs — they are
mutually exclusive installs, so the profile picker can try them in order).

### 2.6 DraStic (NDS)

| Field | Value |
|---|---|
| Package | `com.dsemu.drastic` (Play Store paid). No free/beta variant. |
| Activity | `com.dsemu.drastic.DraSticActivity` |
| Action | `android.intent.action.VIEW` `[VERIFY]` — DraStic is closed-source; the manifest is not publicly readable. The `VIEW` action is inferred from working frontend configs (Pegasus, Daijishou). |
| ROM transport | `Intent.data` — a `file://` URI or bare path. `data_mode = "data"`. `[VERIFY]` whether `content://` URIs are accepted; reports suggest DraStic does not handle SAF content URIs well on Android 11+. |
| Required extras | None. |
| Returns to caller | `[VERIFY]`. |
| Source | [DraStic forum topic 15719](https://drastic-ds.com/viewtopic.php?t=15719) (Pegasus launch config using `VIEW` + `DraSticActivity`); [Daijishou#579](https://github.com/TapiocaFox/Daijishou/issues/579) (working config: `com.dsemu.drastic/.DraSticActivity` + `-d {file.uri}`, no `VIEW` action needed in some Android versions); [Play Store listing](https://play.google.com/store/apps/details?id=com.dsemu.drastic) (confirms package). |

**Caveats:**
- DraStic is closed-source and the developer is inactive; there is no
  official intent-interface documentation. All claims here are from
  community frontend configs.
- DraStic has well-known scoped-storage issues on Android 11+ (forum
  topic 15516: "Open with" does not offer DraStic; ROMs must live in a
  `NDS` folder inside DraStic's own storage). Launching via `content://`
  URI from a frontend is unreliable `[VERIFY]`.
- DraStic does not accept `.zip` ROMs via intent (Daijishou #579); ROMs
  must be unzipped `.nds` files.
- **Recommendation:** support DraStic only as a fallback behind melonDS
  in the NDS launch profile, and mark its template `# UNVERIFIED - needs
  device testing` throughout.

### 2.7 melonDS (NDS)

| Field | Value |
|---|---|
| Package | `me.magnum.melonds` (the active Android port by rafaelvcaetano / SapphireRhodonite lineage). |
| Activity | `me.magnum.melonds.ui.emulator.EmulatorActivity` |
| Action | `android.intent.action.VIEW` |
| ROM transport | `Intent.data` — a `content://` URI is the **preferred** path (the README explicitly says "Intent data (preferred) - a URI of the NDS ROM … Ensure read permission is granted"). `data_mode = "data"`. |
| Required extras | None for the preferred path. The `uri` (SAF URI string) and `PATH` (absolute path string) extras are **deprecated** but still accepted as fallbacks. |
| Returns to caller | `[VERIFY]` — not explicitly documented. |
| Source | [rafaelvcaetano/melonDS-android README](https://github.com/rafaelvcaetano/melonDS-android/blob/master/README.md) (canonical: package, activity, `VIEW` + intent data preferred, `uri`/`PATH` extras deprecated); [melonDS-emu/melonDS#2328](https://github.com/melonDS-emu/melonDS/issues/2328) (working `am start -n me.magnum.melonds/.ui.emulator.EmulatorActivity -a android.intent.action.VIEW -d {file_uri}`). |

**TEMPLATE FIX:** the shipped `melonds.toml` uses package
`io.github.melonds.melonds` and activity `MainActivity`. Both are wrong:
the real package is `me.magnum.melonds` and the real activity is
`ui.emulator.EmulatorActivity`. The template should be rewritten as:
```toml
package = "me.magnum.melonds"
activity = "me.magnum.melonds.ui.emulator.EmulatorActivity"
action = "android.intent.action.VIEW"
data_mode = "data"
```

**Caveat:** melonDS must have **scanned the ROM directory first** (its own
SAF tree grant) for save-file-next-to-ROM to work; otherwise saves fall
back to `Android/data/me.magnum.melonds/files/saves`. This is a UX note,
not a template issue — Relic's read-only URI grant is unaffected.

### 2.8 Mupen64Plus FZ (N64)

| Field | Value |
|---|---|
| Package | `org.mupen64plusae.v3.fzurita` (Play Store free) **or** `org.mupen64plusae.v3.fzurita.pro` (Pro, donation) **or** `org.mupen64plusae.v3.alpha` (F-Droid / upstream `mupen64plus-ae/mupen64plus-ae` build). |
| Activity | `paulscode.android.mupen64plusae.SplashActivity` (the exported launcher activity; `GalleryActivity` and `GameActivity` are not exported). |
| Action | `android.intent.action.VIEW` |
| ROM transport | `Intent.data` — historically a `file://` URI / bare path. `data_mode = "data"`. `[VERIFY]` whether `content://` URIs are accepted; the project has a `useLegacyFileBrowser` / `startSafFilePicker` split (discussion #1136) suggesting SAF support exists, but frontend launch via content URI is not documented. |
| Required extras | None. |
| Returns to caller | `[VERIFY]`. |
| Source | [fzurita/mupen64plus-ae build.gradle](https://github.com/fzurita/mupen64plus-ae/blob/master/app/build.gradle) (`applicationId = "org.mupen64plusae.v3.alpha"` upstream; fzurita's Play Store flavor overrides to `org.mupen64plusae.v3.fzurita` with `.pro` suffix); [fzurita/mupen64plus-ae commit 691083a](https://github.com/fzurita/mupen64plus-ae/commit/691083ae3722a9917d43eed2f6a20684f9906729) (free/pro flavor split, SplashActivity exported with intent-filter); [Pegasus community config](https://pastebin.com/A3PrHZ2i) (working `am start -n org.mupen64plusae.v3.fzurita/paulscode.android.mupen64plusae.SplashActivity -a android.intent.action.VIEW -d {file.path}`); [Play Store listing](https://play.google.com/store/apps/details?id=org.mupen64plusae.v3.fzurita) (confirms package). |

**Note:** the Pro variant's package is `org.mupen64plusae.v3.fzurita.pro`
(applicationIdSuffix `.pro` per commit 691083a). Relic should treat the
free and Pro packages as alternatives in the N64 launch profile priority
list.

### 2.9 AetherSX2 / NetherSX2 (PS2)

| Field | Value |
|---|---|
| Package | `xyz.aethersx2.android` (both — NetherSX2 is a binary patch of AetherSX2 4248 and **keeps the same package ID** so it can be installed over AetherSX2 and inherit the Play Store listing's data). |
| Activity | `xyz.aethersx2.android.EmulationActivity` |
| Action | `android.intent.action.MAIN` |
| ROM transport | `bootPath` string extra — value is a document URI (`content://`). `data_mode = "extra"`, `data_extra_name = "bootPath"`. |
| Required extras | None beyond `bootPath`. |
| Returns to caller | `[VERIFY]` — not documented. |
| Source | [pegasus specials.txt](https://github.com/mmatyas/pegasus-android-appdb/blob/master/specials.txt) (canonical: `xyz.aethersx2.android/.EmulationActivity`, action `MAIN`, `-e bootPath {file.documenturi}`); [NeoGameLab/neostation-systems README](https://github.com/NeoGameLab/neostation-systems/blob/main/README.md) (same: `MAIN` + `bootPath` extra); [AetherSX2 GitHub README](https://github.com/aethersx2/aethersx2) (confirms package `xyz.aethersx2.android`); [Trixarian/NetherSX2-patch README](https://github.com/Trixarian/NetherSX2-patch) (NetherSX2 is a patch on AetherSX2 4248, same package). |

**Note:** AetherSX2 is discontinued; NetherSX2 is the actively-maintained
line. Because both share the package ID, a single `aethersx2.toml`
template covers both — Relic does not need a separate NetherSX2 template.

### 2.10 Flycast (Dreamcast, Naomi, Atomiswave)

| Field | Value |
|---|---|
| Package | `com.flycast.emulator` |
| Activity | `com.flycast.emulator.MainActivity` (newer builds, post-2.2 — the package-internal class was renamed from `com.reicast.emulator.MainActivity` to `com.flycast.emulator.MainActivity`) **or** `com.reicast.emulator.NativeGLActivity` (older builds). Use `com.flycast.emulator.MainActivity` for current Play Store / GitHub releases. |
| Action | `android.intent.action.VIEW` |
| ROM transport | `Intent.data` — a `file://` URI works. `data_mode = "data"`. `[VERIFY]` whether `content://` URIs are accepted: issue #1764 states "flycast can't handle content:// urls" as of 2024, and issue #1658 shows a `file://` path failing on Android 14 scoped storage. The current state is unclear — Flycast may have added content-URI support since. |
| Required extras | None. |
| Returns to caller | `[VERIFY]`. |
| Source | [flyinghead/flycast#226](https://github.com/flyinghead/flycast/issues/226) (canonical `am start` with `VIEW` + `file://` URI on `NativeGLActivity`); [flyinghead/flycast#1584](https://github.com/flyinghead/flycast/issues/1584) (activity renamed to `com.flycast.emulator.MainActivity` in 2.2+); [flyinghead/flycast#1658](https://github.com/flyinghead/flycast/issues/1658) (`file://` path fails on Android 14); [flyinghead/flycast#1764](https://github.com/flyinghead/flycast/issues/1764) (content URI support unclear). |

**Caveat:** Flycast's content-URI support is the shakiest in this digest.
Before shipping a `flycast.toml`, verify on a device whether a SAF
`content://` URI in `Intent.data` actually boots the game on the current
Play Store build. If not, Flycast cannot be launched cleanly under
Relic's read-only-URI security model (§5 of `android-intents.md`) and
should be deferred until upstream adds content-URI support.

### 2.11 Redream (Dreamcast)

| Field | Value |
|---|---|
| Package | `io.recompiled.redream` (Play Store; commercial, closed source). |
| Activity | `io.recompiled.redream.MainActivity` `[VERIFY]` — multiple frontend reports of `ActivityNotFoundException` for this component on Android 13+ (Daijishou #487, #579), despite the same component working on older Android versions. The manifest is not publicly readable (closed source). |
| Action | `android.intent.action.VIEW` `[VERIFY]`. |
| ROM transport | `Intent.data` — a `content://` URI (`{file.uri}`). `data_mode = "data"`. `[VERIFY]` — the redream dev (Iolen) acknowledged "with saf paths it's going off the rails" (Daijishou #487). |
| Required extras | None. |
| Returns to caller | `[VERIFY]`. |
| Source | [Daijishou#487](https://github.com/TapiocaFox/Daijishou/issues/487) (launch config + redream dev comment on SAF path issues); [Daijishou#579](https://github.com/TapiocaFox/Daijishou/issues/579) (`ActivityNotFoundException` for `io.recompiled.redream/.MainActivity` on Android 13); [Play Store listing](https://play.google.com/store/apps/details?id=io.recompiled.redream) (confirms package). |

**Caveat:** Redream is closed-source and the manifest is not accessible.
The component name and intent interface are reconstructed from frontend
configs and bug reports, not a primary source. Launch reliability on
Android 13+ is poor per multiple reports. **Recommendation:** do not ship
a `redream.toml` until a primary source (the manifest, obtained by
inspecting the installed APK) confirms the exported activity and
intent-filter. List Redream in §4 (couldn't verify) until then.

### 2.12 Azahar / Lime3DS / Citra MMJ (3DS)

Three related projects; the active one is **Azahar** (a merge of PabloMK7's
Citra fork and Lime3DS). Lime3DS is superseded by Azahar but the Play Store
package ID was kept as `io.github.lime3ds.android` for listing continuity.

#### Azahar (was Lime3DS)

| Field | Value |
|---|---|
| Package | `io.github.lime3ds.android` (Play Store package — kept for listing continuity; the `applicationId` in `build.gradle.kts` is `org.azahar_emu.azahar` but the **installed package name** is `io.github.lime3ds.android`). `[VERIFY]` whether vanilla ( Obtainium / GitHub release) build uses a different installed package ID. |
| Activity | `org.citra.citra_emu.activities.EmulationActivity` (the activity class lives under the `org.citra.citra_emu` namespace, which is the `namespace` in `build.gradle.kts`; the component is `io.github.lime3ds.android/org.citra.citra_emu.activities.EmulationActivity`). |
| Action | `android.intent.action.VIEW` |
| ROM transport | `Intent.data` — a `content://` URI. `data_mode = "data"`. |
| Required extras | None. |
| Returns to caller | `[VERIFY]`. |
| Source | [azahar-emu/azahar#736](https://github.com/azahar-emu/azahar/issues/736) (canonical: `am start -n io.github.lime3ds.android/org.citra.citra_emu.activities.EmulationActivity -a android.intent.action.VIEW -d {file_uri}`); [azahar-emu/azahar#1484](https://github.com/azahar-emu/azahar/issues/1484) (es_find_rules entry confirms component; scoped-storage permission caveat — Relic's `FLAG_GRANT_READ_URI_PERMISSION` grant is the correct mitigation); [azahar-emu/azahar build.gradle.kts](https://github.com/azahar-emu/azahar/blob/ab6896a2/src/android/app/build.gradle.kts) (`namespace = "org.citra.citra_emu"`, `applicationId = "org.azahar_emu.azahar"`); [Play Store listing](https://play.google.com/store/apps/details?id=io.github.lime3ds.android) (confirms installed package `io.github.lime3ds.android`). |

**Caveat:** there is a known permission-denial error when the frontend
grants a content URI but Azahar tries to resolve sibling files (e.g.
`.exheader` next to a `.cxi`) — issue #1484. This is an upstream bug in
Azahar's content-URI handling, not a Relic bug; Relic's grant is correct.
Worth noting in user-facing troubleshooting docs.

#### Lime3DS (superseded by Azahar)

Same package (`io.github.lime3ds.android`) and same activity — Azahar
inherited the Lime3DS codebase and Play Store listing. A single
`azahar.toml` template covers both; do not ship a separate `lime3ds.toml`.

#### Citra MMJ (weihuoya fork)

| Field | Value |
|---|---|
| Package | `org.citra.emu` |
| Activity | `org.citra.emu.ui.EmulationActivity` (older versions) — **but** per weihuoya/citra#1039, as of the 20250221 build only `org.citra.emu.ui.MainActivity` is exported and it **ignores all extras and intent data**. `[VERIFY]` whether a recent build re-exported `EmulationActivity`. |
| Action | `android.intent.action.VIEW` (when `EmulationActivity` was exported). |
| ROM transport | `Intent.data` (content URI) — when `EmulationActivity` was exported. |
| Required extras | None. |
| Returns to caller | `[VERIFY]`. |
| Source | [weihuoya/citra#1039](https://github.com/weihuoya/citra/issues/1039) (regression: intent launching removed in 20250221 build; working config for older builds: `am start -n org.citra.citra_emu/org.citra.citra_emu.activities.EmulationActivity -a android.intent.action.VIEW -d {file.uri}`). |

**Caveat:** Citra MMJ's intent interface is unstable. The weihuoya fork
is less actively maintained than Azahar and has regressed on external
launch. **Recommendation:** do not ship a `citra_mmj.toml` unless a
device test confirms the current build exports a launch-capable activity.
Prefer Azahar for 3DS.

### 2.13 ScummVM (point & click)

ScummVM is the odd one out: it does **not** launch by ROM file. ScummVM
launches by **game target** — a short identifier configured in
`scummvm.ini` (e.g. `monkey1`, `loom-cd`). The intent interface reflects
this.

| Field | Value |
|---|---|
| Package | `org.scummvm.scummvm` |
| Activity | `org.scummvm.scummvm.ScummVMActivity` |
| Action | `android.intent.action.MAIN` |
| ROM transport | `Intent.data` using a **`scummvm:<target>` URI scheme** (constructed via `Uri.fromParts("scummvm", target, null)`), **not** a `content://` URI. The "ROM" is not a file at all — it is a target string naming a previously-added game. |
| Required extras | None. |
| Returns to caller | `[VERIFY]` — ScummVM resumes the game if already running. |
| Source | [scummvm/scummvm commit 4ecf2e4](https://github.com/scummvm/scummvm/commit/4ecf2e4ccc39adedb1dcd45f890d3520b9035342) (intent-to-start-a-specific-game implementation: `Uri.fromParts("scummvm", target, null)` + `ACTION_MAIN`); [scummvm/scummvm ScummVMActivity.java](https://github.com/scummvm/scummvm/blob/master/backends/platform/android/org/scummvm/scummvm/ScummVMActivity.java) (`setCurrentGame` builds the `scummvm:` URI); [scummvm/scummvm PR #5797](https://github.com/scummvm/scummvm/pull/5797) (launcher shortcut feature). |

**Does not fit Relic's template model.** The TOML schema in
`docs/android-intents.md` §4 assumes the ROM is a `content://` URI carried
via `{rom_uri}` (in `Intent.data` or a named extra). ScummVM needs:
1. A per-game **target string** stored in Relic's DB (not derivable from
   the file path — the user must add the game to ScummVM first, then tell
   Relic the target).
2. A way to build a `scummvm:<target>` URI and set it as `Intent.data`,
   with no `{rom_uri}` substitution.

This is a schema extension, not a template edit. **Recommendation:** defer
ScummVM support until the template format grows a `data_uri_scheme` /
`data_uri_value` field (or a per-game override mechanism for the target).
Until then, ScummVM cannot be launched by the data-driven engine without
per-game code. List ScummVM in §4 as "intent exists but does not fit the
current schema."

### 2.14 Yaba Sanshiro 2 (Saturn)

| Field | Value |
|---|---|
| Package | `org.devmiyax.yabasanshioro2.pro` (Pro, Play Store) **or** `org.devmiyax.yabasanshioro2` (free) `[VERIFY]` free package ID. (The older non-`2` line used `org.uoyabause.android` and `org.uoyabause.android.pro`.) |
| Activity | `org.uoyabause.android.Yabause` |
| Action | `android.intent.action.VIEW` — per the working ES-DE and Daijishou configs. `[VERIFY]` against the manifest: the dev (devmiyax) stated in Daijishou #562 that "YabaSanshiro only support `android.intent.action.MAIN` not `android.intent.action.VIEW`", yet the working community config uses `VIEW` + the `FileNameUri` extra. This may reflect a manifest change across versions; verify on the installed build. |
| ROM transport | `org.uoyabause.android.FileNameUri` string extra (newer builds, value is a `content://` URI) **or** `org.uoyabause.android.FileNameEx` string extra (older builds, value is a bare path). `data_mode = "extra"`, `data_extra_name = "org.uoyabause.android.FileNameUri"`. |
| Required extras | None beyond the filename extra. |
| Returns to caller | `[VERIFY]`. |
| Source | [Daijishou#562](https://github.com/TapiocaFox/Daijishou/issues/562) (working config + devmiyax comment on action + extra rename `FileNameEx`→`FileNameUri`); [devmiyax/yabause#983](https://github.com/devmiyax/yabause/issues/983) (scoped-storage file-access errors via frontend on Android 11+); [ES-DE issue 1664](https://gitlab.com/es-de/emulationstation-de/-/issues/1664) (working ES-DE config: `ACTION=android.intent.action.VIEW` + `EXTRA_org.uoyabause.android.FileNameUri=%ROMSAF%`); [pegasus specials.txt](https://github.com/mmatyas/pegasus-android-appdb/blob/master/specials.txt) (older config using `FileNameEx` + `{file.path}`). |

**Caveats:**
- `.bin/.cue` Saturn ROMs crash when launched from a frontend; only `.chd`
  works reliably (ES-DE issue 1664). This is an upstream bug.
- The extra name changed (`FileNameEx` → `FileNameUri`) across versions;
  the action (`MAIN` vs `VIEW`) is disputed between the dev and the
  working community configs. A `yabasanshiro2.toml` should target the
  **current** Pro build with `FileNameUri` + `VIEW`, and fall back to
  RetroArch's `mednafen_saturn` / `yabasanshiro` libretro core for
  `.bin/.cue`.
- `[VERIFY]` the free (non-Pro) package ID before shipping a template
  that covers both.

---

## 3. Mapping to `core/data/intents/*.toml` fields

The table below maps each emulator's findings to the TOML schema defined in
`docs/android-intents.md` §4. "Extras" lists the `[[extras]]` entries
beyond the ROM-carrying one. "Schema fit" notes whether the emulator fits
the current schema without extension.

| Emulator | `package` | `activity` | `action` | `data_mode` | `data_extra_name` | Extras | Schema fit |
|---|---|---|---|---|---|---|---|
| RetroArch | `com.retroarch` / `com.retroarch.aarch64` | `…RetroActivityFuture` | `MAIN` | `extra` | `ROM` | `LIBRETRO` (`{core}`), `CONFIGFILE`, `QUITFOCUS` (bool) | Yes (shipped; `{core}` path regression needs follow-up) |
| Dolphin | `org.dolphinemu.dolphinemu` | `…ui.main.MainActivity` | `MAIN` | `extra` | `AutoStartFile` | — | Yes (**template fix needed**) |
| Dolphin MMJR2 | `org.dolphinemu.mmjr` | `…ui.main.MainActivity` | `MAIN` | `extra` | `AutoStartFile` | — | Yes (new template) |
| DuckStation | `com.github.stenzek.duckstation` | `…EmulationActivity` | `MAIN` | `extra` | `bootPath` | `resumeState` (bool) | Yes (**template fix needed**) |
| PPSSPP | `org.ppsspp.ppsspp` / `…ppssppgold` / `…ppsspplegacy` | `…PpssppActivity` | `VIEW` | `data` | — | — | Yes (shipped; add Gold/Legacy variants) |
| DraStic | `com.dsemu.drastic` | `…DraSticActivity` | `VIEW` | `data` | — | — | Yes, but `[VERIFY]` throughout |
| melonDS | `me.magnum.melonds` | `…ui.emulator.EmulatorActivity` | `VIEW` | `data` | — | — | Yes (**template fix needed**) |
| Mupen64Plus FZ | `org.mupen64plusae.v3.fzurita` / `….pro` / `….alpha` | `paulscode.android.mupen64plusae.SplashActivity` | `VIEW` | `data` | — | — | Yes (`[VERIFY]` content-URI support) |
| AetherSX2 / NetherSX2 | `xyz.aethersx2.android` | `…EmulationActivity` | `MAIN` | `extra` | `bootPath` | — | Yes (new template) |
| Flycast | `com.flycast.emulator` | `com.flycast.emulator.MainActivity` | `VIEW` | `data` | — | — | Yes, but `[VERIFY]` content-URI support |
| Redream | `io.recompiled.redream` | `…MainActivity` | `VIEW` | `data` | — | — | Yes, but `[VERIFY]` throughout — defer |
| Azahar / Lime3DS | `io.github.lime3ds.android` | `org.citra.citra_emu.activities.EmulationActivity` | `VIEW` | `data` | — | — | Yes (new template) |
| Citra MMJ | `org.citra.emu` | `…ui.EmulationActivity` | `VIEW` | `data` | — | — | Yes, but intent launching regressed — defer |
| ScummVM | `org.scummvm.scummvm` | `…ScummVMActivity` | `MAIN` | `data` (scummvm: scheme) | — | — | **No** — needs schema extension for `scummvm:<target>` URIs and per-game target storage |
| Yaba Sanshiro 2 | `org.devmiyax.yabasanshioro2.pro` | `org.uoyabause.android.Yabause` | `VIEW` | `extra` | `org.uoyabause.android.FileNameUri` | — | Yes (`[VERIFY]` action + extra name across versions) |

---

## 4. Couldn't verify / deferred

Emulators or facts that could not be confirmed from a primary source and
should not be shipped as templates without device testing:

- **Redream** — closed source; manifest not accessible; the component
  `io.recompiled.redream/.MainActivity` is reconstructed from frontend
  configs and bug reports, with multiple `ActivityNotFoundException`
  reports on Android 13+. Do not ship a template until the manifest of an
  installed APK is inspected (e.g. via `aapt dump xmltree` or APK Analyzer).
- **Citra MMJ (weihuoya fork)** — intent launching was removed in the
  20250221 build (only `MainActivity` exported, extras ignored). Do not
  ship a template unless a device test on the current build confirms a
  launch-capable exported activity. Prefer Azahar for 3DS.
- **ScummVM** — has a documented intent interface, but it does not fit
  the current TOML schema (launches by `scummvm:<target>` URI, not by
  `content://` ROM URI). Deferred pending a schema extension for
  non-ROM-URI launch models and per-game target storage in Relic's DB.
- **DraStic** — closed source; all claims are from community frontend
  configs, not a manifest read. Scoped-storage issues on Android 11+ make
  `content://` URI launch unreliable. Ship only as a fallback behind
  melonDS, fully marked `# UNVERIFIED`.
- **Flycast content-URI support** — `file://` URIs work; `content://` URI
  support is unclear as of 2024 (issue #1764 says no, but the project may
  have added it since). Verify on the current Play Store build before
  shipping `flycast.toml`.
- **Yaba Sanshiro 2 action + extra name** — the dev states `MAIN` only;
  the working community config uses `VIEW` + `FileNameUri`. These may
  both be true across different versions. Verify on the installed Pro
  build before shipping. `.bin/.cue` ROMs crash from frontend launch
  (upstream bug); only `.chd` is reliable.
- **RetroArch `LIBRETRO` path regression** — the `{core}` placeholder
  resolves to a core filename stem, but RetroArch nightlies ≥ 2025-01-17
  require a full core path. `[VERIFY]` whether the stable channel is
  affected and whether the placeholder resolution needs to change.
- **Returns-to-caller behavior** for every emulator except RetroArch
  (`QUITFOCUS`) — none of the standalone emulators document an explicit
  return-to-launcher flag. Relic's session watchdog + `onActivityResult`
  is the only reliable session-end signal. `[VERIFY]` per emulator on a
  device.

---

## 5. Proposed priority order for Relic support

Ordered by (a) estimated user base on Android, (b) intent-interface
reliability and primary-source confidence, and (c) coverage of systems
not already served by the RetroArch fallback. Tier 1 is the recommended
seed set for the Phase 3 template engine beyond the five already shipped.

### Tier 1 — ship first (high confidence, high demand)

1. **DuckStation** (`duckstation.toml` **fix**) — PS1 is high-demand;
   the correct interface (`EmulationActivity` + `bootPath`) is
   well-corroborated. The shipped template is wrong and should be fixed
   in the same PR that lands this doc.
2. **melonDS** (`melonds.toml` **fix**) — NDS is high-demand; the correct
   package/activity is documented in the project's own README. The shipped
   template is wrong (wrong package, wrong activity) and should be fixed.
3. **Dolphin** (`dolphin.toml` **fix**) — GameCube/Wii is high-demand;
   the `AutoStartFile`-extra interface is confirmed by Dolphin's own PR
   #10272. The shipped template uses the wrong action (`VIEW`) and wrong
   `data_mode`; fix it.
4. **AetherSX2 / NetherSX2** (`aethersx2.toml` **new**) — PS2 is
   high-demand and not well covered by RetroArch on Android; the
   `bootPath`-extra interface is corroborated by Pegasus and ES-DE.
5. **Azahar** (`azahar.toml` **new**) — 3DS is high-demand; the
   `EmulationActivity` + `VIEW` + content-URI interface is confirmed by
   the project's own issue tracker.

### Tier 2 — ship after Tier 1 (medium confidence or niche)

6. **PPSSPP Gold / Legacy variants** — extend the shipped `ppsspp.toml`
   with Gold/Legacy package alternatives in the launch profile priority
   list (no new template needed, just profile config).
7. **Dolphin MMJR2** (`dolphin_mmjr.toml` **new**) — popular fork on
   low-end devices; same interface as Dolphin, different package.
8. **Mupen64Plus FZ** (`mupen64plus_fz.toml` **new**) — N64 is
   mid-demand; interface is `VIEW` + `Intent.data`. Verify content-URI
   support on a device first.
9. **Yaba Sanshiro 2** (`yabasanshiro2.toml` **new**) — Saturn is
   niche but has no better standalone option on Android; RetroArch's
   `mednafen_saturn` core is the fallback. Verify action + extra name
   on current Pro build first.

### Tier 3 — defer until verified

10. **Flycast** — defer until content-URI support is confirmed on the
    current build. Dreamcast is currently served by RetroArch's
    `flycast` libretro core as a fallback.
11. **DraStic** — defer behind melonDS (Tier 1); closed source and
    scoped-storage-fragile. Ship only as a fallback NDS profile, fully
    `# UNVERIFIED`.
12. **Redream** — defer until the manifest is inspected from an
    installed APK. Dreamcast is served by Flycast/RetroArch in the
    meantime.
13. **Citra MMJ** — defer; intent launching regressed upstream. Azahar
    (Tier 1) covers 3DS.
14. **ScummVM** — defer pending a TOML schema extension for
    non-`content://`-URI launch models (the `scummvm:<target>` scheme)
    and per-game target storage in Relic's DB.

### Cross-cutting follow-ups

- **Fix the three shipped templates** (DuckStation, melonDS, Dolphin) per
  the **TEMPLATE FIX** notes in §2. These are factual corrections from
  primary sources, not preference changes.
- **RetroArch `{core}` path regression** — investigate whether the
  `{core}` placeholder should resolve to a full core path for RetroArch
  nightlies ≥ 2025-01-17, and whether the stable channel is affected.
- **Session-end signal** — none of the standalone emulators document a
  `QUITFOCUS`-equivalent. Relic's session watchdog (PLAN.md §4.3) is the
  only reliable session-end mechanism for all non-RetroArch emulators;
  document this in `apps/android/README.md` when the Phase 3 resolver
  ships.
