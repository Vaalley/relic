//! relic-retroachievements — RetroAchievements companion module (PLAN.md §7.2,
//! full design in `docs/retroachievements-design.md`).
//!
//! T1 (display) / T2 (login): games are hashed with `rcheevos`' per-console
//! hashing rules to map local files to RA game ids, reusing the lazy-hash
//! pipeline from the core scan model. With a T2 login the module fetches and
//! caches achievement lists, unlock progress, points, and mastery status for
//! offline browsing. This is a display-only companion — Relic is a launcher,
//! not an emulator, so it cannot itself unlock achievements; that happens
//! inside RA-aware emulator cores.
//!
//! Planned for Phase 6, after the 1.0 theming milestone. Sub-phase 6a
//! (design doc §7.1) is now complete: module-owned schema (`store`), a
//! match store keyed on content hash, and `rc_hash` bound via FFI (`hash`,
//! `native/rcheevos/`) for cartridge/ROM consoles. Sub-phase 6b groundwork
//! (design doc §7.2) — the hash-library/achievement API client (`client`)
//! and the no-match TTL cache (`no_match_cache`) — exists behind the
//! `network` cargo feature; a default build stays network-free
//! (`--no-default-features` on `relic-core` still compiles this module out
//! entirely).

pub mod hash;
pub mod no_match_cache;
pub mod store;

#[cfg(feature = "network")]
pub mod client;

pub use hash::{hash_buffer, hash_file, RaHash};
pub use no_match_cache::{is_cached_no_match, record_no_match};
pub use store::{has_cheevos, matches_for_file, migrate, save_match, RaGameMatch};

#[cfg(feature = "network")]
pub use client::{GameIdMatch, RaClientConfig, RaClientError, RaHashClient};

/// Stable identifier this module reports through the engine's
/// `capabilities()` API so shells can hide UI for absent modules.
pub fn capability_id() -> &'static str {
    "retroachievements"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_id_is_retroachievements() {
        assert_eq!(capability_id(), "retroachievements");
    }
}
