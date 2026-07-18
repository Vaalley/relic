//! Emulator profiles and launch model (Phase 1/2, PLAN.md §4.5).
//!
//! Planned: arg-template expansion ({rom}, {rom_dir}, {rom_extracted},
//! {core}), desktop child-process spawn with play-session tracking, and the
//! data model consumed by the Android shell's intent templates. The shells own
//! the actual process/Intent mechanics; core owns profile resolution and
//! session bookkeeping.
