//! relic-core — the headless engine behind every Relic shell.
//!
//! Shells (desktop, Android, CLI) talk to [`api::Engine`] exclusively; nothing
//! outside this crate touches SQLite or the filesystem index directly.
//!
//! Layering (see PLAN.md §4):
//! - `db`       schema, migrations, connection handling
//! - `systems`  platform registry loaded from `data/systems/*.toml`
//! - `scan`     filesystem crawl → index pipeline
//! - `metadata` gamelist.xml / DAT parsing and matching (Phase 1)
//! - `media`    thumbnail cache (Phase 1)
//! - `launch`   emulator profiles and argument templating (Phase 1)
//! - `events`   delta events streamed to shells
//! - `stats`    playtime aggregation: recently/most played, totals (Phase 1)
//! - `api`      the single public facade exposed over FFI

pub mod api;
pub mod db;
pub mod events;
pub mod launch;
pub mod media;
pub mod metadata;
pub mod scan;
pub mod stats;
pub mod systems;

mod error;
pub use error::{Error, Result};

/// Core engine version, surfaced through every FFI boundary.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
