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
//! Planned for Phase 6, after the 1.0 theming milestone. Sub-phase 6a scope
//! implemented so far (design doc §7.1): module-owned schema (`store`) and a
//! match store keyed on content hash. **Not yet implemented**: the actual
//! `rc_hash` binding (design doc §2.2 recommends vendoring `rcheevos` as a
//! C dependency — a real decision to make deliberately, not a default to
//! reach for while scaffolding) and any network client (§3). Until those
//! land, this module has zero network activity and compiles out cleanly
//! with `--no-default-features` on `relic-core`.

pub mod store;

pub use store::{has_cheevos, matches_for_file, migrate, save_match, RaGameMatch};

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
