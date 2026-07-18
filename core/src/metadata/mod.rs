//! Offline metadata parsing and matching (Phase 1/4, PLAN.md §5).
//!
//! Planned parsers, each fuzz-tested against `fixtures/`:
//! - `gamelist.xml` (EmulationStation / ES-DE) import and export
//! - No-Intro / Redump DAT files for canonical naming and hash matching
//! - `.m3u` multi-disc grouping
//!
//! Merged into the `metadata` table with per-source rows; source priority
//! decides what the UI shows (user edits always win).
