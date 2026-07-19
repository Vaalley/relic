//! Offline metadata parsing and matching (Phase 1/4, PLAN.md §5).
//!
//! Parsers, each fuzz-tested against `fixtures/`:
//! - `gamelist.xml` (EmulationStation / ES-DE) import and export — see
//!   [`gamelist`] (Phase 1/4, PLAN.md §4.3/§5).
//! - No-Intro / Redump DAT files for canonical naming and hash matching —
//!   see [`dat`] (Phase 4).
//! - `.m3u` multi-disc grouping lives in `scan`, not here (it changes what
//!   gets indexed as a game, not a game's metadata).
//!
//! Merged into the `metadata` table with per-source rows; source priority
//! decides what the UI shows (user edits always win).

pub mod dat;
pub mod gamelist;
