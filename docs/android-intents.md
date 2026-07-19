# Relic — Android Intent Templates

> Companion to `PLAN.md` §4.5 and `apps/android/README.md`. Spec for the
> per-emulator intent templates the Android shell uses to launch games.
> Status: **Draft, Phase 3** — authoritative once the Android shell's intent
> resolver ships in Phase 3. The templates under `core/data/intents/` are now
> parsed and validated at build time by `relic-core::intents` (embedded via
> `include_str!`, same pattern as `core/data/systems/`) and checkable with
> `relic-cli intent-validate`, but no shell yet *fires* an `Intent` from them
> — `apps/android` still hardcodes RetroArch (`RetroArchLauncher.kt`). Fields
> may move or rename until the resolver lands.

---

## 1. Status

**Draft, Phase 3.** This document defines the intent-template format described
in `PLAN.md` §4.5 (Android launching). It is the contract between three
consumers:

- `apps/android` — the shell that resolves a template at launch time and fires
  the resulting explicit `Intent`.
- `core/data/intents/*.toml` — the built-in templates, community-updateable as
  data files without an app release (PLAN.md §4.5).
- `relic-cli intent-validate` (`core/src/intents/mod.rs`) — the validator that
  enforces the rules in §6, and `relic-cli intents` to list the built-in set.

The templates shipped here are the canonical examples and the seed set called
for in `PLAN.md` §4.5, since expanded past the original five (RetroArch,
PPSSPP, Dolphin, melonDS, DuckStation) with the Tier 1/2 emulators identified
by the research digest at `docs/android-standalone-emulators.md`. Where this
spec and a shipped template disagree, the spec is wrong until updated; the
template is the source of truth for the current provisional shape.

These files are **data**: adding a template means adding a file plus one
`include_str!` line in `core/src/intents/mod.rs`'s `BUILTIN` array (mirroring
`core/data/systems/`'s `BUILTIN`), not a schema or engine change. `relic-core`
parses and validates every shipped template in its own test suite
(`cargo test -p relic-core intents::`), but nothing yet *acts* on them at
runtime — no shell builds or fires an `Intent` from a template. Until the
Android resolver lands, they remain best-effort captures of each emulator's
real Android intent interface, marked `# UNVERIFIED - needs device testing`
where the interface is uncertain.

---

## 2. Goals and non-goals

### Goals

- Let Relic launch a ROM in an external emulator on Android by building an
  explicit `Intent` from a data file, with no per-emulator code in the shell.
- Keep the template format narrow enough to validate and safe to load
  untrusted: no expressions, no scripting, no arbitrary string concatenation,
  no filesystem paths the shell doesn't already hold.
- Make templates community-updateable: a user or contributor can add support
  for a new emulator, or correct a component class after an upstream rename,
  by editing a TOML file — no app release, no fork.
- Hold the security boundary at `FLAG_GRANT_READ_URI_PERMISSION`: the launched
  emulator gets read access to exactly the one ROM `content://` URI for the
  duration of the session, and nothing else (§5).

### Non-goals (explicitly out of scope)

- **Bundling emulators.** Relic is not an emulator and does not bundle cores
  (PLAN.md §1). Templates reference external packages by application ID; if
  the package isn't installed, launch fails with a user-visible message.
- **Writing to the emulator.** Only read access is granted. Save states and
  SRAM are written by the emulator to its own app-private storage; Relic does
  not request write access to the ROM URI and does not manage saves.
- **Arbitrary intent construction.** The schema fixes the keys; a template
  cannot set arbitrary `Intent` fields (categories, clip data, type, selector).
  If an emulator needs a field the schema doesn't expose, propose an extension
  (§7) rather than overloading an existing key.
- **Desktop launching.** Desktop arg-template launching is a separate path
  (PLAN.md §4.5, `{rom}` / `{rom_dir}` / `{core}` substitution into a child
  process). This format is Android-only.
- **Network.** Templates make no network calls. Resolution is local; package
  presence is checked against the on-device `PackageManager`.

---

## 3. File layout

