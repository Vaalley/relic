//! Offline metadata parsing and matching (Phase 1/4, PLAN.md §5).
//!
//! Parsers, each fuzz-tested against `fixtures/`:
//! - `gamelist.xml` (EmulationStation / ES-DE) import — implemented, see
//!   [`gamelist`] (Phase 1, PLAN.md §4.3/§5). Export is Phase 4.
//! - No-Intro / Redump DAT files for canonical naming and hash matching (Phase 4)
//! - `.m3u` multi-disc grouping (Phase 4)
//!
//! Merged into the `metadata` table with per-source rows; source priority
//! decides what the UI shows (user edits always win).

pub mod gamelist;
