# Relic — Android shell

Not yet scaffolded. Per `PLAN.md` (Phase 3, §9), this is a native launcher
built with **Kotlin + Jetpack Compose** consuming `relic-core` through the
UniFFI bindings generated in `ffi/uniffi/`.

The Gradle project is deliberately not created yet — an empty Gradle tree
would rot (dependency drift, stale AGP/Kotlin versions, broken CI) long
before Phase 3 starts. It gets scaffolded when Phase 3 begins, against a
core that already has a stable UniFFI surface to bind to.

## Planned key pieces

- **SAF folder access** — libraries are indexed once into the app-private
  SQLite cache; `content://` tree URIs are only touched again at scan time
  and at launch time (scoped-storage friction mitigation, PLAN.md §10).
- **Intent-template launching** — explicit `Intent`s built from per-emulator
  templates (component, extras, data URI, flags), `FLAG_GRANT_READ_URI_PERMISSION`
  granted per launch; built-in templates for RetroArch and common standalones
  (Dolphin, PPSSPP, DraStic-likes, Yaba Sanshiro, etc.), community-extendable
  as data files.
- **HOME launcher role** — Relic can register as, and act as, the device's
  default Home screen on handhelds (target: 1.0).
- **Controller-first focus handling** — physical controller input drives UI
  focus and navigation directly; no dependency on touch.

See `PLAN.md` §2.2, §4.5, and Phase 3 for the full scope and exit criteria.
