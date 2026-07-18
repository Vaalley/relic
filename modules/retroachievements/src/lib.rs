//! relic-retroachievements — RetroAchievements companion module (PLAN.md §7.2).
//!
//! T1 (display) / T2 (login): games are hashed with `rcheevos`' per-console
//! hashing rules to map local files to RA game ids, reusing the lazy-hash
//! pipeline from the core scan model. With a T2 login the module fetches and
//! caches achievement lists, unlock progress, points, and mastery status for
//! offline browsing. This is a display-only companion — Relic is a launcher,
//! not an emulator, so it cannot itself unlock achievements; that happens
//! inside RA-aware emulator cores.
//!
//! Planned for Phase 6, after the 1.0 theming milestone.

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