Built-in templates live in `core/data/intents/<id>.toml`, one emulator per
file. The filename stem is the template `id` (e.g. `retroarch.toml` →
`id = "retroarch"`). Users can override or extend these with files in their
config directory once the Phase 3 loader wires the merge (same override
convention as `core/data/systems/`, PLAN.md §4.4).

Shipped seed set:

| File | Emulator | Targets |
|---|---|---|
| `retroarch.toml` | RetroArch (stable) | All libretro-supported systems (core chosen per system) |
| `retroarch_aarch64.toml` | RetroArch (AArch64 nightly, `com.retroarch.aarch64`) | Same as `retroarch.toml`; mutually exclusive package alias |
| `ppsspp.toml` | PPSSPP (free) | `psp` |
| `ppsspp_gold.toml` | PPSSPP Gold (paid) | `psp` |
| `ppsspp_legacy.toml` | PPSSPP Legacy (pre-scoped-storage) | `psp` |
| `dolphin.toml` | Dolphin | `gamecube`, `wii` |
| `dolphin_mmjr.toml` | Dolphin MMJR2 (performance fork) | `gamecube`, `wii` |
| `melonds.toml` | melonDS (Android) | `nds` |
| `duckstation.toml` | DuckStation | `psx` |
| `aethersx2.toml` | AetherSX2 / NetherSX2 | `ps2` |
| `azahar.toml` | Azahar (was Lime3DS) | `n3ds` |
| `mupen64plus_fz.toml` | Mupen64Plus FZ | `n64` |
| `yabasanshiro2.toml` | Yaba Sanshiro 2 | `saturn` |

