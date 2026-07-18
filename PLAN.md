# Relic — Implementation Plan

> Companion to `RELIC.md`. That file is the elevator pitch; this is the build plan.
> Status: draft v1 — 2026-07-18

---

## 1. Vision & Product Goals

**Relic** is a local-first, zero-telemetry, open-source retro game frontend. It organizes
personal ROM collections and boots standalone emulators, entirely offline by default.

Five product pillars, in priority order (when they conflict, the higher one wins):

1. **Reliable** — never corrupts a library, never loses user edits, survives crashes mid-scan.
2. **Fast** — instant startup, 60 fps navigation, scans that scale to 100k+ files.
3. **Small** — tight binaries, low RAM, no bundled runtimes it doesn't need.
4. **Modular** — everything beyond "scan, browse, launch" is an optional module.
5. **Feature-rich** — via modules, not via a monolith.

**Non-goals** (explicitly out of scope):

- Relic is **not an emulator** and does not bundle emulation cores.
- No ROM downloading, no links to ROM sources, ever.
- No telemetry, analytics, or crash reporting that leaves the device. Diagnostics are
  local files the user can choose to attach to a bug report.
- No mandatory accounts. Every online feature is opt-in, off by default, and removable.

### Privacy tiers

Every feature is classified into one of three tiers, surfaced in the UI:

| Tier | Network | Examples | Default |
|------|---------|----------|---------|
| **T0 — Offline** | None, ever | Scanning, browsing, launching, theming, collections | On |
| **T1 — Opt-in fetch** | Outbound requests to a user-chosen service | Metadata scraping, RetroAchievements display | Off |
| **T2 — Opt-in identity** | Authenticated account | RA login, friends/presence | Off |

The core engine and both shells must be fully functional at T0. T1/T2 code lives in
separate modules that can be compiled out entirely (`--no-default-features` builds a
pure-offline binary).

---

## 2. Technology Decisions

### 2.1 Core language: Rust (recommended change from RELIC.md's Zig suggestion)

`RELIC.md` suggests Zig. Zig fits the "small" pillar, but Rust is the pragmatic choice here
and I recommend switching:

