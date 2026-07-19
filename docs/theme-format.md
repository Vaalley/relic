# Relic Theme Format — Layer 1 (Design Tokens)

> Companion to `PLAN.md` §6. Spec for the layer-1 theme format: design tokens
> (colors, typography, shape, sounds) loaded from a `theme.toml` manifest.
> Status: **Implemented, Phase 5.** `modules/themes` (`load_theme_dir`) loads
> and validates a theme directory; both shells let the user pick one, apply
> its resolved tokens immediately, persist the choice via `Engine::
> set_setting`, and hot-reload on a `theme.toml` mtime change (desktop: a
> polling `slint::Timer`; Android: on `onResume`, alongside rescan-on-resume).
> Fields may still move or rename until layout themes (layer 2) land.

---

## 1. Status

**Implemented, Phase 5** (layer 1 — design tokens only; layer 2 layout themes
are post-1.0, PLAN.md §9). This document defines the layer-1 theme format
described in `PLAN.md` §6. It is the contract between three consumers:

- `modules/themes` — the loader (`load_theme_dir`) and resolver (`resolve`).
- `apps/desktop` and `apps/android` — the shells that let a user pick a theme
  directory, apply its resolved tokens, and hot-reload on changes.
- `relic-cli theme validate` — the validator that enforces the rules in §7.

The bundled `themes/default/theme.toml` is the canonical example and the
built-in fallback theme (§6). Where this spec and that file disagree, the spec
is wrong until updated; the file is the source of truth for the current
provisional shape.

---

## 2. Goals and non-goals

### Goals

- Define every color, font, corner radius, spacing unit, and UI sound both
  shells resolve through, in a single token table loaded from a manifest.
- Enable "deep recolor" themes cheaply and force the shells to be style-clean
  from the start.
- Support light/dark variants, per-system accent colors, custom fonts, and
  sound packs.
- Be pure data plus assets: a theme is a folder (or zip of one) containing
  `theme.toml` and optional asset subdirectories. Nothing else.

### Non-goals (explicitly out of scope for layer 1)

- **Layer-2 declarative layouts** — per-system artwork, screen descriptions,
  constraint-based boxes, text/image/carousel/grid primitives, data binding to
  `game.*` / `system.*`. That is a separate format shipped post-1.0
  (PLAN.md §6, layer 2). Layer 1 carries no layout primitives.
- **Scripting.** No scripting in v1 of the format. No Lua, no expressions, no
  computed tokens, no templating. Tokens are literal values. A sandboxed
  scripting layer is a post-1.0 consideration only if the community hits real
  walls (PLAN.md §6); it will not be silently added here.
- **Network.** Themes make no network calls. A theme that attempts any is
  treated as broken (§6).
- **Filesystem access outside the theme folder.** Asset paths resolve only
  against the theme's own directory. Absolute paths, parent traversal (`..`),
  and symlinks that escape the theme folder are rejected at load.
- **Executable code of any kind.** No bundled binaries, no shell scripts, no
  "pre/post install" hooks. A theme is data and static assets.

---

## 3. Package layout

A theme is distributed as either:

1. A **folder** containing `theme.toml` at its root, or
2. A **zip archive** of such a folder (the archive may wrap a top-level
   directory or place `theme.toml` at the archive root; both are accepted).

```
my-theme/
├── theme.toml          # required, exactly one, at the package root
├── fonts/              # optional; custom font assets
│   └── Inter.ttf
└── sounds/             # optional; UI sound assets referenced by [sounds]
    ├── move.wav
    ├── select.wav
    └── back.wav
```

Rules:

- `theme.toml` is required and must be parseable TOML.
- `fonts/` and `sounds/` are optional. A theme with neither is valid (the
  default theme ships with neither).
- Asset filenames referenced from `theme.toml` are relative to the theme root
  and must resolve inside it (§2, §7).
- Asset formats are restricted to a known-safe set (§7); anything else is a
  validation warning and is ignored at load.
- The package has no other special files. No `README`, no `LICENSE`, no
  `preview.png` are required or read by the loader (creators may include them
  for humans; the loader ignores them).

---

## 4. The `[theme]` table

Manifest metadata. Required.

| Key | Type | Required | Default | Meaning |
|-----|------|----------|---------|---------|
| `name` | string | yes | — | Human-readable theme name shown in the picker. |
| `author` | string | no | `""` | Author or maintainer, free text. |
| `version` | string | no | `"0.0.0"` | Theme version, semver-ish. Not the format version; see `format_version` below. |
| `format_version` | integer | yes | — | Version of **this spec** the theme targets. Drives compatibility policy (§8). |
| `description` | string | no | `""` | Short blurb for the picker. |

`format_version` is the only field the loader interprets for compatibility
decisions. `version` is informational and may be anything.

---

## 5. Token tables

