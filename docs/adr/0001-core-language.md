# ADR 0001: Core engine language — Rust

## Status

Accepted

## Context

`RELIC.md` originally suggested Zig for the headless core engine, on the
grounds of binary size and systems-language control. `PLAN.md` §2.1
re-examines that choice against the actual architecture: one core engine
embedded into three shells (Android, desktop, and later iOS) over a
C-ABI/message-passing boundary, doing filesystem crawling, SQLite indexing,
and metadata parsing that must never corrupt the library.

Relevant forces:

- **FFI story.** The shells need real, maintained Kotlin and Swift bindings,
  not hand-rolled JNI/Swift bridges kept in sync by hand.
- **Ecosystem maturity.** The core needs a battle-tested SQLite binding, a
  cross-platform filesystem watcher, and robust `gamelist.xml`/DAT parsing.
- **Reliability pillar.** Relic's #1 priority (PLAN.md §1) is "never
  corrupts a library" in a multithreaded scanner — memory safety and
  fearless concurrency matter directly here.
- **Size budget.** Core library size budget is < 4 MB (PLAN.md §8); this
  must be achievable without hand-tuning a language runtime.

## Decision

Use **Rust** for `relic-core` (and the C-ABI/module crates around it),
overriding `RELIC.md`'s Zig suggestion.

- [UniFFI](https://mozilla.github.io/uniffi-rs/) generates Kotlin and Swift
  bindings from one interface definition, matching the "one core, many
  shells" architecture directly.
- `rusqlite`, `notify`, `serde`/`quick-xml` cover SQLite, FS watching, and
  gamelist/DAT parsing with mature, maintained crates; `rcheevos` has usable
  Rust bindings for the later RetroAchievements module.
- Ownership and borrowing give the concurrent scanner memory safety without
  hand-written synchronization discipline.
- With `opt-level = "z"`, LTO, and `panic = "abort"` (see the workspace
  `[profile.release]`), the core lands well within the size budget.

The boundary design (C-ABI + message passing, PLAN.md §4) stays
language-agnostic, so this decision is revisitable — but only with a
concrete, evaluated FFI plan, not a preference.

## Consequences

- Contributors need working Rust knowledge; this is a real bar relative to
  Zig's smaller surface area, accepted in exchange for the FFI and ecosystem
  wins above.
- The core commits to `cargo`/`crates.io` as its dependency and build
  ecosystem across all platforms, including Android cross-compilation.
- Revisiting this decision later requires a concrete FFI plan for whatever
  alternative is proposed (bindings quality, ecosystem parity, migration
  cost for `core/`, `ffi/`, and every module crate) — not just a language
  preference.