Deferred pending further verification or a schema extension (see
`docs/android-standalone-emulators.md` §4): Flycast, DraStic, Redream, Citra
MMJ, ScummVM (needs a `scummvm:<target>` URI scheme the current schema
doesn't support).

---

## 4. TOML schema

Every template is a single TOML document with the following keys. Unknown keys
are rejected by the loader (mirrors `core/src/systems/mod.rs`'
`deny_unknown_fields`). Types are TOML types; the `type` field inside
`[[extras]]` is a schema enum, not a TOML type.

### 4.1 Top-level keys

| Key | Type | Required | Meaning |
|---|---|---|---|
| `id` | string | yes | Stable identifier; must equal the filename stem. Used by launch profiles to reference a template. |
| `display_name` | string | yes | Human-readable name shown in the launch-profile picker UI. |
| `package` | string | yes | Target Android application ID (e.g. `com.retroarch`). |
| `activity` | string | yes | Fully-qualified component class name (e.g. `com.retroarch.browser.retroactivity.RetroActivityFuture`). Combined with `package` to form the explicit `ComponentName`. |
| `action` | string | yes | Intent action string. Typically `android.intent.action.VIEW` (when the ROM URI is carried as `Intent.data`) or `android.intent.action.MAIN` (when the ROM is passed entirely via extras). |
| `data_mode` | string, enum | yes | Where the ROM `content://` URI goes. One of: `"data"` (set as `Intent.setData`), `"extra"` (passed via an extra named by `data_extra_name`), `"none"` (URI not transmitted; emulator is launched bare and the user loads the ROM manually — rare, only for emulators with no URI intent interface). |
| `data_extra_name` | string | only if `data_mode = "extra"` | Name of the extra that carries the ROM URI. |
| `data_mime_type` | string | optional | MIME type to set via `Intent.setType` when `data_mode = "data"`. Most emulators ignore this; omit unless the emulator requires it. |
| `flags` | array of strings | optional | Standard Android `Intent` flag names, bare (e.g. `FLAG_GRANT_READ_URI_PERMISSION`, `FLAG_ACTIVITY_NEW_TASK`). The resolver maps each name to the corresponding `Intent.FLAG_*` constant. Unknown names fail validation. |
| `min_version_code` | integer | optional | Lowest installed app `versionCode` this template is known to work against. If the installed package's `versionCode` is below this, the shell warns and offers to fall back to another profile. |
| `per_system` | table | optional | Per-system overrides, keyed by Relic system slug (the slugs listed in `core/data/systems/`). Each sub-table may override `activity`, `action`, `data_mode`, `data_extra_name`, `data_mime_type`, `extras`, `flags`. See §4.4. |
| `systems` | array of strings | yes | The Relic system slugs this template is a launch candidate for, or the single-element wildcard `["*"]` for every registry system (RetroArch only — it derives the actual libretro core per system via `{core}`). Lets the shell pick candidate templates for a game's system (`relic_core::intents::applies_to`) with no per-emulator code. Must not be empty; `"*"` may not be mixed with concrete slugs. |

### 4.2 `[[extras]]` array of tables

Each extra becomes one `Intent.putExtra` call. Order is preserved (some
emulators are order-sensitive in practice, though they shouldn't be).

| Key | Type | Required | Meaning |
|---|---|---|---|
| `name` | string | yes | Extra key (e.g. `ROM`, `LIBRETRO`). |
| `type` | string, enum | yes | One of `"string"`, `"bool"`, `"int"`. Determines which `putExtra` overload is called. |
| `value` | string | yes | The value, with placeholder substitution (§4.3). For `type = "bool"`, the value must be the literal string `"true"` or `"false"`. For `type = "int"`, the value must be a base-10 integer literal (placeholders are not allowed for `int`). |

### 4.3 Placeholders

`value` strings (and only `value` strings) support these placeholders,
substituted by the shell at launch time. No other field accepts placeholders.

| Placeholder | Resolves to | Available when |
|---|---|---|
| `{rom_uri}` | The `content://` URI Relic has granted for the ROM file, via SAF. This is the same URI passed to `Intent.data` when `data_mode = "data"`. | Always. |
| `{rom_path}` | The ROM's path relative to its library root, as Relic stores it in `files.rel_path`. Useful for emulators that key state off filename rather than URI. | Always. |
| `{core}` | The libretro core for the system being launched, taken from the system registry's `default_core` field (`core/data/systems/*.toml`). **RetroArch only** — other emulators are not libretro and have no core; referencing `{core}` in a non-RetroArch template is a validation error. Resolves to the **full path** to the core `.so` (e.g. `/data/data/<pkg>/cores/mesen_libretro_android.so`), not just the filename stem — RetroArch AArch64 nightlies ≥ 2025-01-17 reject a bare stem (`docs/android-standalone-emulators.md` §2.1, `libretro/RetroArch#17433`). The alpha hardcoded launcher (`apps/android/.../RetroArchLauncher.kt`) already does this; the future data-driven resolver must match it. | RetroArch templates only. |

Unknown placeholders fail validation. Literal braces in a value are written
`{{` and `}}` (standard TOML has no escape for this; the resolver treats `{{`
and `}}` as literal `{` and `}` after placeholder substitution).

### 4.4 `per_system` overrides

`per_system` is a table keyed by Relic system slug. Each sub-table may contain
any subset of: `activity`, `action`, `data_mode`, `data_extra_name`,
`data_mime_type`, `extras` (replaces the top-level `extras` entirely — it does
not merge), `flags` (replaces). `package` is **not** overridable per system;
an emulator's application ID does not change between systems. `id`,
`display_name`, `min_version_code` are also not overridable.

Example (illustrative, not shipped):

```toml
[per_system.snes]
activity = "com.retroarch.browser.retroactivity.RetroActivityFuture"
extras = [
  { name = "ROM",      type = "string", value = "{rom_uri}" },
  { name = "LIBRETRO", type = "string", value = "{core}" },
]
```

Slugs are the ones listed in `core/data/systems/` (currently: `arcade`,
`atari2600`, `dreamcast`, `gamecube`, `gamegear`, `gb`, `gba`, `mastersystem`,
`megadrive`, `n3ds`, `n64`, `nds`, `nes`, `pcengine`, `ps2`, `psp`, `psx`,
`saturn`, `snes`, `wii`). A `per_system` key that doesn't match a known slug
fails validation.

### 4.5 `flags` values

Each entry is the bare name of an `Intent.FLAG_*` constant, without the `Intent.`
prefix. The resolver resolves it against `android.content.Intent` at runtime.
Allowed names include at least:

- `FLAG_GRANT_READ_URI_PERMISSION` — **always added implicitly** by the shell
  (§5); listing it explicitly is allowed but redundant.
- `FLAG_ACTIVITY_NEW_TASK` — required because Relic launches from an
  application context; the shell adds this implicitly if missing, but templates
  should list it for clarity.
- `FLAG_ACTIVITY_CLEAR_TOP`, `FLAG_ACTIVITY_SINGLE_TOP`,
  `FLAG_ACTIVITY_NO_HISTORY`, `FLAG_GRANT_WRITE_URI_PERMISSION` (forbidden —
  see §5), `FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS`.

`FLAG_GRANT_WRITE_URI_PERMISSION` is **forbidden**: validation rejects it. The
security model grants read only (§5).

---

## 5. Launch-time resolution

When the user launches a game on Android, the shell:

1. **Selects a template.** From the game's system slug and the user's launch
   profile for that system, resolves a template `id`. If the profile names a
   template whose `package` is not installed, the shell falls back through the
   profile's priority list and, failing that, surfaces a user-visible error
   naming the missing package.
2. **Applies `per_system`.** If the template has a `per_system.<slug>` entry,
   its fields override the top-level fields. Missing sub-keys inherit from the
   top level (except `extras` and `flags`, which replace wholesale when
   present).
3. **Builds the `ComponentName`** from `package` + `activity` and sets it on a
   new explicit `Intent`.
4. **Sets the action** from `action`.
5. **Places the ROM URI.** Resolves `{rom_uri}` against the SAF-granted
   `content://` URI for the ROM file. Depending on `data_mode`:
   - `"data"`: `Intent.setData(rom_uri)` (and `setType(data_mime_type)` if
     present).
   - `"extra"`: `Intent.putExtra(data_extra_name, rom_uri)`.
   - `"none"`: URI is not transmitted.
6. **Substitutes extras.** For each `[[extras]]` entry, substitutes
   placeholders in `value` (`{rom_uri}`, `{rom_path}`, `{core}` as available),
   coerces to the declared `type`, and calls the matching `putExtra` overload.
   `{core}` resolves from the system registry's `default_core` for the game's
   system slug.
7. **Sets flags.** Adds every flag in `flags`, then implicitly adds
   `FLAG_GRANT_READ_URI_PERMISSION` (if not already present) and
   `FLAG_ACTIVITY_NEW_TASK` (if not already present). Rejects any
   `FLAG_GRANT_WRITE_URI_PERMISSION` at validation time, so it can never reach
   this step.
8. **Grants URI access.** Calls `context.grantUriPermission(package, rom_uri,
   Intent.FLAG_GRANT_READ_URI_PERMISSION)` so the target emulator can read the
   ROM even though it lacks the SAF tree grant. This is scoped to the single
   URI and the single read flag — nothing more.
9. **Fires.** `context.startActivity(intent)`. Relic records the
   `play_session` start (`PLAN.md` §4.3 `play_sessions`), drops its render
   surface / minimizes to shed GPU+RAM, and restores focus on return.
10. **Revokes.** On session end (the emulator activity reports back via the
    standard `onActivityResult` / activity-result contract, or on a
    session-timeout watchdog), the shell calls
    `context.revokeUriPermission(package, rom_uri,
    Intent.FLAG_GRANT_READ_URI_PERMISSION)` to drop the grant. The grant is
    ephemeral and scoped to the session; it does not persist across launches
    and is not broadened to the library tree.

### Security note

Only `FLAG_GRANT_READ_URI_PERMISSION`-scoped **read** access is granted, and
only for the single ROM `content://` URI, and only for the duration of the
play session. Relic never grants:

- write access to the ROM (`FLAG_GRANT_WRITE_URI_PERMISSION` is forbidden in
  templates and not added by the resolver),
- access to the SAF tree the ROM lives under (the emulator gets the file, not
  the folder),
- access to any other ROM, the index DB, user data, or Relic's own storage,
- a grant that outlives the session (revoked on session end, §5 step 10).

The launched emulator cannot enumerate the user's library through the grant.
This is the same scoped-storage friction mitigation `apps/android/README.md`
references.

---

## 6. Validation rules

`relic-cli intent-validate` (`core/src/intents/mod.rs`, `fn validate`)
enforces, and `cargo test -p relic-core intents::` checks against every
shipped template:

1. `id` equals the filename stem.
2. `package` and `activity` are non-empty and look like Android component
   names (dotted, no spaces).
3. `action` is one of the known action strings (`android.intent.action.VIEW`,
   `android.intent.action.MAIN`, …) — open set, but must start with
   `android.intent.action.` or a known vendor prefix.
4. `data_mode` is `"data"`, `"extra"`, or `"none"`. If `"extra"`,
   `data_extra_name` is required and non-empty. If `"none"`, no `[[extras]]`
   entry may reference `{rom_uri}` (it would have nowhere to go).
5. Every `[[extras]]` entry has `name`, `type` ∈ {`string`, `bool`, `int`},
   and `value`. For `type = "bool"`, `value` is `"true"` or `"false"`. For
   `type = "int"`, `value` is a base-10 integer literal (no placeholders).
6. `{core}` appears only in templates whose `id = "retroarch"`. (RetroArch is
   the only libretro frontend in the seed set; if a second libretro frontend
   is added, this rule loosens.)
7. Every placeholder is one of `{rom_uri}`, `{rom_path}`, `{core}`. Unknown
   placeholders fail.
8. `flags` contains only known `Intent.FLAG_*` names.
   `FLAG_GRANT_WRITE_URI_PERMISSION` is rejected.
9. Every `per_system` key matches a slug listed in `core/data/systems/`.
10. `min_version_code`, if present, is a non-negative integer.
11. `systems` is non-empty. Its entries are either exactly `["*"]`, or every
    entry is a known system slug (the two forms cannot mix).

Rule 3 above is stricter in the implementation than "known vendor prefix"
suggests: today it requires the literal `android.intent.action.` prefix. No
shipped template uses a vendor action, so this hasn't needed loosening yet —
if one does, extend `validate()` alongside the template.

---

## 7. Contributing a template

To add support for a new emulator, or correct an existing template after an
upstream rename:

1. **Verify the intent interface on a real device.** Install the emulator,
   note its `package` (from the Play Store listing, F-Droid, or
   `adb shell pm list packages`), and the activity that accepts the launch
   intent (`adb shell dumpsys package <package>` → look for an exported
   activity with an `android.intent.action.VIEW` / `MAIN` filter, or the
   emulator's published docs). Do not guess: a wrong component class fails
   silently as "app not installed" to the user.
2. **If a field is uncertain, mark it.** Add a TOML comment
   `# UNVERIFIED - needs device testing` on the line in question. Do not
   silently guess. A shipped template with an `UNVERIFIED` marker is
   acceptable; a shipped template with a guessed-but-unmarked component is not.
3. **Add the file** at `core/data/intents/<id>.toml`. The filename stem is the
   `id`. Register it in the `BUILTIN` array in `core/src/intents/mod.rs` (one
   `include_str!` line) so it's parsed and validated by
   `cargo test -p relic-core` and visible to `relic-cli intents`.
4. **Minimize extras.** Only carry what the emulator needs to boot the ROM.
   Prefer `data_mode = "data"` (URI in `Intent.data`) over an extra when the
   emulator accepts both — it's the more standard interface and the one
   scoped-directory access was designed around.
5. **Never grant write.** Do not add `FLAG_GRANT_WRITE_URI_PERMISSION`. If an
   emulator genuinely cannot function without write access to the ROM, open an
   issue first — that's a format extension, not a template edit.
6. **Test.** Run `relic-cli intent-validate core/data/intents/<id>.toml` (or
   `intent-validate` with no path to check every built-in template). The
   resolver that fires a real `Intent` from a template is still ahead
   (`apps/android` hardcodes RetroArch today), so a device launch isn't yet
   possible through Relic itself for other emulators — until then, manual
   `adb am start` against the resolved component is the best available check.
7. **Document per-system quirks in `per_system`.** If the emulator needs a
   different activity or extra set for one system, put it in `per_system`
   rather than forking the template.

Template files are data; a PR that adds or fixes one does not require touching
any `.rs` file or `Cargo.toml`, and does not require an app release once the
Phase 3 loader is in place — users can drop a corrected file into their config
directory to override a built-in.

---

## 8. Reference: shipped templates

All templates below were cross-checked against primary sources in
`docs/android-standalone-emulators.md`; `[VERIFY]` items from that digest are
carried into the TOML as `# UNVERIFIED - needs device testing` comments
rather than silently resolved.

- `retroarch.toml` — RetroArch. `data_mode = "extra"` (`ROM` extra), with
  `LIBRETRO` (`{core}`), `CONFIGFILE`, and `QUITFOCUS` extras per
  RetroArch's `RetroActivityFuture` interface. Covers every system in the
  registry via `{core}`; no `per_system` overrides needed because the core is
  system-derived.
- `ppsspp.toml` / `ppsspp_gold.toml` / `ppsspp_legacy.toml` — PPSSPP free,
  Gold (paid), and Legacy (pre-scoped-storage) packages. Identical interface
  (`data_mode = "data"`, action `VIEW`, `org.ppsspp.ppsspp/.PpssppActivity`
  in all three), differing only by application ID; mutually exclusive
  installs. PSP only.
- `dolphin.toml` — Dolphin. `data_mode = "extra"` (`AutoStartFile`), action
  `MAIN` — `MainActivity` does not export a `VIEW` filter. Targets `gamecube`
  and `wii`. Return-to-caller behavior is `[VERIFY]`; no `QUITFOCUS`
  equivalent.
- `dolphin_mmjr.toml` — Dolphin MMJR2, a performance-focused fork. Package
  `org.dolphinemu.mmjr` (not `.mmjr2`); interface inherited from Dolphin and
  marked `# UNVERIFIED - needs device testing` throughout since no primary
  source confirms the fork's manifest. Targets `gamecube` and `wii`.
- `melonds.toml` — melonDS (`me.magnum.melonds`). `data_mode = "data"`,
  action `VIEW`, target `.../ui.emulator.EmulatorActivity`. NDS only.
- `duckstation.toml` — DuckStation. `data_mode = "extra"` (`bootPath`),
  action `MAIN`, target `.../EmulationActivity` (not `MainActivity`).
  `resumeState` forced `false` — external-launch save-state resume doesn't
  work. PSX only.
- `aethersx2.toml` — AetherSX2 / NetherSX2 (same package; NetherSX2 is a
  binary patch of AetherSX2 that keeps the package ID). `data_mode = "extra"`
  (`bootPath`), action `MAIN`. PS2 only.
- `azahar.toml` — Azahar (was Lime3DS; also covers Lime3DS, same package).
  `data_mode = "data"`, action `VIEW`. Installed package
  (`io.github.lime3ds.android`) differs from the `build.gradle.kts`
  `applicationId` (`org.azahar_emu.azahar`) — kept for Play Store listing
  continuity. 3DS (`n3ds`) only.
- `mupen64plus_fz.toml` — Mupen64Plus FZ, free package
  (`org.mupen64plusae.v3.fzurita`; Pro and upstream-alpha packages exist as
  alternates, not shipped as separate templates). `data_mode = "data"`,
  action `VIEW`. Content-URI support is `# UNVERIFIED`. N64 only.
- `yabasanshiro2.toml` — Yaba Sanshiro 2, Pro package. `data_mode = "extra"`
  (`org.uoyabause.android.FileNameUri`), action `VIEW` — marked
  `# UNVERIFIED` since the developer states `MAIN`-only while working
  community configs use `VIEW`. Only `.chd` Saturn images launch reliably
  (upstream bug). Saturn only.