All token tables are optional as a whole. A missing table falls back to the
built-in default theme's table (§6). Individual missing keys fall back to the
default theme's key. There is no "partial override merges upward into a
hardcoded base" — the base is always `themes/default/theme.toml`.

### 5.1 `[colors]` and `[colors.dark]` / `[colors.light]`

`[colors]` itself holds no keys; it contains exactly two subtables, `dark` and
`light`. Both are optional; a theme may ship only one variant. The shell picks
which variant to use based on the viewer's theme preference (§6).

Each value is a CSS-style hex color string: `#RGB`, `#RGBA`, `#RRGGBB`, or
`#RRGGBBAA`. Uppercase or lowercase hex digits are accepted.

| Key | Type | Default (dark / light from `themes/default`) | Meaning |
|-----|------|-----------------------------------------------|---------|
| `bg` | color | `#121212` / `#f7f7f7` | App background. |
| `surface` | color | `#1e1e1e` / `#ffffff` | Cards, panels, list rows. |
| `text` | color | `#f2f2f2` / `#121212` | Primary text. |
| `text_dim` | color | `#a0a0a0` / `#5a5a5a` | Secondary/muted text. |
| `accent` | color | `#7c9eff` / `#3a5fd9` | Focus ring, selection, primary action. |
| `favorite` | color | `#ffcf5c` / `#c98a00` | Favorite-star highlight. |

Additional color keys are reserved for future spec versions; unknown keys are
a validation warning (§7), not an error, and are ignored at load.

### 5.2 `[typography]`

| Key | Type | Default | Meaning |
|-----|------|---------|---------|
| `font_family` | string | `"Inter"` | Font family name. If the name matches a font shipped in the theme's `fonts/`, that asset is loaded; otherwise the shell falls back to its platform default for the family, then to the default theme's family. |
| `scale` | float | `1.0` | Multiplier applied to the shell's base type scale. Must be > 0. Values outside a sane range (e.g. `< 0.5` or `> 3.0`) are a validation warning. |

`font_family` is a name, not a path. The loader maps the name to an asset in
`fonts/` by basename (without extension). Matching is case-insensitive.

### 5.3 `[shape]`

| Key | Type | Default | Meaning |
|-----|------|---------|---------|
| `radius` | integer | `8` | Corner radius in device-independent pixels, applied to cards, buttons, and panels. Must be ≥ 0. |

Future shape tokens (stroke widths, spacing units, elevation) are reserved and
will be added in a later `format_version`; unknown keys are a warning.

### 5.4 `[sounds]`

Each value is a string: a path relative to the theme root, or `""` (the
default) meaning "no sound for this cue." Non-empty paths must point at a file
inside the theme's `sounds/` (or, by exception, elsewhere inside the theme
folder) and must be a permitted format (§7).

| Key | Type | Default | Meaning |
|-----|------|---------|---------|
| `move` | string | `""` | Played on focus move between grid/list items. |
| `select` | string | `""` | Played on activating an item. |
| `back` | string | `""` | Played on navigating back. |

Additional sound cues are reserved for future spec versions; unknown keys are
a warning.

---

## 6. Resolution rules

Resolution is the process by which a shell obtains a concrete token value at
runtime. It is deterministic and never raises.

1. **Variant selection.** The viewer's theme preference (a setting in
   `settings`, see PLAN.md §4.3) picks `dark` or `light`. If the selected
   theme defines the chosen variant, that variant is used. If it defines only
   the other variant, the **default theme's** chosen variant is used for the
   missing one — the shell does not silently substitute the theme's other
   variant. If the selected theme defines neither, the whole `[colors]`
   table falls back to the default theme.
2. **Per-key fallback.** For any token key the selected theme does not
   define, the value is taken from the built-in default theme
   (`themes/default/theme.toml`). Missing keys are **never** an error.
3. **Asset fallback.** A referenced asset that cannot be located, fails to
   decode, or is rejected by the sandbox (§2) falls back to the default
   theme's equivalent asset, or to "no asset" (e.g. silent sound, platform
   default font) if the default theme also lacks it.
4. **Broken theme.** A theme that fails to parse, violates a hard rule
   (network, filesystem escape, executable code), or whose `format_version`
   is unsupported (§8) is **broken**. The shell degrades to the default theme
   **with a visible warning** surfaced in the UI (PLAN.md §6). It never
   crashes, never silently ignores the theme, and never leaves the user
   looking at an unexplained wrong theme.
5. **No partial trust.** A theme is either fully loaded or fully rejected.
   There is no mode where some tokens come from a broken theme and the rest
   from the default; the warning in (4) makes "fully rejected" visible.

---

## 7. Validation rules

`relic-cli theme validate <path>` (Phase 5) loads a theme without applying it
and reports problems. Exit code is non-zero if any **error** is present;
warnings do not fail validation but are printed.

**Errors** (theme is rejected at load):