- **FFI story**: [UniFFI](https://mozilla.github.io/uniffi-rs/) generates Kotlin and Swift
  bindings from one interface definition — exactly the "one core, many shells" architecture.
  Zig would require hand-writing and hand-maintaining JNI + Swift bridges.
- **Ecosystem**: `rusqlite` (battle-tested SQLite), `notify` (cross-platform FS watching),
  `serde`/`quick-xml` (gamelist.xml), `zip`/`sevenz-rust` (archives), `rcheevos` has usable
  Rust bindings for RetroAchievements hashing.
- **Reliability pillar**: memory safety + fearless concurrency matter in a multithreaded
  scanner that must never corrupt the DB.
- **Size**: with `opt-level="z"`, LTO, `panic="abort"`, a core like this lands ~1.5–3 MB —
  well within budget.

If a contributor strongly prefers Zig, the boundary design (§4) is language-agnostic
(C-ABI + message passing), so the decision is revisitable — but pick once, before Phase 1.

### 2.2 Shells

| Target | Stack | Rationale |
|--------|-------|-----------|
| Android | Kotlin + Jetpack Compose | Per RELIC.md. First-class launcher/HOME support, controller input APIs, SAF handling. |
| Desktop (Win/mac/Linux) | Rust + **Slint** (primary candidate) | Native, GPU-accelerated, declarative UI language that maps well to theming; tiny runtime (~no webview); single binary. Fallback candidate: `egui` (faster to prototype, weaker styling). Decide via a 1-week spike in Phase 0. |
| iOS (post-1.0) | SwiftUI over the same UniFFI bindings | Deferred; see Phase 8. |

**Why not Tauri/Electron:** webview jank on Linux, larger footprint, and a 10-foot
gamepad-driven grid UI wants direct control of focus and frame pacing.

### 2.3 Storage

- **SQLite** (WAL mode) as the single source of truth for the index. One DB per library.
- User edits (favorites, ratings, custom names) stored in **separate tables from scanned
  data**, so a rescan can never clobber them.
- Versioned schema migrations from day one (`user_version` + migration runner + tests).
- Media cache (thumbnails) as content-addressed files on disk, DB stores hashes only.

---

## 3. Monorepo Layout

```
relic/
├── core/                      # relic-core: the headless engine (Rust workspace member)
│   ├── src/
│   │   ├── db/                # schema, migrations, queries
│   │   ├── scan/              # crawler, hasher, watcher
│   │   ├── systems/           # platform registry (loaded from data/systems/*.toml)
│   │   ├── metadata/          # gamelist.xml, DAT/No-Intro parsing, matching
│   │   ├── launch/            # emulator profiles, arg templating, session tracking
│   │   ├── media/             # image cache, thumbnail pipeline
│   │   ├── events/            # event bus streamed to shells
│   │   └── api/               # the command/query surface exposed over FFI
│   └── data/systems/          # built-in platform definitions (TOML, user-overridable)
├── ffi/
│   ├── uniffi/                # relic.udl → Kotlin/Swift bindings
│   └── capi/                  # stable C ABI + JSON messages (desktop, CLI, 3rd parties)
├── modules/                   # optional, feature-gated crates
│   ├── scraper/               # T1: ScreenScraper / TheGamesDB / LaunchBox metadata
│   ├── retroachievements/     # T1/T2: RA hashing, API client, offline cache
│   ├── sync/                  # T0/T1: LAN save & library sync between own devices
│   └── themes/                # theme engine (T0, but its own crate for isolation)
├── apps/
│   ├── desktop/               # Slint shell
│   ├── android/               # Gradle project, Kotlin + Compose
│   └── ios/                   # (post-1.0)
├── tools/
│   └── relic-cli/             # headless CLI: scan, query, launch — also the test harness
├── themes/                    # bundled default theme + 1–2 alternates
├── docs/                      # ADRs, theme format spec, module API spec
└── fixtures/                  # synthetic test libraries (empty files + gamelists)
```

**Modularity rules:**

- `core` depends on nothing in `modules/`. Modules depend on `core`'s public API only.
- Every module is a Cargo feature; CI builds and tests the pure-T0 configuration
  (`--no-default-features`) on every commit so offline-only never rots.
- Shells discover module capabilities at runtime via a `capabilities()` API call and
  hide UI for absent modules — no dead menus.

---

## 4. Core Engine Design

### 4.1 API boundary

The core exposes a **command/query + event stream** surface (no shell ever touches SQLite
directly):

- **Queries** (synchronous, cheap): `list_systems()`, `query_games(filter, sort, page)`,
  `game_detail(id)`, `search(text)` (SQLite FTS5).
- **Commands** (async, return a job id): `start_scan(library_id)`, `set_favorite(...)`,
  `launch(game_id, profile_id)`, `import_gamelists(...)`.
- **Events** (subscription/callback): `ScanProgress{done,total}`, `GamesChanged{system_id}`,
  `LaunchStarted/Ended{session}`, `Error{code, context}`.

Events are **delta-oriented** so shells update incrementally instead of re-querying whole
lists. Over UniFFI these are typed callbacks; over the C ABI they're JSON messages on a
callback pointer.

### 4.2 Scan pipeline

```
enumerate ──► filter ──► identify ──► match metadata ──► upsert ──► emit deltas
 (walkdir)   (per-system  (size+mtime   (gamelist.xml,    (SQLite      (events)
              extension    quick-key;    filename fuzzy,   batch txn)
              rules)       full hash     DAT lookup)
                           on demand)
```

Key behaviors:

- **Incremental by default**: a file whose (path, size, mtime) is unchanged is skipped.
  Full CRC32/MD5 hashing is deferred/lazy (needed only for DAT matching and RA) and runs
  as a low-priority background job.
- **Resumable & atomic**: scan progress checkpoints; killing the app mid-scan leaves the
  DB consistent (one transaction per batch of ~500 files).
- **Archive-aware**: `.zip`/`.7z` are enumerated without extraction; multi-disc sets
  collapse via `.m3u` or name heuristics into one game with N discs.
- **Watcher**: after initial scan, `notify`-based FS watching keeps the index live on
  desktop; Android uses rescan-on-resume (SAF has no reliable watch API).
- **Never blocks the UI**: scanning runs on a dedicated thread pool; queries stay
  responsive throughout (WAL readers don't block).

### 4.3 Data model (schema v1 sketch)

```
libraries(id, root_uri, name, created_at)
systems(id, slug, name, sort, extensions, …)            -- seeded from data/systems/
games(id, system_id, canonical_name, sort_name, region, …)
files(id, game_id, library_id, rel_path, size, mtime, quick_key, crc32?, md5?, in_archive?)
metadata(game_id, source, title, description, genre, developer, publisher,
         release_date, players, rating, …)               -- one row per source, merged by priority
media(game_id, kind{boxart,screenshot,marquee,video,…}, source, cache_hash)
user_data(game_id, favorite, hidden, user_rating, custom_name, notes)   -- survives rescans
collections(id, name, kind{manual,smart}, smart_query?)
collection_games(collection_id, game_id, position)
emulators(id, name, platform, exec_or_package, …)
launch_profiles(id, emulator_id, system_id, arg_template, priority)
play_sessions(id, game_id, started_at, ended_at, duration_s)
settings(key, value)                                     -- namespaced, incl. per-module
```

Principles: scanned data is disposable and rebuildable; `user_data`, `collections`,
`play_sessions`, `settings` are precious and covered by an export/backup command
(single JSON file) from the start.

### 4.4 Systems registry

Platform definitions (extensions, display names, default RetroArch cores, RA console ids,
theme keys) live in TOML files under `core/data/systems/`, compiled in but overridable by
user files in the config dir. Adding a platform is a data change, not a code change.

### 4.5 Launching

- **Desktop**: spawn child process from `arg_template` with token substitution —
  `{rom}`, `{rom_dir}`, `{rom_extracted}` (temp-extract from archive when the emulator
  can't read archives), `{core}` (libretro core path), plus pre/post launch hooks
  (user scripts, opt-in). Relic tracks the child, records the play session, drops its
  render surface / minimizes to shed GPU+RAM while the game runs, and restores focus on exit.
- **Android**: translate DB paths to `content://` URIs via SAF, build an explicit `Intent`
  from a per-emulator intent template (component, extras, data URI, flags), grant
  `FLAG_GRANT_READ_URI_PERMISSION`, fire, and record the session on return. Ship
  built-in intent templates for RetroArch, and the common standalones (Dolphin, PPSSPP,
  DraStic-likes, Yaba Sanshiro, etc.) as data files, community-extendable.
- **Emulator auto-detection**: scan common install locations / installed packages and
  pre-fill profiles; always user-editable.

---

## 5. Feature Matrix

Legend: **M** = MVP (Phases 1–3) · **1.0** = required for 1.0 · **+** = post-1.0 module/nice-to-have

### Library & browsing (T0)
| Feature | Target |
|---|---|
| Multi-library, multi-system scan & index | M |
| Game grid/list, system browser, gamepad-first navigation | M |
| Instant search (FTS5) with filters (system, genre, players, region, unplayed) | M search · 1.0 filters |
| Favorites, hidden games, custom names/notes, user ratings | M favorites · 1.0 rest |
| Recently played / most played / random "surprise me" | 1.0 |
| Manual collections + smart collections (saved queries) | 1.0 |
| Multi-disc grouping (m3u), archive support without extraction | 1.0 |
| Duplicate/region awareness, 1G1R preferred-copy display | + |
| Kid/kiosk mode (whitelist collections, lock settings behind PIN) | + |
| Attract mode / screensaver cycling artwork | + |
| Localization (i18n scaffolding from Phase 2; translations community-driven) | 1.0 scaffolding |

### Metadata & media
| Feature | Target |
|---|---|
| `gamelist.xml` (ES/ES-DE) import incl. media paths | M |
| Local media folders (boxart/screenshots/marquees) by naming convention | M |
| Thumbnail pipeline (downscaled, content-addressed cache; smooth grid on weak GPUs) | M |
| No-Intro/Redump DAT matching for canonical names | 1.0 |
| `gamelist.xml` **export** (interop back to other frontends) | 1.0 |
| Scraper module (ScreenScraper, TheGamesDB, LaunchBox mirror) — T1, off by default | 1.0 |
| Video snaps in grid/detail | + |
| IPS/BPS patch awareness (list patched variants) | + |

### Launching & play
| Feature | Target |
|---|---|
| Desktop process launch with arg templates; Android intent launch | M |
| Playtime tracking & per-game stats | 1.0 |
| Save file / save state browser + local backup snapshots | + |
| LAN sync of saves & user data between own devices (`sync` module, T0/T1) | + |
| Per-system/per-game launch profile overrides | 1.0 |

### Platform integration
| Feature | Target |
|---|---|
| Android HOME-launcher role (replace stock launcher on handhelds) | 1.0 |
| Android TV / leanback support | + |
| Steam Deck friendly (gamepad UI + flatpak packaging) | 1.0 packaging |
| CLI (`relic-cli`): scan/query/launch headless — power users & scripting | M (it's also the test harness) |

### Theming (T0) — see §6
| Feature | Target |
|---|---|
| Design-token theming (colors, fonts, radii, spacing, sounds) | 1.0 |
| Full layout theming (declarative screen definitions, per-system art) | + (Phase 5) |
| Theme hot-reload for creators; theme packs as folders/zips | + |
| ES-DE theme importer (best-effort subset) | + (stretch) |

### Online modules (T1/T2, all opt-in) — see §7
| Feature | Target |
|---|---|
| RetroAchievements: login, game matching via RA hashes, achievement display, progress, offline cache | + (Phase 6) |
| RA: mastery badges on grid, "cheevo-hunting" smart collection | + |
| Friends/presence ("Relic Circle") — design in §7.3, ships last, possibly self-hosted | + (Phase 7) |

---

## 6. Theming Engine (Phase 5, foundations earlier)

Two layers, shipped in this order:

1. **Design tokens (1.0)** — every color, font, corner radius, spacing unit, and UI sound
   in both shells resolves through a token table loaded from a theme manifest
   (`theme.toml` + assets). This alone enables "deep recolor" themes cheaply and forces
   the shells to be style-clean from the start. Light/dark variants, per-system accent
   colors, custom fonts, sound packs.
2. **Layout themes (post-1.0)** — a declarative screen-description format (constraint-based
   boxes, text, image, carousel, grid primitives; data-bound to `game.*`/`system.*`
   fields), interpreted by both shells. Deliberately **no scripting in v1** of the format
   (sandboxing burden, perf pillar); revisit a sandboxed scripting layer (Lua) only if the
   community hits real walls.

Rules: themes are pure data + assets — no network, no filesystem access outside their
folder; a broken theme degrades to the default theme with a visible warning, never a crash.
A `docs/theme-format.md` spec and a `relic-cli theme validate` command ship with the
feature. ES-DE importer is a stretch goal that maps the compatible subset and reports
what it couldn't translate.

---

## 7. Online Modules (all optional, compiled-out-able)

### 7.1 Scraper module (T1)

- Providers behind one trait: ScreenScraper, TheGamesDB, LaunchBox metadata mirror.
- Match by hash first (DAT/RA hash), filename fallback with confirmation UI for
  low-confidence matches.
- Aggressive local caching; rate-limit compliance per provider ToS; user supplies their
  own ScreenScraper credentials.
- All fetched media flows into the same content-addressed cache as local media.

### 7.2 RetroAchievements module (T1 display / T2 login)

Honest scope: a **frontend can't unlock achievements** — emulators (RetroArch, RA-enabled
standalones) do that. Relic's role is *companion and motivator*:

- Hash games with `rcheevos`' hashing rules (per-console algorithms) to map local files
  to RA game IDs — this reuses the lazy hash pipeline from §4.2.
- T2 login (username + web API key) → show per-game achievement lists, user unlock
  progress, points, mastery status; badge games in the grid ("has cheevos", "mastered").
- Smart collections: "has achievements I haven't unlocked", "close to mastery".
- Everything cached in SQLite for offline browsing; sync on demand or on launch.
- Launch integration: when launching via RetroArch, optionally verify RA hardcore
  settings match the user's preference (informational, not enforced).

### 7.3 Friends / "Relic Circle" (T2, last, smallest possible)

The friends feature must not compromise the local-first identity. Design constraints:

- **Prefer piggybacking**: RA already has friends/feeds — if a user connects RA, Relic can
  show their RA friends' recent unlocks with zero new infrastructure. Ship this first and
  evaluate whether more is actually wanted.
- If a native option proves warranted: a tiny **self-hostable** relay server
  (`relic-server`, single binary + SQLite) sharing only what the user picks
  (now-playing presence, playtime highlights, library counts — never file paths or
  library contents wholesale). Anthropic-of-truth remains the local DB; the server is a
  disposable mailbox. No official hosted instance until the project has governance for it.
- E2E-encrypt anything relayed between friends where feasible; friend pairing via QR/code.

This phase is deliberately last and explicitly cancellable if RA piggybacking satisfies
the need — that outcome would be a success, not a failure.

---

## 8. Performance, Size & Reliability Budgets

Budgets are CI-enforced where practical (benchmarks + binary-size check on release builds).

| Metric | Budget |
|---|---|
| Cold start → interactive grid (desktop, 5k games) | < 400 ms |
| Cold start → interactive (Android mid-range, 5k games) | < 1.5 s |
| Grid scroll | 60 fps, no GC/alloc hitches |
| Initial scan, 10k files on SATA SSD (no full hashes) | < 30 s |
| Incremental rescan, 10k files, no changes | < 2 s |
| Query latency (any list/search, 100k games) | < 30 ms |
| Core library size (release) | < 4 MB |
| Desktop binary total | < 20 MB |
| Android APK | < 15 MB |
| Idle RAM (desktop, grid visible, 5k games) | < 150 MB incl. thumbnail cache |
| RAM while game running (shed mode) | < 40 MB |

**Reliability practices:**

- The scanner and parsers treat all input as hostile: fuzz `gamelist.xml`, DAT, m3u, and
  archive parsers (`cargo-fuzz`) from Phase 1.
- Property-based tests on the scan pipeline (random file trees → invariants: no dupes,
  incremental == full rescan result).
- Golden-file tests against real-world `gamelist.xml` corpus in `fixtures/`.
- Soak test in CI weekly: synthetic 100k-file library, scan + query + watch churn.
- DB: `PRAGMA integrity_check` on open; automatic backup of user tables before migrations.
- Crash handling: local ring-buffer log (`relic doctor` / "Export diagnostics" button
  bundles logs + anonymized config). Nothing ever auto-uploads.

---

## 9. Phased Delivery Plan

Phases are sequential for the critical path (0→3), then largely parallelizable.
Durations assume ~1–2 focused developers; treat as relative sizing.

### Phase 0 — Foundations (2–3 weeks)
Repo scaffolding (workspace, CI for Win/mac/Linux/Android, clippy+fmt+deny), ADR-001
(core language: Rust vs Zig — final), **desktop UI spike** (Slint vs egui: build a
gamepad-navigable 1k-item grid at 60 fps in each, pick one), schema v1 + migration
runner, systems registry format + first 20 platforms, fixtures corpus.
**Exit:** `cargo test` green on 3 OSes; ADRs merged; UI stack chosen with spike evidence.

### Phase 1 — Core engine + CLI (4–6 weeks)
Scan pipeline (incremental, resumable, archive-aware), gamelist.xml import, local media
discovery + thumbnail cache, query/search API, event bus, launch model (desktop spawn),
UniFFI + C-ABI surfaces, `relic-cli` covering the whole API. Fuzzing + property tests live.
**Exit:** `relic-cli scan && relic-cli launch <game>` works on a real 5k-game library
within scan/query budgets; kill -9 during scan leaves DB valid.

### Phase 2 — Desktop shell MVP (4–6 weeks)
System browser, game grid, detail page, search, favorites, settings, first-run wizard
(pick folders, detect emulators), gamepad + keyboard navigation, launch/return lifecycle
with resource shedding, design-token groundwork (§6 layer 1), packaging (MSI, dmg,
flatpak, AppImage).
**Exit:** a stranger can install, point at their ROMs, and be playing in < 5 minutes;
budgets met on a 2015-era laptop.

### Phase 3 — Android shell (6–8 weeks)
Compose UI reusing the same core via UniFFI, SAF folder access + rescan-on-resume,
intent-template launching (RetroArch + top standalones), controller-first focus handling,
HOME launcher role, per-device performance pass on a low-end handheld (e.g. RG-class
device).
**Exit:** daily-drivable as the default launcher on a retro handheld; APK < 15 MB.

**→ MVP release (0.x) after Phase 3.** Public alpha; community feedback loop opens.

### Phase 4 — Metadata & media depth (3–5 weeks, parallelizable with 5)
DAT matching + canonical naming, gamelist export, multi-disc/m3u polish, smart
collections, playtime stats, **scraper module** (T1) with match-confirmation UI.

### Phase 5 — Theming (4–6 weeks)
Token themes shipped in both shells (this finishes the 1.0 requirement), theme
packaging + validation CLI, hot reload; layout-theme format spec drafted with 2–3
community theme authors before implementation.

**→ 1.0 release** when Phases 4 + token-theming are done and budgets hold.

### Phase 6 — RetroAchievements module (3–4 weeks)
rcheevos hashing in the lazy-hash pipeline, RA API client + offline cache, login flow,
achievement UI in both shells, badges + smart collections.

### Phase 7 — Sync & Circle (sized after evaluation)
LAN save/user-data sync between own devices first (clear value, T0/T1). Then RA-friends
piggyback display. Only then, if demand is proven, spec `relic-server` (§7.3).

### Phase 8 — iOS & long tail
SwiftUI shell over existing bindings (AltStore/sideload distribution reality-check),
Android TV, attract mode, kid mode, layout themes, ES-DE importer, localization drive.

---

## 10. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Desktop UI stack can't hit 60 fps themable grid | Phase-0 spike with hard exit criteria before committing; egui fallback |
| Android scoped storage friction (SAF slowness on huge libraries) | Index into app-private SQLite once; SAF only touched at scan & launch; document tree-URI best practices |
| Emulator intent formats drift across versions | Intent templates are data files, community-updateable without app release |
| Scraper providers' ToS / rate limits | Per-provider compliance layer, user-owned credentials, caching; scraping is a module we can drop |
| Scope creep vs "small & fast" pillars | Feature matrix + budgets are the contract; new features must name their module and their budget cost |
| One-person bus factor on core | ADRs for every decision, CLI as executable documentation, fixtures make contributions testable |
| RA API changes | Thin client isolated in its module; offline cache means breakage degrades, not crashes |

---

## 11. Open Questions (resolve in Phase 0 ADRs)

1. Rust vs Zig — final call (this plan recommends Rust; ADR-001).
2. Slint vs egui for desktop (ADR-002, decided by the spike).
3. One SQLite DB per library vs one global DB with `library_id` (plan assumes global; confirm).
4. Thumbnail formats: pre-generate WebP mips vs decode-on-demand with GPU cache.
5. Minimum Android version (SAF + Compose realities suggest API 26+; handheld market check).
6. License: GPLv3 vs MPL-2.0 (module ecosystem implications).
7. Name/branding check for "Relic" collisions in the launcher space.
