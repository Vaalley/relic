# Relic — Agent Instructions

Canonical instructions for every coding agent working in this repo (Claude Code,
Devin, Antigravity, Codex, Gemini, …). `CLAUDE.md` is a symlink to this file —
edit here only. Multi-agent workflow docs live in `.agents/README.md`.

## What this is

Relic: a local-first, **zero-telemetry** retro game frontend launcher.
Read `PLAN.md` before non-trivial work — it is the design authority (architecture,
feature matrix, phases, budgets). `RELIC.md` is the elevator pitch.
Decisions are recorded in `docs/adr/`; don't contradict an Accepted ADR silently.

## Layout

- `core/` — `relic-core`, the headless engine (scan → SQLite index → query/launch).
  Shells and FFI talk to `relic_core::api::Engine` only; nothing else touches the DB.
- `core/data/systems/*.toml` — platform registry (data change, not code change; new
  files must be added to `BUILTIN` in `core/src/systems/mod.rs`).
- `ffi/capi` (C ABI), `ffi/uniffi` (Kotlin/Swift, Phase 1) — FFI boundaries.
- `modules/` — optional feature crates (scraper, retroachievements, sync, themes).
  `core` must never depend on `modules/*`.
- `tools/relic-cli` — the `relic` binary; also the executable documentation/test harness.
- `apps/desktop`, `apps/android` — UI shells. `fixtures/` — synthetic test libraries.

## Commands

- Build/test everything: `cargo build --workspace && cargo test --workspace`
- Lint gate (CI enforces): `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings`
- Offline-purity gate: `cargo build -p relic-core --no-default-features` must always pass.
- Smoke test: `cargo run -p relic-cli -- scan --db test.db fixtures/mini` then
  `... games --db test.db` (delete `test.db*` afterwards; never commit it).
- Android verification: `apps/android/app/src/main/jniLibs/*/librelic_ffi.so` is
  gitignored and NOT rebuilt by `gradlew compileDebugKotlin`/`assembleDebug` — those
  only repackage whatever `.so` is already sitting there. After any change under
  `ffi/uniffi/`, you MUST run `pwsh -File tools/android/build-apk.ps1` (cargo-ndk
  rebuild + binding regen + gradle) before trusting an Android build; a stale `.so`
  compiles and installs fine but crashes instantly on-device with a UniFFI checksum
  `UnsatisfiedLinkError`, which gradle alone will never catch.

## Hard rules

1. **Zero telemetry, forever.** No network calls in `core` or any T0 code path. Online
   behavior lives only in `modules/` behind opt-in feature flags (PLAN.md §1).
2. ROM libraries follow the per-system subfolder convention `<root>/<slug>/…`.
3. Scanned DB tables are disposable; `user_data`, `collections`, `play_sessions`,
   `settings` are precious — no migration or rescan may destroy them.
4. Fixtures contain placeholder bytes only — never real ROM content, never links to ROMs.
5. Migrations are append-only (`core/src/db/migrations/`); never edit a shipped one.
6. Match existing style: module-level `//!` docs state scope + plan phase; comments
   explain constraints, not restate code. Run fmt/clippy/test before declaring done.