- `theme.toml` missing, not at the package root, or unparseable TOML.
- `[theme]` table missing, or `name` / `format_version` missing or wrong type.
- `format_version` not a supported version (§8).
- A color value is not a valid hex string per §5.1.
- `typography.scale` is not a number, or `≤ 0`.
- `shape.radius` is not an integer, or `< 0`.
- A `[sounds]` value is not a string.
- A non-empty `[sounds]` path does not resolve inside the theme folder
  (absolute path, `..` traversal, or symlink escape).
- A referenced asset file does not exist.
- A referenced asset is not in the permitted format set:
  - fonts: `.ttf`, `.otf`, `.woff`, `.woff2`
  - sounds: `.wav`, `.ogg`, `.flac`, `.mp3`
- The package contains an executable file or a file whose name or shebang
  suggests one (`.exe`, `.sh`, `.bat`, `.ps1`, `.dylib`, `.so`, `.dll`).
- Any indication of network access in the manifest (there is no permitted
  form; this is defensive against future fields).

**Warnings** (theme loads, but the author should fix):

- Unknown token key in any table (forward-compat drift).
- `typography.scale` outside `[0.5, 3.0]`.
- `font_family` names a family with no matching asset in `fonts/` (will fall
  back to platform default).
- A `[colors]` variant is defined but the other is missing.
- A `[sounds]` cue is set to a non-empty path while another cue is `""`
  (incomplete sound pack; informational only).
- `version` is not parseable semver.
- Unused asset files in `fonts/` or `sounds/` not referenced by `theme.toml`.

Validation is purely offline. The validator makes no network calls and reads
no files outside the package path given on the command line.

---

## 8. `format_version` and compatibility policy

`format_version` is an integer in `[theme]` identifying the spec version the
theme targets. It is distinct from the theme's own `version` (which is
author-facing metadata).

Supported versions are a contiguous range `[MIN, MAX]` baked into the
`modules/themes` crate at build time. The current draft targets version **1**.

Policy:

- A theme whose `format_version` is in `[MIN, MAX]` loads normally. The loader
  interprets only the keys defined for that version; unknown keys are
  warnings (§7).
- A theme whose `format_version` is **below `MIN`** is rejected as broken
  (§6.4) with a visible warning: "theme targets format vN, this Relic supports
  vMIN–vMAX." The user is pointed at `relic-cli theme validate` for details.
- A theme whose `format_version` is **above `MAX`** is rejected as broken with
  the same shape of warning. We do **not** attempt forward-compat loading of
  newer formats; the warning tells the user to update Relic.
- Bumping `MAX` is a minor Relic release. Bumping `MIN` (dropping support for
  an old format) is a major Relic release and is documented in an ADR under
  `docs/adr/`.
- The set of token keys is appenditive across versions within a major Relic
  release: a new key added at format v2 has a default, so a v1 theme still
  loads under a loader that knows about v2. Removing or renaming a key
  requires bumping `MAX` and is called out in the release notes.

---

## 9. Open questions

1. **Per-system accent colors.** PLAN.md §6 mentions per-system accent colors.
   Do they live in `[colors]` as a `system.<slug>.accent` subtable, in the
   systems registry (`core/data/systems/*.toml`) as a `theme.accent` key, or
   in a separate `[system_colors]` table? The systems registry already carries
   "theme keys" (PLAN.md §4.4) — resolve the overlap before Phase 5 freeze.
2. **Spacing units.** Layer 1 in PLAN.md §6 lists "spacing unit" as a token,
   but `themes/default/theme.toml` does not yet ship one and `[shape]` only
   has `radius`. Decide whether spacing is a single `spacing.unit` multiplier
   or a named scale (`xs`/`sm`/`md`/`lg`/`xl`) before format v1 is frozen.
3. **Zip packaging details.** Top-level directory vs. flat archive (§3) — pick
   one as canonical and treat the other as a tolerated variant, or require
   exactly one. Also: maximum archive size and decompression-bomb guard.
4. **Asset sandbox enforcement point.** Should path validation happen in the
   loader, in `relic-cli theme validate`, or both with shared code? The rule
   is the same; the question is where the canonical implementation lives so
   shells and CLI cannot drift.
5. **Hot-reload surface.** PLAN.md §5 lists "theme hot-reload for creators" as
   post-1.0. Does the loader expose a watch + reload API in Phase 5 so the
   CLI validator can do live preview, or is hot-reload strictly a shell
   feature deferred with layer 2?
6. **ES-DE importer mapping.** The ES-DE importer is a stretch goal
   (PLAN.md §6). Decide whether it produces a layer-1 theme only, or also
   emits a best-effort layer-2 layout. If layer-1 only, document which ES-DE
   color/font/sound concepts map to which tokens here.
7. **`format_version` numbering.** Is v1 the version shipped at Phase 5
   freeze, or do we start at v0 for the current provisional
   `themes/default/theme.toml` and bump to v1 at freeze? Affects whether the
   bundled default needs a `format_version` bump the moment this spec is
   ratified.
