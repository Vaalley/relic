# Relic

A local-first, zero-telemetry, open-source retro game frontend. It organizes
your ROM collection and boots standalone emulators — entirely offline by
default.

## Pillars

In priority order (when they conflict, the higher one wins):

1. **Reliable** — never corrupts a library, never loses user edits, survives
   crashes mid-scan.
2. **Fast** — instant startup, 60 fps navigation, scans that scale to
   100k+ files.
3. **Small** — tight binaries, low RAM, no bundled runtimes it doesn't need.
4. **Modular** — everything beyond "scan, browse, launch" is an optional
   module.
5. **Feature-rich** — via modules, not via a monolith.

Relic is **not an emulator**, bundles no emulation cores, and never links to
ROM sources.

## Privacy tiers

Every feature is classified into one of three tiers, surfaced in the UI:

| Tier | Network | Examples | Default |
|------|---------|----------|---------|
| **T0 — Offline** | None, ever | Scanning, browsing, launching, theming, collections | On |
| **T1 — Opt-in fetch** | Outbound requests to a user-chosen service | Metadata scraping, RetroAchievements display | Off |
| **T2 — Opt-in identity** | Authenticated account | RA login, friends/presence | Off |

The core engine and both shells are fully functional at T0. T1/T2 code lives
in separate, optional module crates that can be compiled out entirely.

## Repo layout

```
relic/
├── core/            # relic-core: the headless engine
├── ffi/             # uniffi/ (Kotlin/Swift) and capi/ (C ABI) bindings
├── modules/          # optional crates: scraper, retroachievements, sync, themes
├── apps/            # desktop, android, (later) ios shells
├── tools/relic-cli/  # headless CLI — scan/query/launch, also the test harness
├── themes/           # bundled default theme + alternates
├── docs/adr/         # architecture decision records
└── fixtures/         # synthetic test libraries (no copyrighted content)
```

## Quick start

```
cargo build --workspace

# Point at a ROM root laid out as <root>/<system-slug>/... (ES-style), e.g.
# roms/snes/*.sfc, roms/nes/*.nes — see fixtures/mini for a tiny example.
cargo run -p relic-cli -- scan --db relic.db <your-rom-root>
cargo run -p relic-cli -- systems --db relic.db
cargo run -p relic-cli -- games --db relic.db
```

## Learn more

- [`RELIC.md`](RELIC.md) — the elevator pitch.
- [`PLAN.md`](PLAN.md) — the full implementation plan (architecture, feature
  matrix, budgets, phased delivery).
- [`docs/adr/`](docs/adr/) — architecture decision records.

## License

GPL-3.0-or-later, pending a final licensing ADR (see `PLAN.md` §11).

## Zero telemetry, forever

Relic sends nothing home. No analytics, no crash reporting, no phone-home
update checks. Every online feature is opt-in, off by default, and fully
removable. Diagnostics are local files you choose to attach to a bug report
— never anything automatic.
